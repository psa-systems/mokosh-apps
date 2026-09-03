//! Reaching into the rendered document (MAPPS-504).
//!
//! Both hosts render the same HTML, but only the browser build runs its
//! Rust in the same address space as the DOM. On the desktop the UI
//! lives in a webview and Rust talks to it by evaluating JavaScript, so
//! the writes here (title, theme class, CSS variables, focus, scroll)
//! are `dioxus::document::eval` calls.
//!
//! `eval` runs the script the moment it is created - dioxus-desktop's
//! own `create_style` / `create_meta` drop the handle the same way - so
//! these stay synchronous and nothing has to be awaited.
//!
//! Reads are the asymmetry. A value has to come BACK from the webview,
//! which is asynchronous, so a read is an `async fn` here
//! ([`scroll_top_async`]) and the caller awaits it from an effect or an
//! event handler. The browser answers from the document it is already
//! in; the desktop asks the webview and waits for the answer.
//!
//! The same channel carries events the other way (MAPPS-511): the
//! injected script attaches a listener and `dioxus.send`s each
//! occurrence, and a spawned task loops on `recv()` and calls the
//! handler ([`watch_task_toggles`], [`watch_system_theme`]). That is how
//! the desktop gets the two things it cannot get from a Dioxus event: a
//! click inside `dangerous_inner_html`, and an OS light/dark switch.
//!
//! What is still read-free is better off that way. [`capture_focus`]
//! parks the focused element in the webview under a token rather than
//! reading it back, because by the time an async read resolved the
//! dialog would already have taken focus.

/// Set the window/tab title.
///
/// Direct `web_sys` write on the web target (PMS-892): `dioxus::document`'s
/// `set_title` runs through its `eval`-based `Document::eval`, which needs
/// `'unsafe-eval'` in the CSP. This SPA's CSP deliberately omits it
/// (`oci-build/Caddyfile`, MAPPS-308), so that call was blocked and, because
/// the wasm-bindgen glue behind it is not marked `catch`, the blocked call
/// panicked the whole wasm instance on every title change - i.e. on most
/// in-app navigation. The desktop renderer still goes through
/// `dioxus::document`, which talks to the webview and is unaffected.
#[cfg(target_arch = "wasm32")]
pub fn set_title(title: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        doc.set_title(title);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_title(title: &str) {
    if !in_runtime() {
        return;
    }
    dioxus::document::document().set_title(title.to_string());
}

/// Is there a Dioxus runtime to talk to?
///
/// `dioxus::document::document()` and `eval` both reach for the current
/// runtime and PANIC when there is none. That happens in the host
/// `cargo test` build, where components are rendered to a string with
/// no renderer attached: `utils::form_guard` focuses a field as part of
/// its validation, and a panic there would fail a test about validation
/// for a reason that has nothing to do with it.
///
/// Both callers (`set_title` and `eval`) are host-only, so the wasm build
/// carried it as dead code and warned on every `just check-web` (MAPPS-629).
#[cfg(not(target_arch = "wasm32"))]
fn in_runtime() -> bool {
    dioxus::core::Runtime::try_current().is_some()
}

/// Add or remove the Tailwind `dark` class on `<html>`.
#[cfg(target_arch = "wasm32")]
pub fn set_root_dark(is_dark: bool) {
    let Some(root) = document_element() else {
        return;
    };
    let class_list = root.class_list();
    let result = if is_dark {
        class_list.add_1(DARK_CLASS)
    } else {
        class_list.remove_1(DARK_CLASS)
    };
    if result.is_err() {
        // Every surface colour in the app keys off this class. If it
        // did not land, the user is looking at the wrong theme and
        // nothing else will say why.
        tracing::error!("could not apply the {DARK_CLASS} class to <html>");
    }
}

/// Set inline CSS custom properties on `<html>`. Inline values override
/// the stylesheet defaults in `input.css`, which is how the accent ramp
/// recolours live.
#[cfg(target_arch = "wasm32")]
pub fn set_root_css_vars(vars: &[(&str, &str)]) {
    use wasm_bindgen::JsCast;

    let Some(root) = document_element() else {
        return;
    };
    let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() else {
        tracing::error!("<html> is not an HtmlElement; accent colours not applied");
        return;
    };
    let style = html.style();
    for (name, value) in vars {
        if style.set_property(name, value).is_err() {
            tracing::error!("could not set {name} on <html>");
        }
    }
}

/// Is the app out of sight - a backgrounded tab, or a minimised window?
///
/// Used by the auth heartbeat to skip a poll nobody is waiting on.
#[cfg(target_arch = "wasm32")]
pub fn window_hidden() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.visibility_state() == web_sys::VisibilityState::Hidden)
        .unwrap_or(false)
}

// --- coming back to the foreground (MAPPS-645) ---------------------------
//
// `window_hidden()` above is a poll, and the auth loops only ever asked it
// once every 30 seconds. A tab suspended past its token's expiry therefore
// woke, fired the requests its pages mount with, and sat on whatever error
// they earned until the remainder of that sleep ran out; a reload only
// re-entered the same race. These three give the loops an edge to wake on
// instead.

thread_local! {
    /// Advanced once per hidden -> visible transition. A generation rather
    /// than a flag: a waiter compares the value it started with, so a wake
    /// that lands between two polls is still seen on the next one.
    static VISIBLE_GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Everything currently parked in [`visible_again`], by waiter id.
    static VISIBLE_WAITERS: std::cell::RefCell<Vec<(u64, std::task::Waker)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static NEXT_WAITER_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The app is back in front of the user: wake everything parked in
/// [`visible_again`].
///
/// The browser's `visibilitychange` listener ([`watch_visibility`]) is the
/// only caller in a shipped build. Nothing calls it on the desktop, where the
/// window reports itself visible at all times (see [`window_hidden`]), so a
/// native `visible_again()` never resolves and the auth loops keep the poll
/// cadence they have always had there.
pub fn notify_visible() {
    VISIBLE_GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
    let waiters = VISIBLE_WAITERS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    for (_, waker) in waiters {
        waker.wake();
    }
}

/// Resolves the next time the app comes back to the foreground.
pub fn visible_again() -> VisibleAgain {
    VisibleAgain {
        start: VISIBLE_GENERATION.with(std::cell::Cell::get),
        id: NEXT_WAITER_ID.with(|n| {
            let next = n.get().wrapping_add(1);
            n.set(next);
            next
        }),
    }
}

/// The future [`visible_again`] hands out. Deregisters itself when dropped,
/// which is every time a caller races it against a timer and the timer wins.
pub struct VisibleAgain {
    start: u64,
    id: u64,
}

impl std::future::Future for VisibleAgain {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if VISIBLE_GENERATION.with(std::cell::Cell::get) != self.start {
            return std::task::Poll::Ready(());
        }
        VISIBLE_WAITERS.with(|w| {
            let mut waiters = w.borrow_mut();
            match waiters.iter_mut().find(|(id, _)| *id == self.id) {
                Some((_, waker)) => waker.clone_from(cx.waker()),
                None => waiters.push((self.id, cx.waker().clone())),
            }
        });
        std::task::Poll::Pending
    }
}

impl Drop for VisibleAgain {
    fn drop(&mut self) {
        VISIBLE_WAITERS.with(|w| w.borrow_mut().retain(|(id, _)| *id != self.id));
    }
}

/// Start reporting a return to the foreground. Idempotent, and mounted from
/// the auth loops at the app root, so a second call installs nothing.
#[cfg(target_arch = "wasm32")]
pub fn watch_visibility() {
    use wasm_bindgen::JsCast;

    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// What the last `visibilitychange` reported, so only the
        /// hidden -> visible edge wakes anybody.
        static WAS_HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    if INSTALLED.with(std::cell::Cell::get) {
        return;
    }
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        // Without this the SPA still works, it just recovers from a wake on
        // the 30s poll instead of immediately, and nothing else would say so.
        tracing::error!(
            "no document to watch for a return to the foreground; the auth loops keep their 30s poll cadence"
        );
        return;
    };
    WAS_HIDDEN.with(|h| h.set(window_hidden()));

    let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
        let hidden_now = window_hidden();
        // `visibilitychange` fires on the way out too, and renewing a token
        // for a tab nobody is looking at is what the skip in the heartbeat
        // exists to avoid.
        if WAS_HIDDEN.with(|h| h.replace(hidden_now)) && !hidden_now {
            notify_visible();
        }
    }) as Box<dyn FnMut(web_sys::Event)>);

    if let Err(e) = document
        .add_event_listener_with_callback("visibilitychange", handler.as_ref().unchecked_ref())
    {
        tracing::error!(
            "could not listen for a return to the foreground: {e:?}; the auth loops keep their 30s poll cadence"
        );
        return;
    }
    INSTALLED.with(|i| i.set(true));
    // The listener outlives this call; it is installed once, at the app root.
    handler.forget();
}

/// A desktop window is always visible (see [`window_hidden`]), so there is no
/// hidden -> visible edge to listen for and nothing to install.
#[cfg(not(target_arch = "wasm32"))]
pub fn watch_visibility() {}

/// Does the operating system ask for a dark UI right now?
#[cfg(target_arch = "wasm32")]
pub fn system_prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false)
}

/// Move keyboard focus to the element with this id. No-op when the
/// element is not in the document.
#[cfg(target_arch = "wasm32")]
pub fn focus_by_id(id: &str) {
    use wasm_bindgen::JsCast;

    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    else {
        return;
    };
    if let Ok(html) = el.dyn_into::<web_sys::HtmlElement>() {
        // MAPPS-503: a dropped focus failure left the expanded global-search
        // entry unusable with no record of why. That call site now routes
        // through here, so the logging lives here and covers every caller.
        if let Err(e) = html.focus() {
            tracing::warn!("focusing #{id} failed: {e:?}");
        }
    }
}

/// Whatever had keyboard focus when this was taken, so a modal can put
/// it back on the control that opened it.
///
/// Opaque, and `Clone` because the caller stashes it in a hook and
/// restores it from a drop handler.
#[derive(Clone)]
#[cfg(target_arch = "wasm32")]
pub struct FocusToken(Option<web_sys::HtmlElement>);

/// The desktop twin holds the key to an element parked in the webview,
/// not the element itself (MAPPS-511).
#[derive(Clone)]
#[cfg(not(target_arch = "wasm32"))]
pub struct FocusToken(u64);

/// Record the currently-focused element.
#[cfg(target_arch = "wasm32")]
pub fn capture_focus() -> FocusToken {
    use wasm_bindgen::JsCast;

    FocusToken(
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element())
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
    )
}

#[cfg(target_arch = "wasm32")]
impl FocusToken {
    /// Put focus back. No-op when nothing was focused, or when the
    /// element has since left the document.
    pub fn restore(&self) {
        if let Some(el) = &self.0 {
            // Best-effort, and expected to fail sometimes: the element
            // that opened the modal may have been unmounted while it was
            // open. There is nothing else to focus and nothing to report.
            let _ = el.focus();
        }
    }
}

/// Record the currently-focused element, in the webview.
///
/// MAPPS-511: reading `document.activeElement` back out would be
/// asynchronous, and by the time the answer arrived the dialog that
/// triggered this would already hold focus - the wrong element, read
/// too late. So nothing is read: the script parks the element under a
/// token and [`FocusToken::restore`] focuses whatever is parked there.
/// Both halves are writes, which the desktop can do synchronously.
#[cfg(not(target_arch = "wasm32"))]
pub fn capture_focus() -> FocusToken {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let token = NEXT.fetch_add(1, Ordering::Relaxed);
    eval(&format!(
        "window.{FOCUS_STASH} = window.{FOCUS_STASH} || new Map(); \
         window.{FOCUS_STASH}.set({token}, document.activeElement);"
    ));
    FocusToken(token)
}

#[cfg(not(target_arch = "wasm32"))]
impl FocusToken {
    /// Put focus back on the parked element and let go of it. No-op when
    /// nothing was focused, or when the element has since left the
    /// document - the same best-effort contract as the browser's.
    pub fn restore(&self) {
        let token = self.0;
        eval(&format!(
            "{{ const stash = window.{FOCUS_STASH}; \
                const el = stash && stash.get({token}); \
                if (stash) stash.delete({token}); \
                if (el && el.focus) el.focus(); }}"
        ));
    }
}

/// Where [`capture_focus`] parks elements. On `window` because each
/// `eval` is a separate script with its own scope, so the capture and
/// the restore have no other way to name the same object.
#[cfg(not(target_arch = "wasm32"))]
const FOCUS_STASH: &str = "__mokoshFocusStash";

/// MAPPS-579: the selection on a `<textarea>`, in UTF-16 code units, or the
/// end of `fallback_len` when the element cannot be reached.
///
/// UTF-16 because that is what `selectionStart` counts. The caller converts;
/// see `crate::utils::md_edit`, which documents why mixing those up with byte
/// offsets survives every English-language test and then corrupts the first
/// article containing an accent.
#[cfg(target_arch = "wasm32")]
pub fn textarea_selection(id: &str, fallback_end: u32) -> (u32, u32) {
    use wasm_bindgen::JsCast;
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    else {
        return (fallback_end, fallback_end);
    };
    let start = el.selection_start().ok().flatten().unwrap_or(fallback_end);
    let end = el.selection_end().ok().flatten().unwrap_or(start);
    (start, end)
}

/// Focus a `<textarea>` and put its selection back, in UTF-16 code units.
#[cfg(target_arch = "wasm32")]
pub fn set_textarea_selection(id: &str, start: u32, end: u32) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        let _ = el.focus();
        let _ = el.set_selection_range(start, end);
    }
}

/// Current scroll offset of the element with this id.
#[cfg(target_arch = "wasm32")]
pub fn scroll_top(id: &str) -> Option<i32> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .map(|el| el.scroll_top())
}

/// MAPPS-511: the read as the shared call sites take it. The browser is
/// already in the document, so this is [`scroll_top`] with an `await` in
/// front of it; the desktop twin asks the webview and waits.
#[cfg(target_arch = "wasm32")]
pub async fn scroll_top_async(id: &str) -> Option<i32> {
    scroll_top(id)
}

/// Restore a scroll offset onto the element with this id.
#[cfg(target_arch = "wasm32")]
pub fn set_scroll_top(id: &str, top: i32) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    {
        el.set_scroll_top(top);
    }
}

/// Bring the element with this id into its scroll container's view.
/// `align_top` picks the top of the element over the bottom.
///
/// MAPPS-503 uses this to follow a keyboard highlight past the edge of a
/// dropdown's `max-h-*` scroll box.
#[cfg(target_arch = "wasm32")]
pub fn scroll_into_view(id: &str, align_top: bool) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    {
        el.scroll_into_view_with_bool(align_top);
    }
}

/// Scroll the element with this id to `frac` of its scrollable extent
/// (`0.0` is the start, `1.0` the end). `vertical` picks `scrollTop`
/// over `scrollLeft`.
///
/// A fraction rather than a pixel offset because the caller wants "put
/// 8am in view" and only the rendered element knows how tall an hour
/// is; computing the pixels here would need a read back out of the
/// document, which the desktop cannot do synchronously.
#[cfg(target_arch = "wasm32")]
pub fn scroll_to_fraction(id: &str, vertical: bool, frac: f64) {
    let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    else {
        return;
    };
    if vertical {
        el.set_scroll_top((f64::from(el.scroll_height()) * frac).round() as i32);
    } else {
        el.set_scroll_left((f64::from(el.scroll_width()) * frac).round() as i32);
    }
}

#[cfg(target_arch = "wasm32")]
const DARK_CLASS: &str = "dark";

#[cfg(target_arch = "wasm32")]
fn document_element() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.document_element()
}

// --- desktop -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn set_root_dark(is_dark: bool) {
    let action = if is_dark { "add" } else { "remove" };
    eval(&format!(
        "document.documentElement.classList.{action}('dark');"
    ));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_root_css_vars(vars: &[(&str, &str)]) {
    // One script for the whole ramp: thirteen separate evals would be
    // thirteen round trips to the webview on every theme change.
    let pairs: Vec<[&str; 2]> = vars.iter().map(|(n, v)| [*n, *v]).collect();
    let json = match serde_json::to_string(&pairs) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("could not encode the accent variables: {e}");
            return;
        }
    };
    eval(&format!(
        "for (const [n, v] of {json}) {{ document.documentElement.style.setProperty(n, v); }}"
    ));
}

/// Always visible.
///
/// A minimised desktop window is still a live session, and tao does not
/// report "the user is not looking at this" in a way that maps onto a
/// hidden tab. Erring towards visible costs one `/auth/me` every 30
/// seconds; erring the other way would mean a minimised window silently
/// stops noticing that its account was deleted, which is the condition
/// the heartbeat exists to catch.
#[cfg(not(target_arch = "wasm32"))]
pub fn window_hidden() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    /// What the webview last reported for `prefers-color-scheme`
    /// (MAPPS-511). `None` until [`watch_system_theme`] has said
    /// anything, which is why the window's own theme answers at boot.
    static WEBVIEW_PREFERS_DARK: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Does the operating system ask for a dark UI right now?
///
/// The webview's own `prefers-color-scheme` once it has reported one,
/// so the theme that got applied and the theme the accent picker shows
/// are the same answer; the window's tao theme until then.
#[cfg(not(target_arch = "wasm32"))]
pub fn system_prefers_dark() -> bool {
    if let Some(is_dark) = WEBVIEW_PREFERS_DARK.with(std::cell::Cell::get) {
        return is_dark;
    }
    window_theme_is_dark()
}

/// The window's theme as the OS reports it to tao.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
fn window_theme_is_dark() -> bool {
    let Some(ctx) = dioxus::prelude::try_consume_context::<dioxus::desktop::DesktopContext>()
    else {
        return false;
    };
    ctx.window.theme() == dioxus::desktop::tao::window::Theme::Dark
}

/// Without the `desktop` feature there is no window to ask - that build
/// is `cargo check` / `cargo test` on the host, which renders nothing -
/// so it reports light and the caller's explicit preference still wins.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
fn window_theme_is_dark() -> bool {
    false
}

/// Follow the OS light/dark preference for as long as the caller's
/// scope lives, calling `on_change` each time it flips (MAPPS-511).
///
/// The webview's `prefers-color-scheme` is the source, not tao's
/// `WindowEvent::ThemeChanged`: tao 0.34 only emits that event on macOS
/// and Windows (`platform_impl/{macos,windows}`), so the Linux build -
/// which is what the repo's own toolchain targets - would go on
/// ignoring OS theme changes. The media query is also the exact
/// mechanism the browser build uses, so one behaviour has one
/// definition on both hosts.
///
/// The listener reports its current value immediately, so a window
/// whose webview disagrees with the tao theme it booted on lands on the
/// webview's answer rather than staying wrong until the user switches.
#[cfg(not(target_arch = "wasm32"))]
pub fn watch_system_theme(mut on_change: impl FnMut() + 'static) {
    if !in_runtime() {
        return;
    }
    // Installed once per window: the effect that calls this re-runs, and
    // a second listener would report every change twice. The query is
    // parked on `window` because a MediaQueryList nothing references can
    // be collected along with the listener attached to it.
    let mut eval = dioxus::document::eval(
        "if (window.__mokoshThemeWatch) return; \
         window.__mokoshThemeWatch = true; \
         const mql = window.matchMedia('(prefers-color-scheme: dark)'); \
         window.__mokoshThemeMql = mql; \
         mql.addEventListener('change', (e) => dioxus.send(e.matches)); \
         dioxus.send(mql.matches);",
    );
    dioxus::prelude::spawn(async move {
        loop {
            match eval.recv::<bool>().await {
                Ok(is_dark) => {
                    WEBVIEW_PREFERS_DARK.with(|c| c.set(Some(is_dark)));
                    on_change();
                }
                Err(e) => {
                    // Losing this pins a `Theme::System` user to whatever
                    // the OS said at boot, with nothing to say why.
                    tracing::error!("stopped following OS dark-mode changes: {e}");
                    return;
                }
            }
        }
    });
}

/// Report every click on an element carrying `data-ti` inside the
/// container with this id, as the index that attribute holds
/// (MAPPS-511).
///
/// Markdown task-list checkboxes are raw HTML injected with
/// `dangerous_inner_html`, so they cannot carry a Dioxus handler. The
/// browser answers that with one delegated DOM listener
/// (`components::markdown`); the desktop cannot attach a listener from
/// Rust at all, so the script attaches it and posts each click back
/// over the `eval` channel instead.
#[cfg(not(target_arch = "wasm32"))]
pub fn watch_task_toggles(dom_id: &str, on_toggle: dioxus::prelude::EventHandler<usize>) {
    if !in_runtime() {
        return;
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); \
         if (!el) return 'missing'; \
         if (el.dataset.mokoshToggles) return 'installed'; \
         el.dataset.mokoshToggles = '1'; \
         el.addEventListener('click', (e) => {{ \
            const index = e.target && e.target.dataset && e.target.dataset.ti; \
            if (index !== undefined && index !== null) dioxus.send(Number(index)); \
         }}); \
         return 'installed';",
        js_string(dom_id)
    ));
    dioxus::prelude::spawn(async move {
        loop {
            match eval.recv::<usize>().await {
                Ok(index) => on_toggle.call(index),
                Err(e) => {
                    // The checkboxes render either way, so without this
                    // the only symptom is a click that does nothing.
                    tracing::error!("stopped listening for markdown task toggles: {e}");
                    return;
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn focus_by_id(id: &str) {
    eval(&format!(
        "document.getElementById({})?.focus();",
        js_string(id)
    ));
}

/// MAPPS-511: ask the webview for the element's scroll offset and wait
/// for the answer. `None` when there is no such element, and when the
/// round trip fails - the caller (the sidebar scroll memory in
/// `components::layout`) reads that as "nothing to record".
#[cfg(not(target_arch = "wasm32"))]
pub async fn scroll_top_async(id: &str) -> Option<i32> {
    if !in_runtime() {
        return None;
    }
    let value = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); return el ? el.scrollTop : null;",
        js_string(id)
    ))
    .await;
    match value {
        // `scrollTop` is fractional on a scaled display, and the offset
        // is put back with `set_scroll_top`, which takes whole pixels.
        Ok(serde_json::Value::Number(n)) => n.as_f64().map(|top| top.round() as i32),
        Ok(serde_json::Value::Null) => None,
        Ok(other) => {
            tracing::warn!("the webview answered {other} for the scroll offset of #{id}");
            None
        }
        Err(e) => {
            // Nothing downstream can tell a failed round trip from an
            // element that is not there, so the reason has to be logged
            // here or the sidebar just stops remembering its place.
            tracing::warn!("could not read the scroll offset of #{id}: {e}");
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn scroll_into_view(id: &str, align_top: bool) {
    eval(&format!(
        "document.getElementById({})?.scrollIntoView({align_top});",
        js_string(id)
    ));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn scroll_to_fraction(id: &str, vertical: bool, frac: f64) {
    let axis = if vertical { "Top" } else { "Left" };
    let extent = if vertical {
        "scrollHeight"
    } else {
        "scrollWidth"
    };
    eval(&format!(
        "{{ const el = document.getElementById({}); \
            if (el) el.scroll{axis} = Math.round(el.{extent} * {frac}); }}",
        js_string(id)
    ));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_scroll_top(id: &str, top: i32) {
    eval(&format!(
        "{{ const el = document.getElementById({}); if (el) el.scrollTop = {top}; }}",
        js_string(id)
    ));
}

/// Fire a script into the webview. The handle is dropped on purpose:
/// `eval` has already sent the script by the time it returns, and none
/// of these writes has a value to read back.
///
/// Silently does nothing when there is no runtime, which is the host
/// test build and not a desktop window (see [`in_runtime`]).
#[cfg(not(target_arch = "wasm32"))]
fn eval(js: &str) {
    if !in_runtime() {
        return;
    }
    let _ = dioxus::document::eval(js);
}

/// Encode `s` as a JavaScript string literal so an id carrying a quote
/// cannot terminate the literal and change what the script does.
#[cfg(not(target_arch = "wasm32"))]
fn js_string(s: &str) -> String {
    // Serializing a `&str` cannot fail; the fallback is an empty literal
    // so a future change that makes it fallible produces a script that
    // matches no element rather than one that is syntactically broken.
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Desktop: the selection cannot be read back synchronously (see the module
/// header on reads), so the caller is told the caret is at the end and the
/// transform appends rather than guessing at a selection it cannot see.
#[cfg(not(target_arch = "wasm32"))]
pub fn textarea_selection(_id: &str, fallback_end: u32) -> (u32, u32) {
    (fallback_end, fallback_end)
}

/// Desktop: a write, so it CAN be done through `eval`.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_textarea_selection(id: &str, start: u32, end: u32) {
    eval(&format!(
        "{{const e=document.getElementById({});if(e){{e.focus();e.setSelectionRange({start},{end});}}}}",
        js_string(id)
    ));
}

/// MAPPS-511: the desktop half of the four behaviours that used to be
/// inert here, pinned in source.
///
/// A source scan, deliberately: every one of them needs a webview to
/// evaluate JavaScript in, so no host test can drive them. What can
/// regress is the wiring, and each of these went missing once already
/// as a stub that answered "nothing to do" and said nothing about it.
#[cfg(test)]
mod desktop_wiring_tests {
    const SRC: &str = include_str!("dom.rs");

    /// The shipping code with runs of whitespace collapsed, so an
    /// assertion pins the decision rather than the line breaks rustfmt
    /// chose. Excludes this module, which quotes every pattern it looks
    /// for and would otherwise match itself.
    fn code_only() -> String {
        let end = SRC
            .find("mod desktop_wiring_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Both hosts answer the read, and neither answers it with `None`
    /// unconditionally. The stub this replaced was invisible: the
    /// sidebar just started at the top after every navigation.
    #[test]
    fn the_scroll_offset_is_read_on_both_hosts() {
        let code = code_only();
        assert_eq!(
            code.matches("pub async fn scroll_top_async(id: &str) -> Option<i32>")
                .count(),
            2,
            "one implementation per target"
        );
        assert!(
            !code.contains("pub fn scroll_top(_id: &str) -> Option<i32> { None }"),
            "the desktop answers from its webview, not with a stub"
        );
    }

    /// The desktop records the focused element and puts focus back.
    #[test]
    fn the_focused_element_is_recorded_and_restored() {
        let code = code_only();
        assert!(
            !code.contains("pub fn restore(&self) {}"),
            "restoring focus is not a no-op on either host"
        );
        assert!(
            code.contains("window.{FOCUS_STASH}.set({token}, document.activeElement);"),
            "the desktop parks the element rather than reading it back, because \
             an async read resolves after the dialog has taken focus"
        );
    }

    /// A markdown checkbox click reaches Rust on the desktop.
    #[test]
    fn a_task_toggle_comes_back_over_the_channel() {
        let code = code_only();
        assert!(
            code.contains("pub fn watch_task_toggles(dom_id: &str"),
            "the desktop has the injected script attach the delegated listener"
        );
        assert!(
            code.contains("dioxus.send(Number(index));"),
            "and each click is posted back as its data-ti index"
        );
    }

    /// A `Theme::System` desktop user follows the OS without a restart.
    #[test]
    fn an_os_theme_change_comes_back_over_the_channel() {
        let code = code_only();
        assert!(
            code.contains("pub fn watch_system_theme("),
            "the desktop subscribes to the OS light/dark preference"
        );
        assert!(
            code.contains("dioxus.send(e.matches));"),
            "through the same `prefers-color-scheme` query the browser uses, \
             because tao only emits ThemeChanged on macOS and Windows"
        );
    }
}

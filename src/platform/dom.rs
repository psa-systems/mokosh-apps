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
//! which is asynchronous, and the callers here are synchronous. Those
//! answer `None` on the desktop and the caller skips the behaviour
//! rather than blocking or guessing.

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

#[derive(Clone)]
#[cfg(not(target_arch = "wasm32"))]
pub struct FocusToken;

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

/// Reading `document.activeElement` needs a value back out of the
/// webview, which is asynchronous, so the desktop build records nothing
/// and restores nothing: closing a modal leaves focus where the webview
/// put it instead of returning it to the trigger. Tracked in MAPPS-511.
#[cfg(not(target_arch = "wasm32"))]
pub fn capture_focus() -> FocusToken {
    FocusToken
}

#[cfg(not(target_arch = "wasm32"))]
impl FocusToken {
    pub fn restore(&self) {}
}

/// Current scroll offset of the element with this id.
#[cfg(target_arch = "wasm32")]
pub fn scroll_top(id: &str) -> Option<i32> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .map(|el| el.scroll_top())
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

/// The window's theme as the OS reports it to tao.
///
/// Without the `desktop` feature there is no window to ask - that build
/// is `cargo check` / `cargo test` on the host, which renders nothing -
/// so it reports light and the caller's explicit preference still wins.
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
pub fn system_prefers_dark() -> bool {
    let Some(ctx) = dioxus::prelude::try_consume_context::<dioxus::desktop::DesktopContext>()
    else {
        return false;
    };
    ctx.window.theme() == dioxus::desktop::tao::window::Theme::Dark
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "desktop")))]
pub fn system_prefers_dark() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn focus_by_id(id: &str) {
    eval(&format!(
        "document.getElementById({})?.focus();",
        js_string(id)
    ));
}

/// Always `None` on the desktop: reading a value out of the webview is
/// asynchronous and this call is not. The one caller (the sidebar
/// scroll memory in `components::layout`) treats `None` as "nothing to
/// restore", so the sidebar simply starts at the top after a
/// navigation. Tracked in MAPPS-511.
#[cfg(not(target_arch = "wasm32"))]
pub fn scroll_top(_id: &str) -> Option<i32> {
    None
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

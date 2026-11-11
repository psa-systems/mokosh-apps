//! MAPPS-292: unsaved-changes guard for long forms.
//!
//! A form opts in by calling [`use_unsaved_guard`] with a `ReadSignal<bool>`
//! that reads true while the form is dirty. The hook mirrors that signal into
//! [`UNSAVED_CHANGES`], the app-wide dirty flag, and the host decides what to
//! do with it:
//!
//! - Browser: a single `beforeunload` handler on `window` prompts the user
//!   when they try to reload, close the tab, or navigate to an external URL.
//!   The prompt text is controlled by the browser (most modern browsers show
//!   a generic "Leave site?" dialog regardless of the string returned; we set
//!   `returnValue` to a non-empty string for legacy compatibility).
//! - Desktop: `platform::window_close` intercepts the window close request and
//!   raises the app's own confirmation modal (MAPPS-506).
//!
//! Both read [`UNSAVED_CHANGES`], so the two hosts prompt off one flag rather
//! than off two copies that can disagree.
//!
//! Scope. This hook covers the OS-level navigation triggers
//! (`beforeunload`, the window close request). It does NOT yet intercept
//! Dioxus router transitions (Link clicks, navigator.push) because Dioxus
//! 0.7's router does not expose a transition-guard API; that part of the
//! unsaved-changes story is tracked as MAPPS-910. The browser cover is the
//! highest-impact half of the data-loss surface the QA report flagged (closing
//! the tab on a half-filled Company form lost ~10 fields silently).
//!
//! Cleanup (MAPPS-1240). There is exactly one `beforeunload` listener for
//! the app, installed by the first guarded form to mount and kept for the
//! life of the page; it reads the shared flag, so it never needs replacing.
//! The previous design stored the closure and replaced it on every mount,
//! on the belief that dropping a `wasm_bindgen::Closure` detaches its
//! listener. It does not: `removeEventListener` is never called, so each
//! mount added a listener and the replaced ones threw when invoked.
//!
//! Several guarded forms can be mounted at once, so the flag is not any one
//! form's dirty bit: each form registers its own state in [`DIRTY_FORMS`]
//! and the flag is "some mounted form is dirty". Unmounting a clean form no
//! longer clears the flag a dirty sibling raised, and the last form's mirror
//! no longer overwrites the others.

#[cfg(feature = "app")]
use dioxus::prelude::*;
#[cfg(all(feature = "app", target_arch = "wasm32"))]
use wasm_bindgen::prelude::*;
#[cfg(all(feature = "app", target_arch = "wasm32"))]
use wasm_bindgen::JsCast;

/// MAPPS-506: the app-wide "a mounted form holds unsaved edits" flag, and the
/// only one. `beforeunload` on the web and the desktop window-close guard both
/// read it, so a change to what counts as dirty reaches both hosts at once.
///
/// A `GlobalSignal` for the same reason [`crate::hooks::fetch::SESSION_ENDED`]
/// is one: it is read from outside the component tree that raises it (a JS
/// event listener, a `tao` event handler), which no context signal can reach.
#[cfg(feature = "app")]
pub static UNSAVED_CHANGES: GlobalSignal<bool> = Signal::global(|| false);

#[cfg(all(feature = "app", target_arch = "wasm32"))]
type BeforeUnloadClosure = Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>;

#[cfg(all(feature = "app", target_arch = "wasm32"))]
thread_local! {
    /// Whether the app's one `beforeunload` listener is attached. Only set
    /// once the attach succeeded, so a failure is retried by the next mount.
    static BEFOREUNLOAD_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(feature = "app")]
thread_local! {
    /// The mounted guarded forms that are dirty right now, by guard id
    /// (MAPPS-1240). [`UNSAVED_CHANGES`] is "this is not empty".
    static DIRTY_FORMS: std::cell::RefCell<std::collections::BTreeSet<u64>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
    static NEXT_GUARD_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A fresh id for one mounted guard.
#[cfg(feature = "app")]
fn next_guard_id() -> u64 {
    NEXT_GUARD_ID.with(|n| {
        let id = n.get();
        n.set(id.wrapping_add(1));
        id
    })
}

/// Record guard `id`'s dirty state; answers whether any form is dirty.
#[cfg(feature = "app")]
fn note_form_dirty(id: u64, dirty: bool) -> bool {
    DIRTY_FORMS.with(|set| {
        let mut set = set.borrow_mut();
        if dirty {
            set.insert(id);
        } else {
            set.remove(&id);
        }
        !set.is_empty()
    })
}

/// Forget guard `id` (its form unmounted); answers whether any other form
/// is still dirty.
#[cfg(feature = "app")]
fn forget_form(id: u64) -> bool {
    note_form_dirty(id, false)
}

/// Publish `dirty()` into the app-wide unsaved-changes flag for as long as
/// the calling form is mounted, and (on the web) make sure the one
/// `beforeunload` prompt that reads it is installed. The flag is raised
/// while ANY mounted guarded form is dirty (MAPPS-1240).
#[cfg(feature = "app")]
pub fn use_unsaved_guard(dirty: ReadSignal<bool>) {
    // Resolve the global once, here in the component, where the Dioxus runtime
    // is current. The resolved handle is what the `beforeunload` closure and
    // the unmount cleanup below use, neither of which runs in a scope.
    let mut shared = use_hook(|| UNSAVED_CHANGES.signal());
    let id = use_hook(next_guard_id);

    use_effect(move || {
        let any_dirty = note_form_dirty(id, *dirty.read());
        if *shared.peek() != any_dirty {
            shared.set(any_dirty);
        }
    });

    // An unmounted form took its edits with it. Leaving its mark raised would
    // make the next close prompt about a form that is no longer on screen,
    // but a sibling that is still dirty keeps the flag up.
    let any_dirty_after = move || forget_form(id);
    use_drop(move || match shared.try_write() {
        Ok(mut flag) => *flag = any_dirty_after(),
        // Only reachable when the app itself is being torn down and the store
        // went first, so there is nothing left to guard. Panicking out of a
        // Drop during shutdown would replace a clean exit with a backtrace.
        Err(err) => {
            tracing::debug!(error = ?err, "use_unsaved_guard: shared flag already dropped")
        }
    });

    #[cfg(target_arch = "wasm32")]
    use_beforeunload(shared);
}

/// Install the browser prompt. Reads `shared` rather than the form's own
/// signal so the web path and the desktop path consult the same flag.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
fn use_beforeunload(shared: Signal<bool>) {
    use_effect(move || {
        if BEFOREUNLOAD_INSTALLED.with(std::cell::Cell::get) {
            return;
        }
        let Some(win) = web_sys::window() else {
            tracing::warn!("use_unsaved_guard: no window to install the beforeunload prompt on");
            return;
        };
        let cb: BeforeUnloadClosure =
            Closure::wrap(Box::new(move |e: web_sys::BeforeUnloadEvent| {
                // MAPPS-299: the `beforeunload` listener stays installed
                // until wasm is replaced, which can outlive the signal
                // it reads (e.g. a sibling crash tears the component
                // tree down before the listener fires). `Signal::read`
                // panics on a dropped signal (`ValueDroppedError`),
                // which would then surface as a secondary panic during
                // teardown. Read via `try_read` and treat a dropped
                // signal as "not dirty" (no prompt), saying so in the log so
                // a missing prompt is not silent.
                let is_dirty = match shared.try_read() {
                    Ok(flag) => *flag,
                    Err(err) => {
                        tracing::debug!(error = ?err, "beforeunload: shared flag already dropped");
                        false
                    }
                };
                if is_dirty {
                    // Modern browsers ignore the message string but
                    // still show the prompt when `returnValue` is set
                    // to anything non-empty. The empty default skips
                    // the prompt, so we explicitly opt in here.
                    e.set_return_value("You have unsaved changes.");
                }
            }));
        if let Err(err) =
            win.add_event_listener_with_callback("beforeunload", cb.as_ref().unchecked_ref())
        {
            tracing::warn!(error = ?err, "use_unsaved_guard: addEventListener failed");
            return;
        }
        // Kept for the life of the page: the listener reads the shared flag,
        // so it is never replaced, and dropping the closure would leave the
        // still-registered listener throwing on every unload.
        cb.forget();
        BEFOREUNLOAD_INSTALLED.with(|i| i.set(true));
    });
}

/// Stub for a build with no app runtime (the dirty flag it would publish has
/// no reader there). Every call site is unchanged.
#[cfg(not(feature = "app"))]
pub fn use_unsaved_guard(_dirty: dioxus::prelude::ReadSignal<bool>) {}

/// MAPPS-850: the two-host invariant, source-scanned so a future refactor
/// cannot silently break it. Both hosts (browser via `beforeunload`,
/// desktop via `platform::window_close`) read the same `UNSAVED_CHANGES`
/// signal; deleting either arm removes the ONLY unsaved-changes guard on
/// that host, and both have been mis-flagged as "stale" in an earlier
/// audit (MAPPS-845). The test reads the two sources directly so the
/// scan runs on native `cargo test --lib` without a browser or a desktop
/// runtime.
/// MAPPS-1240: the dirty flag is "some mounted form is dirty", and the
/// browser listener is installed once rather than replaced.
#[cfg(all(test, feature = "app"))]
mod mapps_1240_tests {
    use super::{forget_form, next_guard_id, note_form_dirty};

    #[test]
    fn a_clean_sibling_unmounting_leaves_a_dirty_form_guarded() {
        let (a, b) = (next_guard_id(), next_guard_id());
        assert!(note_form_dirty(a, true));
        assert!(
            note_form_dirty(b, false),
            "b's clean state does not overwrite a's dirty one"
        );
        assert!(forget_form(b), "b unmounting leaves a guarded");
        assert!(
            !forget_form(a),
            "the last dirty form unmounting clears the flag"
        );
    }

    #[test]
    fn the_flag_tracks_any_dirty_form() {
        let (a, b) = (next_guard_id(), next_guard_id());
        assert!(note_form_dirty(a, true));
        assert!(note_form_dirty(b, true));
        assert!(note_form_dirty(a, false), "b is still dirty");
        assert!(!note_form_dirty(b, false));
    }

    #[test]
    fn the_beforeunload_listener_is_installed_once_and_never_dropped() {
        let src = include_str!("unsaved_guard.rs");
        let code = &src[..src.find("mod mapps_1240_tests").unwrap()];
        assert!(
            !code.contains("UNSAVED_HANDLER"),
            "no stored closure to replace"
        );
        assert!(
            !code.contains("Closure` drop semantics"),
            "the false claim is gone from the docs"
        );
        assert!(code.contains("if BEFOREUNLOAD_INSTALLED.with(std::cell::Cell::get) {"));
        assert!(code.contains("cb.forget();"));
    }
}

#[cfg(test)]
mod mapps850_two_host_guard_invariant_tests {
    /// The browser arm: `use_unsaved_guard` on `wasm32` installs a
    /// `beforeunload` listener whose closure reads `UNSAVED_CHANGES`,
    /// and the same signal is what `dirty` writes into. Removing any
    /// one of these three pieces removes the browser's only guard.
    #[test]
    fn the_browser_arm_installs_beforeunload_reading_unsaved_changes() {
        let src = include_str!("unsaved_guard.rs");
        // The wasm32 branch is what installs the listener.
        assert!(
            src.contains("#[cfg(all(target_arch = \"wasm32\", feature = \"app\"))]")
                || src.contains("#[cfg(target_arch = \"wasm32\")]"),
            "the browser branch guards on wasm32; if the cfg shape changed, update this test"
        );
        assert!(
            src.contains("add_event_listener_with_callback(\"beforeunload\","),
            "the browser arm adds a beforeunload listener; MAPPS-850 pins this"
        );
        // The listener body reads the shared signal.
        assert!(
            src.contains("UNSAVED_CHANGES"),
            "the beforeunload handler must read UNSAVED_CHANGES so the two hosts do not disagree"
        );
    }

    /// The desktop arm: `platform::window_close` is where the desktop
    /// half of the guard lives, and its module doc explicitly names
    /// `beforeunload` as the browser arm so the two are impossible to
    /// mistake for duplicates. Both docs are load-bearing evidence that
    /// closes MAPPS-850 (and preempts its likely re-audit).
    #[test]
    fn the_desktop_arm_lives_in_platform_window_close_and_references_the_browser_arm() {
        let desktop = include_str!("../platform/window_close.rs");
        assert!(
            desktop.contains("beforeunload"),
            "platform::window_close doc names beforeunload as the browser arm; MAPPS-850 pins this cross-reference"
        );
        // A future refactor MIGHT rename the internal state; the name of
        // the shared signal is fixed by both arms, so the desktop arm
        // must still read it (directly or through `is_dirty()`).
        assert!(
            desktop.contains("UNSAVED_CHANGES") || desktop.contains("unsaved_changes"),
            "platform::window_close reads the shared UNSAVED_CHANGES signal"
        );
    }

    /// The module doc names the two-arm shape. A future rewrite that
    /// drops the "browser owns beforeunload; desktop owns window_close"
    /// paragraph is exactly the shape MAPPS-845 misread, so pin it.
    #[test]
    fn the_module_doc_names_both_arms() {
        let src = include_str!("unsaved_guard.rs");
        assert!(
            src.contains("Browser:"),
            "the two-arm doc names the browser arm"
        );
        assert!(
            src.contains("Desktop:"),
            "the two-arm doc names the desktop arm"
        );
    }
}

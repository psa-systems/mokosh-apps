//! MAPPS-292: unsaved-changes guard for long forms.
//!
//! A form opts in by calling [`use_unsaved_guard`] with a `ReadSignal<bool>`
//! that reads true while the form is dirty. The hook installs a single
//! `beforeunload` handler on `window` that prompts the user when they
//! try to reload, close the tab, or navigate to an external URL. The
//! prompt text is controlled by the browser (most modern browsers show
//! a generic "Leave site?" dialog regardless of the string returned;
//! we set `returnValue` to a non-empty string for legacy compatibility).
//!
//! Scope. This hook covers the OS-level navigation triggers
//! (`beforeunload`). It does NOT yet intercept Dioxus router transitions
//! (Link clicks, navigator.push) because Dioxus 0.7's router does not
//! expose a transition-guard API; that part of the unsaved-changes
//! story is tracked as a follow-up. The browser cover is the highest-
//! impact half of the data-loss surface the QA report flagged (closing
//! the tab on a half-filled Company form lost ~10 fields silently).
//!
//! Cleanup. The hook captures a closure that holds a reference to the
//! `dirty` signal; on unmount Dioxus drops the `use_effect`, but the
//! window listener stays installed until the wasm module is replaced.
//! We track the registered closure handle via a per-call thread-local
//! so a remount swaps the listener instead of stacking duplicates.

#[cfg(all(feature = "app", target_arch = "wasm32"))]
use dioxus::prelude::*;
#[cfg(all(feature = "app", target_arch = "wasm32"))]
use wasm_bindgen::prelude::*;
#[cfg(all(feature = "app", target_arch = "wasm32"))]
use wasm_bindgen::JsCast;

#[cfg(all(feature = "app", target_arch = "wasm32"))]
type BeforeUnloadClosure = Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>;

#[cfg(all(feature = "app", target_arch = "wasm32"))]
thread_local! {
    /// Currently-installed `beforeunload` handler, if any. Replacing the
    /// hook on a remount drops the prior closure, which detaches its
    /// listener via wasm_bindgen's `Closure` drop semantics.
    static UNSAVED_HANDLER: std::cell::RefCell<Option<BeforeUnloadClosure>> =
        const { std::cell::RefCell::new(None) };
}

/// Install a `beforeunload` prompt that triggers while `dirty()` is true.
/// Calling the hook with a new signal on a remount swaps the listener
/// (the previous closure is dropped, detaching its handler).
#[cfg(all(feature = "app", target_arch = "wasm32"))]
pub fn use_unsaved_guard(dirty: ReadSignal<bool>) {
    use_effect(move || {
        let dirty = dirty;
        let Some(win) = web_sys::window() else {
            return;
        };
        let cb: BeforeUnloadClosure =
            Closure::wrap(Box::new(move |e: web_sys::BeforeUnloadEvent| {
                // MAPPS-299: the `beforeunload` listener stays installed
                // until wasm is replaced, which can outlive the signal
                // it captures (e.g. a sibling crash tears the
                // component tree down before the listener fires).
                // `Signal::read` panics on a dropped signal
                // (`ValueDroppedError`), which would then surface as a
                // secondary panic during teardown. Read via `try_read`
                // and treat a dropped signal as "not dirty" (no prompt).
                let is_dirty = dirty.try_read().map(|r| *r).unwrap_or(false);
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
        UNSAVED_HANDLER.with(|slot| {
            *slot.borrow_mut() = Some(cb);
        });
    });
}

/// MAPPS-504: `beforeunload` is a browser event about leaving a
/// document, and a desktop window neither unloads nor prompts. Closing
/// the desktop window with unsaved changes currently discards them
/// without asking; the window-close interception that fixes it is
/// MAPPS-506. The dirty tracking itself is target-agnostic and every
/// call site is unchanged.
#[cfg(any(not(feature = "app"), not(target_arch = "wasm32")))]
pub fn use_unsaved_guard(_dirty: dioxus::prelude::ReadSignal<bool>) {}

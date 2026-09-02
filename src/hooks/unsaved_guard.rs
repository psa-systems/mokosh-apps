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
//! unsaved-changes story is tracked as a follow-up. The browser cover is the
//! highest-impact half of the data-loss surface the QA report flagged (closing
//! the tab on a half-filled Company form lost ~10 fields silently).
//!
//! Cleanup. The hook captures a closure that holds a reference to the
//! shared signal; on unmount Dioxus drops the `use_effect`, but the
//! window listener stays installed until the wasm module is replaced.
//! We track the registered closure handle via a per-call thread-local
//! so a remount swaps the listener instead of stacking duplicates.

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
    /// Currently-installed `beforeunload` handler, if any. Replacing the
    /// hook on a remount drops the prior closure, which detaches its
    /// listener via wasm_bindgen's `Closure` drop semantics.
    static UNSAVED_HANDLER: std::cell::RefCell<Option<BeforeUnloadClosure>> =
        const { std::cell::RefCell::new(None) };
}

/// Publish `dirty()` as the app-wide unsaved-changes flag for as long as the
/// calling form is mounted, and (on the web) install the `beforeunload` prompt
/// that reads it. Calling the hook with a new signal on a remount swaps the
/// listener (the previous closure is dropped, detaching its handler).
#[cfg(feature = "app")]
pub fn use_unsaved_guard(dirty: ReadSignal<bool>) {
    // Resolve the global once, here in the component, where the Dioxus runtime
    // is current. The resolved handle is what the `beforeunload` closure and
    // the unmount cleanup below use, neither of which runs in a scope.
    let mut shared = use_hook(|| UNSAVED_CHANGES.signal());

    use_effect(move || {
        let is_dirty = *dirty.read();
        if *shared.peek() != is_dirty {
            shared.set(is_dirty);
        }
    });

    // An unmounted form took its edits with it. Leaving the flag raised would
    // make the next close prompt about a form that is no longer on screen.
    use_drop(move || match shared.try_write() {
        Ok(mut flag) => *flag = false,
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
        let Some(win) = web_sys::window() else {
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
        UNSAVED_HANDLER.with(|slot| {
            *slot.borrow_mut() = Some(cb);
        });
    });
}

/// Stub for a build with no app runtime (the dirty flag it would publish has
/// no reader there). Every call site is unchanged.
#[cfg(not(feature = "app"))]
pub fn use_unsaved_guard(_dirty: dioxus::prelude::ReadSignal<bool>) {}

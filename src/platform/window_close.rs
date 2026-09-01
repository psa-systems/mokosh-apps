//! What "close the window" means on each host (MAPPS-506).
//!
//! The browser owns its own close: `beforeunload` (see
//! `hooks::unsaved_guard`) puts the native "leave site?" prompt in front of a
//! tab close or reload, and nothing here is needed. A desktop window neither
//! unloads nor prompts, so the close request is intercepted here and answered
//! with the app's own [`crate::components::CloseConfirmModal`], in the app's
//! own wording.
//!
//! ## Why this file splits on `feature = "desktop"`
//!
//! The rest of `src/platform/` splits on `target_arch` (see `mod.rs`), because
//! what changes is the machine the same code runs on. This module is the
//! exception: it drives the `tao` event loop, which exists only when the
//! desktop *renderer* is linked. A native `cargo clippy` of the web build is
//! `not(target_arch = "wasm32")` and has no `dioxus::desktop`, so the split has
//! to follow the feature that decides whether that module is there at all.
//!
//! ## How the close request is intercepted
//!
//! `dioxus-desktop` answers `WindowEvent::CloseRequested` by either destroying
//! the webview or hiding the window, and offers no "ignore it" (see
//! `WindowCloseBehaviour`). The window is therefore launched as `WindowHides`,
//! so a close request can never destroy unsaved work, and this module decides
//! what happens instead. Handlers registered with `use_wry_event_handler` run
//! BEFORE `dioxus-desktop` acts on the same event, which is what makes both
//! answers reachable from one place:
//!
//! - Nothing unsaved: switch the window to `WindowCloses` and let the same
//!   event through. `dioxus-desktop` destroys the webview on the spot and the
//!   app exits, so a clean close is one click with no prompt and no delay.
//! - Unsaved changes: leave the window on `WindowHides` and raise the modal.
//!   The webview, and every edit in it, survives. `dioxus-desktop` does hide
//!   the window on its way past, so the handler re-shows it on the next event
//!   (`set_visible` is the only lever it left, and it cannot be pulled before
//!   the hide is queued). Cancelling leaves the window open on the form;
//!   confirming switches to `WindowCloses` and closes for real.
//!
//! That hide-then-re-show can blink the window, which is what an upstream
//! close behaviour that ignores the request would remove. Whether it blinks in
//! practice has not been observed on a real display: MAPPS-631.

#[cfg(feature = "desktop")]
pub use desktop::{cancel_close, close_confirm_open, confirm_close, use_close_guard};

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::tao::event::{Event, WindowEvent};
    use dioxus::desktop::{use_window, use_wry_event_handler, window, WindowCloseBehaviour};
    use dioxus::prelude::*;

    /// Raised while the confirmation modal is up. A `GlobalSignal` because the
    /// `tao` event handler that raises it has no component to hold it.
    static CLOSE_CONFIRM: GlobalSignal<bool> = Signal::global(|| false);

    /// Watch for a window close request and gate it on the unsaved-changes
    /// flag. Mount once, at the app root.
    pub fn use_close_guard() {
        let window = use_window();
        // Raised when a close request has been refused, lowered by the re-show
        // it asks for. A `Cell`, not a signal: it is read and written inside
        // the tao event handler and nothing renders from it.
        let reshow = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(false)));

        use_wry_event_handler(move |event, _| {
            if let Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } = event
            {
                // `peek`, not `read`: this runs in the app's scope, and
                // subscribing the root component to either flag would
                // re-render the whole tree on every keystroke in a guarded
                // form.
                let dirty = *crate::hooks::unsaved_guard::UNSAVED_CHANGES.peek();
                let asking = *CLOSE_CONFIRM.peek();
                if dirty || asking {
                    // Refused. The window is on `WindowHides`, so the webview
                    // and its edits survive; ask (or keep asking), and undo
                    // the hide that comes with the refusal.
                    if !asking {
                        *CLOSE_CONFIRM.write() = true;
                    }
                    reshow.set(true);
                } else {
                    window.set_close_behavior(WindowCloseBehaviour::WindowCloses);
                }
                return;
            }

            // Any later event, which is the first moment at which the hide
            // `dioxus-desktop` queued for the refused request has happened.
            if reshow.replace(false) {
                window.set_visible(true);
            }
        });
    }

    /// Is the desktop close confirmation up? Reactive: the component that
    /// renders the modal re-renders when this flips.
    pub fn close_confirm_open() -> bool {
        *CLOSE_CONFIRM.read()
    }

    /// Keep the window open and the edits intact.
    pub fn cancel_close() {
        *CLOSE_CONFIRM.write() = false;
    }

    /// Discard the unsaved edits and close the window for real.
    pub fn confirm_close() {
        *CLOSE_CONFIRM.write() = false;
        let win = window();
        win.set_close_behavior(WindowCloseBehaviour::WindowCloses);
        win.close();
    }
}

/// The browser prompts for itself, from `beforeunload`. Nothing intercepts a
/// close here, so the modal never opens and these are inert.
#[cfg(not(feature = "desktop"))]
pub fn use_close_guard() {}

#[cfg(not(feature = "desktop"))]
pub fn close_confirm_open() -> bool {
    false
}

#[cfg(not(feature = "desktop"))]
pub fn cancel_close() {}

#[cfg(not(feature = "desktop"))]
pub fn confirm_close() {}

#[cfg(test)]
mod tests {
    /// This module's own source, minus this test module: the assertion below
    /// names the very call it requires, so scanning the whole file would make
    /// it match itself.
    fn production_src() -> &'static str {
        const SRC: &str = include_str!("window_close.rs");
        SRC.split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file has a test module")
    }

    /// MAPPS-506 recurrence guard. The desktop close path is only reachable
    /// with a `tao` event loop and a real window, so what a host test can pin
    /// is the wiring the behaviour rests on.
    ///
    /// `WindowCloses` destroys the webview the moment a close is requested,
    /// which is before anything can ask about the edits in it. Launching on
    /// `WindowHides` is what leaves the guard a decision to make.
    #[test]
    fn the_desktop_window_does_not_destroy_itself_on_a_close_request() {
        const MAIN_SRC: &str = include_str!("../main.rs");
        assert!(
            MAIN_SRC.contains("with_close_behaviour(WindowCloseBehaviour::WindowHides)"),
            "the desktop window must launch non-destructive, or a close request \
             discards unsaved edits before the guard sees it"
        );
    }

    /// MAPPS-506: one dirty flag, read by both hosts. A local copy here would
    /// drift from the one `beforeunload` consults on the web.
    #[test]
    fn the_close_guard_reads_the_shared_unsaved_flag() {
        assert!(
            production_src().contains("crate::hooks::unsaved_guard::UNSAVED_CHANGES"),
            "the close guard must read the app-wide unsaved-changes flag"
        );
    }
}

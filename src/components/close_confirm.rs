//! MAPPS-506: the desktop window-close confirmation.
//!
//! The web build gets the browser's own "leave site?" dialog from
//! `beforeunload`. A desktop window has no such dialog, and a system message
//! box in the middle of an app that confirms everything else through
//! [`crate::components::ConfirmDialog`] reads as a different application, so
//! the desktop close is confirmed with the same dialog every other
//! discard-your-work prompt uses.
//!
//! Mounted at the app root. `close_confirm_open()` is a constant false on the
//! web, so this adds no DOM there.

use dioxus::prelude::*;

use super::modal::ConfirmDialog;
use crate::platform::window_close;

/// Asks before a desktop window close throws away unsaved edits. Renders
/// nothing until `platform::window_close` refuses a close request.
#[component]
pub fn CloseConfirmModal() -> Element {
    rsx! {
        ConfirmDialog {
            open: window_close::close_confirm_open(),
            title: "Unsaved changes".to_string(),
            message: "This window has unsaved changes. Closing it now discards them.".to_string(),
            confirm_text: "Discard and close".to_string(),
            cancel_text: "Keep editing".to_string(),
            destructive: true,
            onconfirm: move |_| window_close::confirm_close(),
            oncancel: move |_| window_close::cancel_close(),
        }
    }
}

//! Global toast notification surface.
//!
//! Any code in the app can push a toast via [`push_toast`] without
//! threading state down the component tree. [`ToastRoot`] is mounted
//! once inside [`crate::components::AppShell`] and renders the
//! current toast stack at the bottom-right of the viewport.

use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;

use crate::components::{AlertType, Toast, ToastContainer};

/// Backing store. WASM is single-threaded so a `GlobalSignal` is the
/// right primitive: every component can read it and re-render on
/// change, and every event handler can mutate it from any closure
/// without setting up a context provider.
pub static TOASTS: GlobalSignal<Vec<Toast>> = Signal::global(Vec::new);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// MAPPS-312: per-`AlertType` auto-dismiss default. Success / Info
/// land for 5s then fade; Warning / Error stay sticky so a real
/// failure cannot be missed by a user who tabbed away.
fn default_auto_dismiss_ms(alert_type: AlertType) -> Option<u32> {
    match alert_type {
        AlertType::Success | AlertType::Info => Some(5_000),
        AlertType::Warning | AlertType::Error => None,
    }
}

/// Push a toast onto the global stack.
///
/// `alert_type` controls the icon and color (`Info`, `Success`,
/// `Warning`, `Error`). `message` is the body line shown to the user.
pub fn push_toast(alert_type: AlertType, message: impl Into<String>) {
    let id = format!("t-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    TOASTS.write().push(Toast {
        id,
        toast_type: alert_type,
        message: message.into(),
        title: None,
        auto_dismiss_ms: default_auto_dismiss_ms(alert_type),
    });
}

/// Push a toast with an explicit title above the message body.
pub fn push_toast_with_title(
    alert_type: AlertType,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    let id = format!("t-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    TOASTS.write().push(Toast {
        id,
        toast_type: alert_type,
        message: message.into(),
        title: Some(title.into()),
        auto_dismiss_ms: default_auto_dismiss_ms(alert_type),
    });
}

/// Convenience wrapper for the common "an API call failed" case. Maps
/// any [`crate::hooks::fetch::api::ApiError`] to a user-facing message
/// via [`ApiError::user_message`].
#[cfg(feature = "app")]
pub fn push_api_error(err: &crate::hooks::fetch::api::ApiError) {
    push_toast(AlertType::Error, err.user_message());
}

/// Mounts the toast container. Place once inside the persistent
/// layout; do not mount per-page.
#[component]
pub fn ToastRoot() -> Element {
    let toasts_snapshot = TOASTS.read().clone();
    if toasts_snapshot.is_empty() {
        return rsx! {};
    }
    rsx! {
        ToastContainer {
            toasts: toasts_snapshot,
            ondismiss: move |id: String| {
                TOASTS.write().retain(|t| t.id != id);
            },
        }
    }
}

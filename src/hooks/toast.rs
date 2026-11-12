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
// SPEC-4.45.1: one global stack; SPEC-4.45.2: ids come from one counter from 0.
pub static TOASTS: GlobalSignal<Vec<Toast>> = Signal::global(Vec::new);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// MAPPS-312: per-`AlertType` auto-dismiss default. Success / Info
/// land for 5s then fade; Warning / Error stay sticky so a real
/// failure cannot be missed by a user who tabbed away.
fn default_auto_dismiss_ms(alert_type: AlertType) -> Option<u32> {
    // SPEC-4.45.3: 5 s for Success/Info, sticky for Warning/Error.
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
        // SPEC-4.45.4: no title on a plain toast.
        title: None,
        auto_dismiss_ms: default_auto_dismiss_ms(alert_type),
        retry: None,
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
        // SPEC-4.45.4: the title rides above the message.
        title: Some(title.into()),
        auto_dismiss_ms: default_auto_dismiss_ms(alert_type),
        retry: None,
    });
}

/// Convenience wrapper for the common "an API call failed" case. Maps
/// any [`crate::hooks::fetch::api::ApiError`] to a user-facing message
/// via [`ApiError::user_message`].
#[cfg(feature = "app")]
pub fn push_api_error(err: &crate::hooks::fetch::api::ApiError) {
    // SPEC-4.45.5: an Error toast carrying the API error's user message.
    push_toast(AlertType::Error, err.user_message());
}

/// MAPPS-1114: same as [`push_api_error`] but attaches a Retry link to
/// the toast. The handler is invoked after the toast is dismissed, so a
/// repeat failure can push a fresh toast cleanly.
///
/// The caller passes a `Callback<()>` so the handler can be built with
/// `use_callback(...)` and keep its captured signals valid across
/// re-renders.
#[cfg(feature = "app")]
pub fn push_api_error_with_retry(
    err: &crate::hooks::fetch::api::ApiError,
    retry: EventHandler<()>,
) {
    let id = format!("t-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    TOASTS.write().push(Toast {
        id,
        toast_type: AlertType::Error,
        message: err.user_message(),
        title: None,
        // SPEC-4.45.6: sticky Error toast with the retry handler attached.
        auto_dismiss_ms: None,
        retry: Some(retry),
    });
}

/// Mounts the toast container. Place once inside the persistent
/// layout; do not mount per-page.
#[component]
pub fn ToastRoot() -> Element {
    let toasts_snapshot = TOASTS.read().clone();
    // SPEC-4.45.7: nothing renders for an empty stack.
    if toasts_snapshot.is_empty() {
        return rsx! {};
    }
    rsx! {
        ToastContainer {
            toasts: toasts_snapshot,
            ondismiss: move |id: String| {
                // SPEC-4.45.7: dismiss removes only the toast with this id.
                TOASTS.write().retain(|t| t.id != id);
            },
        }
    }
}

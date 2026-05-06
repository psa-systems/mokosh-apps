//! Google OAuth login hook (popup + postMessage flow).
//!
//! Opens `/api/v1/auth/google` in a centered popup. The mokosh-server
//! callback page closes the popup and posts the JWT response back here
//! via `window.postMessage`. We accept the message only if its origin
//! equals our own (the SPA and API share an origin in dev via the
//! Dioxus `[[web.proxy]]` and in prod via an outer reverse proxy).

use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::MessageEvent;

use crate::modules::auth::CurrentUser;
use crate::Route;

const POPUP_NAME: &str = "mokosh-google";
const POPUP_FEATURES: &str =
    "width=500,height=600,scrollbars=yes,status=yes,toolbar=no,menubar=no,location=no";
const AUTHORIZE_PATH: &str = "/api/v1/auth/google";
const MESSAGE_TYPE: &str = "mokosh-google-auth";

/// Visible state of the Google sign-in flow.
#[derive(Clone, Debug, PartialEq)]
pub enum GoogleLoginStatus {
    Idle,
    InProgress,
    Error(String),
}

/// Handle returned by [`use_google_login`].
#[derive(Clone)]
pub struct GoogleLoginHandle {
    pub status: Signal<GoogleLoginStatus>,
    pub start: Callback<()>,
}

#[derive(Deserialize)]
struct GoogleAuthMessage {
    #[serde(rename = "type")]
    msg_type: String,
    payload: GoogleAuthPayload,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GoogleAuthPayload {
    Success { data: LoginPayload },
    Error { error: String },
}

#[derive(Deserialize)]
struct LoginPayload {
    user: CurrentUser,
    // `access_token`, `refresh_token`, `expires_at`, `mfa_required`
    // arrive too but are not persisted today (matches the password
    // login placeholder; persistence will land in a separate task).
}

/// Hook: returns a [`GoogleLoginHandle`] whose `start` callback opens
/// the OAuth popup and listens for the postMessage response.
pub fn use_google_login() -> GoogleLoginHandle {
    let mut status = use_signal(|| GoogleLoginStatus::Idle);
    let mut auth = super::auth::use_auth();
    let navigator = use_navigator();

    let start = use_callback(move |_| {
        *status.write() = GoogleLoginStatus::InProgress;

        let win = match web_sys::window() {
            Some(w) => w,
            None => {
                *status.write() = GoogleLoginStatus::Error("no window".to_string());
                return;
            }
        };

        let origin = match win.location().origin() {
            Ok(o) => o,
            Err(_) => {
                *status.write() = GoogleLoginStatus::Error(
                    "could not read window.location.origin".to_string(),
                );
                return;
            }
        };

        let url = format!("{origin}{AUTHORIZE_PATH}");
        match win.open_with_url_and_target_and_features(&url, POPUP_NAME, POPUP_FEATURES) {
            Ok(Some(_popup)) => {}
            _ => {
                *status.write() = GoogleLoginStatus::Error(
                    "popup blocked - allow popups for this site".to_string(),
                );
                return;
            }
        }

        // Build the message handler closure. The closure must outlive
        // its synchronous registration so we cannot let it drop here -
        // `forget()` keeps it alive for the page lifetime. Cleaning
        // up after a single message would require holding the closure
        // in a Signal and removing the listener; the simplification
        // is acceptable since the popup flow runs at most a handful
        // of times per session.
        let mut auth_for_handler = auth;
        let nav = navigator;
        let mut status_for_handler = status;
        let expected_origin = origin.clone();
        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            // Origin check: only accept messages from our own origin.
            if event.origin() != expected_origin {
                return;
            }

            // JSON-roundtrip the JsValue into a typed Rust struct,
            // avoiding a serde-wasm-bindgen dependency.
            let json_str = match js_sys::JSON::stringify(&event.data()) {
                Ok(s) => s.as_string().unwrap_or_default(),
                Err(_) => return,
            };
            let msg: GoogleAuthMessage = match serde_json::from_str(&json_str) {
                Ok(m) => m,
                Err(_) => return,
            };
            if msg.msg_type != MESSAGE_TYPE {
                return;
            }
            match msg.payload {
                GoogleAuthPayload::Success { data } => {
                    auth_for_handler.write().user = Some(data.user);
                    auth_for_handler.write().is_loading = false;
                    *status_for_handler.write() = GoogleLoginStatus::Idle;
                    nav.push(Route::Dashboard {});
                }
                GoogleAuthPayload::Error { error } => {
                    *status_for_handler.write() = GoogleLoginStatus::Error(error);
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        if win
            .add_event_listener_with_callback("message", handler.as_ref().unchecked_ref())
            .is_err()
        {
            *status.write() =
                GoogleLoginStatus::Error("failed to register message listener".to_string());
            return;
        }
        handler.forget();
    });

    GoogleLoginHandle { status, start }
}

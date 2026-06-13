//! Google OAuth login hook (popup + postMessage flow).
//!
//! Opens `/api/v1/auth/google` in a centered popup. The mokosh-server
//! callback page closes the popup and posts the JWT response back here
//! via `window.postMessage`. We accept the message only if its origin
//! equals our own (the SPA and API share an origin in dev via the
//! Dioxus `[[web.proxy]]` and in prod via an outer reverse proxy).

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use futures_util::StreamExt;
use serde::Deserialize;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::MessageEvent;

use crate::modules::auth::CurrentUser;
use crate::Route;

/// Shared slot holding the `message` listener closure so the handler can
/// deregister itself after consuming one valid message.
type MessageHandlerSlot = Rc<RefCell<Option<Closure<dyn FnMut(MessageEvent)>>>>;

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

/// Internal message dispatched from the JS `message` handler into the
/// coroutine that runs inside the Dioxus runtime (where signal writes
/// are legal). Necessary because JS event handlers fire outside of any
/// Dioxus render/effect/callback context.
enum GoogleAuthResult {
    Success(CurrentUser),
    Error(String),
}

/// Hook: returns a [`GoogleLoginHandle`] whose `start` callback opens
/// the OAuth popup and listens for the postMessage response.
pub fn use_google_login() -> GoogleLoginHandle {
    let mut status = use_signal(|| GoogleLoginStatus::Idle);
    let mut auth = super::auth::use_auth();
    let navigator = use_navigator();

    // The JS `message` handler runs outside any Dioxus render context, so
    // it cannot write signals directly (panics with "RefCell already
    // borrowed" / Option::unwrap on missing runtime). It posts results
    // into this coroutine instead, which IS scheduled by Dioxus and so
    // can mutate signals safely.
    let result_tx = use_coroutine(
        move |mut rx: UnboundedReceiver<GoogleAuthResult>| async move {
            while let Some(result) = rx.next().await {
                match result {
                    GoogleAuthResult::Success(user) => {
                        // Batch both AuthContext mutations into one write so
                        // the render between them does not observe a torn
                        // state (user set while `is_loading` still true).
                        {
                            let mut a = auth.write();
                            a.user = Some(user);
                            a.is_loading = false;
                        }
                        *status.write() = GoogleLoginStatus::Idle;
                        navigator.push(Route::Dashboard {});
                    }
                    GoogleAuthResult::Error(message) => {
                        *status.write() = GoogleLoginStatus::Error(message);
                    }
                }
            }
        },
    );

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
                *status.write() =
                    GoogleLoginStatus::Error("could not read window.location.origin".to_string());
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

        let expected_origin = origin.clone();
        let tx = result_tx;
        // Hold the closure in a shared slot so the handler can deregister
        // itself once it has consumed a valid message. Without this, every
        // sign-in click registers another permanent listener (the old
        // `handler.forget()`), so a later message fans out to every prior
        // closure and `tx.send` fires n times.
        let win_for_removal = win.clone();
        let handler_slot: MessageHandlerSlot = Rc::new(RefCell::new(None));
        let handler_slot_inner = handler_slot.clone();
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
            // Valid message for this flow: deregister this listener so a
            // subsequent sign-in does not accumulate duplicate sends.
            if let Some(cb) = handler_slot_inner.borrow().as_ref() {
                let _ = win_for_removal
                    .remove_event_listener_with_callback("message", cb.as_ref().unchecked_ref());
            }
            match msg.payload {
                GoogleAuthPayload::Success { data } => {
                    tx.send(GoogleAuthResult::Success(data.user));
                }
                GoogleAuthPayload::Error { error } => {
                    tx.send(GoogleAuthResult::Error(error));
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
        // Stash the closure so it outlives this synchronous registration
        // and remains reachable for self-removal in the handler above.
        *handler_slot.borrow_mut() = Some(handler);
    });

    GoogleLoginHandle { status, start }
}

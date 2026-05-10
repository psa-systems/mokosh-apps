//! `/signup/:token` - the page the new-user lands on after clicking
//! the email link from POST /v1/auth/signup.
//!
//! Three states: Loading -> Ready (password form) -> Done (redirect
//! to /login). Error states collapse to a single "link not available"
//! message regardless of cause (unknown / used / expired) to match
//! the server's enumeration-resistant 404 contract.
//!
//! Phase 2 of docs/mokosh-auth/10-memberships-and-self-signup.md.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SignupPreview {
    email: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CompleteOk {
    email: String,
    redirect_to: String,
}

#[derive(Clone, Debug, PartialEq)]
enum CompleteState {
    Loading,
    Ready(SignupPreview),
    Submitting(SignupPreview),
    Done(String),
    Error(String),
}

fn invalid_message() -> String {
    "This sign-up link is invalid, expired, or has already been used.".to_string()
}

fn is_404(raw: &str) -> bool {
    raw.contains("status: 404") || raw.contains("signup_not_found") || raw.contains("HTTP 404")
}

fn parse_field_error(raw: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.get("error")?.as_str()? != "invalid_request" {
        return None;
    }
    let details = v.get("details")?.as_object()?;
    let (k, value) = details.iter().next()?;
    Some((k.clone(), value.as_str()?.to_string()))
}

#[component]
pub fn SignupCompletePage(token: String) -> Element {
    let mut state = use_signal(|| CompleteState::Loading);
    let mut password = use_signal(String::new);
    let mut password2 = use_signal(String::new);
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut field_error: Signal<Option<(String, String)>> = use_signal(|| None);

    // Initial preview load. Public endpoint, cross-origin, no Bearer.
    let token_load = token.clone();
    use_future(move || {
        let token = token_load.clone();
        async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/signup/by-token/{token}");
            match crate::modules::oidc::issuer_get::<SignupPreview>(&cfg, &path).await {
                Ok(preview) => state.set(CompleteState::Ready(preview)),
                Err(e) => {
                    let msg = e.to_string();
                    if is_404(&msg) {
                        state.set(CompleteState::Error(invalid_message()));
                    } else {
                        state.set(CompleteState::Error(format!(
                            "Could not load sign-up: {msg}"
                        )));
                    }
                }
            }
        }
    });

    let token_submit = token.clone();
    let mut submit = move || {
        let pw = password.read().clone();
        let pw2 = password2.read().clone();
        if pw != pw2 {
            field_error.set(Some(("password2".into(), "Passwords do not match".into())));
            return;
        }
        field_error.set(None);

        let preview = match state.read().clone() {
            CompleteState::Ready(p) => p,
            _ => return,
        };
        state.set(CompleteState::Submitting(preview));

        let token = token_submit.clone();
        let first = first_name.read().trim().to_string();
        let last = last_name.read().trim().to_string();
        spawn(async move {
            let body = serde_json::json!({
                "password": pw,
                "first_name": if first.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(first) },
                "last_name":  if last.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(last) },
            });
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/signup/by-token/{token}/complete");
            match crate::modules::oidc::issuer_post::<CompleteOk, _>(&cfg, &path, &body).await {
                Ok(ok) => {
                    state.set(CompleteState::Done(ok.email));
                    #[cfg(feature = "web")]
                    {
                        use gloo_timers::future::TimeoutFuture;
                        TimeoutFuture::new(1500).await;
                    }
                    let target = if ok.redirect_to.starts_with('/') {
                        ok.redirect_to
                    } else {
                        "/login".to_string()
                    };
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().set_href(&target);
                    }
                }
                Err(e) => {
                    let raw = e.to_string();
                    if let Some((field, msg)) = parse_field_error(&raw) {
                        field_error.set(Some((field, msg)));
                        let snapshot = state.read().clone();
                        if let CompleteState::Submitting(p) = snapshot {
                            state.set(CompleteState::Ready(p));
                        }
                    } else if is_404(&raw) {
                        state.set(CompleteState::Error(invalid_message()));
                    } else {
                        state.set(CompleteState::Error(format!(
                            "Could not finish sign-up: {raw}"
                        )));
                    }
                }
            }
        });
    };

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                match state.read().clone() {
                    CompleteState::Loading => rsx! {
                        div { class: "text-center py-12",
                            p { class: "text-gray-600 dark:text-gray-300", "Loading..." }
                        }
                    },
                    CompleteState::Error(msg) => rsx! {
                        div { class: "space-y-4",
                            h2 { class: "text-xl font-semibold text-gray-900 dark:text-white text-center", "Link not available" }
                            p { class: "text-sm text-gray-600 dark:text-gray-300 text-center", "{msg}" }
                            div { class: "text-center",
                                a { href: "/login",
                                    class: "text-sm text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                    "Go to sign in"
                                }
                            }
                        }
                    },
                    CompleteState::Done(email) => rsx! {
                        div { class: "space-y-3 text-center",
                            h2 { class: "text-xl font-semibold text-gray-900 dark:text-white", "Account created" }
                            p { class: "text-sm text-gray-600 dark:text-gray-300", "Welcome, {email}." }
                            p { class: "text-xs text-gray-500", "Redirecting you to sign in..." }
                        }
                    },
                    CompleteState::Ready(preview) | CompleteState::Submitting(preview) => {
                        let busy = matches!(*state.read(), CompleteState::Submitting(_));
                        let pwd_err = field_error.read().as_ref()
                            .filter(|(f, _)| f == "password")
                            .map(|(_, m)| m.clone())
                            .unwrap_or_default();
                        let pwd2_err = field_error.read().as_ref()
                            .filter(|(f, _)| f == "password2")
                            .map(|(_, m)| m.clone())
                            .unwrap_or_default();
                        rsx! {
                            div { class: "space-y-4",
                                h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                                    "Finish creating your account"
                                }
                                div { class: "rounded-md bg-gray-50 dark:bg-gray-800 p-4",
                                    p { class: "text-sm",
                                        span { class: "font-medium", "Email: " }
                                        "{preview.email}"
                                    }
                                }

                                form {
                                    class: "space-y-4",
                                    onsubmit: move |e| { e.prevent_default(); submit(); },

                                    Input {
                                        name: "first_name".to_string(),
                                        label: "First name".to_string(),
                                        r#type: "text".to_string(),
                                        placeholder: "Optional".to_string(),
                                        value: first_name.read().clone(),
                                        oninput: move |e: FormEvent| first_name.set(e.value()),
                                    }

                                    Input {
                                        name: "last_name".to_string(),
                                        label: "Last name".to_string(),
                                        r#type: "text".to_string(),
                                        placeholder: "Optional".to_string(),
                                        value: last_name.read().clone(),
                                        oninput: move |e: FormEvent| last_name.set(e.value()),
                                    }

                                    Input {
                                        name: "password".to_string(),
                                        label: "Password".to_string(),
                                        r#type: "password".to_string(),
                                        required: true,
                                        value: password.read().clone(),
                                        oninput: move |e: FormEvent| password.set(e.value()),
                                        help: "At least 12 characters with upper, lower, digit, and symbol.".to_string(),
                                        error: pwd_err,
                                    }

                                    Input {
                                        name: "password2".to_string(),
                                        label: "Confirm password".to_string(),
                                        r#type: "password".to_string(),
                                        required: true,
                                        value: password2.read().clone(),
                                        oninput: move |e: FormEvent| password2.set(e.value()),
                                        error: pwd2_err,
                                    }

                                    Button {
                                        r#type: "submit".to_string(),
                                        variant: ButtonVariant::Primary,
                                        class: "w-full".to_string(),
                                        loading: busy,
                                        "Create account"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

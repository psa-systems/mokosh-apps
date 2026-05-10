//! `/signup` - public self-signup entry point.
//!
//! User types email; SPA POSTs to /v1/auth/signup; SPA shows a "check
//! your email" card. The link in the email lands on /signup/<token>
//! which is handled by `SignupCompletePage`.
//!
//! The server returns 200 whether the email was new or already in
//! use; we render the same "check your email" message either way so
//! a malicious caller cannot enumerate accounts via this page.
//!
//! Phase 2 of docs/mokosh-auth/10-memberships-and-self-signup.md.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Clone, Debug, PartialEq)]
enum State {
    Form,
    Submitting,
    Sent(String), // email shown back to the user
    Disabled,     // server returned signup_disabled
}

#[derive(Clone, Debug, Deserialize)]
struct StartOk {
    #[serde(default)]
    #[allow(dead_code)] // we accept any 200 body shape; field reserved.
    status: Option<String>,
}

#[component]
pub fn SignupPage() -> Element {
    let mut email = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut state = use_signal(|| State::Form);

    let mut submit = move || {
        let value = email.read().trim().to_string();
        if value.is_empty() {
            error.set(Some("Email is required".into()));
            return;
        }
        error.set(None);
        state.set(State::Submitting);
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let body = serde_json::json!({ "email": value.clone() });
            match crate::modules::oidc::issuer_post::<StartOk, _>(
                &cfg,
                "/v1/auth/signup",
                &body,
            )
            .await
            {
                Ok(_) => state.set(State::Sent(value)),
                Err(e) => {
                    let raw = e.to_string();
                    if raw.contains("signup_disabled") {
                        state.set(State::Disabled);
                    } else if raw.contains("invalid email") || raw.contains("invalid_request") {
                        error.set(Some("That email doesn't look right.".into()));
                        state.set(State::Form);
                    } else if raw.contains("HTTP 429") || raw.contains("rate") {
                        error.set(Some(
                            "Too many sign-up attempts. Please wait a moment and try again."
                                .into(),
                        ));
                        state.set(State::Form);
                    } else {
                        error.set(Some(format!("Could not start sign up: {raw}")));
                        state.set(State::Form);
                    }
                }
            }
        });
    };

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                match state.read().clone() {
                    State::Disabled => rsx! {
                        div { class: "text-center space-y-3",
                            h2 { class: "text-2xl font-bold text-gray-900 dark:text-white",
                                "Sign up unavailable"
                            }
                            p { class: "text-sm text-gray-600 dark:text-gray-300",
                                "This deployment of Mokosh does not allow public sign-up. "
                                "An admin must invite you instead."
                            }
                            Link {
                                to: Route::Login {},
                                class: "text-blue-600 hover:text-blue-500 dark:text-blue-400 text-sm",
                                "Back to sign in"
                            }
                        }
                    },
                    State::Sent(addr) => rsx! {
                        div { class: "space-y-3 text-center",
                            h2 { class: "text-2xl font-bold text-gray-900 dark:text-white",
                                "Check your email"
                            }
                            p { class: "text-sm text-gray-600 dark:text-gray-300",
                                "If "
                                span { class: "font-medium", "{addr}" }
                                " can be signed up, we just sent a link there. "
                                "Click it within 24 hours to set your password and finish "
                                "creating your account."
                            }
                            p { class: "text-xs text-gray-500",
                                "If you do not get an email, your address may already be "
                                "associated with an existing account. Try signing in instead."
                            }
                            Link {
                                to: Route::Login {},
                                class: "text-blue-600 hover:text-blue-500 dark:text-blue-400 text-sm",
                                "Back to sign in"
                            }
                        }
                    },
                    State::Form | State::Submitting => {
                        let busy = matches!(*state.read(), State::Submitting);
                        rsx! {
                            div {
                                h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                                    "Create your account"
                                }
                                p { class: "mt-2 text-sm text-gray-600 dark:text-gray-300 text-center",
                                    "We'll send a one-time link to your email. Click it to "
                                    "set a password and finish."
                                }
                            }
                            if let Some(msg) = error.read().as_ref() {
                                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                                    p { class: "text-sm text-red-600 dark:text-red-400", "{msg}" }
                                }
                            }
                            form {
                                class: "space-y-4",
                                onsubmit: move |e| { e.prevent_default(); submit(); },

                                Input {
                                    name: "email".to_string(),
                                    label: "Email address".to_string(),
                                    r#type: "email".to_string(),
                                    placeholder: "you@example.com".to_string(),
                                    required: true,
                                    value: email.read().clone(),
                                    oninput: move |e: FormEvent| email.set(e.value()),
                                }

                                Button {
                                    r#type: "submit".to_string(),
                                    variant: ButtonVariant::Primary,
                                    class: "w-full".to_string(),
                                    loading: busy,
                                    "Continue"
                                }
                            }
                            div { class: "text-center text-sm text-gray-600 dark:text-gray-400",
                                "Already have an account? "
                                Link {
                                    to: Route::Login {},
                                    class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                    "Sign in"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

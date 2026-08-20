//! MAPPS-510: standalone password-reset page.
//!
//! Renders locally when the SPA is running in standalone mode (no
//! OIDC issuer configured, i.e. self-hosted deploy with no bunyip
//! hub). Redeems the `{user_id}.{secret}` token emailed to the user
//! by mokosh-server via `POST /api/v1/auth/reset-password`.
//!
//! Bunyip-configured deploys (staging, prod) never mount this page:
//! `lib.rs::ResetPassword` still HubRedirects there when
//! `is_standalone()` is false.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct ResetBody {
    token: String,
    new_password: String,
    confirm_password: String,
}

#[component]
pub fn StandaloneResetPassword(token: String) -> Element {
    let nav = use_navigator();
    let mut new_pw = use_signal(String::new);
    let mut confirm_pw = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);
    let mut saving = use_signal(|| false);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let pw = new_pw.read().clone();
        let confirm = confirm_pw.read().clone();
        if pw.len() < 12 {
            error.set("Password must be at least 12 characters.".to_string());
            return;
        }
        if pw != confirm {
            error.set("Passwords do not match.".to_string());
            return;
        }
        let tok = token.clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = ResetBody {
                    token: tok,
                    new_password: pw,
                    confirm_password: confirm,
                };
                match crate::hooks::fetch::api::post_typed_no_content("/auth/reset-password", &body)
                    .await
                {
                    Ok(()) => {
                        done.set(true);
                    }
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) => {
                        // Bad/expired token or validation failure.
                        error.set(if message.is_empty() {
                            "Reset link is invalid or has expired.".to_string()
                        } else {
                            message
                        });
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Password updated" }
                    p { class: "mt-2 text-sm text-content",
                        "Your password has been reset. Sign in with the new password."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| { nav.replace(Route::Login {}); },
                        "Go to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Set a new password" }
                    p { class: "mt-2 text-sm text-content",
                        "Choose a password you don't use anywhere else."
                    }
                }
                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        submit(());
                    },
                    Input {
                        name: "new_password",
                        label: "New password",
                        r#type: "password".to_string(),
                        value: new_pw(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            new_pw.set(e.value());
                        },
                    }
                    Input {
                        name: "confirm_password",
                        label: "Confirm password",
                        r#type: "password".to_string(),
                        value: confirm_pw(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            confirm_pw.set(e.value());
                        },
                    }
                    if !error().is_empty() {
                        p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                    div { class: "pt-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: saving(),
                            loading: saving(),
                            r#type: "submit".to_string(),
                            class: "w-full".to_string(),
                            "Set password"
                        }
                    }
                }
            }
        }
    }
}

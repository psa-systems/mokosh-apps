//! MAPPS-510: standalone forgot-password page.
//!
//! Renders locally when the SPA is running in standalone mode (no
//! OIDC issuer configured). Submits to
//! `POST /api/v1/auth/forgot-password` which emails the reset link.
//!
//! Always shows a generic success message regardless of whether the
//! email is registered - matches the server's fail-quiet shape (do
//! not leak account existence to unauthenticated callers).

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct ForgotBody {
    email: String,
}

#[component]
pub fn StandaloneForgotPassword() -> Element {
    let nav = use_navigator();
    let mut email = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut done = use_signal(|| false);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        if em.is_empty() {
            error.set("Enter your email.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = ForgotBody { email: em };
                // Ignore the result: the endpoint is fail-quiet by
                // design (200 for both registered + unregistered
                // emails). Even a 500 shouldn't leak; treat it as
                // sent-anyway.
                let _ =
                    crate::hooks::fetch::api::post_typed_no_content("/auth/forgot-password", &body)
                        .await;
                done.set(true);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Check your email" }
                    p { class: "mt-2 text-sm text-content",
                        "If that address is registered, we've sent you a link to reset your password."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| { nav.replace(Route::Login {}); },
                        "Back to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Reset your password" }
                    p { class: "mt-2 text-sm text-content",
                        "Enter the email you sign in with. We'll email you a link to set a new password."
                    }
                }
                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        submit(());
                    },
                    Input {
                        name: "email",
                        label: "Email",
                        r#type: "email".to_string(),
                        value: email(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            email.set(e.value());
                        },
                    }
                    if !error().is_empty() {
                        p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                    div { class: "pt-2 space-y-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: saving(),
                            loading: saving(),
                            r#type: "submit".to_string(),
                            class: "w-full".to_string(),
                            "Send reset link"
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: saving(),
                            r#type: "button".to_string(),
                            class: "w-full".to_string(),
                            onclick: move |_| { nav.replace(Route::Login {}); },
                            "Back to sign in"
                        }
                    }
                }
            }
        }
    }
}

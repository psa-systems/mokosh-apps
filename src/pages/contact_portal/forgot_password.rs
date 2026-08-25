//! mokosh-contact-login prompt 005: contact-plane forgot-password.
//!
//! Mounted at `/portal/{slug}/forgot-password`. Public. Always renders
//! the same "if that email is on record..." message on submit,
//! regardless of the server response - the endpoint is fail-quiet by
//! design (204 for both hit and miss) so the SPA does not leak
//! account existence either.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct ForgotBody {
    slug: String,
    email: String,
}

#[component]
pub fn ContactForgotPasswordPage(slug: String) -> Element {
    let nav = use_navigator();
    let mut email = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut done = use_signal(|| false);

    let slug_for_submit = slug.clone();
    let mut submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        if em.is_empty() {
            error.set("Enter your email.".to_string());
            return;
        }
        let slug = slug_for_submit.clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = ForgotBody { slug, email: em };
                // Fail-quiet: even a 500 is treated as sent-anyway so
                // the response shape does not leak account existence.
                let _ = crate::hooks::fetch::api::post_typed_no_content(
                    "/contact/auth/forgot-password",
                    &body,
                )
                .await;
                done.set(true);
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (slug, em);
                done.set(true);
            }
            saving.set(false);
        });
    };

    let slug_for_done = slug.clone();
    let slug_for_back = slug.clone();
    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Check your email" }
                    p { class: "mt-2 text-sm text-content",
                        "If that email is on record, we've sent a reset link."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| {
                            nav.replace(Route::ContactLogin { slug: slug_for_done.clone() });
                        },
                        "Back to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Reset your password" }
                    p { class: "mt-2 text-sm text-content",
                        "Enter the email you sign in with and we'll send a reset link."
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
                            onclick: move |_| {
                                nav.replace(Route::ContactLogin { slug: slug_for_back.clone() });
                            },
                            "Back to sign in"
                        }
                    }
                }
            }
        }
    }
}

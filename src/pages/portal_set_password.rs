//! MAPPS-396: `/portal/set-password`, the page the portal setup email links to.
//!
//! When an agent grants a contact portal access, mokosh-server mails
//! `{app_url}/portal/set-password?token={token}`
//! (`src/modules/contacts/service.rs`) and redeems the token at
//! `POST /api/v1/portal/auth/setup-password` (`src/modules/portal/routes.rs`).
//! The token itself is the credential, so the page sends no bearer.
//!
//! Server status contract (`PortalAuthService::setup_password`): 204 on
//! success, 410 for an already-redeemed link, 400 for an expired or unknown
//! one. Each maps to its own copy below so a customer can tell "you already
//! did this" from "ask for a new link".

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{Button, ButtonVariant, Input};
use crate::Route;

/// Client-side floor, mirroring the server's `length(min = 8)` on
/// `PortalSetupPasswordRequest.password`.
const MIN_PASSWORD_LEN: usize = 8;

/// Request body for `POST /api/v1/portal/auth/setup-password`, matching
/// mokosh-server's `PortalSetupPasswordRequest`.
#[derive(Serialize)]
struct SetupPasswordBody {
    token: String,
    password: String,
}

#[component]
pub fn PortalSetPasswordPage() -> Element {
    // The emailed token is `{contact_id}.{64 alphanumerics}`, so the raw
    // (undecoded) query value is already what the server expects.
    let token = use_signal(|| crate::utils::url::current_query_param("token").unwrap_or_default());

    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let tok = token.read().clone();
        if tok.is_empty() {
            error.set(
                "This link is expired or invalid. Ask your account team for a new one.".to_string(),
            );
            return;
        }
        let pw = password.read().clone();
        if pw.chars().count() < MIN_PASSWORD_LEN {
            error.set(format!(
                "Password must be at least {MIN_PASSWORD_LEN} characters."
            ));
            return;
        }
        if pw != *confirm.read() {
            error.set("The two passwords do not match.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = SetupPasswordBody {
                    token: tok.clone(),
                    password: pw.clone(),
                };
                match crate::hooks::fetch::api::post_typed_no_content(
                    "/portal/auth/setup-password",
                    &body,
                )
                .await
                {
                    Ok(()) => done.set(true),
                    // Single-use token already redeemed. The password is set,
                    // so point at signing in rather than at a new link.
                    Err(ApiError::Status { code: 410, .. }) => {
                        error.set(
                            "This link was already used. Your password is set, so sign in to the Client Portal instead."
                                .to_string(),
                        );
                    }
                    // Expired, unknown, or a rejected password. The server
                    // returns 400 for all three and its message names the
                    // password rule, so prefer the server text when it has one.
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(ApiError::Status { code: 400, .. }) => {
                        error.set(
                            "This link is expired or invalid. Ask your account team for a new one."
                                .to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (tok, pw);
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    if done() {
                        div { class: "text-center", role: "status", aria_live: "polite",
                            h1 { class: "text-2xl font-semibold text-content", "Password set" }
                            p { class: "mt-2 text-sm text-content",
                                "You can now sign in to the Client Portal with your email and this password."
                            }
                            // MAPPS-395: the portal sign-in page, which mints
                            // the `typ: "portal_access"` token every portal
                            // page needs.
                            Link {
                                to: Route::PortalLogin {},
                                class: "mt-6 inline-flex items-center justify-center rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent hover:opacity-90",
                                "Sign in to the Client Portal"
                            }
                        }
                    } else {
                        div { class: "text-center mb-6",
                            h1 { class: "text-2xl font-semibold text-content", "Set your portal password" }
                            p { class: "mt-2 text-sm text-content",
                                "Choose a password for the Client Portal. It must be at least {MIN_PASSWORD_LEN} characters."
                            }
                        }

                        form {
                            class: "space-y-4",
                            onsubmit: move |evt: Event<FormData>| {
                                evt.prevent_default();
                                handle_submit(());
                            },

                            Input {
                                name: "password",
                                label: "New password",
                                r#type: "password".to_string(),
                                value: password(),
                                required: true,
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    password.set(e.value());
                                },
                            }

                            Input {
                                name: "confirm_password",
                                label: "Confirm password",
                                r#type: "password".to_string(),
                                value: confirm(),
                                required: true,
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    confirm.set(e.value());
                                },
                            }

                            if !error().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
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
    }
}

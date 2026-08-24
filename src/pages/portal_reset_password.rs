//! PMS-729 phase 2 H3: `/portal/reset-password`, the destination of
//! the forgot-password email.
//!
//! Server status contract (`PortalAuthService::reset_password`): 204 on
//! success, 410 for an already-redeemed link, 400 for an expired /
//! unknown / weak-password one. The message text on 400 comes from the
//! shared `utils::password_policy` module so the copy names the actual
//! reason (length floor, blocklist, low strength).

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{Button, ButtonVariant, Input};
use crate::Route;

/// Client-side floor, mirroring the server's shared
/// `utils::password_policy` module (PMS-729 phase 2 H5). Server
/// enforces this + zxcvbn strength + a common-password blocklist; the
/// hint here is only so the user does not learn the length rule from
/// a round-trip.
const MIN_PASSWORD_LEN: usize = 12;

/// Request body for `POST /api/v1/portal/auth/reset-password`, matching
/// mokosh-server's `PortalResetPasswordRequest`.
#[derive(Serialize)]
struct ResetPasswordBody {
    token: String,
    password: String,
}

#[component]
pub fn PortalResetPasswordPage(token: String) -> Element {
    // MAPPS-560: `token` comes from the route's `?:token` query segment
    // (see `Route::PortalResetPassword { token }` in `lib.rs`). Same
    // shape as `PortalSetPasswordPage`; pre-560 both read
    // `window.location.search` at mount, which the Dioxus router
    // could strip on URL normalization and leave empty.
    let token = use_signal(|| token);

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
                "This link is expired or invalid. Request a new one from the sign-in page."
                    .to_string(),
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
                let body = ResetPasswordBody {
                    token: tok.clone(),
                    password: pw.clone(),
                };
                match crate::hooks::fetch::api::post_typed_no_content(
                    "/portal/auth/reset-password",
                    &body,
                )
                .await
                {
                    Ok(()) => done.set(true),
                    // Single-use token already redeemed. Send the user
                    // to sign in with whatever password they set last.
                    Err(ApiError::Status { code: 410, .. }) => {
                        error.set(
                            "This link was already used. Try signing in, or request another reset link."
                                .to_string(),
                        );
                    }
                    // 400 covers expired, unknown, AND weak-password
                    // rejections. Prefer the server-supplied message so
                    // the customer knows which one applies.
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(ApiError::Status { code: 400, .. }) => {
                        error.set(
                            "This link is expired or invalid. Request a new one from the sign-in page."
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
                            h1 { class: "text-2xl font-semibold text-content", "Password reset" }
                            p { class: "mt-2 text-sm text-content",
                                "Your password has been updated. Sign in with your email and the new password."
                            }
                            Link {
                                to: Route::PortalLogin {},
                                class: "mt-6 inline-flex items-center justify-center rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent hover:opacity-90",
                                "Sign in to the Client Portal"
                            }
                        }
                    } else {
                        div { class: "text-center mb-6",
                            h1 { class: "text-2xl font-semibold text-content", "Set a new portal password" }
                            p { class: "mt-2 text-sm text-content",
                                "Choose a password for the Client Portal. It must be at least {MIN_PASSWORD_LEN} characters and not a common leaked password."
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
                                    "Set new password"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

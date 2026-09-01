//! PMS-832: `/portal/reset-password`, the page the portal reset email links to.
//!
//! PMS-820 gave the portal its own credential lifecycle. `PortalAuthService::send_reset_email`
//! (mokosh-server `src/modules/portal/service.rs`) mails
//! `{SPA_BASE_URL}/portal/reset-password?token={contact_id}.{secret}` and redeems it at
//! `POST /api/v1/portal/auth/reset-password`. The token is the only credential
//! the visitor has, so the page sends no bearer and sits outside `PortalGuard`.
//!
//! Deliberately a sibling of [`crate::pages::PortalSetPasswordPage`] rather than
//! a reuse of it. Both redeem the same `{contact_id}.{secret}` token with the
//! same body and get the same statuses, so the mechanics are identical, but the
//! setup page's copy tells the customer an account "has been created for you",
//! which is wrong for someone who already had one and forgot the password.
//! Keeping them separate is also what lets the two links diverge later.
//!
//! Do NOT point this at `/api/v1/auth/reset-password`. That endpoint resolves
//! the token against `users`, and a portal customer resetting a staff login is
//! the exact defect PMS-820 fixed.
//!
//! Server status contract (`PortalAuthService::reset_password`): 204 on success,
//! 410 for an already-redeemed link, 400 for an expired or unknown one, 429 when
//! the rate limiter (10/min per IP, 3/min per contact) has had enough.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// Client-side floor, mirroring the server's `length(min = 8)` on
/// `PortalResetPasswordRequest.password`.
const MIN_PASSWORD_LEN: usize = 8;

/// Request body for `POST /api/v1/portal/auth/reset-password`, matching
/// mokosh-server's `PortalResetPasswordRequest` (a type alias of
/// `PortalSetupPasswordRequest`, which is why the shape is the setup one).
#[derive(Serialize)]
struct ResetPasswordBody {
    token: String,
    password: String,
}

/// Copy for a 429, with the server's own wait when its body carried one.
///
/// A 429 from `rate_limited_response` is not the canonical error envelope, so
/// `ApiError::Status.message` is the raw JSON body; rendering it would show a
/// customer a line of JSON. This writes the sentence and borrows only the
/// number.
fn too_many_attempts_message(server_body: &str) -> String {
    match crate::utils::rate_limit::retry_after_phrase(server_body) {
        Some(phrase) => format!("Too many attempts. Please try again {phrase}."),
        None => "Too many attempts. Please wait a moment and try again.".to_string(),
    }
}

#[component]
pub fn PortalResetPasswordPage() -> Element {
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
                "This link is expired or invalid. You can request a new one below.".to_string(),
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
            #[cfg(feature = "app")]
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
                    // 204. Only this renders as success; every branch below
                    // leaves `done` false, so a non-2xx can never show the
                    // "password updated" panel.
                    Ok(()) => done.set(true),
                    // 410: single-use token already redeemed. The password on
                    // the account is whatever that redemption set, so point at
                    // signing in rather than at retrying this link.
                    Err(ApiError::Status { code: 410, .. }) => {
                        error.set(
                            "This link was already used. If you did not reset your password, request a new link below."
                                .to_string(),
                        );
                    }
                    // 429: rate limited. Its body is not the canonical
                    // envelope, so the copy is ours and only the wait is the
                    // server's.
                    Err(ApiError::Status {
                        code: 429, message, ..
                    }) => {
                        error.set(too_many_attempts_message(&message));
                    }
                    // 400: expired, unknown, or a rejected password. The server
                    // returns 400 for all three and its message names the
                    // password rule, so prefer the server text when it has one.
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(ApiError::Status { code: 400, .. }) => {
                        error.set(
                            "This link is expired or invalid. You can request a new one below."
                                .to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (tok, pw);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center", role: "status", aria_live: "polite",
                    h1 { class: "text-2xl font-semibold text-content", "Password updated" }
                    p { class: "mt-2 text-sm text-content",
                        "You can now sign in to the Client Portal with your email and this password."
                    }
                    Link {
                        to: Route::PortalLogin {},
                        class: "mt-6 inline-flex items-center justify-center rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent hover:opacity-90",
                        "Sign in to the Client Portal"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Choose a new password" }
                    p { class: "mt-2 text-sm text-content",
                        "Set a new password for the Client Portal. It must be at least {MIN_PASSWORD_LEN} characters."
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
                            "Update password"
                        }
                    }
                }

                // A link rather than prose: every failure branch above tells
                // the customer they can request a new link, and copy that names
                // a control the reader then has to go and find is how the
                // support ticket comes back.
                p { class: "mt-6 text-center text-sm text-muted",
                    Link { to: Route::PortalForgotPassword {}, class: "underline hover:no-underline",
                        "Request a new reset link"
                    }
                    span { class: "mx-2 text-subtle", "\u{b7}" }
                    Link { to: Route::PortalLogin {}, class: "underline hover:no-underline",
                        "Back to sign in"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_matches_the_server_request_shape() {
        let json = serde_json::to_string(&ResetPasswordBody {
            token: "11111111-1111-1111-1111-111111111111.abc".to_string(),
            password: "hunter2hunter2".to_string(),
        })
        .expect("serializes");
        assert!(
            json.contains(r#""token":"11111111-1111-1111-1111-111111111111.abc""#),
            "{json}"
        );
        assert!(json.contains(r#""password":"hunter2hunter2""#), "{json}");
        // The server's `PortalResetPasswordRequest` has exactly these two
        // fields; an extra key would be dropped, but a missing one is a 422.
        assert_eq!(json.matches(':').count(), 2, "{json}");
    }

    #[test]
    fn the_client_floor_matches_the_servers_rule() {
        // mokosh-server: `#[validate(length(min = 8, ...))]` on
        // `PortalSetupPasswordRequest.password`, which
        // `PortalResetPasswordRequest` aliases. A client floor above the
        // server's would refuse passwords the server accepts; below it, the
        // customer learns the rule from a 400 instead of from the form.
        assert_eq!(MIN_PASSWORD_LEN, 8);
    }

    #[test]
    fn a_429_never_renders_the_servers_raw_json() {
        // What `handle_response` actually hands a 429 branch: the raw body,
        // because `rate_limited_response` does not use the canonical envelope.
        let raw = "{\"error\":\"rate_limited\",\"message\":\"Too many password reset attempts, please try again later\",\"retry_after_seconds\":45}";
        let shown = too_many_attempts_message(raw);
        assert!(
            !shown.contains('{') && !shown.contains("retry_after_seconds"),
            "a customer must never be shown the JSON body, got {shown:?}"
        );
        assert!(
            shown.contains("45 seconds"),
            "the one useful part of that body is the wait, got {shown:?}"
        );
    }

    #[test]
    fn a_429_without_a_usable_wait_still_reads_as_a_sentence() {
        let shown = too_many_attempts_message("<html>429</html>");
        assert_eq!(
            shown,
            "Too many attempts. Please wait a moment and try again."
        );
    }
}

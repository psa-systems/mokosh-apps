//! PMS-832 / MAPPS-538: `/portal/forgot-password`, where a portal customer asks
//! for a reset link.
//!
//! POSTs `{ tenant_slug, email }` to `POST /api/v1/portal/auth/forgot-password`
//! (mokosh-server `src/modules/portal/routes.rs`, PMS-820). `tenant_slug` is
//! part of the identity for the same reason it is on the sign-in form:
//! `contacts.email` is unique only within a tenant, so the portal resolves the
//! identity inside its own tenant rather than against the platform's `users`.
//!
//! **Non-enumeration is the whole contract of this page.** The endpoint answers
//! `200` with an empty body whether or not the address has portal access, so
//! this screen renders ONE fixed confirmation and never branches on the result.
//! Anything that differed - different copy, a different delay, a second request
//! made only in one case - would hand an attacker the address list the server
//! is refusing to give them.
//!
//! A separate route rather than an inline panel on the sign-in form, so the
//! reset page can link a customer straight back here when their link has
//! expired, instead of telling them to go and find a control.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// Request body for `POST /api/v1/portal/auth/forgot-password`, matching
/// mokosh-server's `PortalForgotPasswordRequest`.
#[derive(Serialize)]
struct ForgotPasswordBody {
    tenant_slug: String,
    email: String,
}

/// The one confirmation this page ever shows.
///
/// A `const` rather than a literal at the call site so the non-enumeration
/// property is checkable: there is exactly one string, so there is no "found"
/// wording for a test or a reviewer to find.
const CONFIRMATION: &str =
    "If that address has portal access, a reset link is on its way. Check your email.";

/// Copy for a 429, with the server's own wait when its body carried one.
///
/// The rate limiter keys on `(tenant_slug, email)` as submitted, not on whether
/// the address exists, so answering a 429 differently from a 200 leaks nothing:
/// an attacker gets the same throttle either way.
fn too_many_requests_message(server_body: &str) -> String {
    match crate::utils::rate_limit::retry_after_phrase(server_body) {
        Some(phrase) => format!("Too many reset requests. Please try again {phrase}."),
        None => "Too many reset requests. Please wait a moment and try again.".to_string(),
    }
}

#[component]
pub fn PortalForgotPasswordPage() -> Element {
    // Prefilled from `?tenant=`, which the sign-in page passes through so a
    // customer who already typed their account name does not type it twice.
    let mut tenant =
        use_signal(|| crate::utils::url::current_query_param("tenant").unwrap_or_default());
    let mut email = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut sent = use_signal(|| false);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let slug = tenant.read().trim().to_string();
        let em = email.read().trim().to_string();
        if slug.is_empty() || em.is_empty() {
            error.set("Enter your account name and email.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = ForgotPasswordBody {
                    tenant_slug: slug.clone(),
                    email: em.clone(),
                };
                match crate::hooks::fetch::api::post_typed_no_content(
                    "/portal/auth/forgot-password",
                    &body,
                )
                .await
                {
                    // 200 with an empty body, for a known and an unknown
                    // address alike. There is deliberately nothing to inspect
                    // and nothing to branch on.
                    Ok(()) => sent.set(true),
                    Err(ApiError::Status {
                        code: 429, message, ..
                    }) => {
                        error.set(too_many_requests_message(&message));
                    }
                    // 422: the address is not a valid email, or the account
                    // name is empty. About the form, not about the account,
                    // so the server's own message is the useful one.
                    Err(ApiError::Status {
                        code: 422, message, ..
                    }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (slug, em);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if sent() {
                div { class: "text-center", role: "status", aria_live: "polite",
                    h1 { class: "text-2xl font-semibold text-content", "Check your email" }
                    p { class: "mt-2 text-sm text-content", "{CONFIRMATION}" }
                    p { class: "mt-2 text-sm text-muted",
                        "The link is single use and expires in 24 hours."
                    }
                    Link {
                        to: Route::PortalLogin {},
                        class: "mt-6 inline-flex items-center justify-center rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent hover:opacity-90",
                        "Back to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Reset your portal password" }
                    p { class: "mt-2 text-sm text-content",
                        "Enter the account name and email you use for the Client Portal and we will send you a reset link."
                    }
                }

                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        handle_submit(());
                    },

                    Input {
                        name: "tenant_slug",
                        label: "Account name",
                        r#type: "text".to_string(),
                        value: tenant(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            tenant.set(e.value());
                        },
                    }

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
                        p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                    }

                    div { class: "pt-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: saving(),
                            loading: saving(),
                            r#type: "submit".to_string(),
                            class: "w-full".to_string(),
                            "Send reset link"
                        }
                    }
                }

                p { class: "mt-6 text-center text-sm text-muted",
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
        let json = serde_json::to_string(&ForgotPasswordBody {
            tenant_slug: "acme".to_string(),
            email: "contact@example.com".to_string(),
        })
        .expect("serializes");
        assert!(json.contains(r#""tenant_slug":"acme""#), "{json}");
        assert!(json.contains(r#""email":"contact@example.com""#), "{json}");
        // `contacts.email` is unique only within a tenant, so an omitted
        // `tenant_slug` is not a smaller request, it is an unanswerable one.
        assert_eq!(json.matches(':').count(), 2, "{json}");
    }

    #[test]
    fn the_confirmation_does_not_say_whether_the_address_was_found() {
        let copy = CONFIRMATION.to_lowercase();
        // Wording that would assert the address exists. The server answers 200
        // either way precisely so this screen cannot know, and copy that claims
        // otherwise turns the page into the enumeration oracle the endpoint is
        // built to avoid.
        for leak in [
            "we found",
            "your account",
            "no account",
            "not registered",
            "does not exist",
            "sent to your",
        ] {
            assert!(
                !copy.contains(leak),
                "the confirmation must not imply whether the address exists, but says {leak:?}"
            );
        }
        assert!(
            copy.starts_with("if that address"),
            "the confirmation is conditional by construction, got {CONFIRMATION:?}"
        );
    }

    #[test]
    fn a_429_never_renders_the_servers_raw_json() {
        let raw = "{\"error\":\"rate_limited\",\"message\":\"Too many password reset requests, please try again later\",\"retry_after_seconds\":30}";
        let shown = too_many_requests_message(raw);
        assert!(
            !shown.contains('{') && !shown.contains("retry_after_seconds"),
            "a customer must never be shown the JSON body, got {shown:?}"
        );
        assert!(shown.contains("30 seconds"), "{shown:?}");
    }
}

//! MAPPS-395: `/portal/login`, the client portal's sign-in page.
//!
//! Portal identity is a `contacts` row, not a `users` row, so a portal
//! visitor has no agent bearer and the agent bearer is useless here: every
//! `/api/v1/portal/*` route decodes the token and rejects anything whose
//! `typ` is not `portal_access` (mokosh-server `PortalAuthService::decode_token`).
//! The only issuer of such a token is `POST /api/v1/portal/auth/login`, which
//! this page posts to; the result lands in the portal-only holder in
//! `src/hooks/fetch.rs` that the `_portal_authed` fetch helpers read.
//!
//! `tenant_slug` is part of the credential (the same email can be a contact of
//! more than one MSP), so it is a form field, prefilled from `?tenant=` when
//! the customer arrives from a tenant-specific link.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::utils::url::urlencoding_minimal;
use crate::Route;

/// Request body for `POST /api/v1/portal/auth/login`, matching
/// mokosh-server's `PortalLoginRequest`.
#[derive(Serialize)]
struct PortalLoginBody {
    tenant_slug: String,
    email: String,
    password: String,
}

/// The field of mokosh-server's `PortalLoginResponse` this page consumes.
/// `expires_at` and `contact` are ignored: the token is memory-only and the
/// portal pages read their own data from the API.
#[derive(Deserialize)]
struct PortalLoginResp {
    access_token: String,
}

#[component]
pub fn PortalLoginPage() -> Element {
    let nav = use_navigator();

    let mut tenant =
        use_signal(|| crate::utils::url::current_query_param("tenant").unwrap_or_default());
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let slug = tenant.read().trim().to_string();
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if slug.is_empty() || em.is_empty() || pw.is_empty() {
            error.set("Enter your account name, email, and password.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "app")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = PortalLoginBody {
                    tenant_slug: slug.clone(),
                    email: em.clone(),
                    password: pw.clone(),
                };
                match crate::hooks::fetch::api::post_typed::<PortalLoginResp, _>(
                    "/portal/auth/login",
                    &body,
                )
                .await
                {
                    Ok(resp) => {
                        crate::hooks::fetch::api::set_portal_access_token(Some(resp.access_token));
                        nav.replace(Route::PortalHome {});
                    }
                    // The server answers 401 for a wrong password, an unknown
                    // contact, and an unknown tenant alike, so the copy cannot
                    // name which one was wrong.
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid account name, email, or password.".to_string());
                    }
                    // Rate limiter or the persistent failed-attempt lockout
                    // (mokosh-server PMS-501).
                    Err(ApiError::Status { code: 429, .. }) => {
                        error.set(
                            "Too many sign-in attempts. Please wait a moment and try again."
                                .to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (slug, em, pw);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "text-center mb-6",
                h1 { class: "text-2xl font-semibold text-content", "Sign in to the Client Portal" }
                p { class: "mt-2 text-sm text-content",
                    "Use the email your account team set up for you."
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

                Input {
                    name: "password",
                    label: "Password",
                    r#type: "password".to_string(),
                    value: password(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| {
                        error.set(String::new());
                        password.set(e.value());
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
                        "Sign in"
                    }
                }
            }

            // PMS-832: before this, a customer who forgot their portal password
            // had to ask the MSP to mint a new setup link - the support-ticket
            // workflow PMS-820 set out to remove. The account name they have
            // already typed rides along so they do not type it twice.
            p { class: "mt-6 text-center text-sm text-muted",
                Link {
                    to: format!("/portal/forgot-password?tenant={}", urlencoding_minimal(tenant.read().trim())),
                    class: "underline hover:no-underline",
                    "Forgot your password?"
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
        let json = serde_json::to_string(&PortalLoginBody {
            tenant_slug: "acme".to_string(),
            email: "contact@example.com".to_string(),
            password: "pw".to_string(),
        })
        .expect("serializes");
        assert!(json.contains(r#""tenant_slug":"acme""#), "{json}");
        assert!(json.contains(r#""email":"contact@example.com""#), "{json}");
        assert!(json.contains(r#""password":"pw""#), "{json}");
    }

    #[test]
    fn decodes_the_login_response() {
        let resp: PortalLoginResp = serde_json::from_str(
            r#"{"access_token":"portal.jwt","expires_at":"2026-08-01T00:00:00Z",
                "contact":{"id":"00000000-0000-0000-0000-000000000000"}}"#,
        )
        .expect("portal login response decodes");
        assert_eq!(resp.access_token, "portal.jwt");
    }
}

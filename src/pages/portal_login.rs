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

use crate::components::{Button, ButtonVariant, Input};
use crate::Route;

/// Request body for `POST /api/v1/portal/auth/login`, matching
/// mokosh-server's `PortalLoginRequest`.
///
/// PMS-729: `tenant_slug` is now optional server-side. When the SPA is
/// served on `{slug}.client.<apex>` the server resolves the slug from
/// Host and this field can be omitted; when the SPA is served on a
/// legacy host (`?tenant=` link) the field carries the slug the user
/// typed. `#[serde(skip_serializing_if = "String::is_empty")]` keeps the
/// wire small on the host-derived path.
#[derive(Serialize)]
struct PortalLoginBody {
    #[serde(skip_serializing_if = "String::is_empty")]
    tenant_slug: String,
    email: String,
    password: String,
    /// PMS-729 phase 2 H4: TOTP code entered after the server returned
    /// `mfa_required: true`. Dropped from the wire when empty so the
    /// server sees a clean shape on the first-attempt (no-code) path.
    #[serde(skip_serializing_if = "String::is_empty")]
    mfa_code: String,
}

/// The fields of mokosh-server's `PortalLoginResponse` this page consumes.
/// `expires_at`, `refresh_expires_at`, and `contact` are ignored here:
/// tokens are memory-only and the portal pages read their own data from
/// the API. `refresh_token` (PMS-729 phase 2 H2) is stashed alongside
/// the access token so the auto-refresh hook can rotate before expiry.
/// `mfa_required` (PMS-729 phase 2 H4) is `true` when the server needs a
/// second-factor code; the page then shows the MFA input and re-POSTs.
#[derive(Deserialize)]
struct PortalLoginResp {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    mfa_required: bool,
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
    // PMS-729 phase 2 H4: MFA branch. `mfa_required` flips to true
    // when the server said "need a second factor"; the UI then swaps
    // the credentials block for a code input and re-POSTs with
    // `mfa_code` populated.
    let mut mfa_required = use_signal(|| false);
    let mut mfa_code = use_signal(String::new);
    // PMS-729: kick the shared `/portal/host` branding fetch. The result
    // lives in `PORTAL_HOST_HINT` so `PortalLayout` can paint the same
    // MSP name + logo after login without re-fetching. Idempotent - the
    // helper latches to a one-shot flag so both this page and the layout
    // can call it during a session without duplicating the request.
    #[cfg(feature = "web")]
    use_hook(crate::hooks::portal_branding::ensure_portal_branding_fetch);

    // PMS-729: read the shared branding hint. The reactive read here
    // triggers a re-render when the fetch flips it from `None` to
    // `Some(_)`, so the slug input hides and the branding block appears
    // without the page having to poll.
    let hint_snapshot = crate::hooks::portal_branding::use_portal_host_hint();

    let host_derived_for_submit = hint_snapshot.is_some();
    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        // PMS-729: when the SPA is on a portal host, the server resolves
        // the slug from Host; the user does not need to type one and the
        // slug field is hidden. On a legacy host the slug is required.
        let host_derived = host_derived_for_submit;
        let slug = tenant.read().trim().to_string();
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        let code = mfa_code.read().trim().to_string();
        let is_mfa_stage = mfa_required();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your email and password.".to_string());
            return;
        }
        if !host_derived && slug.is_empty() {
            error.set("Enter your account name, email, and password.".to_string());
            return;
        }
        if is_mfa_stage && code.is_empty() {
            error.set("Enter the 6-digit code from your authenticator app.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                // PMS-729: send the slug only when the SPA is on a
                // legacy host (no host hint). On a portal host, leave
                // it empty so `skip_serializing_if` drops the field
                // entirely and the server derives from Host.
                let body = PortalLoginBody {
                    tenant_slug: if host_derived {
                        String::new()
                    } else {
                        slug.clone()
                    },
                    email: em.clone(),
                    password: pw.clone(),
                    mfa_code: code.clone(),
                };
                match crate::hooks::fetch::api::post_typed::<PortalLoginResp, _>(
                    "/portal/auth/login",
                    &body,
                )
                .await
                {
                    // PMS-729 phase 2 H4: MFA stage. Server needs a
                    // second-factor code; swap the UI to the code
                    // input and stop here. The SAME body (email +
                    // password + slug) is re-POSTed on the next
                    // submit; a token is only issued when both
                    // factors verify.
                    Ok(PortalLoginResp {
                        mfa_required: true, ..
                    }) => {
                        mfa_required.set(true);
                        mfa_code.set(String::new());
                    }
                    Ok(resp) => {
                        crate::hooks::fetch::api::set_portal_access_token(Some(resp.access_token));
                        crate::hooks::fetch::api::set_portal_refresh_token(Some(
                            resp.refresh_token,
                        ));
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
            #[cfg(not(feature = "web"))]
            {
                let _ = (slug, em, pw);
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    div { class: "text-center mb-6",
                        // PMS-729: MSP branding block above the credential
                        // fields. Rendered only when the /portal/host
                        // endpoint returned an active tenant; on legacy
                        // hosts, falls back to the generic title below.
                        if let Some(hint) = &hint_snapshot {
                            if let Some(url) = &hint.logo_url {
                                img {
                                    src: "{url}",
                                    alt: "{hint.name}",
                                    class: "h-14 w-auto mx-auto mb-3",
                                }
                            }
                            h1 { class: "text-2xl font-semibold text-content",
                                "Sign in to {hint.name}"
                            }
                        } else {
                            h1 { class: "text-2xl font-semibold text-content",
                                "Sign in to the Client Portal"
                            }
                        }
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

                        if mfa_required() {
                            // PMS-729 phase 2 H4: MFA input stage. The
                            // credentials are already verified server-
                            // side (this is the second POST); the user
                            // types the 6-digit code from their
                            // authenticator app and re-submits.
                            div {
                                p { class: "text-sm text-content mb-2",
                                    "Enter the 6-digit code from your authenticator app to finish signing in."
                                }
                                Input {
                                    name: "mfa_code",
                                    label: "Authenticator code",
                                    r#type: "text".to_string(),
                                    value: mfa_code(),
                                    required: true,
                                    disabled: saving(),
                                    oninput: move |e: FormEvent| {
                                        error.set(String::new());
                                        mfa_code.set(e.value());
                                    },
                                }
                            }
                        } else {
                            // PMS-729: the slug field only renders on legacy
                            // hosts. On {slug}.client.<apex>, the server
                            // resolves the tenant from Host and the input is
                            // hidden (the branding block above already tells
                            // the user which MSP they are signing in to).
                            if hint_snapshot.is_none() {
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
                                if mfa_required() { "Verify code" } else { "Sign in" }
                            }
                        }

                        // PMS-729 phase 2 H3: link to the forgot-password page.
                        div { class: "text-center pt-2",
                            Link {
                                to: Route::PortalForgotPassword {},
                                class: "text-sm text-accent hover:opacity-80",
                                "Forgot your password?"
                            }
                        }
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
        let json = serde_json::to_string(&PortalLoginBody {
            tenant_slug: "acme".to_string(),
            email: "contact@example.com".to_string(),
            password: "pw".to_string(),
            mfa_code: String::new(),
        })
        .expect("serializes");
        assert!(json.contains(r#""tenant_slug":"acme""#), "{json}");
        assert!(json.contains(r#""email":"contact@example.com""#), "{json}");
        assert!(json.contains(r#""password":"pw""#), "{json}");
        // Empty mfa_code is dropped from the wire so the first-attempt
        // POST looks identical to the pre-H4 shape.
        assert!(!json.contains("mfa_code"), "empty mfa_code dropped: {json}");
    }

    #[test]
    fn body_includes_mfa_code_when_supplied() {
        let json = serde_json::to_string(&PortalLoginBody {
            tenant_slug: String::new(),
            email: "contact@example.com".to_string(),
            password: "pw".to_string(),
            mfa_code: "123456".to_string(),
        })
        .expect("serializes");
        assert!(json.contains(r#""mfa_code":"123456""#), "{json}");
    }

    #[test]
    fn decodes_the_login_response() {
        // PMS-729 phase 2 H2: response now carries refresh_token +
        // refresh_expires_at alongside the access_token. `contact` +
        // `expires_at` are still there but this page ignores them.
        let resp: PortalLoginResp = serde_json::from_str(
            r#"{
                "access_token":"portal.jwt",
                "expires_at":"2026-08-01T00:00:00Z",
                "refresh_token":"00000000-0000-0000-0000-000000000042.some-secret",
                "refresh_expires_at":"2026-08-31T00:00:00Z",
                "contact":{"id":"00000000-0000-0000-0000-000000000000"}
            }"#,
        )
        .expect("portal login response decodes");
        assert_eq!(resp.access_token, "portal.jwt");
        assert!(!resp.refresh_token.is_empty());
    }
}

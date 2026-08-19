//! MAPPS-368: standalone username/password login.
//!
//! Shown by `Login` / `AuthGuard` only when no OIDC issuer is configured
//! (`crate::modules::oidc::is_standalone()`) - a self-hosted deployment with no
//! bunyip OP. Posts to mokosh-server's legacy `POST /api/v1/auth/login` and, on
//! success, seeds the in-memory `AuthContext` plus a persisted standalone
//! session so a page reload survives. Deployments that DO configure an issuer
//! never reach this page; they use the bunyip OIDC redirect exactly as before.
//!
//! Two second-step challenges share the endpoint and the re-POST shape: the
//! MFA code (`mfa_required`) and the emailed sign-in approval code
//! (`approval_required`, PMS-658 / MAPPS-397).
//!
//! Deferred (follow-ups, called out in the PR): silent token refresh via
//! `POST /api/v1/auth/refresh` (standalone sessions currently last the
//! access-token TTL, ~1h).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::CurrentUser;
use crate::Route;

/// Request body for `POST /api/v1/auth/login`, a subset of mokosh-server's
/// `LoginRequest` (the server defaults the omitted optional fields).
#[derive(Serialize)]
struct LoginBody {
    email: String,
    password: String,
    remember_me: bool,
    /// MAPPS-368: the TOTP / recovery code, sent on the second step after a
    /// first attempt reported `mfa_required`. Omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
    /// MAPPS-397: the emailed single-use approval code, sent on the re-POST
    /// after a first attempt reported `approval_required`. Omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_code: Option<String>,
    /// MAPPS-396: tenant slug the operator types into the form. The
    /// server resolves it against `tenants.slug WHERE status = 'active'`
    /// before running the email/password lookup, so a non-default
    /// tenant's user (e.g. `agent@acme.example` under `acme`) can sign
    /// in from the standalone form. Omitted (`None`) falls back to
    /// the default tenant server-side, matching pre-MAPPS-396 behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_slug: Option<String>,
}

/// The fields of mokosh-server's `LoginResponse` this SPA consumes. Unknown
/// fields are ignored by serde.
#[derive(Deserialize)]
struct LoginResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    user: Option<CurrentUser>,
    #[serde(default)]
    mfa_required: bool,
    /// PMS-658: the suspicious-login gate emailed a code and withheld the
    /// tokens. Defaulted so a server that predates the field still decodes.
    #[serde(default)]
    approval_required: bool,
}

#[component]
pub fn StandaloneLogin() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-368: the TOTP field + prompt are revealed after the first attempt
    // reports `mfa_required`; the second submit resends with the code.
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);
    // MAPPS-397: same two-step shape for the emailed sign-in approval code.
    let mut approval_code = use_signal(String::new);
    let mut approval_needed = use_signal(|| false);
    // MAPPS-396: tenant slug (e.g. `acme`), typed by the operator. Blank =
    // fall back to the default tenant server-side. The field is always
    // rendered; a self-hosted single-tenant deployment can just ignore it.
    let mut tenant_slug = use_signal(String::new);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your email and password.".to_string());
            return;
        }
        let code = mfa_code.read().trim().to_string();
        let mfa = if code.is_empty() { None } else { Some(code) };
        let appr = approval_code.read().trim().to_string();
        let approval = if appr.is_empty() { None } else { Some(appr) };
        // MAPPS-396: normalize the slug to lowercase (matches
        // `tenants.slug` writes, which are also lowercase). PMS-728
        // AC1: the server now rejects a local-password login without
        // an explicit tenant identifier, so the standalone login form
        // treats the slug field as required; a blank submission is
        // caught here rather than sent as an empty string the server
        // would 401 (which reads to the operator as bad credentials).
        //
        // MAPPS-473 (PMS-728 followup): when the SPA is served on an
        // agent host (e.g. `default.msp.<apex>`), the server derives
        // the slug from the Host header, so the client-side required
        // check is skipped: sending an empty slug on this path is
        // exactly the intended "zero-typing" flow.
        let slug_raw = tenant_slug.read().trim().to_ascii_lowercase();
        let on_agent_host = crate::hooks::fetch::api::on_agent_host();
        if slug_raw.is_empty() && !on_agent_host {
            error.set(
                "Enter your account slug (the leftmost label of your Mokosh URL).".to_string(),
            );
            return;
        }
        let slug = if slug_raw.is_empty() { None } else { Some(slug_raw) };
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = LoginBody {
                    email: em.clone(),
                    password: pw.clone(),
                    remember_me: false,
                    mfa_code: mfa.clone(),
                    approval_code: approval.clone(),
                    tenant_slug: slug.clone(),
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>("/auth/login", &body)
                    .await
                {
                    // MAPPS-397: the sign-in was flagged and a single-use code
                    // emailed; the tokens are empty and `user` is None, so this
                    // must be handled before the `resp.user` match below or the
                    // form shows "no account was returned" and locks the user
                    // out. The next submit resends with `approval_code`.
                    Ok(resp) if resp.approval_required => {
                        approval_needed.set(true);
                        error.set(
                            "Enter the code we emailed you to approve this sign-in.".to_string(),
                        );
                    }
                    // MFA enrolled but no valid code yet: reveal the code field
                    // and prompt. The next submit resends with `mfa_code`.
                    Ok(resp) if resp.mfa_required => {
                        mfa_needed.set(true);
                        error
                            .set("Enter the 6-digit code from your authenticator app.".to_string());
                    }
                    Ok(resp) => match resp.user {
                        Some(user) => {
                            crate::hooks::fetch::api::set_access_token(Some(
                                resp.access_token.clone(),
                            ));
                            save_standalone(&StandaloneSession {
                                access_token: resp.access_token.clone(),
                                refresh_token: resp.refresh_token.clone(),
                                expires_at: resp.expires_at,
                                user: user.clone(),
                            });
                            let active_tenant_id = Some(user.tenant_id);
                            {
                                let mut a = auth.write();
                                a.user = Some(user);
                                a.is_loading = false;
                                a.error = None;
                                // No OIDC tokens in a standalone session; the
                                // refresh hook no-ops when `tokens` is None.
                                a.tokens = None;
                                a.active_tenant_id = active_tenant_id;
                                a.memberships = Vec::new();
                                // The post-login `/me` loader reconciles the
                                // authoritative user within a tick.
                                a.server_loaded = false;
                            }
                            nav.replace(Route::Dashboard {});
                        }
                        None => {
                            error.set(
                                "Sign-in succeeded but no account was returned. Try again."
                                    .to_string(),
                            );
                        }
                    },
                    // MAPPS-397: past the approval challenge the password has
                    // already passed, so a 401 means the emailed code was wrong
                    // or expired, not bad credentials.
                    Err(ApiError::Status { code: 401, .. }) if approval_needed() => {
                        error.set(
                            "That code is not valid, check the email or try signing in again."
                                .to_string(),
                        );
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid email or password.".to_string());
                    }
                    Err(ApiError::Status { code: 429, .. }) => {
                        error.set(
                            "Too many attempts. Please wait a moment and try again.".to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (em, pw, mfa, approval, slug);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content", "Sign in to Mokosh" }
                        p { class: "mt-2 text-sm text-content",
                            "Enter your account email and password."
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
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

                        // MAPPS-396 / MAPPS-473: tenant slug input. Hidden
                        // entirely when the SPA is served on an agent host
                        // (e.g. `default.msp.<apex>`) because the server
                        // derives the slug from the Host header — no operator
                        // types it. Rendered on legacy / dev deploys without
                        // AGENT_HOST_SUFFIX so those still have a way to
                        // reach the right tenant.
                        if !crate::hooks::fetch::api::on_agent_host() {
                            Input {
                                name: "tenant_slug",
                                label: "Account slug",
                                r#type: "text".to_string(),
                                value: tenant_slug(),
                                required: true,
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    tenant_slug.set(e.value());
                                },
                            }
                        }

                        if mfa_needed() {
                            Input {
                                name: "mfa_code",
                                label: "Authentication code",
                                r#type: "text".to_string(),
                                value: mfa_code(),
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    mfa_code.set(e.value());
                                },
                            }
                        }

                        if approval_needed() {
                            Input {
                                name: "approval_code",
                                label: "Approval code",
                                r#type: "text".to_string(),
                                value: approval_code(),
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    approval_code.set(e.value());
                                },
                            }
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
                                "Sign in"
                            }
                        }
                    }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MAPPS-397 recurrence gate: exhaustive destructuring of the server's wire
    /// types. A field added to `mokosh_types::auth::LoginResponse` /
    /// `LoginRequest` fails this build instead of being silently dropped by
    /// serde, the way `approval_required` was.
    ///
    /// These are the shape checks that stand in for using the shared types
    /// directly: `LoginResponse` derives only `Serialize` and `LoginRequest`
    /// only `Deserialize`, so neither can be used on the client side of the
    /// wire until mokosh-types derives both.
    #[allow(dead_code)]
    fn login_response_fields_are_all_read(resp: mokosh_types::auth::LoginResponse) {
        let mokosh_types::auth::LoginResponse {
            access_token,
            refresh_token,
            expires_at,
            user,
            mfa_required,
            approval_required,
        } = resp;
        let _ = LoginResp {
            access_token,
            refresh_token: Some(refresh_token),
            expires_at,
            // The SPA still carries its own hand copy of the user payload.
            user: None,
            mfa_required,
            approval_required,
        };
        let _: Option<mokosh_types::auth::CurrentUser> = user;
    }

    #[allow(dead_code)]
    fn login_request_fields_are_all_considered(req: mokosh_types::auth::LoginRequest) {
        let mokosh_types::auth::LoginRequest {
            email,
            password,
            remember_me,
            mfa_code,
            recovery_code,
            approval_code,
            device_id,
            tenant_id,
            tenant_slug,
        } = req;
        let _ = LoginBody {
            email,
            password,
            remember_me,
            mfa_code,
            approval_code,
            tenant_slug,
        };
        // Deliberately not sent by the standalone form: the MFA recovery code,
        // the per-browser device id, and the hostname-derived tenant UUID hint
        // (MAPPS-396 uses the sibling `tenant_slug` field instead).
        let _ = (recovery_code, device_id, tenant_id);
    }

    #[test]
    fn decodes_approval_required() {
        let resp: LoginResp = serde_json::from_str(
            r#"{"access_token":"","refresh_token":"","expires_at":"2026-07-30T00:00:00Z",
                "mfa_required":false,"approval_required":true}"#,
        )
        .expect("approval challenge decodes");
        assert!(resp.approval_required);
        assert!(!resp.mfa_required);
        assert!(resp.user.is_none());
    }

    #[test]
    fn approval_required_defaults_false_when_absent() {
        let resp: LoginResp = serde_json::from_str(
            r#"{"access_token":"t","refresh_token":"r","expires_at":"2026-07-30T00:00:00Z",
                "mfa_required":false}"#,
        )
        .expect("legacy response decodes");
        assert!(!resp.approval_required);
    }

    fn body(approval_code: Option<&str>) -> LoginBody {
        LoginBody {
            email: "user@example.com".to_string(),
            password: "pw".to_string(),
            remember_me: false,
            mfa_code: None,
            approval_code: approval_code.map(str::to_string),
            tenant_slug: None,
        }
    }

    /// MAPPS-396: `tenant_slug: None` omits the field entirely so a
    /// server that predates the field decodes cleanly, and a
    /// single-tenant self-host does not send a stray empty value.
    #[test]
    fn tenant_slug_is_omitted_when_absent() {
        let json = serde_json::to_string(&body(None)).expect("serializes");
        assert!(!json.contains("tenant_slug"), "{json}");
    }

    /// MAPPS-396: when the operator types a slug the wire carries it
    /// as an ordinary string field, unquoted / unencoded, so the
    /// server's straight column lookup succeeds without extra parsing.
    #[test]
    fn tenant_slug_is_sent_when_present() {
        let mut b = body(None);
        b.tenant_slug = Some("acme".to_string());
        let json = serde_json::to_string(&b).expect("serializes");
        assert!(json.contains(r#""tenant_slug":"acme""#), "{json}");
    }

    #[test]
    fn approval_code_is_omitted_when_absent() {
        let json = serde_json::to_string(&body(None)).expect("serializes");
        assert!(!json.contains("approval_code"), "{json}");
    }

    #[test]
    fn approval_code_is_sent_when_present() {
        let json = serde_json::to_string(&body(Some("123456"))).expect("serializes");
        assert!(json.contains(r#""approval_code":"123456""#), "{json}");
    }
}

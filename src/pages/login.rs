//! MAPPS-368: standalone username/password login.
//!
//! Shown by `Login` / `AuthGuard` only when no OIDC issuer is configured
//! (`crate::modules::oidc::is_standalone()`) - a self-hosted deployment with no
//! bunyip OP. Posts to mokosh-server's legacy `POST /api/v1/auth/login` and, on
//! success, seeds the in-memory `AuthContext` plus a persisted standalone
//! session so a page reload survives. Deployments that DO configure an issuer
//! never reach this page; they use the bunyip OIDC redirect exactly as before.
//!
//! Second-step challenges share the endpoint and the re-POST shape: the MFA
//! code (`mfa_required`) and the emailed sign-in approval code
//! (`approval_required`, PMS-658 / MAPPS-397).
//!
//! MAPPS-492 (MAPPS-474 phase 3): email-only login. The tenant-slug input is
//! GONE; the SPA sends `{email, password}` and the server either auto-scopes
//! (single membership), returns a `needs_selection` picker payload, or a
//! `needs_setup` payload. The picker and the needs-setup screen render
//! in-place in this component rather than as separate routes so the
//! identity_token + memberships never need cross-page persistence.
//!
//! Deferred (follow-ups, called out in the PR): silent token refresh via
//! `POST /api/v1/auth/refresh` (standalone sessions currently last the
//! access-token TTL, ~1h); MFA anti-replay watermark at the identity level.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::CurrentUser;
use crate::Route;

/// Request body for `POST /api/v1/auth/login`, a subset of mokosh-server's
/// `LoginRequest`. MAPPS-492 phase 3: `tenant_slug` is no longer sent by
/// the SPA (the server derives tenant from the identity's memberships).
#[derive(Serialize)]
struct LoginBody {
    email: String,
    password: String,
    remember_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_code: Option<String>,
}

/// MAPPS-492 phase 3: request body for `POST /api/v1/auth/select-tenant`.
#[derive(Serialize)]
struct SelectTenantBody {
    identity_token: String,
    tenant_id: String,
}

/// MAPPS-492 phase 3: one entry in the picker list.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct MembershipItem {
    tenant_id: String,
    tenant_name: String,
    #[serde(default)]
    tenant_slug: String,
    #[serde(default)]
    tenant_kind: String,
    role: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    is_active: bool,
}

/// The fields of mokosh-server's `LoginResponse` this SPA consumes.
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
    #[serde(default)]
    approval_required: bool,
    // MAPPS-492 (MAPPS-474 phase 3):
    #[serde(default)]
    needs_selection: bool,
    #[serde(default)]
    needs_setup: bool,
    #[serde(default)]
    identity_token: Option<String>,
    #[serde(default)]
    memberships: Option<Vec<MembershipItem>>,
}

#[component]
pub fn StandaloneLogin() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);
    let mut approval_code = use_signal(String::new);
    let mut approval_needed = use_signal(|| false);

    // MAPPS-492 (phase 3) in-page picker state. Populated from the login
    // response when `needs_selection` is true; rendered inline so no
    // cross-page state handoff is needed. `needs_setup` uses the same
    // pattern with `identity_token` and a simple "no orgs yet" panel.
    let mut identity_token = use_signal(|| None::<String>);
    let mut memberships = use_signal(Vec::<MembershipItem>::new);
    let mut needs_selection = use_signal(|| false);
    let mut needs_setup = use_signal(|| false);

    // MAPPS-492: installs the auth context + persists the standalone
    // session, then routes to Dashboard. Shared by the auto-scope
    // branch of /auth/login and the /auth/select-tenant response.
    let mut install_session = move |access_token: String,
                                    refresh_token: Option<String>,
                                    expires_at: chrono::DateTime<chrono::Utc>,
                                    user: CurrentUser| {
        crate::hooks::fetch::api::set_access_token(Some(access_token.clone()));
        save_standalone(&StandaloneSession {
            access_token,
            refresh_token,
            expires_at,
            user: user.clone(),
        });
        let active_tenant_id = Some(user.tenant_id);
        {
            let mut a = auth.write();
            a.user = Some(user);
            a.is_loading = false;
            a.error = None;
            a.tokens = None;
            a.active_tenant_id = active_tenant_id;
            a.memberships = Vec::new();
            a.server_loaded = false;
        }
        nav.replace(Route::Dashboard {});
    };

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
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>("/auth/login", &body)
                    .await
                {
                    // MAPPS-397: approval-required challenge.
                    Ok(resp) if resp.approval_required => {
                        approval_needed.set(true);
                        error.set(
                            "Enter the code we emailed you to approve this sign-in.".to_string(),
                        );
                    }
                    // MFA challenge.
                    Ok(resp) if resp.mfa_required => {
                        mfa_needed.set(true);
                        error
                            .set("Enter the 6-digit code from your authenticator app.".to_string());
                    }
                    // MAPPS-492: identity resolved but holds more than one
                    // membership. Render the picker in-place.
                    Ok(resp) if resp.needs_selection => {
                        identity_token.set(resp.identity_token);
                        memberships.set(resp.memberships.unwrap_or_default());
                        needs_selection.set(true);
                    }
                    // MAPPS-492: identity resolved but holds zero
                    // memberships. Render the setup landing.
                    Ok(resp) if resp.needs_setup => {
                        identity_token.set(resp.identity_token);
                        needs_setup.set(true);
                    }
                    // Auto-scope / MFA-completed / approval-completed:
                    // full session returned.
                    Ok(resp) => match resp.user {
                        Some(user) => {
                            install_session(
                                resp.access_token,
                                resp.refresh_token,
                                resp.expires_at,
                                user,
                            );
                        }
                        None => {
                            error.set(
                                "Sign-in succeeded but no account was returned. Try again."
                                    .to_string(),
                            );
                        }
                    },
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
                let _ = (em, pw, mfa, approval);
            }
            saving.set(false);
        });
    };

    // MAPPS-492: the picker click. Trades the identity token + selected
    // tenant_id for a full session at /auth/select-tenant, then routes.
    let mut pick_tenant = move |tenant_id: String| {
        if saving() {
            return;
        }
        let Some(token) = identity_token.read().clone() else {
            error.set("Sign in again to pick a workspace.".to_string());
            needs_selection.set(false);
            return;
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = SelectTenantBody {
                    identity_token: token,
                    tenant_id,
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                    "/auth/select-tenant",
                    &body,
                )
                .await
                {
                    Ok(resp) => match resp.user {
                        Some(user) => install_session(
                            resp.access_token,
                            resp.refresh_token,
                            resp.expires_at,
                            user,
                        ),
                        None => {
                            error.set("Sign-in succeeded but no account was returned.".to_string());
                        }
                    },
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set(
                            "Your sign-in session expired. Enter your email + password again."
                                .to_string(),
                        );
                        needs_selection.set(false);
                        identity_token.set(None);
                    }
                    Err(ApiError::Status { code: 404, .. }) => {
                        error.set(
                            "You do not have access to that workspace. Pick another.".to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = tenant_id;
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if needs_selection() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Choose a workspace" }
                    p { class: "mt-2 text-sm text-content",
                        "You belong to more than one Mokosh organization. Pick one to sign in to."
                    }
                }
                ul { class: "space-y-2",
                    {memberships.read().iter().cloned().map(|m| {
                        let tenant_id = m.tenant_id.clone();
                        let label_name = m.tenant_name.clone();
                        let label_role = m.role.clone();
                        rsx! {
                            li { key: "{tenant_id}",
                                button {
                                    r#type: "button",
                                    class: "w-full text-left px-4 py-3 rounded-md border border-border hover:bg-surface-hover focus:outline-none focus:ring-2 focus:ring-primary",
                                    disabled: saving(),
                                    onclick: {
                                        let tenant_id = tenant_id.clone();
                                        move |_| pick_tenant(tenant_id.clone())
                                    },
                                    div { class: "font-medium text-content", "{label_name}" }
                                    div { class: "text-sm text-content-muted", "Role: {label_role}" }
                                }
                            }
                        }
                    })}
                }
                if !error().is_empty() {
                    p { role: "alert", class: "mt-4 text-sm text-red-600 dark:text-red-400", "{error}" }
                }
            } else if needs_setup() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "No organization yet" }
                    p { class: "mt-2 text-sm text-content",
                        "Your account is not a member of any Mokosh organization. Ask a super-admin to add you, or create your own (self-serve creation lands in a follow-up)."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| {
                            needs_setup.set(false);
                            identity_token.set(None);
                            error.set(String::new());
                        },
                        "Back to sign in"
                    }
                }
            } else {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MAPPS-397 recurrence gate: exhaustive destructuring of the server's wire
    /// types. A field added to `mokosh_types::auth::LoginResponse` /
    /// `LoginRequest` fails this build instead of being silently dropped by
    /// serde, the way `approval_required` was.
    #[allow(dead_code)]
    fn login_response_fields_are_all_read(resp: mokosh_types::auth::LoginResponse) {
        let mokosh_types::auth::LoginResponse {
            access_token,
            refresh_token,
            expires_at,
            user,
            mfa_required,
            approval_required,
            needs_selection,
            needs_setup,
            identity_token,
            memberships,
        } = resp;
        let _ = LoginResp {
            access_token,
            refresh_token: Some(refresh_token),
            expires_at,
            user: None,
            mfa_required,
            approval_required,
            needs_selection,
            needs_setup,
            identity_token,
            memberships: memberships.map(|v| {
                v.into_iter()
                    .map(|m| MembershipItem {
                        tenant_id: m.tenant_id.to_string(),
                        tenant_name: m.tenant_name,
                        tenant_slug: m.tenant_slug,
                        tenant_kind: m.tenant_kind,
                        role: m.role,
                        status: m.status,
                        is_active: m.is_active,
                    })
                    .collect()
            }),
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
        };
        // Deliberately not sent by the standalone form. MAPPS-492 phase 3:
        // tenant_slug and tenant_id are gone from the SPA request; the
        // server derives tenant from the identity's memberships.
        let _ = (recovery_code, device_id, tenant_id, tenant_slug);
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
        .expect("legacy no-approval-field decodes");
        assert!(!resp.approval_required);
    }

    /// MAPPS-492 phase 3: needs_selection payload decodes with the
    /// picker list and identity_token, tokens empty, user absent.
    #[test]
    fn decodes_needs_selection() {
        let resp: LoginResp = serde_json::from_str(
            r#"{"access_token":"","refresh_token":"","expires_at":"2026-07-30T00:00:00Z",
                "mfa_required":false,"approval_required":false,
                "needs_selection":true,"needs_setup":false,
                "identity_token":"eyJfake",
                "memberships":[{"tenant_id":"00000000-0000-0000-0000-000000000001",
                                "tenant_name":"Default","tenant_slug":"default",
                                "tenant_kind":"org","role":"super_admin",
                                "status":"active","is_active":true}]}"#,
        )
        .expect("needs_selection decodes");
        assert!(resp.needs_selection);
        assert!(!resp.needs_setup);
        assert_eq!(resp.identity_token.as_deref(), Some("eyJfake"));
        let mems = resp.memberships.expect("memberships present");
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].tenant_name, "Default");
    }

    #[test]
    fn decodes_needs_setup() {
        let resp: LoginResp = serde_json::from_str(
            r#"{"access_token":"","refresh_token":"","expires_at":"2026-07-30T00:00:00Z",
                "mfa_required":false,"approval_required":false,
                "needs_setup":true,"identity_token":"eyJfake"}"#,
        )
        .expect("needs_setup decodes");
        assert!(resp.needs_setup);
        assert!(!resp.needs_selection);
        assert!(resp.memberships.is_none());
    }
}

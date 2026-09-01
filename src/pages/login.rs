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
/// `LoginRequest`.
///
/// MAPPS-492 phase 3 dropped `tenant_slug` from the apex flow (the server
/// falls into `authenticate_identity_first` and picks tenant via the
/// identity's memberships). MAPPS-553 puts it back for the portal-host
/// flow: when the SPA is served from `<slug>.client.<suffix>` the tenant
/// admin login is intentionally tenant-scoped, so the SPA derives the
/// slug from the current host (via
/// `crate::modules::runtime_config::tenant_slug_from_current_host`) and
/// includes it here. That drives the server into `AuthService::login`,
/// which verifies against `users.password_hash` for that specific
/// (tenant_id, email) row - the credential set on that portal only
/// (MAPPS-551). On the apex the field stays `None` so the identity-first
/// picker path continues to run as before.
#[derive(Serialize)]
struct LoginBody {
    email: String,
    password: String,
    remember_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_slug: Option<String>,
}

/// MAPPS-520: request body for `POST /api/v1/platform/login`. Kept as a
/// distinct struct from `LoginBody` because the platform endpoint takes
/// only email + password (no MFA / approval / remember-me).
#[cfg(feature = "web")]
#[derive(Serialize)]
struct PlatformLoginBody {
    email: String,
    password: String,
}

/// MAPPS-520: subset of mokosh-server's `PlatformLoginResponse` the
/// unified login page decodes. Only the bearer is needed here; the
/// platform admin's profile is refetched from the platform surface
/// after the redirect.
#[cfg(feature = "web")]
#[derive(Deserialize)]
struct PlatformLoginResp {
    access_token: String,
}

/// MAPPS-513 / MAPPS-520: sessionStorage key `/platform/login` (now
/// unified into `/login`) writes its bearer to. Kept in sync with
/// `pages::platform_login::PLATFORM_TOKEN_KEY` and
/// `components::layout::PLATFORM_TOKEN_KEY`.
#[cfg(feature = "web")]
const PLATFORM_TOKEN_KEY: &str = "mokosh:platform_token";

/// MAPPS-549: request body for `POST /api/v1/auth/select-tenant`, used
/// by the auto-pick step to complete a `needs_selection` response
/// without navigating through `/pick-tenant`. Mirrors the pre-existing
/// `pages::pick_tenant::SelectTenantBody`.
#[cfg(feature = "web")]
#[derive(Serialize)]
struct SelectTenantBody {
    identity_token: String,
    tenant_id: String,
}

/// MAPPS-549: default tenant id (00000000-0000-0000-0000-000000000001).
/// Used as a tiebreaker in `choose_membership` so the mokosh operator's
/// canonical Default tenant wins when two same-role memberships come
/// back.
#[cfg(feature = "web")]
const DEFAULT_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";

/// MAPPS-549: role-precedence weight for auto-pick. Higher wins.
#[cfg(feature = "web")]
fn role_rank(role: &str) -> u8 {
    match role {
        "super_admin" => 100,
        "admin" => 80,
        "manager" => 60,
        "technician" => 40,
        "dispatcher" => 30,
        "sales" => 20,
        "finance" => 10,
        _ => 0,
    }
}

/// MAPPS-549: choose the membership the SPA auto-picks when
/// `/auth/login` returns `needs_selection`. Prefers the highest role,
/// ties broken by `DEFAULT_TENANT_ID` then iteration order. Returns
/// `None` for an empty list (caller falls back to the picker route).
#[cfg(feature = "web")]
fn choose_membership(memberships: &[MembershipItem]) -> Option<&MembershipItem> {
    memberships.iter().reduce(|best, next| {
        let best_rank = role_rank(&best.role);
        let next_rank = role_rank(&next.role);
        if next_rank > best_rank {
            return next;
        }
        if next_rank < best_rank {
            return best;
        }
        // Tie on role: prefer DEFAULT_TENANT_ID, then leave the
        // earlier iteration winner in place.
        if next.tenant_id == DEFAULT_TENANT_ID && best.tenant_id != DEFAULT_TENANT_ID {
            return next;
        }
        best
    })
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

    // MAPPS-497 item 6: picker + create-org steps live on dedicated
    // routes (`/pick-tenant`, `/create-org`) that read from
    // `PENDING_LOGIN`. This page no longer holds their state locally;
    // it just navigates + populates the signal.

    // MAPPS-492: installs the auth context + persists the standalone
    // session, then routes to Dashboard. Used only for the auto-scope
    // (single-membership) branch of /auth/login now that pick + create
    // moved to their own routes.
    let mut install_session = move |access_token: String,
                                    refresh_token: Option<String>,
                                    expires_at: chrono::DateTime<chrono::Utc>,
                                    user: CurrentUser| {
        // MAPPS-630: cross-plane isolation. See auth_callback.rs
        // for the OIDC-flow twin of this call.
        crate::hooks::fetch::api::on_staff_signin_clear_contact_side();
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

                // MAPPS-553: on a tenant subdomain
                // (`<slug>.client.<suffix>`) the login is intentionally
                // tenant-scoped: derive the slug from the current host
                // and send it in the LoginBody. The server takes the
                // `AuthService::login` path (see `routes.rs`
                // has_tenant_hint dispatch) and verifies against
                // `users.password_hash` for THAT tenant only. Also
                // skip the platform-login first-attempt here: the
                // platform-admin surface lives at the mokosh apex,
                // not on tenant subdomains, so a POST to
                // `/platform/login` from a subdomain would either be
                // a wasted round-trip or (worse) install a platform
                // bearer under the wrong origin's sessionStorage.
                // mokosh-contact-login: on_portal_host branch retired with
                // the /portal/* route family (prompt 001). The apex login
                // stays; the pre-pivot tenant-slug derivation is gone.
                let on_portal = false;
                let host_slug: Option<String> = None;

                // MAPPS-520: unified /login. Try the platform-admin
                // plane FIRST (only when there is no MFA / approval
                // code in the submission - both are tenant-plane
                // challenges and would confuse a platform login). A
                // 200 stashes the platform bearer in its own
                // sessionStorage slot and navigates to the
                // platform-admin surface. A 401 falls through to the
                // tenant `/auth/login` flow below exactly as before,
                // so a tenant admin at the same email path is
                // unaffected. Any other error also falls through -
                // the tenant call will surface a more actionable
                // message than a naked 500 on the platform endpoint.
                //
                // MAPPS-553: skip this branch entirely on portal
                // hosts (see host_slug derivation above).
                let can_try_platform = !on_portal && mfa.is_none() && approval.is_none();
                if can_try_platform {
                    let platform_body = PlatformLoginBody {
                        email: em.clone(),
                        password: pw.clone(),
                    };
                    match crate::hooks::fetch::api::post_typed::<PlatformLoginResp, _>(
                        "/platform/login",
                        &platform_body,
                    )
                    .await
                    {
                        Ok(resp) => {
                            if let Some(win) = web_sys::window() {
                                if let Ok(Some(store)) = win.session_storage() {
                                    let _ = store.set_item(PLATFORM_TOKEN_KEY, &resp.access_token);
                                }
                            }

                            // MAPPS-520 walkthrough: chain a tenant
                            // login attempt with the same credentials
                            // so a super-admin lands with BOTH
                            // bearers - the platform bearer for
                            // cross-tenant surfaces (Tenants) AND a
                            // tenant admin bearer for tenant-scoped
                            // surfaces (Invitations, Audit Log,
                            // Settings, ...). Server-side
                            // `PlatformAdminService::authenticate`
                            // auto-provisions a matching users row
                            // in DEFAULT_TENANT with role='admin'
                            // (see mokosh-server companion change),
                            // so an operator who has only ever
                            // signed in as platform admin still
                            // gets tenant admin caps here.
                            //
                            // A failure of this chained call is
                            // silent: the platform bearer already
                            // landed, and there are legitimate
                            // reasons the tenant call may not
                            // full-session (needs_selection with
                            // multiple memberships, needs_setup on
                            // a fresh instance with a partial
                            // auto-provision, MFA required on the
                            // tenant users row). The operator can
                            // still use the platform surface; the
                            // tenant surface will kick to /login on
                            // demand.
                            let chained = LoginBody {
                                email: em.clone(),
                                password: pw.clone(),
                                remember_me: false,
                                mfa_code: None,
                                approval_code: None,
                                // MAPPS-553: apex-only path (the
                                // outer `!on_portal` guard prevents
                                // reaching here on a subdomain), so
                                // no tenant hint - the server takes
                                // the identity-first branch and
                                // auto-picks / needs_selects across
                                // the operator's memberships.
                                tenant_slug: None,
                            };
                            if let Ok(tenant_resp) =
                                crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                                    "/auth/login",
                                    &chained,
                                )
                                .await
                            {
                                // MAPPS-549: run the same auto-pick
                                // step here so a chained login that
                                // returns `needs_selection` still
                                // lands a tenant admin session
                                // alongside the platform bearer.
                                // Without this the operator sees
                                // the platform surface but the
                                // Admin sidebar section stays
                                // hidden because is_admin is false
                                // (no tenant user).
                                if let (Some(user), false, false, false, false) = (
                                    tenant_resp.user.clone(),
                                    tenant_resp.mfa_required,
                                    tenant_resp.approval_required,
                                    tenant_resp.needs_selection,
                                    tenant_resp.needs_setup,
                                ) {
                                    install_session(
                                        tenant_resp.access_token,
                                        tenant_resp.refresh_token,
                                        tenant_resp.expires_at,
                                        user,
                                    );
                                } else if tenant_resp.needs_selection {
                                    let memberships =
                                        tenant_resp.memberships.clone().unwrap_or_default();
                                    if let (Some(m), Some(token)) = (
                                        choose_membership(&memberships).cloned(),
                                        tenant_resp.identity_token.clone(),
                                    ) {
                                        let body = SelectTenantBody {
                                            identity_token: token,
                                            tenant_id: m.tenant_id.clone(),
                                        };
                                        if let Ok(resp2) =
                                            crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                                                "/auth/select-tenant",
                                                &body,
                                            )
                                            .await
                                        {
                                            if let Some(user) = resp2.user {
                                                install_session(
                                                    resp2.access_token,
                                                    resp2.refresh_token,
                                                    resp2.expires_at,
                                                    user,
                                                );
                                            }
                                        }
                                    }
                                }
                            }

                            // Land on the platform-admin surface
                            // (tenant-management console). Uses the
                            // platform bearer via the client's
                            // platform-authed fetch helpers; the
                            // tenant bearer (if the chain above
                            // installed it) is used implicitly by
                            // every tenant-scoped fetch elsewhere in
                            // the SPA.
                            // mokosh-contact-login: TenantManagement route
                            // retired with the Clients tab (prompt 001).
                            // Land on the standard dashboard.
                            nav.push(Route::Dashboard {});
                            saving.set(false);
                            return;
                        }
                        Err(ApiError::Status { code: 401, .. }) => {
                            // Not a platform admin (or wrong platform
                            // password). Fall through to try the
                            // tenant credential.
                        }
                        Err(_) => {
                            // Non-401 platform error: fall through
                            // rather than block. The tenant call
                            // below may still succeed and will
                            // surface a clearer error if it fails.
                        }
                    }
                }

                let body = LoginBody {
                    email: em.clone(),
                    password: pw.clone(),
                    remember_me: false,
                    mfa_code: mfa.clone(),
                    approval_code: approval.clone(),
                    // MAPPS-553: `Some(slug)` on the tenant subdomain
                    // (drives `AuthService::login` -> tenant-scoped
                    // verify against `users.password_hash`);
                    // `None` on the apex (drives
                    // `authenticate_identity_first` -> picker /
                    // needs_setup as before).
                    tenant_slug: host_slug.clone(),
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
                    // MAPPS-492 / MAPPS-497 item 6 / MAPPS-549:
                    // identity resolved but the login response asks
                    // for a picker step (multiple active
                    // memberships). Rather than presenting the picker
                    // by default, auto-pick the highest-role
                    // membership (see `choose_membership`) and
                    // complete `/auth/select-tenant` inline. On
                    // success install the session and land on
                    // Dashboard exactly as the auto-scope branch
                    // does. On failure (no memberships, missing
                    // identity_token, or the select-tenant call
                    // itself fails) fall back to the pre-549
                    // behavior: populate `PENDING_LOGIN` and
                    // navigate to `/pick-tenant` so the manual UX
                    // stays as a last-resort path for the small set
                    // of consultants who have equal-role
                    // memberships across multiple client MSPs.
                    Ok(resp) if resp.needs_selection => {
                        let memberships = resp.memberships.clone().unwrap_or_default();
                        let picked = choose_membership(&memberships).cloned();
                        let identity_token = resp.identity_token.clone();
                        let mut auto_picked = false;
                        if let (Some(m), Some(token)) = (picked.as_ref(), identity_token.clone()) {
                            let body = SelectTenantBody {
                                identity_token: token,
                                tenant_id: m.tenant_id.clone(),
                            };
                            if let Ok(resp2) = crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                                "/auth/select-tenant",
                                &body,
                            )
                            .await
                            {
                                if let Some(user) = resp2.user {
                                    install_session(
                                        resp2.access_token,
                                        resp2.refresh_token,
                                        resp2.expires_at,
                                        user,
                                    );
                                    auto_picked = true;
                                }
                            }
                        }
                        if !auto_picked {
                            let carried_memberships: Vec<
                                crate::hooks::pending_login::PickerMembership,
                            > = memberships
                                .into_iter()
                                .map(|m| crate::hooks::pending_login::PickerMembership {
                                    tenant_id: m.tenant_id,
                                    tenant_name: m.tenant_name,
                                    tenant_slug: m.tenant_slug,
                                    tenant_kind: m.tenant_kind,
                                    role: m.role,
                                    status: m.status,
                                    is_active: m.is_active,
                                })
                                .collect();
                            *crate::hooks::pending_login::PENDING_LOGIN.write() =
                                crate::hooks::pending_login::PendingLogin {
                                    identity_token,
                                    memberships: carried_memberships,
                                };
                            nav.push(Route::PickTenant {});
                        }
                    }
                    // MAPPS-492 / MAPPS-497 item 6: identity resolved
                    // but holds zero memberships. Navigate to the
                    // dedicated `/create-org` route (was inline
                    // before item 6).
                    Ok(resp) if resp.needs_setup => {
                        *crate::hooks::pending_login::PENDING_LOGIN.write() =
                            crate::hooks::pending_login::PendingLogin {
                                identity_token: resp.identity_token,
                                memberships: Vec::new(),
                            };
                        nav.push(Route::CreateOrg {});
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

    // MAPPS-497 item 6: `submit_new_org` and `pick_tenant` moved to
    // `pages/create_org.rs` and `pages/pick_tenant.rs` respectively.
    // The login page's sole job is now email+password submit; the
    // response handler above navigates to those routes after
    // populating the shared `PENDING_LOGIN` signal.

    // mokosh-contact-login: portal-footer link retired with the
    // /portal/* route family (prompt 001).
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
            // MAPPS-615 (prompt 014): cross-plane switch. A visitor who
            // meant to sign into a client portal but landed on staff
            // /login can jump without browser-back-buttoning. Sits
            // below the primary form so the staff sign-in stays
            // visually dominant.
            div { class: "pt-6 mt-6 border-t border-line text-center",
                Link {
                    to: Route::ContactGenericLogin {},
                    class: "text-sm text-accent hover:underline",
                    "Sign in to a client portal instead"
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
            // MAPPS-553: SPA now DOES send tenant_slug when running
            // on a tenant subdomain. The apex flow still sends
            // `None` here, but the field is on the wire so the
            // destructuring test carries it back through.
            tenant_slug,
        };
        // Deliberately not sent by the standalone form. MAPPS-492 phase 3
        // dropped tenant_id, and MAPPS-397 doesn't wire recovery_code /
        // device_id into the standalone form yet.
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

//! Authentication hooks

use dioxus::prelude::*;

use crate::modules::auth::CurrentUser;
use crate::modules::oidc::Tokens;
use crate::Route;

/// Authentication context for the application
#[derive(Clone, Default)]
pub struct AuthContext {
    pub user: Option<CurrentUser>,
    pub is_loading: bool,
    pub error: Option<String>,
    /// OIDC tokens. Held in memory only; never persisted (XSS protection).
    /// `None` until the user completes the authorize-redirect-callback dance.
    pub tokens: Option<Tokens>,
}

impl AuthContext {
    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    /// Get the current user, panics if not authenticated
    pub fn user(&self) -> &CurrentUser {
        self.user.as_ref().expect("User not authenticated")
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.user
            .as_ref()
            .map(|u| u.role.as_str() == role)
            .unwrap_or(false)
    }

    /// Check if user has any of the specified roles
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        self.user
            .as_ref()
            .map(|u| roles.contains(&u.role.as_str()))
            .unwrap_or(false)
    }
}

/// Hook to access authentication state
pub fn use_auth() -> Signal<AuthContext> {
    use_context::<Signal<AuthContext>>()
}

/// Hook to require authentication, redirects to login if not authenticated
pub fn use_require_auth() -> Signal<AuthContext> {
    let auth = use_auth();
    let navigator = use_navigator();

    use_effect(move || {
        let auth_state = auth.read();
        if !auth_state.is_loading && !auth_state.is_authenticated() {
            navigator.push(Route::Login {});
        }
    });

    auth
}

/// Hook to require a specific role
pub fn use_require_role(required_role: &'static str) -> Signal<AuthContext> {
    let auth = use_require_auth();
    let navigator = use_navigator();

    use_effect(move || {
        let auth_state = auth.read();
        if !auth_state.is_loading
            && auth_state.is_authenticated()
            && !auth_state.has_role(required_role)
        {
            // Redirect to dashboard if user doesn't have required role
            navigator.push(Route::Dashboard {});
        }
    });

    auth
}

/// Provide authentication context to the application
pub fn use_auth_provider() -> Signal<AuthContext> {
    let auth = use_signal(initial_auth_context);
    use_context_provider(|| auth);
    auth
}

/// DEV ONLY: when both ADMIN_EMAIL and ADMIN_PASSWORD are set at compile time
/// AND non-empty, the app starts pre-authenticated as that admin user and the
/// login screen is bypassed. Mirrors the bootstrap pattern from vervain-server.
/// Compiled out of release builds via `cfg(debug_assertions)`.
#[cfg(debug_assertions)]
fn initial_auth_context() -> AuthContext {
    match (option_env!("ADMIN_EMAIL"), option_env!("ADMIN_PASSWORD")) {
        (Some(email), Some(password)) if !email.is_empty() && !password.is_empty() => AuthContext {
            user: Some(CurrentUser {
                id: uuid::Uuid::nil(),
                tenant_id: uuid::Uuid::nil(),
                email: email.to_string(),
                first_name: "Admin".to_string(),
                last_name: "User".to_string(),
                role: crate::modules::auth::UserRole::Admin,
                timezone: "UTC".to_string(),
                avatar_url: None,
            }),
            is_loading: false,
            error: None,
            tokens: None,
        },
        _ => AuthContext::default(),
    }
}

#[cfg(not(debug_assertions))]
fn initial_auth_context() -> AuthContext {
    AuthContext::default()
}

/// Login form state
#[derive(Clone, Default)]
pub struct LoginFormState {
    pub email: String,
    pub password: String,
    pub remember_me: bool,
    pub is_submitting: bool,
    pub error: Option<String>,
}

/// Hook for the login page. Submitting the form POSTs the credentials
/// directly to mokosh-server's `/v1/auth/login` endpoint. When the
/// request includes our `client_id`, the server responds with both the
/// OP-session cookie AND a minted OIDC token bundle, so the SPA gets
/// the same access/refresh/id tokens it would have received via the
/// OIDC code+PKCE redirect dance - without ever bouncing through a
/// second login page. The OP login UI was retired with this change;
/// the only sign-in screen users see is the SPA's own form.
pub fn use_login_form() -> (Signal<LoginFormState>, Callback<()>) {
    let mut form_state = use_signal(LoginFormState::default);
    let mut auth = use_auth();
    let navigator = use_navigator();

    let submit = use_callback(move |_| {
        let email = form_state.read().email.clone();
        let password = form_state.read().password.clone();
        let nav = navigator;
        spawn(async move {
            form_state.write().is_submitting = true;
            form_state.write().error = None;

            let cfg = crate::modules::oidc::OidcConfig::from_env();
            match crate::modules::oidc::password_login(&cfg, &email, &password).await {
                Ok(tokens) => {
                    let claims = match tokens.id_claims() {
                        Ok(c) => c,
                        Err(e) => {
                            form_state.write().error = Some(format!("invalid id_token: {e}"));
                            form_state.write().is_submitting = false;
                            return;
                        }
                    };
                    let user_id = claims
                        .sub
                        .parse::<uuid::Uuid>()
                        .unwrap_or_else(|_| uuid::Uuid::nil());
                    let tenant_id = claims
                        .tenant_id
                        .as_deref()
                        .and_then(|s| s.parse::<uuid::Uuid>().ok())
                        .unwrap_or_else(uuid::Uuid::nil);
                    let role = claims
                        .role
                        .as_deref()
                        .and_then(|s| match s {
                            "admin" => Some(crate::modules::auth::UserRole::Admin),
                            "manager" => Some(crate::modules::auth::UserRole::Manager),
                            "finance" => Some(crate::modules::auth::UserRole::Finance),
                            _ => None,
                        })
                        .unwrap_or_default();
                    crate::hooks::fetch::api::set_access_token(Some(tokens.access_token.clone()));
                    {
                        let mut a = auth.write();
                        a.user = Some(crate::modules::auth::CurrentUser {
                            id: user_id,
                            tenant_id,
                            email: claims.email.clone().unwrap_or_default(),
                            first_name: String::new(),
                            last_name: String::new(),
                            role,
                            timezone: "UTC".to_string(),
                            avatar_url: None,
                        });
                        a.is_loading = false;
                        a.error = None;
                        a.tokens = Some(tokens);
                    }
                    form_state.write().is_submitting = false;
                    nav.push(Route::Dashboard {});
                }
                Err(e) => {
                    form_state.write().error = Some(e.to_string());
                    form_state.write().is_submitting = false;
                }
            }
        });
    });

    (form_state, submit)
}

/// Background token-refresh loop. Mount once near the root of the app
/// (alongside `use_auth_provider`). Polls the AuthContext every 30
/// seconds; if the access token is within 60s of expiry, exchanges the
/// refresh token for a new pair and pushes the result back into the
/// context. On any refresh failure (the storage layer detected reuse,
/// the refresh token has expired, the network is gone) the local auth
/// state is cleared and the browser is sent to /login. The user
/// experiences a transparent re-login rather than mysterious 401s.
pub fn use_token_refresh() {
    let mut auth = use_auth();
    // Note: this hook is mounted on the root `App` component, which is
    // the *parent* of `Router`, not a descendant. `use_navigator()`
    // panics when called outside a Router subtree, so on refresh
    // failure we fall back to a hard `window.location` redirect to
    // `/login`. Same end result for the user.

    use_future(move || async move {
        loop {
            #[cfg(feature = "web")]
            gloo_timers::future::TimeoutFuture::new(30_000).await;

            // Snapshot what we need under a short read; never hold the
            // lock across the network call.
            let snap = {
                let a = auth.read();
                a.tokens
                    .as_ref()
                    .and_then(|t| {
                        t.refresh_token
                            .as_ref()
                            .map(|rt| (t.access_token.clone(), rt.clone(), t.id_token.clone(), t.expires_at))
                    })
            };
            let (_access, refresh, id_token, expires_at) = match snap {
                Some(s) => s,
                None => continue, // not signed in, nothing to do
            };

            // Refresh window: 60s before expiry. If we already missed
            // it (clock jump / tab was backgrounded), refresh now.
            let now = chrono::Utc::now();
            if expires_at - now > chrono::Duration::seconds(60) {
                continue;
            }

            let cfg = crate::modules::oidc::OidcConfig::from_env();
            match crate::modules::oidc::refresh_tokens(&cfg, &refresh, &id_token).await {
                Ok(new_tokens) => {
                    crate::hooks::fetch::api::set_access_token(Some(new_tokens.access_token.clone()));
                    auth.write().tokens = Some(new_tokens);
                }
                Err(e) => {
                    tracing::warn!("token refresh failed; signing out: {e}");
                    {
                        let mut a = auth.write();
                        a.user = None;
                        a.tokens = None;
                    }
                    crate::hooks::fetch::api::set_access_token(None);
                    // Hard redirect (see note on the hook above): we
                    // are outside the Router subtree, so use_navigator
                    // is unavailable. window.location.set_href works
                    // regardless and triggers a full page reload,
                    // which is appropriate after a forced sign-out.
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().set_href("/login");
                    }
                }
            }
        }
    });
}

/// Hook for logout. Three things happen:
///   1. Refresh token is revoked at `/oauth2/revoke` (RFC 7009 form
///      POST, no credentials needed). Killing the family ensures any
///      stale tokens that survive in browser caches are useless.
///   2. The in-memory auth signal and global access-token holder are
///      cleared.
///   3. The browser navigates to `/login` via `location.replace`, NOT
///      `set_href`. Replace evicts the previous (authenticated) URL
///      from history, so back-button cannot return to the
///      authenticated view (which would otherwise reboot the SPA at
///      `/dashboard` and momentarily flash protected content).
pub fn use_logout() -> impl FnMut() {
    let mut auth = use_auth();

    move || {
        let refresh = auth
            .read()
            .tokens
            .as_ref()
            .and_then(|t| t.refresh_token.clone());
        {
            let mut a = auth.write();
            a.user = None;
            a.tokens = None;
        }
        crate::hooks::fetch::api::set_access_token(None);

        spawn(async move {
            if let Some(rt) = refresh {
                let cfg = crate::modules::oidc::OidcConfig::from_env();
                let _ = crate::modules::oidc::revoke_refresh_token(&cfg, &rt).await;
            }
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/login");
            }
        });
    }
}

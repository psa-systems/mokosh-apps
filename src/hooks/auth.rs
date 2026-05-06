//! Authentication hooks

use dioxus::prelude::*;

use crate::modules::auth::{AuthState, CurrentUser};
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

/// Hook for the login page. Pressing the button starts the OIDC code
/// flow with PKCE: `start_login` redirects the browser to mokosh-server's
/// authorize endpoint. After the user signs in there, the browser comes
/// back to `/auth/callback`, which exchanges the code for tokens and
/// populates [`AuthContext`]. The form's email/password fields are kept
/// in [`LoginFormState`] for compatibility with the existing UI but are
/// not sent to the IdP from here (the IdP renders its own form).
pub fn use_login_form() -> (Signal<LoginFormState>, impl Fn()) {
    let mut form_state = use_signal(LoginFormState::default);

    let submit = move || {
        spawn(async move {
            form_state.write().is_submitting = true;
            form_state.write().error = None;

            let cfg = crate::modules::oidc::OidcConfig::from_env();
            // Returning to "/" means "send the user to /dashboard after
            // login" in our callback handler.
            if let Err(e) = crate::modules::oidc::start_login(&cfg, "/dashboard") {
                form_state.write().error = Some(e.to_string());
                form_state.write().is_submitting = false;
            }
            // On success the browser is navigating away; nothing else
            // to do here.
        });
    };

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
    let navigator = use_navigator();

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
                    navigator.push(Route::Login {});
                }
            }
        }
    });
}

/// Hook for logout. Clears tokens locally, then sends the browser to
/// the OP's `/oauth2/logout` endpoint so the IdP-side session is also
/// killed and any other relying parties get back-channel logout tokens.
pub fn use_logout() -> impl FnMut() {
    let mut auth = use_auth();
    let navigator = use_navigator();

    move || {
        let id_token_hint = auth
            .read()
            .tokens
            .as_ref()
            .map(|t| t.id_token.clone());
        {
            let mut a = auth.write();
            a.user = None;
            a.tokens = None;
        }
        // Clear the global access-token holder so any in-flight api
        // calls from this point on go out unauthenticated.
        crate::hooks::fetch::api::set_access_token(None);

        let cfg = crate::modules::oidc::OidcConfig::from_env();
        let issuer = cfg.issuer.trim_end_matches('/');
        let mut url = format!("{issuer}/oauth2/logout");
        if let Some(hint) = id_token_hint {
            url.push_str("?id_token_hint=");
            let encoded: String = js_sys::encode_uri_component(&hint).into();
            url.push_str(&encoded);
        }
        if let Some(win) = web_sys::window() {
            if win.location().set_href(&url).is_ok() {
                return;
            }
        }
        navigator.push(Route::Login {});
    }
}

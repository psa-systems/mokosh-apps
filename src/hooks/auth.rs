//! Authentication hooks

use dioxus::prelude::*;

use crate::modules::auth::{AuthState, CurrentUser};
use crate::Route;

/// Authentication context for the application
#[derive(Clone, Default)]
pub struct AuthContext {
    pub user: Option<CurrentUser>,
    pub is_loading: bool,
    pub error: Option<String>,
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

/// Hook for login form
pub fn use_login_form() -> (Signal<LoginFormState>, impl Fn()) {
    let mut form_state = use_signal(LoginFormState::default);
    let mut auth = use_auth();
    let navigator = use_navigator();

    let submit = move || {
        let state = form_state.read().clone();

        spawn(async move {
            form_state.write().is_submitting = true;
            form_state.write().error = None;

            // TODO: Call login API
            // For now, simulate a login
            #[cfg(feature = "web")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(1000).await;

                // Simulate success
                auth.write().user = Some(CurrentUser {
                    id: uuid::Uuid::new_v4(),
                    tenant_id: uuid::Uuid::new_v4(),
                    email: state.email.clone(),
                    first_name: "Demo".to_string(),
                    last_name: "User".to_string(),
                    role: crate::modules::auth::UserRole::Admin,
                    timezone: "UTC".to_string(),
                    avatar_url: None,
                });
                auth.write().is_loading = false;

                navigator.push(Route::Dashboard {});
            }

            form_state.write().is_submitting = false;
        });
    };

    (form_state, submit)
}

/// Hook for logout
pub fn use_logout() -> impl FnMut() {
    let mut auth = use_auth();
    let navigator = use_navigator();

    move || {
        // Clear auth state
        auth.write().user = None;

        // TODO: Call logout API to invalidate token

        // Redirect to login
        navigator.push(Route::Login {});
    }
}

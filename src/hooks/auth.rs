//! Authentication hooks

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::modules::auth::CurrentUser;
use crate::modules::oidc::Tokens;
use crate::Route;

/// One row in [`AuthContext::memberships`]. Mirrors the shape of the
/// server's `/v1/auth/memberships` response so the switcher UI can
/// render directly off the cached vec.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct MembershipView {
    pub tenant_id: String,
    pub tenant_name: String,
    pub tenant_kind: String,
    pub role: String,
    pub status: String,
    pub is_active: bool,
}

/// Authentication context for the application
#[derive(Clone, Default)]
pub struct AuthContext {
    pub user: Option<CurrentUser>,
    pub is_loading: bool,
    pub error: Option<String>,
    /// OIDC tokens. Held in memory only; never persisted (XSS protection).
    /// `None` until the user completes the authorize-redirect-callback dance.
    pub tokens: Option<Tokens>,
    /// Tenant the user is currently acting under. Sourced from the
    /// `mokosh_active_tenant` claim in the access token; updated on
    /// switch. None before sign-in.
    pub active_tenant_id: Option<uuid::Uuid>,
    /// Every membership the user has. Loaded from /v1/auth/memberships
    /// after sign-in; powers the tenant switcher UI. Empty before
    /// sign-in.
    pub memberships: Vec<MembershipView>,
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

    /// Return the membership matching `active_tenant_id` so callers can
    /// pull a display-ready tenant name or role for the current scope
    /// without re-walking the membership list. None before sign-in or
    /// while memberships are still loading.
    pub fn active_membership(&self) -> Option<&MembershipView> {
        let active = self.active_tenant_id?;
        let active_str = active.to_string();
        self.memberships
            .iter()
            .find(|m| m.tenant_id == active_str)
    }

    /// Display name for the active org, or `None` when there isn't one
    /// to show (pre-login, mid-bootstrap, or active tenant somehow
    /// missing from memberships).
    pub fn active_org_name(&self) -> Option<&str> {
        self.active_membership().map(|m| m.tenant_name.as_str())
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
            active_tenant_id: None,
            memberships: Vec::new(),
        },
        _ => rehydrate_from_storage().unwrap_or_default(),
    }
}

#[cfg(not(debug_assertions))]
fn initial_auth_context() -> AuthContext {
    rehydrate_from_storage().unwrap_or_default()
}

/// Pull a persisted token bundle out of `sessionStorage` and rebuild
/// an `AuthContext` from it. Called once at WASM boot so a URL-bar
/// navigation or tab reload doesn't drop the user back on /login.
/// The id_token's claims are the source of truth for the
/// `CurrentUser` view model: the same parsing as the OIDC callback.
/// Returns `None` (caller falls back to `default()`) if there is no
/// stored bundle, the bundle's id_token is malformed, or
/// sessionStorage is disabled.
fn rehydrate_from_storage() -> Option<AuthContext> {
    use crate::modules::oidc::storage::load_auth;
    use crate::modules::oidc::Tokens;

    let stored = load_auth()?;
    let tokens = Tokens {
        access_token: stored.access_token,
        id_token: stored.id_token,
        refresh_token: stored.refresh_token,
        expires_at: stored.expires_at,
        scope: stored.scope,
    };
    let claims = tokens.id_claims().ok()?;
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

    let active_tenant_id = claims
        .active_tenant_id
        .as_deref()
        .and_then(|s| s.parse::<uuid::Uuid>().ok())
        .or(Some(tenant_id));
    crate::hooks::fetch::api::set_access_token(Some(tokens.access_token.clone()));
    Some(AuthContext {
        user: Some(CurrentUser {
            id: user_id,
            tenant_id,
            email: claims.email.clone().unwrap_or_default(),
            first_name: String::new(),
            last_name: String::new(),
            role,
            timezone: "UTC".to_string(),
            avatar_url: None,
        }),
        is_loading: false,
        error: None,
        tokens: Some(tokens),
        active_tenant_id,
        // memberships: empty on rehydrate; the App-level loader hook
        // re-fetches them once the SPA mounts. Avoids persisting the
        // membership list (it can drift independently of the token).
        memberships: Vec::new(),
    })
}

/// Load `/v1/auth/memberships` into AuthContext after sign-in. Watches
/// the auth signal and re-fetches whenever the user transitions from
/// "no membership list" to "have a session" (login, page reload that
/// rehydrates from sessionStorage). Cheap GET, runs at most a few
/// times per session. Mount once at the app root.
pub fn use_memberships_loader() {
    let mut auth = use_auth();
    use_effect(move || {
        let needs_load = {
            let a = auth.read();
            a.is_authenticated() && a.memberships.is_empty()
        };
        if !needs_load {
            return;
        }
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
            #[derive(serde::Deserialize)]
            struct Body {
                memberships: Vec<MembershipView>,
                #[serde(default)]
                active_tenant_id: Option<String>,
            }
            match crate::modules::oidc::issuer_get_authed::<Body>(&cfg, "/v1/auth/memberships")
                .await
            {
                Ok(b) => {
                    let active = b
                        .active_tenant_id
                        .as_deref()
                        .and_then(|s| s.parse::<uuid::Uuid>().ok());
                    let mut a = auth.write();
                    a.memberships = b.memberships;
                    if active.is_some() {
                        a.active_tenant_id = active;
                    }
                }
                Err(e) => {
                    tracing::warn!("memberships load failed: {e}");
                }
            }
        });
    });
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
                a.tokens.as_ref().and_then(|t| {
                    t.refresh_token.as_ref().map(|rt| {
                        (
                            t.access_token.clone(),
                            rt.clone(),
                            t.id_token.clone(),
                            t.expires_at,
                        )
                    })
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

            let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
            match crate::modules::oidc::refresh_tokens(&cfg, &refresh, &id_token).await {
                Ok(new_tokens) => {
                    crate::hooks::fetch::api::set_access_token(Some(
                        new_tokens.access_token.clone(),
                    ));
                    crate::modules::oidc::storage::save_auth(
                        &crate::modules::oidc::storage::StoredTokens {
                            access_token: new_tokens.access_token.clone(),
                            id_token: new_tokens.id_token.clone(),
                            refresh_token: new_tokens.refresh_token.clone(),
                            expires_at: new_tokens.expires_at,
                            scope: new_tokens.scope.clone(),
                        },
                    );
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
                    crate::modules::oidc::storage::clear_auth();
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

/// Defeat back-forward-cache (bfcache) restoration of authenticated
/// pages. Browsers snapshot the entire JS heap (including our
/// in-memory auth signals) when a page is unloaded and restore it
/// verbatim on back navigation, which would put a logged-out user
/// back into a populated dashboard until something else triggers a
/// re-render. We listen for `pageshow` with `persisted=true` (only
/// fires for bfcache restores, not normal loads) and force a full
/// reload, which reruns `initial_auth_context()` and drops the user
/// onto `/login` if they have no live session.
pub fn use_bfcache_invalidator() {
    use_effect(move || {
        let win = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |evt: web_sys::Event| {
            // `persisted` is only present on PageTransitionEvent (the
            // pageshow/pagehide payload). We read it via Reflect to
            // avoid pulling in the PageTransitionEvent web-sys feature.
            let persisted =
                js_sys::Reflect::get(&evt, &wasm_bindgen::JsValue::from_str("persisted"))
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            if persisted {
                if let Some(w) = web_sys::window() {
                    let _ = w.location().reload();
                }
            }
        })
            as Box<dyn FnMut(web_sys::Event)>);
        let _ = win.add_event_listener_with_callback("pageshow", handler.as_ref().unchecked_ref());
        // Listener must outlive its registration; SPA root, fires once.
        handler.forget();
    });
}

/// Hook for logout.
///
/// Order matters here: the call to `location.replace("/login")` MUST
/// run before any write to the auth signal. Otherwise:
///   - `auth.write(user=None)` schedules a Dioxus re-render
///   - the microtask fires, route guards see an unauthenticated user
///     on `/dashboard` and call `navigator.push(Login)`, which adds
///     `/login` on TOP of `/dashboard` in history
///   - by the time we navigate away, `/dashboard` is still at
///     `history[-1]` and the back button puts the user right back
///     onto an authenticated-looking page
///
/// Doing the navigation first avoids the re-render entirely: the
/// page is already on its way to a full reload, which will reset all
/// in-memory state from scratch. The refresh-token revoke is
/// fire-and-forget; modern browsers complete in-flight fetches even
/// after the document starts unloading.
pub fn use_logout() -> impl FnMut() {
    let auth = use_auth();

    move || {
        let refresh = auth
            .read()
            .tokens
            .as_ref()
            .and_then(|t| t.refresh_token.clone());

        if let Some(rt) = refresh {
            let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
            spawn(async move {
                let _ = crate::modules::oidc::revoke_refresh_token(&cfg, &rt).await;
            });
        }

        // Drop the persisted bundle so a reload after logout does
        // not silently re-authenticate from a stale id_token.
        crate::modules::oidc::storage::clear_auth();

        if let Some(win) = web_sys::window() {
            let _ = win.location().replace("/login");
        }
    }
}

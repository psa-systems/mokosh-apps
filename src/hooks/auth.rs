//! Authentication hooks

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::modules::auth::{CurrentUser, UserRole};
use crate::modules::oidc::Tokens;
use crate::Route;

/// One organisation the signed-in user acts under.
///
/// MAPPS-427: this used to mirror bunyip's `/v1/auth/memberships` response
/// field for field. It no longer comes from there, and of the six fields only
/// these two were ever read: `tenant_id` to match the active org, `tenant_name`
/// to display it. `tenant_kind`, `role`, `status` and `is_active` existed to
/// mirror a payload nothing consumed, and keeping them now would mean inventing
/// four values per row.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct MembershipView {
    pub tenant_id: String,
    pub tenant_name: String,
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
    /// Tenant the user is currently acting under. Seeded from the home
    /// tenant at sign-in and updated on switch. None before sign-in.
    pub active_tenant_id: Option<uuid::Uuid>,
    /// Every organisation the user acts under. One row today: mokosh is
    /// single-tenant-per-user (PMS-447) and the switcher itself lives in
    /// bunyip's hub. Empty before sign-in, and empty when the load failed.
    pub memberships: Vec<MembershipView>,
    /// MAPPS-427: whether the org load has been attempted, successful or not.
    ///
    /// The effect used to re-fire on an empty list, which meant a failure had
    /// to be papered over with a fabricated row or it would retry on every
    /// render. This lets a failure stay a failure: no org name is shown, and
    /// nothing invents one.
    pub memberships_loaded: bool,
    /// MAPPS-317: false until `/api/v1/auth/me` has reconciled the
    /// optimistic rehydrate at least once. The AuthGuard's forced-
    /// onboarding redirect reads this so it never fires on the
    /// optimistic window. Without the gate, the chain
    /// AuthGuard (stale signal) -> /onboarding/profile -> Onboarding
    /// effect sees profile_completed=true -> /dashboard bounces a
    /// user clicking Calendar to Dashboard on the first click.
    pub server_loaded: bool,
}

impl AuthContext {
    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: UserRole) -> bool {
        self.user.as_ref().is_some_and(|u| u.role == role)
    }

    /// Return the membership matching `active_tenant_id` so callers can
    /// pull a display-ready tenant name or role for the current scope
    /// without re-walking the membership list. None before sign-in or
    /// while memberships are still loading.
    pub fn active_membership(&self) -> Option<&MembershipView> {
        let active = self.active_tenant_id?;
        let active_str = active.to_string();
        self.memberships.iter().find(|m| m.tenant_id == active_str)
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

/// Provide authentication context to the application
pub fn use_auth_provider() -> Signal<AuthContext> {
    let auth = use_signal(initial_auth_context);
    use_context_provider(|| auth);
    auth
}

/// DEV ONLY: when both ADMIN_EMAIL and ADMIN_PASSWORD are set at compile time
/// AND non-empty, the app starts pre-authenticated as that admin user and the
/// login screen is bypassed. Mirrors the bootstrap pattern from vervain-server.
///
/// MAPPS-338: the gate is now `cfg(all(debug_assertions, feature =
/// "dev_admin_bypass"))` instead of `cfg(debug_assertions)` alone. A
/// debug WASM that ships to staging or production by accident (CI
/// mishap, operator-side build) no longer auto-signs the user in as
/// Admin. Dev devs enable the bypass explicitly via
/// `--features dev_admin_bypass`; the feature is off by default in every
/// shipped artifact.
#[cfg(all(debug_assertions, feature = "dev_admin_bypass"))]
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
                profile_completed: true,
                date_format_string: None,
                theme_base_mode: None,
                theme_accent_id: None,
                own_company_id: None,
            }),
            is_loading: false,
            error: None,
            tokens: None,
            active_tenant_id: None,
            memberships: Vec::new(),
            memberships_loaded: false,
            // Dev-only ADMIN_EMAIL bypass; the AuthGuard's onboarding
            // gate trusts this synthesized user without a /me round-trip.
            server_loaded: true,
        },
        _ => rehydrate_from_storage()
            .or_else(rehydrate_standalone)
            .unwrap_or_default(),
    }
}

#[cfg(not(all(debug_assertions, feature = "dev_admin_bypass")))]
fn initial_auth_context() -> AuthContext {
    rehydrate_from_storage()
        .or_else(rehydrate_standalone)
        .unwrap_or_default()
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
    // Role comes from `/api/v1/auth/me` (PMS-158); the id_token carries no
    // usable role claim post-cutover, so seed the Technician default and let
    // the post-rehydrate /me fetch reconcile it within a tick.
    let role = crate::modules::auth::UserRole::default();

    // The id_token has no active-tenant claim; seed the active tenant from the
    // home tenant. A tenant switch (or the memberships load) updates it.
    let active_tenant_id = Some(tenant_id);
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
            // Optimistic default; /me reconciles within one tick. See
            // matching note in pages/auth_callback.rs.
            profile_completed: true,
            date_format_string: None,
            // Optimistic None; the post-rehydrate /me fetch reconciles the
            // authoritative theme prefs within a tick.
            theme_base_mode: None,
            theme_accent_id: None,
            // The id_token has no own-company claim; the post-rehydrate
            // /me fetch (use_current_user_loader) fills it in within a tick.
            own_company_id: None,
        }),
        is_loading: false,
        error: None,
        tokens: Some(tokens),
        active_tenant_id,
        // memberships: empty on rehydrate; the App-level loader hook
        // re-fetches them once the SPA mounts. Avoids persisting the
        // membership list (it can drift independently of the token).
        memberships: Vec::new(),
        memberships_loaded: false,
        // MAPPS-317: false until the post-rehydrate /me fetch lands.
        // AuthGuard's onboarding gate trusts this flag; see lib.rs.
        server_loaded: false,
    })
}

/// MAPPS-368: rehydrate a standalone (non-OIDC) session persisted by the login
/// form. Unlike [`rehydrate_from_storage`] there is no id_token to rebuild the
/// user from, so the stored `CurrentUser` is used directly; the post-boot `/me`
/// loader reconciles it within a tick. Returns `None` when no standalone
/// session is stored, so the caller falls through to the OIDC path / default.
fn rehydrate_standalone() -> Option<AuthContext> {
    use crate::modules::oidc::storage::load_standalone;

    let stored = load_standalone()?;
    let active_tenant_id = Some(stored.user.tenant_id);
    crate::hooks::fetch::api::set_access_token(Some(stored.access_token.clone()));
    Some(AuthContext {
        user: Some(stored.user),
        is_loading: false,
        error: None,
        // No OIDC tokens in a standalone session; the refresh hook no-ops.
        tokens: None,
        active_tenant_id,
        memberships: Vec::new(),
        memberships_loaded: false,
        // /me reconciles the authoritative user on the next tick.
        server_loaded: false,
    })
}

/// Load the organisation the user acts under, from mokosh, after sign-in.
///
/// MAPPS-427: this used to GET bunyip's `/v1/auth/memberships`, and that call
/// could never succeed. bunyip-api's `/v1/*` family is a Resource Server whose
/// verifier enforces `aud == OIDC_RS_AUDIENCE` (BUNYIP-252), and the token this
/// SPA holds is minted for mokosh's audience, so bunyip correctly answered 401
/// on every page load. The failure path then seeded a synthetic membership
/// whose name was the user's EMAIL ADDRESS, which is what the top bar and the
/// board view have been displaying as an organisation name.
///
/// Even authorised it would not have helped: bunyip's handler is a stub that
/// returns one hardcoded row whose `tenant_name` is also the user's email,
/// pending its phase-04 multi-tenant work.
///
/// So the name now comes from mokosh's own `/tenants/current` (PMS-751), which
/// is the exact column every client-facing email is composed from. One row,
/// because mokosh is single-tenant-per-user (PMS-447) and the switcher itself
/// lives in bunyip's hub; this list exists to name the current org, not to
/// choose between orgs.
///
/// A failure leaves the list empty rather than inventing a row. Callers already
/// handle "no org name" (`active_org_name()` returns `None`), and a missing
/// name is a better answer than a wrong one.
pub fn use_active_org_loader() {
    let mut auth = use_auth();
    use_effect(move || {
        let needs_load = {
            let a = auth.read();
            a.is_authenticated() && !a.memberships_loaded
        };
        if !needs_load {
            return;
        }
        spawn(async move {
            #[derive(serde::Deserialize)]
            struct TenantView {
                #[serde(default)]
                id: String,
                #[serde(default)]
                name: String,
            }

            let fetched =
                crate::hooks::fetch::api::get_authed::<TenantView>("/tenants/current").await;

            let mut a = auth.write();
            // Set first and unconditionally: a failed load must not re-fire on
            // every render, which is what forced the fabricated row before.
            a.memberships_loaded = true;
            match fetched {
                Ok(t) if !t.name.trim().is_empty() => {
                    // The authoritative id, replacing whatever the id_token
                    // claim did or did not carry (PMS-751: it carries nothing
                    // today, so this was the nil uuid).
                    if let Ok(id) = t.id.parse::<uuid::Uuid>() {
                        a.active_tenant_id = Some(id);
                    }
                    a.memberships = vec![MembershipView {
                        tenant_id: t.id,
                        tenant_name: t.name,
                    }];
                }
                Ok(_) => {
                    tracing::warn!("organisation load returned no name; leaving it unset");
                }
                Err(e) => {
                    tracing::warn!("organisation load failed, leaving it unset: {e}");
                }
            }
        });
    });
}

/// Background token-refresh loop. Mount once near the root of the app
/// (alongside `use_auth_provider`). Evaluates the persisted token bundle
/// at mount and every 30 seconds after; if the access token is within
/// 60s of expiry, exchanges the refresh token for a new pair and pushes
/// the result back into the context. On any refresh failure (the storage
/// layer detected reuse, the refresh token has expired, the network is
/// gone) the local auth state is cleared and the browser is sent to
/// /login. The user experiences a transparent re-login rather than
/// mysterious 401s.
///
/// MAPPS-435: this loop is no longer the only thing standing between a
/// spent token and a request. The renewal it drives lives in
/// [`crate::hooks::fetch::api::renew_access_token`], which the request
/// path also calls, because a tab the browser discarded and re-created
/// mounts its pages before any loop has ticked.
pub fn use_token_refresh() {
    let mut auth = use_auth();
    // Note: this hook is mounted on the root `App` component, which is
    // the *parent* of `Router`, not a descendant. `use_navigator()`
    // panics when called outside a Router subtree, so on refresh
    // failure we fall back to a hard `window.location` redirect to
    // `/login`. Same end result for the user.

    use_future(move || async move {
        loop {
            oidc_refresh_tick(&mut auth).await;
            // MAPPS-435: the sleep is LAST. Sleeping first meant a tab the
            // browser had discarded and re-created ran on whatever
            // sessionStorage held for the first 30 seconds, however dead, and
            // every page that mounted in that window 401'd.
            #[cfg(feature = "web")]
            gloo_timers::future::TimeoutFuture::new(30_000).await;
        }
    });
}

/// One evaluation of the OIDC refresh window. Returns without renewing when
/// there is no persisted OIDC session, it carries no refresh token, or the
/// access token is still comfortably valid.
async fn oidc_refresh_tick(auth: &mut Signal<AuthContext>) {
    // The persisted bundle is the source of truth, not the context's copy: a
    // renewal on the request path (MAPPS-435) rotates the refresh token in
    // sessionStorage, and replaying the superseded copy the context still
    // holds is what the OP's reuse detection answers by killing the grant.
    let stored = match crate::modules::oidc::storage::load_auth() {
        Some(s) => s,
        None => return, // not signed in through OIDC, nothing to do
    };
    if stored.refresh_token.is_none() {
        return; // nothing to renew with; let it expire
    }

    // Refresh window: 60s before expiry. If we already missed it (clock jump /
    // tab was backgrounded), refresh now.
    let now = chrono::Utc::now();
    if stored.expires_at - now > chrono::Duration::seconds(60) {
        return;
    }

    // Renewal itself lives in the fetch layer so a tick and a request-time
    // renewal that coincide share one flight (MAPPS-435).
    match crate::hooks::fetch::api::renew_access_token().await {
        Ok(()) => {
            // Mirror the rotated bundle into the context so its copy does not
            // age out behind sessionStorage.
            if let Some(fresh) = crate::modules::oidc::storage::load_auth() {
                auth.write().tokens = Some(Tokens {
                    access_token: fresh.access_token,
                    id_token: fresh.id_token,
                    refresh_token: fresh.refresh_token,
                    expires_at: fresh.expires_at,
                    scope: fresh.scope,
                });
            }

            // Refresh the cached CurrentUser from /v1/auth/me so any
            // server-side change since the original id_token was minted (role
            // demotion, name update, active-tenant switch from another tab)
            // propagates within one refresh window. Mokosh is RFC-compliant
            // and typically omits id_token on refresh-grant responses, so the
            // cached id_token's claims will be stale for the life of the
            // session if we don't actively re-fetch. Memberships are also
            // cleared so the membership-loader effect re-runs.
            refresh_user_from_me(auth).await;
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
            // Hard redirect (see note on the hook above): we are outside the
            // Router subtree, so use_navigator is unavailable.
            // window.location.set_href works regardless and triggers a full
            // page reload, which is appropriate after a forced sign-out.
            if let Some(win) = web_sys::window() {
                if let Err(e) = win.location().set_href("/login") {
                    tracing::warn!("redirect to /login after sign-out failed: {e:?}");
                }
            }
        }
    }
}

/// MAPPS-374: silent refresh for STANDALONE (non-OIDC) sessions - the mirror of
/// [`use_token_refresh`] for the legacy mokosh-server email+password path.
/// Standalone sessions live in sessionStorage under `STANDALONE_KEY` with
/// `AuthContext.tokens = None`, so the OIDC loop above skips them (its snapshot
/// short-circuits on `tokens = None`). Without this loop the ~1h standalone
/// access token simply expired and the next 401 dropped the user to /login
/// (MAPPS-368). Mounted once at the app root alongside [`use_token_refresh`].
///
/// Each tick reloads the persisted session (so a login or logout in this tab is
/// picked up) and, once the access token is within 60s of expiry, rotates it via
/// `POST /api/v1/auth/refresh`, persisting the new tokens back to sessionStorage
/// and the fetch access-token holder. On any refresh failure the session is
/// cleared and the browser is sent to /login, matching the OIDC error branch and
/// the MAPPS-368 standalone-401 clear.
pub fn use_standalone_token_refresh() {
    let mut auth = use_auth();

    use_future(move || async move {
        loop {
            standalone_refresh_tick(&mut auth).await;
            // MAPPS-435: sleep LAST, for the reason given on the OIDC loop.
            #[cfg(feature = "web")]
            gloo_timers::future::TimeoutFuture::new(30_000).await;
        }
    });
}

/// One evaluation of the standalone refresh window. Reloads the persisted
/// session each tick so a login or logout in this tab is seen.
async fn standalone_refresh_tick(auth: &mut Signal<AuthContext>) {
    // An OIDC session is the loop above's business, and the shared renewal
    // prefers the OIDC bundle, so bail before reaching for it.
    if crate::modules::oidc::storage::load_auth().is_some() {
        return;
    }
    let session = match crate::modules::oidc::storage::load_standalone() {
        Some(s) => s,
        None => return,
    };
    // A standalone session with no refresh token cannot be renewed; leave it
    // to expire rather than spin on it every tick.
    if session.refresh_token.is_none() {
        return;
    }

    // Refresh window: 60s before expiry. If a backgrounded tab sailed past it
    // (throttled timers), refresh now - same policy as the OIDC loop above.
    let now = chrono::Utc::now();
    if session.expires_at - now > chrono::Duration::seconds(60) {
        return;
    }

    // Shared single-flight renewal (MAPPS-435); it persists the rotated
    // session and swaps the fetch layer's bearer.
    if let Err(e) = crate::hooks::fetch::api::renew_access_token().await {
        tracing::warn!("standalone token refresh failed; signing out: {e}");
        {
            let mut a = auth.write();
            a.user = None;
            a.tokens = None;
        }
        crate::modules::oidc::storage::clear_standalone();
        crate::hooks::fetch::api::set_access_token(None);
        // Hard redirect (mirrors use_token_refresh): this hook is mounted
        // above the Router, so use_navigator is unavailable.
        if let Some(win) = web_sys::window() {
            if let Err(e) = win.location().set_href("/login") {
                tracing::warn!("redirect to /login after sign-out failed: {e:?}");
            }
        }
    }
}

/// MAPPS-355: proactive auth heartbeat. Mount once at the app root
/// (alongside [`use_token_refresh`]). Every 30 seconds while the user is
/// signed in AND the tab is visible AND the account is not already flagged
/// deleted, fires an authed `GET /api/v1/auth/me`. On a 410 response the
/// shared fetch layer's `handle_response` already calls
/// [`crate::hooks::fetch::note_account_deleted`], which flips the
/// [`crate::hooks::fetch::ACCOUNT_DELETED`] `GlobalSignal` and pops the
/// `AccountDeletedOverlay` sitting at `AppLayout` root. So the hook itself
/// discards the response - it only needs to fire the request.
///
/// Without this loop the SPA only discovers a soft-delete when the user
/// happens to touch a page that fetches: a user idle on the dashboard
/// after their Bunyip account was deleted could stare at stale UI for up
/// to the at+jwt TTL (15 min) before any request fired. 30s cadence puts
/// the "you were signed out" overlay in front of them fast without
/// meaningful cost - ~120 requests/hour per active tab, cheaper than the
/// token-refresh loop above.
///
/// Skipped when:
/// - no access token in the holder (unauthenticated / booting),
/// - `document.visibilityState == 'hidden'` (backgrounded tab),
/// - `ACCOUNT_DELETED` is already set (overlay is up, no point poking).
pub fn use_auth_heartbeat() {
    use_future(move || async move {
        loop {
            heartbeat_tick().await;
            // MAPPS-435: sleep LAST, for the reason given on the OIDC loop.
            #[cfg(feature = "web")]
            gloo_timers::future::TimeoutFuture::new(30_000).await;
        }
    });
}

/// One heartbeat evaluation. Returns without firing when the overlay is
/// already up, nobody is signed in, or the tab is in the background.
async fn heartbeat_tick() {
    #[cfg(feature = "web")]
    {
        if *crate::hooks::fetch::ACCOUNT_DELETED.peek() {
            return;
        }
        if crate::hooks::fetch::api::current_access_token().is_none() {
            return;
        }
        if tab_is_hidden() {
            return;
        }
        // Fire and discard. The fetch layer handles the 410 case (flips
        // ACCOUNT_DELETED) and, since MAPPS-435, the 401 case (renew, else
        // clear the session and redirect); 200 / network errors are no-ops
        // here.
        let _ = crate::hooks::fetch::api::get_authed_typed::<serde_json::Value>("/auth/me").await;
    }
}

/// Read `document.visibilityState`. Returns `true` when the tab is hidden
/// (backgrounded, minimised, on another tab), so the heartbeat above skips
/// its request. Non-`web` builds report the tab as visible so the same
/// call site type-checks under `cargo check` without the `web` feature.
#[cfg(feature = "web")]
fn tab_is_hidden() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.visibility_state() == web_sys::VisibilityState::Hidden)
        .unwrap_or(false)
}

#[cfg(not(feature = "web"))]
#[allow(dead_code)]
fn tab_is_hidden() -> bool {
    false
}

/// Pull the authoritative current user from mokosh-server
/// `GET /api/v1/auth/me` and merge fresh fields onto `auth.user`.
///
/// The role is sourced from the API on purpose. The OIDC id_token cannot
/// be trusted for the *mokosh* role: bunyip mints its own `bunyip_role`
/// (`subscriber` / `admin`), and the mapping to a mokosh role
/// (`admin` -> `super_admin`, etc.) is applied server-side (PMS-172). So
/// the id_token claim the SPA used to parse is absent and the user falls
/// back to the Technician default (PMS-158). `/api/v1/auth/me` returns the
/// already-translated role, so it is the single source of truth here.
///
/// Best-effort: on error we leave the cached user as-is.
async fn refresh_user_from_me(auth: &mut Signal<AuthContext>) {
    #[derive(serde::Deserialize)]
    struct MeBody {
        id: String,
        email: String,
        first_name: Option<String>,
        last_name: Option<String>,
        timezone: String,
        avatar_url: Option<String>,
        role: String,
        // `false` until the user confirms first + last name via the
        // onboarding screen. Default `true` for backwards-compat with
        // older server builds.
        #[serde(default = "default_true_me")]
        profile_completed: bool,
        #[serde(default)]
        date_format_string: Option<String>,
        // PMS-413: the tenant's own-company id, used to attribute a
        // General / overhead time entry. `None` on a pre-backfill tenant.
        #[serde(default)]
        own_company_id: Option<uuid::Uuid>,
    }
    fn default_true_me() -> bool {
        true
    }
    let me = match crate::hooks::fetch::api::get_authed_typed::<MeBody>("/auth/me").await {
        Ok(m) => m,
        // MAPPS-368: a 401 with no refreshable token is an expired standalone
        // session. The OIDC path renews via its refresh hook and keeps
        // `tokens = Some`, so its 401s are handled there, not here. Standalone
        // has no refresh, so clear the session and drop to unauthenticated;
        // AuthGuard then routes to the login form, instead of leaving the app
        // stuck "logged in" with a dead token that 401s every request.
        Err(crate::hooks::fetch::api::ApiError::Status { code: 401, .. }) => {
            let standalone_session = auth.read().tokens.is_none();
            if standalone_session {
                crate::modules::oidc::storage::clear_auth();
                crate::hooks::fetch::api::set_access_token(None);
                *auth.write() = AuthContext::default();
            }
            return;
        }
        Err(e) => {
            tracing::warn!("/api/v1/auth/me failed; keeping cached user: {e:?}");
            return;
        }
    };
    // Parse via UserRole::from_str. An unrecognized value is handled
    // explicitly (warn + keep the current role) rather than silently
    // coerced to the Technician default (PMS-158).
    let new_role = match crate::modules::auth::UserRole::from_str(&me.role) {
        Some(r) => Some(r),
        None => {
            tracing::warn!(
                "unrecognized role {:?} from /api/v1/auth/me; keeping cached role",
                me.role
            );
            None
        }
    };
    let mut a = auth.write();
    if let Some(u) = a.user.as_mut() {
        if let Ok(id) = me.id.parse::<uuid::Uuid>() {
            u.id = id;
        }
        u.email = me.email;
        u.first_name = me.first_name.unwrap_or_default();
        u.last_name = me.last_name.unwrap_or_default();
        u.timezone = me.timezone;
        u.avatar_url = me.avatar_url;
        if let Some(role) = new_role {
            u.role = role;
        }
        u.profile_completed = me.profile_completed;
        u.date_format_string = me.date_format_string;
        u.own_company_id = me.own_company_id;
    }
    // MAPPS-317: flip the gate so AuthGuard's onboarding-redirect
    // check now trusts profile_completed. Must run AFTER the user
    // mutate above so any AuthGuard re-render that observes
    // server_loaded=true also sees the reconciled profile flag.
    a.server_loaded = true;
    // MAPPS-427: force the org loader to run again now that /me has confirmed
    // the session. Clearing the flag rather than the list, because the effect
    // keys off the flag: an empty list is a legitimate outcome (the load
    // failed), not a signal to retry on every render.
    a.memberships.clear();
    a.memberships_loaded = false;
}

/// On first authenticated mount, fetch the authoritative user from
/// mokosh-server `/api/v1/auth/me` so the displayed role (and name /
/// avatar) is correct immediately, not only after the first token
/// refresh. Sources the role from the API for the reasons in
/// [`refresh_user_from_me`] (PMS-158). Mount once near the app root.
pub fn use_current_user_loader() {
    let mut auth = use_auth();
    let mut loaded = use_signal(|| false);
    use_effect(move || {
        if !auth.read().is_authenticated() || *loaded.peek() {
            return;
        }
        loaded.set(true);
        spawn(async move {
            refresh_user_from_me(&mut auth).await;
        });
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

#[cfg(test)]
mod tests {
    /// This module's own source, minus this test module: the assertions below
    /// name the very strings they forbid, so scanning the whole file would make
    /// them match themselves.
    fn production_src() -> &'static str {
        const AUTH_SRC: &str = include_str!("auth.rs");
        AUTH_SRC
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file has a test module")
    }

    /// MAPPS-427 recurrence guard.
    ///
    /// The org name must come from mokosh's own tenant row. It used to come
    /// from bunyip's `/v1/auth/memberships`, which answers 401 for this SPA's
    /// token by design (BUNYIP-252 enforces `aud == OIDC_RS_AUDIENCE`), and the
    /// failure path then displayed the user's email address as an organisation
    /// name. A source scan rather than a behavioural test because the loader is
    /// an effect that needs a browser; what is being pinned is which endpoint it
    /// reaches for, and that is visible in the source.
    #[test]
    fn the_org_name_is_read_from_mokosh_not_the_issuer() {
        assert!(
            production_src().contains("get_authed::<TenantView>(\"/tenants/current\")"),
            "the org loader must read mokosh's own tenant row"
        );
        // The doc comments explain the history, so only a real CALL counts.
        assert!(
            !production_src().contains("issuer_get_authed"),
            "an issuer-hosted call cannot succeed with a mokosh-audience token"
        );
    }

    /// MAPPS-435 recurrence guard.
    ///
    /// A polling loop that sleeps before it evaluates is useless to the case
    /// that motivated it: a tab the browser discarded and re-created starts
    /// every loop from scratch, so a sleep-first loop leaves the SPA running
    /// on whatever sessionStorage held for the first 30 seconds, however
    /// stale, and every page that mounts in that window 401s. The condition
    /// therefore has to be evaluated once at mount, before any sleep.
    #[test]
    fn the_auth_loops_evaluate_before_they_sleep() {
        let mut loops = 0;
        for segment in production_src().split("loop {").skip(1) {
            loops += 1;
            let first = segment
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("//"))
                .unwrap_or_default();
            assert!(
                !first.contains("TimeoutFuture") && !first.starts_with("#[cfg"),
                "this loop sleeps before it evaluates anything: {first}"
            );
        }
        assert_eq!(loops, 3, "this file runs three polling loops");
    }

    /// A failed load must leave the org unnamed rather than invent one. The
    /// effect keys off `memberships_loaded`, so an empty list is a legitimate
    /// end state and not a signal to retry on every render.
    #[test]
    fn a_failed_load_does_not_fabricate_an_organisation() {
        assert!(
            !production_src().contains("fn synthesize_single_membership"),
            "the synthetic fallback is what put an email address in the top bar"
        );
        assert!(
            production_src().contains("a.memberships_loaded = true;"),
            "the loader must record the attempt, not the outcome"
        );
    }
}

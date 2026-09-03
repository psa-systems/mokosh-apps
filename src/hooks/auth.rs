//! Authentication hooks

use dioxus::prelude::*;

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

    /// PMS-791 / MAPPS-462: shorthand for "current user has admin
    /// privileges" (admin OR super_admin). Mirrors the pattern used by
    /// every existing admin-gated page (audit_log.rs, sla.rs, settings.rs)
    /// that manually walks `.user.as_ref().map(|u| u.role.is_admin())`.
    pub fn is_admin(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.role.is_admin())
    }

    /// PMS-791 / MAPPS-462: true when the caller's tenant is a
    /// multi-user org tenant. Empty tenant_kind (older server) reads as
    /// org (fail-open UI; server still gates the actual endpoints).
    pub fn is_org_tenant(&self) -> bool {
        let kind = self
            .user
            .as_ref()
            .map(|u| u.tenant_kind.as_str())
            .unwrap_or("");
        matches!(kind, "org" | "")
    }

    /// PMS-791 / MAPPS-462: strict "personal tenant" check — only true
    /// when tenant_kind is explicitly "personal".
    pub fn is_personal_tenant(&self) -> bool {
        self.user
            .as_ref()
            .is_some_and(|u| u.tenant_kind == "personal")
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

    /// Point the cached organisation name at what the server just stored.
    ///
    /// MAPPS-571: [`use_active_org_loader`] runs once per session. It sets
    /// `memberships_loaded` on its first attempt, deliberately, so a failed load
    /// stays a failure instead of re-firing forever (MAPPS-427) - but that also
    /// means nothing re-reads `tenants.name` after a rename. Both writers of
    /// that column call this so the top bar, the dispatch board heading and the
    /// onboarding screen show the new name without a page reload.
    ///
    /// Pass the name from the server's response rather than the form signal: the
    /// server trims and length-checks it, so the response is what was actually
    /// stored and the form value is only what was typed.
    ///
    /// Updates an existing row and otherwise does nothing, returning whether it
    /// found one. A rename is not a reason to break MAPPS-427's rule that a
    /// failed org load leaves the list empty rather than inventing a row: with no
    /// membership the right thing to display is still no name, not one
    /// reconstructed from a form field. An empty or whitespace name is ignored
    /// for the same reason - the server rejects it (`1..=255`), so seeing one
    /// here means the response was not what it claimed, and the previous name is
    /// a better answer than none.
    pub fn set_active_org_name(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(active) = self.active_tenant_id.map(|id| id.to_string()) else {
            return false;
        };
        match self.memberships.iter_mut().find(|m| m.tenant_id == active) {
            Some(m) => {
                m.tenant_name = name.to_string();
                true
            }
            None => false,
        }
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
            // PMS-791 / MAPPS-462: no tenant_kind claim in the id_token
            // either; the post-rehydrate /me fetch fills it. Default
            // empty; is_org_tenant() treats "" as org (fail-open).
            tenant_kind: String::new(),
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

/// Load `/api/v1/auth/memberships` from mokosh into AuthContext after
/// sign-in. Watches the auth signal and re-fetches whenever the user
/// transitions from "no membership list" to "have a session" (login,
/// or a page reload that rehydrates from sessionStorage). Cheap GET,
/// runs at most a few times per session. Mount once at the app root.
///
/// MAPPS-497 item 3: this hook used to hit bunyip's endpoint and fall
/// back to a synthetic single-tenant row when bunyip was unreachable.
/// Phase 2 (MAPPS-491) landed a mokosh-side `/api/v1/auth/memberships`
/// that returns the real multi-tenant list from `tenant_memberships`,
/// which is now the canonical source. The bunyip call + the synthetic
/// fallback are gone; a failed fetch leaves `memberships` empty, which
/// the switcher UI handles (hides the trigger, shows "Create new"
/// through the user menu instead).
///
/// Exposed under the historical `use_active_org_loader` name as well
/// (see the `pub use` further down) so mid-merge callers keep
/// compiling.
pub fn use_memberships_loader() {
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
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::get_authed_typed::<Vec<MembershipView>>(
                    "/auth/memberships",
                )
                .await
                {
                    Ok(list) if !list.is_empty() => {
                        let mut a = auth.write();
                        a.memberships = list;
                        a.memberships_loaded = true;
                    }
                    Ok(_) => {
                        tracing::debug!("mokosh /auth/memberships returned no rows");
                        auth.write().memberships_loaded = true;
                    }
                    Err(e) => {
                        tracing::warn!("mokosh /auth/memberships load failed: {e}");
                        auth.write().memberships_loaded = true;
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                auth.write().memberships_loaded = true;
            }
        });
    });
}

/// Historical name used by `src/main.rs` and older doc comments. Kept as an
/// alias so the mid-merge boot path keeps compiling; delegates straight to
/// [`use_memberships_loader`].
pub fn use_active_org_loader() {
    use_memberships_loader();
}

/// Background token-refresh loop. Mount once near the root of the app
/// (alongside `use_auth_provider`). Evaluates the persisted token bundle
/// at mount and every 30 seconds after; if the access token is within
/// 60s of expiry, exchanges the refresh token for a new pair and pushes
/// the result back into the context. When the refusal says the grant
/// itself is dead (the storage layer detected reuse, the refresh token
/// has expired, there is no refresh token left to spend) the local auth
/// state is cleared and the browser is sent to /login. The user
/// experiences a transparent re-login rather than mysterious 401s. A
/// transient failure keeps the session and retries; see
/// [`renewal_is_unrecoverable`].
///
/// MAPPS-435: this loop is no longer the only thing standing between a
/// spent token and a request. The renewal it drives lives in
/// [`crate::hooks::fetch::api::renew_access_token`], which the request
/// path also calls, because a tab the browser discarded and re-created
/// mounts its pages before any loop has ticked.
///
/// MAPPS-645: nor is the 30-second cadence the only thing that drives it. A
/// tab coming back to the foreground evaluates immediately, because that is
/// the moment its pages re-fetch and the moment the token is most likely to
/// have expired while nobody was looking.
pub fn use_token_refresh() {
    let mut auth = use_auth();
    // Note: this hook is mounted on the root `App` component, which is
    // the *parent* of `Router`, not a descendant. `use_navigator()`
    // panics when called outside a Router subtree, so on refresh
    // failure we fall back to a hard `window.location` redirect to
    // `/login`. Same end result for the user.

    use_future(move || async move {
        crate::platform::dom::watch_visibility();
        loop {
            oidc_refresh_tick(&mut auth).await;
            // MAPPS-435: the sleep is LAST. Sleeping first meant a tab the
            // browser had discarded and re-created ran on whatever
            // sessionStorage held for the first 30 seconds, however dead, and
            // every page that mounted in that window 401'd.
            #[cfg(feature = "app")]
            sleep_or_wake(POLL_INTERVAL_MS).await;
        }
    });
}

/// How long a poll tick waits before re-evaluating, when nothing wakes it.
#[cfg(feature = "app")]
const POLL_INTERVAL_MS: u32 = 30_000;

/// Wait for the next scheduled tick, or for the app to come back to the
/// foreground, whichever happens first (MAPPS-645).
///
/// A suspended tab used to wake, fire the requests its pages mount with
/// against a token that had expired while it slept, and sit on whatever error
/// they earned for the rest of this sleep - up to 30 seconds, with a reload
/// only re-entering the same race. Both arms mean the same thing to the
/// caller, "evaluate now", so there is no outcome to inspect.
#[cfg(feature = "app")]
async fn sleep_or_wake(ms: u32) {
    let sleep = std::pin::pin!(crate::platform::timer::sleep_ms(ms));
    let wake = std::pin::pin!(crate::platform::dom::visible_again());
    futures_util::future::select(sleep, wake).await;
}

/// Is a failed renewal one this session cannot come back from?
///
/// [`crate::hooks::fetch::api::renew_access_token`] answers
/// `Result<(), String>`, so the classification reads the message that layer
/// produced. Anything unrecognised counts as transient on purpose: since
/// MAPPS-645 a tick fires the instant a tab is foregrounded, which is exactly
/// when the network is least likely to be back, and signing a user out over a
/// blip is worse than the one 401 that follows. Nothing is stranded by being
/// wrong in that direction - a request that then 401s runs
/// `note_agent_unauthorized`, which ends the session itself.
fn renewal_is_unrecoverable(err: &str) -> bool {
    // Refused before it reached the wire: there is nothing to renew from and
    // there never will be.
    if err.contains("carries no refresh token") || err.contains("no persisted session to renew") {
        return true;
    }
    // OIDC. `FlowError::TokenEndpoint` renders as
    // `token endpoint: {error} ({description})`; only the OAuth 2.0 codes that
    // mean the grant itself is dead count. A 5xx from the OP arrives here too
    // (as the synthesized `token_endpoint_failed`) and is worth retrying.
    if let Some(rest) = err.strip_prefix("token endpoint: ") {
        let code = rest.split_once(' ').map_or(rest, |(code, _)| code);
        return matches!(
            code,
            "invalid_grant" | "invalid_client" | "unauthorized_client" | "unsupported_grant_type"
        );
    }
    // Standalone. `ApiError` renders as `http {code}: {message}`; a 4xx is the
    // server refusing the refresh token, a 5xx is the server having a bad day.
    if let Some(rest) = err.strip_prefix("http ") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        return matches!(digits.parse::<u16>(), Ok(400..=499));
    }
    false
}

/// Drop an OIDC session that cannot be renewed and put the user on /login.
///
/// The one place that branch lives, so the wake-triggered evaluation and the
/// scheduled one end a dead session identically.
fn end_oidc_session(auth: &mut Signal<AuthContext>, cause: &str) {
    tracing::warn!("token refresh failed unrecoverably; signing out: {cause}");
    {
        let mut a = auth.write();
        a.user = None;
        a.tokens = None;
    }
    crate::hooks::fetch::api::set_access_token(None);
    crate::modules::oidc::storage::clear_auth();
    redirect_to_login();
}

/// The standalone (non-OIDC) equivalent of [`end_oidc_session`].
fn end_standalone_session(auth: &mut Signal<AuthContext>, cause: &str) {
    tracing::warn!("standalone token refresh failed unrecoverably; signing out: {cause}");
    {
        let mut a = auth.write();
        a.user = None;
        a.tokens = None;
    }
    crate::modules::oidc::storage::clear_standalone();
    crate::hooks::fetch::api::set_access_token(None);
    redirect_to_login();
}

/// One evaluation of the OIDC refresh window. Returns without renewing when
/// there is no persisted OIDC session or the access token is still comfortably
/// valid. A session already past expiry with no refresh token left to spend is
/// ended here rather than renewed (MAPPS-645).
async fn oidc_refresh_tick(auth: &mut Signal<AuthContext>) {
    // The persisted bundle is the source of truth, not the context's copy: a
    // renewal on the request path (MAPPS-435) rotates the refresh token in
    // sessionStorage, and replaying the superseded copy the context still
    // holds is what the OP's reuse detection answers by killing the grant.
    let stored = match crate::modules::oidc::storage::load_auth() {
        Some(s) => s,
        None => return, // not signed in through OIDC, nothing to do
    };
    // Refresh window: 60s before expiry. If we already missed it (clock jump /
    // tab was backgrounded), refresh now.
    let now = chrono::Utc::now();
    if stored.expires_at - now > chrono::Duration::seconds(60) {
        return;
    }

    // MAPPS-645: nothing to renew with. While the access token is still good
    // there is nothing to do; once it is spent the session cannot come back,
    // so end it here rather than leave the user on a page whose every request
    // 401s until something else notices.
    if stored.refresh_token.is_none() {
        if stored.expires_at <= now {
            end_oidc_session(auth, "the stored OIDC session carries no refresh token");
        }
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
        Err(e) if renewal_is_unrecoverable(&e) => end_oidc_session(auth, &e),
        // MAPPS-645: a wake-triggered tick runs the moment the tab is
        // foregrounded, which is where a network-shaped failure is most
        // likely and least meaningful. Keep the session; the next tick (or
        // the 401 handler, which ends the session itself) decides.
        Err(e) => {
            tracing::warn!("token refresh failed transiently; keeping the session: {e}");
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
/// and the fetch access-token holder. A refusal that says the grant is dead
/// clears the session and sends the browser to /login, matching the OIDC error
/// branch and the MAPPS-368 standalone-401 clear; a transient failure keeps the
/// session and retries (MAPPS-645).
pub fn use_standalone_token_refresh() {
    let mut auth = use_auth();

    use_future(move || async move {
        crate::platform::dom::watch_visibility();
        loop {
            standalone_refresh_tick(&mut auth).await;
            // MAPPS-435: sleep LAST, for the reason given on the OIDC loop.
            #[cfg(feature = "app")]
            sleep_or_wake(POLL_INTERVAL_MS).await;
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
    // Refresh window: 60s before expiry. If a backgrounded tab sailed past it
    // (throttled timers), refresh now - same policy as the OIDC loop above.
    let now = chrono::Utc::now();
    if session.expires_at - now > chrono::Duration::seconds(60) {
        return;
    }

    // MAPPS-645: a standalone session with no refresh token cannot be renewed.
    // Same policy as the OIDC tick above: nothing to do while the access token
    // is still good, sign out once it is spent.
    if session.refresh_token.is_none() {
        if session.expires_at <= now {
            end_standalone_session(
                auth,
                "the stored standalone session carries no refresh token",
            );
        }
        return;
    }

    // Shared single-flight renewal (MAPPS-435); it persists the rotated
    // session and swaps the fetch layer's bearer.
    match crate::hooks::fetch::api::renew_access_token().await {
        Ok(()) => {}
        Err(e) if renewal_is_unrecoverable(&e) => end_standalone_session(auth, &e),
        // MAPPS-645: transient, for the reason given on the OIDC tick.
        Err(e) => {
            tracing::warn!("standalone token refresh failed transiently; keeping the session: {e}");
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
///
/// MAPPS-645: a tab returning to the foreground fires the beat immediately
/// rather than waiting out the sleep it was in the middle of. That request
/// goes through `get_authed_typed`, so it renews a spent bearer on the way
/// out and the generation bump re-drives every mounted resource with it.
pub fn use_auth_heartbeat() {
    use_future(move || async move {
        crate::platform::dom::watch_visibility();
        loop {
            heartbeat_tick().await;
            // MAPPS-435: sleep LAST, for the reason given on the OIDC loop.
            #[cfg(feature = "app")]
            sleep_or_wake(POLL_INTERVAL_MS).await;
        }
    });
}

/// One heartbeat evaluation. Returns without firing when the overlay is
/// already up, nobody is signed in, or the tab is in the background.
async fn heartbeat_tick() {
    #[cfg(feature = "app")]
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

/// True when the app is out of sight, so the heartbeat above skips its
/// request. In the browser that is `document.visibilityState`; a desktop
/// window always reports itself visible (see
/// [`crate::platform::dom::window_hidden`]). Non-`app` builds report
/// visible too, so the call site type-checks under `cargo check` without
/// the `app` feature.
#[cfg(feature = "app")]
fn tab_is_hidden() -> bool {
    crate::platform::dom::window_hidden()
}

#[cfg(not(feature = "app"))]
#[allow(dead_code)]
fn tab_is_hidden() -> bool {
    false
}

/// Put a signed-out user back on the login screen.
///
/// Every caller has already cleared `AuthContext` and the persisted
/// session, so the route guard would land them there on the next render
/// regardless. In a browser the hard navigation is still worth doing: it
/// reloads the document and drops every other piece of in-memory state
/// with it. MAPPS-504: a desktop window has no document to replace, and
/// the cleared context is what moves it.
fn redirect_to_login() {
    // MAPPS-518 URL swap: tenant login lives at /client/login now; /login is
    // the platform-admin page and would be the wrong destination for a
    // forced tenant sign-out.
    #[cfg(target_arch = "wasm32")]
    if let Err(e) = crate::platform::location::set_href("/client/login") {
        tracing::warn!("redirect to /client/login after sign-out failed: {e}");
    }
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
        // PMS-791 / MAPPS-462: owning tenant's `kind` column
        // ("org" | "personal"). Empty string on older server responses
        // that predate the field; AuthState::is_org_tenant() treats
        // empty as org (fail-open UI).
        #[serde(default)]
        tenant_kind: String,
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
        // PMS-791 / MAPPS-462: reconcile tenant_kind from /me so Teams
        // nav visibility flips off within a tick on personal tenants.
        u.tenant_kind = me.tenant_kind;
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
///
/// MAPPS-504: browser-only by nature. There is no back-forward cache on
/// the desktop because there is no navigation away from the document to
/// be restored from.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
pub fn use_bfcache_invalidator() {
    use wasm_bindgen::JsCast;

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
                crate::platform::location::reload();
            }
        })
            as Box<dyn FnMut(web_sys::Event)>);
        let _ = win.add_event_listener_with_callback("pageshow", handler.as_ref().unchecked_ref());
        // Listener must outlive its registration; SPA root, fires once.
        handler.forget();
    });
}

#[cfg(any(not(feature = "app"), not(target_arch = "wasm32")))]
pub fn use_bfcache_invalidator() {}

/// MAPPS-504: watch [`crate::hooks::fetch::SESSION_ENDED`] and clear the
/// auth context when the fetch layer ends a session from outside the
/// component tree.
///
/// In a browser that path finishes with `location.set_href("/login")`,
/// and the reload wipes `AuthContext` on the way. A desktop window has
/// no reload, so without this the user would keep looking at a populated
/// dashboard whose every request 401s. Clearing the context is enough:
/// the route guard reads it and moves them to the login screen.
///
/// Mounted once at the app root, alongside the refresh loops.
#[cfg(all(feature = "app", not(target_arch = "wasm32")))]
pub fn use_session_end_watch() {
    let mut auth = use_auth();
    use_effect(move || {
        if !*crate::hooks::fetch::SESSION_ENDED.read() {
            return;
        }
        {
            let mut a = auth.write();
            a.user = None;
            a.tokens = None;
        }
        // One-shot: lower it so a later sign-in is not torn down by a
        // flag left raised from the previous session.
        *crate::hooks::fetch::SESSION_ENDED.write() = false;
    });
}

/// The browser reloads instead, which is what clears the context there.
#[cfg(any(not(feature = "app"), target_arch = "wasm32"))]
pub fn use_session_end_watch() {}

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

    /// MAPPS-571: a context holding one membership, the shape the loader
    /// produces on a successful read.
    fn with_org(name: &str) -> (super::AuthContext, uuid::Uuid) {
        let id = uuid::Uuid::from_u128(0x0dd1);
        let ctx = super::AuthContext {
            active_tenant_id: Some(id),
            memberships: vec![super::MembershipView {
                tenant_id: id.to_string(),
                tenant_name: name.to_string(),
            }],
            memberships_loaded: true,
            ..Default::default()
        };
        (ctx, id)
    }

    #[test]
    fn a_rename_updates_the_cached_org_name() {
        let (mut ctx, _) = with_org("My workspace");
        assert!(ctx.set_active_org_name("Niceguy IT"));
        assert_eq!(ctx.active_org_name(), Some("Niceguy IT"));
    }

    #[test]
    fn a_rename_with_no_membership_does_not_invent_one() {
        // MAPPS-427 leaves the list empty when the org load failed, on purpose:
        // no name beats a wrong one. A rename must not be the back door that
        // reintroduces a fabricated row, because the name it would carry comes
        // from a form field rather than from the row the server holds.
        let mut ctx = super::AuthContext {
            active_tenant_id: Some(uuid::Uuid::from_u128(0x0dd1)),
            memberships_loaded: true,
            ..Default::default()
        };
        assert!(!ctx.set_active_org_name("Niceguy IT"));
        assert!(ctx.memberships.is_empty());
        assert_eq!(ctx.active_org_name(), None);
    }

    #[test]
    fn a_rename_before_the_active_tenant_is_known_changes_nothing() {
        let (mut ctx, _) = with_org("My workspace");
        ctx.active_tenant_id = None;
        assert!(!ctx.set_active_org_name("Niceguy IT"));
        assert_eq!(ctx.memberships[0].tenant_name, "My workspace");
    }

    #[test]
    fn a_rename_never_touches_a_row_for_another_tenant() {
        let (mut ctx, _) = with_org("My workspace");
        ctx.memberships.push(super::MembershipView {
            tenant_id: uuid::Uuid::from_u128(0xbeef).to_string(),
            tenant_name: "Someone else".to_string(),
        });
        assert!(ctx.set_active_org_name("Niceguy IT"));
        assert_eq!(ctx.memberships[0].tenant_name, "Niceguy IT");
        assert_eq!(ctx.memberships[1].tenant_name, "Someone else");
    }

    #[test]
    fn an_empty_name_leaves_the_previous_one_standing() {
        // The server enforces `1..=255`, so an empty name in a 2xx response
        // means the body was not what it claimed. Blanking the top bar on the
        // strength of that is worse than keeping the name that was there.
        let (mut ctx, _) = with_org("Niceguy IT");
        assert!(!ctx.set_active_org_name(""));
        assert!(!ctx.set_active_org_name("   "));
        assert_eq!(ctx.active_org_name(), Some("Niceguy IT"));
    }

    #[test]
    fn a_stored_name_is_cached_without_its_surrounding_space() {
        let (mut ctx, _) = with_org("My workspace");
        assert!(ctx.set_active_org_name("  Niceguy IT  "));
        assert_eq!(ctx.active_org_name(), Some("Niceguy IT"));
    }

    /// MAPPS-571 recurrence guard, in the same shape as the MAPPS-427 one
    /// above: both writers of `tenants.name` live in pages that need a browser,
    /// so what is pinned here is the thing this file owns.
    #[test]
    fn a_rename_refreshes_the_cache_rather_than_re_running_the_loader() {
        let src = production_src();
        assert!(
            src.contains("pub fn set_active_org_name"),
            "the writers of tenants.name need a way to refresh the cached name"
        );
        // `memberships_loaded` is cleared in exactly one place: the `/me`
        // reconcile, which re-runs the loader once the session is confirmed.
        // A second site would almost certainly be a rename trying to refresh
        // itself by making the effect fire again, which spends a round-trip on
        // a value the PUT response already carried and re-opens the
        // retry-on-failure behaviour MAPPS-427 closed.
        assert_eq!(
            src.matches("memberships_loaded = false").count(),
            1,
            "the org loader's guard flag is cleared by the /me reconcile and nothing else"
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
                !first.contains("sleep_ms") && !first.starts_with("#[cfg"),
                "this loop sleeps before it evaluates anything: {first}"
            );
        }
        assert_eq!(loops, 3, "this file runs three polling loops");
    }

    /// MAPPS-645 recurrence guard, the wake half of the one above.
    ///
    /// Evaluating before the sleep only helps a tab that started over. A tab
    /// the browser suspended and resumed keeps its loops, so it is mid-sleep
    /// when its pages re-fetch against a token that expired while it was
    /// hidden, and the page it lands on shows a dead-end error until the rest
    /// of that sleep runs out. Every loop therefore waits on the wake-aware
    /// sleep, and the raw timer is reached from exactly one place.
    #[test]
    fn the_auth_loops_wake_when_the_app_returns_to_the_foreground() {
        let src = production_src();
        assert_eq!(
            src.matches("sleep_or_wake(POLL_INTERVAL_MS).await").count(),
            3,
            "all three polling loops must wait on the wake-aware sleep"
        );
        assert_eq!(
            src.matches("crate::platform::timer::sleep_ms").count(),
            1,
            "the raw timer belongs to sleep_or_wake alone; a second caller is a loop that cannot be woken"
        );
        assert_eq!(
            src.matches("crate::platform::dom::watch_visibility()")
                .count(),
            3,
            "each loop installs the visibility listener, so one mounted alone still wakes"
        );
    }

    /// MAPPS-645: the wake has to actually cut the sleep short. Racing a
    /// 30-second timer and returning immediately is the whole behaviour; if
    /// the wake future never resolves this test takes 30 seconds instead of
    /// no time at all.
    #[cfg(feature = "app")]
    #[test]
    fn a_return_to_the_foreground_cuts_the_poll_sleep_short() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a tokio runtime to drive the sleep on");
        let started = std::time::Instant::now();
        rt.block_on(async {
            let waiting = super::sleep_or_wake(super::POLL_INTERVAL_MS);
            let waking = async {
                // One yield so `waiting` is polled and parked before the wake
                // lands; a wake nobody is waiting on is not what is under test.
                tokio::task::yield_now().await;
                crate::platform::dom::notify_visible();
            };
            futures_util::future::join(waiting, waking).await;
        });
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the sleep waited out its interval instead of waking: {:?}",
            started.elapsed()
        );
    }

    /// MAPPS-645: a wake fires a tick the instant the tab is foregrounded,
    /// which is exactly when the network is least likely to be back. Only a
    /// refusal that says the grant is dead may sign the user out.
    #[test]
    fn only_a_dead_grant_signs_the_user_out() {
        for dead in [
            "the stored OIDC session carries no refresh token",
            "the stored standalone session carries no refresh token",
            "no persisted session to renew",
            "token endpoint: invalid_grant (refresh token expired)",
            "token endpoint: invalid_client ()",
            "http 401: token has expired",
            "http 400: invalid refresh token",
        ] {
            assert!(
                super::renewal_is_unrecoverable(dead),
                "{dead:?} means the session is over"
            );
        }
    }

    #[test]
    fn a_transient_renewal_failure_leaves_the_session_alone() {
        for transient in [
            "network: error sending request",
            "network: token body: unexpected end of input",
            "token endpoint: token_endpoint_failed ()",
            "token endpoint: temporarily_unavailable ()",
            "http 500: internal server error",
            "http 502",
            "network error: connection refused",
            "decode error: missing field `access_token`",
            "something nobody has seen before",
        ] {
            assert!(
                !super::renewal_is_unrecoverable(transient),
                "{transient:?} is worth another tick, not a sign-out"
            );
        }
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

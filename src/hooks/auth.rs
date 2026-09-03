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

/// MAPPS-661: whether the identity provider has confirmed, during THIS page
/// lifetime, that the session in the context still exists.
///
/// The persisted bundle is a cache, never an assertion of identity. A tab the
/// browser unloaded and restored rebuilds `AuthContext` from `sessionStorage`
/// (see [`rehydrate_from_storage`]), and that bundle says nothing about
/// whether the session behind it ended while the tab was gone. Holding the two
/// facts apart is what lets the shell wait for the answer instead of asserting
/// one it never asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionConfirmation {
    /// Nothing is outstanding: nobody is signed in, this page lifetime
    /// performed the sign-in itself, or a rotation / `/api/v1/auth/me` has
    /// since answered for the session.
    #[default]
    Confirmed,
    /// Rebuilt from the persisted bundle and not yet checked against the OP.
    Unconfirmed,
    /// The check came back negative. The persisted bundle has been cleared and
    /// the user belongs on the signed-out screen.
    Ended,
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
    /// MAPPS-661: has the OP confirmed this session in this page lifetime?
    ///
    /// [`Self::is_authenticated`] answers "we hold a token bundle", which a
    /// restored tab answers yes to for a session that ended while it was
    /// unloaded. This is the narrower fact, and it is what the mount-time
    /// confirmation ([`confirm_restored_session`]) resolves.
    pub confirmation: SessionConfirmation,
}

impl AuthContext {
    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    /// MAPPS-661: may the signed-in shell render right now?
    ///
    /// Holding a bundle is not the same as having a session, so a restored tab
    /// whose access token is spent waits (`is_loading`) for the confirmation
    /// rather than presenting a dashboard that is about to disappear. The
    /// render site is `AuthGuard` (`src/lib.rs`), whose `is_loading` branch
    /// shows the loading state before it ever reads
    /// [`Self::is_authenticated`]; this names the same rule so it can be
    /// asserted without a browser.
    pub fn may_render_signed_in(&self) -> bool {
        self.is_authenticated() && !self.is_loading
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

/// Provide authentication context to the application.
///
/// MAPPS-661: also runs the mount-time session confirmation, because the
/// context this provides may have been rebuilt from a persisted bundle that
/// outlived the session it describes. Confirming here rather than from a
/// polling loop is the point: a tab the browser discarded and re-created must
/// get its answer at mount, not up to 30 seconds later, and it holds the
/// signed-in shell back until it has one.
pub fn use_auth_provider() -> Signal<AuthContext> {
    let auth = use_signal(initial_auth_context);
    use_context_provider(|| auth);
    let mut confirming = auth;
    use_future(move || async move {
        confirm_restored_session(&mut confirming).await;
    });
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
            // MAPPS-661: this session came from compile-time env, not from a
            // persisted bundle, so there is no OP answer to wait for.
            confirmation: SessionConfirmation::Confirmed,
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
///
/// MAPPS-661: the context this builds is [`SessionConfirmation::Unconfirmed`].
/// The bundle is a cache; nothing here has asked the OP whether the session it
/// describes still exists, and a tab restored after the session ended would
/// otherwise render a dashboard that is about to evict the user.
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
    // MAPPS-661: a bundle whose access token is spent (or nearly) is the
    // discarded-tab case, and nothing signed-in renders until the confirmation
    // answers.
    let awaiting_confirmation = confirmation_gates_render(tokens.expires_at, chrono::Utc::now());
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
        // MAPPS-661: the app's existing loading state IS the render gate.
        // `AuthGuard` shows it before it reads `is_authenticated`, so a
        // restored tab with a spent token waits for the answer instead of
        // presenting a shell it has not confirmed.
        is_loading: awaiting_confirmation,
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
        // MAPPS-661: a bundle, not a session. `confirm_restored_session`
        // resolves it at mount.
        confirmation: SessionConfirmation::Unconfirmed,
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
    // MAPPS-661: same gate as the OIDC path above, for the same reason.
    let awaiting_confirmation = confirmation_gates_render(stored.expires_at, chrono::Utc::now());
    crate::hooks::fetch::api::set_access_token(Some(stored.access_token.clone()));
    Some(AuthContext {
        user: Some(stored.user),
        is_loading: awaiting_confirmation,
        error: None,
        // No OIDC tokens in a standalone session; the refresh hook no-ops.
        tokens: None,
        active_tenant_id,
        memberships: Vec::new(),
        memberships_loaded: false,
        // /me reconciles the authoritative user on the next tick.
        server_loaded: false,
        // MAPPS-661: a persisted session is a cache here too.
        confirmation: SessionConfirmation::Unconfirmed,
    })
}

/// MAPPS-661: seconds of remaining access-token life below which a restored
/// session has to be confirmed before anything signed-in renders.
///
/// Mirrors the fetch layer's renewal window deliberately: inside it the very
/// next request renews anyway, so waiting for the answer costs nothing extra,
/// and outside it the token is short-lived (600s on the client row) and was
/// issued by the OP moments ago, which is confirmation enough to render while
/// `/api/v1/auth/me` catches up. That is what keeps ordinary navigation free
/// of a spinner.
const CONFIRMATION_WINDOW_SECS: i64 = 60;

/// Does a restored bundle expiring at `expires_at` have to be confirmed before
/// the signed-in shell may render?
fn confirmation_gates_render(
    expires_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    expires_at - now <= chrono::Duration::seconds(CONFIRMATION_WINDOW_SECS)
}

/// MAPPS-661: ask the identity provider, once and at mount, whether the
/// session rebuilt from the persisted bundle still exists.
///
/// Runs from [`use_auth_provider`], so it starts with the app rather than on a
/// 30-second loop tick: a tab the browser discarded and re-created is exactly
/// the case the loops are too late for, and the shell it would otherwise
/// render is the bug this closes.
///
/// Which question gets asked depends on the bundle:
///
/// - Access token spent or inside the renewal window: the rotation IS the
///   question, and the OP refusing it (`invalid_grant`) is the authoritative
///   "this session is over". It goes through
///   [`crate::hooks::fetch::api::renew_access_token`], the same single flight
///   the request path uses (MAPPS-435), so a confirmation that coincides with
///   a page fetch shares one flight instead of racing it into the OP's
///   refresh-token reuse detection.
/// - Access token still comfortably valid: the shell renders, and the answer
///   comes from the `GET /api/v1/auth/me` that [`use_current_user_loader`]
///   already fires on this same mount. [`refresh_user_from_me`] records the
///   outcome, and a 401 there ends the session through the fetch layer. Asking
///   again from here would spend a second round-trip on an answer already in
///   flight.
async fn confirm_restored_session(auth: &mut Signal<AuthContext>) {
    if auth.peek().confirmation != SessionConfirmation::Unconfirmed {
        return;
    }
    let Some(expires_at) = crate::hooks::fetch::api::persisted_expiry() else {
        // Rehydrated from a bundle the session store no longer returns (it was
        // cleared between boot and now, or the store is unreadable). There is
        // no question left to ask, so release the gate rather than hold the
        // user on a loading screen forever; the first request to 401 ends the
        // session through the fetch layer.
        release_render_gate(auth, "there is no persisted session to confirm");
        return;
    };
    if !confirmation_gates_render(expires_at, chrono::Utc::now()) {
        return;
    }
    // Which session kind this is decides which store the failure branch must
    // clear. `renew_access_token` prefers the OIDC bundle for the same reason.
    let standalone = crate::modules::oidc::storage::load_auth().is_none();
    match crate::hooks::fetch::api::renew_access_token().await {
        Ok(()) => {
            // A standalone session keeps `tokens: None` by design (there is no
            // OIDC bundle to mirror), so only the OIDC path has a copy to
            // refresh.
            if !standalone {
                mirror_rotated_bundle(auth);
            }
            mark_session_confirmed(auth);
        }
        Err(e) if renewal_is_unrecoverable(&e) => {
            if standalone {
                end_standalone_session(auth, &e);
            } else {
                end_oidc_session(auth, &e);
            }
        }
        // Same policy as the loops (MAPPS-645): a network-shaped failure is not
        // evidence the session ended, and signing the user out over a blip is
        // worse than the one 401 that follows.
        Err(e) => release_render_gate(auth, &e),
    }
}

/// Record that the OP has answered for this session in this page lifetime, and
/// release the render gate the unconfirmed state was holding.
fn mark_session_confirmed(auth: &mut Signal<AuthContext>) {
    let mut a = auth.write();
    a.confirmation = SessionConfirmation::Confirmed;
    a.is_loading = false;
}

/// Let the shell render a session that could NOT be confirmed, because the
/// question could not be put rather than because it was answered.
///
/// Deliberate suppression: the cause is logged, the context stays
/// [`SessionConfirmation::Unconfirmed`] so nothing downstream reads this as an
/// answer, and the refresh loops plus the fetch layer's 401 handling still end
/// a session that really is over. Holding the loading screen instead would
/// strand a live session behind one failed request.
fn release_render_gate(auth: &mut Signal<AuthContext>, cause: &str) {
    tracing::warn!("could not confirm the restored session, rendering it anyway: {cause}");
    auth.write().is_loading = false;
}

/// Copy the freshly rotated bundle out of the session store and into the
/// context, so the context's copy does not age out behind the store.
///
/// The store is the source of truth: the renewal wrote there first (MAPPS-435),
/// and replaying the superseded copy the context still holds is what the OP's
/// reuse detection answers by killing the grant.
///
/// Only for OIDC sessions; a standalone one holds no `tokens` to mirror.
fn mirror_rotated_bundle(auth: &mut Signal<AuthContext>) {
    match crate::modules::oidc::storage::load_auth() {
        Some(fresh) => {
            auth.write().tokens = Some(Tokens {
                access_token: fresh.access_token,
                id_token: fresh.id_token,
                refresh_token: fresh.refresh_token,
                expires_at: fresh.expires_at,
                scope: fresh.scope,
            });
        }
        // The rotation that just succeeded wrote the bundle here, so an empty
        // store means something cleared it underneath us (a sign-out in
        // flight, a store that stopped answering). Leaving the context's copy
        // in place is the safe half; saying nothing about it is how a context
        // silently ages out behind the store, so it is logged.
        None => tracing::warn!(
            "the session store held no bundle right after a successful rotation; \
             the context keeps the copy it had"
        ),
    }
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
        // MAPPS-661: the answer came back, and it was no. Release the render
        // gate with it so the mount path lands on the signed-out state rather
        // than on a loading screen nothing will ever clear.
        a.confirmation = SessionConfirmation::Ended;
        a.is_loading = false;
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
        // MAPPS-661: see the note on [`end_oidc_session`].
        a.confirmation = SessionConfirmation::Ended;
        a.is_loading = false;
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
            mirror_rotated_bundle(auth);
            // MAPPS-661: the OP honoured the grant, so the session it belongs
            // to exists. That is the confirmation, independently of whether
            // the /me below lands.
            mark_session_confirmed(auth);

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
        // MAPPS-661: the server honoured the refresh token, so the session
        // behind it exists.
        Ok(()) => mark_session_confirmed(auth),
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
    #[cfg(target_arch = "wasm32")]
    if let Err(e) = crate::platform::location::set_href("/login") {
        tracing::warn!("redirect to /login after sign-out failed: {e}");
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
                // MAPPS-661: the session is over, and this ran at mount as
                // readily as from a loop tick. `default()` is the signed-out
                // context, so `is_loading` goes false with it and AuthGuard
                // routes to the login screen rather than holding the loading
                // state. Recorded as Ended so nothing reads the cleared
                // context as merely "not confirmed yet".
                *auth.write() = AuthContext {
                    confirmation: SessionConfirmation::Ended,
                    ..AuthContext::default()
                };
            }
            return;
        }
        Err(e) => {
            // MAPPS-661: not an answer, so the confirmation state is left
            // alone; a restored context stays Unconfirmed and the loops (or
            // the next 401) decide. The render gate is released here because
            // one failed /me must not strand a live session on a spinner.
            tracing::warn!("/api/v1/auth/me failed; keeping cached user: {e:?}");
            auth.write().is_loading = false;
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
    // MAPPS-661: a 200 from /me is the OP-backed answer that this session
    // exists (mokosh-server verifies the `at+jwt` before it replies), so it is
    // what confirms a context rebuilt from the persisted bundle. This is the
    // confirmation for a mount whose access token was still comfortably
    // valid; the spent-token case is renewed by `confirm_restored_session`.
    a.confirmation = SessionConfirmation::Confirmed;
    a.is_loading = false;
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
///
/// MAPPS-661: this is also the confirmation for a restored tab whose access
/// token is still comfortably inside its lifetime. It fires at mount, its
/// bearer is renewed on the way out by the fetch layer's single flight if it
/// turns out to be spent, and [`refresh_user_from_me`] records the answer.
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
            // MAPPS-661: the fetch layer ended the session, which is an
            // answer. Release the render gate with it so a restored window
            // cannot sit on the loading state instead of the login screen.
            a.confirmation = SessionConfirmation::Ended;
            a.is_loading = false;
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

    /// MAPPS-661: a context rebuilt from the persisted bundle, as the two
    /// rehydrate paths produce it. `expires_in` is the access token's
    /// remaining life at the moment of the rebuild.
    fn restored(expires_in: chrono::Duration) -> super::AuthContext {
        let now = chrono::Utc::now();
        super::AuthContext {
            user: Some(a_user()),
            is_loading: super::confirmation_gates_render(now + expires_in, now),
            confirmation: super::SessionConfirmation::Unconfirmed,
            ..Default::default()
        }
    }

    fn a_user() -> crate::modules::auth::CurrentUser {
        crate::modules::auth::CurrentUser {
            id: uuid::Uuid::from_u128(0xf00d),
            tenant_id: uuid::Uuid::from_u128(0x0dd1),
            email: "someone@example.com".to_string(),
            first_name: String::new(),
            last_name: String::new(),
            role: crate::modules::auth::UserRole::default(),
            timezone: "UTC".to_string(),
            avatar_url: None,
            profile_completed: true,
            date_format_string: None,
            theme_base_mode: None,
            theme_accent_id: None,
            own_company_id: None,
        }
    }

    /// MAPPS-661 recurrence guard, in the shape of the MAPPS-435 and MAPPS-522
    /// ones: the persisted bundle is a cache, and a tab the browser unloaded
    /// and restored may not present a signed-in shell on the strength of it.
    /// The reported symptom was exactly that - a restored tab showing the
    /// dashboard for a session that had already ended, then evicting the user
    /// out from under themselves ten seconds later.
    #[test]
    fn a_restored_session_does_not_render_before_it_is_confirmed() {
        let ctx = restored(chrono::Duration::seconds(-1));
        assert_eq!(ctx.confirmation, super::SessionConfirmation::Unconfirmed);
        assert!(
            !ctx.may_render_signed_in(),
            "a restored context with a spent access token must wait for the confirmation"
        );
        // Inside the renewal window is the same case: the next request renews
        // anyway, so there is nothing to gain by rendering first.
        assert!(!restored(chrono::Duration::seconds(30)).may_render_signed_in());
        // And the shape above is the shape the rehydrates actually build, so
        // this cannot pass on a context only the test constructs.
        assert_eq!(
            production_src()
                .matches("is_loading: awaiting_confirmation")
                .count(),
            2,
            "both rehydrate paths must hold the render back on a spent token"
        );
    }

    /// The other half of the rule: confirmation must not cost a spinner on
    /// every navigation. A token comfortably inside its 600s lifetime was
    /// issued by the OP moments ago, so the shell renders while
    /// `/api/v1/auth/me` catches up.
    #[test]
    fn a_restored_session_with_a_live_token_renders_immediately() {
        let ctx = restored(chrono::Duration::seconds(500));
        assert_eq!(ctx.confirmation, super::SessionConfirmation::Unconfirmed);
        assert!(ctx.may_render_signed_in());
    }

    #[test]
    fn only_a_spent_token_gates_the_render() {
        let now = chrono::Utc::now();
        for (remaining, gated) in [(-600, true), (-1, true), (0, true), (59, true), (61, false)] {
            assert_eq!(
                super::confirmation_gates_render(now + chrono::Duration::seconds(remaining), now),
                gated,
                "{remaining}s of remaining access-token life"
            );
        }
    }

    /// MAPPS-661: both rehydrate paths produce the unconfirmed state, and the
    /// state only moves on an OP-backed answer. A source scan because the
    /// rehydrates read the session store and the transitions run inside
    /// futures that need a browser; what is pinned is which sites may decide.
    #[test]
    fn only_the_identity_provider_confirms_a_session() {
        let src = production_src();
        assert_eq!(
            src.matches("confirmation: SessionConfirmation::Unconfirmed")
                .count(),
            2,
            "both rehydrate paths must produce the unconfirmed state"
        );
        // The renewal and the /me reconcile, and nothing else.
        assert_eq!(
            src.matches("fn mark_session_confirmed").count(),
            1,
            "one helper owns the move to confirmed"
        );
        assert_eq!(
            src.matches("SessionConfirmation::Confirmed").count(),
            3,
            "confirmed is set by mark_session_confirmed, the /me reconcile, and the dev bypass"
        );
    }

    /// MAPPS-661: the confirmation runs at mount, not on a loop tick, and
    /// spends the renewal already single-flighted by the fetch layer
    /// (MAPPS-435) rather than opening a second one against the same refresh
    /// token - which is what the OP's reuse detection answers by killing the
    /// grant.
    #[test]
    fn the_mount_confirmation_shares_the_single_flight_renewal() {
        let src = production_src();
        let provider = src
            .split_once("pub fn use_auth_provider()")
            .map(|(_, after)| after)
            .expect("the auth provider is what mounts the confirmation");
        assert!(
            provider.contains("confirm_restored_session(&mut confirming).await"),
            "the confirmation must start with the app, not with a poll tick"
        );
        let confirmation = src
            .split_once("async fn confirm_restored_session")
            .map(|(_, after)| after)
            .expect("this file owns the mount confirmation");
        assert!(
            confirmation.contains("crate::hooks::fetch::api::renew_access_token().await"),
            "the shared renewal is the vehicle; a second flight is a reuse detection"
        );
        assert!(
            !confirmation.contains("refresh_tokens("),
            "reaching past the fetch layer would race the request path's renewal"
        );
    }

    /// MAPPS-661: `AuthGuard` is the render site, and its loading branch is
    /// what holds an unconfirmed restore back. It sits in `src/lib.rs`, so
    /// this pins the two properties that make `is_loading` the gate: the
    /// branch exists, and it is reached BEFORE the authenticated one.
    #[test]
    fn the_route_guard_shows_the_loading_state_before_it_reads_the_session() {
        const LIB_SRC: &str = include_str!("../lib.rs");
        let guard = LIB_SRC
            .split_once("pub fn AuthGuard()")
            .map(|(_, after)| after)
            .expect("the router's authenticated layout");
        let loading = guard
            .find("if auth_state.is_loading {")
            .expect("AuthGuard must render the loading state while a session is unconfirmed");
        let authenticated = guard
            .find("if !auth_state.is_authenticated() {")
            .expect("AuthGuard must still route a signed-out user away");
        assert!(
            loading < authenticated,
            "the loading branch has to come first, or an unconfirmed restore renders the shell"
        );
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

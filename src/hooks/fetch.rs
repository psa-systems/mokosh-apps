//! Data fetching hooks

use dioxus::prelude::*;

/// Monotonic "active tenant / token generation" counter.
///
/// The access token itself lives in a thread-local (`ACCESS_TOKEN`),
/// which Dioxus cannot track as a reactive dependency: a `use_resource`
/// closure that only reads `current_access_token()` never re-runs when
/// the token is swapped on an org switch, so the page keeps rendering
/// the previous tenant's cached data (Phase-4 F1).
///
/// This `GlobalSignal` is bumped every time the token is set (login,
/// refresh, org switch, logout). List/detail pages read it inside their
/// `use_resource` closure (see [`active_tenant_generation`]) so Dioxus
/// records it as a dependency and re-fetches whenever the active tenant
/// changes. WASM is single-threaded, so a `GlobalSignal` is the right
/// primitive here (same rationale as the toast surface).
#[cfg(feature = "app")]
pub static TENANT_GENERATION: GlobalSignal<u64> = Signal::global(|| 0);

/// Read the active-tenant generation counter. Call this INSIDE a
/// `use_resource` closure (before any early return) so Dioxus subscribes
/// the resource to it and re-fetches on the next org switch / token swap.
/// The returned value is otherwise unused; it exists purely to register
/// the reactive dependency.
#[cfg(feature = "app")]
pub fn active_tenant_generation() -> u64 {
    *TENANT_GENERATION.read()
}

/// Non-web stub so the same call site compiles under `cargo check`
/// without the `app` feature.
#[cfg(not(feature = "app"))]
pub fn active_tenant_generation() -> u64 {
    0
}

/// MAPPS-635 F: per-contact portal-role assignment generation. Bumped
/// on every successful "Update roles" / "Grant + send email" write so
/// every `use_resource` that reads it re-fetches, matching the
/// [`TENANT_GENERATION`] pattern. Fixes the report that the Portal
/// Access card's role badges kept showing the old set after a
/// successful role edit - `ContactRoleBadges` fetched its own list
/// via `use_resource` with no dep on the write path, so nothing
/// forced a refetch.
#[cfg(feature = "web")]
pub static PORTAL_ROLES_GENERATION: GlobalSignal<u64> = Signal::global(|| 0);

/// Read the portal-roles generation counter. Same shape as
/// [`active_tenant_generation`]: call INSIDE a `use_resource` closure
/// so Dioxus subscribes the resource to it.
#[cfg(feature = "web")]
pub fn active_portal_roles_generation() -> u64 {
    *PORTAL_ROLES_GENERATION.read()
}

/// Bump the portal-roles generation counter. Call from every
/// successful role-write handler (grant + role-edit + revoke).
#[cfg(feature = "web")]
pub fn bump_portal_roles_generation() {
    *PORTAL_ROLES_GENERATION.write() += 1;
}

#[cfg(not(feature = "web"))]
pub fn active_portal_roles_generation() -> u64 {
    0
}

#[cfg(not(feature = "web"))]
pub fn bump_portal_roles_generation() {}

/// App-wide "is mokosh-server reachable" flag (MAPPS-333). `true`
/// (reachable) on boot; flipped to `false` by the classification helpers
/// below when a request fails with a "down" condition, and back to
/// `true` on the first response that proves the server is answering.
///
/// A `GlobalSignal` (not a context `Signal`) for the same reason
/// [`TENANT_GENERATION`] is one: the `api` helpers below are plain async
/// fns, not components, so they cannot reach a context-provided signal.
/// WASM is single-threaded, so a `GlobalSignal` is the right primitive
/// (same rationale as the toast surface). The banner reads it via
/// [`crate::hooks::use_server_reachable`] and the recovery poll lives in
/// [`crate::hooks::server_status`].
#[cfg(feature = "app")]
pub static SERVER_REACHABLE: GlobalSignal<bool> = Signal::global(|| true);

/// MAPPS-348: sticky "the current user's bunyip account was deleted" flag.
/// Flipped to `true` the moment ANY fetch on the SPA sees a `410 Gone`
/// response carrying `error.code == "ACCOUNT_DELETED"` (the terminal
/// signal mokosh-server emits from every auth extractor when
/// `users.deleted_at` is set - see mokosh-server MAPPS-348). Never
/// flipped back to `false`: this is a one-way transition, the same
/// deletion the Bunyip webhook triggered on the server. The AppLayout
/// reads it via [`crate::hooks::use_account_deleted`] and renders the
/// terminal modal + logout countdown; the fetch layer STOPS bubbling
/// the 4xx through the per-page error surface once the signal has
/// flipped, so a burst of 410s does not spam toasts on top of the
/// modal.
///
/// GlobalSignal (not context) for the same reason [`SERVER_REACHABLE`]
/// is: the `api` helpers are plain async fns and can't reach a
/// context-provided signal.
#[cfg(feature = "app")]
pub static ACCOUNT_DELETED: GlobalSignal<bool> = Signal::global(|| false);

/// Flip [`ACCOUNT_DELETED`] to `true`. Idempotent (called from every
/// fetch on the error path once the account is tombstoned, but only
/// writes to the signal on the first observation) so a burst of
/// concurrent 410s does not wake readers repeatedly.
#[cfg(feature = "app")]
pub(crate) fn note_account_deleted() {
    if !*ACCOUNT_DELETED.peek() {
        *ACCOUNT_DELETED.write() = true;
    }
}

/// Force every `use_resource` that reads [`active_tenant_generation`] to
/// re-fetch, without changing the token.
///
/// MAPPS-504: the browser gets this for free from a page reload. The
/// desktop build has no reload, so the one caller that told the user to
/// reload after replacing all their data (the import panel in
/// `pages::settings`) drives the refetch directly.
#[cfg(feature = "app")]
pub fn bump_tenant_generation() {
    *TENANT_GENERATION.write() += 1;
}

/// MAPPS-504: raised when the fetch layer has ended the session and the
/// user has to be put back on the login screen.
///
/// The browser does that with a full page reload, which resets every
/// in-memory signal on the way. A desktop window cannot reload, so it
/// needs something a component can observe:
/// [`crate::hooks::auth::use_session_end_watch`] clears `AuthContext`
/// when this flips, and the route guard takes it from there.
///
/// A `GlobalSignal` for the same reason [`ACCOUNT_DELETED`] is one: the
/// `api` helpers are plain async fns and cannot reach a
/// context-provided signal.
#[cfg(feature = "app")]
pub static SESSION_ENDED: GlobalSignal<bool> = Signal::global(|| false);

/// Raise [`SESSION_ENDED`]. Idempotent, so a burst of 401s on the way
/// out does not wake the watcher repeatedly.
#[cfg(all(feature = "app", not(target_arch = "wasm32")))]
pub(crate) fn note_session_ended() {
    if !*SESSION_ENDED.peek() {
        *SESSION_ENDED.write() = true;
    }
}

/// Classify a completed HTTP response. Any response at all - even a 4xx -
/// proves the server is reachable, so it clears the "down" state. A `5xx`
/// is treated as "down" per MAPPS-333: the server is up but failing, and
/// we surface that the same way as an outage. A `4xx` (auth, validation)
/// is NOT "down" and keeps surfacing through the normal per-call paths.
#[cfg(feature = "app")]
pub(crate) fn note_response_status(status: u16) {
    set_server_reachable(!(500..600).contains(&status));
    // MAPPS-428: a status that could mean "this tab is running a bundle
    // older than the deployed one" also kicks an immediate `build_sha`
    // check, so the app-wide update banner appears in the same
    // interaction that produced the error rather than up to five minutes
    // later. Debounced to one in-flight probe inside the callee. Hooked
    // here, at the single point that already classifies every response,
    // so no per-page code changes are needed.
    if crate::hooks::update_check::is_version_skew_status(status) {
        crate::hooks::update_check::note_possible_version_skew();
    }
}

/// Classify a transport-level failure: the opaque browser fetch rejection
/// (which the console frequently renders as a CORS error even though no
/// CORS misconfiguration exists), DNS failure, or timeout. This is the
/// server being unreachable; per MAPPS-333 it is classified here as
/// server-down and never passed through to the user as a CORS / "Failed
/// to fetch" message. (Mokosh App sets no CSP of its own, so there is no
/// CSP-block case to distinguish from a genuine outage here.)
#[cfg(feature = "app")]
pub(crate) fn note_transport_error() {
    set_server_reachable(false);
}

/// Write the reachability flag, but only on an actual transition so a
/// successful request does not wake every reader on each call.
#[cfg(feature = "app")]
fn set_server_reachable(reachable: bool) {
    if *SERVER_REACHABLE.peek() != reachable {
        *SERVER_REACHABLE.write() = reachable;
    }
}

/// API client for making HTTP requests
pub mod api {
    // MAPPS-504: `gloo-net` in the browser, `reqwest` on the desktop,
    // same builder either way (see `crate::platform::http`).
    #[cfg(feature = "app")]
    use crate::platform::http::{MultipartExt, Request};
    #[cfg(feature = "app")]
    use serde::{de::DeserializeOwned, Serialize};

    /// MAPPS-649: derive the Mokosh API base URL.
    ///
    /// Resolution order:
    ///   1. `window.__MOKOSH_CONFIG__.api_base` if set by the prod
    ///      container's entrypoint. Self-hosters on a custom hostname
    ///      override here without rebuilding the image.
    ///   2. Portal-host derivation (MAPPS-649): when the current host equals
    ///      the configured `portal_host` (e.g. `portal.psa.systems`), point
    ///      at `api.<apex>/api/v1` where `<apex>` is the portal host minus
    ///      its leading label (`psa.systems`). Dev's bare
    ///      `portal.localhost:PORT` collapses to same-origin `/api/v1` so
    ///      the Dioxus dev proxy reaches mokosh-server.
    ///   3. Host-prefix derivation for the canonical `msp.<tld>` staff
    ///      deploys (`msp.a8n.systems` -> `api.msp.a8n.systems`).
    ///   4. Same-origin `/api/v1` for dev (localhost, IP address, or any
    ///      host that doesn't match the cases above) so the Dioxus dev
    ///      server can proxy to a local backend.
    #[cfg(feature = "app")]
    pub fn api_base() -> String {
        if let Some(injected) = crate::modules::runtime_config::get("api_base") {
            return normalize_api_base(&injected);
        }
        if let Some(host) = crate::platform::location::host() {
            if let Some(portal_host) = crate::modules::runtime_config::portal_host() {
                let host_no_port = host.split(':').next().unwrap_or(&host);
                if host_no_port.eq_ignore_ascii_case(&portal_host) {
                    // Dev's `portal.localhost` collapses to same-origin.
                    let portal_lower = portal_host.to_ascii_lowercase();
                    if portal_lower.ends_with(".localhost") || portal_lower == "localhost" {
                        return "/api/v1".to_string();
                    }
                    // Prod: strip the leading label to get the apex.
                    if let Some((_leading, apex)) = portal_lower.split_once('.') {
                        if !apex.is_empty() {
                            return format!("https://api.{apex}/api/v1");
                        }
                    }
                }
            }
            if let Some(rest) = host.strip_prefix("msp.") {
                return format!("https://api.msp.{rest}/api/v1");
            }
        }
        default_api_base()
    }

    /// The base to use when nothing else resolved.
    ///
    /// In a browser that is the same-origin `/api/v1`, which the `dx`
    /// dev-server proxy and Caddy both serve.
    #[cfg(all(feature = "app", target_arch = "wasm32"))]
    fn default_api_base() -> String {
        "/api/v1".to_string()
    }

    /// MAPPS-504: a desktop binary has no origin, so a relative base
    /// would address nothing. The build can bake one in with
    /// `MOKOSH_API_BASE`; failing that this points at a mokosh-server on
    /// the local machine, which is right for development and wrong
    /// everywhere else, so a desktop install is expected to set
    /// `api_base` in its `config.json` (see `crate::platform::config`).
    #[cfg(all(feature = "app", not(target_arch = "wasm32")))]
    fn default_api_base() -> String {
        match option_env!("MOKOSH_API_BASE") {
            Some(base) if !base.is_empty() => normalize_api_base(base),
            _ => "http://localhost:8080/api/v1".to_string(),
        }
    }

    /// PMS-758: the API's ORIGIN, without the `/api/v1` path.
    ///
    /// [`api_base`] is a base for API CALLS, so it carries the version prefix.
    /// A few values the server hands back are already paths from the origin,
    /// notably `branding.logo_url` (`/api/v1/public/tenants/{id}/logo`), which
    /// has to be joinable by both this SPA and the email composer. Joining one
    /// of those to `api_base()` produced `/api/v1/api/v1/...` and a broken
    /// image on every surface that showed the logo.
    ///
    /// Empty in dev, where `api_base()` is the same-origin `/api/v1`: an
    /// origin-relative path is already correct there.
    #[cfg(feature = "app")]
    pub fn api_origin() -> String {
        strip_api_version(&api_base())
    }

    /// The pure half of [`api_origin`], so the rule is testable without a
    /// browser. A base that does not end in the version prefix is left alone
    /// rather than truncated on a guess.
    pub fn strip_api_version(base: &str) -> String {
        base.strip_suffix("/api/v1").unwrap_or(base).to_string()
    }

    /// PMS-751: strip trailing slashes from a configured API base.
    ///
    /// Every call site joins with `format!("{}{}", api_base(), path)` and every
    /// path starts with `/`, so a base ending in one produces `/api/v1//tenants`.
    /// Staging is configured exactly that way
    /// (`window.__MOKOSH_CONFIG__.api_base = "https://api.msp.a8n.systems/api/v1/"`),
    /// and it only goes unnoticed because something in front of the server
    /// collapses the duplicate. That is a proxy behaviour to be grateful for,
    /// not to rely on: the day it stops, every request in the app fails at once.
    ///
    /// Fixed here rather than at the 19 join sites, so a base configured with a
    /// slash cannot reintroduce it through whichever one is added next.
    pub fn normalize_api_base(base: &str) -> String {
        base.trim_end_matches('/').to_string()
    }

    /// MAPPS-649: `true` iff the SPA is currently served on the configured
    /// portal host. Used by the portal login page to decide whether to hide
    /// the identifier input and paint the MSP branding block. Port-agnostic,
    /// case-insensitive host compare against `runtime_config::portal_host`.
    ///
    /// The non-web stub returns `false` so downstream call sites compile
    /// under a plain `cargo check`.
    #[cfg(feature = "web")]
    pub fn on_portal_host() -> bool {
        let Some(portal_host) = crate::modules::runtime_config::portal_host() else {
            return false;
        };
        let Some(host) = crate::platform::location::host() else {
            return false;
        };
        let host_no_port = host.split(':').next().unwrap_or(&host);
        host_no_port.eq_ignore_ascii_case(&portal_host)
    }

    #[cfg(not(feature = "web"))]
    pub fn on_portal_host() -> bool {
        false
    }

    /// PMS-729: the current browser-visible host (`window.location.host`,
    /// including port). Attached as `X-Forwarded-Host` on every portal-side
    /// fetch below so the mokosh-server host-to-tenant extractor sees the
    /// real `{slug}.client.<apex>` value even when a dev reverse proxy
    /// rewrites the `Host` header (Dioxus 0.7.7's `[[web.proxy]]` reaches
    /// the backend as `Host: server:8080`, which would otherwise defeat the
    /// extractor). In production, Traefik resets `X-Forwarded-Host` from
    /// the browser's Host on every forwarded request, so the header the SPA
    /// sets is either overwritten by the reverse proxy (prod) or the sole
    /// source of the original host (dev). Safe either way: the extractor
    /// fails closed on any slug/tenant miss, so a spoofed value cannot
    /// escalate.
    #[cfg(feature = "web")]
    fn current_forwarded_host() -> Option<String> {
        crate::platform::location::host().filter(|s| !s.is_empty())
    }

    // Single-threaded global access-token holder. WASM is strictly
    // single-threaded so a `RefCell` is safe; we don't need a mutex.
    // The token lives only in memory: it is wiped on logout and never
    // written to localStorage.
    #[cfg(feature = "app")]
    thread_local! {
        static ACCESS_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    /// Set the current access token. Called from the OIDC callback
    /// handler once `complete_login` returns successfully, and again on
    /// every token refresh / org switch / logout.
    ///
    /// Bumps the reactive [`super::TENANT_GENERATION`] counter so any
    /// `use_resource` that read [`super::active_tenant_generation`] in
    /// its closure re-fetches against the new tenant (Phase-4 F1). The
    /// generation is bumped only when the token actually changes: an org
    /// switch or a refresh produces a different token (so dependent
    /// resources refetch), but a redundant re-set of the identical token
    /// does not. Startup sets the same token twice (the OIDC callback, then
    /// rehydration / `complete_login`); the previous unconditional bump made
    /// every `active_tenant_generation`-subscribed resource fetch twice on
    /// mount (MAPPS-187).
    #[cfg(feature = "app")]
    pub fn set_access_token(token: Option<String>) {
        let changed = ACCESS_TOKEN.with(|t| {
            let mut slot = t.borrow_mut();
            if *slot == token {
                false
            } else {
                *slot = token;
                true
            }
        });
        if changed {
            *super::TENANT_GENERATION.write() += 1;
        }
    }

    /// Read the current access token. Returns `None` before sign-in.
    ///
    /// This is the AGENT token (`typ: "access"`). It is never the portal
    /// session token: `/portal/*` runs on a separate identity (a `contacts`
    /// row) and its own holder, [`current_portal_access_token`].
    #[cfg(feature = "app")]
    pub fn current_access_token() -> Option<String> {
        ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    /// Test-only setter that seeds the staff `ACCESS_TOKEN` holder WITHOUT
    /// bumping the `TENANT_GENERATION` `GlobalSignal`. The production
    /// [`set_access_token`] writes that signal, which panics outside a
    /// Dioxus runtime; unit tests that only need to observe
    /// `current_access_token().is_some()` (the capability-hook staff
    /// bypass) go through this helper instead.
    #[cfg(all(test, feature = "web"))]
    pub(crate) fn set_access_token_for_test(token: Option<String>) {
        ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
    }

    // --- Access-token renewal on the request path (MAPPS-435) ------------
    //
    // The 30 second refresh loops in `crate::hooks::auth` cannot be what keeps
    // a dead bearer off the wire: a tab that the browser discarded and
    // re-created starts every hook from scratch, so the first page mount fires
    // its fetches before any loop has evaluated anything. The renewal
    // therefore also lives on the request path - fresh before the send, and
    // one recovery attempt when the server rejects the bearer anyway.

    /// Seconds before expiry at which the held access token is treated as
    /// spent. Same window the auth loops use.
    #[cfg(feature = "app")]
    const REFRESH_WINDOW_SECS: i64 = 60;

    /// How long a completed renewal answers for the 401s that were already in
    /// flight when it landed. Without it, a 401 no renewal can fix (revoked
    /// grant, rejected audience) would spend a refresh token per request and
    /// re-drive every mounted resource on each one, forever.
    #[cfg(feature = "app")]
    const RENEWAL_DEBOUNCE_SECS: i64 = 30;

    #[cfg(feature = "app")]
    type Renewal = futures_util::future::Shared<
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>>,
    >;

    #[cfg(feature = "app")]
    thread_local! {
        /// The renewal in flight, tagged with the id of its flight. Concurrent
        /// callers join it instead of each spending the refresh token, which
        /// the OP's reuse detection would read as a replay and answer by
        /// killing the whole grant chain.
        static RENEWAL: std::cell::RefCell<Option<(u64, Renewal)>> =
            const { std::cell::RefCell::new(None) };
        /// Flight counter, so a caller only ever clears its own flight.
        static RENEWAL_SEQ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        /// Outcome of the last completed renewal, and when it completed.
        #[allow(clippy::type_complexity)]
        static LAST_RENEWAL: std::cell::RefCell<
            Option<(chrono::DateTime<chrono::Utc>, Result<(), String>)>,
        > = const { std::cell::RefCell::new(None) };
    }

    /// Expiry of the persisted session the held bearer came from. `None` when
    /// nothing is persisted (dev bypass, sessionStorage disabled), which is
    /// also "nothing to renew from".
    #[cfg(feature = "app")]
    fn persisted_expiry() -> Option<chrono::DateTime<chrono::Utc>> {
        if let Some(t) = crate::modules::oidc::storage::load_auth() {
            return Some(t.expires_at);
        }
        crate::modules::oidc::storage::load_standalone().map(|s| s.expires_at)
    }

    /// Whether the held bearer is inside its refresh window or already past
    /// expiry.
    #[cfg(feature = "app")]
    fn access_token_is_stale() -> bool {
        if current_access_token().is_none() {
            return false;
        }
        match persisted_expiry() {
            Some(exp) => exp - chrono::Utc::now() <= chrono::Duration::seconds(REFRESH_WINDOW_SECS),
            None => false,
        }
    }

    /// Whether `token` is the agent bearer this SPA currently holds.
    ///
    /// This is the lane check. The portal session token lives in its own
    /// holder and never matches (MAPPS-395), and a request that carries no
    /// bearer at all (`POST /auth/login`) matches nothing, so neither can
    /// reach the agent renewal or sign-out paths.
    #[cfg(feature = "app")]
    fn is_agent_bearer(token: &str) -> bool {
        current_access_token().as_deref() == Some(token)
    }

    /// Resolve the bearer a `_with_auth` helper should send, and whether the
    /// request is agent-lane. Agent-lane means the caller handed us the bearer
    /// this SPA holds, so it is renewed when spent and its 401 is a session
    /// event; anything else is passed through untouched.
    #[cfg(feature = "app")]
    async fn agent_lane_bearer(token: &str) -> (bool, String) {
        if !is_agent_bearer(token) {
            return (false, token.to_string());
        }
        ensure_fresh_access_token().await;
        // Re-read: the renewal above may have replaced what the caller holds.
        (
            true,
            current_access_token().unwrap_or_else(|| token.to_string()),
        )
    }

    /// Renew the held access token when it is within [`REFRESH_WINDOW_SECS`]
    /// of expiry or already past it, so no request goes out with a bearer the
    /// SPA already knows is dead. A no-op otherwise: the common case costs one
    /// sessionStorage read.
    ///
    /// Single-flight, see [`renew_access_token`]. A failure here is logged but
    /// does not sign the user out: the request still goes out, and the 401 it
    /// earns runs [`note_agent_unauthorized`], which owns that decision.
    #[cfg(feature = "app")]
    pub async fn ensure_fresh_access_token() {
        if !access_token_is_stale() {
            return;
        }
        if let Err(e) = renew_access_token().await {
            tracing::warn!("access-token renewal before a request failed: {e}");
        }
    }

    /// Exchange the persisted refresh token for a new access token, joining
    /// the renewal already in flight rather than starting a second one.
    ///
    /// Also the auth loops' renewal (`crate::hooks::auth`), so a loop tick and
    /// a request-time renewal that coincide spend one refresh token between
    /// them instead of racing each other into a reuse detection.
    #[cfg(feature = "app")]
    pub async fn renew_access_token() -> Result<(), String> {
        let (flight, shared) = RENEWAL.with(|slot| {
            if let Some((flight, existing)) = slot.borrow().as_ref() {
                return (*flight, existing.clone());
            }
            let flight = RENEWAL_SEQ.with(|seq| {
                let next = seq.get().wrapping_add(1);
                seq.set(next);
                next
            });
            let started: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>> =
                Box::pin(renew_persisted_session());
            let shared = futures_util::FutureExt::shared(started);
            *slot.borrow_mut() = Some((flight, shared.clone()));
            (flight, shared)
        });
        let outcome = shared.await;
        // Clear only OUR flight. A renewal started after this one finished is
        // a different flight, and clearing it would have every caller joining
        // it start yet another.
        let cleared = RENEWAL.with(|slot| {
            let mut held = slot.borrow_mut();
            if held
                .as_ref()
                .is_some_and(|(held_flight, _)| *held_flight == flight)
            {
                *held = None;
                true
            } else {
                false
            }
        });
        if cleared {
            LAST_RENEWAL
                .with(|slot| *slot.borrow_mut() = Some((chrono::Utc::now(), outcome.clone())));
        }
        outcome
    }

    /// The renewal implementation. OIDC first, because that is the bundle
    /// `rehydrate_from_storage` prefers when both are somehow present.
    #[cfg(feature = "app")]
    async fn renew_persisted_session() -> Result<(), String> {
        use crate::modules::oidc::storage;

        if let Some(stored) = storage::load_auth() {
            let refresh = stored
                .refresh_token
                .clone()
                .ok_or_else(|| "the stored OIDC session carries no refresh token".to_string())?;
            let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
            let fresh = crate::modules::oidc::refresh_tokens(&cfg, &refresh, &stored.id_token)
                .await
                .map_err(|e| e.to_string())?;
            storage::save_auth(&storage::StoredTokens {
                access_token: fresh.access_token.clone(),
                id_token: fresh.id_token,
                refresh_token: fresh.refresh_token,
                expires_at: fresh.expires_at,
                scope: fresh.scope,
            });
            // Last, because the generation bump re-drives every mounted
            // resource: the new token must already be persisted when they go.
            set_access_token(Some(fresh.access_token));
            return Ok(());
        }

        if let Some(session) = storage::load_standalone() {
            let refresh = session.refresh_token.clone().ok_or_else(|| {
                "the stored standalone session carries no refresh token".to_string()
            })?;

            #[derive(serde::Serialize)]
            struct RefreshReq {
                refresh_token: String,
            }
            #[derive(serde::Deserialize)]
            struct RefreshResp {
                access_token: String,
                refresh_token: String,
                expires_at: chrono::DateTime<chrono::Utc>,
            }

            // `post_typed` sends no bearer, so this cannot recurse back into
            // the renewal path through a 401.
            let resp = post_typed::<RefreshResp, _>(
                "/auth/refresh",
                &RefreshReq {
                    refresh_token: refresh,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            storage::save_standalone(&storage::StandaloneSession {
                access_token: resp.access_token.clone(),
                refresh_token: Some(resp.refresh_token),
                expires_at: resp.expires_at,
                user: session.user,
            });
            set_access_token(Some(resp.access_token));
            return Ok(());
        }

        Err("no persisted session to renew".to_string())
    }

    /// A request that carried the agent bearer was answered 401.
    ///
    /// One renewal attempt: on success [`set_access_token`] bumps
    /// [`super::TENANT_GENERATION`], and every `use_resource` closure that
    /// reads [`super::active_tenant_generation`] re-fetches with the new token
    /// on its own. On failure, or with nothing to renew from, the session is
    /// cleared and the browser is sent to `/login` - the same end state as the
    /// refresh loop's failure branch in `crate::hooks::auth::use_token_refresh`.
    ///
    /// Only reached from the agent lane ([`is_agent_bearer`]). A `/portal/*`
    /// 401 belongs to a separate identity and a `POST /auth/login` 401 means
    /// wrong password; neither may sign an agent out.
    #[cfg(feature = "app")]
    async fn note_agent_unauthorized() {
        match recent_renewal() {
            // A renewal landed moments ago, so this 401 was answered against
            // the token it replaced; the resources it re-drove carry the new
            // one. Renewing again would spend a refresh token per stale reply.
            Some(Ok(())) => return,
            // The renewal that just failed is why this 401 happened. Do not
            // try again, and do not leave the screen sitting on the error.
            Some(Err(e)) => {
                tracing::warn!("agent bearer rejected and the last renewal failed: {e}");
                end_agent_session();
                return;
            }
            None => {}
        }
        if let Err(e) = renew_access_token().await {
            tracing::warn!("agent bearer rejected and could not be renewed: {e}");
            end_agent_session();
        }
    }

    /// Outcome of the last renewal while it is still recent enough to answer
    /// for a 401 that was already in flight. `None` once it has aged out.
    #[cfg(feature = "app")]
    fn recent_renewal() -> Option<Result<(), String>> {
        LAST_RENEWAL.with(|slot| {
            slot.borrow().as_ref().and_then(|(at, outcome)| {
                (chrono::Utc::now() - *at < chrono::Duration::seconds(RENEWAL_DEBOUNCE_SECS))
                    .then(|| outcome.clone())
            })
        })
    }

    /// Clear the session and hard-redirect to `/login`. The fetch layer sits
    /// below the Router and cannot reach `AuthContext`, so the full reload is
    /// what drops the cleared state onto the login screen - the same reason
    /// `use_token_refresh` redirects this way.
    #[cfg(feature = "app")]
    fn end_agent_session() {
        set_access_token(None);
        // Clears the standalone session too (see `storage::clear_auth`).
        crate::modules::oidc::storage::clear_auth();
        // MAPPS-504: in the browser the full reload is what drops the
        // cleared state onto the login screen. A desktop window has no
        // reload, so it raises `SESSION_ENDED` instead and the watcher at
        // the app root (`crate::hooks::auth::use_session_end_watch`)
        // clears `AuthContext`, which is what the route guard reads.
        // Either way the user ends up on the login screen; what must not
        // happen is the session ending with nothing on screen changing.
        // MAPPS-518 URL swap: tenant login lives at /client/login now.
        #[cfg(target_arch = "wasm32")]
        if let Err(e) = crate::platform::location::set_href("/client/login") {
            tracing::warn!("could not redirect to /client/login after signing out: {e}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        super::note_session_ended();
    }

    // MAPPS-395: the client-portal session token, a separate token class from
    // `ACCESS_TOKEN`. mokosh-server mints it at `POST /portal/auth/login` with
    // `typ: "portal_access"` over a `contacts` row, and every `/portal/*` route
    // rejects an agent bearer (`typ: "access"`) with 401. The two holders never
    // cross-populate: the agent sign-in paths write only `ACCESS_TOKEN`, and
    // only the `_portal_authed` helpers below read this one.
    //
    // Memory-only like its sibling, so a reload returns the visitor to
    // `/portal/login` rather than leaving a bearer in storage.
    #[cfg(feature = "app")]
    thread_local! {
        static PORTAL_ACCESS_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
        // PMS-729 phase 2 H2: refresh token holder. Memory-only, sibling of
        // the access-token slot. Rotation replaces it; logout clears it.
        static PORTAL_REFRESH_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    /// Set the portal session token. Called from the portal login page on a
    /// successful `POST /portal/auth/login`, and with `None` to end the
    /// session.
    ///
    /// Deliberately does NOT bump [`super::TENANT_GENERATION`]: that counter is
    /// the agent-side "active tenant changed" signal, and the portal pages
    /// mount fresh after the post-login navigation anyway.
    #[cfg(feature = "app")]
    pub fn set_portal_access_token(token: Option<String>) {
        PORTAL_ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
    }

    /// Read the portal session token. `None` until a portal contact signs in.
    ///
    /// Only the `_portal_authed` helpers below call this; anything that merely
    /// needs to know whether a session exists (the route guard) asks
    /// [`has_portal_session`] instead, so the token itself stays inside this
    /// module.
    #[cfg(feature = "app")]
    pub fn current_portal_access_token() -> Option<String> {
        PORTAL_ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    /// Whether a portal session is held. The predicate `PortalGuard` gates
    /// `/portal/*` on, without handing the token out.
    #[cfg(feature = "app")]
    pub fn has_portal_session() -> bool {
        PORTAL_ACCESS_TOKEN.with(|t| t.borrow().is_some())
    }

    /// MAPPS-563: `localStorage` key under which the portal refresh token
    /// is persisted so it survives a hard refresh / deep-link cold-load.
    /// Distinct from the platform-admin `mokosh:platform_token`
    /// (`sessionStorage`) and the standalone-agent session keys so a
    /// stale value from another plane cannot cross-populate this one.
    ///
    /// XSS trade-off: the refresh token becomes readable by scripts
    /// running on the portal origin. That is strictly worse than the
    /// pre-563 in-memory-only shape, but a full HttpOnly-cookie
    /// implementation crosses tenant subdomain <-> API subdomain and
    /// requires CORS + Domain=.<apex> cookie work that we don't have
    /// today. The follow-up ticket to move this to a cookie is filed
    /// as a note in `docs/mokosh-client-login/dashboard-overhaul-1.md`
    /// under B2.
    #[cfg(feature = "web")]
    const PORTAL_REFRESH_STORAGE_KEY: &str = "mokosh:portal_refresh_token";

    /// PMS-729 phase 2 H2 / MAPPS-563: set the portal refresh token.
    /// Called from the login page on `POST /portal/auth/login` and from
    /// the auto-refresh hook after `POST /portal/auth/refresh`. `None`
    /// clears both the in-memory slot and the localStorage mirror so a
    /// hard refresh after logout lands the visitor at /portal/login
    /// rather than silently re-authenticating.
    #[cfg(feature = "web")]
    pub fn set_portal_refresh_token(token: Option<String>) {
        PORTAL_REFRESH_TOKEN.with(|t| *t.borrow_mut() = token.clone());
        // Persist to localStorage so a cold-load can re-mint the access
        // token via /portal/auth/refresh before PortalGuard bounces.
        // Any storage access can throw (private-mode browsers, disabled
        // site data), so degrade to in-memory-only on failure.
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                match token.as_deref() {
                    Some(value) => {
                        let _ = storage.set_item(PORTAL_REFRESH_STORAGE_KEY, value);
                    }
                    None => {
                        let _ = storage.remove_item(PORTAL_REFRESH_STORAGE_KEY);
                    }
                }
            }
        }
    }

    /// PMS-729 phase 2 H2 / MAPPS-563: read the portal refresh token.
    /// Only the refresh flow and the logout flow call this.
    ///
    /// MAPPS-563: when the in-memory slot is empty (cold-load after a
    /// hard refresh / deep-link), fall back to the localStorage mirror.
    /// The first successful `POST /portal/auth/refresh` rotates the
    /// token and calls `set_portal_refresh_token(Some(new))`, which
    /// updates both places. If the localStorage read succeeds we also
    /// prime the in-memory slot so subsequent reads in the same
    /// browser session skip the storage round-trip.
    #[cfg(feature = "web")]
    pub fn current_portal_refresh_token() -> Option<String> {
        let in_memory = PORTAL_REFRESH_TOKEN.with(|t| t.borrow().clone());
        if in_memory.is_some() {
            return in_memory;
        }
        let win = web_sys::window()?;
        let storage = win.local_storage().ok().flatten()?;
        let stored = storage
            .get_item(PORTAL_REFRESH_STORAGE_KEY)
            .ok()
            .flatten()?;
        if stored.is_empty() {
            return None;
        }
        // Prime the in-memory slot so the next caller skips storage.
        PORTAL_REFRESH_TOKEN.with(|t| *t.borrow_mut() = Some(stored.clone()));
        Some(stored)
    }

    // --- Contact-plane session holders (mokosh-contact-login, prompt 005) ---
    //
    // The `/api/v1/contact/*` tree runs on the contact JWT (`typ:
    // "contact"`) minted at `POST /contact/auth/login`. Kept in a
    // dedicated slot so a visitor holding both a staff bearer AND a
    // contact bearer (rare, but the mokosh workspace routes render for
    // either identity) does not accidentally cross the two: the
    // `_contact_authed` helpers read ONLY this slot, and the staff
    // helpers read ONLY `ACCESS_TOKEN`. Refresh mirror lives in
    // localStorage under `CONTACT_REFRESH_STORAGE_KEY` so a hard
    // refresh / deep-link cold-load can re-mint via
    // `POST /contact/auth/refresh` before AuthGuard bounces.
    #[cfg(feature = "web")]
    thread_local! {
        static CONTACT_ACCESS_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
        static CONTACT_REFRESH_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
        /// MAPPS-604: the Company UUID the current contact session is
        /// scoped to. Populated from the `contact.company_id` field on
        /// every /contact/auth/{login,refresh,login-link/select}
        /// response (PMS-935 extends the wire shape with this id). Pages
        /// read it via [`current_contact_company_id`] to build scoped
        /// URLs (e.g. the Cross-Company probe check) without re-parsing
        /// the current route.
        static CONTACT_COMPANY_ID: std::cell::RefCell<Option<uuid::Uuid>> = const { std::cell::RefCell::new(None) };
        /// MAPPS-609: the UUID of the Contact behind the current session.
        /// Populated from the `contact.contact_id` field on every
        /// /contact/auth/{login,refresh,login-link/select} response
        /// (PMS-937 extends the wire shape). Pages read it via
        /// [`current_contact_id`] to gate ownership-scoped controls -
        /// specifically the "edit ticket" Edit affordance on
        /// ticket detail (rendered only for the ticket's reporter).
        /// Optional so a pre-PMS-937 server that omits the field still
        /// deserialises; the store is left at `None` and any ownership
        /// gate that requires the id falls closed.
        static CONTACT_ID: std::cell::RefCell<Option<uuid::Uuid>> = const { std::cell::RefCell::new(None) };
    }

    #[cfg(feature = "web")]
    const CONTACT_REFRESH_STORAGE_KEY: &str = "mokosh:contact_refresh_token";

    /// `localStorage` key that remembers "this browser last signed in
    /// as a contact on slug X" so the AuthGuard bounce for an expired
    /// session can send them to `/portal/{slug}/login` rather than the
    /// staff `/login`. Written by the contact-login page on success;
    /// cleared alongside the refresh token on logout.
    ///
    /// DEPRECATED post MAPPS-589 (prompt 011): kept for one release
    /// cycle so a cold-load on old client code still finds a value.
    /// New writes flow through both this key and
    /// [`CONTACT_LAST_PORTAL_ID_STORAGE_KEY`]; bootstrap prefers the
    /// portal-id key when present.
    #[cfg(feature = "web")]
    pub const CONTACT_LAST_SLUG_STORAGE_KEY: &str = "mokosh:contact_last_slug";

    /// MAPPS-589 (prompt 011): `localStorage` key that remembers "this
    /// browser last signed in as a contact on Portal ID N" so the
    /// AuthGuard bounce for an expired session can send them to
    /// `/portal/{portal_id}/login` (the new URL shape) rather than the
    /// slug-based path. Written by every contact login flow that gets
    /// a Portal ID (either from the URL handle or from the server
    /// response's `contact.portal_id` field, once PMS-928 lands).
    /// Cleared alongside the refresh token on logout.
    #[cfg(feature = "web")]
    pub const CONTACT_LAST_PORTAL_ID_STORAGE_KEY: &str = "mokosh:contact_last_portal_id";

    /// Set the contact access token. `None` clears the in-memory slot.
    /// Does NOT bump [`super::TENANT_GENERATION`]: the contact plane
    /// has its own tenant scope and the pages it mounts fresh after
    /// login navigation anyway.
    #[cfg(feature = "web")]
    pub fn set_contact_access_token(token: Option<String>) {
        CONTACT_ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
    }

    /// Read the contact access token. `None` until a contact signs in.
    #[cfg(feature = "web")]
    pub fn current_contact_access_token() -> Option<String> {
        CONTACT_ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    /// Whether a contact session is held. Cheap predicate for gates
    /// (AuthGuard) that don't need to touch the token itself.
    #[cfg(feature = "web")]
    pub fn has_contact_session() -> bool {
        CONTACT_ACCESS_TOKEN.with(|t| t.borrow().is_some())
    }

    /// Set the contact refresh token. Also mirrors to localStorage so
    /// a cold-load can bootstrap via `/contact/auth/refresh`. Passing
    /// `None` clears both the in-memory slot and the storage mirror.
    #[cfg(feature = "web")]
    pub fn set_contact_refresh_token(token: Option<String>) {
        CONTACT_REFRESH_TOKEN.with(|t| *t.borrow_mut() = token.clone());
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                match token.as_deref() {
                    Some(value) => {
                        let _ = storage.set_item(CONTACT_REFRESH_STORAGE_KEY, value);
                    }
                    None => {
                        let _ = storage.remove_item(CONTACT_REFRESH_STORAGE_KEY);
                    }
                }
            }
        }
    }

    /// Read the contact refresh token. Falls back to the localStorage
    /// mirror on a cold-load; primes the in-memory slot so subsequent
    /// reads skip the storage round-trip.
    #[cfg(feature = "web")]
    pub fn current_contact_refresh_token() -> Option<String> {
        let in_memory = CONTACT_REFRESH_TOKEN.with(|t| t.borrow().clone());
        if in_memory.is_some() {
            return in_memory;
        }
        let win = web_sys::window()?;
        let storage = win.local_storage().ok().flatten()?;
        let stored = storage
            .get_item(CONTACT_REFRESH_STORAGE_KEY)
            .ok()
            .flatten()?;
        if stored.is_empty() {
            return None;
        }
        CONTACT_REFRESH_TOKEN.with(|t| *t.borrow_mut() = Some(stored.clone()));
        Some(stored)
    }

    /// Remember the last slug this browser signed in on. Used by the
    /// AuthGuard bounce on expired sessions to route the visitor back
    /// to the same portal they came from.
    #[cfg(feature = "web")]
    pub fn set_contact_last_slug(slug: &str) {
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.set_item(CONTACT_LAST_SLUG_STORAGE_KEY, slug);
            }
        }
    }

    /// Read the last-known slug the browser signed in on. `None` when
    /// no contact login has ever happened on this browser.
    #[cfg(feature = "web")]
    pub fn current_contact_last_slug() -> Option<String> {
        let win = web_sys::window()?;
        let storage = win.local_storage().ok().flatten()?;
        let stored = storage
            .get_item(CONTACT_LAST_SLUG_STORAGE_KEY)
            .ok()
            .flatten()?;
        if stored.is_empty() {
            None
        } else {
            Some(stored)
        }
    }

    /// MAPPS-589 (prompt 011): remember the last Portal ID this
    /// browser signed in on. Written by every contact login flow that
    /// receives a numeric Portal ID; preferred by the AuthGuard
    /// cold-load bootstrap over the legacy slug key.
    #[cfg(feature = "web")]
    pub fn set_contact_last_portal_id(value: &str) {
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.set_item(CONTACT_LAST_PORTAL_ID_STORAGE_KEY, value);
            }
        }
    }

    /// MAPPS-615 (prompt 014): drop the last-Portal-ID hint so a
    /// visitor who explicitly says "not my portal" on the step 2 page
    /// does not immediately bounce back to `/portal/{that}/login` via
    /// the AuthGuard cold-load or last-slug-remember paths. Called
    /// from `ContactLoginByPortalIdPage`'s "Choose a different portal"
    /// button.
    #[cfg(feature = "web")]
    pub fn clear_contact_last_portal_id() {
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.remove_item(CONTACT_LAST_PORTAL_ID_STORAGE_KEY);
            }
        }
    }

    /// MAPPS-589 (prompt 011): read the last-known Portal ID this
    /// browser signed in on. `None` when no contact login carrying a
    /// Portal ID has happened yet (the legacy slug-only flow leaves
    /// this key unset).
    #[cfg(feature = "web")]
    pub fn current_contact_last_portal_id() -> Option<String> {
        let win = web_sys::window()?;
        let storage = win.local_storage().ok().flatten()?;
        let stored = storage
            .get_item(CONTACT_LAST_PORTAL_ID_STORAGE_KEY)
            .ok()
            .flatten()?;
        if stored.is_empty() {
            None
        } else {
            Some(stored)
        }
    }

    /// MAPPS-604: overwrite the Company UUID this contact session is
    /// scoped to. `None` clears it (called on logout / session drop).
    #[cfg(feature = "web")]
    pub fn set_contact_company_id(id: Option<uuid::Uuid>) {
        CONTACT_COMPANY_ID.with(|slot| *slot.borrow_mut() = id);
    }

    /// MAPPS-604: read the Company UUID this contact session is scoped
    /// to. `None` before the first `/contact/auth/*` response that
    /// carries `company_id` (pre-PMS-935 servers omit the field, so a
    /// contact signed in against an older mokosh sees `None` and
    /// callers fall back to whatever URL-derived id they have).
    #[cfg(feature = "web")]
    pub fn current_contact_company_id() -> Option<uuid::Uuid> {
        CONTACT_COMPANY_ID.with(|slot| *slot.borrow())
    }

    /// MAPPS-609: overwrite the contact UUID this session belongs to.
    /// `None` clears it (called on logout / session drop).
    #[cfg(feature = "web")]
    pub fn set_contact_id(id: Option<uuid::Uuid>) {
        CONTACT_ID.with(|slot| *slot.borrow_mut() = id);
    }

    /// MAPPS-609: read the contact UUID the current session belongs to.
    /// `None` before the first `/contact/auth/*` response that carries
    /// `contact_id` (pre-PMS-937 servers omit the field, so a contact
    /// signed in against an older mokosh sees `None`; callers that need
    /// ownership must treat that as "unknown" and fall closed).
    #[cfg(feature = "web")]
    pub fn current_contact_id() -> Option<uuid::Uuid> {
        CONTACT_ID.with(|slot| *slot.borrow())
    }

    /// Clear the entire contact session. Setting the refresh token to
    /// `None` clears its localStorage mirror as well; both last-*
    /// keys are wiped so a follow-up cold-load starts blank rather
    /// than latching onto a stale hint.
    #[cfg(feature = "web")]
    pub fn clear_contact_session() {
        set_contact_access_token(None);
        set_contact_refresh_token(None);
        set_contact_company_id(None);
        set_contact_id(None);
        if let Some(win) = web_sys::window() {
            if let Ok(Some(storage)) = win.local_storage() {
                let _ = storage.remove_item(CONTACT_LAST_SLUG_STORAGE_KEY);
                let _ = storage.remove_item(CONTACT_LAST_PORTAL_ID_STORAGE_KEY);
            }
        }
    }

    /// MAPPS-630: clear every remnant of the STAFF session. Called on
    /// contact sign-in so the two planes stay mutually exclusive
    /// within one browser origin. Mirrors what a full staff logout
    /// clears: the in-memory access token, both OIDC + standalone
    /// stored bundles in sessionStorage, and the platform-admin
    /// bearer that sits alongside them.
    #[cfg(feature = "web")]
    pub fn clear_staff_session_for_plane_switch() {
        set_access_token(None);
        crate::modules::oidc::storage::clear_auth();
        if let Some(win) = web_sys::window() {
            if let Ok(Some(session)) = win.session_storage() {
                // Kept in sync with the `PLATFORM_TOKEN_KEY` const in
                // hooks/capabilities.rs + pages/platform_login.rs.
                let _ = session.remove_item("mokosh:platform_token");
            }
        }
    }

    /// MAPPS-630: cross-plane isolation on sign-in. Call at every
    /// path that lands a fresh STAFF access token (standalone login
    /// success, OIDC callback exchange, platform-login success). No
    /// return value; the effect is entirely in the token holders.
    #[cfg(feature = "web")]
    pub fn on_staff_signin_clear_contact_side() {
        clear_contact_session();
        crate::hooks::capabilities::clear_contact_capabilities();
        crate::hooks::branding::clear_effective_branding();
    }

    /// MAPPS-630: cross-plane isolation on sign-in. Call at every
    /// path that lands a fresh CONTACT access token (portal password
    /// login success, magic-link redeem success, contact set-password
    /// -then-auto-login).
    #[cfg(feature = "web")]
    pub fn on_contact_signin_clear_staff_side() {
        clear_staff_session_for_plane_switch();
        crate::hooks::branding::clear_effective_branding();
    }

    // The app-only API helpers below are grouped under this `api`
    // module; the non-`app` build compiles the module with no items.

    /// Map a transport-level send failure to a `String` error, classifying
    /// it as a server-unreachable condition (MAPPS-333) on the way out.
    /// Used only at `.send()` sites - serialization (`.json()`) failures
    /// keep the plain mapping since they are not connectivity problems.
    #[cfg(feature = "app")]
    fn transport_err(e: impl std::fmt::Display) -> String {
        super::note_transport_error();
        e.to_string()
    }

    /// Transport-error sibling of [`transport_err`] for the typed helpers,
    /// classifying the failure as server-unreachable before wrapping it as
    /// [`ApiError::Network`].
    #[cfg(feature = "app")]
    fn network_err(e: impl std::fmt::Display) -> ApiError {
        super::note_transport_error();
        ApiError::Network(e.to_string())
    }

    /// Get request
    #[cfg(feature = "app")]
    pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Tolerant GET that returns the HTTP status and raw body together,
    /// instead of collapsing any non-2xx into an `Err`. Used by the
    /// System Status page (PMS-237) to read diagnostic endpoints like
    /// `/ready`, which deliberately answers `503` with a JSON breakdown
    /// when a dependency is down. A transport-level failure (server
    /// unreachable, DNS, CORS) is still an `Err`.
    #[cfg(feature = "app")]
    pub async fn probe(path: &str) -> Result<(u16, String), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url).send().await.map_err(transport_err)?;
        let status = response.status();
        // Classify the probe result so the recovery poll's `/ready` hit
        // flips the app back to reachable on success (MAPPS-333).
        super::note_response_status(status);
        let body = response.text().await.map_err(|e| e.to_string())?;
        Ok((status, body))
    }

    /// Build a user-facing error string for a non-2xx response on the
    /// string-returning helpers. Parses the standard
    /// `{"error":{"message","errors":[...]}}` envelope so forms that still
    /// use the `String`-error helpers surface the real validation message
    /// (e.g. "Title must be between 1 and 500 characters") instead of a bare
    /// "Request failed with status: 422" (MAPPS-210). The `_typed` helpers
    /// carry the field-level envelope separately via `handle_response`; this
    /// is the flat-string sibling for the many existing callers. Falls back
    /// to the status line when the body is not a recognised envelope.
    #[cfg(feature = "app")]
    async fn status_error(response: crate::platform::http::Response) -> String {
        let status = response.status();
        // A real HTTP response (even an error one) proves reachability; a
        // 5xx is classified as "down" (MAPPS-333).
        super::note_response_status(status);
        let body = response.text().await.unwrap_or_default();
        match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
            Ok(env) => {
                // MAPPS-348: 410 Gone with `ACCOUNT_DELETED` is the terminal
                // signal from mokosh-server that the bunyip account was
                // deleted (users.deleted_at is set). Flip the global signal
                // so AppLayout renders the terminal modal; the returned
                // message is still surfaced to any caller that inspects it,
                // but the overlay will normally block further UI.
                if status == 410 && env.error.code == "ACCOUNT_DELETED" {
                    super::note_account_deleted();
                }
                let fields = env.error.errors.unwrap_or_default();
                if status == 422 && !fields.is_empty() {
                    fields
                        .iter()
                        .map(|f| f.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                } else if is_generic_auth_message(status, &env.error.message) {
                    // MAPPS-624: the server ships a generic
                    // "Authentication required" / "Access denied"
                    // message on 401/403. Surfacing it verbatim
                    // reads to a signed-in caller as "your session
                    // is gone" when the real cause is "you don't
                    // have permission for this endpoint" (portal
                    // contact hitting a staff URL, staff missing a
                    // capability, etc.). Replace it with the
                    // plane-aware permission message so the copy
                    // matches what actually happened.
                    permission_message()
                } else if !env.error.message.is_empty() {
                    env.error.message
                } else {
                    user_friendly_status(status)
                }
            }
            Err(_) => user_friendly_status(status),
        }
    }

    /// MAPPS-282: replace the developer-facing "Request failed with status:
    /// 422" string fallback with a user-facing message keyed on the status
    /// class. This is the last-resort branch in `status_error` (server
    /// returned a non-2xx with no `ErrorResponse` envelope and no field
    /// errors), reached by the many forms that still use the string-error
    /// helpers (`post_authed`, `put_authed`). The forms that have been
    /// migrated to the `_typed` variants already get the field-level
    /// `ApiError::user_message` treatment; this brings the legacy callers
    /// to at least non-developer-facing parity.
    /// MAPPS-624: "Your session has expired" reads as an incorrect
    /// signal when the caller is authenticated and only lacks
    /// permission for the specific endpoint (e.g. a portal contact
    /// hitting a staff-only URL). Treat 401 the same as 403 whenever
    /// we hold any session; only bounce to the "sign in again" copy
    /// when the browser has no session at all, which is the true
    /// "session expired" case a returning visitor would hit.
    ///
    /// MAPPS-624: recognise the server's generic 401/403 envelope
    /// messages so we override them with the plane-aware
    /// [`permission_message`]. Anything else (e.g. "This organization
    /// is not active" for a suspended tenant, MFA-required copy)
    /// stays verbatim so users still see the specific reason.
    #[cfg(feature = "web")]
    fn is_generic_auth_message(status: u16, msg: &str) -> bool {
        if !(status == 401 || status == 403) {
            return false;
        }
        let m = msg.trim();
        matches!(
            m,
            ""
                | "Authentication required"
                | "Access denied"
                | "Access denied: Access denied"
                | "Forbidden"
                | "Unauthorized"
        )
    }

    #[cfg(feature = "web")]
    fn permission_message() -> String {
        let has_any_session = current_access_token().is_some()
            || current_contact_access_token().is_some();
        if has_any_session {
            "You don't have permission to perform this action. Contact your administrator or support team if you think you should.".into()
        } else {
            "Your session has ended. Please sign in again.".into()
        }
    }

    /// Non-web fallback so callers compile under `cargo check` without
    /// pulling in the plane-aware permission logic.
    #[cfg(all(feature = "app", not(feature = "web")))]
    fn permission_message() -> String {
        "You don't have permission to perform this action.".into()
    }

    #[cfg(feature = "app")]
    fn user_friendly_status(status: u16) -> String {
        match status {
            400 => "The request was rejected. Please check the form and try again.".into(),
            401 | 403 => permission_message(),
            404 => "The requested resource was not found.".into(),
            409 => "The change conflicts with another update. Please refresh and retry.".into(),
            422 => "Validation failed. Please check the form fields.".into(),
            429 => "Too many requests. Please try again shortly.".into(),
            500..=599 => "The server hit an error. Please try again.".into(),
            _ => format!("Request failed ({status})."),
        }
    }

    /// Get request with auth token
    #[cfg(feature = "app")]
    pub async fn get_with_auth<T: DeserializeOwned>(path: &str, token: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    // --- Whole-list reads (MAPPS-528) ------------------------------------
    //
    // mokosh-server caps `per_page` at `PaginationParams::MAX_PER_PAGE` and
    // CLAMPS anything larger instead of rejecting it, so a page that asked
    // for 200 got 100 rows and no sign that the rest existed. Fifteen call
    // sites asked for 200 or 500 and read `resp.data` once. These helpers
    // are the single way to read a whole collection: they request the cap
    // and keep going until a short page arrives.

    /// The server's `per_page` ceiling. Mirrors
    /// `PaginationParams::MAX_PER_PAGE`, which is itself the client's copy
    /// of the server's constant.
    pub const MAX_PER_PAGE: u32 = crate::utils::PaginationParams::MAX_PER_PAGE;

    /// Page ceiling for the `get_all_*` helpers: 100 pages at the cap is
    /// 10,000 rows. Reaching it means either a list no screen can use or an
    /// endpoint that ignores `page`, so the helper fails loudly rather than
    /// handing back a list that is short for a reason nobody can see.
    #[cfg(feature = "app")]
    const MAX_PAGES: u32 = 100;

    /// Append the paging query the `get_all_*` helpers drive, preserving any
    /// filters the caller already put on `path`.
    #[cfg(feature = "app")]
    pub(crate) fn paged_path(path: &str, page: u32) -> String {
        let sep = if path.contains('?') { '&' } else { '?' };
        format!("{path}{sep}page={page}&per_page={MAX_PER_PAGE}")
    }

    /// Read every page of a list endpoint with an explicit bearer.
    ///
    /// `path` carries the endpoint's own filters and must NOT spell `page`
    /// or `per_page`; both are appended per request.
    #[cfg(feature = "app")]
    pub async fn get_all_with_auth<T: DeserializeOwned>(
        path: &str,
        token: &str,
    ) -> Result<Vec<T>, String> {
        let mut rows: Vec<T> = Vec::new();
        for page in 1..=MAX_PAGES {
            let resp: crate::utils::Paginated<T> =
                get_with_auth(&paged_path(path, page), token).await?;
            let full = resp.data.len() as u32 >= MAX_PER_PAGE;
            rows.extend(resp.data);
            if !full {
                return Ok(rows);
            }
        }
        Err(format!(
            "{path} returned more than {MAX_PAGES} full pages of {MAX_PER_PAGE} rows; \
             refusing to render a list that is silently short"
        ))
    }

    /// Post request
    #[cfg(feature = "app")]
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request with auth token
    #[cfg(feature = "app")]
    pub async fn post_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Put request with auth token
    #[cfg(feature = "app")]
    pub async fn put_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// PATCH request with auth token. The first caller is the saved
    /// dashboards module (PMS-453), which surfaces partial-update
    /// semantics (toggle `is_default`, edit just the name); a PUT
    /// helper would have invited callers to send the whole row and
    /// blow away unset fields, which is the wrong shape for that
    /// surface.
    #[cfg(feature = "app")]
    pub async fn patch_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Delete request with auth token
    #[cfg(feature = "app")]
    pub async fn delete_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            Ok(())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request with auth token for endpoints that return an empty
    /// body (e.g. `POST /notifications/{id}/read`, which responds 200
    /// with no JSON). The body-parsing `post_with_auth` would fail on
    /// the empty payload, so this variant only checks the status, like
    /// `delete_with_auth`. No request body is sent.
    #[cfg(feature = "app")]
    pub async fn post_no_content_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);
        let (agent_lane, token) = agent_lane_bearer(token).await;

        let response = Request::post(&url)
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if agent_lane && response.status() == 401 {
            note_agent_unauthorized().await;
        }
        if response.ok() {
            Ok(())
        } else {
            Err(status_error(response).await)
        }
    }

    /// MAPPS-429: PUT a single file as `multipart/form-data` under the part
    /// name `file`, with the caller's bearer token.
    ///
    /// The body is assembled by the browser from a `FormData` carrying a
    /// `Blob`, so the boundary is the browser's problem. Writing the multipart
    /// envelope by hand would mean generating a boundary and hoping it never
    /// occurs inside the image bytes.
    ///
    /// Deliberately not folded into the JSON helpers above: those set
    /// `Content-Type: application/json`, and here the header must be left
    /// ALONE. Setting it manually omits the boundary parameter, and the server
    /// then rejects a body it cannot split.
    #[cfg(feature = "app")]
    pub async fn put_file_authed<T: DeserializeOwned>(
        path: &str,
        file_name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let token = current_access_token()
            .ok_or_else(|| ApiError::Network("not authenticated".to_string()))?;

        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .multipart_file(file_name, mime, bytes)
            .map_err(network_err)?
            .send()
            .await
            .map_err(network_err)?;
        if resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    /// MAPPS-587: POST a single file as `multipart/form-data` under the part
    /// name `file`, with the caller's bearer token.
    ///
    /// The same shape as [`put_file_authed`] and split from it only by method:
    /// the tenant logo is a PUT because there is one of them, and a KB
    /// attachment is a POST because an article has many. Everything the doc
    /// comment there says about leaving `Content-Type` alone applies here for
    /// the same reason - setting it by hand omits the boundary parameter and
    /// the server cannot split the body.
    #[cfg(feature = "app")]
    pub async fn post_file_authed<T: DeserializeOwned>(
        path: &str,
        file_name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let token = current_access_token()
            .ok_or_else(|| ApiError::Network("not authenticated".to_string()))?;

        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .multipart_file(file_name, mime, bytes)
            .map_err(network_err)?
            .send()
            .await
            .map_err(network_err)?;
        if resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    // --- Auto-authed wrappers --------------------------------------------
    //
    // These read the current access token from the thread-local holder so
    // page code does not have to thread it through. If the user is not
    // signed in (`ACCESS_TOKEN` is None) we send the request without an
    // Authorization header and let the server's 401 surface naturally;
    // the OIDC SPA pattern then redirects to the login page.
    //
    // MAPPS-435: each renews a spent token before it reads the holder, so the
    // bearer that goes out is one the SPA believes in. The 401 recovery lives
    // one level down, in the `_with_auth` helpers, which is also where the
    // page code that resolves its own bearer arrives.

    #[cfg(feature = "app")]
    pub async fn get_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        ensure_fresh_access_token().await;
        match current_access_token() {
            Some(t) => get_with_auth(path, &t).await,
            None => get(path).await,
        }
    }

    /// Auto-authed sibling of [`get_all_with_auth`]: reads a whole list,
    /// paging until a short page (MAPPS-528).
    #[cfg(feature = "app")]
    pub async fn get_all_authed<T: DeserializeOwned>(path: &str) -> Result<Vec<T>, String> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        get_all_with_auth(path, &t).await
    }

    #[cfg(feature = "app")]
    pub async fn post_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        ensure_fresh_access_token().await;
        match current_access_token() {
            Some(t) => post_with_auth(path, body, &t).await,
            None => post(path, body).await,
        }
    }

    #[cfg(feature = "app")]
    pub async fn put_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        put_with_auth(path, body, &t).await
    }

    #[cfg(feature = "app")]
    pub async fn delete_authed(path: &str) -> Result<(), String> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        delete_with_auth(path, &t).await
    }

    #[cfg(feature = "app")]
    pub async fn patch_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        patch_with_auth(path, body, &t).await
    }

    /// Auto-authed POST for empty-body endpoints (see
    /// `post_no_content_with_auth`).
    #[cfg(feature = "app")]
    pub async fn post_authed_no_content(path: &str) -> Result<(), String> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        post_no_content_with_auth(path, &t).await
    }

    // --- Portal-authed wrappers (MAPPS-395) ------------------------------
    //
    // The `/portal/*` tree is guarded by mokosh-server's
    // `portal_auth_middleware`, which decodes the bearer and rejects anything
    // whose `typ` is not `portal_access`. Sending the agent bearer there is a
    // guaranteed 401, so these helpers read ONLY `PORTAL_ACCESS_TOKEN` and
    // fail fast when there is no portal session instead of falling back to the
    // agent token or firing an anonymous request. They are the only functions
    // that may read the portal token (pinned by the tests at the bottom of
    // this file).

    /// Error for a `/portal/*` call made with no portal session. Surfaced
    /// through the same `Result<_, String>` channel the pages already render.
    #[cfg(feature = "app")]
    fn portal_not_signed_in() -> String {
        "not signed in to the portal".to_string()
    }

    #[cfg(feature = "app")]
    pub async fn get_portal_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        get_with_auth(path, &t).await
    }

    /// Portal sibling of [`get_all_authed`]: reads a whole `/portal/*` list
    /// on the portal bearer, paging until a short page (MAPPS-528).
    #[cfg(feature = "app")]
    pub async fn get_all_portal_authed<T: DeserializeOwned>(path: &str) -> Result<Vec<T>, String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        get_all_with_auth(path, &t).await
    }

    #[cfg(feature = "app")]
    pub async fn post_portal_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        post_with_auth(path, body, &t).await
    }

    /// Typed sibling of [`post_portal_authed`], for the portal call sites that
    /// need the status code (the ticket reply form).
    #[cfg(feature = "app")]
    pub async fn post_portal_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_portal_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// PMS-729 phase 2 §7 slice B / I2: portal-authed multipart POST for
    /// attaching a file to one of the customer's own ticket notes. The
    /// browser sets the `Content-Type: multipart/form-data; boundary=...`
    /// header itself from the `FormData` body, so this helper deliberately
    /// omits it - overriding it here would strip the boundary and the
    /// server would 400.
    #[cfg(feature = "web")]
    pub async fn post_portal_authed_multipart<T: DeserializeOwned>(
        path: &str,
        form: &web_sys::FormData,
    ) -> Result<T, ApiError> {
        let t = current_portal_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .body(form)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-618 phase B: staff-authed PUT with a multipart body.
    /// Powers the Company-scoped logo / favicon / background upload
    /// (`PUT /api/v1/companies/{id}/{asset}`). Deliberately omits the
    /// `Content-Type` header - the browser sets it (with the
    /// `boundary=...` parameter) from the `FormData` body itself.
    #[cfg(feature = "web")]
    pub async fn put_authed_multipart<T: DeserializeOwned>(
        path: &str,
        form: &web_sys::FormData,
    ) -> Result<T, ApiError> {
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .body(form)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-618 phase B: contact-authed PUT with a multipart body.
    /// Powers the same asset uploads on the contact plane
    /// (`PUT /api/v1/contact/companies/self/{asset}`), gated on the
    /// caller holding `settings:manage_company_branding`.
    #[cfg(feature = "web")]
    pub async fn put_contact_authed_multipart<T: DeserializeOwned>(
        path: &str,
        form: &web_sys::FormData,
    ) -> Result<T, ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .body(form)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// PMS-729 follow-up: portal-authed PUT with a JSON body that returns 204.
    /// Used by change-password. Surfaces `ApiError` so the caller can key on
    /// specific status codes (401 = current password wrong, 400 = new
    /// password fails policy) with typed `.message` for the server text.
    #[cfg(feature = "web")]
    pub async fn put_portal_authed_json_no_content<B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<(), ApiError> {
        let t = current_portal_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        // Reuse the standard 4xx envelope parse: `crate::utils::error::ErrorResponse`
        // is the shape mokosh-server ships for every validation / auth failure.
        let body_text = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body_text) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body_text).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body_text.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    /// Portal-authed PATCH with a JSON body that returns 204. Mirrors
    /// [`put_portal_authed_json_no_content`] for the profile self-edit
    /// path (`PATCH /portal/auth/me`).
    #[cfg(feature = "web")]
    pub async fn patch_portal_authed_json_no_content<B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<(), ApiError> {
        let t = current_portal_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body_text = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body_text) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body_text).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body_text.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    /// Portal-authed POST that discards the response body. Used by
    /// mutating endpoints that respond 204 (`/portal/company/contacts/
    /// {id}/resend-invite`, ...). Surfaces `ApiError` so the caller
    /// can branch on the envelope code without a second parse.
    #[cfg(feature = "web")]
    pub async fn post_portal_authed_no_content(path: &str) -> Result<(), ApiError> {
        let t = current_portal_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body_text = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body_text) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body_text).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body_text.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    /// PMS-729 phase 2 §7 slice B / I12: portal-authed empty-body PUT.
    /// Used by the inbox mark-read call (`PUT
    /// /portal/notifications/{id}/read` responds 204 with no payload).
    /// Only checks the status; a 4xx surfaces through the standard
    /// `status_error` string.
    #[cfg(feature = "web")]
    pub async fn put_portal_authed_no_content(path: &str) -> Result<(), String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(|e| {
                super::note_transport_error();
                e.to_string()
            })?;
        super::note_response_status(resp.status());
        if resp.ok() {
            Ok(())
        } else {
            Err(status_error(resp).await)
        }
    }

    /// PMS-729 phase 2 §7 slice D / I18: portal-authed DELETE that
    /// discards the response body. Used by the delegation revoke path
    /// (`DELETE /portal/company/delegations/{id}` responds 204). Only
    /// checks the status; a 4xx surfaces through `status_error`.
    #[cfg(feature = "web")]
    pub async fn delete_portal_authed_no_content(path: &str) -> Result<(), String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::delete(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(|e| {
                super::note_transport_error();
                e.to_string()
            })?;
        super::note_response_status(resp.status());
        if resp.ok() {
            Ok(())
        } else {
            Err(status_error(resp).await)
        }
    }

    /// PMS-729 phase 2 §7 slice B / I2: portal-authed download of a raw
    /// response body plus the server's `Content-Disposition` filename. The
    /// SPA holds the portal bearer in WASM memory so an attachment cannot
    /// be reached via a plain `<a href>`; this helper is what the
    /// `PortalAttachmentLink` handler pipes into a Blob URL.
    #[cfg(feature = "web")]
    pub async fn get_portal_authed_bytes(path: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::get(&url)
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(|e| {
                super::note_transport_error();
                e.to_string()
            })?;
        super::note_response_status(resp.status());
        if !resp.ok() {
            return Err(status_error(resp).await);
        }
        let filename = resp
            .headers()
            .get("content-disposition")
            .as_deref()
            .and_then(content_disposition_filename);
        let bytes = resp.binary().await.map_err(|e| e.to_string())?;
        Ok((bytes, filename))
    }

    // --- Contact-authed wrappers (mokosh-contact-login, prompt 005) ------
    //
    // Contact JWT (`typ: "contact"`) minted at
    // `POST /api/v1/contact/auth/login`. Every `/api/v1/contact/*`
    // extractor (`RequireContactAuth`) rejects any bearer whose typ is
    // not "contact", so these helpers read ONLY `CONTACT_ACCESS_TOKEN`
    // and fail fast with a 401 when no contact session is held, rather
    // than falling back to the staff bearer or firing an anonymous
    // request.

    #[cfg(feature = "web")]
    fn contact_not_signed_in_api() -> ApiError {
        ApiError::Status {
            code: 401,
            message: "not signed in to the contact portal".to_string(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        }
    }

    #[cfg(feature = "web")]
    pub async fn get_contact_authed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::get(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn post_contact_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn post_contact_authed_no_content(path: &str) -> Result<(), ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body_text = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body_text) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body_text).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body_text.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    #[cfg(feature = "web")]
    pub async fn put_contact_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-620 (mokosh-branding prompt 004): contact-plane PATCH.
    /// Powers `PATCH /contact/companies/self/branding` (JSONB merge
    /// subset of the caller's own Company branding). Same shape as
    /// [`put_contact_authed_typed`]; separate verb because the server
    /// gates PATCH separately from PUT.
    #[cfg(feature = "web")]
    pub async fn patch_contact_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_contact_authed_no_content(path: &str) -> Result<(), ApiError> {
        let t = current_contact_access_token().ok_or_else(contact_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body_text = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body_text) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body_text).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body_text.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    // --- Session-agnostic authed helpers (MAPPS-604) -------------------
    //
    // The prompt 013 pages (dashboard, tickets, billing, quotes,
    // contracts, assets, projects) share ONE URL space between the staff
    // workspace and the contact portal: `RequireCallerContext` on the
    // server accepts either bearer and scopes the response accordingly.
    // These wrappers pick the RIGHT bearer for the caller: contact
    // session first when held (so the server sees `typ: "contact"` and
    // filters to `company_id = contact.company_id`), else staff bearer,
    // else unauth (mirrors `get_authed`'s legacy anon fall-through so a
    // no-session caller does not spuriously 401 the whole page).

    /// GET the same path with whichever bearer the caller holds today.
    /// Prefers the contact bearer over the staff bearer so a contact
    /// session on a URL that ALSO renders for staff routes through the
    /// contact identity and lands on the contact-scoped filter path.
    #[cfg(feature = "web")]
    pub async fn get_authed_any<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        if let Some(t) = current_contact_access_token() {
            return get_with_auth(path, &t).await;
        }
        match current_access_token() {
            Some(t) => get_with_auth(path, &t).await,
            None => get(path).await,
        }
    }

    /// Typed sibling of [`get_authed_any`]. Same contact-first bearer
    /// selection; surfaces `ApiError::Status` so a page can branch on
    /// 401 / 403 / 404 without re-parsing the envelope.
    #[cfg(feature = "web")]
    pub async fn get_authed_any_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        let bearer = current_contact_access_token().or_else(current_access_token);
        if let Some(t) = bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-607: typed POST that picks the caller's bearer (contact
    /// first, staff second, anon otherwise) the same way
    /// [`get_authed_any_typed`] does. Used by the ticket detail's
    /// Reopen and Attach controls and the asset detail's Report an
    /// Issue, all of which sit on dual-plane routes gated per-cap on
    /// the server. Surfaces `ApiError::Status` so a caller can branch
    /// on 403 (missing cap) or 501 (not implemented yet) without
    /// re-parsing the envelope.
    #[cfg(feature = "web")]
    pub async fn post_authed_any_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        let bearer = current_contact_access_token().or_else(current_access_token);
        if let Some(t) = bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-609: typed PATCH that picks the caller's bearer (contact
    /// first, staff second, anon otherwise) the same way
    /// [`post_authed_any_typed`] does. Used by the ticket detail's
    /// contact-visible Edit button, which fires `PATCH /tickets/{id}`
    /// with `{ title, description }` on a dual-plane route gated
    /// per-cap on the server. Surfaces `ApiError::Status` so a caller
    /// can branch on 403 (missing cap / not-owner) without re-parsing
    /// the envelope.
    #[cfg(feature = "web")]
    pub async fn patch_authed_any_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::patch(&url).header("Content-Type", "application/json");
        let bearer = current_contact_access_token().or_else(current_access_token);
        if let Some(t) = bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-607: bytes GET that picks the caller's bearer (contact
    /// first, staff second, anon otherwise) - shape identical to
    /// [`get_authed_bytes`] except for the two-bearer selection. Used
    /// by the invoice and quote detail Download PDF controls, both on
    /// dual-plane routes. Surfaces the raw response body plus the
    /// server's `Content-Disposition` filename, or `ApiError::Status`
    /// so a caller can branch on 501 (PDF generation stubbed out) and
    /// render the fallback copy inline.
    #[cfg(feature = "web")]
    pub async fn get_authed_any_bytes(path: &str) -> Result<(Vec<u8>, Option<String>), ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url);
        let bearer = current_contact_access_token().or_else(current_access_token);
        if let Some(t) = bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if !(200..300).contains(&status) {
            let body = resp.text().await.unwrap_or_default();
            let (message, fields, envelope_code, envelope_body) =
                match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                    Ok(env) => {
                        let code = env.error.code.clone();
                        let raw = serde_json::from_str::<serde_json::Value>(&body).ok();
                        (
                            env.error.message,
                            env.error.errors.unwrap_or_default(),
                            code,
                            raw,
                        )
                    }
                    Err(_) => (
                        body.chars().take(200).collect(),
                        Vec::new(),
                        String::new(),
                        None,
                    ),
                };
            return Err(ApiError::Status {
                code: status,
                message,
                fields,
                envelope_code,
                envelope_body,
            });
        }
        let filename = resp
            .headers()
            .get("content-disposition")
            .as_deref()
            .and_then(content_disposition_filename);
        let bytes = resp
            .binary()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok((bytes, filename))
    }

    // --- Platform-authed wrappers (MAPPS-518) ---------------------------
    //
    // MAPPS-518: the platform super-admin persona lives in
    // `platform_admins` on the server, with its own JWT typ
    // (`"platform"`) and its own bearer minted at
    // `POST /api/v1/platform/login`. The client stashes that bearer
    // under `PLATFORM_TOKEN_KEY` in `sessionStorage` (see
    // `pages::platform_login`); the tenant `ACCESS_TOKEN` holder is
    // untouched. Every tenant-management endpoint that used to be
    // `RequireSuperAdmin` (list/create tenants, suspend/activate,
    // get/update tenant admin, resend welcome) is now
    // `RequirePlatformAdmin`, so the SPA MUST send the platform
    // bearer instead of the tenant bearer on those calls or the
    // server returns 401.
    //
    // These wrappers read ONLY the platform-token slot in
    // sessionStorage; they never fall back to the tenant bearer
    // (mirroring the portal-authed helpers above).

    /// MAPPS-518: the sessionStorage key `/platform/login` writes to.
    /// Kept in sync with `pages::platform_login::PLATFORM_TOKEN_KEY`.
    #[cfg(feature = "web")]
    const PLATFORM_TOKEN_KEY: &str = "mokosh:platform_token";

    /// MAPPS-518: read the current platform-admin bearer from
    /// sessionStorage. `None` when the operator has not signed in on
    /// `/platform/login` (or the browser blocks sessionStorage).
    #[cfg(feature = "web")]
    pub fn current_platform_access_token() -> Option<String> {
        let win = web_sys::window()?;
        let store = win.session_storage().ok()??;
        let token = store.get_item(PLATFORM_TOKEN_KEY).ok()??;
        if token.trim().is_empty() {
            None
        } else {
            Some(token)
        }
    }

    #[cfg(feature = "web")]
    fn platform_not_signed_in() -> String {
        "not signed in as a platform admin".to_string()
    }

    #[cfg(feature = "web")]
    fn platform_not_signed_in_api() -> ApiError {
        ApiError::Status {
            code: 401,
            message: platform_not_signed_in(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        }
    }

    #[cfg(feature = "web")]
    pub async fn get_platform_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let t = current_platform_access_token().ok_or_else(platform_not_signed_in)?;
        get_with_auth(path, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn get_platform_authed_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let t = current_platform_access_token().ok_or_else(platform_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::get(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn post_platform_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_platform_access_token().ok_or_else(platform_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn put_platform_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_platform_access_token().ok_or_else(platform_not_signed_in_api)?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn post_platform_authed_no_content(path: &str) -> Result<(), String> {
        let t = current_platform_access_token().ok_or_else(platform_not_signed_in)?;
        post_no_content_with_auth(path, &t).await
    }

    // --- Typed error layer ----------------------------------------------
    //
    // The string-returning helpers above are kept so existing callers
    // (companies/calendar/system_version) compile unchanged. New call
    // sites should prefer the `_typed` variants, which return an
    // `ApiError` enum that pages can map into a user-facing toast via
    // `ApiError::user_message()`.

    /// Typed HTTP error returned from the `_typed` API helpers.
    ///
    /// Variants are deliberately coarse - the goal is to give callers
    /// enough signal to render an actionable toast without forcing them
    /// to match on every possible status code.
    #[cfg(feature = "app")]
    #[derive(Debug, Clone)]
    pub enum ApiError {
        /// Transport failed before a response was received.
        Network(String),
        /// Server returned a non-2xx status. `message` is the server's
        /// `error.message` field when it parsed, otherwise the raw body
        /// truncated to a sensible length. `fields` carries the
        /// `error.errors[]` validation envelope (field / message / code)
        /// when present, so forms can surface a message next to the
        /// offending input instead of only a status string (MAPPS-210).
        Status {
            code: u16,
            message: String,
            fields: Vec<crate::utils::error::FieldError>,
            /// Server envelope's `error.code` (the string identifier), when
            /// present. Distinct from the HTTP `code` (the numeric status)
            /// above so callers can branch on the domain-level signal
            /// (`CAPTCHA_REQUIRED`, `ACCOUNT_DELETED`, ...) without
            /// pattern-matching on human-facing message text.
            envelope_code: String,
            /// Raw JSON body of the error response, preserved when the
            /// caller needs a subfield the typed shape does not carry
            /// (e.g. the portal-login CAPTCHA `error.captcha.site_key`).
            /// Empty when the body did not parse as JSON.
            envelope_body: Option<serde_json::Value>,
        },
        /// Response was 2xx but the body could not be decoded into the
        /// target type.
        Decode(String),
    }

    #[cfg(feature = "app")]
    impl ApiError {
        /// User-facing message suitable for a toast.
        pub fn user_message(&self) -> String {
            match self {
                Self::Network(_) => "Network error. Check your connection and try again.".into(),
                Self::Status {
                    code,
                    message,
                    fields,
                    ..
                } => match *code {
                    // Server sometimes ships a specific message on
                    // 401/403 that the user must see verbatim (e.g.
                    // suspended tenant -> "This organization is not
                    // active", account deactivated, MFA required
                    // mid-session). Keep those. Replace the generic
                    // "Authentication required" / "Access denied"
                    // envelope with the plane-aware permission
                    // message so a signed-in caller stops seeing
                    // "your session has expired" for a plain
                    // permission miss (MAPPS-624).
                    401 | 403 if !is_generic_auth_message(*code, message) => message.clone(),
                    401 | 403 => permission_message(),
                    404 => "The requested resource was not found.".into(),
                    409 if !message.is_empty() => message.clone(),
                    // Surface the field-level validation messages when the
                    // server returned them, so a 422 reads as the actual
                    // rule ("Title must be ...") instead of the generic
                    // envelope message (MAPPS-210).
                    422 if !fields.is_empty() => fields
                        .iter()
                        .map(|f| f.message.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                    422 if !message.is_empty() => message.clone(),
                    429 => "Too many requests. Please try again shortly.".into(),
                    500..=599 => "The server hit an error. Please try again.".into(),
                    _ if !message.is_empty() => message.clone(),
                    _ => format!("Request failed ({}).", code),
                },
                Self::Decode(_) => {
                    "The server's response could not be read. Please refresh and retry.".into()
                }
            }
        }

        /// Status code if the error was a non-2xx HTTP response.
        pub fn status_code(&self) -> Option<u16> {
            match self {
                Self::Status { code, .. } => Some(*code),
                _ => None,
            }
        }

        /// Server-provided field-level validation errors, if any
        /// (MAPPS-210). Empty for transport / decode errors and for
        /// non-2xx responses that carried no `error.errors[]` envelope.
        pub fn field_errors(&self) -> &[crate::utils::error::FieldError] {
            match self {
                Self::Status { fields, .. } => fields,
                _ => &[],
            }
        }

        /// Validation message the server attached to a specific form field,
        /// if present. Lets a form route the message next to the offending
        /// input (MAPPS-210).
        pub fn field_message(&self, field: &str) -> Option<String> {
            self.field_errors()
                .iter()
                .find(|fe| fe.field == field)
                .map(|fe| fe.message.clone())
        }
    }

    #[cfg(feature = "app")]
    impl std::fmt::Display for ApiError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Network(e) => write!(f, "network error: {e}"),
                Self::Status { code, message, .. } => {
                    if message.is_empty() {
                        write!(f, "http {code}")
                    } else {
                        write!(f, "http {code}: {message}")
                    }
                }
                Self::Decode(e) => write!(f, "decode error: {e}"),
            }
        }
    }

    #[cfg(feature = "app")]
    async fn handle_response<T: DeserializeOwned>(
        response: crate::platform::http::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();
        // Any response clears the "down" state; a 5xx sets it (MAPPS-333).
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return response
                .json::<T>()
                .await
                .map_err(|e| ApiError::Decode(e.to_string()));
        }
        let body = response.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                Ok(env) => {
                    // MAPPS-348: mirror `status_error`'s terminal-state
                    // detection on the typed path so a fetch on either
                    // helper flips the global `ACCOUNT_DELETED` signal.
                    if status == 410 && env.error.code == "ACCOUNT_DELETED" {
                        super::note_account_deleted();
                    }
                    let code = env.error.code.clone();
                    // Keep the raw parse around so callers who need a
                    // sub-object (`error.captcha.site_key` on the portal
                    // login CAPTCHA challenge) can pluck it out without
                    // a second parse.
                    let raw = serde_json::from_str::<serde_json::Value>(&body).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                // Fall back to the raw body, capped so a runaway HTML
                // 500 page doesn't end up in a toast.
                Err(_) => (
                    body.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    #[cfg(feature = "app")]
    pub async fn get_authed_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        let bearer = current_access_token();
        if let Some(t) = &bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(network_err)?;
        // Only when we actually sent the agent bearer: an anonymous 401 is the
        // caller's to render, not a session event.
        if bearer.is_some() && resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    /// Bearer-authed GET that returns the raw response body plus the server's
    /// `Content-Disposition` filename. Used for attachment downloads the SPA
    /// cannot fetch through a plain `<a href>` because the bearer lives in WASM
    /// memory rather than a cookie (the data export, MAPPS-364).
    #[cfg(feature = "app")]
    pub async fn get_authed_bytes(path: &str) -> Result<(Vec<u8>, Option<String>), String> {
        ensure_fresh_access_token().await;
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url);
        let bearer = current_access_token();
        if let Some(t) = &bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(|e| {
            // Keep the server-reachable flag accurate on a transport failure,
            // like the other helpers do via `network_err`.
            super::note_transport_error();
            e.to_string()
        })?;
        // Any response clears the "down" state (a 5xx re-flags it).
        super::note_response_status(resp.status());
        if bearer.is_some() && resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        if !resp.ok() {
            return Err(status_error(resp).await);
        }
        let filename = resp
            .headers()
            .get("content-disposition")
            .as_deref()
            .and_then(content_disposition_filename);
        let bytes = resp.binary().await.map_err(|e| e.to_string())?;
        Ok((bytes, filename))
    }

    /// Extract the `filename="..."` value from a `Content-Disposition` header.
    /// Only the simple quoted or unquoted `filename=` form is handled (no
    /// RFC 5987 `filename*=`), which is all the export endpoint emits.
    #[cfg(feature = "app")]
    fn content_disposition_filename(header: &str) -> Option<String> {
        let idx = header.to_ascii_lowercase().find("filename=")?;
        let raw = header[idx + "filename=".len()..].trim();
        let value = if let Some(rest) = raw.strip_prefix('"') {
            rest.split('"').next().unwrap_or("")
        } else {
            raw.split(';').next().unwrap_or(raw).trim()
        };
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    #[cfg(feature = "app")]
    pub async fn post_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        let bearer = current_access_token();
        if let Some(t) = &bearer {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        if bearer.is_some() && resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    /// MAPPS-368: unauthed typed POST, for the standalone login form. Same as
    /// [`post_authed_typed`] but sends no bearer (the user is not signed in
    /// yet), so the caller can inspect `ApiError::Status { code, .. }` and map
    /// 401 -> "invalid credentials" / 429 -> "too many attempts".
    #[cfg(feature = "app")]
    pub async fn post_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        if let Some(host) = current_forwarded_host() {
            req = req.header("X-Forwarded-Host", &host);
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// PMS-730: unauthed typed GET, for the public client request-form page.
    /// [`get_authed_typed`] would attach whatever bearer happens to be in
    /// memory, and the plain [`get`] collapses every failure to a `String`, so
    /// the caller could not tell 410 (link already submitted) from 400
    /// (expired or unknown). This keeps the typed error.
    #[cfg(feature = "app")]
    pub async fn get_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        if let Some(host) = current_forwarded_host() {
            req = req.header("X-Forwarded-Host", &host);
        }
        let resp = req.send().await.map_err(network_err)?;
        handle_response(resp).await
    }

    /// MAPPS-396: unauthed typed POST for an endpoint that answers 204 with no
    /// body (`POST /portal/auth/setup-password`). [`post_typed`] would fail to
    /// decode the empty payload, so this variant only inspects the status and
    /// keeps the typed error so the caller can tell 410 (replayed link) from
    /// 400 (expired / unknown link).
    ///
    /// MAPPS-554 fix (2026-08-24 operator report: "Forget password in
    /// client portal doesn't actually work"): attach `X-Forwarded-Host`
    /// to match `post_typed` / `get_typed`. Without it, the portal's
    /// `POST /portal/auth/forgot-password` sees `Host: server:8080`
    /// (the Dioxus dev proxy rewrite) instead of the real
    /// `{slug}.client.<apex>` the browser is visiting; `lookup_host_tenant`
    /// then fails silently and the handler returns 204 with no
    /// email dispatched. Same reason `post_typed` already attaches it.
    #[cfg(feature = "web")]
    pub async fn post_typed_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        if let Some(host) = current_forwarded_host() {
            req = req.header("X-Forwarded-Host", &host);
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body = resp.text().await.unwrap_or_default();
        let (message, fields, envelope_code, envelope_body) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                Ok(env) => {
                    let code = env.error.code.clone();
                    let raw = serde_json::from_str::<serde_json::Value>(&body).ok();
                    (
                        env.error.message,
                        env.error.errors.unwrap_or_default(),
                        code,
                        raw,
                    )
                }
                Err(_) => (
                    body.chars().take(200).collect(),
                    Vec::new(),
                    String::new(),
                    None,
                ),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
            envelope_code,
            envelope_body,
        })
    }

    #[cfg(feature = "app")]
    pub async fn put_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        if resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    /// PMS-731: typed PATCH. The forms surface (and the rest of the server's
    /// partial-update routes) is PATCH rather than PUT, and only the
    /// `String`-error `patch_authed` existed, which loses the per-field 422
    /// envelope the editor needs to report a bad field set.
    #[cfg(feature = "app")]
    pub async fn patch_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        if resp.status() == 401 {
            note_agent_unauthorized().await;
        }
        handle_response(resp).await
    }

    #[cfg(feature = "app")]
    pub async fn delete_authed_typed(path: &str) -> Result<(), ApiError> {
        ensure_fresh_access_token().await;
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if status == 401 {
            note_agent_unauthorized().await;
        }
        if (200..300).contains(&status) {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            let (message, fields, envelope_code, envelope_body) =
                match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                    Ok(env) => {
                        let code = env.error.code.clone();
                        let raw = serde_json::from_str::<serde_json::Value>(&body).ok();
                        (
                            env.error.message,
                            env.error.errors.unwrap_or_default(),
                            code,
                            raw,
                        )
                    }
                    Err(_) => (
                        body.chars().take(200).collect(),
                        Vec::new(),
                        String::new(),
                        None,
                    ),
                };
            Err(ApiError::Status {
                code: status,
                message,
                fields,
                envelope_code,
                envelope_body,
            })
        }
    }
}

/// mokosh-contact-login: MAPPS-395 portal-lane tests removed with
/// prompt 001 (`src/pages/portal.rs` retired; the `include_str!` below
/// would fail at compile time otherwise). Contact-plane replacement
/// lands in prompt 004.
///
/// Left in the tree under an always-false `any()` cfg so the shape is
/// visible to whoever writes the contact-plane replacement, without
/// participating in `cargo test`. The `strip_api_version` /
/// `normalize_api_base` unit tests main added live in the pure helpers
/// where they belong (still compiled), so nothing on the main side is
/// lost.
#[cfg(all(test, any()))]
mod tests_RETIRED {
    use super::api::{normalize_api_base, strip_api_version};

    const FETCH_SRC: &str = include_str!("fetch.rs");
    const PORTAL_PAGE_SRC: &str = include_str!("../pages/portal.rs");

    /// The only functions allowed to touch the portal token holder.
    const PORTAL_TOKEN_READERS: &[&str] = &[
        "set_portal_access_token",
        "current_portal_access_token",
        "has_portal_session",
        "get_portal_authed",
        "get_all_portal_authed",
        "post_portal_authed",
        "post_portal_authed_typed",
        "post_portal_authed_multipart",
        "get_portal_authed_bytes",
        "put_portal_authed_no_content",
        "put_portal_authed_json_no_content",
        "patch_portal_authed_json_no_content",
        "post_portal_authed_no_content",
        "delete_portal_authed_no_content",
    ];

    /// Agent-token helpers. None of them may appear in the portal page: a
    /// `typ: "access"` bearer is a guaranteed 401 on every `/portal/*` route.
    const AGENT_HELPERS: &[&str] = &[
        "get_authed",
        "post_authed",
        "put_authed",
        "patch_authed",
        "delete_authed",
        "current_access_token",
    ];

    /// This file minus its test module, which names the same symbols in its
    /// own fixtures.
    fn production_src() -> &'static str {
        FETCH_SRC
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields a first segment")
    }

    /// The only bearer-sending helper that is NOT on the agent lane: it reads
    /// the portal holder, and a `/portal/*` 401 belongs to that identity.
    const PORTAL_BEARER_SENDERS: &[&str] = &["post_portal_authed_typed"];

    /// Helpers that deliberately send no bearer. A 401 from one of them is the
    /// caller's to render (wrong password on `POST /auth/login`, an expired
    /// public link), never a signal to end the agent session.
    const UNAUTHENTICATED_HELPERS: &[&str] = &[
        "get",
        "post",
        "post_typed",
        "get_typed",
        "post_typed_no_content",
    ];

    /// Name of the function a `fn` line declares, if it declares one.
    fn fn_name(trimmed: &str) -> Option<String> {
        let idx = if trimmed.starts_with("fn ") {
            0
        } else {
            trimmed.find(" fn ")? + 1
        };
        let name: String = trimmed[idx + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    }

    /// Every `fn` in the production half of this file, paired with its source.
    /// The body runs to the next `fn` line, which is enough to attribute the
    /// lines the scans below look for.
    fn function_bodies() -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for line in production_src().lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("//") {
                if let Some(name) = fn_name(trimmed) {
                    out.push((name, String::new()));
                }
            }
            if let Some((_, body)) = out.last_mut() {
                body.push_str(line);
                body.push('\n');
            }
        }
        out
    }

    fn body_of(name: &str) -> String {
        function_bodies()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} is defined in this file"))
            .1
    }

    /// MAPPS-435 recurrence gate, and the invariant sweep in code: a request
    /// that carries the agent bearer renews a spent token BEFORE it sends, and
    /// treats the 401 it may still earn as a session event rather than page
    /// copy. Derived from the shape of the code (an `Authorization` header)
    /// rather than from the list of helpers that existed when this was
    /// written, so the next helper added has to classify itself.
    #[test]
    fn every_agent_bearer_sender_renews_and_recovers() {
        let mut checked = 0;
        for (name, body) in function_bodies() {
            if !body.contains(".header(\"Authorization\"") {
                continue;
            }
            if PORTAL_BEARER_SENDERS.contains(&name.as_str()) {
                assert!(
                    !body.contains("note_agent_unauthorized"),
                    "{name} sends the portal session token; its 401 must leave the agent \
                     session alone"
                );
                continue;
            }
            checked += 1;
            assert!(
                body.contains("ensure_fresh_access_token") || body.contains("agent_lane_bearer"),
                "{name} sends the agent bearer without renewing a spent one first: a tab \
                 restored after a suspend would send a token it already knows is dead"
            );
            assert!(
                body.contains("note_agent_unauthorized"),
                "{name} sends the agent bearer but leaves its 401 as page copy: nothing \
                 refreshes, nothing signs out, and the screen sits on the error"
            );
        }
        assert!(
            checked >= 13,
            "the scan found only {checked} agent-bearer senders; it used to find 13, so it \
             has stopped matching the code it polices"
        );
    }

    /// The other half of the boundary: a helper that sends no bearer, or the
    /// portal one, never reaches the agent sign-out path. The portal wrappers
    /// delegate to `get_with_auth` / `post_with_auth`, whose recovery is gated
    /// on the token being the one in the agent holder (`is_agent_bearer`), so
    /// the portal token passes straight through it.
    #[test]
    fn the_portal_and_unauthenticated_helpers_never_end_the_agent_session() {
        for name in UNAUTHENTICATED_HELPERS
            .iter()
            .chain(PORTAL_BEARER_SENDERS)
            .chain(
                [
                    "get_portal_authed",
                    "get_all_portal_authed",
                    "post_portal_authed",
                    "post_portal_authed_no_content",
                ]
                .iter(),
            )
        {
            let body = body_of(name);
            assert!(
                !body.contains("note_agent_unauthorized"),
                "{name} must not end the agent session: a 401 there means wrong password, \
                 an expired link, or a portal identity"
            );
        }
    }

    #[test]
    fn only_the_portal_helpers_read_the_portal_token() {
        let mut current_fn = String::new();
        let mut offenders: Vec<String> = Vec::new();
        for line in production_src().lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(name) = fn_name(trimmed) {
                current_fn = name;
            }
            if !line.contains("PORTAL_ACCESS_TOKEN")
                && !line.contains("current_portal_access_token")
            {
                continue;
            }
            // The holder's own declaration sits in a `thread_local!` block,
            // outside any function.
            if trimmed.starts_with("static PORTAL_ACCESS_TOKEN") {
                continue;
            }
            if !PORTAL_TOKEN_READERS.contains(&current_fn.as_str()) {
                offenders.push(format!("{current_fn}(): {trimmed}"));
            }
        }
        assert!(
            offenders.is_empty(),
            "only {PORTAL_TOKEN_READERS:?} may read the portal token, but it is read by: {offenders:?}"
        );
    }

    #[test]
    fn agent_helpers_never_read_the_portal_token() {
        // The mirror of the test above: the agent-side auto-authed wrappers
        // resolve their bearer through `current_access_token` only, so
        // `get_authed` cannot send a portal token even when one is held.
        for helper in ["get_authed", "post_authed", "put_authed"] {
            let body = production_src()
                .split(&format!("pub async fn {helper}"))
                .nth(1)
                .unwrap_or_else(|| panic!("{helper} is defined in this file"));
            let body = &body[..body.find("\n    #[cfg").unwrap_or(body.len())];
            assert!(
                !body.contains("portal"),
                "{helper} must not reference the portal token: {body}"
            );
        }
    }

    #[cfg(feature = "app")]
    #[test]
    fn the_token_holders_are_independent() {
        use super::api::{
            current_access_token, current_portal_access_token, set_portal_access_token,
        };
        set_portal_access_token(Some("portal-token".to_string()));
        assert_eq!(
            current_portal_access_token().as_deref(),
            Some("portal-token")
        );
        // The bearer every agent helper sends is unchanged by a portal
        // sign-in: the two holders are separate cells.
        assert_eq!(
            current_access_token(),
            None,
            "a portal sign-in must not populate the agent token"
        );
        set_portal_access_token(None);
        assert_eq!(current_portal_access_token(), None);
    }

    #[test]
    fn portal_pages_use_only_the_portal_helpers() {
        for helper in AGENT_HELPERS {
            assert!(
                !PORTAL_PAGE_SRC.contains(helper),
                "src/pages/portal.rs must not reference `{helper}`: every /portal/* route \
                 rejects the agent bearer, so portal fetches go through the \
                 `_portal_authed` helpers"
            );
        }
    }

    /// PMS-758: `logo_url` arrives as a path from the ORIGIN, so joining it to
    /// the versioned API base produced `/api/v1/api/v1/...` and a broken image
    /// everywhere the logo appeared.
    #[test]
    fn the_api_origin_drops_the_version_prefix() {
        assert_eq!(
            strip_api_version("https://api.msp.a8n.systems/api/v1"),
            "https://api.msp.a8n.systems"
        );
        assert_eq!(
            strip_api_version("/api/v1"),
            "",
            "dev is same-origin, where an origin-relative path is already right"
        );
        assert_eq!(
            strip_api_version("https://api.example.test/custom"),
            "https://api.example.test/custom",
            "a base that is not version-suffixed is left alone rather than truncated"
        );
    }

    /// PMS-751: staging is configured with a trailing slash, which every join
    /// site turns into `/api/v1//tenants/...`. It survives only because
    /// something in front of the server collapses the duplicate.
    #[test]
    fn a_configured_api_base_never_keeps_a_trailing_slash() {
        assert_eq!(
            normalize_api_base("https://api.msp.a8n.systems/api/v1/"),
            "https://api.msp.a8n.systems/api/v1",
            "this is the value staging actually serves"
        );
        assert_eq!(
            normalize_api_base("https://api.msp.a8n.systems/api/v1"),
            "https://api.msp.a8n.systems/api/v1",
            "a correctly configured base is left alone"
        );
        assert_eq!(
            normalize_api_base("https://api.example.test/api/v1///"),
            "https://api.example.test/api/v1"
        );
    }

    /// MAPPS-528: the client's copy of the cap is the server's constant, not
    /// a number typed twice. If mokosh-server moves `MAX_PER_PAGE`, the
    /// mirror in `src/utils/pagination.rs` moves with it and the `get_all_*`
    /// helpers follow, instead of silently asking over the new cap.
    #[cfg(feature = "app")]
    #[test]
    fn the_paging_cap_is_the_servers_cap() {
        assert_eq!(
            super::api::MAX_PER_PAGE,
            crate::utils::PaginationParams::MAX_PER_PAGE
        );
    }

    /// The `get_all_*` helpers own `page` / `per_page`, so a caller's own
    /// filters must survive and the cap must be what goes on the wire. An
    /// endpoint with no filters must not gain a `?&`, which is not a query
    /// string the server's `Query<PaginationParams>` extractor accepts.
    #[cfg(feature = "app")]
    #[test]
    fn the_paging_helpers_append_the_cap_to_any_path() {
        use super::api::{paged_path, MAX_PER_PAGE};
        assert_eq!(
            paged_path("/auth/users", 1),
            format!("/auth/users?page=1&per_page={MAX_PER_PAGE}"),
            "a bare path opens its own query string"
        );
        assert_eq!(
            paged_path("/invoices?company_id=abc", 3),
            format!("/invoices?company_id=abc&page=3&per_page={MAX_PER_PAGE}"),
            "an existing filter is kept and the paging is appended to it"
        );
    }

    /// The page loop stops on the first SHORT page, so the guard the pages
    /// are compared against has to be the cap itself. Pinning it here keeps
    /// the "is this page full?" test and the page size that was requested
    /// from drifting apart, which would either truncate the list (stopping on
    /// a full page) or spin to `MAX_PAGES` on every read.
    #[cfg(feature = "app")]
    #[test]
    fn the_page_loop_measures_a_short_page_against_the_requested_size() {
        let body = production_src()
            .split("pub async fn get_all_with_auth")
            .nth(1)
            .expect("get_all_with_auth is defined in this file");
        let body = &body[..body.find("\n    /// Post request").unwrap_or(body.len())];
        assert!(
            body.contains("resp.data.len() as u32 >= MAX_PER_PAGE"),
            "the loop must compare the page it got against the page it asked \
             for; anything else silently truncates or never terminates: {body}"
        );
        assert!(
            body.contains("MAX_PAGES"),
            "the loop must be bounded, so an endpoint that ignores `page` \
             fails loudly instead of spinning: {body}"
        );
    }
}

/// mokosh-contact-login prompt 005: unit tests for the contact-plane
/// session holders. Covers only the in-memory holder round-trip and
/// the isolation from the staff `ACCESS_TOKEN`. The refresh-token
/// setter mirrors to `localStorage` via `web_sys::window()`, which
/// panics on the native test target (`cannot access imported statics
/// on non-wasm targets`); those paths are exercised in the browser
/// end-to-end run described in prompt 005's Verify section rather
/// than in `cargo test --lib`.
#[cfg(all(test, feature = "web"))]
mod contact_session_tests {
    use super::api::{
        current_access_token, current_contact_access_token, has_contact_session,
        set_contact_access_token,
    };

    #[test]
    fn contact_access_token_roundtrip() {
        set_contact_access_token(Some("contact-access".to_string()));
        assert_eq!(
            current_contact_access_token().as_deref(),
            Some("contact-access")
        );
        assert!(has_contact_session());
        // Staff bearer is untouched by a contact sign-in: separate cells.
        assert_eq!(
            current_access_token(),
            None,
            "a contact sign-in must not populate the staff access token"
        );
        set_contact_access_token(None);
        assert_eq!(current_contact_access_token(), None);
        assert!(!has_contact_session());
    }

    /// MAPPS-604: the Company UUID slot round-trips independently from
    /// the token holders; setting `None` clears it. Verifies the state
    /// wiring in `set_contact_company_id` /
    /// `current_contact_company_id`.
    #[test]
    fn contact_company_id_roundtrip() {
        use super::api::{current_contact_company_id, set_contact_company_id};
        let id = uuid::Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
        set_contact_company_id(Some(id));
        assert_eq!(current_contact_company_id(), Some(id));
        set_contact_company_id(None);
        assert_eq!(current_contact_company_id(), None);
    }

    /// MAPPS-609: the contact UUID slot round-trips independently from
    /// the token holders; setting `None` clears it. Verifies the state
    /// wiring in `set_contact_id` / `current_contact_id`, mirroring the
    /// company-id round-trip above.
    #[test]
    fn contact_id_roundtrip() {
        use super::api::{current_contact_id, set_contact_id};
        let id = uuid::Uuid::from_u128(0xaabb_ccdd_eeff_0011_2233_4455_6677_8899);
        set_contact_id(Some(id));
        assert_eq!(current_contact_id(), Some(id));
        set_contact_id(None);
        assert_eq!(current_contact_id(), None);
    }
}

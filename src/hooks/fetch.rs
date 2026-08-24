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
#[cfg(feature = "web")]
pub static TENANT_GENERATION: GlobalSignal<u64> = Signal::global(|| 0);

/// Read the active-tenant generation counter. Call this INSIDE a
/// `use_resource` closure (before any early return) so Dioxus subscribes
/// the resource to it and re-fetches on the next org switch / token swap.
/// The returned value is otherwise unused; it exists purely to register
/// the reactive dependency.
#[cfg(feature = "web")]
pub fn active_tenant_generation() -> u64 {
    *TENANT_GENERATION.read()
}

/// Non-web stub so the same call site compiles under `cargo check`
/// without the `web` feature.
#[cfg(not(feature = "web"))]
pub fn active_tenant_generation() -> u64 {
    0
}

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
#[cfg(feature = "web")]
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
#[cfg(feature = "web")]
pub static ACCOUNT_DELETED: GlobalSignal<bool> = Signal::global(|| false);

/// Flip [`ACCOUNT_DELETED`] to `true`. Idempotent (called from every
/// fetch on the error path once the account is tombstoned, but only
/// writes to the signal on the first observation) so a burst of
/// concurrent 410s does not wake readers repeatedly.
#[cfg(feature = "web")]
pub(crate) fn note_account_deleted() {
    if !*ACCOUNT_DELETED.peek() {
        *ACCOUNT_DELETED.write() = true;
    }
}

/// Classify a completed HTTP response. Any response at all - even a 4xx -
/// proves the server is reachable, so it clears the "down" state. A `5xx`
/// is treated as "down" per MAPPS-333: the server is up but failing, and
/// we surface that the same way as an outage. A `4xx` (auth, validation)
/// is NOT "down" and keeps surfacing through the normal per-call paths.
#[cfg(feature = "web")]
pub(crate) fn note_response_status(status: u16) {
    set_server_reachable(!(500..600).contains(&status));
}

/// Classify a transport-level failure: the opaque browser fetch rejection
/// (which the console frequently renders as a CORS error even though no
/// CORS misconfiguration exists), DNS failure, or timeout. This is the
/// server being unreachable; per MAPPS-333 it is classified here as
/// server-down and never passed through to the user as a CORS / "Failed
/// to fetch" message. (Mokosh App sets no CSP of its own, so there is no
/// CSP-block case to distinguish from a genuine outage here.)
#[cfg(feature = "web")]
pub(crate) fn note_transport_error() {
    set_server_reachable(false);
}

/// Write the reachability flag, but only on an actual transition so a
/// successful request does not wake every reader on each call.
#[cfg(feature = "web")]
fn set_server_reachable(reachable: bool) {
    if *SERVER_REACHABLE.peek() != reachable {
        *SERVER_REACHABLE.write() = reachable;
    }
}

/// API client for making HTTP requests
pub mod api {
    #[cfg(feature = "web")]
    use gloo_net::http::Request;
    #[cfg(feature = "web")]
    use serde::{de::DeserializeOwned, Serialize};

    /// PMS-729: read the configured client-portal host suffix, checking
    /// (in order) the container-emitted `window.__MOKOSH_CONFIG__.portal_host_suffix`
    /// and, as a last-resort compile-time fallback, the
    /// `MOKOSH_PORTAL_HOST_SUFFIX` env var baked in at build time. The
    /// compile-time path lets a dev developer run
    /// `MOKOSH_PORTAL_HOST_SUFFIX=.client.localhost dx serve` and hit
    /// `http://acme.client.localhost:8080/portal/login` without also
    /// having to author a `_mokosh_config.js` shim (dev has no
    /// entrypoint to write one). Empty at every layer returns `None`.
    #[cfg(feature = "web")]
    fn portal_host_suffix() -> Option<String> {
        if let Some(v) = crate::modules::runtime_config::get("portal_host_suffix") {
            if !v.is_empty() {
                return Some(v);
            }
        }
        option_env!("MOKOSH_PORTAL_HOST_SUFFIX")
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }

    /// Derive the Mokosh API base URL.
    ///
    /// Resolution order:
    ///   1. `window.__MOKOSH_CONFIG__.api_base` if set by the prod
    ///      container's entrypoint. Self-hosters on a custom hostname
    ///      override here without rebuilding the image.
    ///   2. PMS-729: portal-host derivation. When the current host ends
    ///      with the configured `portal_host_suffix` (e.g.
    ///      `.client.a8n.systems`), strip the suffix's leading label and
    ///      point at `api.msp.<tld>` where `<tld>` is the rest of the
    ///      suffix without its leading dot. Keeps the API host
    ///      unchanged regardless of which client subdomain the SPA is
    ///      served from, and stays shape-agnostic - swapping the
    ///      `client` word (§13) is a config-only change with no Rust
    ///      edit.
    ///   3. Host-prefix derivation for the canonical `msp.<tld>`
    ///      deploys (e.g. `msp.a8n.systems` SPA → `api.msp.a8n.systems`
    ///      API).
    ///   4. Same-origin `/api/v1` for dev (localhost, IP address, or
    ///      any host that doesn't start with `msp.`) so the Dioxus dev
    ///      server can proxy to a local backend.
    #[cfg(feature = "web")]
    pub fn api_base() -> String {
        if let Some(injected) = crate::modules::runtime_config::get("api_base") {
            return injected;
        }
        if let Some(win) = web_sys::window() {
            if let Ok(host) = win.location().host() {
                // PMS-729: portal-host case. `host` includes port on
                // non-443 dev; strip it before the suffix match.
                if let Some(suffix) = portal_host_suffix() {
                    let host_no_port = host.split(':').next().unwrap_or(&host);
                    let suffix_lower = suffix.to_ascii_lowercase();
                    let host_lower = host_no_port.to_ascii_lowercase();
                    if host_lower.ends_with(&suffix_lower) {
                        // Strip the leading dot off the suffix to get the
                        // apex. For dev (`.client.localhost`) the apex is
                        // just `localhost`; aim at same-origin `/api/v1`
                        // so the Dioxus dev proxy reaches mokosh-server.
                        // For staging / prod, prefer `api.msp.<apex>`.
                        let apex = suffix_lower.trim_start_matches('.');
                        if apex == "localhost" || apex.ends_with(".localhost") {
                            return "/api/v1".to_string();
                        }
                        if !apex.is_empty() {
                            return format!("https://api.msp.{apex}/api/v1");
                        }
                    }
                }
                if let Some(rest) = host.strip_prefix("msp.") {
                    return format!("https://api.msp.{rest}/api/v1");
                }
            }
        }
        "/api/v1".to_string()
    }

    /// PMS-729: `true` iff the SPA is currently served on a portal host
    /// (i.e. the current `window.location.host` ends with the configured
    /// `portal_host_suffix`). Used by the portal login page to decide
    /// whether to hide the slug input and paint the MSP branding block.
    ///
    /// Kept in this module because it reads `runtime_config` + a
    /// window location, both of which are `cfg(feature = "web")`. The
    /// non-web stub returns `false` so downstream call sites compile
    /// under a plain `cargo check`.
    #[cfg(feature = "web")]
    pub fn on_portal_host() -> bool {
        let Some(suffix) = portal_host_suffix() else {
            return false;
        };
        let Some(win) = web_sys::window() else {
            return false;
        };
        let Ok(host) = win.location().host() else {
            return false;
        };
        let host_no_port = host.split(':').next().unwrap_or(&host);
        host_no_port
            .to_ascii_lowercase()
            .ends_with(&suffix.to_ascii_lowercase())
    }

    #[cfg(not(feature = "web"))]
    pub fn on_portal_host() -> bool {
        false
    }

    /// PMS-729: the current browser-visible host (`window.location.host`,
    /// including port). Attached as `X-Forwarded-Host` on every
    /// portal-side fetch below so the mokosh-server host-to-tenant
    /// extractor sees the real `{slug}.client.<apex>` value even when a
    /// dev reverse proxy rewrites the `Host` header (Dioxus 0.7.7's
    /// `[[web.proxy]]` reaches the backend as `Host: server:8080`, which
    /// would otherwise defeat the extractor). In production, Traefik
    /// resets `X-Forwarded-Host` from the browser's Host on every
    /// forwarded request, so the header the SPA sets is either
    /// overwritten by the reverse proxy (prod) or the sole source of the
    /// original host (dev). Safe either way: the extractor fails closed
    /// on any slug/tenant miss, so a spoofed value cannot escalate.
    #[cfg(feature = "web")]
    fn current_forwarded_host() -> Option<String> {
        web_sys::window()?
            .location()
            .host()
            .ok()
            .filter(|s| !s.is_empty())
    }

    // Single-threaded global access-token holder. WASM is strictly
    // single-threaded so a `RefCell` is safe; we don't need a mutex.
    // The token lives only in memory: it is wiped on logout and never
    // written to localStorage.
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
    pub fn current_access_token() -> Option<String> {
        ACCESS_TOKEN.with(|t| t.borrow().clone())
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
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
    pub fn set_portal_access_token(token: Option<String>) {
        PORTAL_ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
    }

    /// Read the portal session token. `None` until a portal contact signs in.
    ///
    /// Only the `_portal_authed` helpers below call this; anything that merely
    /// needs to know whether a session exists (the route guard) asks
    /// [`has_portal_session`] instead, so the token itself stays inside this
    /// module.
    #[cfg(feature = "web")]
    pub fn current_portal_access_token() -> Option<String> {
        PORTAL_ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    /// Whether a portal session is held. The predicate `PortalGuard` gates
    /// `/portal/*` on, without handing the token out.
    #[cfg(feature = "web")]
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
        let stored = storage.get_item(PORTAL_REFRESH_STORAGE_KEY).ok().flatten()?;
        if stored.is_empty() {
            return None;
        }
        // Prime the in-memory slot so the next caller skips storage.
        PORTAL_REFRESH_TOKEN.with(|t| *t.borrow_mut() = Some(stored.clone()));
        Some(stored)
    }

    // The web-only API helpers below are grouped under this `api`
    // module; the non-`web` build compiles the module with no items.

    /// Map a transport-level send failure to a `String` error, classifying
    /// it as a server-unreachable condition (MAPPS-333) on the way out.
    /// Used only at `.send()` sites - serialization (`.json()`) failures
    /// keep the plain mapping since they are not connectivity problems.
    #[cfg(feature = "web")]
    fn transport_err(e: impl std::fmt::Display) -> String {
        super::note_transport_error();
        e.to_string()
    }

    /// Transport-error sibling of [`transport_err`] for the typed helpers,
    /// classifying the failure as server-unreachable before wrapping it as
    /// [`ApiError::Network`].
    #[cfg(feature = "web")]
    fn network_err(e: impl std::fmt::Display) -> ApiError {
        super::note_transport_error();
        ApiError::Network(e.to_string())
    }

    /// Get request
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
    async fn status_error(response: gloo_net::http::Response) -> String {
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
    #[cfg(feature = "web")]
    fn user_friendly_status(status: u16) -> String {
        match status {
            400 => "The request was rejected. Please check the form and try again.".into(),
            401 => "Your session has expired. Please sign in again.".into(),
            403 => "You do not have permission to do that.".into(),
            404 => "The requested resource was not found.".into(),
            409 => "The change conflicts with another update. Please refresh and retry.".into(),
            422 => "Validation failed. Please check the form fields.".into(),
            429 => "Too many requests. Please try again shortly.".into(),
            500..=599 => "The server hit an error. Please try again.".into(),
            _ => format!("Request failed ({status})."),
        }
    }

    /// Get request with auth token
    #[cfg(feature = "web")]
    pub async fn get_with_auth<T: DeserializeOwned>(path: &str, token: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
    pub async fn post_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
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

    /// Put request with auth token
    #[cfg(feature = "web")]
    pub async fn put_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
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

    /// PATCH request with auth token. The first caller is the saved
    /// dashboards module (PMS-453), which surfaces partial-update
    /// semantics (toggle `is_default`, edit just the name); a PUT
    /// helper would have invited callers to send the whole row and
    /// blow away unset fields, which is the wrong shape for that
    /// surface.
    #[cfg(feature = "web")]
    pub async fn patch_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
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

    /// Delete request with auth token
    #[cfg(feature = "web")]
    pub async fn delete_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

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
    #[cfg(feature = "web")]
    pub async fn post_no_content_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            Ok(())
        } else {
            Err(status_error(response).await)
        }
    }

    // --- Auto-authed wrappers --------------------------------------------
    //
    // These read the current access token from the thread-local holder so
    // page code does not have to thread it through. If the user is not
    // signed in (`ACCESS_TOKEN` is None) we send the request without an
    // Authorization header and let the server's 401 surface naturally;
    // the OIDC SPA pattern then redirects to the login page.

    #[cfg(feature = "web")]
    pub async fn get_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        match current_access_token() {
            Some(t) => get_with_auth(path, &t).await,
            None => get(path).await,
        }
    }

    #[cfg(feature = "web")]
    pub async fn post_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        match current_access_token() {
            Some(t) => post_with_auth(path, body, &t).await,
            None => post(path, body).await,
        }
    }

    #[cfg(feature = "web")]
    pub async fn put_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        put_with_auth(path, body, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_authed(path: &str) -> Result<(), String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        delete_with_auth(path, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn patch_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        patch_with_auth(path, body, &t).await
    }

    /// Auto-authed POST for empty-body endpoints (see
    /// `post_no_content_with_auth`).
    #[cfg(feature = "web")]
    pub async fn post_authed_no_content(path: &str) -> Result<(), String> {
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
    #[cfg(feature = "web")]
    fn portal_not_signed_in() -> String {
        "not signed in to the portal".to_string()
    }

    #[cfg(feature = "web")]
    pub async fn get_portal_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        get_with_auth(path, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn post_portal_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_portal_access_token().ok_or_else(portal_not_signed_in)?;
        post_with_auth(path, body, &t).await
    }

    /// Typed sibling of [`post_portal_authed`], for the portal call sites that
    /// need the status code (the ticket reply form).
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
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

    #[cfg(feature = "web")]
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
                    401 => "Your session has expired. Please sign in again.".into(),
                    403 => "You do not have permission to do that.".into(),
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

    #[cfg(feature = "web")]
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

    #[cfg(feature = "web")]
    async fn handle_response<T: DeserializeOwned>(
        response: gloo_net::http::Response,
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

    #[cfg(feature = "web")]
    pub async fn get_authed_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        if let Some(t) = current_access_token() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(network_err)?;
        handle_response(resp).await
    }

    /// Bearer-authed GET that returns the raw response body plus the server's
    /// `Content-Disposition` filename. Used for attachment downloads the SPA
    /// cannot fetch through a plain `<a href>` because the bearer lives in WASM
    /// memory rather than a cookie (the data export, MAPPS-364).
    #[cfg(feature = "web")]
    pub async fn get_authed_bytes(path: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url);
        if let Some(t) = current_access_token() {
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
    #[cfg(feature = "web")]
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

    #[cfg(feature = "web")]
    pub async fn post_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        if let Some(t) = current_access_token() {
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

    /// MAPPS-368: unauthed typed POST, for the standalone login form. Same as
    /// [`post_authed_typed`] but sends no bearer (the user is not signed in
    /// yet), so the caller can inspect `ApiError::Status { code, .. }` and map
    /// 401 -> "invalid credentials" / 429 -> "too many attempts".
    #[cfg(feature = "web")]
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
    #[cfg(feature = "web")]
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

    #[cfg(feature = "web")]
    pub async fn put_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
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
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// PMS-731: typed PATCH. The forms surface (and the rest of the server's
    /// partial-update routes) is PATCH rather than PUT, and only the
    /// `String`-error `patch_authed` existed, which loses the per-field 422
    /// envelope the editor needs to report a bad field set.
    #[cfg(feature = "web")]
    pub async fn patch_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
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
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_authed_typed(path: &str) -> Result<(), ApiError> {
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

/// MAPPS-395 recurrence gates: keep the agent bearer and the portal session
/// token in separate lanes. Both tests are source scans plus a holder check,
/// because the request helpers themselves need a browser to run.
#[cfg(test)]
mod tests {
    const FETCH_SRC: &str = include_str!("fetch.rs");
    const PORTAL_PAGE_SRC: &str = include_str!("../pages/portal.rs");

    /// The only functions allowed to touch the portal token holder.
    const PORTAL_TOKEN_READERS: &[&str] = &[
        "set_portal_access_token",
        "current_portal_access_token",
        "has_portal_session",
        "get_portal_authed",
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

    #[cfg(feature = "web")]
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
}

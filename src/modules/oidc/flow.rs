//! `start_login` and `complete_login`: the two halves of the OIDC code
//! flow as seen from a browser SPA.
//!
//! MAPPS-504: the desktop build compiles this but cannot run it - there
//! is no origin for a `redirect_uri` and no document to redirect. It
//! signs in through the standalone username/password path instead, and
//! `start_login` reports that rather than failing quietly. MAPPS-505
//! adds the RFC 8252 loopback flow that makes this work there too.

use crate::platform::http::Request;
use chrono::{Duration, Utc};
use serde::Deserialize;

use super::config::OidcConfig;
use super::pkce::{generate_code_verifier, random_opaque, s256_challenge};
use super::storage::{save_pending, take_pending, PendingFlow};
use super::tokens::Tokens;

#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("config: {0}")]
    Config(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("network: {0}")]
    Network(String),
    #[error("token endpoint: {error} ({description})")]
    TokenEndpoint { error: String, description: String },
    /// MAPPS-432: `code` or `state` absent from the callback URL. A variant of
    /// its own (not a `TokenEndpoint { error: "invalid_request" }`) so
    /// [`classify_flow_error`] can recognise "no live flow on this URL"
    /// without matching `Display` prose.
    #[error("missing callback parameter: {0}")]
    MissingCallbackParam(&'static str),
    #[error("state mismatch (possible CSRF)")]
    StateMismatch,
    #[error("nonce mismatch (possible token replay)")]
    NonceMismatch,
    #[error("redirect failed: {0}")]
    Redirect(String),
}

/// Begin the login flow. Generates PKCE + state + nonce, persists them
/// in `sessionStorage`, then navigates the browser to the authorize
/// endpoint. This function does not return on success: the page is
/// replaced.
pub fn start_login(cfg: &OidcConfig, return_to: impl Into<String>) -> Result<(), FlowError> {
    let verifier = generate_code_verifier();
    let challenge = s256_challenge(&verifier);
    let state = random_opaque();
    let nonce = random_opaque();
    let return_to = return_to.into();
    let redirect_uri = cfg
        .resolve_redirect_uri()
        .map_err(|e| FlowError::Config(e.to_string()))?;

    save_pending(&mut PendingFlow {
        code_verifier: verifier,
        state: state.clone(),
        nonce: nonce.clone(),
        return_to: return_to.clone(),
        // MAPPS-338: stamped inside `save_pending` (defaults to 0 here).
        issued_at_ms: 0,
    })
    .map_err(FlowError::Storage)?;

    // Build the authorize URL.
    let issuer = cfg.issuer.trim_end_matches('/');
    let mut url = format!("{issuer}/oauth2/authorize");
    url.push('?');
    let q = [
        ("response_type", "code"),
        ("client_id", cfg.client_id),
        ("redirect_uri", &redirect_uri),
        ("scope", cfg.scopes),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ];
    for (i, (k, v)) in q.iter().enumerate() {
        if i > 0 {
            url.push('&');
        }
        url.push_str(k);
        url.push('=');
        url.push_str(&urlencode(v));
    }

    // MAPPS-504/505: the desktop build has nowhere to be redirected TO,
    // so this reports why instead of appearing to do nothing. The
    // standalone username/password path (MAPPS-368) is what a desktop
    // sign-in uses until the loopback flow lands.
    crate::platform::location::set_href(&url).map_err(|e| {
        FlowError::Redirect(format!("could not hand off to the identity provider: {e}"))
    })
}

// Snapshot of `?code=...&state=...` taken before the Dioxus router
// mounts. Some routers (Dioxus 0.7 included) call
// `history.replaceState()` during initialization to normalize the URL
// to its declared route shape (no query params), which would erase
// the OAuth response before `AuthCallbackPage` ever reads it. We
// freeze the original `window.location.search` in a thread-local at
// program entry; `complete_login` reads from here.
//
// WASM is single-threaded, so the `RefCell` is safe.
thread_local! {
    static INITIAL_SEARCH: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Capture `window.location.search` once. Call from `main()` before
/// `dioxus::launch(App)` so the snapshot is taken before the Router
/// has a chance to rewrite the URL.
pub fn snapshot_initial_search() {
    let s = crate::platform::location::search().unwrap_or_default();
    INITIAL_SEARCH.with(|cell| *cell.borrow_mut() = Some(s));
}

fn current_search() -> String {
    INITIAL_SEARCH
        .with(|cell| cell.borrow().clone())
        .filter(|s| !s.is_empty())
        .or_else(crate::platform::location::search)
        .unwrap_or_default()
}

/// True for routes that must never be a post-login `return_to` target:
/// the auth plumbing itself (round-tripping back through `/login` or
/// `/auth/callback` would loop) and the bare root.
fn is_auth_plumbing(path: &str) -> bool {
    path.is_empty()
        || path == "/"
        || path.starts_with("/login")
        || path.starts_with("/auth/callback")
}

/// Pure core of [`current_return_to`]: build the `return_to` from a
/// `pathname` + query string, or `""` for routes that must not be a
/// target. Split out so the filtering is unit-testable without a window.
fn sanitize_return_to(path: &str, search: &str) -> String {
    if is_auth_plumbing(path) {
        return String::new();
    }
    format!("{path}{search}")
}

/// The route the browser is currently on (`pathname` + the preserved
/// original query), suitable as `start_login`'s `return_to` so a
/// cold-loaded / bookmarked deep link survives the authorize round-trip
/// instead of collapsing to `/dashboard` (MAPPS-323).
///
/// Uses [`current_search`] rather than the live `location.search` because
/// the Dioxus router strips the query during init; the snapshot taken in
/// `main()` is the only reliable copy. Returns `""` (which the callback
/// treats as "no specific target" -> dashboard) for the auth-plumbing
/// routes that must never be a return target, and off-wasm where there is
/// no window.
pub fn current_return_to() -> String {
    let Some(path) = crate::platform::location::pathname() else {
        return String::new();
    };
    sanitize_return_to(&path, &current_search())
}

/// What `/auth/callback` should do with a `return_to` after a successful
/// token exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTarget {
    /// No specific page (`""` / `/` / `/dashboard`), an unsafe target
    /// (protocol-relative or cross-origin), or an auth-plumbing route:
    /// soft router-push to the dashboard, keeping the in-memory session.
    Dashboard,
    /// A concrete same-origin deep link: hard-nav to restore the exact
    /// path. The token bundle saved to sessionStorage lets the rebooted
    /// tree re-auth, so there is no re-login loop.
    Restore,
}

/// Decide how the callback restores a `return_to`. Only a same-origin
/// absolute path (single leading `/`, not `//host`) that is not the
/// dashboard / auth-plumbing is restored with a hard nav; everything else
/// (empty, root, dashboard, protocol-relative, cross-origin, scheme)
/// soft-routes to the dashboard. Keeping `/dashboard` on the soft path
/// avoids a redundant full-page reload on the interactive `/login` flow
/// (which passes `return_to = "/dashboard"`), since the callback is
/// already a fresh boot.
pub fn classify_return_to(return_to: &str) -> ReturnTarget {
    let same_origin_path = return_to.starts_with('/') && !return_to.starts_with("//");
    if !same_origin_path || return_to == "/dashboard" || is_auth_plumbing(return_to) {
        ReturnTarget::Dashboard
    } else {
        ReturnTarget::Restore
    }
}

/// MAPPS-432: what `/auth/callback` should do with a failed exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackRecovery {
    /// "There is no live authorization flow on this URL": restart the login
    /// flow silently instead of showing the user a failure they cannot act on.
    Restart,
    /// A genuine protocol / security / configuration fault: render it.
    Show,
}

/// OP `?error=` codes that mean "send the user back to the OP to
/// re-authenticate" (OIDC Core 3.1.2.6). A restart does exactly that.
const REAUTH_ERROR_CODES: [&str; 4] = [
    "login_required",
    "interaction_required",
    "consent_required",
    "account_selection_required",
];

/// Decide whether a callback failure is recoverable by restarting the login
/// flow. Matches on the `FlowError` variant, never on its `Display` output, so
/// rewording an `#[error(...)]` attribute cannot change auth behaviour; the
/// match is exhaustive, so a new variant is a compile error rather than a
/// silent [`CallbackRecovery::Show`].
///
/// Restart covers only "no live flow here": a missing / expired `PendingFlow`
/// (`Storage`), a callback URL with no `code` or `state`
/// (`MissingCallbackParam`), and the OP's re-authentication signals. Everything
/// else shows, including `StateMismatch` / `NonceMismatch` (CSRF and replay
/// indicators a silent retry would hide) and every other token-endpoint code
/// (`invalid_grant`, `invalid_client`, `server_error`, …), which a retry would
/// only loop on.
pub fn classify_flow_error(e: &FlowError) -> CallbackRecovery {
    match e {
        FlowError::Storage(_) | FlowError::MissingCallbackParam(_) => CallbackRecovery::Restart,
        FlowError::TokenEndpoint { error, .. } if REAUTH_ERROR_CODES.contains(&error.as_str()) => {
            CallbackRecovery::Restart
        }
        FlowError::TokenEndpoint { .. }
        | FlowError::Config(_)
        | FlowError::Network(_)
        | FlowError::StateMismatch
        | FlowError::NonceMismatch
        | FlowError::Redirect(_) => CallbackRecovery::Show,
    }
}

/// Handle the callback URL. Reads `code` + `state` from the snapshot
/// taken at startup (see [`snapshot_initial_search`]), verifies state
/// against the pending flow, exchanges the code at the token endpoint,
/// returns parsed tokens plus the URL the original `start_login` asked
/// to return to.
pub async fn complete_login(cfg: &OidcConfig) -> Result<(Tokens, String), FlowError> {
    let search = current_search();
    let params = crate::utils::url::QueryString::parse(&search);
    // Surface an OP error response before extracting code/state. An error
    // redirect carries no `code`, so checking code first masks the real
    // error as a generic "missing code".
    if let Some(err) = params.get("error") {
        return Err(FlowError::TokenEndpoint {
            error: err,
            description: params.get("error_description").unwrap_or_default(),
        });
    }

    // MAPPS-432: a bare `/auth/callback` (reload, restored tab, bookmark) is
    // the normal post-MAPPS-336 state, not a protocol fault. Typed so the
    // callback page restarts the login flow instead of showing an error.
    let code = params
        .get("code")
        .ok_or(FlowError::MissingCallbackParam("code"))?;
    let state = params
        .get("state")
        .ok_or(FlowError::MissingCallbackParam("state"))?;

    let pending = take_pending().map_err(FlowError::Storage)?;
    if pending.state != state {
        return Err(FlowError::StateMismatch);
    }

    let redirect_uri = cfg
        .resolve_redirect_uri()
        .map_err(|e| FlowError::Config(e.to_string()))?;

    // POST to /oauth2/token (form-encoded).
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("code_verifier", &pending.code_verifier),
        ("client_id", cfg.client_id),
    ]);
    let issuer = cfg.issuer.trim_end_matches('/');
    let url = format!("{issuer}/oauth2/token");
    let resp = Request::post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .map_err(|e| FlowError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| FlowError::Network(e.to_string()))?;

    if !resp.ok() {
        let body: ErrorBody = resp
            .json()
            .await
            .unwrap_or_else(|_| ErrorBody::generic("token_endpoint_failed"));
        return Err(FlowError::TokenEndpoint {
            error: body.error,
            description: body.error_description.unwrap_or_default(),
        });
    }

    let body: TokenBody = resp
        .json()
        .await
        .map_err(|e| FlowError::Network(format!("token body: {e}")))?;

    // OIDC Core 3.1.3.3: the code-grant response MUST include an
    // id_token when `openid` was in the request scope. We always
    // request `openid`, so a missing field is a protocol violation.
    let id_token = body.id_token.ok_or_else(|| FlowError::TokenEndpoint {
        error: "invalid_response".into(),
        description: "id_token missing from authorization_code response".into(),
    })?;

    // OIDC Core 3.1.3.7 step 11: the id_token's `nonce` claim MUST
    // equal the nonce we sent on the authorize request. State already
    // guards the redirect against CSRF; the nonce binds the *id_token*
    // itself to this browser's login attempt, defeating replay of an
    // id_token captured from another session. We decode the claims
    // unverified (signature verification is an intentional WASM
    // tradeoff, see `IdTokenClaims::parse_unverified`) purely to read
    // the nonce; a mismatch or a missing nonce is a hard failure.
    let claims =
        super::tokens::IdTokenClaims::parse_unverified(&id_token).map_err(FlowError::Network)?;
    match claims.nonce.as_deref() {
        Some(n) if n == pending.nonce => {}
        _ => return Err(FlowError::NonceMismatch),
    }

    let tokens = Tokens {
        access_token: body.access_token,
        id_token,
        refresh_token: body.refresh_token,
        expires_at: Utc::now() + Duration::seconds(body.expires_in.max(0)),
        scope: body.scope.unwrap_or_default(),
    };
    Ok((tokens, pending.return_to))
}

/// Exchange a refresh token for a fresh pair via `/oauth2/token`.
///
/// The mokosh-server side rotates under SERIALIZABLE isolation and
/// detects reuse: any second attempt with the same refresh token
/// revokes the entire family and returns `invalid_grant`. The caller
/// should treat any error here as "session is over" and route the
/// browser to the login page.
///
/// Note: the OP does not return a new `id_token` from a refresh grant
/// (per OIDC Core 12.2; the prior id_token is still authoritative for
/// the sub/aud/auth_time it carries). We preserve the previously
/// issued one alongside the rotated access + refresh tokens.
pub async fn refresh_tokens(
    cfg: &OidcConfig,
    refresh_token: &str,
    prior_id_token: &str,
) -> Result<Tokens, FlowError> {
    let body = form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", cfg.client_id),
    ]);
    let issuer = cfg.issuer.trim_end_matches('/');
    let url = format!("{issuer}/oauth2/token");
    let resp = Request::post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(body)
        .map_err(|e| FlowError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| FlowError::Network(e.to_string()))?;

    if !resp.ok() {
        let body: ErrorBody = resp
            .json()
            .await
            .unwrap_or_else(|_| ErrorBody::generic("token_endpoint_failed"));
        return Err(FlowError::TokenEndpoint {
            error: body.error,
            description: body.error_description.unwrap_or_default(),
        });
    }

    let body: TokenBody = resp
        .json()
        .await
        .map_err(|e| FlowError::Network(format!("token body: {e}")))?;

    Ok(Tokens {
        access_token: body.access_token,
        // The OP omits id_token on a refresh response; carry the prior
        // one so downstream code (logout's id_token_hint, claim parsing)
        // keeps working.
        id_token: body.id_token.unwrap_or_else(|| prior_id_token.to_string()),
        refresh_token: body.refresh_token,
        expires_at: Utc::now() + Duration::seconds(body.expires_in.max(0)),
        scope: body.scope.unwrap_or_default(),
    })
}

/// MAPPS-427 WARNING: bunyip's `/v1/*` family is a Resource Server whose at+jwt
/// verifier enforces `aud == OIDC_RS_AUDIENCE` (BUNYIP-252), and the access
/// token this SPA holds is minted for MOKOSH's audience. Any call made through
/// the two helpers below is therefore answered 401, by design, and cannot be
/// fixed on bunyip's side without letting another RP's token through. They have
/// no call sites today for that reason. Reaching for one means first arranging
/// a token bunyip will accept.
/// Issuer-authed GET. Hits `{issuer}{path}` with the in-memory access
/// token in `Authorization: Bearer`. Used for self-service auth APIs
/// that live on the issuer host (e.g. `/v1/auth/sessions`) and are
/// not reachable through the same-origin `/api/*` proxy.
pub async fn issuer_get_authed<T: serde::de::DeserializeOwned>(
    cfg: &OidcConfig,
    path: &str,
) -> Result<T, FlowError> {
    let token = crate::hooks::fetch::api::current_access_token()
        .ok_or_else(|| FlowError::Network("not signed in".into()))?;
    let issuer = cfg.issuer.trim_end_matches('/');
    let url = format!("{issuer}{path}");
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| FlowError::Network(e.to_string()))?;
    if !resp.ok() {
        return Err(FlowError::Network(format!("HTTP {}", resp.status())));
    }
    resp.json::<T>()
        .await
        .map_err(|e| FlowError::Network(format!("body: {e}")))
}

/// Issuer-authed POST with a JSON body. The 4xx error body is
/// preserved as-is in the `Network` variant so callers can parse
/// structured `{"error":"..."}` payloads. (`Network` is a
/// transport-shaped variant by name, but in practice it's how the
/// codebase already smuggles error bodies through.)
pub async fn issuer_post_authed<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    cfg: &OidcConfig,
    path: &str,
    body: &B,
) -> Result<T, FlowError> {
    let token = crate::hooks::fetch::api::current_access_token()
        .ok_or_else(|| FlowError::Network("not signed in".into()))?;
    let issuer = cfg.issuer.trim_end_matches('/');
    let url = format!("{issuer}{path}");
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(body)
        .map_err(|e| FlowError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| FlowError::Network(e.to_string()))?;
    if !resp.ok() {
        let raw = resp.text().await.unwrap_or_default();
        return Err(FlowError::Network(raw));
    }
    resp.json::<T>()
        .await
        .map_err(|e| FlowError::Network(format!("body: {e}")))
}

/// RFC 7009 token revocation. Best-effort: the spec requires the OP
/// to return 200 even for unknown/invalid tokens, so we treat any
/// non-network failure as success. Used by the logout flow to kill
/// the refresh-token family server-side before the browser navigates
/// away.
pub async fn revoke_refresh_token(cfg: &OidcConfig, refresh_token: &str) -> Result<(), FlowError> {
    let body = form_encode(&[
        ("token", refresh_token),
        ("token_type_hint", "refresh_token"),
        ("client_id", cfg.client_id),
    ]);
    let issuer = cfg.issuer.trim_end_matches('/');
    let url = format!("{issuer}/oauth2/revoke");
    let _ = Request::post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .map_err(|e| FlowError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| FlowError::Network(e.to_string()))?;
    Ok(())
}

#[derive(Deserialize)]
struct TokenBody {
    access_token: String,
    /// Optional on refresh responses (OIDC Core 12.2). Required on
    /// authorization-code responses; the caller distinguishes by
    /// context.
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}
impl ErrorBody {
    fn generic(s: &str) -> Self {
        Self {
            error: s.to_string(),
            error_description: None,
        }
    }
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencode(s: &str) -> String {
    // application/x-www-form-urlencoded percent-encoding. Spaces become
    // `+`. MAPPS-504: `encode_uri_component` is now ours (it was
    // `js_sys`'s), so this rule holds on both targets; the `%20` patch
    // is unchanged.
    crate::utils::url::encode_uri_component(s).replace("%20", "+")
}

#[cfg(test)]
mod tests {
    use super::{
        classify_flow_error, classify_return_to, sanitize_return_to, CallbackRecovery, FlowError,
        ReturnTarget,
    };

    fn token_endpoint(error: &str) -> FlowError {
        FlowError::TokenEndpoint {
            error: error.to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn sanitize_keeps_concrete_paths_with_query() {
        assert_eq!(sanitize_return_to("/tickets/new", ""), "/tickets/new");
        assert_eq!(
            sanitize_return_to("/tickets/new", "?company_id=abc"),
            "/tickets/new?company_id=abc"
        );
        assert_eq!(sanitize_return_to("/contacts/new", ""), "/contacts/new");
    }

    #[test]
    fn sanitize_drops_auth_plumbing_and_root() {
        for p in ["", "/", "/login", "/login/2fa", "/auth/callback"] {
            assert_eq!(sanitize_return_to(p, "?code=x"), "", "path {p:?}");
        }
    }

    #[test]
    fn classify_restores_concrete_deep_links() {
        assert_eq!(classify_return_to("/tickets/new"), ReturnTarget::Restore);
        assert_eq!(
            classify_return_to("/tickets/new?company_id=abc"),
            ReturnTarget::Restore
        );
        assert_eq!(classify_return_to("/contacts/new"), ReturnTarget::Restore);
    }

    #[test]
    fn classify_sends_dashboard_and_empty_to_soft_push() {
        // `/dashboard` must stay soft: the interactive `/login` flow passes
        // it, and a hard nav would add a redundant reload (regression guard).
        for p in ["", "/", "/dashboard"] {
            assert_eq!(classify_return_to(p), ReturnTarget::Dashboard, "path {p:?}");
        }
    }

    #[test]
    fn classify_rejects_offorigin_and_plumbing() {
        for p in [
            "//evil.example",         // protocol-relative
            "https://evil.example/x", // cross-origin
            "javascript:alert(1)",    // scheme injection
            "/login",                 // would loop
            "/auth/callback?code=x",  // would loop
        ] {
            assert_eq!(classify_return_to(p), ReturnTarget::Dashboard, "path {p:?}");
        }
    }

    // MAPPS-432: one case per FlowError variant. The classifier's match is
    // exhaustive, so a new variant breaks the build; these pin the mapping.
    #[test]
    fn classify_restarts_when_no_live_flow_exists() {
        for e in [
            FlowError::Storage("no pending OIDC flow".into()),
            FlowError::Storage("pending OIDC flow expired (age 1 ms > 0 ms)".into()),
            FlowError::MissingCallbackParam("code"),
            FlowError::MissingCallbackParam("state"),
        ] {
            assert_eq!(classify_flow_error(&e), CallbackRecovery::Restart, "{e}");
        }
    }

    #[test]
    fn classify_restarts_on_op_reauth_signals() {
        for code in [
            "login_required",
            "interaction_required",
            "consent_required",
            "account_selection_required",
        ] {
            let e = token_endpoint(code);
            assert_eq!(classify_flow_error(&e), CallbackRecovery::Restart, "{code}");
        }
    }

    #[test]
    fn classify_shows_real_faults() {
        for e in [
            FlowError::StateMismatch,
            FlowError::NonceMismatch,
            FlowError::Config("no redirect_uri".into()),
            FlowError::Network("failed to fetch".into()),
            FlowError::Redirect("set_href failed".into()),
        ] {
            assert_eq!(classify_flow_error(&e), CallbackRecovery::Show, "{e}");
        }
    }

    #[test]
    fn classify_shows_non_recoverable_token_endpoint_codes() {
        for code in [
            "invalid_request",
            "invalid_grant",
            "invalid_client",
            "invalid_response",
            "server_error",
            "access_denied",
            "token_endpoint_failed",
        ] {
            let e = token_endpoint(code);
            assert_eq!(classify_flow_error(&e), CallbackRecovery::Show, "{code}");
        }
    }
}

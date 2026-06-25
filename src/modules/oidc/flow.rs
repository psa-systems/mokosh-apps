//! `start_login` and `complete_login`: the two halves of the OIDC code
//! flow as seen from a browser SPA.

use chrono::{Duration, Utc};
use gloo_net::http::Request;
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

    save_pending(&PendingFlow {
        code_verifier: verifier,
        state: state.clone(),
        nonce: nonce.clone(),
        return_to: return_to.clone(),
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

    let win = web_sys::window().ok_or_else(|| FlowError::Redirect("no window".into()))?;
    win.location()
        .set_href(&url)
        .map_err(|_| FlowError::Redirect("set_href failed".into()))
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
    let s = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    INITIAL_SEARCH.with(|cell| *cell.borrow_mut() = Some(s));
}

fn current_search() -> String {
    INITIAL_SEARCH
        .with(|cell| cell.borrow().clone())
        .filter(|s| !s.is_empty())
        .or_else(|| web_sys::window().and_then(|w| w.location().search().ok()))
        .unwrap_or_default()
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
    let Some(win) = web_sys::window() else {
        return String::new();
    };
    let path = win.location().pathname().unwrap_or_default();
    if path.is_empty()
        || path == "/"
        || path.starts_with("/login")
        || path.starts_with("/auth/callback")
    {
        return String::new();
    }
    format!("{path}{}", current_search())
}

/// Handle the callback URL. Reads `code` + `state` from the snapshot
/// taken at startup (see [`snapshot_initial_search`]), verifies state
/// against the pending flow, exchanges the code at the token endpoint,
/// returns parsed tokens plus the URL the original `start_login` asked
/// to return to.
pub async fn complete_login(cfg: &OidcConfig) -> Result<(Tokens, String), FlowError> {
    let search = current_search();
    let params = web_sys::UrlSearchParams::new_with_str(&search)
        .map_err(|_| FlowError::Redirect("UrlSearchParams".into()))?;
    // Surface an OP error response before extracting code/state. An error
    // redirect carries no `code`, so checking code first masks the real
    // error as a generic "missing code".
    if let Some(err) = params.get("error") {
        return Err(FlowError::TokenEndpoint {
            error: err,
            description: params.get("error_description").unwrap_or_default(),
        });
    }

    let code = params.get("code").ok_or_else(|| FlowError::TokenEndpoint {
        error: "invalid_request".into(),
        description: "missing code".into(),
    })?;
    let state = params
        .get("state")
        .ok_or_else(|| FlowError::TokenEndpoint {
            error: "invalid_request".into(),
            description: "missing state".into(),
        })?;

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
    // `+`. We use js_sys's encodeURIComponent and then patch spaces.
    let encoded = js_sys::encode_uri_component(s);
    let s: String = encoded.into();
    s.replace("%20", "+")
}

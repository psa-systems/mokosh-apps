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

/// Handle the callback URL. Reads `code` + `state` from the current
/// location's query string, verifies state against the pending flow,
/// exchanges the code at the token endpoint, returns parsed tokens plus
/// the URL the original `start_login` asked to return to.
pub async fn complete_login(cfg: &OidcConfig) -> Result<(Tokens, String), FlowError> {
    let win = web_sys::window().ok_or_else(|| FlowError::Redirect("no window".into()))?;
    let search = win
        .location()
        .search()
        .map_err(|_| FlowError::Redirect("location.search".into()))?;
    let params = web_sys::UrlSearchParams::new_with_str(&search)
        .map_err(|_| FlowError::Redirect("UrlSearchParams".into()))?;
    let code = params
        .get("code")
        .ok_or_else(|| FlowError::TokenEndpoint {
            error: "invalid_request".into(),
            description: "missing code".into(),
        })?;
    let state = params
        .get("state")
        .ok_or_else(|| FlowError::TokenEndpoint {
            error: "invalid_request".into(),
            description: "missing state".into(),
        })?;

    if let Some(err) = params.get("error") {
        return Err(FlowError::TokenEndpoint {
            error: err,
            description: params.get("error_description").unwrap_or_default(),
        });
    }

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
    let id_token = body
        .id_token
        .ok_or_else(|| FlowError::TokenEndpoint {
            error: "invalid_response".into(),
            description: "id_token missing from authorization_code response".into(),
        })?;

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

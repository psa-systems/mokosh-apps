//! Token holder and ID-token claim parser.
//!
//! Tokens live ONLY in memory (inside Dioxus signals). They are not
//! persisted across reloads: a refresh sends the user through authorize
//! again. This is the OAuth 2.0 for Browser-Based Apps recommendation
//! since localStorage is XSS-readable.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct Tokens {
    pub access_token: String,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub scope: String,
}

/// Subset of OIDC ID-token claims we surface to the UI.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct IdTokenClaims {
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default, rename = "mokosh_tenant_id")]
    pub tenant_id: Option<String>,
    /// Tenant the user is currently acting under. Set by the server
    /// at login / switch / refresh. Older servers omit this field;
    /// the SPA falls back to `tenant_id` (home tenant) in that case.
    #[serde(default, rename = "mokosh_active_tenant")]
    pub active_tenant_id: Option<String>,
    #[serde(default, rename = "mokosh_role")]
    pub role: Option<String>,
    /// OIDC `nonce` echoed back from the authorize request. Compared
    /// against the stored `PendingFlow.nonce` in `complete_login` to
    /// bind the id_token to this browser's login attempt (replay
    /// defense). Optional in the struct so refresh-grant / rehydrate
    /// claim parsing (where no nonce is expected) still decodes.
    #[serde(default)]
    pub nonce: Option<String>,
    pub exp: i64,
    pub iat: i64,
}

impl IdTokenClaims {
    /// Decode (without verification) the claims from a compact JWT.
    ///
    /// Verification is intentionally not performed in the browser:
    /// fetching JWKS and validating an EdDSA signature in WASM is
    /// possible but adds bundle size for no security gain. The token
    /// arrived over TLS in the response body of a request to the
    /// trusted issuer; treating it as authoritative is correct for the
    /// SPA pattern. (Server-side relying parties verify it normally.)
    pub fn parse_unverified(jwt: &str) -> Result<Self, String> {
        let mut parts = jwt.split('.');
        let _header = parts.next().ok_or("malformed jwt: missing header")?;
        let payload = parts.next().ok_or("malformed jwt: missing payload")?;
        let raw = URL_SAFE_NO_PAD
            .decode(payload.as_bytes())
            .map_err(|e| format!("payload base64: {e}"))?;
        serde_json::from_slice(&raw).map_err(|e| format!("payload json: {e}"))
    }
}

impl Tokens {
    pub fn id_claims(&self) -> Result<IdTokenClaims, String> {
        IdTokenClaims::parse_unverified(&self.id_token)
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }
}

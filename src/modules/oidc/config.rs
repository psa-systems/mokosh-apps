//! Compile-time OIDC configuration.
//!
//! Values are baked in via `option_env!` so a deployment can be locked to
//! a specific issuer at build time. For dev convenience there are
//! reasonable defaults pointing at a local mokosh-server.

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: &'static str,
    pub client_id: &'static str,
    /// If `None`, the runtime default is `<origin>/auth/callback`.
    pub redirect_uri: Option<&'static str>,
    pub scopes: &'static str,
    /// Origin of the Bunyip hub (e.g. `https://a contributor-bunyip.a8n.run`).
    /// Used by the legacy `/login`, `/forgot-password`,
    /// `/reset-password/:token`, `/invite/:token`, and signup redirect
    /// stubs so existing bookmarks land on the hub instead of a 404. No
    /// trailing slash.
    pub hub_base_url: &'static str,
}

impl OidcConfig {
    pub const fn from_env() -> Self {
        Self {
            issuer: match option_env!("MOKOSH_OIDC_ISSUER") {
                Some(s) => s,
                None => "http://localhost:8080",
            },
            client_id: match option_env!("MOKOSH_OIDC_CLIENT_ID") {
                Some(s) => s,
                None => "00000000-0000-0000-0000-000000000000",
            },
            redirect_uri: option_env!("MOKOSH_OIDC_REDIRECT_URI"),
            scopes: match option_env!("MOKOSH_OIDC_SCOPES") {
                Some(s) => s,
                None => "openid email offline_access",
            },
            hub_base_url: match option_env!("MOKOSH_HUB_BASE_URL") {
                Some(s) => s,
                None => "http://localhost:4400",
            },
        }
    }

    pub fn hub_url(&self, path: &str) -> String {
        format!("{}{}", self.hub_base_url.trim_end_matches('/'), path)
    }

    /// Resolve the redirect_uri. Falls back to `<origin>/auth/callback`
    /// at runtime when not pinned at compile time.
    pub fn resolve_redirect_uri(&self) -> Result<String, &'static str> {
        if let Some(s) = self.redirect_uri {
            return Ok(s.to_string());
        }
        let win = web_sys::window().ok_or("no window")?;
        let origin = win.location().origin().map_err(|_| "no origin")?;
        Ok(format!("{origin}/auth/callback"))
    }
}

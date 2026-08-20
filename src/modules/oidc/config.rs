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
                // MAPPS-368: empty by default. A deploy with no OIDC issuer
                // (no runtime injection, non-`msp.` host) then resolves to an
                // empty issuer, which flips the SPA to standalone
                // username/password login instead of a dead `/oauth2/authorize`.
                None => "",
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

    /// Build a config by resolving each field in priority order:
    ///   1. `window.__MOKOSH_CONFIG__` keys injected by the prod
    ///      container's entrypoint (`oidc_issuer`, `oidc_client_id`,
    ///      `hub_base_url`). Self-hosters override here without
    ///      rebuilding the image.
    ///   2. Host-prefix derivation for the canonical `msp.<tld>`
    ///      deploys: issuer `msp.<tld>` → `https://api.msp.<tld>`,
    ///      hub `msp.<tld>` → `https://<tld>` (Bunyip apex).
    ///   3. Compile-time `option_env!` defaults baked into the binary
    ///      (the `Self::from_env()` baseline).
    ///
    /// One image works for both staging (`msp.a8n.systems`),
    /// production (`msp.psa.systems`), and arbitrary self-hosted
    /// hostnames.
    ///
    /// The result is memoized in a thread-local cache so the up-to-three
    /// `Box::leak` allocations on first resolution stay bounded at one
    /// set per session even though this is called from five different
    /// call sites and re-runs each render. WASM is single-threaded so
    /// the thread-local is effectively a process-global.
    pub fn for_current_origin() -> Self {
        thread_local! {
            static CACHED: std::cell::RefCell<Option<OidcConfig>> =
                const { std::cell::RefCell::new(None) };
        }
        CACHED.with(|cell| {
            if let Some(cfg) = cell.borrow().as_ref() {
                return cfg.clone();
            }
            let cfg = Self::resolve();
            *cell.borrow_mut() = Some(cfg.clone());
            cfg
        })
    }

    /// Field-by-field resolution. Split out so [`for_current_origin`]
    /// can wrap it in a one-shot cache. Each leaked string corresponds
    /// to one field; at most three leaks per session.
    ///
    /// After the bunyip-as-OP cutover (docs/new-auth/mokosh) the issuer is
    /// bunyip-api itself, NOT mokosh-server. A `msp.<tld>` deploy resolves to
    /// issuer `https://api.<tld>` (bunyip-api) and hub `https://<tld>`
    /// (bunyip-web), so one SPA image targets staging and prod identically.
    ///
    /// Consequence (confirmed MAPPS-138): when an OIDC issuer IS configured,
    /// mokosh-server's own `/api/v1/auth/*` surface is not consumed by this
    /// SPA; the client authenticates via bunyip. MAPPS-368 adds the exception:
    /// when no issuer resolves (`has_issuer()` is false), the SPA falls back to
    /// mokosh-server's `/api/v1/auth/login` for standalone username/password
    /// sign-in.
    fn resolve() -> Self {
        let mut cfg = Self::from_env();

        let injected_issuer = crate::modules::runtime_config::get("oidc_issuer");
        let injected_client_id = crate::modules::runtime_config::get("oidc_client_id");
        let injected_hub = crate::modules::runtime_config::get("hub_base_url");
        // BUNYIP-142: per-deployment scope override. Lets c-01 / nc-01
        // opt in to bunyip's `profile` scope (so the JIT path receives
        // given_name + family_name claims) without rebuilding the SPA
        // image. Absent the override the compile-time default applies.
        let injected_scopes = crate::modules::runtime_config::get("oidc_scopes");

        // MAPPS-504: `None` on the desktop, which has no host to derive
        // from. A desktop install configures `oidc_issuer` / `hub_base_url`
        // explicitly (see `crate::platform::config`) or runs standalone.
        let host_rest = crate::platform::location::host()
            .and_then(|h| h.strip_prefix("msp.").map(str::to_string));

        if let Some(issuer) = injected_issuer {
            cfg.issuer = Box::leak(issuer.into_boxed_str());
        } else if let Some(rest) = host_rest.as_deref() {
            // Issuer = bunyip-api on the apex's `api.` subdomain. Pre-cutover
            // this was `https://api.msp.{rest}` (the mokosh-server host).
            cfg.issuer = Box::leak(format!("https://api.{rest}").into_boxed_str());
        }

        if let Some(client_id) = injected_client_id {
            cfg.client_id = Box::leak(client_id.into_boxed_str());
        }

        if let Some(hub) = injected_hub {
            cfg.hub_base_url = Box::leak(hub.into_boxed_str());
        } else if let Some(rest) = host_rest.as_deref() {
            cfg.hub_base_url = Box::leak(format!("https://{rest}").into_boxed_str());
        }

        if let Some(scopes) = injected_scopes {
            cfg.scopes = Box::leak(scopes.into_boxed_str());
        }

        cfg
    }

    pub fn hub_url(&self, path: &str) -> String {
        format!("{}{}", self.hub_base_url.trim_end_matches('/'), path)
    }

    /// MAPPS-368: whether a real OIDC issuer is configured (runtime-injected
    /// via `window.__MOKOSH_CONFIG__.oidc_issuer`, or `msp.<tld>`-host-derived).
    /// When this is false the deployment has no bunyip OP, and the SPA falls
    /// back to standalone username/password login against mokosh-server's
    /// `/api/v1/auth/login` instead of redirecting to `/oauth2/authorize`.
    pub fn has_issuer(&self) -> bool {
        !self.issuer.trim().is_empty()
    }

    /// Resolve the redirect_uri. Falls back to `<origin>/auth/callback`
    /// at runtime when not pinned at compile time.
    pub fn resolve_redirect_uri(&self) -> Result<String, &'static str> {
        if let Some(s) = self.redirect_uri {
            return Ok(s.to_string());
        }
        // MAPPS-505 will give the desktop build a loopback redirect URI.
        // Until then there is no origin to derive one from, and saying so
        // is what makes `start_login` fail loudly instead of sending the
        // OP a redirect_uri it cannot honour.
        let origin = crate::platform::location::origin()
            .ok_or("this build has no origin to derive a redirect URI from")?;
        Ok(format!("{origin}/auth/callback"))
    }
}

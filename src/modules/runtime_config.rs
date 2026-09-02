//! Runtime configuration injected by the production container.
//!
//! The mokosh-www OCI image's entrypoint writes a tiny
//! `/_mokosh_config.js` that sets `window.__MOKOSH_CONFIG__` from
//! env vars at container start. This module is the SPA-side reader.
//!
//! Lookup order, in priority:
//!   1. `window.__MOKOSH_CONFIG__.<field>` (this module)
//!   2. Host-prefix derivation for the canonical `msp.<tld>` deploys
//!      (`api_base()` in `hooks::fetch::api`,
//!      `OidcConfig::for_current_origin()`).
//!   3. Compile-time `option_env!()` defaults baked into the binary.
//!
//! In dev (or any deployment that does not set the env vars) the
//! window global is absent and every reader returns `None`, so the
//! existing fallback chain kicks in unchanged.

#[cfg(feature = "web")]
use wasm_bindgen::JsValue;

/// Read a string field off `window.__MOKOSH_CONFIG__`. Returns `None`
/// when either the global, the field, or its string conversion is
/// missing, and also when the resolved string is empty (operators
/// who leave an env var unset get the same outcome as if it was
/// never declared).
#[cfg(feature = "web")]
pub fn get(field: &str) -> Option<String> {
    let win = web_sys::window()?;
    let cfg = js_sys::Reflect::get(&win, &JsValue::from_str("__MOKOSH_CONFIG__")).ok()?;
    if cfg.is_undefined() || cfg.is_null() {
        return None;
    }
    let value = js_sys::Reflect::get(&cfg, &JsValue::from_str(field)).ok()?;
    let s = value.as_string()?;
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(not(feature = "web"))]
pub fn get(_field: &str) -> Option<String> {
    None
}

/// MAPPS-329: read a boolean feature-flag field off `window.__MOKOSH_CONFIG__`.
/// Returns true ONLY when the field is present AND the string parses as
/// truthy (`"true"` case-insensitive, or `"1"`). Anything else - including
/// the field being unset entirely - returns false so flags are locked-off
/// by default and need an explicit operator opt-in to enable.
pub fn flag_enabled(field: &str) -> bool {
    get(field).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
}

/// MAPPS-649: resolve the single portal host (typically
/// `portal.<apex>`) the running deploy serves the portal from. Checks
/// (in order) the container-emitted `window.__MOKOSH_CONFIG__.portal_host`
/// and the compile-time `MOKOSH_PORTAL_HOST` env fallback. Returns
/// `None` when neither is set (dev without env baked in, or a deploy
/// that does not serve the portal on a dedicated host).
///
/// Value is a bare host (no scheme, no leading dot). This replaces
/// the pre-649 `portal_host_suffix` shape which encoded a per-tenant
/// wildcard suffix (`.client.<apex>`).
pub fn portal_host() -> Option<String> {
    if let Some(v) = get("portal_host") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    option_env!("MOKOSH_PORTAL_HOST")
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// MAPPS-649: build the sign-in URL a customer follows to reach a
/// specific Company's portal. `portal_id` is the 9-digit numeric id
/// on `companies.portal_id`. Returns `None` when no portal host is
/// configured, so the caller can render a generic hint instead of a
/// broken URL.
pub fn portal_url_for_portal_id(portal_id: i64) -> Option<String> {
    let host = portal_host()?;
    // Dev's bare `portal.localhost:PORT` stays on http; anything else
    // uses https.
    let scheme = if host.contains("localhost") {
        "http"
    } else {
        "https"
    };
    Some(format!("{scheme}://{host}/portal/{portal_id}/login"))
}

/// MAPPS-649: the portal root URL (`https://portal.<apex>`) - useful
/// for a "here is where your customers sign in" hint on the tenant /
/// Company detail pages when a specific `portal_id` is not yet
/// available. Returns `None` when no portal host is configured.
pub fn portal_root_url() -> Option<String> {
    let host = portal_host()?;
    let scheme = if host.contains("localhost") {
        "http"
    } else {
        "https"
    };
    Some(format!("{scheme}://{host}"))
}

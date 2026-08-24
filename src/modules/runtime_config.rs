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

/// Resolve the portal host suffix for the running deploy, checking (in
/// order) the container-emitted `window.__MOKOSH_CONFIG__.portal_host_suffix`
/// and the compile-time `MOKOSH_PORTAL_HOST_SUFFIX` env fallback. Returns
/// `None` when neither is set (dev without env baked in, or a legacy
/// deploy that never hosted the portal on its own suffix).
///
/// Public so the agent-side "Create tenant" flow can preview the
/// portal URL a new tenant will inherit; the api module uses a
/// private mirror for its host derivation (see `hooks::fetch::api`).
/// Always includes the leading dot, e.g. `.client.a8n.systems`.
pub fn portal_host_suffix() -> Option<String> {
    if let Some(v) = get("portal_host_suffix") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    option_env!("MOKOSH_PORTAL_HOST_SUFFIX")
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Build the URL a customer would use to sign in to the tenant's own
/// client portal. Returns `None` when no portal-host suffix is
/// configured (i.e. the portal is not served on this deploy), so
/// the caller can render a generic hint instead of a broken URL.
pub fn portal_url_for_slug(slug: &str) -> Option<String> {
    let suffix = portal_host_suffix()?;
    let slug = slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        return None;
    }
    // Prefer https for real deploys; dev's `.client.localhost:PORT`
    // suffix stays on http.
    let scheme = if suffix.contains("localhost") {
        "http"
    } else {
        "https"
    };
    Some(format!("{scheme}://{slug}{suffix}"))
}

/// MAPPS-553: the inverse of `portal_url_for_slug`. Reads the current
/// browser-visible host, strips the configured `portal_host_suffix`,
/// and returns the leftmost label as the tenant slug for the SPA that
/// is currently rendering. Returns `None` when the SPA is served from
/// a non-portal host (apex, localhost without the portal suffix, or a
/// deploy that does not configure the suffix at all), so callers can
/// treat the absence as "not on a tenant subdomain" instead of a
/// broken slug. Lowercases the label; rejects a prefix that itself
/// contains a dot (that would be a nested subdomain we do not model).
#[cfg(feature = "web")]
pub fn tenant_slug_from_current_host() -> Option<String> {
    let suffix = portal_host_suffix()?;
    let host = web_sys::window()?.location().host().ok()?;
    let host_no_port = host.split(':').next().unwrap_or(&host).to_ascii_lowercase();
    let suffix_lower = suffix.to_ascii_lowercase();
    let prefix = host_no_port.strip_suffix(&suffix_lower)?;
    let slug = prefix.trim_end_matches('.');
    if slug.is_empty() || slug.contains('.') {
        return None;
    }
    Some(slug.to_string())
}

#[cfg(not(feature = "web"))]
pub fn tenant_slug_from_current_host() -> Option<String> {
    None
}

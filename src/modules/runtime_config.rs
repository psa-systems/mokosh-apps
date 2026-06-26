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

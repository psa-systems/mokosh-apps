//! Runtime configuration injected by the production container.
//!
//! The mokosh-www OCI image's entrypoint writes a tiny
//! `/_mokosh_config.js` that sets `window.__MOKOSH_CONFIG__` from
//! env vars at container start. This module is the SPA-side reader.
//!
//! MAPPS-504: on the desktop build there is no container and no window
//! global, so the same fields come from a `config.json` in the per-user
//! config directory (or a `MOKOSH_<FIELD>` environment variable). Both
//! are read through [`crate::platform::config`]; the lookup order below
//! is unchanged.
//!
//! Lookup order, in priority:
//!   1. `window.__MOKOSH_CONFIG__.<field>` / `config.json` (this module)
//!   2. Host-prefix derivation for the canonical `msp.<tld>` deploys
//!      (`api_base()` in `hooks::fetch::api`,
//!      `OidcConfig::for_current_origin()`).
//!   3. Compile-time `option_env!()` defaults baked into the binary.
//!
//! In dev (or any deployment that does not set the env vars) the
//! window global is absent and every reader returns `None`, so the
//! existing fallback chain kicks in unchanged.

/// Read a string field of the runtime configuration. Returns `None`
/// when it is absent or empty, so the caller's own fallback chain runs.
///
/// MAPPS-504: the source is [`crate::platform::config`] -
/// `window.__MOKOSH_CONFIG__` in the browser, `config.json` under the
/// per-user config directory on the desktop.
pub fn get(field: &str) -> Option<String> {
    crate::platform::config::get(field)
}

/// MAPPS-329: read a boolean feature-flag field off `window.__MOKOSH_CONFIG__`.
/// Returns true ONLY when the field is present AND the string parses as
/// truthy (`"true"` case-insensitive, or `"1"`). Anything else - including
/// the field being unset entirely - returns false so flags are locked-off
/// by default and need an explicit operator opt-in to enable.
pub fn flag_enabled(field: &str) -> bool {
    get(field).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1")
}

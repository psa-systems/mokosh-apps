//! Operator-supplied runtime configuration (MAPPS-504).
//!
//! Browser: the `window.__MOKOSH_CONFIG__` object the mokosh-www image's
//! entrypoint writes into `/_mokosh_config.js` from env vars at
//! container start.
//!
//! Desktop: a `config.json` in the per-user config directory, with a
//! `MOKOSH_<FIELD>` environment variable as an override. There is no
//! container entrypoint to inject a global into, and a desktop install
//! has no origin to derive anything from, so this file is where a
//! desktop user points the app at their server.
//!
//! Both return `None` for an absent OR empty value, so an operator who
//! leaves a setting blank gets the same result as one who never wrote
//! it, and the caller's own fallback chain runs.

#[cfg(target_arch = "wasm32")]
pub fn get(field: &str) -> Option<String> {
    use wasm_bindgen::JsValue;

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

#[cfg(not(target_arch = "wasm32"))]
pub fn get(field: &str) -> Option<String> {
    if let Ok(v) = std::env::var(format!("MOKOSH_{}", field.to_uppercase())) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    native::file().get(field).filter(|s| !s.is_empty()).cloned()
}

/// Where a desktop user puts their `config.json`, for the message that
/// tells them so.
#[cfg(not(target_arch = "wasm32"))]
pub fn file_path() -> Option<std::path::PathBuf> {
    Some(crate::platform::prefs::config_dir()?.join("config.json"))
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// Read once per run. The web side reads a global the container
    /// wrote at start and never re-reads it either, so the desktop side
    /// matching that means one resolution path, not two.
    pub(super) fn file() -> &'static HashMap<String, String> {
        static FILE: OnceLock<HashMap<String, String>> = OnceLock::new();
        FILE.get_or_init(load)
    }

    fn load() -> HashMap<String, String> {
        let Some(path) = super::file_path() else {
            tracing::warn!("no per-user config directory; using built-in defaults");
            return HashMap::new();
        };
        match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<HashMap<String, String>>(&raw) {
                Ok(map) => map,
                Err(e) => {
                    // Loud: the user wrote this file on purpose, and
                    // silently ignoring it would leave the app pointed
                    // at a default they thought they had replaced.
                    tracing::error!("{} is not valid JSON: {e}", path.display());
                    HashMap::new()
                }
            },
            // No file is the ordinary case: a build with its defaults
            // baked in needs none.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => {
                tracing::error!("could not read {}: {e}", path.display());
                HashMap::new()
            }
        }
    }
}

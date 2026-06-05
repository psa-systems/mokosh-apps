//! Tiny typed wrapper over `localStorage` for per-user KB UI prefs.
//! Non-web builds and storage errors degrade to the provided default
//! so callers never have to special-case the environment. Values are
//! stored as "1"/"0".

/// Read a boolean pref, falling back to `default` when unset or storage
/// is unavailable.
pub fn get_bool(key: &str, default: bool) -> bool {
    #[cfg(feature = "web")]
    {
        let Some(window) = web_sys::window() else {
            return default;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return default;
        };
        match storage.get_item(key) {
            Ok(Some(v)) => v == "1",
            _ => default,
        }
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = key;
        default
    }
}

/// Persist a boolean pref. No-op off web or on storage error.
pub fn set_bool(key: &str, value: bool) {
    #[cfg(feature = "web")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return;
        };
        let _ = storage.set_item(key, if value { "1" } else { "0" });
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = (key, value);
    }
}

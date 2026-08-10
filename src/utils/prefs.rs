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

/// Read a string pref, falling back to `default` when unset or storage
/// is unavailable. Used for enum-shaped preferences (theme, time
/// format, first day of week) where the set of valid values is small
/// and matched on the caller side.
pub fn get_str(key: &str, default: &str) -> String {
    #[cfg(feature = "web")]
    {
        let Some(window) = web_sys::window() else {
            return default.to_string();
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return default.to_string();
        };
        match storage.get_item(key) {
            Ok(Some(v)) if !v.is_empty() => v,
            _ => default.to_string(),
        }
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = key;
        default.to_string()
    }
}

/// Persist a string pref. No-op off web or on storage error.
pub fn set_str(key: &str, value: &str) {
    #[cfg(feature = "web")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return;
        };
        let _ = storage.set_item(key, value);
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = (key, value);
    }
}

/// PMS-754: drop a stored value. Used to clear an in-progress draft once it has
/// been saved or deliberately discarded, so the next New Form starts empty.
pub fn clear(key: &str) {
    #[cfg(feature = "web")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return;
        };
        let _ = storage.remove_item(key);
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = key;
    }
}

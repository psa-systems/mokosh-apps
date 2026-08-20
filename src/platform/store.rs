//! Session-scoped key/value storage (MAPPS-504).
//!
//! Browser: `sessionStorage`. It is tab-scoped and cleared when the tab
//! closes, which is why the OIDC tokens live there and never in
//! `localStorage` (see `docs/oidc-token-storage.md`).
//!
//! Desktop: an in-process map. A desktop window IS the session, so the
//! lifetime matches, and keeping it off disk preserves the property the
//! browser choice was made for - the tokens do not outlive the run.
//!
//! The method signatures mirror `web_sys::Storage` so the OIDC storage
//! module reads the same on both targets.

/// A handle to the session store. Cheap to obtain; not cached by the
/// callers, matching how `web_sys::Storage` is used.
pub struct Store {
    #[cfg(target_arch = "wasm32")]
    inner: web_sys::Storage,
}

/// Obtain the session store.
///
/// The browser can genuinely refuse (`sessionStorage` disabled by
/// policy or in a partitioned context), so this is fallible and the
/// reason is carried out to the caller rather than flattened into an
/// empty store, which would read as "signed out" instead of "storage
/// unavailable".
#[cfg(target_arch = "wasm32")]
pub fn session() -> Result<Store, String> {
    let inner = web_sys::window()
        .ok_or_else(|| "no window".to_string())?
        .session_storage()
        .map_err(|_| "no sessionStorage handle".to_string())?
        .ok_or_else(|| "sessionStorage disabled".to_string())?;
    Ok(Store { inner })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn session() -> Result<Store, String> {
    Ok(Store {})
}

#[cfg(not(target_arch = "wasm32"))]
fn map() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(Default::default)
}

impl Store {
    #[cfg(target_arch = "wasm32")]
    pub fn get_item(&self, key: &str) -> Result<Option<String>, String> {
        self.inner
            .get_item(key)
            .map_err(|_| "sessionStorage read failed".to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_item(&self, key: &str, value: &str) -> Result<(), String> {
        self.inner
            .set_item(key, value)
            .map_err(|_| "sessionStorage write failed".to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn remove_item(&self, key: &str) -> Result<(), String> {
        self.inner
            .remove_item(key)
            .map_err(|_| "sessionStorage delete failed".to_string())
    }

    /// A poisoned lock means another thread panicked mid-write. The
    /// stored strings are independent values, not an invariant one
    /// panic can have broken, so recovering the guard is correct and
    /// losing the session to a `PoisonError` would not be.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_item(&self, key: &str) -> Result<Option<String>, String> {
        Ok(map()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_item(&self, key: &str, value: &str) -> Result<(), String> {
        map()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn remove_item(&self, key: &str) -> Result<(), String> {
        map().lock().unwrap_or_else(|e| e.into_inner()).remove(key);
        Ok(())
    }
}

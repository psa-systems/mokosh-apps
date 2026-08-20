//! Durable per-user key/value storage (MAPPS-504).
//!
//! Browser: `localStorage`. Desktop: a JSON file under the per-user
//! config directory, which is what gives the desktop build the property
//! `localStorage` provides - a preference set today is still set
//! tomorrow.
//!
//! Never used for tokens. Those go to [`crate::platform::store`], which
//! does not outlive the session.

/// Read a raw value. `None` when unset or when the store is
/// unreachable; the typed accessors in `crate::utils::prefs` fold that
/// into their caller-supplied default.
#[cfg(target_arch = "wasm32")]
pub fn get(key: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(key)
        .ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn set(key: &str, value: &str) {
    let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    else {
        return;
    };
    if let Err(_e) = storage.set_item(key, value) {
        // Quota exceeded, or storage disabled mid-session. Best-effort:
        // a lost UI preference must not take the interaction with it.
        tracing::warn!("could not persist preference {key}");
    }
}

#[cfg(target_arch = "wasm32")]
pub fn remove(key: &str) {
    let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    else {
        return;
    };
    if storage.remove_item(key).is_err() {
        tracing::warn!("could not clear preference {key}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    /// `~/.config/mokosh-apps/prefs.json` on Linux, the platform
    /// equivalent elsewhere. `None` when the OS reports no config
    /// directory at all, in which case preferences stay in memory for
    /// the run and the reason is logged once at load.
    pub(super) fn path() -> Option<PathBuf> {
        Some(super::config_dir()?.join("prefs.json"))
    }

    fn cache() -> &'static Mutex<HashMap<String, String>> {
        static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(load()))
    }

    fn load() -> HashMap<String, String> {
        let Some(path) = path() else {
            tracing::warn!(
                "no per-user config directory; preferences will not persist across runs"
            );
            return HashMap::new();
        };
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                // Corrupt file: start clean rather than refusing to run,
                // but say so, because the user's settings just vanished.
                tracing::error!("preferences at {} are unreadable: {e}", path.display());
                HashMap::new()
            }),
            // A missing file is the first run, not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => {
                tracing::error!("could not read preferences at {}: {e}", path.display());
                HashMap::new()
            }
        }
    }

    fn flush(map: &HashMap<String, String>) {
        let Some(path) = path() else {
            return;
        };
        let Some(dir) = path.parent() else {
            tracing::error!("preferences path {} has no parent", path.display());
            return;
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::error!("could not create {}: {e}", dir.display());
            return;
        }
        match serde_json::to_string_pretty(map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::error!("could not write preferences to {}: {e}", path.display());
                }
            }
            Err(e) => tracing::error!("could not encode preferences: {e}"),
        }
    }

    fn locked<T>(f: impl FnOnce(&mut HashMap<String, String>) -> T) -> T {
        // See the note in `platform::store`: the entries are independent
        // values, so recovering a poisoned guard is correct.
        let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    pub(super) fn get(key: &str) -> Option<String> {
        locked(|m| m.get(key).cloned())
    }

    pub(super) fn set(key: &str, value: &str) {
        locked(|m| {
            m.insert(key.to_string(), value.to_string());
            flush(m);
        });
    }

    pub(super) fn remove(key: &str) {
        locked(|m| {
            m.remove(key);
            flush(m);
        });
    }
}

/// The app's per-user configuration directory. Shared by the
/// preferences file and the runtime-config file that
/// [`crate::platform::config`] reads.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("mokosh-apps"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get(key: &str) -> Option<String> {
    native::get(key)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set(key: &str, value: &str) {
    native::set(key, value);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove(key: &str) {
    native::remove(key);
}

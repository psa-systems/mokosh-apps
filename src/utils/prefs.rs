//! Tiny typed wrapper over the durable key/value store for per-user KB
//! UI prefs. Storage errors degrade to the provided default so callers
//! never have to special-case the environment. Values are stored as
//! "1"/"0".
//!
//! MAPPS-504: the store itself is [`crate::platform::prefs`] -
//! `localStorage` in the browser, a JSON file under the per-user config
//! directory on the desktop.

use crate::platform::prefs as store;

/// Read a boolean pref, falling back to `default` when unset or storage
/// is unavailable.
pub fn get_bool(key: &str, default: bool) -> bool {
    // SPEC-4.78.1
    match store::get(key) {
        Some(v) => v == "1",
        None => default,
    }
}

/// Persist a boolean pref. No-op on storage error.
pub fn set_bool(key: &str, value: bool) {
    // SPEC-4.78.2, SPEC-4.78.6: the store logs a failed write; no error is returned.
    store::set(key, if value { "1" } else { "0" });
}

/// Read a string pref, falling back to `default` when unset or storage
/// is unavailable. Used for enum-shaped preferences (theme, time
/// format, first day of week) where the set of valid values is small
/// and matched on the caller side.
pub fn get_str(key: &str, default: &str) -> String {
    // SPEC-4.78.3
    match store::get(key) {
        Some(v) if !v.is_empty() => v,
        _ => default.to_string(),
    }
}

/// Persist a string pref. No-op on storage error.
pub fn set_str(key: &str, value: &str) {
    // SPEC-4.78.4
    store::set(key, value);
}

/// PMS-754: drop a stored value. Used to clear an in-progress draft once it has
/// been saved or deliberately discarded, so the next New Form starts empty.
pub fn clear(key: &str) {
    // SPEC-4.78.5
    store::remove(key);
}

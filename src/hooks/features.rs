//! Which runtime feature toggles the server has turned on (MAPPS-955).
//!
//! The server publishes two surfaces (PMS-1414):
//! - `GET /api/v1/public/config` returns `{features: {key: bool}}` without
//!   auth; this is the one the gating hook reads.
//! - `GET/PUT /api/v1/settings/feature-toggles` lets a super admin flip
//!   a key; the admin page uses that.
//!
//! The gating signal is a global `HashMap<String, bool>` read into a shared
//! `GlobalSignal` once at startup and then every 60 seconds, plus an explicit
//! bump after an admin flip so the switcher is instant from the actor's
//! point of view. A missing key, an unreachable server, or an unloaded cache
//! all read as **off** — the ticket's AC is "unreachable server leaves every
//! gated feature hidden". The 60s cadence matches the ticket; the ticker uses
//! the same `spawn + sleep_ms` loop pattern as `use_periodic_tick` in
//! `src/pages/big_view.rs`.

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;

/// Shape of `GET /api/v1/public/config`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PublicConfig {
    #[serde(default)]
    pub features: HashMap<String, bool>,
}

/// What the server currently has turned on. `None` while the first read is
/// in flight or has failed; readers resolve this to "off" per the ticket's
/// negative-case AC.
pub static FEATURE_MAP: GlobalSignal<Option<HashMap<String, bool>>> =
    Signal::global(|| None);

/// Monotonic counter the admin page bumps on a successful flip so other
/// panels on the page re-read without waiting for the 60s tick.
pub static FEATURES_GENERATION: GlobalSignal<u64> = Signal::global(|| 0);

const REFRESH_INTERVAL_SECS: u32 = 60;

/// Load `/public/config` once. Called on first mount and on every tick.
/// A failed read leaves the previous map in place (so a transient outage
/// does not flip every gate off); the first-read failure keeps `None`,
/// which gates read as off, which is the right fail-closed posture.
#[cfg(feature = "app")]
pub async fn refresh_feature_map() {
    let result =
        crate::hooks::fetch::api::get_authed::<PublicConfig>("/public/config").await;
    match result {
        Ok(cfg) => {
            *FEATURE_MAP.write() = Some(cfg.features);
        }
        Err(err) => {
            tracing::warn!("feature-map refresh failed: {err}");
        }
    }
}

/// Mount the shared feature-map ticker. Idempotent per component: the
/// first component to call this starts the loop, every later caller
/// re-uses the same `GlobalSignal`. Called once from the root layout.
pub fn use_feature_map_refresh() {
    #[cfg(feature = "app")]
    {
        use_effect(move || {
            dioxus::prelude::spawn(async move {
                // One prime read so the first render has data instead of
                // a 60-second window of every gate off.
                refresh_feature_map().await;
                loop {
                    crate::platform::timer::sleep_ms(REFRESH_INTERVAL_SECS * 1000).await;
                    refresh_feature_map().await;
                }
            });
        });
    }
}

/// Is `key` on? A missing key, an unloaded map, or a failed read all
/// resolve to false — the ticket's "negative case" AC.
pub fn feature_enabled(map: &Option<HashMap<String, bool>>, key: &str) -> bool {
    map.as_ref()
        .and_then(|m| m.get(key).copied())
        .unwrap_or(false)
}

/// Reader hook: subscribes the caller to the shared signal and the
/// generation bump the admin page emits, so a flipped feature on this
/// user's own session reflects the next render.
pub fn use_feature_enabled(key: &str) -> bool {
    let _generation = *FEATURES_GENERATION.read();
    let map = FEATURE_MAP.read();
    feature_enabled(&map, key)
}

/// Admin-page helper: a key list for render.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureRow {
    pub key: String,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unloaded_map_reads_as_off_for_every_key() {
        let map: Option<HashMap<String, bool>> = None;
        assert!(!feature_enabled(&map, "any_key"));
    }

    #[test]
    fn a_missing_key_reads_as_off_even_with_a_loaded_map() {
        let mut m = HashMap::new();
        m.insert("other_feature".to_string(), true);
        let map = Some(m);
        assert!(!feature_enabled(&map, "absent_feature"));
    }

    #[test]
    fn a_present_key_reads_its_bool() {
        let mut m = HashMap::new();
        m.insert("kb_quote_notes".to_string(), true);
        m.insert("other_feature".to_string(), false);
        let map = Some(m);
        assert!(feature_enabled(&map, "kb_quote_notes"));
        assert!(!feature_enabled(&map, "other_feature"));
    }
}

//! Which runtime feature toggles the server has turned on (MAPPS-955).
//!
//! The server publishes two surfaces (PMS-1414):
//! - `GET /api/v1/public/config` returns `{features: {key: bool}}` and
//!   needs no auth; this is the one the gating hook reads. The read goes
//!   through `get_authed`, which attaches the bearer when one is held, so a
//!   signed-in session is not a different request path (MAPPS-1238: this
//!   said the read itself was unauthenticated).
//! - `GET/PUT /api/v1/settings/feature-toggles` lets a super admin flip
//!   a key; the admin page uses that.
//!
//! The gating signal is a global `HashMap<String, bool>` read into a shared
//! `GlobalSignal` once at startup and then every 60 seconds, plus an explicit
//! bump after an admin flip so the switcher is instant from the actor's
//! point of view. A missing key, an unreachable server, or an unloaded cache
//! all read as **off** (the ticket's AC is "unreachable server leaves every
//! gated feature hidden"). The 60s cadence matches the ticket; the ticker uses
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
pub static FEATURE_MAP: GlobalSignal<Option<HashMap<String, bool>>> = Signal::global(|| None);

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
    let result = crate::hooks::fetch::api::get_authed::<PublicConfig>("/public/config").await;
    match result {
        Ok(cfg) => {
            *FEATURE_MAP.write() = Some(cfg.features);
        }
        Err(err) => {
            tracing::warn!("feature-map refresh failed: {err}");
        }
    }
}

/// Set while a refresh loop is running, so a second caller does not start
/// another (MAPPS-1238).
#[cfg(feature = "app")]
static LOOP_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Claim the one loop slot, or `None` when a loop already holds it. The
/// claim is released when dropped, which is when the owning component's task
/// is cancelled, so a remounted root starts a fresh loop.
#[cfg(feature = "app")]
fn claim_loop() -> Option<LoopClaim> {
    use std::sync::atomic::Ordering;
    LOOP_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .ok()
        .map(|_| LoopClaim)
}

#[cfg(feature = "app")]
struct LoopClaim;

#[cfg(feature = "app")]
impl Drop for LoopClaim {
    fn drop(&mut self) {
        LOOP_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Mount the shared feature-map ticker. The first caller starts the loop;
/// a later caller, while it runs, re-uses the same `GlobalSignal` and starts
/// nothing (MAPPS-1238: there was no guard, so each call spawned its own
/// 60-second loop). Called once from the root layout.
pub fn use_feature_map_refresh() {
    #[cfg(feature = "app")]
    {
        use_effect(move || {
            let Some(claim) = claim_loop() else {
                tracing::debug!("feature-map refresh loop already running; not starting another");
                return;
            };
            dioxus::prelude::spawn(async move {
                // Held for the task's lifetime; dropping it frees the slot.
                let _claim = claim;
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
/// resolve to false, the ticket's "negative case" AC.
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

    /// MAPPS-1238: one loop at a time, and the slot comes back when the
    /// loop's task is dropped.
    #[test]
    fn only_one_refresh_loop_holds_the_slot_at_a_time() {
        let first = claim_loop().expect("the first caller starts the loop");
        assert!(
            claim_loop().is_none(),
            "a second caller must not start another"
        );
        drop(first);
        let again = claim_loop().expect("a remount after the loop ended starts a new one");
        drop(again);
    }

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

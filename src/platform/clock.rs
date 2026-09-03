//! Wall-clock reads (MAPPS-504).
//!
//! No target split: `chrono`'s `wasmbind` feature (enabled in
//! `Cargo.toml`) already routes `Utc::now()` through `Date.now()` in the
//! browser and through the OS clock everywhere else, so the call sites
//! that used `js_sys::Date::now()` directly get the same value here
//! without a browser binding.

/// Milliseconds since the Unix epoch.
///
/// Clamped at zero: the callers store this in a `u64` and compare it to
/// an expiry, and a pre-1970 clock (a desktop machine with its RTC
/// unset) would otherwise wrap to an enormous value and make every
/// stored token look fresh forever.
pub fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

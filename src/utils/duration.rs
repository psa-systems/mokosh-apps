//! Logged-duration display formatting (PMS-265).
//!
//! Time entries are stored on the server as whole `duration_minutes`.
//! The SPA offers two display shapes, selectable per-device in the
//! Profile "Duration format" preference:
//!
//! - `decimal` (default): one-decimal hours with an `h` suffix, e.g.
//!   30 min -> "0.5h", 90 min -> "1.5h". The original shape; kept as the
//!   default so existing users see no change until they opt in.
//! - `hm`: `H:MM` clock style, e.g. 30 min -> "0:30", 90 min -> "1:30".
//!   Matches how most timesheet tools present logged time.
//!
//! `fmt_duration` reads the stored preference on each call (a short
//! string in `localStorage`) so a change on the Profile page is
//! reflected on the next render without threading the value through
//! every component.

use crate::utils::prefs;

/// localStorage key for the duration display preference. Stable across
/// releases; renaming would silently reset every user back to default.
pub const PREF_DURATION_FORMAT: &str = "mokosh_duration_format";

/// Preference value when the user has chosen nothing. Decimal preserves
/// the long-standing behaviour (PMS-265 adds H:MM as the opt-in).
pub const DEFAULT_DURATION_FORMAT: &str = "decimal";

/// `H:MM` from whole minutes (e.g. 30 -> "0:30", 90 -> "1:30"). Negative
/// inputs are clamped to zero; minutes are zero-padded to two digits.
pub fn fmt_hm(minutes: i64) -> String {
    let m = minutes.max(0);
    format!("{}:{:02}", m / 60, m % 60)
}

/// One-decimal hours with an `h` suffix (e.g. 90 -> "1.5h"). The legacy
/// shape, kept for users who read time in fractions.
pub fn fmt_decimal(minutes: i64) -> String {
    format!("{:.1}h", minutes as f64 / 60.0)
}

/// Format a logged duration using the user's stored preference, falling
/// back to decimal when unset or off-web.
pub fn fmt_duration(minutes: i64) -> String {
    match prefs::get_str(PREF_DURATION_FORMAT, DEFAULT_DURATION_FORMAT).as_str() {
        "hm" => fmt_hm(minutes),
        _ => fmt_decimal(minutes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hm_zero() {
        assert_eq!(fmt_hm(0), "0:00");
    }

    #[test]
    fn hm_sub_hour_pads_minutes() {
        assert_eq!(fmt_hm(30), "0:30");
        assert_eq!(fmt_hm(5), "0:05");
    }

    #[test]
    fn hm_exact_hour() {
        assert_eq!(fmt_hm(60), "1:00");
        assert_eq!(fmt_hm(120), "2:00");
    }

    #[test]
    fn hm_mixed() {
        assert_eq!(fmt_hm(90), "1:30");
        assert_eq!(fmt_hm(145), "2:25");
    }

    #[test]
    fn hm_negative_clamps_to_zero() {
        assert_eq!(fmt_hm(-15), "0:00");
    }

    #[test]
    fn decimal_shape() {
        assert_eq!(fmt_decimal(0), "0.0h");
        assert_eq!(fmt_decimal(30), "0.5h");
        assert_eq!(fmt_decimal(90), "1.5h");
    }
}

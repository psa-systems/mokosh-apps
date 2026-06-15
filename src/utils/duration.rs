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

/// Parse a user-typed duration into whole minutes (PMS-314). Accepts two
/// shapes and is the single source of truth for the Log Time / Edit Time
/// Entry "Hours" field:
///
/// - `H:MM` clock style: hours `>= 0`, minutes `00..=59`. e.g. `0:30` -> 30,
///   `1:30` -> 90, `2:05` -> 125. The minutes side may be one or two digits
///   (`0:5` -> 5) but must be below 60.
/// - decimal hours: `2.5` -> 150, `.25` -> 15, `8` -> 480. Rounded to the
///   nearest whole minute, matching the legacy `(hours * 60).round()` path.
///
/// Returns `None` for empty, negative, or malformed input (`1:60`, `1:`,
/// `1:2:3`, `abc`). Callers enforce the upper bound (24h) and reject zero;
/// keeping those policies at the call site preserves the existing form
/// error message.
pub fn parse_input_to_minutes(input: &str) -> Option<i64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    if let Some((h_str, m_str)) = s.split_once(':') {
        // H:MM. Reject a second colon, empty sides, and minutes >= 60.
        if m_str.contains(':') {
            return None;
        }
        let hours: i64 = h_str.trim().parse().ok()?;
        let mins: i64 = m_str.trim().parse().ok()?;
        if hours < 0 || !(0..60).contains(&mins) {
            return None;
        }
        return Some(hours * 60 + mins);
    }

    // Decimal hours.
    let hours: f64 = s.parse().ok()?;
    if !hours.is_finite() || hours < 0.0 {
        return None;
    }
    Some((hours * 60.0).round() as i64)
}

/// A parseable, preference-aware pre-fill for the "Hours" input (PMS-314):
/// `H:MM` when the duration-format pref is `hm`, otherwise a trimmed decimal
/// with no `h` suffix. Both round-trip back through
/// [`parse_input_to_minutes`] to the same minute (2-dp decimal is exact to
/// within 0.3 min, which `round()` recovers).
pub fn fmt_input(minutes: i64) -> String {
    match prefs::get_str(PREF_DURATION_FORMAT, DEFAULT_DURATION_FORMAT).as_str() {
        "hm" => fmt_hm(minutes),
        _ => fmt_input_decimal(minutes),
    }
}

/// Decimal hours with no `h` suffix and trailing zeros trimmed, so
/// 90 -> "1.5", 480 -> "8", 15 -> "0.25". Two decimals keep it parseable
/// back to the exact minute. The pure (pref-free) half of [`fmt_input`].
fn fmt_input_decimal(minutes: i64) -> String {
    let s = format!("{:.2}", minutes.max(0) as f64 / 60.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Like [`parse_input_to_minutes`] but returns decimal **hours**, for fields
/// stored as fractional hours rather than whole minutes (PMS-319: task
/// estimated_hours). `"1:30"` -> `1.5`, `"2.5"` -> `2.5`, `"0"` -> `0.0`.
/// `None` on the same malformed/negative input the minute parser rejects.
pub fn parse_input_to_hours(input: &str) -> Option<f64> {
    parse_input_to_minutes(input).map(|m| m as f64 / 60.0)
}

/// Preference-aware pre-fill for an hours-valued field (PMS-319), mirroring
/// [`fmt_input`]. The stored hours are quantized to the nearest minute first
/// so the `H:MM` shape is exact.
pub fn fmt_input_hours(hours: f64) -> String {
    fmt_input((hours * 60.0).round() as i64)
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

    #[test]
    fn parse_hm() {
        assert_eq!(parse_input_to_minutes("0:30"), Some(30));
        assert_eq!(parse_input_to_minutes("1:30"), Some(90));
        assert_eq!(parse_input_to_minutes("2:05"), Some(125));
        assert_eq!(parse_input_to_minutes("0:00"), Some(0));
        assert_eq!(parse_input_to_minutes("24:00"), Some(1440));
        // One-digit minutes are allowed as long as they are below 60.
        assert_eq!(parse_input_to_minutes("0:5"), Some(5));
        // Surrounding whitespace is tolerated.
        assert_eq!(parse_input_to_minutes("  1:15  "), Some(75));
    }

    #[test]
    fn parse_decimal() {
        assert_eq!(parse_input_to_minutes("2.5"), Some(150));
        assert_eq!(parse_input_to_minutes("0.25"), Some(15));
        assert_eq!(parse_input_to_minutes("8"), Some(480));
        assert_eq!(parse_input_to_minutes(".25"), Some(15));
        // Rounds to the nearest whole minute.
        assert_eq!(parse_input_to_minutes("0.17"), Some(10));
    }

    #[test]
    fn parse_rejects_malformed() {
        assert_eq!(parse_input_to_minutes(""), None);
        assert_eq!(parse_input_to_minutes("   "), None);
        assert_eq!(parse_input_to_minutes("abc"), None);
        assert_eq!(parse_input_to_minutes("1:60"), None); // minutes out of range
        assert_eq!(parse_input_to_minutes("1:"), None); // empty minutes
        assert_eq!(parse_input_to_minutes(":30"), None); // empty hours
        assert_eq!(parse_input_to_minutes("1:2:3"), None); // second colon
        assert_eq!(parse_input_to_minutes("-1"), None); // negative decimal
        assert_eq!(parse_input_to_minutes("-1:30"), None); // negative hours
        assert_eq!(parse_input_to_minutes("1:-5"), None); // negative minutes
    }

    #[test]
    fn fmt_input_decimal_round_trips() {
        // The decimal pre-fill half (the `hm` half is just `fmt_hm`, tested
        // above). Tested directly to stay pref-free: `fmt_input` reads
        // localStorage, which is not reachable on the native test target.
        assert_eq!(fmt_input_decimal(90), "1.5");
        assert_eq!(fmt_input_decimal(480), "8");
        assert_eq!(fmt_input_decimal(15), "0.25");
        assert_eq!(fmt_input_decimal(0), "0");
        // Every minute value round-trips back through the parser.
        for m in [5_i64, 10, 25, 55, 125, 1440] {
            assert_eq!(parse_input_to_minutes(&fmt_input_decimal(m)), Some(m));
        }
    }

    #[test]
    fn parse_hours_hm_and_decimal() {
        assert_eq!(parse_input_to_hours("1:30"), Some(1.5));
        assert_eq!(parse_input_to_hours("0:30"), Some(0.5));
        assert_eq!(parse_input_to_hours("2.5"), Some(2.5));
        assert_eq!(parse_input_to_hours("8"), Some(8.0));
        assert_eq!(parse_input_to_hours("0"), Some(0.0));
        assert_eq!(parse_input_to_hours(""), None);
        assert_eq!(parse_input_to_hours("abc"), None);
        assert_eq!(parse_input_to_hours("1:60"), None);
        assert_eq!(parse_input_to_hours("-2"), None);
    }
}

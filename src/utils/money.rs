//! Currency formatting (MAPPS-197).
//!
//! One implementation so every screen renders money identically: grouped
//! thousands, two decimal places, a leading `$`, a `-` sign for negatives, and
//! `-` for an absent value. Previously projects (`fmt_money`, f64, no
//! separators/cents), billing (`money`, raw server string, no separators), and
//! contracts (`format_money`, the correct one) each formatted money
//! differently. This module is the single source of truth; the per-screen
//! helpers were removed in favor of these functions.

use rust_decimal::Decimal;
use std::str::FromStr;

/// Format a `Decimal` as `$1,234.56`: grouped thousands, two decimals, leading
/// `$`, and a `-` sign for negative amounts.
pub fn format_money(amount: Decimal) -> String {
    // SPEC-4.58.1: round first, then sign and group.
    let rounded = amount.round_dp(2);
    let negative = rounded.is_sign_negative();
    let abs = rounded.abs();
    // SPEC-4.58.2, SPEC-4.58.4
    let s = format!("{abs:.2}");
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s.as_str(), "00"));
    let mut grouped = String::new();
    let digits: Vec<char> = int_part.chars().collect();
    for (i, ch) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*ch);
    }
    // SPEC-4.58.3: sign goes before the dollar sign.
    let sign = if negative { "-" } else { "" };
    format!("{sign}${grouped}.{frac_part}")
}

/// Format minor units (cents) as `$1,234.56` (MAPPS-1157).
///
/// Bills and expenses store money as `i64` minor units, matching the
/// schema's `*_minor` columns, where invoices serialise a decimal string.
/// Rendering goes through the same formatter either way, so a bill total
/// and an invoice total are grouped and signed identically.
// SPEC-4.58.5
pub fn format_money_minor(minor: i64) -> String {
    format_money(Decimal::new(minor, 2))
}

/// Format an optional `Decimal`; `None` renders as `-`.
// SPEC-4.58.6
pub fn format_money_opt(amount: Option<Decimal>) -> String {
    match amount {
        Some(d) => format_money(d),
        None => "-".to_string(),
    }
}

/// What a value that is present but cannot be shown as money renders as
/// (MAPPS-1241). Distinct from `-`, which means "no amount", so a failed
/// conversion is not mistaken for an absent one; the value is logged.
pub const INVALID_AMOUNT: &str = "Invalid amount";

/// Format an optional `f64` amount (the projects budget/actual fields are
// SPEC-4.58.7
/// `f64`). Converts to `Decimal` for display; `None` renders as `-`, and a
/// NaN or infinite value as [`INVALID_AMOUNT`], logged with the value.
pub fn format_money_f64(v: Option<f64>) -> String {
    let Some(v) = v else {
        return "-".to_string();
    };
    match Decimal::from_f64_retain(v) {
        Some(d) => format_money(d),
        None => {
            tracing::warn!("cannot show {v} as an amount");
            INVALID_AMOUNT.to_string()
        }
    }
}

/// Format a server-serialized decimal string (billing amounts arrive as
/// strings, e.g. `"60000.00"`). An empty value renders as `-`, matching
/// `format_money_opt`/`format_money_f64`; an unparseable one renders as
/// [`INVALID_AMOUNT`] and is logged with the value (MAPPS-1241). A call site that
/// wants a zero default instead of `-` must supply `"0"` explicitly, e.g.
/// `.unwrap_or_else(|| "0".into())`, before calling this function.
pub fn format_money_str(raw: &str) -> String {
    // SPEC-4.58.8, SPEC-4.58.10
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "-".to_string();
    }
    // SPEC-4.58.9
    match Decimal::from_str(trimmed) {
        Ok(d) => format_money(d),
        Err(e) => {
            tracing::warn!("the server sent the amount {trimmed:?}, which is not a decimal: {e}");
            INVALID_AMOUNT.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands_and_keeps_two_decimals() {
        assert_eq!(format_money(Decimal::from(60000)), "$60,000.00");
        assert_eq!(format_money(Decimal::from(150)), "$150.00");
        assert_eq!(
            format_money(Decimal::from_str("1234567.5").unwrap()),
            "$1,234,567.50"
        );
    }

    #[test]
    fn handles_negative_and_small() {
        assert_eq!(format_money(Decimal::from(-50)), "-$50.00");
        assert_eq!(format_money(Decimal::ZERO), "$0.00");
    }

    #[test]
    fn opt_and_adapters() {
        assert_eq!(format_money_opt(None), "-");
        assert_eq!(format_money_opt(Some(Decimal::from(5))), "$5.00");
        assert_eq!(format_money_f64(None), "-");
        assert_eq!(format_money_f64(Some(60000.0)), "$60,000.00");
        assert_eq!(format_money_str("60000.00"), "$60,000.00");
        assert_eq!(format_money_str(""), "-");
        assert_eq!(format_money_str("not-a-number"), INVALID_AMOUNT);
    }

    /// MAPPS-1241: a failed conversion is not rendered like an absent amount.
    #[test]
    fn a_failed_conversion_is_distinct_from_no_amount() {
        assert_eq!(format_money_f64(Some(f64::NAN)), INVALID_AMOUNT);
        assert_eq!(format_money_f64(Some(f64::INFINITY)), INVALID_AMOUNT);
        assert_eq!(format_money_str("12,00"), INVALID_AMOUNT);
        assert_eq!(format_money_str("   "), "-");
        assert_ne!(INVALID_AMOUNT, "-");
    }
}

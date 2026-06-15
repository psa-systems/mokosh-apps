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
    let rounded = amount.round_dp(2);
    let negative = rounded.is_sign_negative();
    let abs = rounded.abs();
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
    let sign = if negative { "-" } else { "" };
    format!("{sign}${grouped}.{frac_part}")
}

/// Format an optional `Decimal`; `None` renders as `-`.
pub fn format_money_opt(amount: Option<Decimal>) -> String {
    match amount {
        Some(d) => format_money(d),
        None => "-".to_string(),
    }
}

/// Format an optional `f64` amount (the projects budget/actual fields are
/// `f64`). Converts to `Decimal` for display; `None` or an unrepresentable
/// value renders as `-`.
pub fn format_money_f64(v: Option<f64>) -> String {
    match v.and_then(Decimal::from_f64_retain) {
        Some(d) => format_money(d),
        None => "-".to_string(),
    }
}

/// Format a server-serialized decimal string (billing amounts arrive as
/// strings, e.g. `"60000.00"`). An empty or unparseable value renders as
/// `$0.00`, preserving the previous billing-helper behavior.
pub fn format_money_str(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "$0.00".to_string();
    }
    match Decimal::from_str(trimmed) {
        Ok(d) => format_money(d),
        Err(_) => "$0.00".to_string(),
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
        assert_eq!(format_money_str(""), "$0.00");
        assert_eq!(format_money_str("not-a-number"), "$0.00");
    }
}

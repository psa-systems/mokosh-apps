//! WCAG contrast math (MAPPS-259).
//!
//! Pure, dependency-free, and native-testable (no web_sys), so the
//! accent catalog's "passes AA on both bases" guarantee is enforced by
//! `cargo test`. Used at runtime as the safety net behind the curated
//! accents (and to validate phase-2 named palettes).

/// WCAG AA threshold for normal-size text/icons.
pub const WCAG_AA_NORMAL: f64 = 4.5;
/// WCAG AA threshold for large text and UI component boundaries.
pub const WCAG_AA_LARGE: f64 = 3.0;

/// Parse a `#rrggbb` (or `rrggbb`) hex string into 0..=255 RGB.
/// Returns `None` for any other shape so callers can treat a bad token
/// as "unknown" rather than silently mis-coloring.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Linearize one sRGB channel (0..=255) per the WCAG definition.
fn linearize(channel: u8) -> f64 {
    let c = channel as f64 / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance (0.0..=1.0) of an sRGB color, per WCAG 2.x.
pub fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    let (r, g, b) = rgb;
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

/// Contrast ratio (1.0..=21.0) between two hex colors. Returns `None` if
/// either color fails to parse.
pub fn contrast_ratio(a: &str, b: &str) -> Option<f64> {
    let la = relative_luminance(parse_hex(a)?);
    let lb = relative_luminance(parse_hex(b)?);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    Some((hi + 0.05) / (lo + 0.05))
}

/// True when `fg` on `bg` meets WCAG AA for normal text.
pub fn passes_aa(fg: &str, bg: &str) -> bool {
    contrast_ratio(fg, bg).is_some_and(|r| r >= WCAG_AA_NORMAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_with_and_without_hash() {
        assert_eq!(parse_hex("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex("#14b8a6"), Some((0x14, 0xb8, 0xa6)));
    }

    #[test]
    fn parse_hex_rejects_bad_shapes() {
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#1234567"), None);
    }

    #[test]
    fn black_on_white_is_max_contrast() {
        let r = contrast_ratio("#000000", "#ffffff").unwrap();
        assert!((r - 21.0).abs() < 0.01, "got {r}");
    }

    #[test]
    fn identical_colors_have_ratio_one() {
        let r = contrast_ratio("#14b8a6", "#14b8a6").unwrap();
        assert!((r - 1.0).abs() < 0.001, "got {r}");
    }

    #[test]
    fn ratio_is_symmetric() {
        let a = contrast_ratio("#0f766e", "#ffffff").unwrap();
        let b = contrast_ratio("#ffffff", "#0f766e").unwrap();
        assert!((a - b).abs() < 1e-9);
    }
}

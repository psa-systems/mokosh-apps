//! Invisible-character sanitizing for text input (MAPPS-582).
//!
//! `str::trim` is not enough. It removes characters where `char::is_whitespace`
//! is true, and the characters that break a field while looking like nothing at
//! all are Unicode format characters (general category `Cf`) plus the soft
//! hyphen, none of which are whitespace. `char::is_control` does not help
//! either: it is true only for `Cc`, so it answers `false` for U+200B and
//! U+FEFF. A value carrying one of those survives every check in the app and is
//! then stored, so `Acme\u{200B}` and `Acme` become two records that look
//! identical in every list, search box and picker.

/// Characters that render as nothing, so a value carrying one is
/// indistinguishable from a value without it.
///
/// ZWJ (U+200D) and ZWNJ (U+200C) are deliberately absent: they are meaningful
/// inside Persian, Arabic and Indic text and inside emoji sequences, so
/// removing them from free text corrupts legitimate names. They are removed
/// only by [`clean_strict`], for fields whose grammar (a phone number, a postal
/// code, a UUID) admits no such character anywhere.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00AD}'                  // soft hyphen
        | '\u{200B}'                // zero width space
        | '\u{200E}' | '\u{200F}'   // left-to-right / right-to-left mark
        | '\u{202A}'..='\u{202E}'   // bidi embeddings and overrides
        | '\u{2060}'..='\u{2064}'   // word joiner, invisible operators
        | '\u{2066}'..='\u{2069}'   // bidi isolates
        | '\u{FEFF}'                // BOM / zero width no-break space
    )
}

/// A whitespace character that is not plain ASCII (U+00A0, U+202F, U+2007,
/// U+3000, ...). These are visible as a gap but are not the space every
/// validator's character set was written against.
fn is_exotic_space(c: char) -> bool {
    c.is_whitespace() && !c.is_ascii()
}

/// Whether [`strip_invisible`] would change `raw`.
///
/// The shared field components call this on every keystroke, so the
/// overwhelmingly common case (ordinary text) answers `false` and skips the
/// allocation entirely.
pub fn has_invisible(raw: &str) -> bool {
    raw.chars().any(|c| is_invisible(c) || is_exotic_space(c))
}

/// Applied on every keystroke by the shared field components. Removes
/// invisible characters and maps every non-ASCII whitespace character to a
/// plain space.
///
/// Deliberately does NOT trim or collapse: trimming per keystroke makes it
/// impossible to type the space in "John Smith". ASCII whitespace (including
/// `\n` and `\t`) passes through untouched, so a textarea keeps its line
/// breaks.
pub fn strip_invisible(raw: &str) -> String {
    raw.chars()
        .filter(|c| !is_invisible(*c))
        .map(|c| if is_exotic_space(c) { ' ' } else { c })
        .collect()
}

/// Applied by validators for fields whose grammar admits no invisible
/// character anywhere: phone, postal code, country, email, URL, UUID,
/// timezone, slug, date, money and other numerics. [`strip_invisible`], plus
/// ZWJ / ZWNJ, plus a trim.
pub fn clean_strict(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !is_invisible(*c) && !matches!(c, '\u{200C}' | '\u{200D}'))
        .map(|c| if is_exotic_space(c) { ' ' } else { c })
        .collect();
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character the MAPPS-582 table measured as surviving `.trim()` and
    /// every validator downstream of it.
    const INVISIBLES: &[char] = &[
        '\u{00AD}', '\u{200B}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}',
        '\u{202D}', '\u{202E}', '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}',
        '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{FEFF}',
    ];

    /// Whitespace that is not the ASCII space every character set was written
    /// against. U+00A0 is stripped by `.trim()`; U+202F is too, yet still broke
    /// `validate_phone_field`, which is why they are mapped rather than trusted.
    const EXOTIC_SPACES: &[char] = &['\u{00A0}', '\u{202F}', '\u{2007}', '\u{2009}', '\u{3000}'];

    #[test]
    fn strip_invisible_removes_every_invisible() {
        for c in INVISIBLES {
            let raw = format!("919-397-4144{c}");
            assert!(has_invisible(&raw), "U+{:04X} must be detected", *c as u32);
            assert_eq!(
                strip_invisible(&raw),
                "919-397-4144",
                "U+{:04X} must be removed",
                *c as u32
            );
        }
    }

    #[test]
    fn strip_invisible_maps_exotic_spaces_to_a_plain_space() {
        for c in EXOTIC_SPACES {
            let raw = format!("John{c}Smith");
            assert!(has_invisible(&raw), "U+{:04X} must be detected", *c as u32);
            assert_eq!(
                strip_invisible(&raw),
                "John Smith",
                "U+{:04X} must become a plain space",
                *c as u32
            );
        }
    }

    #[test]
    fn a_clean_value_round_trips_unchanged() {
        for value in [
            "Acme Corp",
            "919-397-4144",
            "user@example.com",
            "Ünïcödé Ltd",
            "日本語の会社",
            "line one\nline two\tend",
            "",
        ] {
            assert!(!has_invisible(value), "{value:?} must need no cleaning");
            assert_eq!(strip_invisible(value), value);
        }
    }

    /// ZWJ / ZWNJ carry meaning in Persian, Arabic and Indic text and in emoji
    /// sequences, so free text keeps them.
    #[test]
    fn interior_zwj_and_zwnj_survive_strip_invisible() {
        for c in ['\u{200C}', '\u{200D}'] {
            let raw = format!("Ac{c}me");
            assert!(!has_invisible(&raw), "U+{:04X} is not stripped", c as u32);
            assert_eq!(strip_invisible(&raw), raw);
        }
        // The canonical emoji ZWJ sequence must come through byte-identical.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(strip_invisible(family), family);
    }

    #[test]
    fn clean_strict_removes_zwj_and_zwnj_and_trims() {
        assert_eq!(clean_strict("Ac\u{200C}me"), "Acme");
        assert_eq!(clean_strict("Ac\u{200D}me"), "Acme");
        assert_eq!(clean_strict("  +1 415 555 1234  "), "+1 415 555 1234");
        assert_eq!(clean_strict("919-397-4144\u{202F}"), "919-397-4144");
        assert_eq!(clean_strict("919-397-4144\u{200B}"), "919-397-4144");
        // U+00A0 becomes a plain space, so a leading/trailing one now trims.
        assert_eq!(clean_strict("\u{00A0}Acme\u{00A0}"), "Acme");
    }

    /// Trimming per keystroke would make the space in "John Smith" untypable.
    #[test]
    fn strip_invisible_does_not_trim() {
        assert_eq!(strip_invisible("John "), "John ");
        assert_eq!(strip_invisible(" John"), " John");
        assert_eq!(strip_invisible("  "), "  ");
    }

    /// The interior space of a name is a plain ASCII space and must be left
    /// exactly where the user typed it.
    #[test]
    fn interior_ascii_space_is_untouched() {
        assert_eq!(strip_invisible("John Smith"), "John Smith");
        assert_eq!(strip_invisible("a  b"), "a  b");
    }
}

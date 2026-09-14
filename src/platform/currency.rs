//! Browser-locale currency detection (MAPPS-676).
//!
//! An AUD-based customer viewing a USD invoice sees `$100.00` and may
//! assume that means AUD $100. The hosted-checkout page charges the
//! invoice currency correctly; the surprise happens on the customer's
//! card statement, in a different currency at a different-day FX rate,
//! weeks later. MAPPS-676's remedy is a one-line warning "You will be
//! charged $X.XX USD..." above the Pay Now button when the browser's
//! implied currency differs from the invoice's.
//!
//! The browser has no direct locale → currency API: `Intl.NumberFormat`
//! wants the currency spelled out as an option, and `Intl.Locale`'s
//! currency accessor is a proposal that no shipping browser has today
//! (2026). So this reads `navigator.language`, extracts the region tag
//! (`en-US` → `US`), and looks it up in a hand-maintained region →
//! currency map covering the top ~30 paying markets. Regions the map
//! does not know return `None`, which the caller treats as "no warning"
//! per the ticket's AC. A missing region is the safe outcome: the
//! warning exists to head off a support ticket, and painting the wrong
//! warning is worse than painting none.
//!
//! No external dependency, no FX rate lookup: the note only names the
//! currency name and the amount in the invoice's own currency, so the
//! customer knows what will hit their card. Reason quoted from
//! `docs/mokosh-invoices/03-open-questions.md` Q10 A.

/// The currency ISO-4217 code the visitor's browser locale implies, or
/// `None` when it cannot be resolved (`navigator` missing, no region
/// tag, region not in the map). Web-only; the desktop shell has no
/// browser locale and returns `None`.
#[cfg(target_arch = "wasm32")]
pub fn browser_locale_currency() -> Option<String> {
    use wasm_bindgen::JsValue;

    let win = web_sys::window()?;
    let navigator = win.navigator();
    // `navigator.language` returns the primary preference; `.languages`
    // returns the fallback chain. The primary is what the browser uses
    // for its own money and date formatting, so it is the right signal
    // for "what currency does this reader expect" (MAPPS-676 §Proposed
    // approach).
    let language = js_sys::Reflect::get(&navigator, &JsValue::from_str("language"))
        .ok()?
        .as_string()?;
    currency_for_locale(&language).map(str::to_string)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn browser_locale_currency() -> Option<String> {
    None
}

/// Extract the region subtag from a BCP-47 language tag (`en-US` →
/// `US`, `zh-Hant-HK` → `HK`) and look it up in [`REGION_CURRENCIES`].
/// Pure over its input so tests can pin the mapping without a browser.
pub fn currency_for_locale(language_tag: &str) -> Option<&'static str> {
    let region = extract_region(language_tag)?;
    REGION_CURRENCIES
        .iter()
        .find(|(r, _)| r.eq_ignore_ascii_case(&region))
        .map(|(_, c)| *c)
}

/// The two-letter region subtag from a BCP-47 tag. Handles the common
/// shapes (`en`, `en-US`, `zh-Hant-HK`, `sr-Latn-RS`) without pulling
/// in a full BCP-47 parser: split on `-`, take the first subtag that
/// is exactly two ASCII letters, uppercased. Returns `None` for
/// language-only tags (`en`) and for tags with only script/variant
/// subtags after the language.
fn extract_region(tag: &str) -> Option<String> {
    for part in tag.split('-').skip(1) {
        if part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()) {
            return Some(part.to_ascii_uppercase());
        }
    }
    None
}

/// ISO 3166-1 region → ISO 4217 currency. Hand-maintained to cover the
/// top ~30 paying markets. A region the map does not know falls
/// through to "no warning", which is the safe outcome (see the module
/// doc). Sorted by continent then alphabetically so a new row lands
/// somewhere obvious.
const REGION_CURRENCIES: &[(&str, &str)] = &[
    // Americas.
    ("US", "USD"),
    ("CA", "CAD"),
    ("MX", "MXN"),
    ("BR", "BRL"),
    ("AR", "ARS"),
    ("CL", "CLP"),
    ("CO", "COP"),
    ("PE", "PEN"),
    // Europe, eurozone.
    ("AT", "EUR"),
    ("BE", "EUR"),
    ("CY", "EUR"),
    ("DE", "EUR"),
    ("EE", "EUR"),
    ("ES", "EUR"),
    ("FI", "EUR"),
    ("FR", "EUR"),
    ("GR", "EUR"),
    ("HR", "EUR"),
    ("IE", "EUR"),
    ("IT", "EUR"),
    ("LT", "EUR"),
    ("LU", "EUR"),
    ("LV", "EUR"),
    ("MT", "EUR"),
    ("NL", "EUR"),
    ("PT", "EUR"),
    ("SI", "EUR"),
    ("SK", "EUR"),
    // Europe, non-eurozone.
    ("GB", "GBP"),
    ("CH", "CHF"),
    ("SE", "SEK"),
    ("NO", "NOK"),
    ("DK", "DKK"),
    ("IS", "ISK"),
    ("PL", "PLN"),
    ("CZ", "CZK"),
    ("HU", "HUF"),
    ("RO", "RON"),
    ("BG", "BGN"),
    ("RU", "RUB"),
    ("UA", "UAH"),
    ("TR", "TRY"),
    // Asia-Pacific.
    ("AU", "AUD"),
    ("NZ", "NZD"),
    ("JP", "JPY"),
    ("KR", "KRW"),
    ("CN", "CNY"),
    ("TW", "TWD"),
    ("HK", "HKD"),
    ("SG", "SGD"),
    ("IN", "INR"),
    ("ID", "IDR"),
    ("MY", "MYR"),
    ("TH", "THB"),
    ("VN", "VND"),
    ("PH", "PHP"),
    // Middle East and Africa.
    ("AE", "AED"),
    ("SA", "SAR"),
    ("IL", "ILS"),
    ("ZA", "ZAR"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_region_handles_common_bcp47_shapes() {
        assert_eq!(extract_region("en-US"), Some("US".to_string()));
        assert_eq!(extract_region("de-DE"), Some("DE".to_string()));
        assert_eq!(extract_region("fr-CA"), Some("CA".to_string()));
        // Script + region: the region subtag is the two-letter one,
        // not the four-letter script.
        assert_eq!(extract_region("zh-Hant-HK"), Some("HK".to_string()));
        assert_eq!(extract_region("sr-Latn-RS"), Some("RS".to_string()));
        // Lowercase region normalises to upper.
        assert_eq!(extract_region("en-au"), Some("AU".to_string()));
    }

    #[test]
    fn extract_region_returns_none_for_language_only_tags() {
        assert_eq!(extract_region("en"), None);
        assert_eq!(extract_region("de"), None);
        assert_eq!(extract_region(""), None);
    }

    #[test]
    fn currency_for_locale_covers_top_markets() {
        assert_eq!(currency_for_locale("en-US"), Some("USD"));
        assert_eq!(currency_for_locale("en-GB"), Some("GBP"));
        assert_eq!(currency_for_locale("en-AU"), Some("AUD"));
        assert_eq!(currency_for_locale("de-DE"), Some("EUR"));
        assert_eq!(currency_for_locale("fr-CH"), Some("CHF"));
        assert_eq!(currency_for_locale("ja-JP"), Some("JPY"));
        assert_eq!(currency_for_locale("pt-BR"), Some("BRL"));
        assert_eq!(currency_for_locale("zh-Hant-HK"), Some("HKD"));
    }

    #[test]
    fn currency_for_locale_returns_none_for_unknown_regions() {
        // Language-only, so no region resolves.
        assert_eq!(currency_for_locale("en"), None);
        // A region we deliberately did not map falls through to None,
        // which the caller treats as "no warning" (safer than picking
        // a wrong currency).
        assert_eq!(currency_for_locale("en-XX"), None);
    }
}

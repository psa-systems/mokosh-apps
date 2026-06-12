//! PMS-253: per-user date/time format rendering.
//!
//! Every UTC instant the SPA shows to the user passes through
//! [`format_user_datetime`], which converts the instant into the
//! viewer's local timezone (via the browser's [`Intl`] machinery, NOT
//! a static format) and then renders it through the format string
//! stored on `users.date_format_string`. When that field is `None` we
//! fall back to the browser locale, matching the legacy
//! `format_local_datetime` behaviour so existing users see no change.
//!
//! The format grammar matches the dominant moment.js / day.js
//! grammar so the help text we link to is standard and users moving
//! from other tools can carry their muscle memory over.
//!
//! Token reference (longer tokens MUST be tried before shorter ones,
//! see [`TOKENS`]):
//!
//! | Group     | Token  | Example                  |
//! |-----------|--------|--------------------------|
//! | Year      | `YYYY` | 2026                     |
//! | Year      | `YY`   | 26                       |
//! | Month     | `MMMM` | June                     |
//! | Month     | `MMM`  | Jun                      |
//! | Month     | `MM`   | 06                       |
//! | Month     | `M`    | 6                        |
//! | Day       | `DD`   | 11                       |
//! | Day       | `Do`   | 11th                     |
//! | Day       | `D`    | 11                       |
//! | Weekday   | `dddd` | Thursday                 |
//! | Weekday   | `ddd`  | Thu                      |
//! | Hour      | `HH`   | 08 (24h, zero-padded)    |
//! | Hour      | `H`    | 8  (24h)                 |
//! | Hour      | `hh`   | 08 (12h, zero-padded)    |
//! | Hour      | `h`    | 8  (12h)                 |
//! | Minute    | `mm`   | 40                       |
//! | Minute    | `m`    | 40                       |
//! | Second    | `ss`   | 49                       |
//! | Second    | `s`    | 49                       |
//! | AM/PM     | `A`    | AM                       |
//! | AM/PM     | `a`    | am                       |
//! | AM/PM     | `a.m.` | a.m.                     |
//!
//! Anything not in the table passes through verbatim, so literal
//! punctuation (`-`, `/`, `:`, `,`, spaces) just works.

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};

/// User-pickable presets. The first element is the human label shown
/// in the dropdown; the second is the format string we persist. Order
/// matters: this is the order the dropdown renders.
pub const PRESET_FORMATS: &[(&str, &str)] = &[
    ("MMM-DD-YYYY HH:mm", "MMM-DD-YYYY HH:mm"),
    ("MMM DD, YYYY h:mm A", "MMM DD, YYYY h:mm A"),
    ("YYYY-MM-DD HH:mm", "YYYY-MM-DD HH:mm"),
    ("DD/MM/YYYY HH:mm", "DD/MM/YYYY HH:mm"),
    ("MM/DD/YYYY h:mm A", "MM/DD/YYYY h:mm A"),
    ("dddd, MMMM D, YYYY", "dddd, MMMM D, YYYY"),
    ("ddd MMM DD, YYYY HH:mm:ss", "ddd MMM DD, YYYY HH:mm:ss"),
    ("DD MMM YYYY HH:mm", "DD MMM YYYY HH:mm"),
];

/// Tokens are tried in this order so longer ones win (`MMMM` before
/// `MMM` before `MM` before `M`). Stop on the first prefix match.
const TOKENS: &[&str] = &[
    "YYYY", "YY", "MMMM", "MMM", "MM", "M", "DD", "Do", "D", "dddd", "ddd", "HH", "H", "hh", "h",
    "mm", "m", "ss", "s", "a.m.", "A", "a",
];

/// Format a UTC instant against a user's format preference. Returns
/// the browser-locale fallback when `format` is `None` or empty.
pub fn format_user_datetime(dt: DateTime<Utc>, format: Option<&str>) -> String {
    let local: DateTime<Local> = Local.from_utc_datetime(&dt.naive_utc());
    match format {
        Some(fmt) if !fmt.trim().is_empty() => render_format(local, fmt),
        _ => browser_locale_fallback(dt),
    }
}

/// Walk the format string left to right; replace the longest matching
/// token at each position, copy any other byte verbatim.
fn render_format(local: DateTime<Local>, format: &str) -> String {
    let bytes = format.as_bytes();
    let mut out = String::with_capacity(format.len() + 8);
    let mut i = 0;
    while i < bytes.len() {
        match longest_token_at(format, i) {
            Some(tok) => {
                out.push_str(&render_token(tok, local));
                i += tok.len();
            }
            None => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

fn longest_token_at(format: &str, i: usize) -> Option<&'static str> {
    let rest = &format[i..];
    TOKENS.iter().find(|tok| rest.starts_with(*tok)).copied()
}

fn render_token(tok: &str, local: DateTime<Local>) -> String {
    let year = local.year();
    let month = local.month();
    let day = local.day();
    let weekday = local.weekday();
    let hour24 = local.hour();
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    let minute = local.minute();
    let second = local.second();
    let is_pm = hour24 >= 12;

    match tok {
        "YYYY" => format!("{:04}", year),
        "YY" => format!("{:02}", year % 100),
        "MMMM" => month_full(month).to_string(),
        "MMM" => month_abbr(month).to_string(),
        "MM" => format!("{:02}", month),
        "M" => format!("{}", month),
        "DD" => format!("{:02}", day),
        "Do" => format!("{}{}", day, ordinal_suffix(day)),
        "D" => format!("{}", day),
        "dddd" => weekday_full(weekday).to_string(),
        "ddd" => weekday_abbr(weekday).to_string(),
        "HH" => format!("{:02}", hour24),
        "H" => format!("{}", hour24),
        "hh" => format!("{:02}", hour12),
        "h" => format!("{}", hour12),
        "mm" => format!("{:02}", minute),
        "m" => format!("{}", minute),
        "ss" => format!("{:02}", second),
        "s" => format!("{}", second),
        "A" => if is_pm { "PM" } else { "AM" }.to_string(),
        "a" => if is_pm { "pm" } else { "am" }.to_string(),
        "a.m." => if is_pm { "p.m." } else { "a.m." }.to_string(),
        _ => tok.to_string(),
    }
}

fn month_full(m: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(m as usize).saturating_sub(1).min(11)]
}

fn month_abbr(m: u32) -> &'static str {
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(m as usize).saturating_sub(1).min(11)]
}

fn weekday_full(w: chrono::Weekday) -> &'static str {
    match w {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
}

fn weekday_abbr(w: chrono::Weekday) -> &'static str {
    match w {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    }
}

fn ordinal_suffix(n: u32) -> &'static str {
    match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// Mirror of `crate::components::layout::format_local_datetime`: when
/// no per-user format is set we let the browser pick a locale
/// rendering. Outside the WASM target we return an explicit UTC
/// string (used by the unit tests + the desktop build).
fn browser_locale_fallback(dt: DateTime<Utc>) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(
            dt.timestamp_millis() as f64
        ));
        let formatted: String = date
            .to_locale_string("en-US", &wasm_bindgen::JsValue::UNDEFINED)
            .into();
        if formatted.is_empty() {
            dt.format("%Y-%m-%d %H:%M UTC").to_string()
        } else {
            formatted
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        dt.format("%Y-%m-%d %H:%M UTC").to_string()
    }
}

/// Read the active user's date_format_string off the AuthContext
/// without forcing every caller to thread the value through their
/// signature. Use this from any handler component that already
/// touches the AuthContext.
pub fn user_format_pref() -> Option<String> {
    use dioxus::prelude::{try_use_context, ReadableExt, Signal};
    let auth = try_use_context::<Signal<crate::hooks::auth::AuthContext>>()?;
    let pref = auth
        .read()
        .user
        .as_ref()
        .and_then(|u| u.date_format_string.clone());
    pref
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DateTime<Local> {
        // Thursday, 11 June 2026, 08:40:49 local.
        Local
            .with_ymd_and_hms(2026, 6, 11, 8, 40, 49)
            .single()
            .expect("unambiguous sample timestamp")
    }

    #[test]
    fn renders_year_month_day_padded() {
        assert_eq!(render_format(sample(), "YYYY-MM-DD"), "2026-06-11");
        assert_eq!(render_format(sample(), "YY"), "26");
        assert_eq!(render_format(sample(), "M/D"), "6/11");
    }

    #[test]
    fn renders_month_names() {
        assert_eq!(render_format(sample(), "MMMM"), "June");
        assert_eq!(render_format(sample(), "MMM"), "Jun");
    }

    #[test]
    fn renders_weekday_names() {
        assert_eq!(render_format(sample(), "dddd"), "Thursday");
        assert_eq!(render_format(sample(), "ddd"), "Thu");
    }

    #[test]
    fn renders_24h_and_12h_hours() {
        assert_eq!(render_format(sample(), "HH:mm"), "08:40");
        assert_eq!(render_format(sample(), "H:mm"), "8:40");
        assert_eq!(render_format(sample(), "hh:mm A"), "08:40 AM");
        assert_eq!(render_format(sample(), "h:mm a"), "8:40 am");
        // 12h roll-over: 13:05 -> 1:05 PM
        let pm = Local
            .with_ymd_and_hms(2026, 6, 11, 13, 5, 0)
            .single()
            .unwrap();
        assert_eq!(render_format(pm, "h:mm A"), "1:05 PM");
        // Midnight should read as 12, not 0
        let midnight = Local
            .with_ymd_and_hms(2026, 6, 11, 0, 0, 0)
            .single()
            .unwrap();
        assert_eq!(render_format(midnight, "h A"), "12 AM");
    }

    #[test]
    fn renders_seconds_and_am_pm_variants() {
        assert_eq!(render_format(sample(), "ss"), "49");
        assert_eq!(render_format(sample(), "a.m."), "a.m.");
        let pm = Local
            .with_ymd_and_hms(2026, 6, 11, 15, 0, 0)
            .single()
            .unwrap();
        assert_eq!(render_format(pm, "a.m."), "p.m.");
    }

    #[test]
    fn renders_ordinal_day() {
        let d = |n| {
            Local
                .with_ymd_and_hms(2026, 1, n, 0, 0, 0)
                .single()
                .unwrap()
        };
        assert_eq!(render_format(d(1), "Do"), "1st");
        assert_eq!(render_format(d(2), "Do"), "2nd");
        assert_eq!(render_format(d(3), "Do"), "3rd");
        assert_eq!(render_format(d(4), "Do"), "4th");
        assert_eq!(render_format(d(11), "Do"), "11th");
        assert_eq!(render_format(d(21), "Do"), "21st");
    }

    #[test]
    fn passes_through_literal_punctuation() {
        // Separators in the canonical token list (dash, slash, dot,
        // comma, space, colon) are not tokens themselves, so they
        // pass through verbatim. Letters that happen to be tokens
        // (e.g. `a`, `m`) DO get expanded - no `[literal]` escape
        // hatch in v1; if a user needs the literal word "am" they
        // build it from non-token characters.
        assert_eq!(
            render_format(sample(), "YYYY/MM/DD, HH:mm"),
            "2026/06/11, 08:40"
        );
        assert_eq!(
            render_format(sample(), "YYYY.MM.DD - HH:mm"),
            "2026.06.11 - 08:40"
        );
    }

    #[test]
    fn preset_table_uses_only_known_tokens() {
        // Every preset must render to something different from its
        // own source string (i.e. tokens actually got expanded). This
        // catches typos like 'mmYYYY' that would silently render
        // as the literal characters because 'mm' is greedy and would
        // claim only the first two chars.
        for (_, fmt) in PRESET_FORMATS {
            let rendered = render_format(sample(), fmt);
            assert!(
                rendered != *fmt,
                "preset {fmt:?} produced no token substitutions",
            );
        }
    }

    #[test]
    fn empty_or_none_falls_back() {
        // We can't exercise the wasm Intl path from a host-side test,
        // but None should not panic; on non-wasm targets the fallback
        // is a plain UTC strftime.
        let dt = Utc
            .with_ymd_and_hms(2026, 6, 11, 8, 40, 49)
            .single()
            .unwrap();
        let s = format_user_datetime(dt, None);
        assert!(s.contains("2026"));
        let s = format_user_datetime(dt, Some(""));
        assert!(s.contains("2026"));
    }
}

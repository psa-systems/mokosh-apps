//! URL helpers shared across the client.

/// Tiny percent-encoder for the few characters that actually break URL
/// query strings: space, `&`, `#`, `?`, `+`, `=`, and control bytes.
/// Avoids pulling in the full `urlencoding` crate for the handful of
/// places that build paths inline. The server ILIKE / exact-matches the
/// result, so non-ASCII passes straight through.
pub fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '+' => out.push_str("%2B"),
            '=' => out.push_str("%3D"),
            c if (c as u32) < 0x20 => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Read a query-string parameter from the route the app is currently on.
///
/// MAPPS-249: the company context cards' "View All" links carry a
/// `?company_id=<uuid>` so the destination list page can scope itself to that
/// company. The list pages read it back through this helper. Returns `None`
/// when the current location cannot be resolved (logged by
/// `location::current_query`), when the key is absent, or when its value is
/// empty. Values are
/// matched on the raw (percent-encoded) text; callers that need a decoded
/// value should decode it themselves, but `company_id` is a bare UUID so no
/// decoding is required.
///
/// MAPPS-683: sourced from the router rather than the browser URL, so the
/// desktop build reads the same scoping the link carried.
pub fn current_query_param(key: &str) -> Option<String> {
    query_param_in(&crate::platform::location::current_query()?, key)
}

/// Pure core of [`current_query_param`]: the same lookup against a query
/// string the caller already holds. Split out so the rule is testable
/// without a host that has a location at all.
pub fn query_param_in(search: &str, key: &str) -> Option<String> {
    let search = search.strip_prefix('?').unwrap_or(search);
    for pair in search.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            let value = parts.next().unwrap_or("").trim();
            if value.is_empty() {
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
}

/// Extract the explicit URL scheme from `value`, lower-cased, if present.
///
/// Returns `None` for a scheme-less / relative reference. The scheme is the
/// text before the first `:` and never contains `/`: a `:` that appears after
/// a `/` belongs to a path or port, not a scheme.
///
/// ASCII whitespace and control bytes are stripped before the scheme is
/// resolved, because browsers strip those before resolving the scheme
/// (e.g. `java\tscript:alert(1)` runs as `javascript:`). Sharing this helper
/// means the client-side field validators reject the same whitespace tricks
/// that [`safe_href`] guards against at render time (MAPPS-213).
pub fn scheme_of(value: &str) -> Option<String> {
    let collapsed: String = value
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && (*c as u32) >= 0x20)
        .collect();
    let colon = collapsed.find(':')?;
    let scheme = &collapsed[..colon];
    if scheme.is_empty() || scheme.contains('/') {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

/// Return a value only when it is safe to place in an `href` attribute.
///
/// Browsers execute `javascript:`, `data:`, and `vbscript:` URLs, so a
/// user-supplied value rendered straight into `href` is a stored-XSS
/// vector (MAPPS-149). This allowlists schemes: `http`, `https`, and
/// `mailto` are returned as-is; any other explicit scheme yields `None`
/// so the caller can fall back to rendering plain text instead of a live
/// link. Scheme-less / relative references carry no executable scheme and
/// are returned unchanged.
pub fn safe_href(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    match scheme_of(trimmed) {
        // Explicit scheme: only the allowlisted ones may become a live link.
        Some(scheme) => match scheme.as_str() {
            "http" | "https" | "mailto" => Some(trimmed.to_string()),
            _ => None,
        },
        // No scheme: relative reference, safe to link as-is.
        None => Some(trimmed.to_string()),
    }
}

/// `encodeURIComponent`, in Rust (MAPPS-504).
///
/// Percent-encodes every byte outside JavaScript's unreserved set
/// (`A-Z a-z 0-9 - _ . ! ~ * ' ( )`), UTF-8 first. Replaces the
/// `js_sys::encode_uri_component` calls that only existed because the
/// browser had the function built in; there is nothing browser-specific
/// about the rule, and having it here makes it testable on the host.
pub fn encode_uri_component(s: &str) -> String {
    const UNRESERVED: &[u8] = b"-_.!~*'()";
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || UNRESERVED.contains(byte) {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A parsed query string, standing in for `web_sys::UrlSearchParams`.
///
/// Same decoding rules: `+` is a space, `%XX` is a byte, and a key with
/// no `=` has an empty value. Invalid UTF-8 in the decoded bytes is
/// replaced rather than dropping the parameter, matching what the
/// browser does with a malformed query.
pub struct QueryString {
    pairs: Vec<(String, String)>,
}

impl QueryString {
    pub fn parse(search: &str) -> Self {
        let trimmed = search.strip_prefix('?').unwrap_or(search);
        let pairs = trimmed
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| match part.split_once('=') {
                Some((k, v)) => (decode_component(k), decode_component(v)),
                None => (decode_component(part), String::new()),
            })
            .collect();
        Self { pairs }
    }

    /// First value for `key`, matching `UrlSearchParams::get`.
    pub fn get(&self, key: &str) -> Option<String> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
}

fn decode_component(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    // Not a valid escape: pass the `%` through as a
                    // literal, which is what browsers do.
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        decode_component, encode_uri_component, query_param_in, safe_href, scheme_of, QueryString,
    };

    #[test]
    fn query_param_in_reads_the_query_a_link_carried() {
        // The string `location::current_query()` hands back on either host.
        let search = "?company_id=11111111-1111-1111-1111-111111111111&page=2";
        assert_eq!(
            query_param_in(search, "company_id"),
            Some("11111111-1111-1111-1111-111111111111".to_string())
        );
        assert_eq!(query_param_in(search, "page"), Some("2".to_string()));
        assert_eq!(query_param_in(search, "absent"), None);
    }

    #[test]
    fn query_param_in_treats_an_empty_value_as_absent() {
        assert_eq!(query_param_in("?company_id=", "company_id"), None);
        assert_eq!(query_param_in("?company_id", "company_id"), None);
        assert_eq!(query_param_in("", "company_id"), None);
    }

    #[test]
    fn query_param_in_tolerates_a_missing_leading_question_mark() {
        assert_eq!(query_param_in("tenant=acme", "tenant"), Some("acme".into()));
    }

    #[test]
    fn encode_uri_component_matches_the_javascript_unreserved_set() {
        assert_eq!(encode_uri_component("abcXYZ019"), "abcXYZ019");
        assert_eq!(encode_uri_component("-_.!~*'()"), "-_.!~*'()");
        assert_eq!(encode_uri_component("a b"), "a%20b");
        assert_eq!(encode_uri_component("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_uri_component("/path?x#y"), "%2Fpath%3Fx%23y");
    }

    #[test]
    fn encode_uri_component_percent_encodes_utf8_bytes() {
        // encodeURIComponent("é") === "%C3%A9"
        assert_eq!(encode_uri_component("\u{e9}"), "%C3%A9");
    }

    #[test]
    fn query_string_reads_values_the_way_urlsearchparams_does() {
        let q = QueryString::parse("?code=abc&state=x%20y&empty=&plus=a+b");
        assert_eq!(q.get("code"), Some("abc".to_string()));
        assert_eq!(q.get("state"), Some("x y".to_string()));
        assert_eq!(q.get("empty"), Some(String::new()));
        assert_eq!(q.get("plus"), Some("a b".to_string()));
        assert_eq!(q.get("absent"), None);
    }

    #[test]
    fn query_string_tolerates_a_missing_leading_question_mark_and_bare_keys() {
        let q = QueryString::parse("flag&code=1");
        assert_eq!(q.get("flag"), Some(String::new()));
        assert_eq!(q.get("code"), Some("1".to_string()));
    }

    #[test]
    fn query_string_returns_the_first_value_for_a_repeated_key() {
        let q = QueryString::parse("code=first&code=second");
        assert_eq!(q.get("code"), Some("first".to_string()));
    }

    #[test]
    fn decode_component_passes_through_a_malformed_escape() {
        assert_eq!(decode_component("100%"), "100%");
        assert_eq!(decode_component("%zz"), "%zz");
        assert_eq!(decode_component("%C3%A9"), "\u{e9}");
    }

    #[test]
    fn scheme_of_extracts_explicit_scheme_lowercased() {
        assert_eq!(scheme_of("https://example.com"), Some("https".to_string()));
        assert_eq!(scheme_of("HTTP://example.com"), Some("http".to_string()));
        assert_eq!(
            scheme_of("mailto:a@example.com"),
            Some("mailto".to_string())
        );
        assert_eq!(
            scheme_of("javascript:alert(1)"),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn scheme_of_collapses_whitespace_tricks() {
        assert_eq!(
            scheme_of("java\tscript:alert(1)"),
            Some("javascript".to_string())
        );
        assert_eq!(
            scheme_of("  JavaScript:alert(1)  "),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn scheme_of_is_none_for_scheme_less() {
        assert_eq!(scheme_of("example.com"), None);
        assert_eq!(scheme_of("/internal/path"), None);
        // A ':' after a '/' is a path/port, not a scheme.
        assert_eq!(scheme_of("example.com/a:b"), None);
        assert_eq!(scheme_of(""), None);
    }

    #[test]
    fn allows_http_and_https_and_mailto() {
        assert_eq!(
            safe_href("https://example.com"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            safe_href("http://example.com/path?q=1"),
            Some("http://example.com/path?q=1".to_string())
        );
        assert_eq!(
            safe_href("mailto:a@example.com"),
            Some("mailto:a@example.com".to_string())
        );
    }

    #[test]
    fn rejects_dangerous_schemes() {
        assert_eq!(safe_href("javascript:alert(1)"), None);
        assert_eq!(safe_href("data:text/html,<script>alert(1)</script>"), None);
        assert_eq!(safe_href("vbscript:msgbox(1)"), None);
    }

    #[test]
    fn rejects_dangerous_schemes_with_case_and_whitespace_tricks() {
        assert_eq!(safe_href("JavaScript:alert(1)"), None);
        assert_eq!(safe_href("  javascript:alert(1)  "), None);
        assert_eq!(safe_href("java\tscript:alert(1)"), None);
        assert_eq!(safe_href("java\nscript:alert(1)"), None);
    }

    #[test]
    fn empty_or_blank_is_none() {
        assert_eq!(safe_href(""), None);
        assert_eq!(safe_href("   "), None);
    }

    #[test]
    fn scheme_less_relative_is_allowed() {
        assert_eq!(safe_href("example.com"), Some("example.com".to_string()));
        assert_eq!(
            safe_href("/internal/path"),
            Some("/internal/path".to_string())
        );
    }
}

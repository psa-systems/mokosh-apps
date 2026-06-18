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

/// Read a query-string parameter from the current browser URL.
///
/// MAPPS-249: the company context cards' "View All" links carry a
/// `?company_id=<uuid>` so the destination list page can scope itself to that
/// company. The list pages read it back through this helper. Returns `None`
/// off-web, when the key is absent, or when its value is empty. Values are
/// matched on the raw (percent-encoded) text; callers that need a decoded
/// value should decode it themselves, but `company_id` is a bare UUID so no
/// decoding is required.
pub fn current_query_param(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        let search = search.strip_prefix('?').unwrap_or(&search);
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
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        None
    }
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

#[cfg(test)]
mod tests {
    use super::{safe_href, scheme_of};

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

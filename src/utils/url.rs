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

/// Return a value only when it is safe to place in an `href` attribute.
///
/// Browsers execute `javascript:`, `data:`, and `vbscript:` URLs, so a
/// user-supplied value rendered straight into `href` is a stored-XSS
/// vector (MAPPS-149). This allowlists schemes: `http`, `https`, and
/// `mailto` are returned as-is; any other explicit scheme yields `None`
/// so the caller can fall back to rendering plain text instead of a live
/// link. Scheme-less / relative references carry no executable scheme and
/// are returned unchanged.
///
/// The scheme check ignores ASCII whitespace and control bytes embedded in
/// the value, because browsers strip those before resolving the scheme
/// (e.g. `java\tscript:alert(1)` runs as `javascript:`).
pub fn safe_href(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let collapsed: String = trimmed
        .chars()
        .filter(|c| !c.is_whitespace() && (*c as u32) >= 0x20)
        .collect();
    // The scheme is the text before the first ':' and never contains '/'.
    // A ':' that appears after a '/' belongs to a path/port, not a scheme.
    if let Some(colon) = collapsed.find(':') {
        let scheme = &collapsed[..colon];
        if !scheme.is_empty() && !scheme.contains('/') {
            return match scheme.to_ascii_lowercase().as_str() {
                "http" | "https" | "mailto" => Some(trimmed.to_string()),
                _ => None,
            };
        }
    }
    // No scheme: relative reference, safe to link as-is.
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::safe_href;

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

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

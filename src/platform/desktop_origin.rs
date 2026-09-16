//! The origin `dioxus-desktop` 0.7.7 gives the app's webview document, per OS
//! (MAPPS-817).
//!
//! `mokosh-server`'s CORS allowlist is exact-match and credentialed, so
//! `CORS_ORIGIN` needs this origin verbatim. `dioxus-desktop` fixes it per OS
//! at compile time in a private, unexported `BASE_URI` constant
//! (`dioxus-desktop-0.7.7/src/protocol.rs:15-22`; the version is pinned in
//! `Cargo.lock`), so it cannot be read from this crate at compile or run
//! time. The constants below are a manual mirror of that constant, cited to
//! the exact source line, and `docs/desktop.md` quotes the same three
//! literals. The test at the bottom of this module is the guard against the
//! two drifting apart; it does not catch `dioxus-desktop` itself changing
//! `BASE_URI` on an upgrade, since that value is not reachable from here.

/// Android: `BASE_URI = "https://dioxus.index.html/"` (an https scheme is
/// needed there for secure-context web APIs; see
/// `WebViewBuilderExtAndroid::with_https_scheme` in the same crate).
pub const ANDROID_ORIGIN: &str = "https://dioxus.index.html";

/// Windows: `BASE_URI = "http://dioxus.index.html/"`.
pub const WINDOWS_ORIGIN: &str = "http://dioxus.index.html";

/// macOS, Linux, iOS: `BASE_URI = "dioxus://index.html/"`, the custom
/// scheme wry registers a request handler for.
pub const OTHER_ORIGIN: &str = "dioxus://index.html";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_desktop_md_names_every_platform_origin() {
        let docs = include_str!("../../docs/desktop.md");
        for (platform, origin) in [
            ("Windows", WINDOWS_ORIGIN),
            ("Android", ANDROID_ORIGIN),
            ("macOS, Linux, iOS", OTHER_ORIGIN),
        ] {
            assert!(
                docs.contains(origin),
                "docs/desktop.md does not name the {platform} origin {origin:?}; \
                 it must match dioxus-desktop 0.7.7's BASE_URI \
                 (dioxus-desktop-0.7.7/src/protocol.rs:15-22) for CORS_ORIGIN to work",
            );
        }
    }
}

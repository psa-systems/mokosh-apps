//! Where the app thinks it is, and how it leaves (MAPPS-504).
//!
//! A desktop window has no URL bar, so the readers that ask about one
//! (`search`, `origin`, `host`) answer `None` on the native build and each
//! caller falls back to what it already falls back to when the browser
//! refuses (configured values, the hub URL, or "not on that route"). That
//! is a real difference, not a stub pretending to be a browser: the
//! callers were already written for a `None`, because `web_sys::window()`
//! is itself an `Option`.
//!
//! [`current_query`] (MAPPS-683) and `pathname` (MAPPS-697) are the
//! exceptions. A reader that wants the path or query the app was
//! NAVIGATED to is not asking about the URL bar at all: a `Link` pushes its
//! target verbatim into the router's history on both hosts, so the router
//! answers them on the desktop too.

/// Path component of the current URL, e.g. `/onboarding/profile`.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.1
pub fn pathname() -> Option<String> {
    web_sys::window()?.location().pathname().ok()
}

/// Query string of the current URL, leading `?` included.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.1
pub fn search() -> Option<String> {
    web_sys::window()?.location().search().ok()
}

/// Query string the app is currently on, leading `?` included, or
/// `None` when the route carries none (MAPPS-683).
///
/// Distinct from [`search`], which is the browser's URL bar and so is
/// browser-only. This is the query of the *route*, which both hosts
/// have: the router's history holds whatever the `Link` pushed, query
/// included. Use this for anything read back off an internal
/// navigation target (`?ticket_id=`, `?company_id=`, ...); `search`
/// stays for the browser-only cases, such as an OAuth response that
/// arrived in the address bar.
///
/// Never silently `None`: an unreachable location is logged, because a
/// prefill that quietly does not arrive looks exactly like a link that
/// forgot to carry it.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.4, SPEC-3.37.5
pub fn current_query() -> Option<String> {
    let Some(win) = web_sys::window() else {
        tracing::warn!("no window to read the current query string from");
        return None;
    };
    match win.location().search() {
        Ok(search) => query_of(&search),
        Err(_) => {
            tracing::warn!("the browser refused to report the current query string");
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.4, SPEC-3.37.5
pub fn current_query() -> Option<String> {
    router_route_string("current query string")
        .as_deref()
        .and_then(query_of)
}

/// The route string the app is currently on, or `None` when it cannot be
/// resolved (no runtime, no router). Shared by [`current_query`] and the
/// native [`pathname`] so the two cannot drift; MAPPS-697.
///
/// Never silently `None`: warns naming what the caller was after, because
/// a prefill or a guard bypass that quietly does not resolve looks the
/// same as an unset one.
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.3, SPEC-3.37.5: warns instead of answering None silently.
fn router_route_string(what: &str) -> Option<String> {
    // `try_router` consumes a context, and consuming one outside a
    // Dioxus runtime panics rather than answering `None`. The host test
    // build has no runtime, so check for one first.
    if dioxus::core::Runtime::try_current().is_none() {
        tracing::warn!("no Dioxus runtime to read the {what} from");
        return None;
    }
    let Some(router) = dioxus::prelude::try_router() else {
        tracing::warn!("no router to read the {what} from");
        return None;
    };
    Some(router.full_route_string())
}

/// The query of an internal route string, leading `?` included.
///
/// `None` for a route with no query or an empty one, so a caller cannot
/// tell "no query" from "a query of nothing" and act on the difference.
// SPEC-3.37.4
fn query_of(route: &str) -> Option<String> {
    // A fragment ends the query, and the router's route strings can
    // carry one (`Route::from_str` splits `#` off before `?`).
    let route = route.split('#').next().unwrap_or(route);
    let (_, query) = route.split_once('?')?;
    (!query.is_empty()).then(|| format!("?{query}"))
}

/// The path component of an internal route string, without any query or
/// fragment (MAPPS-697). The router's `full_route_string` mirrors what
/// `Link` pushed - a path, an optional `?query` and an optional `#frag`
/// in that order - so cutting the query and the fragment off leaves the
/// path a browser's `location.pathname` would report for the same
/// navigation.
///
/// `""` is an empty path a caller can compare against; the shared
/// `router_route_string` decides when to answer `None` instead, and does
/// so with a `warn`.
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.3
fn path_of(route: &str) -> String {
    let route = route.split('#').next().unwrap_or(route);
    route
        .split_once('?')
        .map(|(p, _)| p)
        .unwrap_or(route)
        .to_string()
}

/// Rewrite the address bar to `url` without adding a history entry, so a
/// credential that arrived in the URL does not linger in browser history
/// (MAPPS-336, MAPPS-919). Browser-only: a desktop window has no URL bar.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.6: replaces the entry so a credential in the URL does not linger in history.
pub fn replace_state(url: &str) {
    // MAPPS-1236: a failure here leaves a credential in the address bar and
    // in history, so it is logged. Only the path: the query is what is
    // being scrubbed.
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else {
        tracing::error!("no history to rewrite the address bar to {path}");
        return;
    };
    if let Err(e) = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(url)) {
        tracing::error!("could not rewrite the address bar to {path}: {e:?}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.6
pub fn replace_state(_url: &str) {}

/// Drop the query string from the address bar, keeping the path. For the
/// routes whose token arrives as `?token=`.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.6
pub fn strip_url_query() {
    if let Some(path) = pathname() {
        replace_state(&path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.6
pub fn strip_url_query() {}

/// Scheme + host + port, e.g. `https://msp.example.com`.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.1
pub fn origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

/// Host + port, e.g. `msp.example.com`.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.1
pub fn host() -> Option<String> {
    web_sys::window()?.location().host().ok()
}

/// Replace the current document with `url`, leaving no history entry.
/// Used for the hand-off to the OP's logout endpoint.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.7
pub fn replace(url: &str) {
    let Some(win) = web_sys::window() else {
        tracing::error!("no window to navigate to {url}");
        return;
    };
    if win.location().replace(url).is_err() {
        // The user clicked something that was supposed to take them
        // elsewhere and nothing happened. Say so somewhere.
        tracing::error!("navigation to {url} was refused");
    }
}

/// Navigate this app to `url`, keeping a history entry.
///
/// Distinct from [`replace`]: this is how the OIDC flow hands the user
/// to the OP and expects them BACK on a callback route, so on a host
/// that cannot be navigated it has to fail rather than silently open
/// something that can never return.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.9: fails on a host that cannot be navigated and returned to.
pub fn set_href(url: &str) -> Result<(), String> {
    web_sys::window()
        .ok_or_else(|| "no window".to_string())?
        .location()
        .set_href(url)
        .map_err(|_| format!("navigation to {url} was refused"))
}

/// Reload the current document. `false` when the host has no notion of
/// reloading, so the caller can offer something else instead of leaving
/// a dead control on screen.
#[cfg(target_arch = "wasm32")]
// SPEC-3.37.11
pub fn reload() -> bool {
    let Some(win) = web_sys::window() else {
        return false;
    };
    if win.location().reload().is_err() {
        tracing::error!("reload was refused");
        return false;
    }
    true
}

/// Path the app is currently on, e.g. `/onboarding/profile`, on the
/// desktop (MAPPS-697). Sourced from the router the same way
/// [`current_query`] is: a desktop window has no URL bar, but every
/// `Link` pushes its target verbatim into the router's history, so the
/// router holds the current path on both hosts.
///
/// Distinct from the wasm build's `pathname`, which reads the browser's
/// URL bar directly and so answers even outside a runtime.
///
/// Never silently `None`: warns when the location cannot be resolved,
/// because a guard bypass (`on_onboarding_route`) that quietly does not
/// resolve is the exact defect this closes.
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.3
pub fn pathname() -> Option<String> {
    router_route_string("current pathname")
        .as_deref()
        .map(path_of)
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.2
pub fn search() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.2
pub fn origin() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.2
pub fn host() -> Option<String> {
    None
}

/// Hand `url` to the OS so it opens in the user's real browser.
///
/// The desktop window is the application, not a document to be replaced,
/// and the destinations these calls carry (the OP's logout endpoint, the
/// Bunyip hub) are web pages that belong in a browser anyway.
///
/// Failure is logged rather than returned: this hands the URL to the OS
/// and returns without navigating the window, so the app is left in
/// whatever in-memory state its caller put it in beforehand. Unlike a
/// browser's `location.replace`, this never resets local session state
/// itself (MAPPS-815); a caller that needs that has to clear it before
/// calling this.
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.8
pub fn replace(url: &str) {
    if let Err(e) = open::that_detached(url) {
        tracing::error!("could not open {url} in a browser: {e}");
    }
}

/// A desktop window cannot be navigated to a URL and brought back, so
/// this always fails. The one flow that needed it, the OIDC authorize
/// redirect, is an RFC 8252 loopback exchange here instead (MAPPS-505,
/// `crate::platform::loopback`).
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.9
pub fn set_href(_url: &str) -> Result<(), String> {
    Err("this build cannot follow a browser redirect".to_string())
}

/// Open `url` in the user's real browser, reporting whether it could
/// (MAPPS-505).
///
/// Distinct from [`replace`], which logs its failure because every caller
/// has already done what the user asked by the time it runs. This one
/// starts the RFC 8252 sign-in: a browser that never opened leaves the
/// user waiting for a window that is not coming, so the caller has to
/// hear about it.
///
/// Native only. The browser build hands the OP the document itself, via
/// [`set_href`].
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.10: the error is returned because the sign-in flow would otherwise wait forever.
pub fn open_external(url: &str) -> Result<(), String> {
    open::that_detached(url).map_err(|e| format!("could not open {url} in a browser: {e}"))
}

/// There is no document to reload; the caller substitutes its own
/// recovery (see the error screen in `lib.rs`, which navigates home).
#[cfg(not(target_arch = "wasm32"))]
// SPEC-3.37.11
pub fn reload() -> bool {
    false
}

/// MAPPS-1236: the module doc and the browser `replace_state`, pinned in
/// source (the latter needs a browser to drive).
#[cfg(test)]
mod mapps_1236_tests {
    const SRC: &str = include_str!("location.rs");

    fn code_only() -> String {
        let end = SRC
            .find("mod mapps_1236_tests")
            .expect("this module is in this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn the_module_doc_names_both_router_backed_readers() {
        let doc: String = SRC
            .lines()
            .take_while(|l| l.starts_with("//!"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!doc.contains("every reader here answers `None`"), "{doc}");
        assert!(doc.contains("`pathname` (MAPPS-697)"), "{doc}");
        assert!(doc.contains("[`current_query`] (MAPPS-683)"), "{doc}");
    }

    #[test]
    fn a_failed_address_bar_rewrite_is_logged_without_its_query() {
        let code = code_only();
        let at = code
            .find("pub fn replace_state(url: &str) {")
            .expect("the browser replace_state");
        let body = &code[at..at + 700.min(code.len() - at)];
        assert!(
            !body.contains("let _ = history.replace_state_with_url"),
            "{body}"
        );
        assert!(
            body.contains("could not rewrite the address bar to {path}"),
            "{body}"
        );
        assert!(body.contains("split(['?', '#'])"), "{body}");
    }
}

#[cfg(test)]
mod tests {
    use super::query_of;

    #[test]
    fn query_of_reads_the_query_a_link_pushed() {
        // What `Link { to: "/time/new?ticket_id=..." }` leaves in the
        // router's history, verbatim, on the desktop.
        assert_eq!(
            query_of("/time/new?ticket_id=11111111-1111-1111-1111-111111111111"),
            Some("?ticket_id=11111111-1111-1111-1111-111111111111".to_string())
        );
        assert_eq!(
            query_of("/tickets/new?company_id=abc&company_name=Acme%20%26%20Co"),
            Some("?company_id=abc&company_name=Acme%20%26%20Co".to_string())
        );
    }

    #[test]
    fn query_of_is_none_without_a_query() {
        assert_eq!(query_of("/time/new"), None);
        assert_eq!(query_of("/"), None);
        assert_eq!(query_of(""), None);
        // A `?` with nothing after it is no query, not an empty one.
        assert_eq!(query_of("/time/new?"), None);
    }

    #[test]
    fn query_of_stops_at_a_fragment() {
        assert_eq!(query_of("/home#features"), None);
        assert_eq!(query_of("/x?a=1#f"), Some("?a=1".to_string()));
    }

    #[test]
    fn query_of_accepts_a_bare_browser_search_string() {
        // `window.location.search` is the whole value, `?` and all.
        assert_eq!(query_of("?a=1"), Some("?a=1".to_string()));
        assert_eq!(query_of(""), None);
    }

    /// The host test build has no Dioxus runtime, so this exercises the
    /// unreachable-location branch: `None`, logged, and no panic out of
    /// `try_router`'s `consume_context`.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn current_query_is_none_without_a_runtime() {
        assert_eq!(super::current_query(), None);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn pathname_is_none_without_a_runtime() {
        // same shape as current_query, and shares the same
        // warn-rather-than-silent contract through router_route_string.
        assert_eq!(super::pathname(), None);
    }

    // the four shapes the router hands `full_route_string`.
    // A path with a query, a path with a fragment, a path with both, and
    // a path with neither. Named separately so a failure names which
    // shape drifted.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn path_of_drops_a_query() {
        assert_eq!(
            super::path_of("/onboarding/profile?next=/home"),
            "/onboarding/profile"
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn path_of_drops_a_fragment() {
        assert_eq!(
            super::path_of("/onboarding/profile#step-2"),
            "/onboarding/profile"
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn path_of_drops_a_query_and_a_fragment() {
        // A fragment ends the query on the wire, so cutting `#` first
        // and then `?` matches how a browser would parse the string.
        assert_eq!(
            super::path_of("/onboarding/profile?next=/home#top"),
            "/onboarding/profile"
        );
        // The reverse order in the source is unusual but stripping `#`
        // first still recovers the path.
        assert_eq!(
            super::path_of("/onboarding/profile#top?next=/home"),
            "/onboarding/profile"
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn path_of_keeps_a_bare_path() {
        assert_eq!(super::path_of("/onboarding/profile"), "/onboarding/profile");
        assert_eq!(super::path_of("/"), "/");
        assert_eq!(super::path_of(""), "");
    }
}

//! Where the app thinks it is, and how it leaves (MAPPS-504).
//!
//! A desktop window has no URL bar, so every reader here answers `None`
//! on the native build and each caller falls back to what it already
//! falls back to when the browser refuses (configured values, the hub
//! URL, or "not on that route"). That is a real difference, not a
//! stub pretending to be a browser: the callers were already written
//! for a `None`, because `web_sys::window()` is itself an `Option`.
//!
//! [`current_query`] is the exception, and MAPPS-683 is why. A reader
//! that wants the query the app was NAVIGATED with is not asking about
//! the URL bar at all: a `Link` pushes its target verbatim into the
//! router's history on both hosts, so the router can answer it on the
//! desktop too.

/// Path component of the current URL, e.g. `/onboarding/profile`.
#[cfg(target_arch = "wasm32")]
pub fn pathname() -> Option<String> {
    web_sys::window()?.location().pathname().ok()
}

/// Query string of the current URL, leading `?` included.
#[cfg(target_arch = "wasm32")]
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
pub fn current_query() -> Option<String> {
    // `try_router` consumes a context, and consuming one outside a
    // Dioxus runtime panics rather than answering `None`. The host test
    // build has no runtime, so check for one first.
    if dioxus::core::Runtime::try_current().is_none() {
        tracing::warn!("no Dioxus runtime to read the current query string from");
        return None;
    }
    let Some(router) = dioxus::prelude::try_router() else {
        tracing::warn!("no router to read the current query string from");
        return None;
    };
    query_of(&router.full_route_string())
}

/// The query of an internal route string, leading `?` included.
///
/// `None` for a route with no query or an empty one, so a caller cannot
/// tell "no query" from "a query of nothing" and act on the difference.
fn query_of(route: &str) -> Option<String> {
    // A fragment ends the query, and the router's route strings can
    // carry one (`Route::from_str` splits `#` off before `?`).
    let route = route.split('#').next().unwrap_or(route);
    let (_, query) = route.split_once('?')?;
    (!query.is_empty()).then(|| format!("?{query}"))
}

/// Scheme + host + port, e.g. `https://msp.example.com`.
#[cfg(target_arch = "wasm32")]
pub fn origin() -> Option<String> {
    web_sys::window()?.location().origin().ok()
}

/// Host + port, e.g. `msp.example.com`.
#[cfg(target_arch = "wasm32")]
pub fn host() -> Option<String> {
    web_sys::window()?.location().host().ok()
}

/// Replace the current document with `url`, leaving no history entry.
/// Used for the hand-off to the OP's logout endpoint.
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn pathname() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn search() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn origin() -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn host() -> Option<String> {
    None
}

/// Hand `url` to the OS so it opens in the user's real browser.
///
/// The desktop window is the application, not a document to be replaced,
/// and the destinations these calls carry (the OP's logout endpoint, the
/// Bunyip hub) are web pages that belong in a browser anyway.
///
/// Failure is logged rather than returned: every caller reaches this
/// AFTER it has already cleared local session state, so the app is in
/// the state the user asked for whether or not the browser opened.
#[cfg(not(target_arch = "wasm32"))]
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
pub fn open_external(url: &str) -> Result<(), String> {
    open::that_detached(url).map_err(|e| format!("could not open {url} in a browser: {e}"))
}

/// There is no document to reload; the caller substitutes its own
/// recovery (see the error screen in `lib.rs`, which navigates home).
#[cfg(not(target_arch = "wasm32"))]
pub fn reload() -> bool {
    false
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
}

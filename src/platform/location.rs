//! Where the app thinks it is, and how it leaves (MAPPS-504).
//!
//! A desktop window has no URL bar, so every reader here answers `None`
//! on the native build and each caller falls back to what it already
//! falls back to when the browser refuses (configured values, the hub
//! URL, or "not on that route"). That is a real difference, not a
//! stub pretending to be a browser: the callers were already written
//! for a `None`, because `web_sys::window()` is itself an `Option`.

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

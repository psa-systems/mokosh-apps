//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_apps::components::{use_page_title_provider, CloseConfirmModal};
use mokosh_apps::hooks::{
    use_active_org_loader, use_apply_theme, use_auth_heartbeat, use_auth_provider,
    use_bfcache_invalidator, use_current_user_loader, use_server_status_monitor,
    use_session_end_watch, use_sidebar_collapsed_provider, use_sidebar_provider,
    use_sidebar_scroll_provider, use_standalone_token_refresh, use_theme_sync, use_token_refresh,
    use_update_check, use_version_cache_provider,
};
use mokosh_apps::Route;

// PMS-884: a wasm build with no renderer feature compiles cleanly and then
// dies on `dioxus::launch` with "No platform feature enabled", which reaches
// the user as a blank page and a console panic. That is exactly what shipped
// to staging, because `dx build` does not take this crate's default features:
// it picks the crate feature whose name matches the platform. Fail at compile
// time instead, so a bundle that cannot start cannot be built.
#[cfg(all(target_arch = "wasm32", not(feature = "web")))]
compile_error!(
    "a wasm build needs the `web` feature (it is what enables `dioxus/web`); \
     without it `dioxus::launch` panics at runtime. `dx` substitutes its own \
     feature list for this crate's defaults, so every dx invocation that \
     targets the browser has to pass `--features web` explicitly (see \
     oci-build/Dockerfile, Dockerfile and the `build` recipe)."
);

fn main() {
    // MAPPS-299: install the wasm panic hook BEFORE anything else runs so
    // a panic during boot (snapshot_initial_search, the first render,
    // hook setup) still produces a readable trace in the browser console
    // instead of an opaque `RuntimeError: unreachable` at a hex offset.
    // Web-only: the desktop / native builds inherit the default Rust
    // panic handler, which already writes to stderr.
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    // Snapshot ?code=...&state=... BEFORE the Dioxus Router mounts.
    // Dioxus 0.7's router can `history.replaceState` the URL to match
    // its declared route shape (no query params in our `/auth/callback`
    // route definition), which would erase the OAuth response before
    // AuthCallbackPage ever reads it. By capturing here, the OIDC flow
    // sees what the OP actually sent.
    mokosh_apps::modules::oidc::snapshot_initial_search();

    launch();
}

/// Start the app on the web renderer.
#[cfg(not(feature = "desktop"))]
fn launch() {
    dioxus::launch(App);
}

/// MAPPS-504: start the app in a native window.
///
/// The window is sized for the layout the SPA was designed around: the
/// persistent sidebar plus a content column. The minimum stops the user
/// dragging it narrower than the point where the sidebar rail and the
/// table columns start colliding.
#[cfg(feature = "desktop")]
fn launch() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};

    let mut window = WindowBuilder::new()
        .with_title("Mokosh Platform")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(1024.0, 680.0));

    match window_icon() {
        Ok(icon) => window = window.with_window_icon(Some(icon)),
        // Not fatal - the window opens with the toolkit default - but it
        // is a packaging mistake worth seeing rather than a blank icon
        // nobody can explain.
        Err(e) => tracing::error!("could not load the window icon: {e}"),
    }

    // MAPPS-506: a close request must never be the thing that destroys the
    // webview, or unsaved edits are gone before anything can ask about them.
    // `WindowHides` is dioxus-desktop's only non-destructive answer;
    // `platform::window_close` runs ahead of it on every close request and
    // decides between closing for real and asking first.
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                .with_window(window)
                .with_close_behaviour(WindowCloseBehaviour::WindowHides),
        )
        .launch(App);
}

/// Decode the 128x128 PNG that `Dioxus.toml`'s `[bundle]` block also
/// ships, so a `cargo run --features desktop` window carries the same
/// icon a bundled install does.
#[cfg(feature = "desktop")]
fn window_icon() -> Result<dioxus::desktop::tao::window::Icon, String> {
    const ICON_PNG: &[u8] = include_bytes!("../assets/icons/128x128.png");
    dioxus::desktop::icon_from_memory(ICON_PNG).map_err(|e| e.to_string())
}

#[component]
fn App() -> Element {
    use_auth_provider();
    use_sidebar_provider();
    // MAPPS-203: hold the desktop sidebar's scroll offset at the App root
    // so it survives the AppLayout re-mount on every navigation. Without
    // it the re-mounted sidebar starts at scrollTop 0 and visibly jumps to
    // the top on every nav click.
    use_sidebar_scroll_provider();
    // MAPPS-250: hold the desktop rail's collapsed/expanded choice at the App
    // root, alongside the scroll offset above, so it survives the AppLayout
    // re-mount on each navigation instead of resetting to expanded every click.
    use_sidebar_collapsed_provider();
    // MAPPS-366: hold the current page title at the App root so the persistent
    // AppShell top bar and document.title read it while each page sets it via
    // use_page_title. Mounted once, alongside the sidebar providers above.
    use_page_title_provider();
    // MAPPS-203: cache the result of GET /api/v1/version at App root so
    // the admin UpdateBanner does not re-run its async fetch (and the
    // 200ms reserve-then-collapse animation that goes with the
    // resource's None -> Some(...) transition) on every page navigation.
    use_version_cache_provider();
    // Background loop: rotates access tokens before expiry. No-op when
    // the user is not signed in. Mounted once at the app root so it
    // keeps running across navigations.
    use_token_refresh();
    // MAPPS-374: the standalone (legacy email+password) twin of the loop above.
    // Keeps a standalone session alive past its ~1h access-token expiry via
    // POST /api/v1/auth/refresh; no-op for OIDC sessions.
    use_standalone_token_refresh();
    // MAPPS-355: proactive 30s /auth/me heartbeat. On a 410
    // (ACCOUNT_DELETED) from mokosh-server the shared fetch layer flips
    // the ACCOUNT_DELETED GlobalSignal and the terminal overlay pops
    // within one interval, even if the user is idle on a page that does
    // not otherwise poll.
    use_auth_heartbeat();
    // MAPPS-427: name the organisation from mokosh's own tenant row. This used
    // to GET bunyip's /v1/auth/memberships, which 401s on every load and left
    // the top bar displaying the user's email address as an org name.
    use_active_org_loader();
    // Fetch the authoritative current user (role, name, avatar) from
    // mokosh-server /api/v1/auth/me on first authenticated mount, so the
    // displayed role reflects the server-side (PMS-172) translation rather
    // than the Technician default the id_token falls back to (PMS-158).
    use_current_user_loader();
    // Force a full reload when the browser restores this page from
    // bfcache. Without this, hitting back after logout would restore
    // the dashboard's prior JS state (including populated auth) from
    // the cache.
    use_bfcache_invalidator();
    // MAPPS-504: put the user on the login screen when the fetch layer
    // ends a session from outside the component tree. The browser does
    // that with a full reload; a desktop window has none, so it watches
    // the signal and clears the auth context instead. No-op on wasm.
    use_session_end_watch();
    // MAPPS-506: gate the desktop window's close request on the unsaved-changes
    // flag `use_unsaved_guard` publishes, so closing the window with a dirty
    // form asks first instead of discarding it. The browser does that from
    // `beforeunload`; no-op on wasm.
    mokosh_apps::platform::window_close::use_close_guard();
    // Apply the persisted theme preference on boot and follow system
    // dark-mode changes for `Theme::System` users.
    use_apply_theme();
    // MAPPS-259/PMS-410: once authenticated, reconcile the account's theme
    // prefs into localStorage (server wins) and re-apply, so the choice
    // follows the user across devices.
    use_theme_sync();
    // Background poll of `_mokosh_config.js` `build_sha`. Reloads the
    // tab at the next visibility-hidden boundary when a new SPA build
    // is detected, so users pick up deploys automatically (no
    // Ctrl+Shift+R required).
    use_update_check();
    // MAPPS-333: while mokosh-server is unreachable, poll /ready on an
    // interval and flip the app back to reachable on recovery. Idle (no
    // network) while healthy. Drives the ServerStatusBanner mounted in
    // AppLayout.
    use_server_status_monitor();

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
        // MAPPS-506: the answer to a refused window close. Renders nothing
        // until one is refused, and nothing at all on the web.
        CloseConfirmModal {}
    }
}

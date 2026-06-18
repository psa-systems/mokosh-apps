//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_apps::hooks::{
    use_apply_theme, use_auth_provider, use_bfcache_invalidator, use_current_user_loader,
    use_memberships_loader, use_sidebar_collapsed_provider, use_sidebar_provider,
    use_sidebar_scroll_provider, use_theme_sync, use_token_refresh, use_update_check,
    use_version_cache_provider,
};
use mokosh_apps::Route;

fn main() {
    // Snapshot ?code=...&state=... BEFORE the Dioxus Router mounts.
    // Dioxus 0.7's router can `history.replaceState` the URL to match
    // its declared route shape (no query params in our `/auth/callback`
    // route definition), which would erase the OAuth response before
    // AuthCallbackPage ever reads it. By capturing here, the OIDC flow
    // sees what the OP actually sent.
    mokosh_apps::modules::oidc::snapshot_initial_search();

    dioxus::launch(App);
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
    // MAPPS-203: cache the result of GET /api/v1/version at App root so
    // the admin UpdateBanner does not re-run its async fetch (and the
    // 200ms reserve-then-collapse animation that goes with the
    // resource's None -> Some(...) transition) on every page navigation.
    use_version_cache_provider();
    // Background loop: rotates access tokens before expiry. No-op when
    // the user is not signed in. Mounted once at the app root so it
    // keeps running across navigations.
    use_token_refresh();
    // Load /v1/auth/memberships after sign-in so AuthContext.memberships
    // is populated for the tenant switcher.
    use_memberships_loader();
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

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
    }
}

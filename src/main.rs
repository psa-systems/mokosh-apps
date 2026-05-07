//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_client::hooks::{use_auth_provider, use_sidebar_provider, use_token_refresh};
use mokosh_client::Route;

fn main() {
    // Snapshot ?code=...&state=... BEFORE the Dioxus Router mounts.
    // Dioxus 0.7's router can `history.replaceState` the URL to match
    // its declared route shape (no query params in our `/auth/callback`
    // route definition), which would erase the OAuth response before
    // AuthCallbackPage ever reads it. By capturing here, the OIDC flow
    // sees what the OP actually sent.
    mokosh_client::modules::oidc::snapshot_initial_search();

    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_auth_provider();
    use_sidebar_provider();
    // Background loop: rotates access tokens before expiry. No-op when
    // the user is not signed in. Mounted once at the app root so it
    // keeps running across navigations.
    use_token_refresh();

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
    }
}

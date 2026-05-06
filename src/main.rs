//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_client::hooks::{use_auth_provider, use_token_refresh};
use mokosh_client::Route;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_auth_provider();
    // Background loop: rotates access tokens before expiry. No-op when
    // the user is not signed in. Mounted once at the app root so it
    // keeps running across navigations.
    use_token_refresh();

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
    }
}

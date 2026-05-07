//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_client::hooks::{use_auth_provider, use_sidebar_provider};
use mokosh_client::Route;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_auth_provider();
    use_sidebar_provider();

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
    }
}

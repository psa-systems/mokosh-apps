//! Mokosh Platform - Cross-platform Dioxus client

use dioxus::prelude::*;
use mokosh_client::Route;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        Router::<Route> {}
    }
}

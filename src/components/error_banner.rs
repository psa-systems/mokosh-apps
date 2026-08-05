//! Shared inline error-message banner (MAPPS-418).
//!
//! One home for the compact red error recipe that was hand-rolled across
//! ~13 pages (and diverged into a few tints). Callers pass the message as
//! `children` and, optionally, their own spacing via `class` (e.g. `mb-3`).
//! Unlike the heavier icon + title [`crate::components::Alert`], this is the
//! icon-less, single-line banner shown above a form or list. It always sets
//! `role="alert"` so the error is announced (MAPPS-412 recipe:
//! `dark:bg-red-950/30` + `dark:border-red-900`).

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBannerProps {
    children: Element,
    /// Extra classes appended after the base recipe (e.g. caller margin "mb-3"/"mb-4").
    #[props(default)]
    class: String,
}

/// Compact inline error banner (MAPPS-418). Renders the standardized red
/// recipe with `role="alert"`; the message is passed as `children`.
#[component]
pub fn ErrorBanner(props: ErrorBannerProps) -> Element {
    let class = props.class;
    rsx! {
        div {
            role: "alert",
            class: "rounded-md border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 px-3 py-2 text-sm text-red-700 dark:text-red-300 {class}",
            {props.children}
        }
    }
}

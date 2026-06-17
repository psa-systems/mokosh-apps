//! Shared loading skeletons (PMS-353).
//!
//! `TableLoading` (in `table.rs`) already covers list pages that render a
//! `Table`. These two cover the layouts it cannot: the card-grid lists (e.g.
//! Projects) and the single-record detail pages. Using one shimmer style
//! (`bg-gray-200 dark:bg-gray-700 rounded animate-pulse`, matching
//! `TableLoading`) keeps every loading state consistent so no route renders a
//! bare "Loading…" string or a blank screen during the initial fetch.

use dioxus::prelude::*;

use crate::components::Card;

/// Placeholder grid that mirrors the project-card list layout
/// (`grid-cols-1 md:grid-cols-2 lg:grid-cols-3`) so the loading state matches
/// the populated grid shape instead of a single text line.
#[derive(Props, Clone, PartialEq)]
pub struct CardGridSkeletonProps {
    /// How many placeholder cards to render.
    #[props(default = 6)]
    pub count: usize,
}

#[component]
pub fn CardGridSkeleton(props: CardGridSkeletonProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
            for _ in 0..props.count {
                div {
                    class: "rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-6 space-y-4",
                    // title + subtitle
                    div { class: "h-5 w-2/3 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                    div { class: "h-4 w-1/2 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                    // a body line
                    div { class: "h-3 w-full bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                    // a progress-bar-shaped block (project cards show a budget bar)
                    div { class: "h-2 w-full bg-gray-200 dark:bg-gray-700 rounded-full animate-pulse" }
                }
            }
        }
    }
}

/// Placeholder card for a single-record detail page, used while the record
/// fetch is in flight instead of a bare "Loading…" line.
#[derive(Props, Clone, PartialEq)]
pub struct DetailSkeletonProps {
    /// How many shimmer body lines to render under the title block.
    #[props(default = 4)]
    pub rows: usize,
}

#[component]
pub fn DetailSkeleton(props: DetailSkeletonProps) -> Element {
    rsx! {
        Card {
            div { class: "space-y-4",
                // title block
                div { class: "h-6 w-1/3 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                for _ in 0..props.rows {
                    div { class: "h-4 w-full bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                }
                // a shorter trailing line
                div { class: "h-4 w-2/3 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
            }
        }
    }
}

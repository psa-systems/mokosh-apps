//! Button variant reference (PMS-357 AC4).
//!
//! A live showcase of the canonical `Button` variants, sizes, and states so
//! the convention is enforceable in review: when a new "create" button drifts
//! to the wrong colour, this page is the reference for what it should be. The
//! prose version lives in `dev-docs/button-variants.md`.
//!
//! Reachable at `/dev/buttons` (behind the auth guard).

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Button, ButtonSize, ButtonVariant, Card, IconSize, PageHeader, PlusIcon,
};

/// One labeled row in the showcase: a heading, a "when to use" note, and the
/// rendered example(s).
#[component]
fn ShowcaseRow(title: String, usage: String, children: Element) -> Element {
    rsx! {
        div { class: "py-4 border-b border-gray-200 dark:border-gray-700 last:border-0",
            h3 { class: "text-sm font-semibold text-gray-900 dark:text-white", "{title}" }
            p { class: "mt-0.5 mb-3 text-sm text-gray-500 dark:text-gray-400", "{usage}" }
            div { class: "flex flex-wrap items-center gap-3", {children} }
        }
    }
}

/// Canonical button reference page.
#[component]
pub fn ButtonShowcasePage() -> Element {
    rsx! {
        AppLayout { title: "Button Variants",
            PageHeader {
                title: "Button Variants",
                subtitle: "Canonical Button variants, sizes, and states (PMS-357). Match these in review.",
            }

            Card {
                ShowcaseRow {
                    title: "Primary".to_string(),
                    usage: "The page's main action: every \"New <thing>\" / create / submit button. Blue.".to_string(),
                    Button {
                        variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Ticket"
                    }
                    Button { variant: ButtonVariant::Primary, "Save" }
                }

                ShowcaseRow {
                    title: "Secondary".to_string(),
                    usage: "Neutral companion action next to a Primary: Cancel, Close, Filter, Back. Gray.".to_string(),
                    Button { variant: ButtonVariant::Secondary, "Cancel" }
                    Button { variant: ButtonVariant::Secondary, "Filter" }
                }

                ShowcaseRow {
                    title: "Danger".to_string(),
                    usage: "Destructive confirmation: Delete, Revoke, Remove (typically inside a confirm dialog). Red.".to_string(),
                    Button { variant: ButtonVariant::Danger, "Delete" }
                }

                ShowcaseRow {
                    title: "Ghost".to_string(),
                    usage: "Low-emphasis inline / toolbar action where a filled button would be too heavy. Transparent.".to_string(),
                    Button { variant: ButtonVariant::Ghost, "Edit" }
                }

                ShowcaseRow {
                    title: "Link".to_string(),
                    usage: "Inline text action that reads as a hyperlink (e.g. \"View all\"). Blue text, no fill.".to_string(),
                    Button { variant: ButtonVariant::Link, "View all" }
                }

                ShowcaseRow {
                    title: "Sizes".to_string(),
                    usage: "Small for dense toolbars / table rows, Medium (default) for forms and headers, Large for hero CTAs.".to_string(),
                    Button { variant: ButtonVariant::Primary, size: ButtonSize::Small, "Small" }
                    Button { variant: ButtonVariant::Primary, size: ButtonSize::Medium, "Medium" }
                    Button { variant: ButtonVariant::Primary, size: ButtonSize::Large, "Large" }
                }

                ShowcaseRow {
                    title: "States".to_string(),
                    usage: "Loading shows a spinner and blocks repeat clicks; disabled dims and ignores input.".to_string(),
                    Button { variant: ButtonVariant::Primary, "Default" }
                    Button { variant: ButtonVariant::Primary, loading: true, "Loading" }
                    Button { variant: ButtonVariant::Primary, disabled: true, "Disabled" }
                }
            }
        }
    }
}

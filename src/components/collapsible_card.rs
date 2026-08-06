//! Collapsible card section (MAPPS-247)
//!
//! A `Card` variant whose body collapses behind a chevron toggle in the
//! header. The company-detail page uses it to keep growable child sections
//! (contacts, sites, contracts, projects, invoices, assets) compact: each
//! card shows a capped preview that the user can collapse out of the way,
//! with an optional item count beside the title and the card's "View all"
//! actions left visible in the header even while collapsed.
//!
//! Mirrors `Card`'s API (title / actions / padding / class) plus a `count`
//! (rendered from the list envelope's `meta.total`) and a `default_open`
//! initial state. Only a dedicated chevron button toggles the body, so the
//! action links/buttons in the header keep working without bubbling a toggle.

use dioxus::prelude::*;

use crate::components::{ChevronDownIcon, ChevronRightIcon, IconSize};

/// Collapsible card props
#[derive(Props, Clone, PartialEq)]
pub struct CollapsibleCardProps {
    /// Card body, shown only while expanded.
    children: Element,
    /// Additional CSS classes on the outer container.
    #[props(default)]
    class: String,
    /// Header title (always required for a collapsible card).
    title: String,
    /// Optional item count shown beside the title (typically `meta.total`).
    #[props(default)]
    count: Option<u64>,
    /// Optional header actions (e.g. a "View all" link); stay visible while
    /// collapsed.
    actions: Option<Element>,
    /// Whether the body keeps its own padding (false for full-bleed tables).
    #[props(default = true)]
    padding: bool,
    /// Whether the body starts expanded.
    #[props(default = true)]
    default_open: bool,
}

/// Card whose body collapses behind a chevron toggle.
#[component]
pub fn CollapsibleCard(props: CollapsibleCardProps) -> Element {
    let mut open = use_signal(|| props.default_open);

    let base_class = "bg-surface rounded-lg shadow border border-line";
    let class = format!("{} {}", base_class, props.class);

    // Match `Card`'s body padding for the with-header case so a full-bleed
    // table (padding: false) sits flush while a padded body stays inset.
    let body_class = if props.padding { "px-6 pb-6 pt-4" } else { "" };

    // MAPPS-286: derive a stable id from the title so the toggle button's
    // `aria-controls` can point at the body region. The id is namespaced
    // to avoid collisions when the same card appears twice on a page;
    // assistive tech then ties "this toggle controls that region" without
    // a separate heading association. Spaces and other non-id-safe chars
    // are collapsed to dashes.
    let body_id = format!(
        "lc-collapsible-{}",
        props
            .title
            .chars()
            .map(|c| if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            })
            .collect::<String>()
    );

    rsx! {
        div { class: "{class}",
            div {
                class: "flex items-center justify-between px-6 pt-6 pb-4 border-b border-line",
                button {
                    r#type: "button",
                    class: "flex items-center gap-2 text-left",
                    aria_expanded: if open() { "true" } else { "false" },
                    aria_controls: "{body_id}",
                    onclick: move |_| {
                        let next = !open();
                        open.set(next);
                    },
                    if open() {
                        ChevronDownIcon { size: IconSize::Small, class: "text-subtle".to_string() }
                    } else {
                        ChevronRightIcon { size: IconSize::Small, class: "text-subtle".to_string() }
                    }
                    h3 { class: "text-lg font-medium text-content", "{props.title}" }
                    if let Some(count) = props.count {
                        span {
                            class: "ml-1 rounded-full bg-surface-2 px-2 py-0.5 text-xs font-medium text-muted",
                            "{count}"
                        }
                    }
                }
                div { class: "flex items-center space-x-2",
                    {props.actions}
                }
            }
            if open() {
                div { id: "{body_id}", class: "{body_class}",
                    {props.children}
                }
            }
        }
    }
}

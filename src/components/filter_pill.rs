//! Filter-pill chip bar for list surfaces (MAPPS-asset-search-v2 phase 1).
//!
//! One pill per applied facet. The chip renders a short `field:label`
//! and a close button; the page owns the pill vector and reads back
//! which filters to send to the server. Reusable: the asset list
//! consumes it first, but the shape is deliberately generic so the
//! tickets, companies, and contacts lists can mount the same bar
//! without re-rolling the chip.
//!
//! The field + op + values triple mirrors the server's `FacetFilter`
//! wire shape (`mokosh-server::modules::assets::models::FacetFilter`),
//! so the page's `onchange` can forward the pill set to the server
//! without translating.

use dioxus::prelude::*;

use crate::components::IconSize;

/// One applied filter. `field` is the server-side column name as the
/// server's `AssetField` enum serializes it (snake_case); `op` is one
/// of `eq | in | not_in | is_null | is_not_null | before | after |
/// within | contains_tag`; `values` holds the raw JSON values that
/// ride under the key. `label` is the human-readable chip body - the
/// page composes it from the field + the resolved display name
/// (`Company: Acme Corp`) rather than the uuid.
#[derive(Clone, PartialEq, Debug)]
pub struct FilterPill {
    pub field: String,
    pub op: String,
    pub values: Vec<serde_json::Value>,
    pub label: String,
}

impl FilterPill {
    /// Equality pill: single value, label `field: display`.
    pub fn eq(
        field: impl Into<String>,
        display: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        let field = field.into();
        let display = display.into();
        Self {
            label: format!("{field}: {display}"),
            field,
            op: "eq".to_string(),
            values: vec![value],
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct FilterPillBarProps {
    /// Current pill set.
    pub pills: Vec<FilterPill>,
    /// Called with the pill's index when the operator clicks the close
    /// button. The page writes `pills.remove(i)` and triggers a
    /// re-fetch.
    pub onremove: EventHandler<usize>,
    /// Optional "clear all" affordance. Rendered only when
    /// `pills.len() >= 2`, since a one-pill bar is just one click to
    /// clear anyway.
    #[props(default)]
    pub onclear: Option<EventHandler<()>>,
    #[props(default)]
    pub class: Option<String>,
}

#[component]
pub fn FilterPillBar(props: FilterPillBarProps) -> Element {
    if props.pills.is_empty() {
        return rsx! {};
    }
    let extra = props.class.unwrap_or_default();
    let pills = props.pills.clone();
    let pill_count = pills.len();
    rsx! {
        div {
            class: "flex flex-wrap items-center gap-2 {extra}",
            for (i, pill) in pills.into_iter().enumerate() {
                FilterChip {
                    key: "{pill.field}:{pill.op}:{i}",
                    label: pill.label.clone(),
                    onclose: move |_| props.onremove.call(i),
                }
            }
            if pill_count >= 2 {
                if let Some(onclear) = props.onclear {
                    button {
                        r#type: "button",
                        class: "text-sm text-gray-500 hover:text-gray-700 underline underline-offset-2",
                        onclick: move |_| onclear.call(()),
                        "Clear all"
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct FilterChipProps {
    pub label: String,
    pub onclose: EventHandler<()>,
}

#[component]
fn FilterChip(props: FilterChipProps) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center gap-1 rounded-full bg-indigo-50 border border-indigo-200 \
                    px-2.5 py-1 text-xs font-medium text-indigo-800 dark:bg-indigo-900/40 \
                    dark:border-indigo-800 dark:text-indigo-200",
            "{props.label}"
            button {
                r#type: "button",
                class: "ml-0.5 rounded-full p-0.5 hover:bg-indigo-100 dark:hover:bg-indigo-800/60",
                "aria-label": "Remove filter",
                onclick: move |_| props.onclose.call(()),
                crate::components::XMarkIcon { size: IconSize::Small }
            }
        }
    }
}

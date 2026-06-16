//! Reusable Asset picker (PMS-344).
//!
//! Mirror of [`crate::components::CompanyPicker`]: hits
//! `GET /assets?q=...&per_page=20` on each keystroke, renders the
//! matches in a click-to-select dropdown, and reports the selected
//! asset's UUID + name back through callbacks. Used on the New Ticket
//! form and on the ticket-detail inline-edit sidebar to associate a
//! ticket with an asset.
//!
//! Stays narrow on purpose: no `allow_inline_create` (creating an asset
//! is heavier than creating a company and is best done from the Assets
//! page itself), no per-page server-side scope to `company_id` (the
//! ticket form filters by company elsewhere; pre-filtering here would
//! constrain the picker awkwardly if the user changes the ticket's
//! company after picking an asset).

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::Input;
use crate::utils::url::urlencoding_minimal;

#[derive(Clone, Debug, Deserialize)]
struct PickerAsset {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    asset_tag: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PickerPage {
    data: Vec<PickerAsset>,
}

#[derive(Props, Clone, PartialEq)]
pub struct AssetPickerProps {
    /// Currently selected asset name (or empty if none selected).
    pub value: String,
    /// Optional currently selected id. When `Some` the picker renders
    /// as a "selected chip with a Change button" instead of the
    /// search dropdown - matches the CompanyPicker shape.
    pub selected_id: Option<String>,
    /// Field label rendered above the input.
    #[props(default = String::from("Asset"))]
    pub label: String,
    /// Mark the underlying input required. Defaults to false because
    /// most tickets have no asset; the field is opt-in per ticket.
    #[props(default)]
    pub required: bool,
    /// Fires once when the user picks a row. Receives `(id, name)`.
    pub onselect: EventHandler<(String, String)>,
    /// Fires when the user clears the selection (Change / X button).
    pub onclear: EventHandler<()>,
}

#[component]
pub fn AssetPicker(props: AssetPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);

    let query_text = query.read().trim().to_string();
    let resource_q = query_text.clone();
    let results = use_resource(move || {
        let q = resource_q.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let path = if q.is_empty() {
                "/assets?per_page=20".to_string()
            } else {
                format!("/assets?q={}&per_page=20", urlencoding_minimal(&q))
            };
            crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
                .await
                .ok()
                .map(|p| p.data)
        }
    });

    if let Some(id) = &props.selected_id {
        let id = id.clone();
        let name = props.value.clone();
        let onclear = props.onclear;
        return rsx! {
            div { class: "space-y-1",
                label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-0.5", "*" }
                    }
                }
                div {
                    class: "flex items-center justify-between border border-gray-300 dark:border-gray-700 rounded-md px-3 py-2 bg-gray-50 dark:bg-gray-900",
                    div { class: "min-w-0",
                        p { class: "text-sm font-medium text-gray-900 dark:text-gray-100 truncate", "{name}" }
                        p { class: "text-xs text-gray-500 truncate", "{id}" }
                    }
                    button {
                        r#type: "button",
                        class: "text-xs text-blue-600 hover:text-blue-500 px-2 py-1",
                        onclick: move |_| {
                            onclear.call(());
                            query.set(String::new());
                        },
                        "Change"
                    }
                }
            }
        };
    }

    let snap = results.read_unchecked();
    let onselect = props.onselect;
    rsx! {
        div { class: "relative space-y-1",
            Input {
                name: "asset_search",
                label: props.label,
                placeholder: "Search assets...",
                required: props.required,
                value: query.read().clone(),
                oninput: move |e: FormEvent| {
                    query.set(e.value());
                    show_dropdown.set(true);
                },
            }
            if *show_dropdown.read() {
                // Backdrop dismisses the dropdown on click-outside; sits
                // below the dropdown's z-20 so rows stay clickable.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| show_dropdown.set(false),
                }
                div {
                    class: "absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-lg",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-gray-500", "Searching..." }
                        },
                        Some(None) => rsx! {
                            div { class: "px-3 py-2 text-sm text-red-600", "Could not load assets." }
                        },
                        Some(Some(rows)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-gray-500",
                                if query_text.is_empty() {
                                    "No assets yet."
                                } else {
                                    "No matches."
                                }
                            }
                        },
                        Some(Some(rows)) => {
                            let rows = rows.clone();
                            rsx! {
                                ul { class: "py-1",
                                    for row in rows.into_iter() {
                                        {
                                            let id_str = row.id.to_string();
                                            let key = id_str.clone();
                                            let name = row.name.clone();
                                            let tag = row
                                                .asset_tag
                                                .clone()
                                                .filter(|s| !s.trim().is_empty());
                                            let id_for_click = id_str.clone();
                                            let name_for_click = name.clone();
                                            rsx! {
                                                li {
                                                    key: "{key}",
                                                    button {
                                                        r#type: "button",
                                                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-800",
                                                        onclick: move |_| {
                                                            onselect.call((id_for_click.clone(), name_for_click.clone()));
                                                            show_dropdown.set(false);
                                                            query.set(name_for_click.clone());
                                                        },
                                                        span { class: "font-medium", "{name}" }
                                                        if let Some(t) = tag {
                                                            span { class: "ml-2 text-xs text-gray-500 font-mono", "{t}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

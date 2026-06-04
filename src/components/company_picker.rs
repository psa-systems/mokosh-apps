//! Reusable Company picker.
//!
//! Hits `GET /v1/companies?q=...&page_size=20` with a small debounce
//! and renders the matches in a click-to-select dropdown. The selected
//! company's UUID is reported back via the `onselect` callback; the
//! displayed name is reported via the `value` signal so the calling
//! form can persist the human label across renders.
//!
//! Used by the new/edit Contact form (replacing the hardcoded
//! `("1", "Acme Corp")` dropdown), and intended to be reused by the
//! ticket new form and the time-entry "Internal / no ticket" path
//! when those land.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::Input;

#[derive(Clone, Debug, Deserialize)]
struct PickerCompany {
    id: uuid::Uuid,
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct PickerPage {
    data: Vec<PickerCompany>,
}

#[derive(Props, Clone, PartialEq)]
pub struct CompanyPickerProps {
    /// Currently selected company name (or empty if none selected).
    pub value: String,
    /// Optional currently selected id. When `Some` the picker renders
    /// as a "selected chip with a Change button" instead of the
    /// search dropdown.
    pub selected_id: Option<String>,
    /// Field label rendered above the input.
    #[props(default = String::from("Company"))]
    pub label: String,
    /// Mark the underlying input required.
    #[props(default)]
    pub required: bool,
    /// Fires once when the user picks a row. Receives `(id, name)`.
    pub onselect: EventHandler<(String, String)>,
    /// Fires when the user clears the selection (Change / X button).
    pub onclear: EventHandler<()>,
}

#[component]
pub fn CompanyPicker(props: CompanyPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);

    let query_text = query.read().trim().to_string();
    let resource_q = query_text.clone();
    let results = use_resource(move || {
        let q = resource_q.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let path = if q.is_empty() {
                "/companies?page_size=20".to_string()
            } else {
                // The server stores names as-is and matches with ILIKE so
                // we can pass the trimmed query straight through.
                format!("/companies?q={}&page_size=20", urlencoding_minimal(&q))
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
                name: "company_search",
                label: props.label,
                placeholder: "Search companies...",
                required: props.required,
                value: query.read().clone(),
                oninput: move |e: FormEvent| {
                    query.set(e.value());
                    show_dropdown.set(true);
                },
            }
            if *show_dropdown.read() {
                div {
                    class: "absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto rounded-md border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 shadow-lg",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-gray-500", "Searching..." }
                        },
                        Some(None) => rsx! {
                            div { class: "px-3 py-2 text-sm text-red-600", "Could not load companies." }
                        },
                        Some(Some(rows)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-gray-500",
                                if query_text.is_empty() {
                                    "No companies yet."
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
                                                        "{name}"
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

/// Tiny percent-encoder for the few characters that actually break URL
/// query strings. The server-side ILIKE comparison tolerates trimmed
/// raw text, so the only chars we need to escape are `&`, `#`, `?`,
/// `+`, space, and a couple of control bytes. Avoids pulling in a full
/// `urlencoding` crate just for the picker.
fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '+' => out.push_str("%2B"),
            '=' => out.push_str("%3D"),
            c if (c as u32) < 0x20 => out.push_str(&format!("%{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

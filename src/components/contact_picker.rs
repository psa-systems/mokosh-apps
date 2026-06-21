//! Reusable Contact picker (MAPPS-207).
//!
//! Mirror of [`crate::components::AssetPicker`] / [`crate::components::CompanyPicker`]:
//! hits `GET /contacts/contacts?q=...&per_page=20` on each keystroke,
//! renders the matches in a click-to-select dropdown, and reports the
//! selected contact's UUID + full name back through callbacks.
//!
//! Used to close the relational gaps in MAPPS-207:
//!   * the company-detail "Add Contact" flow, to attach an *existing*
//!     contact to a company instead of only creating a new one, and
//!   * the New Ticket form, to associate the ticket with an existing
//!     contact.
//!
//! Optional `company_filter`: when `Some(uuid)` the search is scoped to
//! that company via the server's `company_id` query param so the New
//! Ticket form only offers contacts that belong to the selected company.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::Input;
use crate::utils::url::urlencoding_minimal;

#[derive(Clone, Debug, Deserialize)]
struct PickerContact {
    id: uuid::Uuid,
    /// The server's `ContactResponse` carries a precomputed `full_name`;
    /// fall back to first/last when an older payload omits it.
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

impl PickerContact {
    fn display_name(&self) -> String {
        let name = self.full_name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        format!("{} {}", self.first_name, self.last_name)
            .trim()
            .to_string()
    }
}

#[derive(Clone, Debug, Deserialize)]
struct PickerPage {
    data: Vec<PickerContact>,
}

#[derive(Props, Clone, PartialEq)]
pub struct ContactPickerProps {
    /// Currently selected contact name (or empty if none selected).
    pub value: String,
    /// Optional currently selected id. When `Some` the picker renders
    /// as a "selected chip with a Change button" instead of the search
    /// dropdown - matches the Company/Asset picker shape.
    pub selected_id: Option<String>,
    /// Field label rendered above the input.
    #[props(default = String::from("Contact"))]
    pub label: String,
    /// Mark the underlying input required.
    #[props(default)]
    pub required: bool,
    /// When `Some`, scope the search to this company's contacts via the
    /// server's `company_id` filter.
    #[props(default)]
    pub company_filter: Option<String>,
    /// Fires once when the user picks a row. Receives `(id, name)`.
    pub onselect: EventHandler<(String, String)>,
    /// Fires when the user clears the selection (Change button).
    pub onclear: EventHandler<()>,
}

#[component]
pub fn ContactPicker(props: ContactPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    let mut editing = use_signal(|| false);

    let company_filter = props.company_filter.clone();
    // Read the query signal INSIDE the use_resource closure so Dioxus
    // subscribes the resource to it and re-fetches on every keystroke
    // (same pattern as the Company/Asset pickers).
    let query_text = query.read().trim().to_string();
    let results = use_resource(move || {
        let company_filter = company_filter.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let q = query.read().trim().to_string();
            let mut path = String::from("/contacts/contacts?per_page=20");
            if !q.is_empty() {
                path.push_str(&format!("&q={}", urlencoding_minimal(&q)));
            }
            if let Some(cid) = company_filter.as_ref().filter(|s| !s.is_empty()) {
                path.push_str(&format!("&company_id={}", urlencoding_minimal(cid)));
            }
            crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
                .await
                .ok()
                .map(|p| p.data)
        }
    });

    // Chip view: parent supplied a selected_id and the user is not in
    // inline-edit mode. Change swaps to search mode locally without a
    // server round trip.
    if let Some(_id) = &props.selected_id {
        if !editing() {
            let name = props.value.clone();
            let onclear = props.onclear;
            // MAPPS-273: drop the raw UUID secondary line. See the
            // CompanyPicker fix in the same change for context.
            return rsx! {
                div { class: "space-y-1",
                    label { class: "block text-sm font-medium text-content",
                        "{props.label}"
                        if props.required {
                            span { class: "text-red-500 ml-0.5", "*" }
                        }
                    }
                    div {
                        class: "flex items-center justify-between border border-line rounded-md px-3 py-2 bg-app",
                        div { class: "min-w-0",
                            p { class: "text-sm font-medium text-content truncate", "{name}" }
                        }
                        button {
                            r#type: "button",
                            class: "text-xs text-accent hover:opacity-90 px-2 py-1",
                            onclick: move |_| {
                                onclear.call(());
                                query.set(String::new());
                                editing.set(true);
                                show_dropdown.set(true);
                            },
                            "Change"
                        }
                    }
                }
            };
        }
    }

    let snap = results.read_unchecked();
    let onselect = props.onselect;
    rsx! {
        div { class: "relative space-y-1",
            Input {
                name: "contact_search",
                label: props.label,
                placeholder: "Search contacts...",
                required: props.required,
                value: query.read().clone(),
                oninput: move |e: FormEvent| {
                    query.set(e.value());
                    show_dropdown.set(true);
                },
            }
            if *show_dropdown.read() {
                // Transparent full-viewport backdrop dismisses the
                // dropdown on click-outside; sits below it (z-10 vs z-20)
                // so the rows stay clickable.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        show_dropdown.set(false);
                        editing.set(false);
                    },
                }
                div {
                    class: "absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto rounded-md border border-line bg-raised shadow-lg",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching..." }
                        },
                        Some(None) => rsx! {
                            div { class: "px-3 py-2 text-sm text-red-600", "Could not load contacts." }
                        },
                        Some(Some(rows)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted",
                                if query_text.is_empty() {
                                    "No contacts yet."
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
                                            let name = row.display_name();
                                            let company = row
                                                .company_name
                                                .clone()
                                                .filter(|s| !s.trim().is_empty());
                                            let email = row
                                                .email
                                                .clone()
                                                .filter(|s| !s.trim().is_empty());
                                            let id_for_click = id_str.clone();
                                            let name_for_click = name.clone();
                                            rsx! {
                                                li {
                                                    key: "{key}",
                                                    button {
                                                        r#type: "button",
                                                        class: "w-full text-left px-3 py-2 text-sm hover:bg-surface-2",
                                                        onclick: move |_| {
                                                            onselect.call((id_for_click.clone(), name_for_click.clone()));
                                                            show_dropdown.set(false);
                                                            editing.set(false);
                                                            query.set(name_for_click.clone());
                                                        },
                                                        span { class: "font-medium", "{name}" }
                                                        if let Some(c) = company {
                                                            span { class: "ml-2 text-xs text-muted", "{c}" }
                                                        }
                                                        if let Some(e) = email {
                                                            span { class: "ml-2 text-xs text-subtle", "{e}" }
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

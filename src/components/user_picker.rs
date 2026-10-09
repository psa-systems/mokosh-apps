//! Reusable User picker.
//!
//! Mirror of [`crate::components::AssetPicker`] / [`crate::components::ContactPicker`]:
//! hits `GET /auth/users?q=...&per_page=20` on each keystroke, renders
//! the matches in a click-to-select dropdown, and reports the selected
//! user's UUID + display name back through callbacks.
//!
//! Introduced to retire the raw-UUID input on the Members > Teams
//! "Add member" flow, where every operator was expected to paste a UUID
//! lifted from another page. The endpoint is `RequireManager` on the
//! server, which matches the audience that reaches the Teams tab in
//! the first place.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Button, ButtonSize, ButtonVariant, ErrorBanner, Input};
use crate::hooks::use_dropdown_nav;
use crate::utils::url::urlencoding_minimal;

#[derive(Clone, Debug, Deserialize)]
struct PickerUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: String,
}

impl PickerUser {
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
    data: Vec<PickerUser>,
}

#[derive(Props, Clone, PartialEq)]
pub struct UserPickerProps {
    /// Currently selected user name (or empty if none selected).
    pub value: String,
    /// Optional currently selected id. When `Some` the picker renders
    /// as a chip with a Change button instead of the search dropdown.
    pub selected_id: Option<String>,
    /// Field label rendered above the input.
    #[props(default = String::from("User"))]
    pub label: String,
    /// Placeholder on the search input.
    #[props(default = String::from("Search users…"))]
    pub placeholder: String,
    /// Mark the underlying input required.
    #[props(default)]
    pub required: bool,
    /// Hide a user id from the match list. Used to filter out the
    /// current team roster so an already-added member doesn't surface
    /// as a selectable row.
    #[props(default)]
    pub exclude_ids: Vec<uuid::Uuid>,
    /// Fires once when the user picks a row. Receives `(id, name)`.
    pub onselect: EventHandler<(String, String)>,
    /// Fires when the user clears the selection (Change button).
    pub onclear: EventHandler<()>,
}

#[component]
pub fn UserPicker(props: UserPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut nav = use_dropdown_nav("user-picker").enter_takes_first_match();
    let mut editing = use_signal(|| false);

    let query_text = query.read().trim().to_string();
    let query_debounced = crate::hooks::use_debounced_signal(query, 300);
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query_debounced.read().trim().to_string();
        let path = if q.is_empty() {
            "/auth/users?per_page=20".to_string()
        } else {
            format!("/auth/users?q={}&per_page=20", urlencoding_minimal(&q))
        };
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .map(|p| p.data)
            .inspect_err(|e| tracing::warn!("user search failed: {e}"))
    });

    if let Some(_id) = &props.selected_id {
        if !editing() {
            let name = props.value.clone();
            let onclear = props.onclear;
            let show_label = !props.label.trim().is_empty();
            return rsx! {
                div { class: "space-y-1 w-full",
                    if show_label {
                        label { class: "block text-sm font-medium text-content",
                            "{props.label}"
                            if props.required {
                                span { class: "text-red-500 dark:text-red-400 ml-0.5", "*" }
                            }
                        }
                    }
                    div {
                        class: "flex items-center justify-between border border-line rounded-md px-3 py-2 bg-app w-full min-w-0",
                        div { class: "min-w-0 flex-1 text-left",
                            p { class: "text-sm font-medium text-content truncate", "{name}" }
                        }
                        div { class: "flex items-center gap-1 shrink-0 ml-2",
                            Button {
                                variant: ButtonVariant::Link,
                                size: ButtonSize::Small,
                                onclick: move |_| {
                                    onclear.call(());
                                    query.set(String::new());
                                    editing.set(true);
                                    nav.open();
                                },
                                "Change"
                            }
                        }
                    }
                }
            };
        }
    }

    let snap = results.read_unchecked();
    let onselect = props.onselect;
    let exclude: Vec<uuid::Uuid> = props.exclude_ids.clone();
    let rows: Vec<PickerUser> = match &*snap {
        Some(Ok(rows)) => rows
            .iter()
            .filter(|r| !exclude.contains(&r.id))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    let nav_len = rows.len();
    let rows_for_keys = rows.clone();
    rsx! {
        div { class: "relative space-y-1",
            div {
                role: "combobox",
                aria_expanded: nav.expanded(),
                aria_controls: nav.panel_id(),
                aria_activedescendant: nav.active_descendant(),
                onfocusin: move |_| nav.open(),
                onclick: move |_| nav.open(),
                onkeydown: move |e: KeyboardEvent| {
                    let rows = rows_for_keys.clone();
                    nav.keydown(&e, nav_len, move |index| {
                        if let Some(row) = rows.get(index) {
                            let name = row.display_name();
                            onselect.call((row.id.to_string(), name.clone()));
                            editing.set(false);
                            query.set(name);
                        }
                    });
                },
                Input {
                    name: "user_search",
                    label: props.label,
                    placeholder: props.placeholder,
                    required: props.required,
                    value: query.read().clone(),
                    oninput: move |e: FormEvent| {
                        query.set(e.value());
                        nav.open_fresh();
                    },
                }
            }
            if nav.is_open() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        nav.close();
                        editing.set(false);
                    },
                }
                div {
                    id: nav.panel_id(),
                    role: "listbox",
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching…" }
                        },
                        Some(Err(_)) => rsx! {
                            ErrorBanner { class: "m-1", "Could not search. Try again." }
                        },
                        Some(Ok(_)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted",
                                if query_text.is_empty() {
                                    "No users yet."
                                } else {
                                    "No matches."
                                }
                            }
                        },
                        Some(Ok(_)) => rsx! {
                            ul { class: "py-1", role: "none",
                                for (index , row) in rows.iter().enumerate() {
                                    {
                                        let id_str = row.id.to_string();
                                        let key = id_str.clone();
                                        let name = row.display_name();
                                        let email = {
                                            let e = row.email.trim();
                                            if e.is_empty() { None } else { Some(e.to_string()) }
                                        };
                                        let id_for_click = id_str.clone();
                                        let name_for_click = name.clone();
                                        rsx! {
                                            li {
                                                key: "{key}",
                                                id: nav.row_id(index),
                                                role: "option",
                                                aria_selected: nav.row_selected(index),
                                                button {
                                                    r#type: "button",
                                                    tabindex: "-1",
                                                    class: nav.row_class(index, "w-full text-left px-3 py-2 text-sm hover:bg-surface-2"),
                                                    onclick: move |_| {
                                                        onselect.call((id_for_click.clone(), name_for_click.clone()));
                                                        nav.close();
                                                        editing.set(false);
                                                        query.set(name_for_click.clone());
                                                    },
                                                    span { class: "font-medium", "{name}" }
                                                    if let Some(e) = email {
                                                        span { class: "ml-2 text-xs text-muted", "{e}" }
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

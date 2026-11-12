//! Reusable User picker.
//!
//! Mirror of [`crate::components::AssetPicker`] / [`crate::components::ContactPicker`]:
//! hits `GET /auth/users?q=...&per_page=20` once the query has been still
//! for 300 ms (MAPPS-855), renders
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

// SPEC-5.74.3: response row shape; absent fields default to empty
/// One row of `GET /auth/users`, mokosh-server's `UserResponse`.
///
/// MAPPS-1247: no `#[serde(default)]`. The server serialises all four name
/// and email fields as plain (non-optional) strings, so a body missing one
/// is a broken response; it now fails to decode and the search shows its
/// failure banner, instead of becoming a row with an empty name.
#[derive(Clone, Debug, Deserialize)]
struct PickerUser {
    id: uuid::Uuid,
    full_name: String,
    first_name: String,
    last_name: String,
    email: String,
}

impl PickerUser {
    // SPEC-5.74.4: display name rule
    /// The name to show: the full name, else first and last, else the email.
    /// A user with none of those shows [`UNNAMED_USER`] and is logged once
    /// with its id (MAPPS-1247), rather than rendering a blank row and chip.
    fn display_name(&self) -> String {
        let name = self.full_name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        let parts = format!("{} {}", self.first_name.trim(), self.last_name.trim());
        let parts = parts.trim();
        if !parts.is_empty() {
            return parts.to_string();
        }
        let email = self.email.trim();
        if !email.is_empty() {
            return email.to_string();
        }
        let _ =
            crate::utils::unresolved("user_picker.row", "user name", Some(self.id), UNNAMED_USER);
        UNNAMED_USER.to_string()
    }
}

/// Shown for a user with no name and no email (MAPPS-1247).
const UNNAMED_USER: &str = "Unnamed user";

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
    // SPEC-5.75.5: prop defaults
    #[props(default = String::from("User"))]
    pub label: String,
    /// Placeholder on the search input.
    #[props(default = String::from("Search users…"))]
    pub placeholder: String,
    /// Mark the underlying input required.
    #[props(default)]
    pub required: bool,
    // SPEC-5.74.5: ids hidden from the match list
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
    // SPEC-5.77.2: keyboard navigation, Enter takes the first match
    let mut nav = use_dropdown_nav("user-picker").enter_takes_first_match();
    let mut editing = use_signal(|| false);

    let query_text = query.read().trim().to_string();
    // SPEC-5.74.2: 300 ms debounce
    let query_debounced = crate::hooks::use_debounced_signal(query, 300);
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query_debounced.read().trim().to_string();
        // SPEC-5.74.1: empty query omits q; per_page is 20
        let path = if q.is_empty() {
            "/auth/users?per_page=20".to_string()
        } else {
            format!("/auth/users?q={}&per_page=20", urlencoding_minimal(&q))
        };
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .map(|p| p.data)
            // SPEC-5.74.6: log the failure; the dropdown shows it (SPEC-5.76.6)
            .inspect_err(|e| tracing::warn!("user search failed: {e}"))
    });

    // SPEC-5.75.1: chip while selected and not editing
    // SPEC-5.75.4: in editing mode the search input and dropdown show even with selected_id Some.
    if let Some(_id) = &props.selected_id {
        if !editing() {
            let name = props.value.clone();
            let onclear = props.onclear;
            // SPEC-5.75.2: label shown only when non-blank
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
                                // SPEC-5.75.3: Change clears, enters editing, opens the dropdown; SPEC-5.77.4 (the only onclear call)
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
            // SPEC-5.74.5: exclusion applied to rendered rows
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
                // SPEC-5.76.1: combobox wrapper
                role: "combobox",
                aria_expanded: nav.expanded(),
                aria_controls: nav.panel_id(),
                aria_activedescendant: nav.active_descendant(),
                onfocusin: move |_| nav.open(),
                onclick: move |_| nav.open(),
                // SPEC-5.77.3: keyboard selection mirrors click selection
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
                    // SPEC-5.76.2: each input stores the query and reopens fresh
                    oninput: move |e: FormEvent| {
                        query.set(e.value());
                        nav.open_fresh();
                    },
                }
            }
            if nav.is_open() {
                // SPEC-5.76.3: overlay click closes and leaves editing
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        nav.close();
                        editing.set(false);
                    },
                }
                // SPEC-5.76.4: listbox panel
                div {
                    id: nav.panel_id(),
                    role: "listbox",
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto",
                    match &*snap {
                        // SPEC-5.76.5: loading text
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching…" }
                        },
                        // SPEC-5.76.6: visible search failure
                        Some(Err(_)) => rsx! {
                            ErrorBanner { class: "m-1", "Could not search. Try again." }
                        },
                        // SPEC-5.76.7: empty states by query
                        Some(Ok(_)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted",
                                if query_text.is_empty() {
                                    "No users yet."
                                } else {
                                    "No matches."
                                }
                            }
                        },
                        // SPEC-5.76.8: option rows
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
                                                    // SPEC-5.77.1: click selection; SPEC-5.77.4 (onselect once per action)
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

/// MAPPS-1247: no blank names, and no silent defaults on the server's
/// required fields.
#[cfg(test)]
mod mapps_1247_tests {
    use super::{PickerUser, UNNAMED_USER};

    fn user(full: &str, first: &str, last: &str, email: &str) -> PickerUser {
        PickerUser {
            id: uuid::Uuid::from_u128(3),
            full_name: full.into(),
            first_name: first.into(),
            last_name: last.into(),
            email: email.into(),
        }
    }

    #[test]
    fn the_name_falls_back_to_parts_then_email_then_a_placeholder() {
        assert_eq!(user("Alex Ng", "", "", "").display_name(), "Alex Ng");
        assert_eq!(user(" ", "Alex", "Ng", "").display_name(), "Alex Ng");
        assert_eq!(
            user("", "", "", "alex@example.com").display_name(),
            "alex@example.com"
        );
        assert_eq!(user("", " ", "", "").display_name(), UNNAMED_USER);
    }

    #[test]
    fn a_row_missing_a_required_field_fails_to_decode() {
        let full = r#"{"id":"00000000-0000-0000-0000-000000000003","full_name":"A","first_name":"A","last_name":"","email":"a@example.com"}"#;
        assert!(serde_json::from_str::<PickerUser>(full).is_ok());
        let missing = r#"{"id":"00000000-0000-0000-0000-000000000003","first_name":"A","last_name":"","email":"a@example.com"}"#;
        assert!(serde_json::from_str::<PickerUser>(missing).is_err());
    }
}

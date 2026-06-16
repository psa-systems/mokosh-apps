//! Reusable Company picker.
//!
//! Hits `GET /contacts/companies?q=...&per_page=20` on each keystroke
//! (no debounce) and renders the matches in a click-to-select dropdown.
//! The selected
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

use crate::components::{Button, ButtonVariant, Input, Modal, ModalSize};
use crate::utils::url::urlencoding_minimal;

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
    /// PMS-352: when true, the dropdown renders a "Create new company"
    /// action that opens an inline modal with a minimal name-only New
    /// Company form. On successful POST /contacts/companies the new
    /// row is auto-selected via `onselect`, so the user lands back on
    /// the parent form with the newly-created company already picked
    /// and no fields lost. Opt-in (default off) because contact / time-
    /// entry pickers do not want the affordance.
    #[props(default)]
    pub allow_inline_create: bool,
}

/// Subset of the server's `CompanyResponse` the inline-create modal
/// needs to read back. We only consume `id` + `name` to feed `onselect`
/// after a successful POST; serde drops the rest.
#[derive(Clone, Debug, Deserialize)]
struct CreatedCompany {
    id: uuid::Uuid,
    name: String,
}

#[component]
pub fn CompanyPicker(props: CompanyPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    // PMS-352: inline create-company modal state. `new_name` carries
    // whatever was typed into the picker input when the modal opened so
    // the user doesn't have to re-type the company name they were
    // already searching for.
    let allow_inline_create = props.allow_inline_create;
    let mut show_create_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut create_error = use_signal(String::new);

    // PMS-371: read the query signal INSIDE the use_resource closure so
    // Dioxus subscribes the resource to it. The prior pattern (read
    // outside, capture the String) only subscribed the parent component,
    // not the resource, so keystrokes never re-fetched and the dropdown
    // showed the unfiltered initial result list regardless of what the
    // user had typed.
    let query_text = query.read().trim().to_string();
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query.read().trim().to_string();
        let path = if q.is_empty() {
            "/contacts/companies?per_page=20".to_string()
        } else {
            // The server stores names as-is and matches with ILIKE so
            // we can pass the trimmed query straight through.
            format!(
                "/contacts/companies?q={}&per_page=20",
                urlencoding_minimal(&q)
            )
        };
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .ok()
            .map(|p| p.data)
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
                // Transparent full-viewport backdrop: a click anywhere outside
                // the dropdown dismisses it. Sits below the dropdown (z-10 vs
                // z-20) so the rows stay clickable.
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
                            div { class: "px-3 py-2 text-sm text-red-600", "Could not load companies." }
                        },
                        Some(Some(rows)) if rows.is_empty() => {
                            let query_for_seed = query_text.clone();
                            rsx! {
                                div { class: "px-3 py-2 text-sm text-gray-500",
                                    if query_text.is_empty() {
                                        "No companies yet."
                                    } else {
                                        "No matches."
                                    }
                                }
                                // PMS-352: inline create affordance. Renders
                                // only when the parent opted in via the new
                                // `allow_inline_create` prop, so contact /
                                // time-entry pickers stay unchanged.
                                if allow_inline_create {
                                    button {
                                        r#type: "button",
                                        class: "w-full text-left px-3 py-2 text-sm border-t border-gray-100 dark:border-gray-800 text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/30",
                                        onclick: move |_| {
                                            new_name.set(query_for_seed.clone());
                                            create_error.set(String::new());
                                            show_dropdown.set(false);
                                            show_create_modal.set(true);
                                        },
                                        if query_text.is_empty() {
                                            "+ Create new company"
                                        } else {
                                            {
                                                let q = query_text.clone();
                                                rsx! { "+ Create \"{q}\"" }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Some(Some(rows)) => {
                            let rows = rows.clone();
                            let query_for_seed = query_text.clone();
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
                                // PMS-352: same inline create affordance at
                                // the bottom of the populated list so a user
                                // searching for a similar-name company that
                                // is not actually in the list can still
                                // create one without leaving the form.
                                // PMS-371: echo the typed text in the label
                                // when non-empty (`+ Create "Wile E. Coyote
                                // Demolition"`) so the affordance matches
                                // the empty-list branch and gives the user
                                // a clear preview of what name will be
                                // submitted.
                                if allow_inline_create {
                                    div { class: "border-t border-gray-100 dark:border-gray-800",
                                        button {
                                            r#type: "button",
                                            class: "w-full text-left px-3 py-2 text-sm text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/30",
                                            onclick: move |_| {
                                                new_name.set(query_for_seed.clone());
                                                create_error.set(String::new());
                                                show_dropdown.set(false);
                                                show_create_modal.set(true);
                                            },
                                            if query_text.is_empty() {
                                                "+ Create new company"
                                            } else {
                                                {
                                                    let q = query_text.clone();
                                                    rsx! { "+ Create \"{q}\"" }
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
            // PMS-352: inline New Company modal. Renders unconditionally
            // when `allow_inline_create` is true so the parent component
            // does not need to plumb its own modal; Modal's `open=false`
            // means it returns rsx!{} anyway.
            if allow_inline_create {
                {
                    // Wire the Create button. The closure here issues the
                    // POST and, on success, fires `onselect` with the new
                    // row's id + name so the calling form (e.g. New Ticket)
                    // lands with the new company already picked.
                    let mut results_resource = results;
                    let on_create = move |_| {
                        let name_v = new_name.read().trim().to_string();
                        if name_v.is_empty() {
                            create_error.set("Enter a company name.".to_string());
                            return;
                        }
                        if creating() {
                            return;
                        }
                        creating.set(true);
                        create_error.set(String::new());
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let body = serde_json::json!({ "name": name_v });
                                match crate::hooks::fetch::api::post_authed::<CreatedCompany, _>(
                                    "/contacts/companies",
                                    &body,
                                )
                                .await
                                {
                                    Ok(created) => {
                                        let id_str = created.id.to_string();
                                        let name = created.name.clone();
                                        // Refresh the picker's list so the
                                        // new row shows up if the user clears
                                        // the selection later in the same session.
                                        results_resource.restart();
                                        onselect.call((id_str, name.clone()));
                                        query.set(name);
                                        new_name.set(String::new());
                                        show_create_modal.set(false);
                                    }
                                    Err(err) => {
                                        create_error.set(format!(
                                            "Could not create company: {err}"
                                        ));
                                    }
                                }
                            }
                            creating.set(false);
                        });
                    };
                    rsx! {
                        Modal {
                            open: show_create_modal(),
                            title: "Create new company".to_string(),
                            size: ModalSize::Medium,
                            onclose: move |_| {
                                if !creating() {
                                    show_create_modal.set(false);
                                }
                            },
                            footer: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        if !creating() {
                                            show_create_modal.set(false);
                                        }
                                    },
                                    "Cancel"
                                }
                                Button {
                                    variant: ButtonVariant::Primary,
                                    loading: creating(),
                                    onclick: on_create,
                                    "Create"
                                }
                            },
                            div { class: "space-y-3",
                                if !create_error.read().is_empty() {
                                    p { class: "text-sm text-red-600 dark:text-red-400",
                                        "{create_error}"
                                    }
                                }
                                Input {
                                    name: "new_company_name",
                                    label: "Company name",
                                    placeholder: "Acme Corp",
                                    required: true,
                                    value: new_name.read().clone(),
                                    oninput: move |e: FormEvent| new_name.set(e.value()),
                                }
                                p { class: "text-xs text-gray-500 dark:text-gray-400",
                                    "Creates a Client company with default settings. Edit type, status, billing details, and contact info from the company detail page."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

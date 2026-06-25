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

use crate::components::{Button, ButtonVariant, Input, Modal, ModalSize};
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
    /// MAPPS-276: opt-in inline "+ Create new contact" affordance.
    /// Mirrors the same flag on `CompanyPicker`. When `true`, an empty
    /// or no-match dropdown shows a "+ Create" button that opens an
    /// inline modal with a minimal first / last / email form. On
    /// successful POST, the new row is auto-selected via `onselect` so
    /// the parent form lands with the newly-created contact already
    /// picked. Defaults off so parents that intentionally only attach
    /// existing contacts stay unchanged.
    #[props(default)]
    pub allow_inline_create: bool,
}

/// Subset of the server's `ContactResponse` the inline-create modal
/// reads back. We only consume `id` and the name fields to feed
/// `onselect` after a successful POST; serde drops everything else.
#[derive(Clone, Debug, Deserialize)]
struct CreatedContact {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

impl CreatedContact {
    fn display_name(&self) -> String {
        let full = self.full_name.trim();
        if !full.is_empty() {
            return full.to_string();
        }
        format!("{} {}", self.first_name, self.last_name)
            .trim()
            .to_string()
    }
}

#[component]
pub fn ContactPicker(props: ContactPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    let mut editing = use_signal(|| false);
    // MAPPS-276: inline-create modal state. Same shape as the
    // CompanyPicker's. `new_first`/`new_last` seed the form when opened
    // (we split the typed query into first/last on a single space so a
    // user who typed "Jane Doe" lands on a pre-filled form).
    let allow_inline_create = props.allow_inline_create;
    let mut show_create_modal = use_signal(|| false);
    let mut new_first = use_signal(String::new);
    let mut new_last = use_signal(String::new);
    let mut new_email = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut create_error = use_signal(String::new);

    let company_filter = props.company_filter.clone();
    let company_filter_for_create = props.company_filter.clone();
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
                        Some(Some(rows)) if rows.is_empty() => {
                            let query_for_seed = query_text.clone();
                            rsx! {
                                div { class: "px-3 py-2 text-sm text-muted",
                                    if query_text.is_empty() {
                                        "No contacts yet."
                                    } else {
                                        "No matches."
                                    }
                                }
                                // MAPPS-276: inline create affordance. Opt-in
                                // per parent so contact picker call sites
                                // that should only attach existing contacts
                                // stay unchanged.
                                if allow_inline_create {
                                    button {
                                        r#type: "button",
                                        class: "w-full text-left px-3 py-2 text-sm border-t border-line text-accent hover:bg-accent-50 dark:hover:bg-accent-900/30",
                                        onclick: move |_| {
                                            seed_create_form(&query_for_seed, &mut new_first, &mut new_last);
                                            new_email.set(String::new());
                                            create_error.set(String::new());
                                            show_dropdown.set(false);
                                            show_create_modal.set(true);
                                        },
                                        if query_text.is_empty() {
                                            "+ Create new contact"
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
                                // MAPPS-276: inline-create affordance at the
                                // bottom of the populated list so a user
                                // searching for a similar-name contact who
                                // is not actually in the list can still
                                // create one without leaving the form.
                                if allow_inline_create {
                                    div { class: "border-t border-line",
                                        button {
                                            r#type: "button",
                                            class: "w-full text-left px-3 py-2 text-sm text-accent hover:bg-accent-50 dark:hover:bg-accent-900/30",
                                            onclick: move |_| {
                                                seed_create_form(&query_for_seed, &mut new_first, &mut new_last);
                                                new_email.set(String::new());
                                                create_error.set(String::new());
                                                show_dropdown.set(false);
                                                show_create_modal.set(true);
                                            },
                                            if query_text.is_empty() {
                                                "+ Create new contact"
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
            // MAPPS-276: inline New Contact modal. Renders unconditionally
            // (Modal's `open=false` returns rsx!{} anyway) so the parent
            // doesn't need its own modal plumbing.
            if allow_inline_create {
                {
                    let mut results_resource = results;
                    let company_filter_for_create = company_filter_for_create.clone();
                    let on_create = move |_| {
                        let first = new_first.read().trim().to_string();
                        let last = new_last.read().trim().to_string();
                        if first.is_empty() || last.is_empty() {
                            create_error.set("First and last name are required.".to_string());
                            return;
                        }
                        if creating() {
                            return;
                        }
                        creating.set(true);
                        create_error.set(String::new());
                        let email_v = new_email.read().trim().to_string();
                        let cid = company_filter_for_create.clone();
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let mut body = serde_json::Map::new();
                                body.insert("first_name".into(), serde_json::json!(first));
                                body.insert("last_name".into(), serde_json::json!(last));
                                if !email_v.is_empty() {
                                    body.insert("email".into(), serde_json::json!(email_v));
                                }
                                // Inherit the picker's company scope when the
                                // parent passed one (e.g. the New Ticket form
                                // scopes contacts to the picked company), so
                                // the new contact lands attached to that
                                // company without an extra step.
                                if let Some(cid) = cid.filter(|c| !c.is_empty()) {
                                    if let Ok(uuid) = uuid::Uuid::parse_str(&cid) {
                                        body.insert("company_id".into(), serde_json::json!(uuid));
                                    }
                                }
                                let body = serde_json::Value::Object(body);
                                match crate::hooks::fetch::api::post_authed::<CreatedContact, _>(
                                    "/contacts/contacts",
                                    &body,
                                )
                                .await
                                {
                                    Ok(created) => {
                                        let id_str = created.id.to_string();
                                        let name = created.display_name();
                                        results_resource.restart();
                                        onselect.call((id_str, name.clone()));
                                        query.set(name);
                                        new_first.set(String::new());
                                        new_last.set(String::new());
                                        new_email.set(String::new());
                                        show_create_modal.set(false);
                                    }
                                    Err(err) => {
                                        create_error.set(format!(
                                            "Could not create contact: {err}"
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
                            title: "Create new contact".to_string(),
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
                                div { class: "grid grid-cols-1 gap-3 sm:grid-cols-2",
                                    Input {
                                        name: "new_contact_first",
                                        label: "First name",
                                        required: true,
                                        value: new_first.read().clone(),
                                        oninput: move |e: FormEvent| new_first.set(e.value()),
                                    }
                                    Input {
                                        name: "new_contact_last",
                                        label: "Last name",
                                        required: true,
                                        value: new_last.read().clone(),
                                        oninput: move |e: FormEvent| new_last.set(e.value()),
                                    }
                                }
                                Input {
                                    name: "new_contact_email",
                                    label: "Email (optional)",
                                    r#type: "email".to_string(),
                                    value: new_email.read().clone(),
                                    oninput: move |e: FormEvent| new_email.set(e.value()),
                                }
                                p { class: "text-xs text-muted",
                                    "Creates the contact with the default contact type. Edit phone, mobile, title, and other fields from the contact detail page."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Split a typed search query into first / last name guesses so the
/// inline-create modal opens with a pre-filled form when the user
/// invokes it after typing.
fn seed_create_form(query: &str, first: &mut Signal<String>, last: &mut Signal<String>) {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        first.set(String::new());
        last.set(String::new());
        return;
    }
    if let Some((f, l)) = trimmed.split_once(char::is_whitespace) {
        first.set(f.trim().to_string());
        last.set(l.trim().to_string());
    } else {
        first.set(trimmed.to_string());
        last.set(String::new());
    }
}

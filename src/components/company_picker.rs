//! Reusable Company picker.
//!
//! Hits `GET /contacts/companies?q=...&status=active&per_page=20` on each keystroke
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

use crate::components::form::submit_on_enter;
use crate::components::{
    Button, ButtonSize, ButtonVariant, ErrorBanner, IconSize, Input, Modal, ModalSize, PlusIcon,
};
use crate::hooks::{use_dropdown_nav, NavRows};
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
    /// MAPPS-322: inline validation message rendered under the picker (same
    /// slot the wrapped [`Input`] uses for its own errors). The parent form's
    /// submit guard sets this when no company is picked, so "Company is
    /// required." surfaces next to the picker instead of in the form-level
    /// banner. Empty string renders nothing.
    #[props(default)]
    pub error: String,
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
    /// MAPPS-484: render a "+ New company" button beside the input that opens
    /// the same create modal `allow_inline_create` puts in the dropdown. The
    /// dropdown affordance only exists once the dropdown is open, so a form
    /// that needs a visible create control sets this too. Default off, so no
    /// existing call site changes; ignored without `allow_inline_create`,
    /// which owns the modal.
    #[props(default)]
    pub show_create_button: bool,
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
    // MAPPS-503: open / highlight state and the shared keyboard contract.
    // MAPPS-653: the field has to end up holding a company, so Enter takes the
    // first match when the user has not arrowed onto one.
    let mut nav = use_dropdown_nav("company-picker").enter_takes_first_match();
    // PMS-352: inline create-company modal state. `new_name` carries
    // whatever was typed into the picker input when the modal opened so
    // the user doesn't have to re-type the company name they were
    // already searching for.
    let allow_inline_create = props.allow_inline_create;
    // MAPPS-484: the modal lives behind `allow_inline_create`, so the button
    // that opens it cannot render without it.
    let show_create_button = props.show_create_button && allow_inline_create;
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
        // MAPPS-575: active companies only. Archiving is meant to take a
        // company out of day-to-day use, and this picker IS day-to-day use: an
        // archived company still offered here would keep being attached to new
        // contacts, tickets and time entries, which is the state the operator
        // archived it to leave. Editing a record that already names an archived
        // company is unaffected, because the stored name is rendered from the
        // record rather than re-resolved through this search.
        let path = if q.is_empty() {
            "/contacts/companies?status=active&per_page=20".to_string()
        } else {
            // The server stores names as-is and matches with ILIKE so
            // we can pass the trimmed query straight through.
            format!(
                "/contacts/companies?q={}&status=active&per_page=20",
                urlencoding_minimal(&q)
            )
        };
        // MAPPS-503: keep the failure. `.ok()` here made a failed search
        // indistinguishable from "still loading", so the panel sat on
        // "Searching…" forever with nothing logged.
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .map(|p| p.data)
            .inspect_err(|e| tracing::warn!("company search failed: {e}"))
    });

    if let Some(_id) = &props.selected_id {
        let name = props.value.clone();
        let onclear = props.onclear;
        // MAPPS-273: render only the human-readable name in the
        // selected-chip state. The previous "secondary line" leaked the
        // raw company UUID into the UI, which surprised users on the
        // Contact form ("what is this hex string under Acme Corp?") and
        // doesn't survive the "no raw UUIDs in user-facing UI" rule.
        return rsx! {
            div { class: "space-y-1",
                label { class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 dark:text-red-400 ml-0.5", "*" }
                    }
                }
                div {
                    class: "flex items-center justify-between border border-line rounded-md px-3 py-2 bg-app",
                    div { class: "min-w-0",
                        p { class: "text-sm font-medium text-content truncate", "{name}" }
                    }
                    Button {
                        variant: ButtonVariant::Link,
                        size: ButtonSize::Small,
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
    // MAPPS-503: the navigable rows. The inline create action is the last
    // one, so Down reaches it, and it only counts while a result list is
    // actually rendered (not under "Searching…" or the failure banner).
    let rows: Vec<PickerCompany> = match &*snap {
        Some(Ok(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loaded = matches!(&*snap, Some(Ok(_)));
    // MAPPS-653: with nothing matching the typed text, the create action is
    // row 0, which is what makes Enter on a no-match query start a new company.
    let list = NavRows::new(rows.len(), allow_inline_create && loaded);
    // The button sits beside the input, so it drops by the height of the
    // label when the picker renders one.
    let create_button_class = if props.label.is_empty() {
        "whitespace-nowrap"
    } else {
        "whitespace-nowrap mt-6"
    };
    let query_for_button = query_text.clone();
    let rows_for_keys = rows.clone();
    let query_for_keys = query_text.clone();
    rsx! {
        div { class: "relative space-y-1",
            div { class: "flex items-start gap-2",
                // MAPPS-503: combobox seam. The handlers live on this wrapper
                // rather than on the shared `Input` (MAPPS-347), and on the
                // input's own wrapper rather than the row above it, so a click
                // on the "+ New company" button does not bubble back in here
                // and re-open the list it just closed.
                div {
                    class: "flex-1 min-w-0",
                    role: "combobox",
                    aria_expanded: nav.expanded(),
                    aria_controls: nav.panel_id(),
                    aria_activedescendant: nav.active_descendant(),
                    onfocusin: move |_| nav.open(),
                    onclick: move |_| nav.open(),
                    onkeydown: move |e: KeyboardEvent| {
                        let rows = rows_for_keys.clone();
                        let seed = query_for_keys.clone();
                        nav.keydown(&e, list.len, move |index| {
                            match rows.get(index) {
                                Some(row) => {
                                    let name = row.name.clone();
                                    onselect.call((row.id.to_string(), name.clone()));
                                    query.set(name);
                                }
                                // Past the last result row: the inline create action.
                                None => {
                                    new_name.set(seed);
                                    create_error.set(String::new());
                                    show_create_modal.set(true);
                                }
                            }
                        });
                    },
                    Input {
                        name: "company_search",
                        label: props.label,
                        placeholder: "Search companies…",
                        required: props.required,
                        // MAPPS-322: forward the parent's validation message so a
                        // blank-company submit paints the red border + inline error
                        // on the picker, matching every other required field.
                        error: props.error.clone(),
                        value: query.read().clone(),
                        oninput: move |e: FormEvent| {
                            query.set(e.value());
                            nav.open_fresh();
                        },
                    }
                }
                // MAPPS-484: the visible create affordance. Opens the same
                // modal as the in-dropdown "Create new company", so what it
                // creates is a real `companies` row, not a typed name.
                if show_create_button {
                    Button {
                        variant: ButtonVariant::Secondary,
                        class: create_button_class.to_string(),
                        onclick: move |_| {
                            new_name.set(query_for_button.clone());
                            create_error.set(String::new());
                            nav.close();
                            show_create_modal.set(true);
                        },
                        PlusIcon { size: IconSize::Small, class: "mr-1".to_string() }
                        "New company"
                    }
                }
            }
            if nav.is_open() {
                // Transparent full-viewport backdrop: a click anywhere outside
                // the dropdown dismisses it. Sits below the dropdown (z-10 vs
                // z-20) so the rows stay clickable.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| nav.close(),
                }
                div {
                    id: nav.panel_id(),
                    role: "listbox",
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching…" }
                        },
                        // MAPPS-503: a failed search is its own state, distinct
                        // from "Searching…" and "No matches."
                        // MAPPS-444: it takes the shared banner (paired hues,
                        // role="alert"); `m-1` keeps its border off the
                        // dropdown's own edge.
                        Some(Err(_)) => rsx! {
                            ErrorBanner { class: "m-1", "Could not search. Try again." }
                        },
                        Some(Ok(_)) => rsx! {
                            if rows.is_empty() {
                                div { class: "px-3 py-2 text-sm text-muted",
                                    if query_text.is_empty() {
                                        "No companies yet."
                                    } else {
                                        "No matches."
                                    }
                                }
                            } else {
                                // MAPPS-503: `role="none"` so the rows stay the
                                // listbox panel's own options.
                                ul { class: "py-1", role: "none",
                                    for (index , row) in rows.iter().enumerate() {
                                        {
                                            let id_str = row.id.to_string();
                                            let key = id_str.clone();
                                            let name = row.name.clone();
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
                                                        // MAPPS-503: out of the tab order, so Tab
                                                        // commits and moves to the next field
                                                        // instead of walking into the list.
                                                        tabindex: "-1",
                                                        class: nav.row_class(index, "w-full text-left px-3 py-2 text-sm hover:bg-surface-2"),
                                                        onclick: move |_| {
                                                            onselect.call((id_for_click.clone(), name_for_click.clone()));
                                                            nav.close();
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
                            // PMS-352: inline create affordance. Renders only
                            // when the parent opted in via `allow_inline_create`,
                            // so contact / time-entry pickers stay unchanged. It
                            // sits below the matches so a user searching for a
                            // similar-name company that is not actually in the
                            // list can still create one without leaving the form.
                            // PMS-371: echo the typed text in the label when
                            // non-empty (`+ Create "Wile E. Coyote Demolition"`),
                            // so the user sees what name will be submitted.
                            // MAPPS-320: visually distinct Create band (gap + top
                            // border + muted background + leading icon) so a
                            // fat-finger click on the bottom match can't bleed
                            // into Create.
                            // MAPPS-503: it is the last navigable row, reachable
                            // by Down and committed by Enter / Tab.
                            if let Some(index) = list.create_index {
                                {
                                    let query_for_seed = query_text.clone();
                                    rsx! {
                                        div { class: "mt-1 border-t border-line",
                                            button {
                                                r#type: "button",
                                                tabindex: "-1",
                                                id: nav.row_id(index),
                                                role: "option",
                                                aria_selected: nav.row_selected(index),
                                                class: nav.row_class(index, "flex w-full items-center gap-1.5 text-left px-3 py-2 text-sm bg-surface-2/50 text-accent hover:bg-accent-50 dark:hover:bg-accent-900/30"),
                                                onclick: move |_| {
                                                    new_name.set(query_for_seed.clone());
                                                    create_error.set(String::new());
                                                    nav.close();
                                                    show_create_modal.set(true);
                                                },
                                                PlusIcon { size: IconSize::Small }
                                                if query_text.is_empty() {
                                                    "Create new company"
                                                } else {
                                                    {
                                                        let q = query_text.clone();
                                                        rsx! { "Create \"{q}\"" }
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
                    // MAPPS-694: takes `()` rather than the click event, so the
                    // Create button and the modal's Enter key run the same
                    // action. Every capture is Copy, so both handlers get one.
                    let mut on_create = move |_: ()| {
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
                            #[cfg(feature = "app")]
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
                                    onclick: move |_| on_create(()),
                                    "Create"
                                }
                            },
                            div {
                                class: "space-y-3",
                                // MAPPS-694: Enter from any field in the modal
                                // creates, so the name typed into the picker is
                                // committed without tabbing to the button.
                                onkeydown: submit_on_enter(move || on_create(())),
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
                                    // MAPPS-694: the modal opens with this
                                    // already prefilled, so it is where the
                                    // caret belongs.
                                    autofocus: true,
                                    value: new_name.read().clone(),
                                    oninput: move |e: FormEvent| new_name.set(e.value()),
                                }
                                p { class: "text-xs text-muted",
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

#[cfg(test)]
mod tests {
    /// This file up to the test module, so a needle below cannot match itself.
    fn component_source() -> &'static str {
        include_str!("company_picker.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap()
    }

    /// MAPPS-694: Enter in the create modal runs the SAME action as the Create
    /// button, so the name typed into the picker is committed without tab
    /// counting. What the handler does with the key is covered by
    /// `submit_on_enter` in `form.rs`; what is decided here is that both
    /// controls call one `on_create`, and that the handler sits on the modal
    /// body rather than on a single field, so Enter commits from anywhere in
    /// the modal. The modal only exists inside a mounted picker with a live
    /// search resource, which the host test harness cannot render, so the
    /// wiring is asserted where it is written.
    #[test]
    fn the_create_button_and_enter_run_one_create_action() {
        let src = component_source();
        assert_eq!(
            src.matches("on_create(())").count(),
            2,
            "exactly two callers: the Create button and the modal's Enter key"
        );
        assert!(
            src.contains("onclick: move |_| on_create(()),"),
            "the Create button runs it"
        );
        assert!(
            src.contains("onkeydown: submit_on_enter(move || on_create(())),"),
            "and so does Enter, through the shared handler"
        );

        let body = src
            .find(r#"class: "space-y-3","#)
            .expect("the modal body is the `space-y-3` div");
        let keydown = src
            .find("onkeydown: submit_on_enter")
            .expect("the Enter handler is wired");
        let first_field = src
            .find(r#"name: "new_company_name","#)
            .expect("the modal's first field");
        assert!(
            body < keydown && keydown < first_field,
            "the handler belongs to the modal BODY, so Enter commits from any \
             field in it, not only the one it was attached to"
        );
    }

    /// And the field the modal opens on is the prefilled one, so the caret
    /// lands on the name rather than on the dialog panel.
    #[test]
    fn the_prefilled_name_field_is_the_one_that_takes_focus() {
        let src = component_source();
        assert_eq!(
            src.matches("autofocus: true,").count(),
            1,
            "one field takes focus, and it is the prefilled name"
        );
        let field = src
            .find(r#"name: "new_company_name","#)
            .expect("the modal's first field");
        let focus = src.find("autofocus: true,").expect("it autofocuses");
        let next = src
            .find(r#"value: new_name.read().clone(),"#)
            .expect("the field binds the seeded name");
        assert!(
            field < focus && focus < next,
            "the autofocus belongs to new_company_name"
        );
    }
}

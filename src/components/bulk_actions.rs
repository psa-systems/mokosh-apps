//! Bulk-action primitives shared across every list view (MAPPS-290).
//!
//! Pages opt in by mounting a [`BulkSelection`] signal at page scope,
//! wrapping their `for row in rows` loop's first cell in
//! [`SelectRowCell`], adding a [`SelectAllHeader`] as the first column,
//! and mounting [`BulkActionsBar`] above the table. The bar renders
//! only when the selection is non-empty and exposes whatever verbs the
//! page passes as children (each typically a `Button` whose onclick
//! reads the selection ids, fires a parallel `spawn` of PUT/DELETE
//! calls, then clears the selection on success).
//!
//! Why a single shared signal type instead of letting each page roll
//! its own `HashSet<String>`: every page wants identical
//! "select-all-on-current-page" / "clear" / "is-selected" semantics, so
//! lifting the helpers behind a single [`BulkSelection`] keeps each
//! list view from re-implementing them slightly differently.

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::components::Button;
use crate::components::ButtonVariant;

/// Page-scoped selection state. Holds the set of currently-selected
/// row ids (the page picks the id type - usually `entity.id.to_string()`).
/// The signal is `Copy` (Dioxus signals are), so every event handler
/// captures it cheaply without context plumbing.
pub type BulkSelection = Signal<HashSet<String>>;

/// Helper: mount the page-scoped selection signal at component scope.
/// Returns a fresh empty selection every mount.
pub fn use_bulk_selection() -> BulkSelection {
    use_signal(HashSet::<String>::new)
}

/// Helper: toggle a single id in the selection.
pub fn toggle_selected(selection: &mut BulkSelection, id: &str) {
    let mut set = selection.write();
    if set.contains(id) {
        set.remove(id);
    } else {
        set.insert(id.to_string());
    }
}

/// Helper: replace the selection with all the ids in `ids`. Used by the
/// header checkbox to select / clear the current page.
pub fn set_all_selected(selection: &mut BulkSelection, ids: impl IntoIterator<Item = String>) {
    let next: HashSet<String> = ids.into_iter().collect();
    *selection.write() = next;
}

/// Helper: empty the selection. Called by `BulkActionsBar`'s Cancel and
/// by bulk-action `onclick` handlers after a successful mutation.
pub fn clear_selection(selection: &mut BulkSelection) {
    selection.write().clear();
}

#[derive(Props, Clone, PartialEq)]
pub struct SelectRowCellProps {
    selection: BulkSelection,
    id: String,
}

/// Per-row checkbox. Renders inside a `td` so it sits in the table's
/// own first column without needing a separate wrapper. Reading the
/// signal inside the closure subscribes the cell to selection changes
/// so unchecking a row elsewhere repaints this one.
#[component]
pub fn SelectRowCell(mut props: SelectRowCellProps) -> Element {
    let id_for_check = props.id.clone();
    let id_for_click = props.id.clone();
    let checked = props.selection.read().contains(&id_for_check);
    rsx! {
        td {
            class: "px-4 py-4 whitespace-nowrap w-10",
            // Stop the row's clickable onclick from firing when the user
            // is targeting the checkbox itself - otherwise selecting a
            // row would also navigate to its detail page.
            onclick: move |e: MouseEvent| e.stop_propagation(),
            input {
                r#type: "checkbox",
                class: "h-4 w-4 rounded border-line text-accent focus:ring-accent",
                aria_label: "Select row",
                checked,
                onchange: move |_| toggle_selected(&mut props.selection, &id_for_click),
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct SelectAllHeaderProps {
    selection: BulkSelection,
    /// The ids visible on the current page. Used both to compute the
    /// "all visible selected" state (so the header reads `checked` when
    /// the page set matches the selection) and to populate the
    /// selection when the user clicks the header.
    ids: Vec<String>,
}

/// Select-all checkbox in the table header. Clicking it toggles
/// between "page set is the selection" and "selection is empty".
#[component]
pub fn SelectAllHeader(mut props: SelectAllHeaderProps) -> Element {
    let ids_for_check = props.ids.clone();
    let ids_for_click = props.ids.clone();
    let all_selected = !ids_for_check.is_empty()
        && ids_for_check
            .iter()
            .all(|id| props.selection.read().contains(id));
    rsx! {
        th {
            class: "px-4 py-3 w-10",
            input {
                r#type: "checkbox",
                class: "h-4 w-4 rounded border-line text-accent focus:ring-accent",
                aria_label: if all_selected { "Clear selection" } else { "Select all rows on this page" },
                checked: all_selected,
                onchange: move |_| {
                    if all_selected {
                        clear_selection(&mut props.selection);
                    } else {
                        set_all_selected(&mut props.selection, ids_for_click.clone());
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BulkActionsBarProps {
    selection: BulkSelection,
    /// Singular noun for the toast ("ticket", "asset"). Pluralised by
    /// the bar to read "3 tickets selected".
    label: String,
    /// The verb buttons. The caller's `onclick` handlers read
    /// `selection.read()` for the ids, fire their mutations, then call
    /// `clear_selection(&mut selection)` on success.
    children: Element,
}

/// Sticky bar that mounts above a list when the selection is
/// non-empty. Shows the count, hosts the caller's verb buttons, and
/// offers a Cancel button that clears the selection.
#[component]
pub fn BulkActionsBar(mut props: BulkActionsBarProps) -> Element {
    let count = props.selection.read().len();
    if count == 0 {
        return rsx! {};
    }
    let label = if count == 1 {
        format!("1 {} selected", props.label)
    } else {
        format!("{} {}s selected", count, props.label)
    };
    rsx! {
        div {
            class: "mb-4 flex items-center justify-between rounded-md border border-accent bg-accent-50 dark:bg-accent-900/30 px-4 py-2",
            div {
                class: "flex items-center gap-3",
                span { class: "text-sm font-medium text-content", "{label}" }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| clear_selection(&mut props.selection),
                    "Cancel"
                }
            }
            div { class: "flex items-center gap-2",
                {props.children}
            }
        }
    }
}

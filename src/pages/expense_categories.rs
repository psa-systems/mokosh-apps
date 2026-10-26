//! The tenant's own taxonomy of what money went to (MAPPS-1157, server
//! MAPPS-1100 Phase A).
//!
//! A category is applied to a bill line or an expense line to classify the
//! cost. One taxonomy per tenant: a tenant seeds its own set rather than
//! inheriting a global list, because an MSP's chart of costs is theirs and
//! an inherited list is one nobody owns.
//!
//! The same Settings page shape Products and Vendors use, with one
//! difference the server dictates: there is no DELETE route, so a category
//! is retired with `is_active = false` and the modal offers no Delete
//! button. Retiring rather than deleting is also the right model, because a
//! bill line already classified keeps naming it.
//!
//! Finance-gated, like its siblings and for the same reason: the server
//! answers `super_admin | admin | finance`.
//!
//! A QuickBooks tenant maps each of these to a ledger account on the
//! provider's mapping page (MAPPS-1156): a QuickBooks bill line carries an
//! `AccountRef`, and the category is what resolves it.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, DataTable, ErrorBanner, IconSize,
    PageHeader, PlusIcon, SettingFormModal, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

const PER_PAGE: usize = 25;

/// `ExpenseCategoryResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteCategory {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_active: bool,
}

/// The list path for a page. Pagination and nothing else, which is all
/// `GET /expense-categories` reads.
pub(crate) fn list_path(page: usize) -> String {
    format!("/expense-categories?page={page}&per_page={PER_PAGE}")
}

/// `/settings/expense-categories`. Finance-gated like Products.
#[component]
pub fn ExpenseCategoriesSettingsPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Expense Categories");
    if !has_finance {
        return rsx! {
            crate::pages::billing::NoFinancePermission { title: "Expense Categories" }
        };
    }
    rsx! { ExpenseCategoriesSettingsBody {} }
}

#[component]
fn ExpenseCategoriesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<CategoryFormState>);

    let current_page = (*page.read()).max(1);
    let mut resource = use_resource(move || {
        // Read inside the closure so the resource subscribes to the page
        // signal itself (MAPPS-847).
        let path = list_path((*page.read()).max(1));
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteCategory>>(&path)
                .await
                .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
                .ok()
        }
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteCategory>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Expense Categories".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Expense Categories",
            subtitle: "What a cost was for, on a bill line or an expense claim",
            breadcrumbs: rsx! {
                crate::pages::settings::SettingsBreadcrumb { current: Route::SettingsExpenseCategories {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(CategoryFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Category"
                }
            },
        }

        p { class: "mb-4 text-sm text-muted",
            "Your own list, not an inherited one. A QuickBooks tenant also maps each category to a ledger account on the QuickBooks page, because every bill line QuickBooks accepts names the account it posts to."
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load expense categories. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 2,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Status" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 2, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 2,
                        title: "No categories yet".to_string(),
                        description: "Add the kinds of cost you want to see split out: hardware, software, colocation, travel.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                                onclick: move |_| editing.set(Some(CategoryFormState::new())),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Category"
                            }
                        },
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = CategoryFormState::from_existing(&row);
                                let name = row.name.clone();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: {
                                            let edit_state = edit_state.clone();
                                            move |_| editing.set(Some(edit_state.clone()))
                                        },
                                        TableCell {
                                            // MAPPS-569: the row opens a modal, so this cell is
                                            // the keyboard path.
                                            onactivate: move |_| editing.set(Some(edit_state.clone())),
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if active {
                                                Badge { variant: BadgeVariant::Green, "Active" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "Retired" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            CategoryFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CategoryFormState {
    id: Option<String>,
    name: String,
    is_active: bool,
}

impl CategoryFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            is_active: true,
        }
    }

    fn from_existing(c: &RemoteCategory) -> Self {
        Self {
            id: Some(c.id.to_string()),
            name: c.name.clone(),
            is_active: c.is_active,
        }
    }

    /// The `UpsertExpenseCategoryRequest` body.
    fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name.trim(),
            "is_active": self.is_active,
        })
    }
}

#[derive(Props, Clone, PartialEq)]
struct CategoryFormModalProps {
    state: CategoryFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn CategoryFormModal(props: CategoryFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut name_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());

        let mut guard = FormGuard::new();
        let name_v = name.read().trim().to_string();
        name_err.set(guard.field(
            "category_name",
            &name_v,
            "Name",
            &[Rule::Required, Rule::MaxLen(100)],
        ));
        if guard.blocked() {
            return;
        }

        saving.set(true);
        let body = CategoryFormState {
            id: save_id.clone(),
            name: name_v,
            is_active: *is_active.read(),
        }
        .body();
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/expense-categories",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                        &format!("/expense-categories/{id}"),
                        &body,
                    )
                    .await
                    .map(|_| ()),
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save category: {err}")),
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Category".to_string() } else { "New Category".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: false,
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            // No DELETE route exists, and retiring is the right model
            // anyway: a bill line already classified keeps naming it. The
            // handler is required by the shared modal's props and is never
            // reachable, because `deletable: false` renders no button.
            deletable: false,
            ondelete: move |_| {},
            create_label: "Create Category".to_string(),
            crate::components::Input {
                name: "category_name",
                label: "Name",
                placeholder: "e.g. Colocation",
                required: true,
                maxlength: 100,
                rules: vec![Rule::Required, Rule::MaxLen(100)],
                help: "Unique in your list, ignoring case.",
                error: name_err(),
                value: name.read().clone(),
                oninput: move |e: FormEvent| {
                    name_err.set(String::new());
                    name.set(e.value());
                },
            }
            crate::components::Checkbox {
                name: "category_active",
                label: "Active",
                help: "Untick to retire: it stays on every line already classified with it and is not offered on a new one. Tick again to bring it back.",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_path_asks_only_for_a_page() {
        assert_eq!(list_path(1), "/expense-categories?page=1&per_page=25");
        assert_eq!(list_path(2), "/expense-categories?page=2&per_page=25");
    }

    /// The name is trimmed and the active flag goes as a boolean.
    #[test]
    fn the_body_matches_the_upsert_request() {
        let body = CategoryFormState {
            id: None,
            name: "  Colocation  ".into(),
            is_active: false,
        }
        .body();
        assert_eq!(body["name"], "Colocation");
        assert_eq!(body["is_active"], false);
        assert_eq!(
            body.as_object().map(|o| o.len()),
            Some(2),
            "two fields, which is what UpsertExpenseCategoryRequest takes"
        );
    }

    /// Retiring is the only way out, because the server mounts no DELETE.
    ///
    /// A Delete button here would post to a route that does not exist and
    /// come back 405, which reads as a bug rather than as a decision.
    #[test]
    fn the_modal_offers_no_delete() {
        let src = include_str!("expense_categories.rs");
        let modal = &src[src.find("fn CategoryFormModal").expect("the modal")
            ..src.find("mod tests").expect("this module")];
        assert!(
            modal.contains("deletable: false"),
            "the modal withholds Delete"
        );
        assert!(
            modal.contains("ondelete: move |_| {}"),
            "and the handler the shared props require does nothing, because \
             no button can reach it"
        );
    }
}

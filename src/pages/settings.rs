//! Centralized Settings hub (MAPPS-169).
//!
//! Type-2 "configuration" surfaces - the things you edit to shape how
//! the rest of the app behaves, as opposed to individual business
//! entities - are gathered behind a single left-nav "Settings" entry.
//!
//! `SettingsHomePage` is a grouped card index. Each card links to a
//! sub-route under `/settings/*`:
//!
//!   - Re-homed surfaces (SLA, rate cards, tax rates, payment gateways)
//!     render the SAME page components as their original routes, which
//!     stay in place (MAPPS-169 chose "keep old links + add Settings").
//!   - Net-new "standard type" editors (work types, task statuses, asset
//!     types) live here. They wire to the existing server CRUD endpoints
//!     (`/work-types`, `/task-statuses`, `/asset-types`) - no server
//!     change was needed.
//!
//! The ticket statuses/types/priorities and project-type editors the
//! issue also mentions are intentionally NOT built here: the server
//! exposes those as read-only (no POST/PUT/DELETE) or as hardcoded
//! VARCHAR enums, so a management editor is blocked on backend work.
//! Tracked as follow-ups.
//!
//! CRUD conventions mirror `src/pages/billing.rs` (`TaxRateListPage`):
//! page-local `Deserialize` row structs, `active_tenant_generation()`
//! read inside each `use_resource` so an org switch re-fetches, and a
//! single create/edit modal driven by an `Option<FormState>` signal.

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow,
};
use crate::utils::Paginated;
use crate::Route;

/// Rows per page for the settings list views.
const PER_PAGE: usize = 25;

/// True when the signed-in user is an admin/super_admin. The Settings
/// surfaces configure tenant-wide behavior, so they match the same gate
/// the sidebar uses to show the Settings entry. Server endpoints re-check,
/// so this is a UX affordance, not a security boundary.
fn use_is_admin() -> bool {
    let auth = crate::hooks::use_auth();
    auth.read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false)
}

/// Shown in place of a settings page when a non-admin lands on one.
#[component]
fn AdminOnlyNotice(title: String) -> Element {
    rsx! {
        AppLayout { title: title.clone(),
            PageHeader { title, subtitle: "Settings" }
            Card {
                div { class: "text-sm text-gray-600 dark:text-gray-300",
                    "You need an administrator role to manage these settings."
                }
            }
        }
    }
}

// ============================================================================
// Settings hub
// ============================================================================

/// `/settings` - grouped index of every configuration surface.
#[component]
pub fn SettingsHomePage() -> Element {
    rsx! {
        AppLayout { title: "Settings",
            PageHeader {
                title: "Settings",
                subtitle: "Manage the standard types and configuration that shape how your workspace behaves",
            }

            SettingsGroup { heading: "Service & Asset Types",
                SettingsCard {
                    to: Route::SettingsWorkTypes {},
                    title: "Work Types",
                    description: "Billable work categories used when logging time entries.",
                }
                SettingsCard {
                    to: Route::SettingsTaskStatuses {},
                    title: "Task Statuses",
                    description: "Workflow states a project task can move through.",
                }
                SettingsCard {
                    to: Route::SettingsAssetTypes {},
                    title: "Asset Types",
                    description: "Categories for the assets you track per company.",
                }
            }

            SettingsGroup { heading: "Billing & SLA",
                SettingsCard {
                    to: Route::SettingsSla {},
                    title: "SLA Management",
                    description: "Service-level policies, business hours, and holiday calendars.",
                }
                SettingsCard {
                    to: Route::SettingsRateCards {},
                    title: "Rate Cards",
                    description: "Hourly rates billed per work type.",
                }
                SettingsCard {
                    to: Route::SettingsTaxRates {},
                    title: "Tax Rates",
                    description: "Tax rates applied to invoice line items.",
                }
                SettingsCard {
                    to: Route::SettingsGateways {},
                    title: "Payment Gateways",
                    description: "Connect and configure payment providers.",
                }
            }
        }
    }
}

#[component]
fn SettingsGroup(heading: String, children: Element) -> Element {
    rsx! {
        div { class: "mb-8",
            h2 { class: "text-sm font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400 mb-3",
                "{heading}"
            }
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3", {children} }
        }
    }
}

#[component]
fn SettingsCard(to: Route, title: String, description: String) -> Element {
    rsx! {
        Link {
            to,
            class: "block rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 hover:border-blue-400 dark:hover:border-blue-500 hover:shadow-sm transition-colors",
            div { class: "font-medium text-gray-900 dark:text-white", "{title}" }
            div { class: "mt-1 text-sm text-gray-500 dark:text-gray-400", "{description}" }
        }
    }
}

// ============================================================================
// Work types  (GET/POST `/work-types`, PUT/DELETE `/work-types/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct WorkTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default_billable: bool,
    // Server serializes `Decimal` as a JSON string, so mirror it as one.
    #[serde(default)]
    default_rate: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn WorkTypesSettingsPage() -> Element {
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Work Types" } };
    }

    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<WorkTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let path = format!("/work-types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<WorkTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Work Types",
            PageHeader {
                title: "Work Types",
                subtitle: "Billable work categories used when logging time entries",
                actions: rsx! {
                    Link { to: Route::SettingsHome {},
                        Button { variant: ButtonVariant::Secondary, "Back to Settings" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing.set(Some(WorkTypeFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Work Type"
                    }
                },
            }

            if fetch_failed {
                LoadError { what: "work types" }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 4,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Billable" }
                            TableHeader { class: "text-right", "Default Rate" }
                            TableHeader { "Active" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 4, rows: 4 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 4,
                            message: "No work types yet. Click New Work Type to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for row in rows.iter().cloned() {
                                {
                                    let key = row.id.to_string();
                                    let edit_state = WorkTypeFormState::from_existing(&row);
                                    let name = row.name.clone();
                                    let billable = row.default_billable;
                                    let active = row.is_active;
                                    let rate = row.default_rate.clone().unwrap_or_default();
                                    let rate_display = if rate.trim().is_empty() {
                                        "-".to_string()
                                    } else {
                                        format!("${rate}")
                                    };
                                    rsx! {
                                        TableRow { key: "{key}", clickable: true,
                                            onclick: move |_| editing.set(Some(edit_state.clone())),
                                            TableCell {
                                                span { class: "font-medium text-blue-600", "{name}" }
                                            }
                                            TableCell {
                                                if billable {
                                                    Badge { variant: BadgeVariant::Blue, "Billable" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Non-billable" }
                                                }
                                            }
                                            TableCell { class: "text-right", "{rate_display}" }
                                            TableCell {
                                                if active {
                                                    Badge { variant: BadgeVariant::Green, "Active" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Inactive" }
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
                WorkTypeFormModal {
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
}

#[derive(Clone, Debug, PartialEq)]
struct WorkTypeFormState {
    id: Option<String>,
    name: String,
    description: String,
    default_billable: bool,
    default_rate: String,
    is_active: bool,
    sort_order: String,
}

impl WorkTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            default_billable: true,
            default_rate: String::new(),
            is_active: true,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &WorkTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            default_billable: r.default_billable,
            default_rate: r.default_rate.clone().unwrap_or_default(),
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct WorkTypeFormModalProps {
    state: WorkTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn WorkTypeFormModal(props: WorkTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut default_billable = use_signal(|| initial.default_billable);
    let mut default_rate = use_signal(|| initial.default_rate.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let rate = default_rate.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "description": if desc.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(desc) },
            "default_billable": *default_billable.read(),
            // Server parses the rate string into `rust_decimal::Decimal`.
            "default_rate": if rate.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(rate) },
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), String> = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/work-types",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/work-types/{id}");
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save work type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this work type? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/work-types/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete work type: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Work Type" } else { "New Work Type" },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Work Type",
            crate::components::Input {
                name: "work_type_name",
                label: "Name",
                placeholder: "e.g. On-site Support",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_rate",
                label: "Default rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "e.g. 150.00",
                value: default_rate.read().clone(),
                oninput: move |e: FormEvent| default_rate.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_sort_order",
                label: "Sort order",
                r#type: "number",
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "work_type_billable",
                label: "Billable by default",
                checked: *default_billable.read(),
                onchange: move |_| {
                    let next = !*default_billable.read();
                    default_billable.set(next);
                },
            }
            crate::components::Checkbox {
                name: "work_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Task statuses  (GET/POST `/task-statuses`, PUT/DELETE `/task-statuses/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TaskStatusRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    is_completed: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TaskStatusesSettingsPage() -> Element {
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Task Statuses" } };
    }

    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TaskStatusFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let path = format!("/task-statuses?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TaskStatusRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TaskStatusRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Task Statuses",
            PageHeader {
                title: "Task Statuses",
                subtitle: "Workflow states a project task can move through",
                actions: rsx! {
                    Link { to: Route::SettingsHome {},
                        Button { variant: ButtonVariant::Secondary, "Back to Settings" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing.set(Some(TaskStatusFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Status"
                    }
                },
            }

            if fetch_failed {
                LoadError { what: "task statuses" }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 3,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Color" }
                            TableHeader { "Completed" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 3, rows: 4 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 3,
                            message: "No task statuses yet. Click New Status to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for row in rows.iter().cloned() {
                                {
                                    let key = row.id.to_string();
                                    let edit_state = TaskStatusFormState::from_existing(&row);
                                    let name = row.name.clone();
                                    let color = row.color.clone();
                                    let completed = row.is_completed;
                                    rsx! {
                                        TableRow { key: "{key}", clickable: true,
                                            onclick: move |_| editing.set(Some(edit_state.clone())),
                                            TableCell {
                                                span { class: "font-medium text-blue-600", "{name}" }
                                            }
                                            TableCell {
                                                div { class: "flex items-center gap-2",
                                                    span {
                                                        class: "inline-block h-4 w-4 rounded-full border border-gray-300 dark:border-gray-600",
                                                        style: "background-color: {color}",
                                                    }
                                                    span { class: "text-xs text-gray-500", "{color}" }
                                                }
                                            }
                                            TableCell {
                                                if completed {
                                                    Badge { variant: BadgeVariant::Green, "Completed" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Open" }
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
                TaskStatusFormModal {
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
}

#[derive(Clone, Debug, PartialEq)]
struct TaskStatusFormState {
    id: Option<String>,
    name: String,
    color: String,
    is_completed: bool,
    sort_order: String,
}

impl TaskStatusFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            color: "#6b7280".to_string(),
            is_completed: false,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TaskStatusRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            color: if r.color.trim().is_empty() {
                "#6b7280".to_string()
            } else {
                r.color.clone()
            },
            is_completed: r.is_completed,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TaskStatusFormModalProps {
    state: TaskStatusFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TaskStatusFormModal(props: TaskStatusFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut color = use_signal(|| initial.color.clone());
    let mut is_completed = use_signal(|| initial.is_completed);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        let color_val = color.read().trim().to_string();
        if color_val.is_empty() {
            error.set("Color is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "color": color_val,
            "is_completed": *is_completed.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), String> = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/task-statuses",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/task-statuses/{id}");
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save task status: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this task status? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/task-statuses/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete task status: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Task Status" } else { "New Task Status" },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Status",
            crate::components::Input {
                name: "task_status_name",
                label: "Name",
                placeholder: "e.g. In Progress",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "task_status_color",
                label: "Color",
                r#type: "color",
                value: color.read().clone(),
                oninput: move |e: FormEvent| color.set(e.value()),
            }
            crate::components::Input {
                name: "task_status_sort_order",
                label: "Sort order",
                r#type: "number",
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "task_status_completed",
                label: "Counts as completed",
                help: "Tasks in this status are treated as done.",
                checked: *is_completed.read(),
                onchange: move |_| {
                    let next = !*is_completed.read();
                    is_completed.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Asset types  (GET/POST `/asset-types`, PUT/DELETE `/asset-types/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct AssetTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    is_active: bool,
}

#[component]
pub fn AssetTypesSettingsPage() -> Element {
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Asset Types" } };
    }

    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<AssetTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let path = format!("/asset-types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<AssetTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Asset Types",
            PageHeader {
                title: "Asset Types",
                subtitle: "Categories for the assets you track per company",
                actions: rsx! {
                    Link { to: Route::SettingsHome {},
                        Button { variant: ButtonVariant::Secondary, "Back to Settings" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing.set(Some(AssetTypeFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Asset Type"
                    }
                },
            }

            if fetch_failed {
                LoadError { what: "asset types" }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 3,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Icon" }
                            TableHeader { "Active" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 3, rows: 4 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 3,
                            message: "No asset types yet. Click New Asset Type to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for row in rows.iter().cloned() {
                                {
                                    let key = row.id.to_string();
                                    let edit_state = AssetTypeFormState::from_existing(&row);
                                    let name = row.name.clone();
                                    let icon = row.icon.clone().unwrap_or_default();
                                    let active = row.is_active;
                                    rsx! {
                                        TableRow { key: "{key}", clickable: true,
                                            onclick: move |_| editing.set(Some(edit_state.clone())),
                                            TableCell {
                                                span { class: "font-medium text-blue-600", "{name}" }
                                            }
                                            TableCell {
                                                if icon.is_empty() {
                                                    span { class: "text-gray-400", "-" }
                                                } else {
                                                    span { class: "text-sm text-gray-600 dark:text-gray-300", "{icon}" }
                                                }
                                            }
                                            TableCell {
                                                if active {
                                                    Badge { variant: BadgeVariant::Green, "Active" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Inactive" }
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
                AssetTypeFormModal {
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
}

#[derive(Clone, Debug, PartialEq)]
struct AssetTypeFormState {
    id: Option<String>,
    name: String,
    icon: String,
    is_active: bool,
}

impl AssetTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            icon: String::new(),
            is_active: true,
        }
    }

    fn from_existing(r: &AssetTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            icon: r.icon.clone().unwrap_or_default(),
            is_active: r.is_active,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AssetTypeFormModalProps {
    state: AssetTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn AssetTypeFormModal(props: AssetTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut icon = use_signal(|| initial.icon.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let icon_val = icon.read().trim().to_string();
        // `parent_type_id` is always sent null: this v1 editor manages
        // flat (top-level) asset types only. Nested types are a follow-up.
        let body = serde_json::json!({
            "name": name.read().trim(),
            "icon": if icon_val.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(icon_val) },
            "parent_type_id": serde_json::Value::Null,
            "is_active": *is_active.read(),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), String> = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/asset-types",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/asset-types/{id}");
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save asset type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this asset type? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/asset-types/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete asset type: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Asset Type" } else { "New Asset Type" },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Asset Type",
            crate::components::Input {
                name: "asset_type_name",
                label: "Name",
                placeholder: "e.g. Laptop",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "asset_type_icon",
                label: "Icon",
                placeholder: "Optional icon name",
                value: icon.read().clone(),
                oninput: move |e: FormEvent| icon.set(e.value()),
            }
            crate::components::Checkbox {
                name: "asset_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Shared bits
// ============================================================================

/// Inline error banner shown above a list when its fetch fails.
#[component]
fn LoadError(what: String) -> Element {
    rsx! {
        div {
            class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
            "Could not load {what}. Refresh the page to retry."
        }
    }
}

/// Shared create/edit modal chrome for the settings editors. Owns the
/// footer (Delete / Cancel / Save) and the error banner; the per-type
/// form fields are passed as `children`.
#[derive(Props, Clone, PartialEq)]
struct SettingFormModalProps {
    title: &'static str,
    is_edit: bool,
    saving: bool,
    deleting: bool,
    error: String,
    create_label: &'static str,
    onclose: EventHandler<()>,
    onsave: EventHandler<MouseEvent>,
    ondelete: EventHandler<MouseEvent>,
    children: Element,
}

#[component]
fn SettingFormModal(props: SettingFormModalProps) -> Element {
    let onclose = props.onclose;
    let onsave = props.onsave;
    let ondelete = props.ondelete;
    let is_edit = props.is_edit;

    let footer = rsx! {
        if is_edit {
            Button {
                variant: ButtonVariant::Danger,
                loading: props.deleting,
                onclick: move |e| ondelete.call(e),
                "Delete"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: props.saving,
            onclick: move |e| onsave.call(e),
            if is_edit { "Save Changes" } else { "{props.create_label}" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: props.title.to_string(),
            size: crate::components::ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !props.error.is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{props.error}"
                    }
                }
                {props.children}
            }
        }
    }
}

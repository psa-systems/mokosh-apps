//! SLA management pages (admin-gated).
//!
//! A single admin surface with three tabs - Policies, Business Hours,
//! and Holiday Calendars - plus a per-policy Targets editor. Wired to
//! `/api/v1/sla/*` (see `mokosh-server/src/modules/sla/routes.rs`).
//!
//! All mutating endpoints require an admin token server-side
//! (`RequireAdmin`); the whole page is additionally gated on
//! `use_auth().is_admin()` (via `CurrentUser::role`), mirroring the
//! admin-route convention. List endpoints only need auth, so a
//! non-admin who reaches the route still sees a clean "not authorized"
//! card instead of a half-broken page.
//!
//! Patterns mirror `src/pages/contacts.rs`: every `use_resource`
//! closure reads `crate::hooks::fetch::active_tenant_generation()`
//! first so it re-fetches on org switch, then the access token, then a
//! typed `get_authed_typed` call. Mutations go through the `_typed`
//! helpers and surface `ApiError::user_message()` via the toast surface.

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, PageHeader, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::modules::sla::{
    BusinessHours, HolidayCalendar, SlaPolicy, SlaTarget, TicketPriorityOption,
};
use crate::utils::Paginated;

/// Rows per page for the SLA list views. SLA config sets are small, so a
/// single page is almost always enough; the pager still appears if a
/// tenant defines more than this.
const PER_PAGE: usize = 100;

/// Which SLA config surface is active. Tabs rather than separate routes:
/// the three sets are small and admins flip between them constantly when
/// wiring up a policy (policy -> its business-hours -> its targets).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SlaTab {
    Policies,
    BusinessHours,
    Holidays,
}

/// SLA management page. Admin-gated; renders the active tab's surface.
#[component]
pub fn SlaManagementPage() -> Element {
    let auth = crate::hooks::use_auth();
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false);

    let mut tab = use_signal(|| SlaTab::Policies);

    if !is_admin {
        return rsx! {
            AppLayout { title: "SLA Management",
                PageHeader { title: "SLA Management" }
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-gray-600 dark:text-gray-300",
                            "You do not have permission to manage SLA policies. Ask an administrator for access."
                        }
                    }
                }
            }
        };
    }

    let current = *tab.read();

    rsx! {
        AppLayout { title: "SLA Management",
            PageHeader {
                title: "SLA Management",
                subtitle: "Define service-level policies, business hours, and holiday calendars",
            }

            // Tab bar
            div { class: "mb-6 border-b border-gray-200 dark:border-gray-700",
                nav { class: "-mb-px flex space-x-6",
                    SlaTabButton {
                        label: "Policies",
                        active: current == SlaTab::Policies,
                        onclick: move |_| tab.set(SlaTab::Policies),
                    }
                    SlaTabButton {
                        label: "Business Hours",
                        active: current == SlaTab::BusinessHours,
                        onclick: move |_| tab.set(SlaTab::BusinessHours),
                    }
                    SlaTabButton {
                        label: "Holiday Calendars",
                        active: current == SlaTab::Holidays,
                        onclick: move |_| tab.set(SlaTab::Holidays),
                    }
                }
            }

            match current {
                SlaTab::Policies => rsx! { SlaPoliciesTab {} },
                SlaTab::BusinessHours => rsx! { BusinessHoursTab {} },
                SlaTab::Holidays => rsx! { HolidayCalendarsTab {} },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SlaTabButtonProps {
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn SlaTabButton(props: SlaTabButtonProps) -> Element {
    let class = if props.active {
        "whitespace-nowrap border-b-2 border-blue-500 px-1 py-3 text-sm font-medium text-blue-600 dark:text-blue-400"
    } else {
        "whitespace-nowrap border-b-2 border-transparent px-1 py-3 text-sm font-medium text-gray-500 hover:border-gray-300 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
    };
    rsx! {
        button {
            r#type: "button",
            class: "{class}",
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}

// =========================================================================
// Policies tab
// =========================================================================

#[component]
fn SlaPoliciesTab() -> Element {
    let mut policies = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Paginated<SlaPolicy>>(&format!(
            "/sla/policies?page=1&per_page={PER_PAGE}"
        ))
        .await
        .ok()
    });

    // Editing state: Some(None-id) => create, Some(Some(id)) => edit.
    let mut editing = use_signal(|| None::<PolicyFormState>);
    // Targets editor: Some((policy_id, policy_name)) => open.
    let mut targets_for = use_signal(|| None::<(String, String)>);

    let snap = policies.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<SlaPolicy> = match &*snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let total = match &*snap {
        Some(Some(resp)) => resp.meta.total as usize,
        _ => 0,
    };

    rsx! {
        div { class: "mb-4 flex justify-end",
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| editing.set(Some(PolicyFormState::new())),
                "New Policy"
            }
        }

        if fetch_failed {
            div {
                class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                "Could not load SLA policies. Refresh the page to retry."
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total,
            current_page: 1,
            per_page: PER_PAGE,
            columns: 4,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Description" }
                        TableHeader { "Default" }
                        TableHeader { "Actions" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 4 }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 4,
                        message: "No SLA policies yet. Click New Policy to create one.".to_string(),
                    }
                } else {
                    TableBody {
                        for policy in rows.into_iter() {
                            {
                                let key = policy.id.to_string();
                                let policy_for_edit = policy.clone();
                                let policy_id = policy.id.to_string();
                                let policy_name = policy.name.clone();
                                let desc = policy.description.clone().unwrap_or_default();
                                let is_default = policy.is_default;
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium text-gray-900 dark:text-white", "{policy.name}" }
                                        }
                                        TableCell { class: "text-gray-500", "{desc}" }
                                        TableCell {
                                            if is_default {
                                                Badge { variant: BadgeVariant::Green, "Default" }
                                            }
                                        }
                                        TableCell {
                                            div { class: "flex space-x-3",
                                                button {
                                                    r#type: "button",
                                                    class: "text-sm text-blue-600 hover:text-blue-500",
                                                    onclick: move |_| editing.set(Some(PolicyFormState::from_existing(&policy_for_edit))),
                                                    "Edit"
                                                }
                                                button {
                                                    r#type: "button",
                                                    class: "text-sm text-blue-600 hover:text-blue-500",
                                                    onclick: move |_| targets_for.set(Some((policy_id.clone(), policy_name.clone()))),
                                                    "Targets"
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
        }

        if let Some(state) = editing.read().clone() {
            PolicyFormModal {
                state,
                onclose: move |_| { editing.set(None); },
                onsaved: move |_| {
                    editing.set(None);
                    policies.restart();
                },
            }
        }

        if let Some((policy_id, policy_name)) = targets_for.read().clone() {
            TargetsModal {
                policy_id,
                policy_name,
                onclose: move |_| { targets_for.set(None); },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyFormState {
    /// Some => edit existing policy.
    id: Option<String>,
    name: String,
    description: String,
    business_hours_id: String,
    is_default: bool,
}

impl PolicyFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            business_hours_id: String::new(),
            is_default: false,
        }
    }

    fn from_existing(p: &SlaPolicy) -> Self {
        Self {
            id: Some(p.id.to_string()),
            name: p.name.clone(),
            description: p.description.clone().unwrap_or_default(),
            business_hours_id: p
                .business_hours_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            is_default: p.is_default,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PolicyFormModalProps {
    state: PolicyFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn PolicyFormModal(props: PolicyFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let modal_title = if is_edit {
        "Edit SLA Policy"
    } else {
        "New SLA Policy"
    };

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut business_hours_id = use_signal(|| initial.business_hours_id.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Business-hours options for the dropdown. Loaded lazily; the
    // policy still saves with no business hours selected (the field is
    // optional server-side).
    let bh_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Paginated<BusinessHours>>(&format!(
            "/sla/business-hours?page=1&per_page={PER_PAGE}"
        ))
        .await
        .ok()
    });
    let bh_options: Vec<(String, String)> = match &*bh_resource.read_unchecked() {
        Some(Some(resp)) => resp
            .data
            .iter()
            .map(|b| (b.id.to_string(), b.name.clone()))
            .collect(),
        _ => Vec::new(),
    };

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Policy name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        let req = crate::modules::sla::UpsertSlaPolicyRequest {
            name: name.read().trim().to_string(),
            description: optional_trim(&description.read()),
            business_hours_id: uuid::Uuid::parse_str(business_hours_id.read().trim()).ok(),
            is_default: *is_default.read(),
        };
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed_typed::<SlaPolicy, _>(
                        "/sla/policies",
                        &req,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/sla/policies/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<SlaPolicy, _>(&path, &req)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "SLA policy saved.",
                        );
                        onsaved.call(());
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    };

    let footer = rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create Policy" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: modal_title,
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                crate::components::Input {
                    name: "policy_name",
                    label: "Name",
                    placeholder: "e.g. Standard SLA",
                    required: true,
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
                crate::components::Textarea {
                    name: "policy_description",
                    label: "Description",
                    rows: 3,
                    value: description.read().clone(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
                crate::components::Select {
                    name: "policy_business_hours",
                    label: "Business Hours",
                    placeholder: "None (24x7)",
                    options: {
                        let mut opts = vec![crate::components::SelectOption::new("", "None (24x7)")];
                        for (id, label) in bh_options.iter() {
                            opts.push(crate::components::SelectOption::new(id.clone(), label.clone()));
                        }
                        opts
                    },
                    value: business_hours_id.read().clone(),
                    onchange: move |e: FormEvent| business_hours_id.set(e.value()),
                }
                crate::components::Checkbox {
                    name: "policy_is_default",
                    label: "Default policy",
                    checked: *is_default.read(),
                    help: "Applied to tickets when no other policy matches.",
                    onchange: move |_| {
                        let next = !*is_default.read();
                        is_default.set(next);
                    },
                }
            }
        }
    }
}

// =========================================================================
// Targets editor (per policy, per priority)
// =========================================================================

#[derive(Props, Clone, PartialEq)]
struct TargetsModalProps {
    policy_id: String,
    policy_name: String,
    onclose: EventHandler<()>,
}

#[component]
fn TargetsModal(props: TargetsModalProps) -> Element {
    let policy_id = props.policy_id.clone();
    let onclose = props.onclose;

    // Existing targets for this policy.
    let pid_for_targets = policy_id.clone();
    let mut targets = use_resource(move || {
        let pid = pid_for_targets.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            #[cfg(feature = "web")]
            {
                let path = format!("/sla/policies/{pid}/targets?page=1&per_page={PER_PAGE}");
                crate::hooks::fetch::api::get_authed_typed::<Paginated<SlaTarget>>(&path)
                    .await
                    .ok()
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = pid;
                None::<Paginated<SlaTarget>>
            }
        }
    });

    // The tenant's priority list - one editor row per priority.
    let priorities = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed_typed::<Paginated<TicketPriorityOption>>(&format!(
                "/tickets/priorities?page=1&per_page={PER_PAGE}"
            ))
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<Paginated<TicketPriorityOption>>
        }
    });

    let targets_snap = targets.read_unchecked();
    let priorities_snap = priorities.read_unchecked();

    let priority_rows: Vec<TicketPriorityOption> = match &*priorities_snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let existing: Vec<SlaTarget> = match &*targets_snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let loading = targets_snap.is_none() || priorities_snap.is_none();
    let load_failed = matches!(*targets_snap, Some(None)) || matches!(*priorities_snap, Some(None));

    let footer = rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Close"
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: "SLA Targets",
            size: crate::components::ModalSize::XLarge,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                p { class: "text-sm text-gray-500 dark:text-gray-400",
                    "Targets for policy "
                    span { class: "font-medium text-gray-900 dark:text-white", "{props.policy_name}" }
                    ". Set first-response and resolution hours per priority, then Save the row."
                }

                if load_failed {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "Could not load targets or priorities. Close and retry."
                    }
                }

                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Priority" }
                            TableHeader { "First Response (hrs)" }
                            TableHeader { "Resolution (hrs)" }
                            TableHeader { "Operational Hours" }
                            TableHeader { "Actions" }
                        }
                    }
                    if loading {
                        TableLoading { columns: 5, rows: 4 }
                    } else if priority_rows.is_empty() {
                        TableEmpty {
                            columns: 5,
                            message: "No ticket priorities defined. Create priorities first.".to_string(),
                        }
                    } else {
                        TableBody {
                            for priority in priority_rows.into_iter() {
                                {
                                    let existing_target = existing
                                        .iter()
                                        .find(|t| t.priority_id == priority.id)
                                        .cloned();
                                    let key = priority.id.to_string();
                                    rsx! {
                                        TargetRow {
                                            key: "{key}",
                                            policy_id: policy_id.clone(),
                                            priority_id: priority.id.to_string(),
                                            priority_name: priority.name.clone(),
                                            existing: existing_target,
                                            onsaved: move |_| { targets.restart(); },
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
}

#[derive(Props, Clone, PartialEq)]
struct TargetRowProps {
    policy_id: String,
    priority_id: String,
    priority_name: String,
    existing: Option<SlaTarget>,
    onsaved: EventHandler<()>,
}

#[component]
fn TargetRow(props: TargetRowProps) -> Element {
    let existing = props.existing.clone();
    let init_first = existing
        .as_ref()
        .and_then(|t| t.first_response_hours)
        .map(|d| d.to_string())
        .unwrap_or_default();
    let init_resolution = existing
        .as_ref()
        .and_then(|t| t.resolution_hours)
        .map(|d| d.to_string())
        .unwrap_or_default();
    let init_ops = existing
        .as_ref()
        .map(|t| t.operational_hours.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "24x7".to_string());
    let target_id = existing.as_ref().map(|t| t.id.to_string());

    let mut first_hours = use_signal(|| init_first.clone());
    let mut resolution_hours = use_signal(|| init_resolution.clone());
    let mut operational_hours = use_signal(|| init_ops.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);

    let onsaved = props.onsaved;
    let policy_id = props.policy_id.clone();
    let priority_id = props.priority_id.clone();

    let save_policy_id = policy_id.clone();
    let save_priority_id = priority_id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        // Parse the priority UUID; it always comes from the server list so
        // this should never fail, but guard rather than unwrap.
        let Ok(priority_uuid) = uuid::Uuid::parse_str(&save_priority_id) else {
            crate::hooks::push_toast(
                crate::components::AlertType::Error,
                "Invalid priority id; reload and retry.",
            );
            return;
        };
        let first = parse_hours(&first_hours.read());
        let resolution = parse_hours(&resolution_hours.read());
        let ops = {
            let v = operational_hours.read().clone();
            if v.is_empty() {
                "24x7".to_string()
            } else {
                v
            }
        };
        saving.set(true);
        let req = crate::modules::sla::UpsertSlaTargetRequest {
            priority_id: priority_uuid,
            first_response_hours: first,
            resolution_hours: resolution,
            operational_hours: ops,
        };
        let pid = save_policy_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                // Upsert: the server keys on (policy, priority) so POST
                // both creates and updates a target for this priority.
                let path = format!("/sla/policies/{pid}/targets");
                match crate::hooks::fetch::api::post_authed_typed::<SlaTarget, _>(&path, &req).await
                {
                    Ok(_) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Target saved.",
                        );
                        onsaved.call(());
                    }
                    Err(err) => crate::hooks::push_api_error(&err),
                }
            }
            saving.set(false);
        });
    };

    let delete_target_id = target_id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_target_id.clone() else {
            return;
        };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| w.confirm_with_message("Remove this SLA target?").ok())
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/sla/targets/{id}");
                    match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                        Ok(()) => {
                            crate::hooks::push_toast(
                                crate::components::AlertType::Success,
                                "Target removed.",
                            );
                            onsaved.call(());
                        }
                        Err(err) => crate::hooks::push_api_error(&err),
                    }
                }
            }
            deleting.set(false);
        });
    };

    let has_target = target_id.is_some();

    rsx! {
        TableRow {
            TableCell {
                span { class: "font-medium text-gray-900 dark:text-white", "{props.priority_name}" }
            }
            TableCell {
                crate::components::Input {
                    name: "first_{props.priority_id}",
                    // Free-text so H:MM (e.g. "1:30") and fractional hours
                    // can be entered; type="number" blocks both. PMS-340.
                    r#type: "text",
                    placeholder: "1.5 or 1:30",
                    value: first_hours.read().clone(),
                    oninput: move |e: FormEvent| first_hours.set(e.value()),
                }
            }
            TableCell {
                crate::components::Input {
                    name: "resolution_{props.priority_id}",
                    // Free-text for H:MM / fractional hours (PMS-340).
                    r#type: "text",
                    placeholder: "8 or 8:30",
                    value: resolution_hours.read().clone(),
                    oninput: move |e: FormEvent| resolution_hours.set(e.value()),
                }
            }
            TableCell {
                crate::components::Select {
                    name: "ops_{props.priority_id}",
                    options: vec![
                        crate::components::SelectOption::new("24x7", "24x7"),
                        crate::components::SelectOption::new("business_hours", "Business Hours"),
                    ],
                    value: operational_hours.read().clone(),
                    onchange: move |e: FormEvent| operational_hours.set(e.value()),
                }
            }
            TableCell {
                div { class: "flex space-x-3",
                    Button {
                        variant: ButtonVariant::Primary,
                        size: crate::components::ButtonSize::Small,
                        loading: *saving.read(),
                        onclick: handle_save,
                        "Save"
                    }
                    if has_target {
                        Button {
                            variant: ButtonVariant::Danger,
                            size: crate::components::ButtonSize::Small,
                            loading: *deleting.read(),
                            onclick: handle_delete,
                            "Remove"
                        }
                    }
                }
            }
        }
    }
}

// =========================================================================
// Business hours tab
// =========================================================================

#[component]
fn BusinessHoursTab() -> Element {
    let mut hours = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            let _token = crate::hooks::fetch::api::current_access_token()?;
            crate::hooks::fetch::api::get_authed_typed::<Paginated<BusinessHours>>(&format!(
                "/sla/business-hours?page=1&per_page={PER_PAGE}"
            ))
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<Paginated<BusinessHours>>
        }
    });

    let mut editing = use_signal(|| None::<BusinessHoursFormState>);

    let snap = hours.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<BusinessHours> = match &*snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let total = match &*snap {
        Some(Some(resp)) => resp.meta.total as usize,
        _ => 0,
    };

    rsx! {
        div { class: "mb-4 flex justify-end",
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| editing.set(Some(BusinessHoursFormState::new())),
                "New Business Hours"
            }
        }

        if fetch_failed {
            div {
                class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                "Could not load business hours. Refresh the page to retry."
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total,
            current_page: 1,
            per_page: PER_PAGE,
            columns: 4,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Timezone" }
                        TableHeader { "Default" }
                        TableHeader { "Actions" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 3 }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 4,
                        message: "No business-hours sets yet. Click New Business Hours to create one.".to_string(),
                    }
                } else {
                    TableBody {
                        for bh in rows.into_iter() {
                            {
                                let key = bh.id.to_string();
                                let bh_for_edit = bh.clone();
                                let tz = bh.timezone.clone();
                                let is_default = bh.is_default;
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium text-gray-900 dark:text-white", "{bh.name}" }
                                        }
                                        TableCell { class: "text-gray-500", "{tz}" }
                                        TableCell {
                                            if is_default {
                                                Badge { variant: BadgeVariant::Green, "Default" }
                                            }
                                        }
                                        TableCell {
                                            button {
                                                r#type: "button",
                                                class: "text-sm text-blue-600 hover:text-blue-500",
                                                onclick: move |_| editing.set(Some(BusinessHoursFormState::from_existing(&bh_for_edit))),
                                                "Edit"
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
            BusinessHoursFormModal {
                state,
                onclose: move |_| { editing.set(None); },
                onsaved: move |_| {
                    editing.set(None);
                    hours.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BusinessHoursFormState {
    id: Option<String>,
    name: String,
    timezone: String,
    schedule_json: String,
    is_default: bool,
}

impl BusinessHoursFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            timezone: "UTC".to_string(),
            schedule_json: DEFAULT_SCHEDULE_JSON.to_string(),
            is_default: false,
        }
    }

    fn from_existing(b: &BusinessHours) -> Self {
        Self {
            id: Some(b.id.to_string()),
            name: b.name.clone(),
            timezone: if b.timezone.is_empty() {
                "UTC".to_string()
            } else {
                b.timezone.clone()
            },
            schedule_json: pretty_json(&b.schedule),
            is_default: b.is_default,
        }
    }
}

/// Seed shown in the schedule editor for a new set. Matches the per-day
/// window shape the server documents (`{"mon": [{"start", "end"}], ...}`).
const DEFAULT_SCHEDULE_JSON: &str = r#"{
  "mon": [{"start": "09:00", "end": "17:00"}],
  "tue": [{"start": "09:00", "end": "17:00"}],
  "wed": [{"start": "09:00", "end": "17:00"}],
  "thu": [{"start": "09:00", "end": "17:00"}],
  "fri": [{"start": "09:00", "end": "17:00"}]
}"#;

#[derive(Props, Clone, PartialEq)]
struct BusinessHoursFormModalProps {
    state: BusinessHoursFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn BusinessHoursFormModal(props: BusinessHoursFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let modal_title = if is_edit {
        "Edit Business Hours"
    } else {
        "New Business Hours"
    };

    let mut name = use_signal(|| initial.name.clone());
    let mut timezone = use_signal(|| initial.timezone.clone());
    let mut schedule_json = use_signal(|| initial.schedule_json.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        // Parse the schedule blob before sending so a typo surfaces here
        // rather than as an opaque 422 from the server.
        let schedule = match serde_json::from_str::<serde_json::Value>(schedule_json.read().trim())
        {
            Ok(v) => v,
            Err(e) => {
                error.set(format!("Schedule is not valid JSON: {e}"));
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        let req = crate::modules::sla::UpsertBusinessHoursRequest {
            name: name.read().trim().to_string(),
            timezone: {
                let tz = timezone.read().trim().to_string();
                if tz.is_empty() {
                    "UTC".to_string()
                } else {
                    tz
                }
            },
            schedule,
            is_default: *is_default.read(),
        };
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed_typed::<BusinessHours, _>(
                        "/sla/business-hours",
                        &req,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/sla/business-hours/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<BusinessHours, _>(&path, &req)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Business hours saved.",
                        );
                        onsaved.call(());
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    };

    let footer = rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: modal_title,
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "bh_name",
                        label: "Name",
                        placeholder: "e.g. Weekday 9-5",
                        required: true,
                        value: name.read().clone(),
                        oninput: move |e: FormEvent| name.set(e.value()),
                    }
                    crate::components::Input {
                        name: "bh_timezone",
                        label: "Timezone",
                        placeholder: "e.g. America/New_York",
                        value: timezone.read().clone(),
                        oninput: move |e: FormEvent| timezone.set(e.value()),
                    }
                }
                crate::components::Textarea {
                    name: "bh_schedule",
                    label: "Schedule (JSON)",
                    rows: 10,
                    help: "Per-day windows, e.g. {{\"mon\": [{{\"start\": \"09:00\", \"end\": \"17:00\"}}]}}.",
                    value: schedule_json.read().clone(),
                    oninput: move |e: FormEvent| schedule_json.set(e.value()),
                }
                crate::components::Checkbox {
                    name: "bh_is_default",
                    label: "Default business hours",
                    checked: *is_default.read(),
                    onchange: move |_| {
                        let next = !*is_default.read();
                        is_default.set(next);
                    },
                }
            }
        }
    }
}

// =========================================================================
// Holiday calendars tab
// =========================================================================

#[component]
fn HolidayCalendarsTab() -> Element {
    let mut calendars = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            let _token = crate::hooks::fetch::api::current_access_token()?;
            crate::hooks::fetch::api::get_authed_typed::<Paginated<HolidayCalendar>>(&format!(
                "/sla/holiday-calendars?page=1&per_page={PER_PAGE}"
            ))
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<Paginated<HolidayCalendar>>
        }
    });

    let mut editing = use_signal(|| None::<HolidayFormState>);

    let snap = calendars.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<HolidayCalendar> = match &*snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };
    let total = match &*snap {
        Some(Some(resp)) => resp.meta.total as usize,
        _ => 0,
    };

    rsx! {
        div { class: "mb-4 flex justify-end",
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| editing.set(Some(HolidayFormState::new())),
                "New Holiday Calendar"
            }
        }

        if fetch_failed {
            div {
                class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                "Could not load holiday calendars. Refresh the page to retry."
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total,
            current_page: 1,
            per_page: PER_PAGE,
            columns: 3,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Holidays" }
                        TableHeader { "Actions" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 3 }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 3,
                        message: "No holiday calendars yet. Click New Holiday Calendar to create one.".to_string(),
                    }
                } else {
                    TableBody {
                        for cal in rows.into_iter() {
                            {
                                let key = cal.id.to_string();
                                let cal_for_edit = cal.clone();
                                let count = holiday_count(&cal.holidays);
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium text-gray-900 dark:text-white", "{cal.name}" }
                                        }
                                        TableCell { class: "text-gray-500", "{count} dates" }
                                        TableCell {
                                            button {
                                                r#type: "button",
                                                class: "text-sm text-blue-600 hover:text-blue-500",
                                                onclick: move |_| editing.set(Some(HolidayFormState::from_existing(&cal_for_edit))),
                                                "Edit"
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
            HolidayFormModal {
                state,
                onclose: move |_| { editing.set(None); },
                onsaved: move |_| {
                    editing.set(None);
                    calendars.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct HolidayFormState {
    id: Option<String>,
    name: String,
    holidays_json: String,
}

impl HolidayFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            holidays_json: DEFAULT_HOLIDAYS_JSON.to_string(),
        }
    }

    fn from_existing(c: &HolidayCalendar) -> Self {
        Self {
            id: Some(c.id.to_string()),
            name: c.name.clone(),
            holidays_json: pretty_json(&c.holidays),
        }
    }
}

/// Seed shown in the holidays editor for a new calendar. Matches the
/// `{date, name}` entry shape the server documents.
const DEFAULT_HOLIDAYS_JSON: &str = r#"[
  {"date": "2026-01-01", "name": "New Year's Day"}
]"#;

#[derive(Props, Clone, PartialEq)]
struct HolidayFormModalProps {
    state: HolidayFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn HolidayFormModal(props: HolidayFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let modal_title = if is_edit {
        "Edit Holiday Calendar"
    } else {
        "New Holiday Calendar"
    };

    let mut name = use_signal(|| initial.name.clone());
    let mut holidays_json = use_signal(|| initial.holidays_json.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        let holidays = match serde_json::from_str::<serde_json::Value>(holidays_json.read().trim())
        {
            Ok(v) => v,
            Err(e) => {
                error.set(format!("Holidays is not valid JSON: {e}"));
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        let req = crate::modules::sla::UpsertHolidayCalendarRequest {
            name: name.read().trim().to_string(),
            holidays,
        };
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed_typed::<HolidayCalendar, _>(
                        "/sla/holiday-calendars",
                        &req,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/sla/holiday-calendars/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<HolidayCalendar, _>(
                            &path, &req,
                        )
                        .await
                        .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Holiday calendar saved.",
                        );
                        onsaved.call(());
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    };

    let footer = rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: modal_title,
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                crate::components::Input {
                    name: "hol_name",
                    label: "Name",
                    placeholder: "e.g. US Federal Holidays",
                    required: true,
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
                crate::components::Textarea {
                    name: "hol_holidays",
                    label: "Holidays (JSON)",
                    rows: 10,
                    help: "List of {{date, name}} entries, e.g. [{{\"date\": \"2026-01-01\", \"name\": \"New Year's Day\"}}].",
                    value: holidays_json.read().clone(),
                    oninput: move |e: FormEvent| holidays_json.set(e.value()),
                }
            }
        }
    }
}

// =========================================================================
// Helpers
// =========================================================================

/// Trim and map empty to `None` for optional string request fields.
fn optional_trim(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse an hours input into an optional `Decimal`. Empty or unparseable
/// input becomes `None` (server treats a missing target hour as "no
/// commitment for this dimension").
fn parse_hours(value: &str) -> Option<rust_decimal::Decimal> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Accept H:MM as well as a plain decimal (PMS-340). For H:MM, reuse the
    // shared minute parser (so validation matches the Log Time field) and
    // convert to exact Decimal hours; a plain decimal keeps its exact value.
    if trimmed.contains(':') {
        let mins = crate::utils::duration::parse_input_to_minutes(trimmed)?;
        return Some(rust_decimal::Decimal::from(mins) / rust_decimal::Decimal::from(60));
    }
    trimmed.parse::<rust_decimal::Decimal>().ok()
}

/// Pretty-print a JSON value for an editor textarea. Falls back to the
/// compact form if pretty-printing somehow fails.
fn pretty_json(value: &serde_json::Value) -> String {
    if value.is_null() {
        return String::new();
    }
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Count entries in a holidays JSON blob for the list summary. Non-array
/// shapes report 0 so a malformed value never panics the row.
fn holiday_count(value: &serde_json::Value) -> usize {
    value.as_array().map(|a| a.len()).unwrap_or(0)
}

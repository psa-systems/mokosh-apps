//! Time tracking pages

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, Checkbox, ChevronRightIcon,
    DataTable, IconSize, Modal, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::utils::Paginated;
use crate::Route;

/// A time entry (`GET /api/v1/time-entries`). The work-item names (ticket
/// number/title, project name, task title) are joined server-side (PMS-332),
/// so the list shows real names via `work_item_label` rather than bare ids.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteTimeEntry {
    id: uuid::Uuid,
    date: NaiveDate,
    #[serde(default)]
    duration_minutes: i64,
    #[serde(default)]
    work_type_id: Option<uuid::Uuid>,
    #[serde(default)]
    ticket_id: Option<uuid::Uuid>,
    #[serde(default)]
    project_id: Option<uuid::Uuid>,
    // Work-item names joined server-side (PMS-332) so the list can show the
    // ticket/project and task name instead of a bare "Ticket"/"Project".
    #[serde(default)]
    ticket_number: Option<String>,
    #[serde(default)]
    ticket_title: Option<String>,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    task_title: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_billable: bool,
    #[serde(default)]
    billing_status: String,
}

/// A ticket, used to populate the Work Item picker. Selecting one supplies
/// both `ticket_id` and the required `company_id` for the create request.
#[derive(Clone, Debug, Deserialize)]
struct TicketOption {
    id: uuid::Uuid,
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    company_id: uuid::Uuid,
}

/// A work type (`GET /api/v1/work-types`) for the required `work_type_id`.
#[derive(Clone, Debug, Deserialize)]
struct WorkTypeOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// A project (`GET /api/v1/projects`) offered as a work item. Only
/// projects with a `company_id` are pickable, since a time entry needs a
/// company and a project's is optional.
#[derive(Clone, Debug, Deserialize)]
struct ProjectPick {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
}

/// A task within the selected project (`GET /api/v1/projects/:id/tasks`).
#[derive(Clone, Debug, Deserialize)]
struct TaskPick {
    id: uuid::Uuid,
    #[serde(default)]
    title: String,
}

/// A project (`GET /api/v1/projects`), used to label project-linked time in
/// the timesheet grid.
#[derive(Clone, Debug, Deserialize)]
struct ProjectOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// A week summary (`GET /api/v1/timesheets`). The endpoint aggregates a
/// `(user, week)` over `time_entries`; the grid pivots the raw entries
/// itself, so only the week-level `approval_status` is consumed here.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTimesheet {
    #[serde(default)]
    approval_status: String,
}

/// Start of the Monday-Sunday week that contains `date`.
fn monday_of_week(date: NaiveDate) -> NaiveDate {
    let offset = match date.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    date - Duration::days(offset)
}

/// Human label for a time entry's work item: "Ticket {n}: {title}" or
/// "{project} · {task}" using the names joined server-side (PMS-332). Falls
/// back to the bare kind when a name is missing, or "-" when unlinked.
fn work_item_label(e: &RemoteTimeEntry) -> String {
    if e.ticket_id.is_some() {
        match (e.ticket_number.as_deref(), e.ticket_title.as_deref()) {
            (Some(n), Some(t)) => format!("Ticket {n}: {t}"),
            (Some(n), None) => format!("Ticket {n}"),
            _ => "Ticket".to_string(),
        }
    } else if e.project_id.is_some() {
        let p = e
            .project_name
            .clone()
            .unwrap_or_else(|| "Project".to_string());
        match e.task_title.as_deref() {
            Some(t) if !t.is_empty() => format!("{p} · {t}"),
            _ => p,
        }
    } else {
        "-".to_string()
    }
}

/// Time entry list page
#[component]
pub fn TimeEntryListPage() -> Element {
    let mut entries_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>("/time-entries")
            .await
            .ok()
            .map(|p| p.data)
    });
    // MAPPS-166: click-to-edit a time entry via the modal below.
    let mut selected_entry = use_signal(|| None::<RemoteTimeEntry>);

    // MAPPS-202: resolve each entry's `work_type_id` to the work type's human
    // name for display, so the list shows e.g. "On-site Support" rather than a
    // bare UUID. Mirrors how the timesheet resolves ticket/project names
    // client-side from a fetched lookup list.
    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>(
            "/work-types?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let work_type_name_by_id: std::collections::HashMap<uuid::Uuid, String> = work_types_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|w| (w.id, w.name))
        .collect();

    let snapshot = entries_resource.read_unchecked().clone();
    // `None` while loading; `Some(None)` on fetch failure; `Some(Some(rows))`.
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let entries: Vec<RemoteTimeEntry> = snapshot.flatten().unwrap_or_default();

    // Stat cards computed from the fetched entries (no hardcoded totals).
    let today = Utc::now().date_naive();
    let week_start = monday_of_week(today);
    let hours = crate::utils::duration::fmt_duration;
    let today_h = hours(sum_minutes(&entries, |e| e.date == today));
    let week_h = hours(sum_minutes(&entries, |e| e.date >= week_start));
    let billable_h = hours(sum_minutes(&entries, |e| e.is_billable));
    let nonbillable_h = hours(sum_minutes(&entries, |e| !e.is_billable));
    let total = entries.len();

    rsx! {
        AppLayout { title: "Time Entries",
            PageHeader {
                title: "Time Entries",
                subtitle: "Track and manage time spent on work",
                actions: rsx! {
                    Link {
                        to: Route::TimeEntryNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Log Time"
                        }
                    }
                },
            }

            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Today" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "{today_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "This Week" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "{week_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Billable" }
                    p { class: "text-2xl font-bold text-green-600", "{billable_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Non-Billable" }
                    p { class: "text-2xl font-bold text-gray-500", "{nonbillable_h}" }
                }
            }

            if load_failed {
                Card { class: "mb-6",
                    p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                        "Could not load time entries from the server."
                    }
                }
            }

            DataTable {
                total_items: total,
                current_page: 1,
                per_page: if total == 0 { 25 } else { total },
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Work Item" }
                            TableHeader { "Work Type" }
                            TableHeader { "Description" }
                            TableHeader { "Hours" }
                            TableHeader { "Billable" }
                        }
                    }
                    TableBody {
                        if is_loading {
                            TableRow { TableCell { class: "text-gray-400", "Loading…" } }
                        } else if entries.is_empty() {
                            TableRow {
                                TableCell { class: "text-gray-400 italic", "No time logged yet." }
                            }
                        } else {
                            for e in entries.iter() {
                                {
                                    let hrs = hours(e.duration_minutes);
                                    let note = e
                                        .notes
                                        .clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| "-".to_string());
                                    let status = e.billing_status.clone();
                                    let wi_label = work_item_label(e);
                                    let wt_label = e
                                        .work_type_id
                                        .and_then(|id| work_type_name_by_id.get(&id).cloned())
                                        .unwrap_or_else(|| "-".to_string());
                                    let entry = e.clone();
                                    rsx! {
                                        TableRow {
                                            clickable: true,
                                            onclick: move |_| selected_entry.set(Some(entry.clone())),
                                            TableCell { class: "text-gray-500", "{e.date}" }
                                            TableCell {
                                                if let Some(tid) = e.ticket_id {
                                                    // Stop the link click from also opening the
                                                    // row's edit modal; let it just navigate.
                                                    span {
                                                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                        Link {
                                                            to: Route::TicketDetail { id: tid.to_string() },
                                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                                            "{wi_label}"
                                                        }
                                                    }
                                                } else if let Some(pid) = e.project_id {
                                                    // PMS-320: link the project like the ticket
                                                    // above. stop_propagation so the link
                                                    // navigates instead of opening the edit modal.
                                                    span {
                                                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                        Link {
                                                            to: Route::ProjectDetail { id: pid.to_string() },
                                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                                            "{wi_label}"
                                                        }
                                                    }
                                                } else {
                                                    span { class: "text-gray-400", "-" }
                                                }
                                            }
                                            TableCell { class: "text-gray-500", "{wt_label}" }
                                            TableCell { class: "max-w-xs truncate", "{note}" }
                                            TableCell { class: "font-medium", "{hrs}" }
                                            TableCell {
                                                if e.is_billable {
                                                    Badge { variant: BadgeVariant::Green, "Billable" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Non-Billable" }
                                                }
                                                if !status.is_empty() && status != "not_billed" {
                                                    span { class: "ml-2 text-xs text-gray-400", "{status}" }
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

            if let Some(entry) = selected_entry() {
                TimeEntryEditModal {
                    entry,
                    onclose: move |_| selected_entry.set(None),
                    onsaved: move |_| {
                        selected_entry.set(None);
                        entries_resource.restart();
                    },
                }
            }
        }
    }
}

/// Sum `duration_minutes` over the entries matching `pred`.
fn sum_minutes(entries: &[RemoteTimeEntry], pred: impl Fn(&RemoteTimeEntry) -> bool) -> i64 {
    entries
        .iter()
        .filter(|e| pred(e))
        .map(|e| e.duration_minutes)
        .sum()
}

/// Minutes rendered using the user's chosen duration format (decimal
/// "1.5h" or H:MM "1:30"); see `crate::utils::duration` (PMS-265).
fn fmt_hours(minutes: i64) -> String {
    crate::utils::duration::fmt_duration(minutes)
}

/// First 8 chars of a UUID, for a compact label when a name isn't loaded.
fn short_id(id: uuid::Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// New time entry page
#[component]
pub fn TimeEntryNewPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    let mut work_item = use_signal(String::new);
    let mut task = use_signal(String::new);
    let mut work_type = use_signal(String::new);
    let mut hours = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut is_billable = use_signal(|| true);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Work items come from two sources: tickets (supply ticket_id +
    // company_id) and projects (supply project_id + their company_id). The
    // select value is prefixed `ticket:` / `project:` to tell them apart.
    let tickets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<TicketOption>>("/tickets")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let projects_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<ProjectPick>>("/projects")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>(
            "/work-types?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    // Tasks for the selected project (only when a project work item is
    // picked); re-runs when `work_item` changes.
    let tasks_resource = use_resource(move || {
        let wi = work_item();
        async move {
            if let Some(pid) = wi.strip_prefix("project:") {
                let pid = pid.to_string();
                let _gen = crate::hooks::fetch::active_tenant_generation();
                crate::hooks::fetch::api::get_authed::<Paginated<TaskPick>>(&format!(
                    "/projects/{pid}/tasks"
                ))
                .await
                .ok()
                .map(|p| p.data)
                .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
    });

    let tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let projects = projects_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let work_types = work_types_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let tasks = tasks_resource.read_unchecked().clone().unwrap_or_default();

    let mut work_item_options = vec![SelectOption::new("", "Select a work item")];
    work_item_options.extend(tickets.iter().map(|t| {
        SelectOption::new(
            format!("ticket:{}", t.id),
            format!("Ticket {}: {}", t.ticket_number, t.title),
        )
    }));
    // Only projects with a company can take a time entry (company_id is
    // required and a project's is optional).
    work_item_options.extend(
        projects.iter().filter(|p| p.company_id.is_some()).map(|p| {
            SelectOption::new(format!("project:{}", p.id), format!("Project: {}", p.name))
        }),
    );
    let mut work_type_options = vec![SelectOption::new("", "Select a work type")];
    work_type_options.extend(
        work_types
            .iter()
            .map(|w| SelectOption::new(w.id.to_string(), w.name.clone())),
    );
    let mut task_options = vec![SelectOption::new("", "No specific task")];
    task_options.extend(
        tasks
            .iter()
            .map(|t| SelectOption::new(t.id.to_string(), t.title.clone())),
    );

    let is_project_item = work_item.read().starts_with("project:");
    let tickets_for_submit = tickets.clone();
    let projects_for_submit = projects.clone();
    let err = error.read().clone();

    rsx! {
        AppLayout { title: "Log Time",
            PageHeader { title: "Log Time", subtitle: "Record time spent on work" }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        error.set(String::new());
                        let wi = work_item.read().clone();
                        let wtid = work_type.read().clone();
                        let hrs = hours.read().clone();
                        let desc = description.read().clone();
                        let billable = *is_billable.read();

                        if wtid.is_empty() {
                            error.set("Please pick a work type.".to_string());
                            return;
                        }
                        // The Hours field is free-text (it accepts H:MM as
                        // well as decimal), so the submit path owns all
                        // validation: parse either shape into whole minutes
                        // and require 0 < t <= 24h.
                        let duration_minutes = match crate::utils::duration::parse_input_to_minutes(
                            &hrs,
                        ) {
                            Some(m) if m > 0 && m <= 24 * 60 => m,
                            _ => {
                                error.set(
                                    "Enter time as hours (2.5) or H:MM (1:30), greater than 0 and at most 24h."
                                        .to_string(),
                                );
                                return;
                            }
                        };
                        // Resolve the work item into (ticket_id, project_id,
                        // task_id, company_id). A ticket carries its company;
                        // a project carries its own (required, which is why the
                        // picker only lists projects that have one).
                        let (ticket_id, project_id, task_id, company_id) = if let Some(tid) =
                            wi.strip_prefix("ticket:")
                        {
                            match tickets_for_submit.iter().find(|t| t.id.to_string() == tid) {
                                Some(t) => (Some(tid.to_string()), None, None, t.company_id),
                                None => {
                                    error.set("Could not resolve the ticket.".to_string());
                                    return;
                                }
                            }
                        } else if let Some(pid) = wi.strip_prefix("project:") {
                            match projects_for_submit.iter().find(|p| p.id.to_string() == pid) {
                                Some(p) => match p.company_id {
                                    Some(cid) => {
                                        let tk = task.read().clone();
                                        let tk = if tk.is_empty() { None } else { Some(tk) };
                                        (None, Some(pid.to_string()), tk, cid)
                                    }
                                    None => {
                                        error.set(
                                            "That project has no company; pick a ticket or a project with a company."
                                                .to_string(),
                                        );
                                        return;
                                    }
                                },
                                None => {
                                    error.set("Could not resolve the project.".to_string());
                                    return;
                                }
                            }
                        } else {
                            error.set("Please pick a work item.".to_string());
                            return;
                        };
                        let user_id = match auth.read().user.as_ref().map(|u| u.id) {
                            Some(id) => id,
                            None => {
                                error.set("Not signed in.".to_string());
                                return;
                            }
                        };
                        let date = Utc::now().date_naive().to_string();

                        is_submitting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let mut body = serde_json::json!({
                                    "user_id": user_id,
                                    "date": date,
                                    "duration_minutes": duration_minutes,
                                    "work_type_id": wtid,
                                    "company_id": company_id,
                                    "notes": desc,
                                    "is_billable": billable,
                                });
                                if let Some(t) = ticket_id {
                                    body["ticket_id"] = serde_json::json!(t);
                                }
                                if let Some(p) = project_id {
                                    body["project_id"] = serde_json::json!(p);
                                }
                                if let Some(tk) = task_id {
                                    body["task_id"] = serde_json::json!(tk);
                                }
                                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                    "/time-entries",
                                    &body,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::TimeEntryList {});
                                    }
                                    Err(e) => {
                                        error.set(format!("Could not save time entry: {e}"));
                                    }
                                }
                            }
                            is_submitting.set(false);
                        });
                    },

                    if !err.is_empty() {
                        div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                            p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "work_item",
                            label: "Work Item",
                            options: work_item_options,
                            value: work_item.read().clone(),
                            placeholder: "Select work item",
                            required: true,
                            onchange: move |e: FormEvent| {
                                work_item.set(e.value());
                                // Reset the task when the work item changes so a
                                // stale task from a previous project isn't kept.
                                task.set(String::new());
                            },
                        }
                        Select {
                            name: "work_type",
                            label: "Work Type",
                            options: work_type_options,
                            value: work_type.read().clone(),
                            placeholder: "Select work type",
                            required: true,
                            onchange: move |e: FormEvent| work_type.set(e.value()),
                        }
                    }

                    if is_project_item {
                        Select {
                            name: "task",
                            label: "Task (optional)",
                            options: task_options,
                            value: task.read().clone(),
                            onchange: move |e: FormEvent| task.set(e.value()),
                        }
                    }

                    crate::components::Input {
                        name: "hours",
                        label: "Hours",
                        // Free-text so H:MM (e.g. "0:30") can be typed; a
                        // type="number" input blocks the colon. PMS-314.
                        // Validation lives in the submit handler.
                        r#type: "text",
                        placeholder: "2, 2.5, or 1:30",
                        help: "Decimal hours or H:MM.",
                        required: true,
                        value: hours.read().clone(),
                        oninput: move |e: FormEvent| hours.set(e.value()),
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "What did you work on?",
                        rows: 3,
                        required: true,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    crate::components::Checkbox {
                        name: "billable",
                        label: "Billable",
                        checked: *is_billable.read(),
                        help: "Mark this time entry as billable to the customer",
                        onchange: move |_| {
                            let current = *is_billable.read();
                            is_billable.set(!current);
                        },
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::TimeEntryList {},
                            Button {
                                variant: ButtonVariant::Secondary,
                                "Cancel"
                            }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Save Time Entry"
                        }
                    }
                }
            }
        }
    }
}

/// Timesheets page. Builds the weekly grid by pivoting the signed-in user's
/// `time_entries` for the selected week (the `/timesheets` summary endpoint
/// returns week totals, not the per-work-item/day breakdown the grid needs),
/// shows the week's approval status, and wires the Submit action.
#[component]
pub fn TimesheetsPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    let today = Utc::now().date_naive();
    let mut week_start = use_signal(|| monday_of_week(today));
    let mut is_submitting = use_signal(|| false);
    let mut action_msg = use_signal(String::new);
    let mut action_err = use_signal(String::new);
    // PMS-183 submit-confirmation modal + certification gate, and withdraw.
    let mut show_submit_modal = use_signal(|| false);
    let mut certified = use_signal(|| false);
    let mut is_withdrawing = use_signal(|| false);

    // The selected week's entries, scoped to the signed-in user. Reading
    // `week_start`/`auth` inside makes the resource re-run when they change.
    let entries_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let start = week_start();
        let end = start + Duration::days(6);
        let user_id = auth.read().user.as_ref().map(|u| u.id)?;
        let path =
            format!("/time-entries?user_id={user_id}&date_from={start}&date_to={end}&per_page=500");
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>(&path)
            .await
            .ok()
            .map(|p| p.data)
    });

    // The week summary, for the approval-status badge.
    let summary_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let start = week_start();
        let user_id = auth.read().user.as_ref().map(|u| u.id)?;
        let path = format!("/timesheets?user_id={user_id}&week={start}");
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimesheet>>(&path)
            .await
            .ok()
            .and_then(|p| p.data.into_iter().next())
    });

    // Work-item labels: tickets and projects.
    let tickets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<TicketOption>>("/tickets")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let projects_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<ProjectOption>>("/projects")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let projects = projects_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let snapshot = entries_resource.read_unchecked().clone();
    // `None` while loading; `Some(None)` on fetch failure (or no signed-in
    // user yet); `Some(Some(rows))` once loaded.
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let entries: Vec<RemoteTimeEntry> = snapshot.flatten().unwrap_or_default();

    let start = week_start();
    let end = start + Duration::days(6);

    // Pivot the week's entries into per-work-item rows with seven day buckets
    // (Mon..Sun), summing duration into each (row, weekday) cell. Each row also
    // carries a link to its work item's detail page (MAPPS-206 for projects,
    // and tickets too for parity), or `None` for internal time.
    let work_item_of = |e: &RemoteTimeEntry| -> (String, Option<Route>) {
        if let Some(tid) = e.ticket_id {
            let label = tickets
                .iter()
                .find(|t| t.id == tid)
                .map(|t| format!("{}: {}", t.ticket_number, t.title))
                .unwrap_or_else(|| format!("Ticket {}", short_id(tid)));
            return (
                label,
                Some(Route::TicketDetail {
                    id: tid.to_string(),
                }),
            );
        }
        if let Some(pid) = e.project_id {
            let label = projects
                .iter()
                .find(|p| p.id == pid)
                .map(|p| format!("Project: {}", p.name))
                .unwrap_or_else(|| format!("Project {}", short_id(pid)));
            return (
                label,
                Some(Route::ProjectDetail {
                    id: pid.to_string(),
                }),
            );
        }
        ("Internal".to_string(), None)
    };

    let mut rows: Vec<(String, Option<Route>, [i64; 7])> = Vec::new();
    for e in &entries {
        let day_idx = (e.date - start).num_days();
        if !(0..7).contains(&day_idx) {
            continue;
        }
        let (label, route) = work_item_of(e);
        let pos = match rows.iter().position(|(l, _, _)| *l == label) {
            Some(p) => p,
            None => {
                rows.push((label, route, [0; 7]));
                rows.len() - 1
            }
        };
        rows[pos].2[day_idx as usize] += e.duration_minutes;
    }
    let mut daily_totals = [0i64; 7];
    for (_, _, buckets) in &rows {
        for (i, m) in buckets.iter().enumerate() {
            daily_totals[i] += m;
        }
    }
    let grand_total: i64 = daily_totals.iter().sum();
    let has_entries = !rows.is_empty();

    let week_label = format!(
        "Week of {} {}-{}, {}",
        crate::utils::datetime::month_name(start.month()),
        start.day(),
        end.day(),
        start.year()
    );

    // Week-level approval status -> badge.
    let approval = summary_resource
        .read_unchecked()
        .clone()
        .flatten()
        .map(|s| s.approval_status)
        .unwrap_or_default();
    let (status_variant, status_text) = match approval.as_str() {
        "approved" => (BadgeVariant::Green, "Approved"),
        "rejected" => (BadgeVariant::Red, "Rejected"),
        "pending" => (BadgeVariant::Yellow, "Pending approval"),
        _ => (BadgeVariant::Gray, "Not submitted"),
    };
    let already_approved = approval == "approved";
    // A week that has been submitted (pending) can be withdrawn; one that is
    // draft/rejected can be (re)submitted.
    let is_pending = approval == "pending";
    // PMS-310: make the empty-week state self-explanatory. The current week
    // drives the "jump back" control; the hint explains why Submit is greyed
    // out (no time logged) rather than leaving the user with a dead button.
    let current_week = monday_of_week(today);
    let is_current_week = start == current_week;
    let show_no_entries_hint = !is_pending && !already_approved && !has_entries;
    let submitting = *is_submitting.read();
    let withdrawing = *is_withdrawing.read();
    let msg = action_msg.read().clone();
    let err = action_err.read().clone();

    rsx! {
        AppLayout { title: "Timesheets",
            PageHeader {
                title: "Timesheets",
                subtitle: "Weekly timesheet management",
                actions: rsx! {
                    div { class: "flex items-center gap-3",
                        Badge { variant: status_variant, "{status_text}" }
                        if is_pending {
                            // Submitted, not yet approved: allow withdrawal.
                            Button {
                                variant: ButtonVariant::Secondary,
                                loading: withdrawing,
                                onclick: move |_| {
                                    action_msg.set(String::new());
                                    action_err.set(String::new());
                                    let start = week_start();
                                    let user_id = match auth.read().user.as_ref().map(|u| u.id) {
                                        Some(id) => id,
                                        None => {
                                            action_err.set("Not signed in.".to_string());
                                            return;
                                        }
                                    };
                                    is_withdrawing.set(true);
                                    let mut sr = summary_resource;
                                    let mut er = entries_resource;
                                    spawn(async move {
                                        #[cfg(feature = "web")]
                                        {
                                            let path = format!("/timesheets/{user_id}/{start}/withdraw");
                                            match crate::hooks::fetch::api::post_authed::<
                                                serde_json::Value,
                                                _,
                                            >(&path, &serde_json::json!({}))
                                                .await
                                            {
                                                Ok(_) => {
                                                    action_msg
                                                        .set("Timesheet withdrawn back to draft.".to_string());
                                                    sr.restart();
                                                    er.restart();
                                                }
                                                Err(e) => {
                                                    action_err
                                                        .set(format!("Could not withdraw timesheet: {e}"));
                                                }
                                            }
                                        }
                                        is_withdrawing.set(false);
                                    });
                                },
                                "Withdraw"
                            }
                        } else {
                            // Draft / rejected: open the submit confirmation modal.
                            // Column so the disabled reason (PMS-310) can sit
                            // directly under the button, right-aligned with it.
                            div { class: "flex flex-col items-end gap-1",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    disabled: already_approved || !has_entries,
                                    onclick: move |_| {
                                        action_msg.set(String::new());
                                        action_err.set(String::new());
                                        certified.set(false);
                                        show_submit_modal.set(true);
                                    },
                                    "Submit Timesheet"
                                }
                                if show_no_entries_hint {
                                    span { class: "text-xs text-gray-500 dark:text-gray-400",
                                        "No time logged this week yet."
                                    }
                                }
                            }
                        }
                    }
                },
            }

            if !msg.is_empty() {
                div { class: "mb-4 rounded-md bg-green-50 dark:bg-green-900/20 p-3",
                    p { class: "text-sm text-green-700 dark:text-green-400", "{msg}" }
                }
            }
            if !err.is_empty() {
                div { class: "mb-4 rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                    p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                }
            }

            // Week selector
            Card { class: "mb-6",
                div { class: "flex items-center justify-between",
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Previous week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() - Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                    }
                    div { class: "flex flex-col items-center gap-1",
                        span { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{week_label}"
                        }
                        // PMS-310: one-click return to the current week (only
                        // shown when paged away from it), so a future/empty
                        // week is not a dead end.
                        if !is_current_week {
                            button {
                                r#type: "button",
                                class: "text-xs font-medium text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                onclick: move |_| {
                                    action_msg.set(String::new());
                                    action_err.set(String::new());
                                    week_start.set(current_week);
                                },
                                "Jump to current week"
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Next week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() + Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5".to_string() }
                    }
                }
            }

            // Weekly grid
            Card { padding: false,
                div { class: "overflow-x-auto",
                    table { class: "min-w-full divide-y divide-gray-200 dark:divide-gray-700",
                        thead { class: "bg-gray-50 dark:bg-gray-800",
                            tr {
                                th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider",
                                    "Work Item"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Mon"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Tue"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Wed"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Thu"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Fri"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Sat"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Sun"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Total"
                                }
                            }
                        }
                        tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
                            if is_loading {
                                // PMS-353: shimmer rows (matching the shared
                                // skeleton style) instead of a bare text line.
                                // This timesheet is a bespoke weekly grid, not
                                // the shared Table, so it can't use TableLoading.
                                for _ in 0..4 {
                                    tr {
                                        td { class: "px-6 py-4", colspan: "9",
                                            div { class: "h-4 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                                        }
                                    }
                                }
                            } else if load_failed {
                                tr {
                                    td {
                                        class: "px-6 py-8 text-center text-sm text-red-500",
                                        colspan: "9",
                                        "Could not load timesheet. The time-tracking service may be unavailable."
                                    }
                                }
                            } else if !has_entries {
                                // MAPPS-201: empty-week prompt + one-click path
                                // to a new entry for the current week. Submit
                                // stays disabled until an entry exists.
                                tr {
                                    td {
                                        class: "px-6 py-8 text-center",
                                        colspan: "9",
                                        p { class: "text-sm text-gray-500 dark:text-gray-400",
                                            "No time logged this week yet. Select a time to log for this week."
                                        }
                                        div { class: "mt-3",
                                            Link {
                                                to: Route::TimeEntryNew {},
                                                Button {
                                                    variant: ButtonVariant::Primary,
                                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                                    "Log Time"
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                for (label , route , buckets) in rows.iter() {
                                    {
                                        let row_total = buckets.iter().sum::<i64>();
                                        rsx! {
                                            tr {
                                                td { class: "px-6 py-3 text-sm",
                                                    // MAPPS-206: link the work item to its detail.
                                                    if let Some(r) = route {
                                                        Link {
                                                            to: r.clone(),
                                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                                            "{label}"
                                                        }
                                                    } else {
                                                        span { class: "text-gray-900 dark:text-white", "{label}" }
                                                    }
                                                }
                                                for m in buckets.iter() {
                                                    td { class: "px-4 py-3 text-center text-sm",
                                                        if *m == 0 {
                                                            span { class: "text-gray-300 dark:text-gray-600", "-" }
                                                        } else {
                                                            span { class: "text-gray-900 dark:text-white", "{fmt_hours(*m)}" }
                                                        }
                                                    }
                                                }
                                                td { class: "px-4 py-3 text-center text-sm font-medium text-gray-900 dark:text-white",
                                                    "{fmt_hours(row_total)}"
                                                }
                                            }
                                        }
                                    }
                                }
                                tr { class: "bg-gray-50 dark:bg-gray-800 font-medium",
                                    td { class: "px-6 py-3 text-sm text-gray-900 dark:text-white",
                                        "Daily Total"
                                    }
                                    for m in daily_totals.iter() {
                                        td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white",
                                            if *m == 0 {
                                                span { class: "text-gray-500", "0" }
                                            } else {
                                                "{fmt_hours(*m)}"
                                            }
                                        }
                                    }
                                    td { class: "px-4 py-3 text-center text-sm font-bold text-blue-600",
                                        "{fmt_hours(grand_total)}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // PMS-183 submit confirmation with required certification.
            {
                let do_submit = move |_| {
                    if !certified() || submitting {
                        return;
                    }
                    let start = week_start();
                    let user_id = match auth.read().user.as_ref().map(|u| u.id) {
                        Some(id) => id,
                        None => {
                            action_err.set("Not signed in.".to_string());
                            return;
                        }
                    };
                    is_submitting.set(true);
                    let mut sr = summary_resource;
                    let mut er = entries_resource;
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/timesheets/{user_id}/{start}/submit");
                            match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                &path,
                                &serde_json::json!({}),
                            )
                            .await
                            {
                                Ok(_) => {
                                    action_msg
                                        .set("Timesheet submitted for approval.".to_string());
                                    show_submit_modal.set(false);
                                    sr.restart();
                                    er.restart();
                                }
                                Err(e) => {
                                    action_err.set(format!("Could not submit timesheet: {e}"));
                                }
                            }
                        }
                        is_submitting.set(false);
                    });
                };
                rsx! {
                    Modal {
                        open: show_submit_modal(),
                        title: "Submit Timesheet",
                        size: crate::components::ModalSize::Medium,
                        onclose: move |_| show_submit_modal.set(false),
                        footer: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| show_submit_modal.set(false),
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: submitting,
                                disabled: !certified(),
                                onclick: do_submit,
                                "Submit for Approval"
                            }
                        },
                        div { class: "space-y-4",
                            p { class: "text-sm text-gray-600 dark:text-gray-300",
                                "Once submitted, this timesheet goes to your manager for approval. You can withdraw it back to draft until it is approved."
                            }
                            Checkbox {
                                name: "certify",
                                label: "I certify that the timesheet I am submitting is correct.",
                                checked: certified(),
                                onchange: move |e: FormEvent| certified.set(e.checked()),
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Timesheet approvals (MAPPS-194)
//
// Manager/admin surface for the approval half of the timesheet workflow.
// The employee side (submit/withdraw + status badge) lives in
// `TimesheetsPage` above; submitted weeks had no reachable approve/reject
// control, so they sat in "pending" forever. This page lists every user's
// week summary (`GET /timesheets?week=` with no `user_id` -> tenant-wide),
// keeps the ones still awaiting approval, and wires Approve / Reject per row.
// ============================================================================

/// A week summary row from `GET /timesheets` (no `user_id` filter aggregates
/// every user). The badge-only `RemoteTimesheet` above drops everything but
/// the status; the approvals queue needs the user, totals and entry count to
/// render and act on a row.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ApprovalSummary {
    user_id: uuid::Uuid,
    #[serde(default)]
    total_minutes: i64,
    #[serde(default)]
    billable_minutes: i64,
    #[serde(default)]
    entry_count: i64,
    #[serde(default)]
    approval_status: String,
}

/// A user for resolving a summary's `user_id` to a name on the queue
/// (`GET /auth/users`, server-gated to Admin / Manager).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ApprovalUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

impl ApprovalUser {
    fn display_name(&self) -> String {
        if !self.full_name.trim().is_empty() {
            return self.full_name.clone();
        }
        let joined = format!("{} {}", self.first_name, self.last_name);
        let joined = joined.trim();
        if joined.is_empty() {
            "Unknown user".to_string()
        } else {
            joined.to_string()
        }
    }
}

/// Manager/admin timesheet approvals queue.
#[component]
pub fn TimesheetApprovalsPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    // Match the server's `RequireManager` gate on approve/reject (manager,
    // admin, super_admin). The page re-checks server-side, so this is a UX
    // affordance, not a security boundary.
    let can_manage = auth
        .read()
        .user
        .as_ref()
        .is_some_and(|u| u.role.can_manage_users());

    let today = Utc::now().date_naive();
    let mut week_start = use_signal(|| monday_of_week(today));
    let mut action_msg = use_signal(String::new);
    let mut action_err = use_signal(String::new);
    // Per-row approve in flight (so only the pressed row shows a spinner).
    let mut approving = use_signal::<Option<uuid::Uuid>>(|| None);
    // Reject modal: the (user_id, display name) of the row being rejected,
    // plus the required reason and an in-flight flag.
    let mut reject_target = use_signal::<Option<(uuid::Uuid, String)>>(|| None);
    let mut reject_reason = use_signal(String::new);
    let mut is_rejecting = use_signal(|| false);

    // Every user's summary for the selected week. No `user_id` filter, so the
    // server aggregates tenant-wide. Lower roles would get a 403, so skip the
    // fetch for them entirely (the gate below renders a notice instead).
    let summaries_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let can = auth
            .read()
            .user
            .as_ref()
            .is_some_and(|u| u.role.can_manage_users());
        if !can {
            return None;
        }
        let start = week_start();
        let path = format!("/timesheets?week={start}");
        crate::hooks::fetch::api::get_authed::<Paginated<ApprovalSummary>>(&path)
            .await
            .ok()
            .map(|p| p.data)
    });

    // Names for the user_ids in the summaries.
    let users_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let can = auth
            .read()
            .user
            .as_ref()
            .is_some_and(|u| u.role.can_manage_users());
        if !can {
            return Vec::<ApprovalUser>::new();
        }
        crate::hooks::fetch::api::get_authed::<Paginated<ApprovalUser>>("/auth/users?per_page=100")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    if !can_manage {
        return rsx! {
            AppLayout { title: "Timesheet Approvals",
                PageHeader {
                    title: "Timesheet Approvals",
                    subtitle: "Review and approve submitted timesheets",
                }
                Card {
                    p { class: "text-sm text-gray-500 dark:text-gray-400",
                        "You need a manager or admin role to review timesheets."
                    }
                }
            }
        };
    }

    let start = week_start();
    let end = start + Duration::days(6);
    let current_week = monday_of_week(today);
    let is_current_week = start == current_week;
    let week_label = format!(
        "Week of {} {}-{}, {}",
        crate::utils::datetime::month_name(start.month()),
        start.day(),
        end.day(),
        start.year()
    );

    let snapshot = summaries_resource.read_unchecked().clone();
    // `None` while loading; `Some(None)` on fetch failure; `Some(Some(rows))`.
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let summaries: Vec<ApprovalSummary> = snapshot.flatten().unwrap_or_default();

    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let name_of = |uid: uuid::Uuid| -> String {
        users
            .iter()
            .find(|u| u.id == uid)
            .map(|u| u.display_name())
            .unwrap_or_else(|| format!("User {}", short_id(uid)))
    };

    // A "submitted, awaiting approval" week is `pending` with entries. The
    // summary rolls per-entry approval_status up to pending when not all
    // entries are approved/rejected; an empty week never appears here.
    let pending: Vec<ApprovalSummary> = summaries
        .iter()
        .filter(|s| s.approval_status == "pending" && s.entry_count > 0)
        .cloned()
        .collect();
    let pending_count = pending.len();

    let approving_id = *approving.read();
    let msg = action_msg.read().clone();
    let err = action_err.read().clone();

    rsx! {
        AppLayout { title: "Timesheet Approvals",
            PageHeader {
                title: "Timesheet Approvals",
                subtitle: "Review and approve submitted timesheets",
                actions: rsx! {
                    Badge { variant: BadgeVariant::Yellow, "{pending_count} awaiting approval" }
                },
            }

            if !msg.is_empty() {
                div { class: "mb-4 rounded-md bg-green-50 dark:bg-green-900/20 p-3",
                    p { class: "text-sm text-green-700 dark:text-green-400", "{msg}" }
                }
            }
            if !err.is_empty() {
                div { class: "mb-4 rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                    p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                }
            }

            // Week selector (mirrors the employee timesheet page).
            Card { class: "mb-6",
                div { class: "flex items-center justify-between",
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Previous week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() - Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                    }
                    div { class: "flex flex-col items-center gap-1",
                        span { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{week_label}"
                        }
                        if !is_current_week {
                            button {
                                r#type: "button",
                                class: "text-xs font-medium text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                onclick: move |_| {
                                    action_msg.set(String::new());
                                    action_err.set(String::new());
                                    week_start.set(current_week);
                                },
                                "Jump to current week"
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Next week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() + Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5".to_string() }
                    }
                }
            }

            DataTable {
                total_items: pending_count,
                current_page: 1,
                per_page: if pending_count == 0 { 25 } else { pending_count },
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Employee" }
                            TableHeader { "Total" }
                            TableHeader { "Billable" }
                            TableHeader { "Entries" }
                            TableHeader { "" }
                        }
                    }
                    TableBody {
                        if is_loading {
                            TableRow {
                                TableCell { class: "text-gray-400", "Loading…" }
                            }
                        } else if load_failed {
                            TableRow {
                                TableCell { class: "text-red-500",
                                    "Could not load timesheets. The time-tracking service may be unavailable."
                                }
                            }
                        } else if pending.is_empty() {
                            TableRow {
                                TableCell { class: "text-gray-400 italic",
                                    "No timesheets awaiting approval for this week."
                                }
                            }
                        } else {
                            for s in pending.iter() {
                                {
                                    let uid = s.user_id;
                                    let name = name_of(uid);
                                    let name_for_reject = name.clone();
                                    let total = fmt_hours(s.total_minutes);
                                    let billable = fmt_hours(s.billable_minutes);
                                    let entries = s.entry_count;
                                    let row_busy = approving_id == Some(uid) || is_rejecting();
                                    rsx! {
                                        TableRow { key: "{uid}",
                                            TableCell { class: "font-medium text-gray-900 dark:text-white", "{name}" }
                                            TableCell { "{total}" }
                                            TableCell { class: "text-green-600", "{billable}" }
                                            TableCell { "{entries}" }
                                            TableCell {
                                                div { class: "flex justify-end gap-2",
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        disabled: row_busy,
                                                        onclick: move |_| {
                                                            action_msg.set(String::new());
                                                            action_err.set(String::new());
                                                            reject_reason.set(String::new());
                                                            reject_target.set(Some((uid, name_for_reject.clone())));
                                                        },
                                                        "Reject"
                                                    }
                                                    Button {
                                                        variant: ButtonVariant::Primary,
                                                        loading: approving_id == Some(uid),
                                                        disabled: row_busy,
                                                        onclick: move |_| {
                                                            action_msg.set(String::new());
                                                            action_err.set(String::new());
                                                            approving.set(Some(uid));
                                                            let start = week_start();
                                                            let mut sr = summaries_resource;
                                                            spawn(async move {
                                                                #[cfg(feature = "web")]
                                                                {
                                                                    let path = format!("/timesheets/{uid}/{start}/approve");
                                                                    match crate::hooks::fetch::api::post_authed::<
                                                                        serde_json::Value,
                                                                        _,
                                                                    >(&path, &serde_json::json!({}))
                                                                        .await
                                                                    {
                                                                        Ok(_) => {
                                                                            action_msg.set("Timesheet approved.".to_string());
                                                                            sr.restart();
                                                                        }
                                                                        Err(e) => {
                                                                            action_err
                                                                                .set(format!("Could not approve timesheet: {e}"));
                                                                        }
                                                                    }
                                                                }
                                                                approving.set(None);
                                                            });
                                                        },
                                                        "Approve"
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

            // Reject reason modal (MAPPS-189: in-app modal, not window.prompt).
            {
                let target = reject_target.read().clone();
                let open = target.is_some();
                let reject_name = target
                    .as_ref()
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                let rejecting = is_rejecting();
                let reason_empty = reject_reason.read().trim().is_empty();
                let do_reject = move |_| {
                    let Some((uid, _)) = reject_target.read().clone() else {
                        return;
                    };
                    let reason = reject_reason.read().trim().to_string();
                    if reason.is_empty() || rejecting {
                        return;
                    }
                    is_rejecting.set(true);
                    let start = week_start();
                    let mut sr = summaries_resource;
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/timesheets/{uid}/{start}/reject");
                            match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                &path,
                                &serde_json::json!({ "reason": reason }),
                            )
                            .await
                            {
                                Ok(_) => {
                                    action_msg
                                        .set("Timesheet rejected; the employee will see the reason.".to_string());
                                    reject_target.set(None);
                                    reject_reason.set(String::new());
                                    sr.restart();
                                }
                                Err(e) => {
                                    action_err.set(format!("Could not reject timesheet: {e}"));
                                }
                            }
                        }
                        is_rejecting.set(false);
                    });
                };
                rsx! {
                    Modal {
                        open,
                        title: "Reject Timesheet",
                        size: crate::components::ModalSize::Medium,
                        onclose: move |_| {
                            if !is_rejecting() {
                                reject_target.set(None);
                            }
                        },
                        footer: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: rejecting,
                                onclick: move |_| {
                                    if !is_rejecting() {
                                        reject_target.set(None);
                                    }
                                },
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Danger,
                                loading: rejecting,
                                disabled: reason_empty,
                                onclick: do_reject,
                                "Reject Timesheet"
                            }
                        },
                        div { class: "space-y-4",
                            p { class: "text-sm text-gray-600 dark:text-gray-300",
                                "Reject {reject_name}'s timesheet for this week. A reason is required and is shown to the employee so they can correct and resubmit."
                            }
                            crate::components::Textarea {
                                name: "reject_reason",
                                label: "Reason",
                                placeholder: "Explain what needs to change before resubmitting.",
                                rows: 3,
                                required: true,
                                value: reject_reason.read().clone(),
                                oninput: move |e: FormEvent| reject_reason.set(e.value()),
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Time-entry edit modal (MAPPS-166)
//
// Click-to-edit a logged time entry. Edits the common fields (work type,
// hours, date, description, billable) and supports delete. The work item
// (ticket/project) is intentionally NOT changeable here: the server's PUT
// direct-sets ticket_id/project_id (no COALESCE) and never recomputes
// company_id, so changing them would risk a stale company. Those ids are
// re-sent unchanged so the partial-looking update does not null them. `task_id`
// is not in the time-entry response, so it cannot be echoed; the entry's task
// is preserved server-side by PMS-328 (COALESCE on task_id when omitted). To
// move an entry to a different work item, delete it and re-log.
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TimeEntryEditModalProps {
    entry: RemoteTimeEntry,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TimeEntryEditModal(props: TimeEntryEditModalProps) -> Element {
    let entry = props.entry.clone();
    let eid = entry.id;
    let ticket_id = entry.ticket_id;
    let project_id = entry.project_id;
    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>(
            "/work-types?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let work_types = work_types_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let mut work_type_options = vec![SelectOption::new("", "Select a work type")];
    work_type_options.extend(
        work_types
            .iter()
            .map(|w| SelectOption::new(w.id.to_string(), w.name.clone())),
    );

    let mut work_type = use_signal(|| {
        entry
            .work_type_id
            .map(|v| v.to_string())
            .unwrap_or_default()
    });
    // Pre-fill in a clean, parseable shape honoring the duration-format
    // pref (PMS-314); avoids raw decimals like "0.16666666666666666".
    let mut hours = use_signal(|| crate::utils::duration::fmt_input(entry.duration_minutes));
    let mut date = use_signal(|| entry.date.to_string());
    let mut description = use_signal(|| entry.notes.clone().unwrap_or_default());
    let mut is_billable = use_signal(|| entry.is_billable);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let wi_label = work_item_label(&entry);

    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        let wtid = work_type.read().trim().to_string();
        if wtid.is_empty() {
            error.set("Please pick a work type.".to_string());
            return;
        }
        let duration_minutes = match crate::utils::duration::parse_input_to_minutes(&hours.read()) {
            Some(m) if m > 0 && m <= 24 * 60 => m,
            _ => {
                error.set(
                    "Enter time as hours (2.5) or H:MM (1:30), greater than 0 and at most 24h."
                        .to_string(),
                );
                return;
            }
        };
        let d = date.read().trim().to_string();
        if d.is_empty() {
            error.set("Please pick a date.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().clone();
        let billable = *is_billable.read();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                // Re-send ticket/project so the direct-set update keeps them.
                // `task_id` is intentionally omitted: the response does not
                // carry it, so we cannot echo the current value; PMS-328 makes
                // the server preserve task_id when it is absent (COALESCE).
                let body = serde_json::json!({
                    "date": d,
                    "duration_minutes": duration_minutes,
                    "work_type_id": wtid,
                    "notes": desc,
                    "is_billable": billable,
                    "ticket_id": ticket_id,
                    "project_id": project_id,
                });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    &format!("/time-entries/{eid}"),
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(e) => error.set(format!("Could not save time entry: {e}")),
                }
            }
            saving.set(false);
        });
    };

    // MAPPS-189: the Delete button opens the styled ConfirmDialog instead
    // of the native window.confirm(); the DELETE fires from
    // `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let handle_delete = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        confirming_delete.set(true);
    };
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::delete_authed(&format!("/time-entries/{eid}")).await
                {
                    Ok(()) => onsaved.call(()),
                    Err(e) => error.set(format!("Could not delete time entry: {e}")),
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Edit Time Entry",
            size: crate::components::ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Danger,
                    loading: *deleting.read(),
                    onclick: handle_delete,
                    "Delete"
                }
                div { class: "flex-1" }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *saving.read(),
                    onclick: handle_save,
                    "Save Changes"
                }
            },
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                        p { class: "text-sm text-red-600 dark:text-red-400", "{error.read()}" }
                    }
                }
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "Work item: {wi_label}. To move this entry to a different work item, delete it and log again."
                }
                Select {
                    name: "edit_work_type",
                    label: "Work Type",
                    options: work_type_options,
                    value: work_type.read().clone(),
                    required: true,
                    onchange: move |e: FormEvent| work_type.set(e.value()),
                }
                crate::components::Input {
                    name: "edit_hours",
                    label: "Hours",
                    // Free-text so H:MM (e.g. "0:30") can be typed; a
                    // type="number" input blocks the colon. PMS-314.
                    // The save handler enforces 0 < t <= 24h.
                    r#type: "text",
                    placeholder: "2, 2.5, or 1:30",
                    help: "Decimal hours or H:MM.",
                    required: true,
                    value: hours.read().clone(),
                    oninput: move |e: FormEvent| hours.set(e.value()),
                }
                crate::components::DateField {
                    name: "edit_date",
                    label: "Date",
                    required: true,
                    value: date.read().clone(),
                    oninput: move |e: FormEvent| date.set(e.value()),
                }
                crate::components::Textarea {
                    name: "edit_description",
                    label: "Description",
                    rows: 3,
                    value: description.read().clone(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
                Checkbox {
                    name: "edit_billable",
                    label: "Billable",
                    checked: *is_billable.read(),
                    onchange: move |_| {
                        let c = *is_billable.read();
                        is_billable.set(!c);
                    },
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete time entry".to_string(),
            message: "Delete this time entry? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
    }
}

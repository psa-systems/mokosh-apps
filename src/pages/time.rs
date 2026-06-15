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

/// A time entry (`GET /api/v1/time-entries`). Names aren't joined into the
/// response, so the list renders ids/links rather than ticket titles.
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
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Work Item" }
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
                                                            "Ticket"
                                                        }
                                                    }
                                                } else if e.project_id.is_some() {
                                                    span { class: "font-medium text-gray-700 dark:text-gray-300", "Project" }
                                                } else {
                                                    span { class: "text-gray-400", "-" }
                                                }
                                            }
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
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>("/work-types")
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
                        // Mirror the input's min/max bounds so a value that
                        // slips past browser validation (or non-browser submit)
                        // is still rejected: positive, decimal, at most 24h.
                        let hours_val: f64 = match hrs.trim().parse() {
                            Ok(h) if h > 0.0 && h <= 24.0 => h,
                            _ => {
                                error.set(
                                    "Enter hours greater than 0 and no more than 24.".to_string(),
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
                        let duration_minutes = (hours_val * 60.0).round() as i64;
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
                        r#type: "number",
                        // Decimal hours only; HH:MM (e.g. "0:30") is not
                        // supported. step/min keep 0.25, 0.5, ... valid while
                        // the browser rejects negatives and zero; max caps
                        // absurd magnitudes (e.g. 1000h) at a single day.
                        step: "0.25",
                        min: "0.25",
                        max: "24",
                        placeholder: "0.00",
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
    // (Mon..Sun), summing duration into each (row, weekday) cell.
    let label_for = |e: &RemoteTimeEntry| -> String {
        if let Some(tid) = e.ticket_id {
            return tickets
                .iter()
                .find(|t| t.id == tid)
                .map(|t| format!("{}: {}", t.ticket_number, t.title))
                .unwrap_or_else(|| format!("Ticket {}", short_id(tid)));
        }
        if let Some(pid) = e.project_id {
            return projects
                .iter()
                .find(|p| p.id == pid)
                .map(|p| format!("Project: {}", p.name))
                .unwrap_or_else(|| format!("Project {}", short_id(pid)));
        }
        "Internal".to_string()
    };

    let mut rows: Vec<(String, [i64; 7])> = Vec::new();
    for e in &entries {
        let day_idx = (e.date - start).num_days();
        if !(0..7).contains(&day_idx) {
            continue;
        }
        let label = label_for(e);
        let pos = match rows.iter().position(|(l, _)| *l == label) {
            Some(p) => p,
            None => {
                rows.push((label, [0; 7]));
                rows.len() - 1
            }
        };
        rows[pos].1[day_idx as usize] += e.duration_minutes;
    }
    let mut daily_totals = [0i64; 7];
    for (_, buckets) in &rows {
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
                    span { class: "text-lg font-medium text-gray-900 dark:text-white",
                        "{week_label}"
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
                                tr {
                                    td {
                                        class: "px-6 py-8 text-center text-sm text-gray-500",
                                        colspan: "9",
                                        "Loading timesheet..."
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
                                tr {
                                    td {
                                        class: "px-6 py-8 text-center text-sm text-gray-500",
                                        colspan: "9",
                                        "No time logged this week."
                                    }
                                }
                            } else {
                                for (label , buckets) in rows.iter() {
                                    {
                                        let row_total = buckets.iter().sum::<i64>();
                                        rsx! {
                                            tr {
                                                td { class: "px-6 py-3 text-sm text-gray-900 dark:text-white",
                                                    "{label}"
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
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>("/work-types")
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
    let mut hours = use_signal(|| format!("{}", entry.duration_minutes as f64 / 60.0));
    let mut date = use_signal(|| entry.date.to_string());
    let mut description = use_signal(|| entry.notes.clone().unwrap_or_default());
    let mut is_billable = use_signal(|| entry.is_billable);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let work_item_label = if ticket_id.is_some() {
        "Ticket"
    } else if project_id.is_some() {
        "Project"
    } else {
        "none"
    };

    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        let wtid = work_type.read().trim().to_string();
        if wtid.is_empty() {
            error.set("Please pick a work type.".to_string());
            return;
        }
        let hours_val: f64 = match hours.read().trim().parse() {
            Ok(h) if h > 0.0 && h <= 24.0 => h,
            _ => {
                error.set("Enter hours greater than 0 and no more than 24.".to_string());
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
        let duration_minutes = (hours_val * 60.0).round() as i64;
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

    let handle_delete = move |_| {
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
                        w.confirm_with_message("Delete this time entry? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    match crate::hooks::fetch::api::delete_authed(&format!("/time-entries/{eid}"))
                        .await
                    {
                        Ok(()) => onsaved.call(()),
                        Err(e) => error.set(format!("Could not delete time entry: {e}")),
                    }
                }
            }
            deleting.set(false);
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
                    "Work item: {work_item_label}. To move this entry to a different work item, delete it and log again."
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
                    r#type: "number",
                    // `step: "any"` so an existing off-grid duration (e.g. a
                    // 10-minute entry -> 0.1667h) does not render :invalid; the
                    // save handler still enforces 0 < hours <= 24.
                    step: "any",
                    min: "0",
                    max: "24",
                    required: true,
                    value: hours.read().clone(),
                    oninput: move |e: FormEvent| hours.set(e.value()),
                }
                crate::components::Input {
                    name: "edit_date",
                    label: "Date",
                    r#type: "date",
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
    }
}

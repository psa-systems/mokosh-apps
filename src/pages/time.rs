//! Time tracking pages

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, Checkbox, ChevronRightIcon,
    DataTable, IconSize, Modal, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::utils::{FormGuard, Paginated, Rule};
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
    // PMS-394 / MAPPS-243: server classifier for the kind of work
    // ("ticketed" | "project" | "general"). Drives the "General" label for
    // a ticketless overhead entry. `None` on older entries the server has
    // not classified.
    #[serde(default)]
    work_category: Option<String>,
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

/// Per-key tenant setting response (`GET /settings/...`). Only `value` is
/// read; the server stores the max-hours-per-day cap as a JSON integer
/// (MAPPS-244 / PMS-396).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct SettingValueRow {
    #[serde(default)]
    value: serde_json::Value,
}

/// Hard upper bound on a single entry's duration, and the fallback per-day
/// cap when the tenant has not configured one (404). Kept as minutes.
const MAX_SINGLE_ENTRY_MINUTES: i64 = 24 * 60;
const DEFAULT_MAX_MINUTES_PER_DAY: i64 = 24 * 60;

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
/// "{project} · {task}" using the names joined server-side (PMS-332).
/// MAPPS-243: a ticketless general/overhead entry shows "General". Falls
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
    } else if e.work_category.as_deref() == Some("general") {
        // MAPPS-243: a deliberate overhead entry (no ticket, no project),
        // classified server-side. Distinguishes it from an unlinked "-".
        "General".to_string()
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
                    p { class: "text-sm text-muted", "Today" }
                    p { class: "text-2xl font-bold text-content", "{today_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "This Week" }
                    p { class: "text-2xl font-bold text-content", "{week_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "Billable" }
                    p { class: "text-2xl font-bold text-green-600", "{billable_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "Non-Billable" }
                    p { class: "text-2xl font-bold text-muted", "{nonbillable_h}" }
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
                            TableRow { TableCell { class: "text-subtle", "Loading…" } }
                        } else if entries.is_empty() {
                            TableRow {
                                TableCell { class: "text-subtle italic", "No time logged yet." }
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
                                            TableCell { class: "text-muted", "{e.date}" }
                                            TableCell {
                                                if let Some(tid) = e.ticket_id {
                                                    // Stop the link click from also opening the
                                                    // row's edit modal; let it just navigate.
                                                    span {
                                                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                        Link {
                                                            to: Route::TicketDetail { id: tid.to_string() },
                                                            class: "font-medium text-accent hover:opacity-90",
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
                                                            class: "font-medium text-accent hover:opacity-90",
                                                            "{wi_label}"
                                                        }
                                                    }
                                                } else if e.work_category.as_deref()
                                                    == Some("general")
                                                {
                                                    // MAPPS-243: a ticketless overhead entry. No
                                                    // detail page to link to, so render the
                                                    // "General" label as plain content.
                                                    span { class: "font-medium text-content", "{wi_label}" }
                                                } else {
                                                    span { class: "text-subtle", "-" }
                                                }
                                            }
                                            TableCell { class: "text-muted", "{wt_label}" }
                                            TableCell { class: "max-w-xs truncate", "{note}" }
                                            TableCell { class: "font-medium", "{hrs}" }
                                            TableCell {
                                                if e.is_billable {
                                                    Badge { variant: BadgeVariant::Green, "Billable" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Non-Billable" }
                                                }
                                                if !status.is_empty() && status != "not_billed" {
                                                    span { class: "ml-2 text-xs text-subtle", "{status}" }
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

/// PMS-362: optional ticket prefill carried on the Log Time URL
/// (`/time/new?ticket_id=<uuid>`). The ticket-detail "Log Time" affordance
/// links here so the work-item picker opens with that ticket preselected,
/// instead of dropping the user into an empty form they have to re-search.
/// Returns the `work_item` select value (`ticket:<uuid>`) or empty.
fn read_ticket_prefill_from_url() -> String {
    #[cfg(feature = "web")]
    {
        if let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) {
            if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                let id = params.get("ticket_id").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return format!("ticket:{id}");
                }
                // MAPPS-275: also honour `?project_id=` so the
                // project-detail "Log Time" affordance can pre-select
                // that project in the work-item picker. Mirrors the
                // existing ticket prefill (the picker value is the
                // `project:<uuid>` / `ticket:<uuid>` discriminator).
                let pid = params.get("project_id").unwrap_or_default();
                if uuid::Uuid::parse_str(&pid).is_ok() {
                    return format!("project:{pid}");
                }
            }
        }
    }
    String::new()
}

/// New time entry page
#[component]
pub fn TimeEntryNewPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    // PMS-362: seed the work-item picker from `?ticket_id=` when linked from a
    // ticket; falls back to empty for direct navigation (no regression).
    let mut work_item = use_signal(read_ticket_prefill_from_url);
    let mut task = use_signal(String::new);
    let mut work_type = use_signal(String::new);
    let mut hours = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut is_billable = use_signal(|| true);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error slots, fed by the FormGuard in the submit
    // handler. The form-level `error` banner is kept for the cross-field /
    // resolution / daily-cap messages that have no single field to attach to.
    let mut work_item_error = use_signal(String::new);
    let mut work_type_error = use_signal(String::new);
    let mut hours_error = use_signal(String::new);
    let mut description_error = use_signal(String::new);

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

    // MAPPS-244: the configured per-day cap (minutes). A missing setting
    // (404) or any fetch failure falls back to the 24h default so a settings
    // outage never blocks logging; the server still enforces the real cap.
    let cap_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            match crate::hooks::fetch::api::get_authed_typed::<SettingValueRow>(
                "/settings/time_tracking/max_hours_per_day",
            )
            .await
            {
                Ok(row) => row
                    .value
                    .as_i64()
                    .filter(|h| (1..=24).contains(h))
                    .map(|h| h * 60)
                    .unwrap_or(DEFAULT_MAX_MINUTES_PER_DAY),
                Err(_) => DEFAULT_MAX_MINUTES_PER_DAY,
            }
        }
        #[cfg(not(feature = "web"))]
        {
            DEFAULT_MAX_MINUTES_PER_DAY
        }
    });

    // MAPPS-244: minutes already logged by this user for today, so the cap
    // check below sees the day's running total. The page always logs against
    // today (see `date` in the submit handler). Any failure falls back to 0.
    let today_total_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let today = Utc::now().date_naive();
        let user_id = auth.read().user.as_ref().map(|u| u.id)?;
        let path = format!(
            "/time-entries?user_id={user_id}&date_from={today}&date_to={today}&per_page=500"
        );
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>(&path)
            .await
            .ok()
            .map(|p| p.data.iter().map(|e| e.duration_minutes).sum::<i64>())
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

    // Snapshots captured by the submit closure for the pre-flight day-cap
    // check (MAPPS-244). Both are `Some` only once their resource resolves;
    // while loading, the pre-flight check is skipped and the server enforces.
    let cap_minutes: i64 = (*cap_resource.read_unchecked()).unwrap_or(DEFAULT_MAX_MINUTES_PER_DAY);
    let existing_today_minutes: Option<i64> = (*today_total_resource.read_unchecked()).flatten();

    // MAPPS-243: the tenant's own-company id (PMS-413), the company_id a
    // General / overhead entry is attributed to. Sourced from the cached
    // `/auth/me` user. `None` only on a tenant that predates the backfill;
    // in that case the General option is offered but disabled (see below)
    // rather than POSTing a null company_id.
    let own_company_id: Option<uuid::Uuid> =
        auth.read().user.as_ref().and_then(|u| u.own_company_id);

    // MAPPS-274: the Select component already renders its own disabled
    // placeholder option from the `placeholder: "Select work item"` prop
    // below, so seeding a second `("", "Select a work item")` row here
    // produced two near-identical empty entries in the dropdown. Start
    // empty and let the Select's placeholder do that job.
    let mut work_item_options: Vec<SelectOption> = Vec::new();
    // MAPPS-243: a deliberate General (no ticket or project) overhead entry,
    // modeled like the required Work Type field. Offered only when the tenant
    // has an own-company to attribute it to; otherwise the option is disabled
    // (see the inline notice under the picker) so we never send a null
    // company_id.
    work_item_options.push(SelectOption {
        disabled: own_company_id.is_none(),
        ..SelectOption::new("general", "General (no ticket or project)")
    });
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
    // MAPPS-243: explain the disabled "General" option when the tenant has no
    // own-company on file yet (pre-backfill), so the greyed-out row is not a
    // dead end. Empty otherwise (no help text).
    let work_item_help = if own_company_id.is_none() {
        "General (no ticket or project) needs your company on file, which isn't set yet."
    } else {
        ""
    };

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
                        let desc = description.read().trim().to_string();
                        let billable = *is_billable.read();

                        // PMS-518: validate every required field through the shared
                        // FormGuard so all "you forgot to fill X" failures surface at
                        // once (each in its own inline slot) and the first is focused.
                        // Description is now enforced (it carried the asterisk but was
                        // never validated). The cross-field resolution and per-day cap
                        // errors below have no single field, so they stay on the
                        // form-level banner.
                        let mut guard = FormGuard::new();
                        work_item_error
                            .set(guard.field("work_item", &wi, "Work item", &[Rule::Required]));
                        work_type_error
                            .set(guard.field("work_type", &wtid, "Work type", &[Rule::Required]));
                        description_error
                            .set(guard.field("description", &desc, "Description", &[Rule::Required]));

                        // The Hours field is free-text (it accepts H:MM as well as
                        // decimal), so it keeps its custom parse: 0 < t <= 24h (the
                        // hard single-entry bound, MAPPS-244 AC6). It reports through
                        // the guard so it joins the same up-front pass.
                        let duration_minutes =
                            match crate::utils::duration::parse_input_to_minutes(&hrs) {
                                Some(m) if m > 0 && m <= MAX_SINGLE_ENTRY_MINUTES => {
                                    hours_error.set(String::new());
                                    Some(m)
                                }
                                _ => {
                                    hours_error.set(
                                        "Enter time as hours (2.5) or H:MM (1:30), greater than 0 and at most 24h."
                                            .to_string(),
                                    );
                                    guard.note_invalid(Some("hours"));
                                    None
                                }
                            };

                        if guard.blocked() {
                            return;
                        }
                        // Past the guard: Hours parsed to a valid duration.
                        let Some(duration_minutes) = duration_minutes else {
                            return;
                        };
                        // MAPPS-244: pre-flight per-day cap check. Once today's
                        // existing total is known, block (before the network
                        // call) any entry that would push the day over the
                        // configured cap, naming the cap and the minutes left.
                        if let Some(existing) = existing_today_minutes {
                            if existing + duration_minutes > cap_minutes {
                                let remaining = (cap_minutes - existing).max(0);
                                error.set(format!(
                                    "This entry would put today's total over the {}h/day cap. \
                                     You have {} minute(s) left to log today.",
                                    cap_minutes / 60,
                                    remaining,
                                ));
                                return;
                            }
                        }
                        // Resolve the work item into (ticket_id, project_id,
                        // task_id, company_id, work_category). A ticket carries
                        // its company; a project carries its own (required,
                        // which is why the picker only lists projects that have
                        // one). MAPPS-243: a "general" selection carries no work
                        // item and attributes to the tenant's own company; the
                        // server classifies it via work_category (PMS-394).
                        let (ticket_id, project_id, task_id, company_id, work_category) =
                            if wi == "general" {
                                // MAPPS-243: a deliberate overhead entry. The
                                // option is UI-disabled when own_company_id is
                                // None, but re-check here so we never POST a
                                // null company_id (no invented fallback).
                                match own_company_id {
                                    Some(cid) => (None, None, None, cid, "general"),
                                    None => {
                                        error.set(
                                            "General time needs your company on file, which isn't set yet. Pick a ticket or project, or contact an admin."
                                                .to_string(),
                                        );
                                        return;
                                    }
                                }
                            } else if let Some(tid) = wi.strip_prefix("ticket:") {
                                match tickets_for_submit.iter().find(|t| t.id.to_string() == tid) {
                                    Some(t) => {
                                        (Some(tid.to_string()), None, None, t.company_id, "ticketed")
                                    }
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
                                            (None, Some(pid.to_string()), tk, cid, "project")
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
                                    // MAPPS-243 / PMS-394: classify the entry so
                                    // reports split overhead ("general") from
                                    // client-attributable work. "ticketed" with
                                    // a ticket and "project" with a project both
                                    // pass the server's derive_work_category
                                    // consistency check.
                                    "work_category": work_category,
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
                                match crate::hooks::fetch::api::post_authed_typed::<serde_json::Value, _>(
                                    "/time-entries",
                                    &body,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::TimeEntryList {});
                                    }
                                    // MAPPS-244: surface the server's day-cap
                                    // rejection (409/422 from PMS-396, e.g. a
                                    // race past the pre-flight check) and any
                                    // other validation message in the banner.
                                    Err(e) => {
                                        error.set(format!(
                                            "Could not save time entry: {}",
                                            e.user_message()
                                        ));
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
                            rules: vec![Rule::Required],
                            error: work_item_error.read().clone(),
                            help: work_item_help.to_string(),
                            onchange: move |e: FormEvent| {
                                work_item_error.set(String::new());
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
                            rules: vec![Rule::Required],
                            error: work_type_error.read().clone(),
                            onchange: move |e: FormEvent| {
                                work_type_error.set(String::new());
                                work_type.set(e.value());
                            },
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
                        // Validation lives in the submit handler (free-text
                        // parse, not a simple `rules` rule), surfaced inline
                        // via `hours_error` (PMS-518).
                        r#type: "text",
                        placeholder: "2, 2.5, or 1:30",
                        help: "Decimal hours or H:MM.",
                        required: true,
                        error: hours_error.read().clone(),
                        value: hours.read().clone(),
                        oninput: move |e: FormEvent| {
                            hours_error.set(String::new());
                            hours.set(e.value());
                        },
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "What did you work on?",
                        rows: 3,
                        required: true,
                        rules: vec![Rule::Required],
                        error: description_error.read().clone(),
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| {
                            description_error.set(String::new());
                            description.set(e.value());
                        },
                    }

                    crate::components::Checkbox {
                        name: "billable",
                        label: "Billable",
                        checked: *is_billable.read(),
                        help: "Mark this time entry as billable to the customer",
                        // PMS-571: drive state from the event's actual checked
                        // value (re-anchoring to the DOM) instead of inverting
                        // stored state, which could desync a controlled checkbox
                        // so clicks appeared to do nothing. Matches the working
                        // `certified` checkbox.
                        onchange: move |e: FormEvent| is_billable.set(e.checked()),
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
                                    title: if already_approved {
                                        Some("This timesheet has already been approved.".to_string())
                                    } else if !has_entries {
                                        Some("No time logged this week yet.".to_string())
                                    } else {
                                        None
                                    },
                                    onclick: move |_| {
                                        action_msg.set(String::new());
                                        action_err.set(String::new());
                                        certified.set(false);
                                        show_submit_modal.set(true);
                                    },
                                    "Submit Timesheet"
                                }
                                if show_no_entries_hint {
                                    span { class: "text-xs text-muted",
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
                        class: "p-2 text-subtle hover:text-content",
                        title: "Previous week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() - Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                    }
                    div { class: "flex flex-col items-center gap-1",
                        span { class: "text-lg font-medium text-content",
                            "{week_label}"
                        }
                        // PMS-310: one-click return to the current week (only
                        // shown when paged away from it), so a future/empty
                        // week is not a dead end.
                        if !is_current_week {
                            button {
                                r#type: "button",
                                class: "text-xs font-medium text-accent hover:opacity-90",
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
                        class: "p-2 text-subtle hover:text-content",
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
                    table { class: "min-w-full divide-y divide-line",
                        thead { class: "bg-surface-2",
                            tr {
                                th { class: "px-6 py-3 text-left text-xs font-medium text-muted uppercase tracking-wider",
                                    "Work Item"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Mon"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Tue"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Wed"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Thu"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Fri"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Sat"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Sun"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-muted uppercase tracking-wider w-20",
                                    "Total"
                                }
                            }
                        }
                        tbody { class: "bg-surface divide-y divide-line",
                            if is_loading {
                                // PMS-353: shimmer rows (matching the shared
                                // skeleton style) instead of a bare text line.
                                // This timesheet is a bespoke weekly grid, not
                                // the shared Table, so it can't use TableLoading.
                                for _ in 0..4 {
                                    tr {
                                        td { class: "px-6 py-4", colspan: "9",
                                            div { class: "h-4 bg-surface-2 rounded animate-pulse" }
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
                                        p { class: "text-sm text-muted",
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
                                                            class: "font-medium text-accent hover:opacity-90",
                                                            "{label}"
                                                        }
                                                    } else {
                                                        span { class: "text-content", "{label}" }
                                                    }
                                                }
                                                for m in buckets.iter() {
                                                    td { class: "px-4 py-3 text-center text-sm",
                                                        if *m == 0 {
                                                            span { class: "text-subtle", "-" }
                                                        } else {
                                                            span { class: "text-content", "{fmt_hours(*m)}" }
                                                        }
                                                    }
                                                }
                                                td { class: "px-4 py-3 text-center text-sm font-medium text-content",
                                                    "{fmt_hours(row_total)}"
                                                }
                                            }
                                        }
                                    }
                                }
                                tr { class: "bg-surface-2 font-medium",
                                    td { class: "px-6 py-3 text-sm text-content",
                                        "Daily Total"
                                    }
                                    for m in daily_totals.iter() {
                                        td { class: "px-4 py-3 text-center text-sm text-content",
                                            if *m == 0 {
                                                span { class: "text-muted", "0" }
                                            } else {
                                                "{fmt_hours(*m)}"
                                            }
                                        }
                                    }
                                    td { class: "px-4 py-3 text-center text-sm font-bold text-accent",
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
                                title: (!certified())
                                    .then(|| "Check the certification box to submit.".to_string()),
                                onclick: do_submit,
                                "Submit for Approval"
                            }
                        },
                        div { class: "space-y-4",
                            p { class: "text-sm text-content",
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
/// render and act on a row. PMS-506 adds `week_start` (now that the page
/// can span a multi-week range) and the rolled decision audit (so a
/// history row labels who approved/rejected and when, plus the rejection
/// reason).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ApprovalSummary {
    user_id: uuid::Uuid,
    #[serde(default)]
    week_start: Option<NaiveDate>,
    #[serde(default)]
    total_minutes: i64,
    #[serde(default)]
    billable_minutes: i64,
    #[serde(default)]
    entry_count: i64,
    #[serde(default)]
    approval_status: String,
    #[serde(default)]
    decided_by_id: Option<uuid::Uuid>,
    #[serde(default)]
    decided_at: Option<DateTime<Utc>>,
    #[serde(default)]
    rejection_reason: Option<String>,
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
    // Reject modal: the (user_id, week_start, display name) of the row being
    // rejected, plus the required reason and an in-flight flag. PMS-506:
    // the (uid, week) pair is the natural key now that the queue can span
    // multiple weeks in range mode.
    let mut reject_target = use_signal::<Option<(uuid::Uuid, NaiveDate, String)>>(|| None);
    let mut reject_reason = use_signal(String::new);
    let mut is_rejecting = use_signal(|| false);

    // PMS-506: status filter (default `pending` so the action queue stays
    // first-visit) + range mode toggle. `Range` mode swaps the single-week
    // selector for a from/to date pair (default = last 12 weeks).
    let mut status_filter = use_signal(|| "pending".to_string());
    let mut range_mode = use_signal(|| false);
    let mut range_from = use_signal(|| monday_of_week(today) - Duration::weeks(11));
    let mut range_to = use_signal(|| monday_of_week(today));

    // Every user's summary for the selected scope. No `user_id` filter, so the
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
        // PMS-506: build the query off the active mode. Range mode sends
        // ?from=&to= so the server scans the full span in one call;
        // single-week stays on ?week=. Status is always sent (the server
        // treats `all` the same as missing, so the SPA may default to
        // `pending` without changing the legacy contract).
        let status = status_filter.read().clone();
        let path = if *range_mode.read() {
            let from = range_from();
            let to = range_to();
            format!("/timesheets?from={from}&to={to}&status={status}&per_page=200")
        } else {
            let start = week_start();
            format!("/timesheets?week={start}&status={status}&per_page=200")
        };
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
            AppLayout { title: "Timesheet Approvals & History",
                PageHeader {
                    title: "Timesheet Approvals & History",
                    subtitle: "Review submitted timesheets and audit past decisions",
                }
                Card {
                    p { class: "text-sm text-muted",
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

    // PMS-506: server already filters by `status`, so the SPA renders
    // whatever it received. The legacy `pending && entries > 0` guard
    // stays applied only when the status filter is `pending` so an empty
    // never-submitted week does not pollute the action queue.
    let active_status = status_filter.read().clone();
    let rows: Vec<ApprovalSummary> = summaries
        .iter()
        .filter(|s| {
            if active_status == "pending" {
                s.approval_status == "pending" && s.entry_count > 0
            } else {
                true
            }
        })
        .cloned()
        .collect();
    let rows_count = rows.len();
    let in_range_mode = *range_mode.read();
    let header_badge_label = format!("{rows_count} matching");

    let approving_id = *approving.read();
    let msg = action_msg.read().clone();
    let err = action_err.read().clone();

    rsx! {
        AppLayout { title: "Timesheet Approvals & History",
            PageHeader {
                title: "Timesheet Approvals & History",
                subtitle: "Review submitted timesheets and audit past decisions",
                actions: rsx! {
                    Badge { variant: BadgeVariant::Gray, "{header_badge_label}" }
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

            // PMS-506: status filter + range-mode toggle. The status
            // Select defaults to `pending` so the action queue stays the
            // first-visit surface; admins flip to `approved` / `all` to
            // see history. The range checkbox swaps the single-week
            // selector for a from/to date pair pre-seeded to the last
            // 12 weeks.
            Card { class: "mb-4",
                div { class: "flex flex-wrap items-end gap-4",
                    div { class: "min-w-[180px]",
                        Select {
                            name: "ts_status",
                            label: "Status",
                            options: vec![
                                SelectOption::new("pending", "Pending (action queue)"),
                                SelectOption::new("approved", "Approved"),
                                SelectOption::new("rejected", "Rejected"),
                                SelectOption::new("all", "All"),
                            ],
                            value: status_filter.read().clone(),
                            onchange: move |e: FormEvent| {
                                action_msg.set(String::new());
                                action_err.set(String::new());
                                status_filter.set(e.value());
                            },
                        }
                    }
                    div { class: "pt-6",
                        Checkbox {
                            name: "ts_range_mode",
                            label: "Range mode",
                            checked: in_range_mode,
                            help: "Span multiple weeks. Server caps the scan at 26 weeks.",
                            onchange: move |e: FormEvent| {
                                action_msg.set(String::new());
                                action_err.set(String::new());
                                range_mode.set(e.checked());
                            },
                        }
                    }
                    if in_range_mode {
                        div { class: "min-w-[160px]",
                            crate::components::Input {
                                name: "ts_from",
                                label: "From",
                                r#type: "date".to_string(),
                                value: range_from().format("%Y-%m-%d").to_string(),
                                oninput: move |e: FormEvent| {
                                    if let Ok(d) = NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d") {
                                        range_from.set(monday_of_week(d));
                                    }
                                },
                            }
                        }
                        div { class: "min-w-[160px]",
                            crate::components::Input {
                                name: "ts_to",
                                label: "To",
                                r#type: "date".to_string(),
                                value: range_to().format("%Y-%m-%d").to_string(),
                                oninput: move |e: FormEvent| {
                                    if let Ok(d) = NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d") {
                                        range_to.set(monday_of_week(d));
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // Single-week selector. Hidden in range mode (the from/to
            // date inputs above take its place).
            if !in_range_mode {
            Card { class: "mb-6",
                div { class: "flex items-center justify-between",
                    button {
                        r#type: "button",
                        class: "p-2 text-subtle hover:text-content",
                        title: "Previous week",
                        onclick: move |_| {
                            action_msg.set(String::new());
                            action_err.set(String::new());
                            week_start.set(week_start() - Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                    }
                    div { class: "flex flex-col items-center gap-1",
                        span { class: "text-lg font-medium text-content",
                            "{week_label}"
                        }
                        if !is_current_week {
                            button {
                                r#type: "button",
                                class: "text-xs font-medium text-accent hover:opacity-90",
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
                        class: "p-2 text-subtle hover:text-content",
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
            } // PMS-506: end `if !in_range_mode` single-week selector.

            DataTable {
                total_items: rows_count,
                current_page: 1,
                per_page: if rows_count == 0 { 25 } else { rows_count },
                columns: 7,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Employee" }
                            TableHeader { "Week" }
                            TableHeader { "Status" }
                            TableHeader { "Total" }
                            TableHeader { "Billable" }
                            TableHeader { "Decision" }
                            TableHeader { "" }
                        }
                    }
                    TableBody {
                        if is_loading {
                            TableRow {
                                TableCell { class: "text-subtle", "Loading…" }
                            }
                        } else if load_failed {
                            TableRow {
                                TableCell { class: "text-red-500",
                                    "Could not load timesheets. The time-tracking service may be unavailable."
                                }
                            }
                        } else if rows.is_empty() {
                            TableRow {
                                TableCell { class: "text-subtle italic",
                                    if active_status == "pending" {
                                        "No timesheets awaiting approval for this scope."
                                    } else {
                                        "No timesheets match the current filter."
                                    }
                                }
                            }
                        } else {
                            for s in rows.iter() {
                                {
                                    let uid = s.user_id;
                                    let row_week = s.week_start.unwrap_or(start);
                                    let name = name_of(uid);
                                    let name_for_reject = name.clone();
                                    let total = fmt_hours(s.total_minutes);
                                    let billable = fmt_hours(s.billable_minutes);
                                    let row_busy = approving_id == Some(uid) || is_rejecting();
                                    let row_pending = s.approval_status == "pending";
                                    let (status_variant, status_label) = match s.approval_status.as_str() {
                                        "approved" => (BadgeVariant::Green, "Approved"),
                                        "rejected" => (BadgeVariant::Red, "Rejected"),
                                        "pending" => (BadgeVariant::Yellow, "Pending"),
                                        _ => (BadgeVariant::Gray, "-"),
                                    };
                                    let week_label_row = row_week.format("%b %-d, %Y").to_string();
                                    let decided_at_label = s
                                        .decided_at
                                        .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                        .unwrap_or_default();
                                    let decided_by_label = s
                                        .decided_by_id
                                        .map(name_of)
                                        .unwrap_or_default();
                                    let rejection = s.rejection_reason.clone().unwrap_or_default();
                                    let row_key = format!("{uid}-{row_week}");
                                    rsx! {
                                        TableRow { key: "{row_key}",
                                            TableCell { class: "font-medium text-content", "{name}" }
                                            TableCell { class: "text-muted", "{week_label_row}" }
                                            TableCell { Badge { variant: status_variant, "{status_label}" } }
                                            TableCell { "{total}" }
                                            TableCell { class: "text-green-600", "{billable}" }
                                            TableCell { class: "text-xs text-muted",
                                                if row_pending {
                                                    "-"
                                                } else {
                                                    if !decided_by_label.is_empty() {
                                                        div { "by {decided_by_label}" }
                                                    }
                                                    if !decided_at_label.is_empty() {
                                                        div { "{decided_at_label}" }
                                                    }
                                                    if !rejection.is_empty() {
                                                        div { class: "italic mt-1", "\"{rejection}\"" }
                                                    }
                                                }
                                            }
                                            TableCell {
                                                if row_pending {
                                                    div { class: "flex justify-end gap-2",
                                                        Button {
                                                            variant: ButtonVariant::Secondary,
                                                            disabled: row_busy,
                                                            onclick: move |_| {
                                                                action_msg.set(String::new());
                                                                action_err.set(String::new());
                                                                reject_reason.set(String::new());
                                                                reject_target.set(Some((uid, row_week, name_for_reject.clone())));
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
                                                                let mut sr = summaries_resource;
                                                                spawn(async move {
                                                                    #[cfg(feature = "web")]
                                                                    {
                                                                        let path = format!("/timesheets/{uid}/{row_week}/approve");
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
            } // PMS-506: close DataTable

            // Reject reason modal (MAPPS-189: in-app modal, not window.prompt).
            {
                let target = reject_target.read().clone();
                let open = target.is_some();
                let reject_name = target
                    .as_ref()
                    .map(|(_, _, n)| n.clone())
                    .unwrap_or_default();
                let rejecting = is_rejecting();
                let reason_empty = reject_reason.read().trim().is_empty();
                let do_reject = move |_| {
                    let Some((uid, week, _)) = reject_target.read().clone() else {
                        return;
                    };
                    let reason = reject_reason.read().trim().to_string();
                    if reason.is_empty() || rejecting {
                        return;
                    }
                    is_rejecting.set(true);
                    let mut sr = summaries_resource;
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let path = format!("/timesheets/{uid}/{week}/reject");
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
                                title: reason_empty
                                    .then(|| "Enter a reason to reject this timesheet.".to_string()),
                                onclick: do_reject,
                                "Reject Timesheet"
                            }
                        },
                        div { class: "space-y-4",
                            p { class: "text-sm text-content",
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
    // PMS-518: per-field inline error slots fed by the FormGuard in
    // handle_save. The form-level `error` banner is kept for the server
    // save/delete failures, which have no single field to attach to.
    let mut work_type_error = use_signal(String::new);
    let mut hours_error = use_signal(String::new);
    let mut date_error = use_signal(String::new);

    let wi_label = work_item_label(&entry);

    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());
        let wtid = work_type.read().trim().to_string();
        let d = date.read().trim().to_string();
        let hrs = hours.read().clone();

        // PMS-518: validate every required field through the shared FormGuard
        // so all "you forgot to fill X" failures surface at once (each in its
        // own inline slot) and the first invalid field is focused.
        let mut guard = FormGuard::new();
        work_type_error.set(guard.field("edit_work_type", &wtid, "Work type", &[Rule::Required]));
        date_error.set(guard.field("edit_date", &d, "Date", &[Rule::Required]));

        // Hours is free-text (it accepts H:MM as well as decimal), so it keeps
        // its custom parse: 0 < t <= 24h. It reports through the guard so it
        // joins the same up-front pass, surfaced inline via `hours_error`.
        let duration_minutes = match crate::utils::duration::parse_input_to_minutes(&hrs) {
            Some(m) if m > 0 && m <= 24 * 60 => {
                hours_error.set(String::new());
                Some(m)
            }
            _ => {
                hours_error.set(
                    "Enter time as hours (2.5) or H:MM (1:30), greater than 0 and at most 24h."
                        .to_string(),
                );
                guard.note_invalid(Some("edit_hours"));
                None
            }
        };

        if guard.blocked() {
            return;
        }
        // Past the guard: Hours parsed to a valid duration.
        let Some(duration_minutes) = duration_minutes else {
            return;
        };
        saving.set(true);
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
                p { class: "text-xs text-muted",
                    "Work item: {wi_label}. To move this entry to a different work item, delete it and log again."
                }
                Select {
                    name: "edit_work_type",
                    label: "Work Type",
                    options: work_type_options,
                    value: work_type.read().clone(),
                    required: true,
                    rules: vec![Rule::Required],
                    error: work_type_error.read().clone(),
                    onchange: move |e: FormEvent| {
                        work_type_error.set(String::new());
                        work_type.set(e.value());
                    },
                }
                crate::components::Input {
                    name: "edit_hours",
                    label: "Hours",
                    // Free-text so H:MM (e.g. "0:30") can be typed; a
                    // type="number" input blocks the colon. PMS-314.
                    // Validation lives in the save handler (free-text parse,
                    // not a simple `rules` rule), surfaced inline via
                    // `hours_error` (PMS-518). Enforces 0 < t <= 24h.
                    r#type: "text",
                    placeholder: "2, 2.5, or 1:30",
                    help: "Decimal hours or H:MM.",
                    required: true,
                    error: hours_error.read().clone(),
                    value: hours.read().clone(),
                    oninput: move |e: FormEvent| {
                        hours_error.set(String::new());
                        hours.set(e.value());
                    },
                }
                crate::components::DateField {
                    name: "edit_date",
                    label: "Date",
                    required: true,
                    rules: vec![Rule::Required],
                    error: date_error.read().clone(),
                    value: date.read().clone(),
                    oninput: move |e: FormEvent| {
                        date_error.set(String::new());
                        date.set(e.value());
                    },
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
                    // PMS-571: re-anchor to the event's checked value (see the
                    // create-form billable checkbox) so the toggle is reliable.
                    onchange: move |e: FormEvent| is_billable.set(e.checked()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare entry with everything unset; each test overrides only the
    /// fields it exercises so `work_item_label`'s branch under test is clear.
    fn entry() -> RemoteTimeEntry {
        RemoteTimeEntry {
            id: uuid::Uuid::nil(),
            date: NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
            duration_minutes: 60,
            work_type_id: None,
            ticket_id: None,
            project_id: None,
            work_category: None,
            ticket_number: None,
            ticket_title: None,
            project_name: None,
            task_title: None,
            notes: None,
            is_billable: false,
            billing_status: String::new(),
        }
    }

    // MAPPS-243: a ticketless entry the server classified as "general" reads
    // as "General", not the bare "-" used for a truly unlinked entry.
    #[test]
    fn work_item_label_general_entry_reads_general() {
        let e = RemoteTimeEntry {
            work_category: Some("general".to_string()),
            ..entry()
        };
        assert_eq!(work_item_label(&e), "General");
    }

    // An entry with no work item and no category stays "-" (no regression):
    // "General" is reserved for the explicit server classification.
    #[test]
    fn work_item_label_unlinked_entry_reads_dash() {
        assert_eq!(work_item_label(&entry()), "-");
    }

    // Ticket and project labels are unchanged by the general branch.
    #[test]
    fn work_item_label_ticket_and_project_unchanged() {
        let ticketed = RemoteTimeEntry {
            ticket_id: Some(uuid::Uuid::nil()),
            ticket_number: Some("123".to_string()),
            ticket_title: Some("Fix login".to_string()),
            // A real ticketed entry also carries the category; the label must
            // still prefer the ticket, not fall through to "General".
            work_category: Some("ticketed".to_string()),
            ..entry()
        };
        assert_eq!(work_item_label(&ticketed), "Ticket 123: Fix login");

        let project = RemoteTimeEntry {
            project_id: Some(uuid::Uuid::nil()),
            project_name: Some("Migration".to_string()),
            task_title: Some("Cutover".to_string()),
            work_category: Some("project".to_string()),
            ..entry()
        };
        assert_eq!(work_item_label(&project), "Migration · Cutover");
    }
}

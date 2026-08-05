//! Time tracking pages

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Checkbox,
    ChevronRightIcon, DataTable, ErrorBanner, IconSize, Modal, PageHeader, PlusIcon, Select,
    SelectOption, StatCard, Table, TableAlign, TableBody, TableCell, TableEmptyRow, TableHead,
    TableHeader, TableRow,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;
// MAPPS-383: the enum behind `TimeEntryResponse.billing_status`.
// `mokosh_types::time_tracking` imports it privately, so it is reached here
// through its defining module.
use mokosh_types::tickets::BillingStatus;

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
    // MAPPS-383: the shared enum, not a free `String`, so an unknown wire tag
    // fails decoding instead of reaching the UI as raw text.
    #[serde(default)]
    billing_status: BillingStatus,
    // MAPPS-340: creation / last-edit timestamps (server `TimeEntryResponse`
    // always carries them; `Option` tolerates an older server that omits
    // them). The approvals history derives a "Logged" event from `created_at`
    // and a "Edited" event when `updated_at` is strictly later.
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
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

/// Human label for the shared `BillingStatus`. MAPPS-383: the entry row used
/// to print the raw wire tag, so a billed-ready entry read "ready_to_bill".
fn billing_status_label(status: BillingStatus) -> &'static str {
    match status {
        BillingStatus::NotBilled => "Not Billed",
        BillingStatus::ReadyToBill => "Ready to Bill",
        BillingStatus::Billed => "Billed",
    }
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
    use_page_title("Time Entries");
    let mut entries_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
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

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (which keeps the
    // nav + banner) instead of an empty time-entry table. A fetch that fails
    // while still reachable (a 4xx) keeps the inline notice + empty state below.
    let reachable = crate::hooks::use_server_reachable();
    if load_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Time Entries".to_string() }
        };
    }

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

        div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-6",
            StatCard { label: "Today", value: "{today_h}" }
            StatCard { label: "This Week", value: "{week_h}" }
            StatCard { label: "Billable", value: "{billable_h}" }
            StatCard { label: "Non-Billable", value: "{nonbillable_h}" }
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
                        // MAPPS-388: centered across the table, not left-aligned.
                        TableEmptyRow { columns: 6, class: "text-subtle italic", "No time logged yet." }
                    } else {
                        for e in entries.iter() {
                            {
                                let hrs = hours(e.duration_minutes);
                                let note = e
                                    .notes
                                    .clone()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| "-".to_string());
                                let status = e.billing_status;
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
                                            if status != BillingStatus::NotBilled {
                                                span { class: "ml-2 text-xs text-subtle", "{billing_status_label(status)}" }
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
    use_page_title("Log Time");
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

    // MAPPS-357: N/A for ContentUnavailable - this is a write (create) form, not
    // a data-display page. The tickets / projects / work-types / tasks resources
    // above are all SECONDARY lookups that degrade to empty dropdowns, so there
    // is no primary resource whose absence means "no meaningful content". We only
    // gate the write: block the submit while the server is unreachable so a click
    // can't silently fail (`can_mutate` re-enables itself on reconnect).
    let can_mutate = crate::hooks::use_can_mutate();

    rsx! {
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
                    ErrorBanner { "{err}" }
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
                        // MAPPS-357: block submit while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        "Save Time Entry"
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
    use_page_title("Timesheets");
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
        // MAPPS-357: subscribe to reachability so the week auto-refetches when
        // the server returns (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
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

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty week - render the honest unavailable state (keeping the nav
    // + banner) instead of an empty timesheet grid. A fetch that fails while
    // still reachable keeps the inline "Could not load" row below. Writes are
    // blocked while down; `can_mutate` disables Submit / Withdraw.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if load_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Timesheets".to_string() }
        };
    }

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
                            // MAPPS-357: block withdraw while the server is down.
                            disabled: !can_mutate,
                            title: (!can_mutate).then(|| "Can't withdraw while the server is unreachable".to_string()),
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
                                // MAPPS-357: also block while the server is down.
                                disabled: already_approved || !has_entries || !can_mutate,
                                title: if already_approved {
                                    Some("This timesheet has already been approved.".to_string())
                                } else if !has_entries {
                                    Some("No time logged this week yet.".to_string())
                                } else if !can_mutate {
                                    Some("Can't submit while the server is unreachable".to_string())
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
            ErrorBanner { class: "mb-4", "{err}" }
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
                        Button {
                            variant: ButtonVariant::Link,
                            size: ButtonSize::Small,
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
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Work Item" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Mon" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Tue" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Wed" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Thu" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Fri" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Sat" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Sun" }
                        TableHeader { align: TableAlign::Center, compact: true, class: "w-20", "Total" }
                    }
                }
                TableBody {
                    if is_loading {
                        // PMS-353: shimmer rows in the shared grid's empty body.
                        for _ in 0..4 {
                            TableRow {
                                TableCell { colspan: Some(9),
                                    div { class: "h-4 bg-surface-2 rounded animate-pulse" }
                                }
                            }
                        }
                    } else if load_failed {
                        TableRow {
                            TableCell { colspan: Some(9), class: "text-center text-red-500",
                                "Could not load timesheet. The time-tracking service may be unavailable."
                            }
                        }
                    } else if !has_entries {
                        // MAPPS-201: empty-week prompt + one-click path to a new
                        // entry for the week. Submit stays disabled until one exists.
                        TableRow {
                            TableCell { colspan: Some(9), class: "text-center",
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
                                    TableRow {
                                        TableCell {
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
                                            TableCell { compact: true, class: "text-center",
                                                if *m == 0 {
                                                    span { class: "text-subtle", "-" }
                                                } else {
                                                    span { class: "text-content", "{fmt_hours(*m)}" }
                                                }
                                            }
                                        }
                                        TableCell { compact: true, class: "text-center font-medium",
                                            "{fmt_hours(row_total)}"
                                        }
                                    }
                                }
                            }
                        }
                        TableRow { class: "bg-surface-2 font-medium",
                            TableCell { "Daily Total" }
                            for m in daily_totals.iter() {
                                TableCell { compact: true, class: "text-center",
                                    if *m == 0 {
                                        span { class: "text-muted", "0" }
                                    } else {
                                        "{fmt_hours(*m)}"
                                    }
                                }
                            }
                            TableCell { compact: true, class: "text-center font-bold text-accent",
                                "{fmt_hours(grand_total)}"
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
                            // MAPPS-357: also block while the server is down.
                            disabled: !certified() || !can_mutate,
                            title: if !can_mutate {
                                Some("Can't submit while the server is unreachable".to_string())
                            } else {
                                (!certified())
                                    .then(|| "Check the certification box to submit.".to_string())
                            },
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

// ============================================================================
// MAPPS-340: reviewable submit / change / approve / reject history.
//
// A timesheet is a week aggregation over `time_entries`, so its history is
// composed client-side from data the server already exposes: each entry's
// `created_at` (a "Logged" event) and `updated_at` (an "Edited" event when it
// is strictly later), plus the week's rolled-up approve/reject decision. No new
// endpoint is needed - the reviewable sequence falls out of the existing
// `/time-entries` and `/timesheets` responses.
// ============================================================================

/// One event in a timesheet's reviewable history.
#[derive(Clone, Debug, PartialEq)]
struct HistoryEvent {
    at: DateTime<Utc>,
    kind: HistoryKind,
    /// Who performed the action (the employee for log/edit events, the
    /// approver for the decision event).
    actor: String,
    /// Short human description: the work item for a log/edit event, the
    /// rejection reason (if any) for a decision event.
    detail: String,
    /// Duration in minutes for a `Logged` event, so the view can render the
    /// hours in the user's format. `None` for edit/decision events. Kept as
    /// raw minutes (not a formatted string) so this builder stays free of the
    /// localStorage-backed duration-format pref and is unit-testable natively.
    minutes: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryKind {
    Logged,
    Edited,
    Approved,
    Rejected,
}

impl HistoryKind {
    fn label(self) -> &'static str {
        match self {
            HistoryKind::Logged => "Logged",
            HistoryKind::Edited => "Edited",
            HistoryKind::Approved => "Approved",
            HistoryKind::Rejected => "Rejected",
        }
    }

    fn variant(self) -> BadgeVariant {
        match self {
            HistoryKind::Logged => BadgeVariant::Gray,
            HistoryKind::Edited => BadgeVariant::Yellow,
            HistoryKind::Approved => BadgeVariant::Green,
            HistoryKind::Rejected => BadgeVariant::Red,
        }
    }
}

/// The week's rolled-up approve/reject decision, as the approvals summary
/// already carries it. Fed into `build_timesheet_history` only when the week
/// is actually decided.
#[derive(Clone, Debug, PartialEq)]
struct DecisionEvent {
    /// "approved" | "rejected".
    status: String,
    at: DateTime<Utc>,
    actor: String,
    reason: Option<String>,
}

/// Build the chronological submit / change / approve / reject history for one
/// timesheet week (MAPPS-340). Each entry contributes a `Logged` event at its
/// `created_at` and, when `updated_at` is strictly later, an `Edited` event;
/// the decision (if any) contributes the terminal `Approved`/`Rejected` event.
/// Events are returned oldest-first so the reader follows the progression.
fn build_timesheet_history(
    entries: &[RemoteTimeEntry],
    employee: &str,
    decision: Option<DecisionEvent>,
) -> Vec<HistoryEvent> {
    let mut events: Vec<HistoryEvent> = Vec::new();
    for e in entries {
        let label = work_item_label(e);
        if let Some(created) = e.created_at {
            events.push(HistoryEvent {
                at: created,
                kind: HistoryKind::Logged,
                actor: employee.to_string(),
                detail: label.clone(),
                minutes: Some(e.duration_minutes),
            });
        }
        // An `updated_at` strictly after `created_at` means the entry was
        // changed after it was first logged: surface that as a change event.
        if let (Some(created), Some(updated)) = (e.created_at, e.updated_at) {
            if updated > created {
                events.push(HistoryEvent {
                    at: updated,
                    kind: HistoryKind::Edited,
                    actor: employee.to_string(),
                    detail: label,
                    minutes: None,
                });
            }
        }
    }
    if let Some(d) = decision {
        let kind = if d.status == "rejected" {
            HistoryKind::Rejected
        } else {
            HistoryKind::Approved
        };
        let detail = d
            .reason
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .unwrap_or_default();
        events.push(HistoryEvent {
            at: d.at,
            kind,
            actor: if d.actor.trim().is_empty() {
                "Unknown reviewer".to_string()
            } else {
                d.actor
            },
            detail,
            minutes: None,
        });
    }
    // Stable oldest-first sort; equal timestamps keep insertion order (a
    // entry's Logged before its Edited).
    events.sort_by(|a, b| a.at.cmp(&b.at));
    events
}

/// A tenant rounding rule (`GET /time-rounding-rules`), one of the applicable
/// "timesheet rules" surfaced alongside the approvals history (MAPPS-340).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RoundingRuleRow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    increment_minutes: i32,
    #[serde(default)]
    rounding_method: String,
    #[serde(default)]
    minimum_minutes: i32,
    #[serde(default)]
    is_default: bool,
}

/// The row a manager clicked "History" on: the natural (user, week) key plus
/// the decision fields already resolved for display (MAPPS-340).
#[derive(Clone, Debug, PartialEq)]
struct HistoryTarget {
    uid: uuid::Uuid,
    week: NaiveDate,
    employee: String,
    /// "pending" | "approved" | "rejected".
    status: String,
    decided_at: Option<DateTime<Utc>>,
    decided_by: String,
    rejection_reason: Option<String>,
}

/// Manager/admin timesheet approvals queue.
#[component]
pub fn TimesheetApprovalsPage() -> Element {
    use_page_title("Timesheet Approvals & History");
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
    // MAPPS-340: the row whose reviewable history modal is open (None = closed).
    let mut history_target = use_signal::<Option<HistoryTarget>>(|| None);

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
        // MAPPS-357: subscribe to reachability so the queue auto-refetches when
        // the server returns (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
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

    // MAPPS-377: read reachability before the manager / outage early returns
    // below so the hook set stays stable across renders. Both only read global
    // signals and take no post-return state.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();

    if !can_manage {
        return rsx! {
            PageHeader {
                title: "Timesheet Approvals & History",
                subtitle: "Review submitted timesheets and audit past decisions",
            }
            Card {
                p { class: "text-sm text-muted",
                    "You need a manager or admin role to review timesheets."
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

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty queue - render the honest unavailable state (keeping the nav
    // + banner) instead of an empty approvals table. A fetch that fails while
    // still reachable keeps the inline "Could not load" row below. Writes are
    // blocked while down; `can_mutate` disables Approve / Reject.
    if load_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Timesheet Approvals & History".to_string() }
        };
    }

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
            ErrorBanner { class: "mb-4", "{err}" }
        }

        // PMS-506: status filter + range-mode toggle. The status
        // Select defaults to `pending` so the action queue stays the
        // first-visit surface; admins flip to `approved` / `all` to
        // see history. The range checkbox swaps the single-week
        // selector for a from/to date pair pre-seeded to the last
        // 12 weeks.
        Card { class: "mb-6",
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
                        Button {
                            variant: ButtonVariant::Link,
                            size: ButtonSize::Small,
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
                        // MAPPS-388: centered across the table, not left-aligned.
                        TableEmptyRow { columns: 7, class: "text-subtle italic",
                            if active_status == "pending" {
                                "No timesheets awaiting approval for this scope."
                            } else {
                                "No timesheets match the current filter."
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
                                // MAPPS-340: everything the History modal needs, cloned out
                                // of the row before it is moved into the button handler.
                                let name_for_history = name.clone();
                                let history_status = s.approval_status.clone();
                                let history_decided_at = s.decided_at;
                                let history_decided_by = decided_by_label.clone();
                                let history_reason = s.rejection_reason.clone();
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
                                            div { class: "flex justify-end gap-2",
                                                // MAPPS-340: History opens the reviewable
                                                // submit/change/approve/reject timeline. It is a
                                                // read-only view, so it stays enabled while the
                                                // server is down (it fetches on open) and is
                                                // present on decided rows too, so an approved
                                                // timesheet's history stays reachable.
                                                Button {
                                                    variant: ButtonVariant::Secondary,
                                                    onclick: move |_| {
                                                        action_msg.set(String::new());
                                                        action_err.set(String::new());
                                                        history_target
                                                            .set(
                                                                Some(HistoryTarget {
                                                                    uid,
                                                                    week: row_week,
                                                                    employee: name_for_history.clone(),
                                                                    status: history_status.clone(),
                                                                    decided_at: history_decided_at,
                                                                    decided_by: history_decided_by.clone(),
                                                                    rejection_reason: history_reason.clone(),
                                                                }),
                                                            );
                                                    },
                                                    "History"
                                                }
                                                if row_pending {
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        // MAPPS-357: also block while the server is down.
                                                        disabled: row_busy || !can_mutate,
                                                        title: (!can_mutate).then(|| "Can't reject while the server is unreachable".to_string()),
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
                                                        // MAPPS-357: also block while the server is down.
                                                        disabled: row_busy || !can_mutate,
                                                        title: (!can_mutate).then(|| "Can't approve while the server is unreachable".to_string()),
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
                            // MAPPS-357: also block while the server is down.
                            disabled: reason_empty || !can_mutate,
                            title: if !can_mutate {
                                Some("Can't reject while the server is unreachable".to_string())
                            } else {
                                reason_empty
                                    .then(|| "Enter a reason to reject this timesheet.".to_string())
                            },
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

        // MAPPS-340: reviewable submit/change/approve/reject history for the
        // clicked row. Mounted on demand so the entry + rules fetches only
        // fire when a manager actually opens the timeline.
        if let Some(t) = history_target.read().clone() {
            TimesheetHistoryModal {
                uid: t.uid,
                week: t.week,
                employee: t.employee.clone(),
                status: t.status.clone(),
                decided_at: t.decided_at,
                decided_by: t.decided_by.clone(),
                rejection_reason: t.rejection_reason.clone(),
                onclose: move |_| history_target.set(None),
            }
        }
    }
}

/// MAPPS-340: the reviewable-history modal for one (user, week) timesheet.
/// Fetches the week's entries to derive the submit/change timeline, folds in
/// the rolled-up decision, and shows the applicable timesheet rules (per-day
/// hours cap + rounding rules) alongside it. Read-only: no writes here.
#[derive(Props, Clone, PartialEq)]
struct TimesheetHistoryModalProps {
    uid: uuid::Uuid,
    week: NaiveDate,
    employee: String,
    /// "pending" | "approved" | "rejected".
    status: String,
    decided_at: Option<DateTime<Utc>>,
    decided_by: String,
    rejection_reason: Option<String>,
    onclose: EventHandler<()>,
}

#[component]
fn TimesheetHistoryModal(props: TimesheetHistoryModalProps) -> Element {
    let uid = props.uid;
    let week = props.week;
    let employee = props.employee.clone();
    let onclose = props.onclose;

    // The week's entries drive the Logged/Edited events.
    let entries_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let end = week + Duration::days(6);
        let path =
            format!("/time-entries?user_id={uid}&date_from={week}&date_to={end}&per_page=500");
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>(&path)
            .await
            .ok()
            .map(|p| p.data)
    });

    // AC2: the applicable per-day max-hours cap (a JSON integer setting;
    // MAPPS-244). A missing setting (404) simply omits the line.
    let cap_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<SettingValueRow>(
            "/settings/time_tracking/max_hours_per_day",
        )
        .await
        .ok()
        .and_then(|row| row.value.as_i64())
        .filter(|h| (1..=24).contains(h))
    });

    // AC2: the tenant rounding rules. The endpoint returns a bare array today;
    // decode to a Value first so a future paginated `{data:[...]}` shape is
    // tolerated too. Any failure yields an empty list (the panel omits it).
    let rules_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let v = crate::hooks::fetch::api::get_authed::<serde_json::Value>(
            "/time-rounding-rules?per_page=100",
        )
        .await
        .ok()?;
        let arr = v
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .or_else(|| v.as_array().cloned())?;
        Some(
            arr.into_iter()
                .filter_map(|item| serde_json::from_value::<RoundingRuleRow>(item).ok())
                .collect::<Vec<_>>(),
        )
    });

    let entries_snap = entries_resource.read_unchecked();
    let entries_loading = entries_snap.is_none();
    let entries_failed = matches!(&*entries_snap, Some(None));
    let entries: Vec<RemoteTimeEntry> = match &*entries_snap {
        Some(Some(v)) => v.clone(),
        _ => Vec::new(),
    };

    // A decided week folds its approve/reject into the timeline; a pending
    // week has no terminal event yet.
    let decision = match props.status.as_str() {
        "approved" | "rejected" => props.decided_at.map(|at| DecisionEvent {
            status: props.status.clone(),
            at,
            actor: props.decided_by.clone(),
            reason: props.rejection_reason.clone(),
        }),
        _ => None,
    };
    let events = build_timesheet_history(&entries, &employee, decision);

    let cap = (*cap_resource.read_unchecked()).flatten();
    let rules = rules_resource
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();
    let week_label = week.format("%b %-d, %Y").to_string();

    rsx! {
        Modal {
            open: true,
            title: "Timesheet History",
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Close"
                }
            },
            div { class: "space-y-5",
                p { class: "text-sm text-content",
                    strong { "{employee}" }
                    " - week of {week_label}"
                }

                // AC1: chronological submit/change/approve/reject timeline.
                div {
                    h3 { class: "text-sm font-semibold text-content mb-2", "History" }
                    if entries_loading {
                        p { class: "text-sm text-muted", "Loading history…" }
                    } else {
                        if entries_failed {
                            p { class: "text-sm text-red-600 dark:text-red-400",
                                "Could not load the week's entries; showing the decision only."
                            }
                        }
                        if events.is_empty() {
                            p { class: "text-sm text-muted italic",
                                "No history recorded for this timesheet yet."
                            }
                        } else {
                            ol { class: "space-y-3",
                                for (i , ev) in events.iter().enumerate() {
                                    {
                                        let when = ev.at.format("%b %-d, %Y %H:%M UTC").to_string();
                                        let kind = ev.kind;
                                        let actor = ev.actor.clone();
                                        // Fold the work item + (for a log event) the hours in
                                        // the user's duration format into one detail string.
                                        let detail = match ev.minutes {
                                            Some(m) if !ev.detail.is_empty() => {
                                                format!("{} ({})", ev.detail, fmt_hours(m))
                                            }
                                            Some(m) => fmt_hours(m),
                                            None => ev.detail.clone(),
                                        };
                                        rsx! {
                                            li { key: "{i}", class: "flex items-start gap-3",
                                                Badge { variant: kind.variant(), "{kind.label()}" }
                                                div { class: "min-w-0",
                                                    p { class: "text-sm text-content",
                                                        span { class: "font-medium", "{actor}" }
                                                        if !detail.is_empty() {
                                                            " - {detail}"
                                                        }
                                                    }
                                                    p { class: "text-xs text-subtle", "{when}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // AC2: applicable timesheet rules alongside the history.
                div { class: "border-t border-line pt-4",
                    h3 { class: "text-sm font-semibold text-content mb-2", "Timesheet rules" }
                    if cap.is_none() && rules.is_empty() {
                        p { class: "text-sm text-muted italic",
                            "No timesheet rules configured for this tenant."
                        }
                    } else {
                        ul { class: "space-y-1 text-sm text-muted",
                            if let Some(h) = cap {
                                li { "Maximum {h}h logged per day." }
                            }
                            for (i , r) in rules.iter().enumerate() {
                                {
                                    let default_marker = if r.is_default { " (default)" } else { "" };
                                    let name = if r.name.trim().is_empty() {
                                        "Rounding rule".to_string()
                                    } else {
                                        r.name.clone()
                                    };
                                    let method = if r.rounding_method.trim().is_empty() {
                                        "nearest".to_string()
                                    } else {
                                        r.rounding_method.clone()
                                    };
                                    let inc = r.increment_minutes;
                                    let min = r.minimum_minutes;
                                    rsx! {
                                        li { key: "rule-{i}",
                                            "{name}{default_marker}: round {method} to {inc} min"
                                            if min > 0 {
                                                ", minimum {min} min"
                                            }
                                            "."
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

    // MAPPS-357: block save / delete while the server is unreachable so a click
    // can't silently fail. This modal is opened from TimeEntryListPage, whose
    // primary-resource outage state renders ContentUnavailable; this component
    // is a write surface with no primary resource of its own, so it only gates
    // the writes (`can_mutate` re-enables itself on reconnect).
    let can_mutate = crate::hooks::use_can_mutate();

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
                    // MAPPS-357: block delete while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
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
                    // MAPPS-357: block save while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                    onclick: handle_save,
                    "Save Changes"
                }
            },
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
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
            billing_status: BillingStatus::NotBilled,
            created_at: None,
            updated_at: None,
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

    // ---- MAPPS-340: reviewable history assembly ---------------------------

    fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
            .and_utc()
    }

    // A logged-then-approved timesheet yields two events, oldest-first, with
    // the employee as the log actor and the reviewer on the decision.
    #[test]
    fn history_logs_then_approves_in_order() {
        let e = RemoteTimeEntry {
            created_at: Some(ts(2026, 6, 15, 9, 0)),
            updated_at: Some(ts(2026, 6, 15, 9, 0)),
            ..entry()
        };
        let decision = Some(DecisionEvent {
            status: "approved".to_string(),
            at: ts(2026, 6, 18, 14, 30),
            actor: "Manager Meg".to_string(),
            reason: None,
        });
        let h = build_timesheet_history(&[e], "Employee Ed", decision);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].kind, HistoryKind::Logged);
        assert_eq!(h[0].actor, "Employee Ed");
        // The log event carries the raw minutes for the view to format.
        assert_eq!(h[0].minutes, Some(60));
        assert_eq!(h[1].kind, HistoryKind::Approved);
        assert_eq!(h[1].minutes, None);
        assert_eq!(h[1].actor, "Manager Meg");
        // Oldest-first.
        assert!(h[0].at < h[1].at);
    }

    // An entry whose `updated_at` is strictly later than `created_at` emits a
    // distinct "Edited" change event between the log and the decision.
    #[test]
    fn history_surfaces_an_edit_as_a_change_event() {
        let e = RemoteTimeEntry {
            created_at: Some(ts(2026, 6, 15, 9, 0)),
            updated_at: Some(ts(2026, 6, 16, 11, 0)),
            ..entry()
        };
        let decision = Some(DecisionEvent {
            status: "rejected".to_string(),
            at: ts(2026, 6, 17, 8, 0),
            actor: "Manager Meg".to_string(),
            reason: Some("Fix Tuesday".to_string()),
        });
        let h = build_timesheet_history(&[e], "Employee Ed", decision);
        let kinds: Vec<HistoryKind> = h.iter().map(|ev| ev.kind).collect();
        assert_eq!(
            kinds,
            vec![
                HistoryKind::Logged,
                HistoryKind::Edited,
                HistoryKind::Rejected
            ]
        );
        // The rejection reason rides along on the decision event's detail.
        assert_eq!(h[2].detail, "Fix Tuesday");
    }

    // No edit event when the entry was never changed after logging.
    #[test]
    fn history_omits_edit_when_unchanged() {
        let e = RemoteTimeEntry {
            created_at: Some(ts(2026, 6, 15, 9, 0)),
            updated_at: Some(ts(2026, 6, 15, 9, 0)),
            ..entry()
        };
        let h = build_timesheet_history(&[e], "Ed", None);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, HistoryKind::Logged);
    }

    // AC3: an approved timesheet's history still assembles (the decision
    // event persists) even when the week's entries could not be loaded.
    #[test]
    fn history_survives_after_approval_with_no_entries() {
        let decision = Some(DecisionEvent {
            status: "approved".to_string(),
            at: ts(2026, 6, 18, 14, 30),
            actor: "Manager Meg".to_string(),
            reason: None,
        });
        let h = build_timesheet_history(&[], "Ed", decision);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, HistoryKind::Approved);
    }

    // A blank reviewer name degrades to a readable placeholder, never empty.
    #[test]
    fn history_decision_actor_falls_back_when_blank() {
        let decision = Some(DecisionEvent {
            status: "approved".to_string(),
            at: ts(2026, 6, 18, 14, 30),
            actor: "   ".to_string(),
            reason: None,
        });
        let h = build_timesheet_history(&[], "Ed", decision);
        assert_eq!(h[0].actor, "Unknown reviewer");
    }

    // Multiple entries sort into one interleaved oldest-first timeline.
    #[test]
    fn history_interleaves_multiple_entries_by_time() {
        let a = RemoteTimeEntry {
            created_at: Some(ts(2026, 6, 15, 9, 0)),
            updated_at: Some(ts(2026, 6, 17, 9, 0)),
            ..entry()
        };
        let b = RemoteTimeEntry {
            created_at: Some(ts(2026, 6, 16, 9, 0)),
            updated_at: Some(ts(2026, 6, 16, 9, 0)),
            ..entry()
        };
        let h = build_timesheet_history(&[a, b], "Ed", None);
        // a.created (15) < b.created (16) < a.updated (17)
        let times: Vec<DateTime<Utc>> = h.iter().map(|ev| ev.at).collect();
        assert_eq!(
            times,
            vec![
                ts(2026, 6, 15, 9, 0),
                ts(2026, 6, 16, 9, 0),
                ts(2026, 6, 17, 9, 0)
            ]
        );
        assert_eq!(h[0].kind, HistoryKind::Logged);
        assert_eq!(h[1].kind, HistoryKind::Logged);
        assert_eq!(h[2].kind, HistoryKind::Edited);
    }

    /// MAPPS-383: the entry row printed the raw wire tag while
    /// `billing_status` was a `String`. The match is exhaustive over the
    /// shared enum, so a new variant fails to compile rather than leaking.
    #[test]
    fn billing_status_labels_every_shared_variant() {
        assert_eq!(billing_status_label(BillingStatus::NotBilled), "Not Billed");
        assert_eq!(
            billing_status_label(BillingStatus::ReadyToBill),
            "Ready to Bill"
        );
        assert_eq!(billing_status_label(BillingStatus::Billed), "Billed");
    }

    /// The wire tags the server sends must decode into the shared enum; an
    /// unknown tag is a decode failure, not a silently rendered string.
    #[test]
    fn billing_status_decodes_from_the_wire_tags() {
        let decoded: RemoteTimeEntry = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::nil(),
            "date": "2026-06-18",
            "billing_status": "ready_to_bill",
        }))
        .expect("wire tag decodes");
        assert_eq!(decoded.billing_status, BillingStatus::ReadyToBill);

        assert!(
            serde_json::from_value::<RemoteTimeEntry>(serde_json::json!({
                "id": uuid::Uuid::nil(),
                "date": "2026-06-18",
                "billing_status": "invoiced",
            }))
            .is_err()
        );
    }
}

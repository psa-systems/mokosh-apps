//! Ticket pages

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    clear_selection, ticket_status_badge, use_bulk_selection, AlertType, AppLayout, Badge,
    BadgeVariant, BulkActionsBar, BulkSelection, Button, ButtonVariant, Card, ClockIcon, DataTable,
    IconSize, Modal, PageHeader, PencilIcon, PlusIcon, SearchInput, Select, SelectAllHeader,
    SelectOption, SelectRowCell, SortDirection, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow, Textarea, UserCircleIcon,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

/// Subset of mokosh-server's `TicketResponse` we render in the list. The
/// server returns more fields; serde silently drops the ones we don't
/// ask for, so adding columns later just means extending this struct.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicket {
    id: uuid::Uuid,
    ticket_number: String,
    title: String,
    #[serde(default)]
    company_name: String,
    #[serde(default)]
    status: RemoteSummary,
    #[serde(default)]
    priority: RemoteSummary,
    #[serde(default)]
    assigned_to_name: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RemoteSummary {
    /// Server-side `TicketStatusSummary` / `TicketPrioritySummary` always
    /// carry an `id`; the field is optional here only because legacy code
    /// paths and the demo-data fallback may omit it. PMS-359 reads it as
    /// the canonical "currently saved" selection for the inline editors
    /// on the ticket detail page.
    #[serde(default)]
    id: Option<uuid::Uuid>,
    #[serde(default)]
    name: String,
}

/// The fields of mokosh-server's `TicketResponse` the detail page renders.
/// Serde drops every field we don't ask for. The SLA pair (`sla_due_date`
/// + `sla_status`, a snake_case enum) drives the at-risk / breach badge.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketDetail {
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: String,
    #[serde(default)]
    contact_name: Option<String>,
    #[serde(default)]
    queue_name: String,
    #[serde(default)]
    status: RemoteSummary,
    #[serde(default)]
    priority: RemoteSummary,
    #[serde(default)]
    assigned_to_id: Option<uuid::Uuid>,
    // PMS-359: assigned_to_name is no longer read on the detail page
    // (the inline Assignee editor renders the chosen user by looking up
    // the id in the cached `/auth/users` list); dropped from the
    // deserialise shape entirely to keep the type honest. The list page
    // still reads `RemoteTicket.assigned_to_name`.
    /// PMS-344: the asset this ticket is associated with, if any. Both
    /// the id (for the inline AssetPicker editor + the asset-detail
    /// link) and the name (so the sidebar can render the asset's
    /// display name without an extra fetch) come straight off the
    /// server's joined TicketResponse.
    #[serde(default)]
    asset_id: Option<uuid::Uuid>,
    #[serde(default)]
    asset_name: Option<String>,
    #[serde(default)]
    created_by_name: String,
    created_at: DateTime<Utc>,
    #[serde(default)]
    sla_due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    sla_status: SlaStatus,
}

/// One row from `GET /tickets/statuses` (PMS-359). Tenant-scoped lookup
/// powering the inline Status editor on the ticket detail sidebar. The
/// `is_closed` flag is read so the renderer can keep the badge colour
/// stable when the user picks a Resolved / Closed row.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketStatus {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    is_closed: bool,
}

/// One row from `GET /tickets/priorities` (PMS-358 / PMS-359). Tenant-
/// scoped lookup the New Ticket form (PMS-358, defaults to the row with
/// `is_default = true`) and the detail-page inline editor (PMS-359)
/// both consume. Server's `CreateTicketRequest` / `UpdateTicketRequest`
/// both accept `priority_id: Option<Uuid>`, so any non-UUID would be
/// silently coerced away.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketPriority {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    is_default: bool,
}

/// MAPPS-296: minimal shape of a row from `GET /tickets/types` /
/// `GET /tickets/categories`. Both endpoints share the `(id, name)`
/// projection the New Ticket form needs; serde drops every other
/// field. Created at the top so the new lookups in `TicketNewPage`
/// resolve.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketLookup {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// MAPPS-296: minimal user-row shape for the New Ticket assignee
/// dropdown. The server's user list lives at `/auth/users`. `full_name`
/// is the precomputed `first last` projection; we fall back to `email`
/// when it is empty.
#[derive(Clone, Debug, Deserialize)]
struct RemoteUserLookup {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    email: String,
}

/// A ticket note (`GET /tickets/:id/notes`), rendered as an Activity item.
#[derive(Clone, Debug, Deserialize)]
struct RemoteNote {
    #[serde(default)]
    note_type: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    created_by_name: String,
    created_at: DateTime<Utc>,
}

/// One change-history entry (`GET /audit-log/entity/tickets/:id`, PMS-182).
/// `changed_fields` is the set of columns the edit touched; `changes` carries
/// their before/after values (PMS-204).
#[derive(Clone, Debug, Deserialize)]
struct HistoryEntry {
    #[serde(default)]
    action: String,
    #[serde(default)]
    user_id: Option<uuid::Uuid>,
    #[serde(default)]
    changed_fields: Vec<String>,
    #[serde(default)]
    changes: Vec<FieldChange>,
    timestamp: DateTime<Utc>,
}

/// The before/after value of one changed column (PMS-204).
#[derive(Clone, Debug, Deserialize)]
struct FieldChange {
    #[serde(default)]
    field: String,
    #[serde(default)]
    old: Option<serde_json::Value>,
    #[serde(default)]
    new: Option<serde_json::Value>,
}

/// User option for resolving history actor ids to names (`/auth/users`).
#[derive(Clone, Debug, Deserialize)]
struct UserOpt {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
}

/// A time entry (`GET /time-entries?ticket_id=:id`), summed into Time Logged.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTimeEntry {
    date: NaiveDate,
    // `i32` to match the server `TimeEntryResponse.duration_minutes`
    // (mokosh-types::time_tracking, MAPPS-138). Was `i64`; harmless over
    // JSON but the types disagreed.
    #[serde(default)]
    duration_minutes: i32,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_billable: bool,
}

/// Mirror of the server `SlaStatus` enum (snake_case wire form). Defaults
/// to `NotApplicable` so a ticket with no SLA configured still decodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SlaStatus {
    OnTrack,
    Warning,
    Breached,
    #[default]
    NotApplicable,
}

impl SlaStatus {
    /// (badge variant, label) for rendering. `NotApplicable` returns
    /// `None` so the caller can skip the badge entirely.
    fn badge(self) -> Option<(BadgeVariant, &'static str)> {
        match self {
            SlaStatus::OnTrack => Some((BadgeVariant::Green, "On Track")),
            SlaStatus::Warning => Some((BadgeVariant::Yellow, "At Risk")),
            SlaStatus::Breached => Some((BadgeVariant::Red, "Breached")),
            SlaStatus::NotApplicable => None,
        }
    }
}

/// Format an SLA due date as an absolute timestamp plus a coarse
/// remaining/overdue hint, e.g. "Jan 15, 2025 5:00 PM (2 hours left)".
/// PMS-253: honours the per-user format pref for the absolute part.
fn format_sla_due(due: DateTime<Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    let absolute = match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(due, Some(fmt)),
        None => due.format("%b %-d, %Y %-I:%M %p").to_string(),
    };
    let now = Utc::now();
    let delta = due.signed_duration_since(now);
    let secs = delta.num_seconds();
    let hint = if secs <= 0 {
        let overdue = (-secs).max(0);
        if overdue < 3600 {
            format!("{} min overdue", (overdue / 60).max(1))
        } else if overdue < 86_400 {
            format!("{} hr overdue", overdue / 3600)
        } else {
            format!("{} days overdue", overdue / 86_400)
        }
    } else if secs < 3600 {
        format!("{} min left", (secs / 60).max(1))
    } else if secs < 86_400 {
        format!("{} hr left", secs / 3600)
    } else {
        format!("{} days left", secs / 86_400)
    };
    format!("{absolute} ({hint})")
}

/// Source of the rows currently on screen. Mirrors the companies-page
/// pattern: backend if the fetch returned rows, demo otherwise.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TicketSource {
    Backend,
    Demo,
}

/// Render a `DateTime<Utc>` as a coarse "X ago" string. Good enough
/// for a list view where exact times live on the detail page.
fn relative_time(when: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(when);
    let secs = delta.num_seconds();
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".into()
        } else {
            format!("{hours} hours ago")
        }
    } else {
        let days = secs / 86_400;
        if days == 1 {
            "1 day ago".into()
        } else {
            format!("{days} days ago")
        }
    }
}

/// Convert the lowercase status name the server returns into the
/// title-case label `TicketRow` keys its badge color on. Unknown
/// values pass through so future statuses don't disappear.
fn humanize_ticket_status(raw: &str) -> String {
    match raw {
        "" => "Open".into(),
        "open" => "Open".into(),
        "in_progress" | "in progress" => "In Progress".into(),
        "pending" => "Pending".into(),
        "resolved" => "Resolved".into(),
        "closed" => "Closed".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn humanize_priority(raw: &str) -> String {
    match raw {
        "" => "Medium".into(),
        "critical" => "Critical".into(),
        "high" => "High".into(),
        "medium" => "Medium".into(),
        "low" => "Low".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Absolute timestamp for created / activity lines, e.g. "Jun 05, 2026 14:30".
/// PMS-253: honours the per-user format pref when set.
fn fmt_datetime(dt: DateTime<Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(dt, Some(fmt)),
        None => dt.format("%b %d, %Y %H:%M").to_string(),
    }
}

/// Resolve a history actor id to a display name; "-" when unknown so the
/// change-history feed never shows a bare UUID (PMS-182).
fn actor_name(users: &[UserOpt], id: &Option<uuid::Uuid>) -> String {
    match id {
        Some(uid) => users
            .iter()
            .find(|u| &u.id == uid)
            .map(|u| u.full_name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "-".to_string()),
        None => "-".to_string(),
    }
}

/// Humanize an audit action code for the change-history feed.
fn action_label(action: &str) -> &'static str {
    match action {
        "create" => "Created",
        "update" => "Updated",
        "delete" => "Deleted",
        _ => "Changed",
    }
}

/// "description, status" to "Description, Status" for the change summary.
fn fields_label(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| title_field(f))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "due_date" to "Due date" for a single field name.
///
/// PMS-370: column names for foreign-key fields end in `_id`
/// (`status_id`, `priority_id`, `assigned_to_id`). The audit log records
/// the raw column name, so without trimming the suffix the change-history
/// feed reads "Status id" / "Priority id" / "Assigned to id". Strip the
/// trailing `_id` first so future FK fields render cleanly without a
/// per-column allow-list.
fn title_field(f: &str) -> String {
    let trimmed = f.strip_suffix("_id").unwrap_or(f);
    let mut s = trimmed.replace('_', " ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    s
}

/// A 36-char hyphenated UUID, which is not worth showing as before/after text.
fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes().iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Render an audit value for display: "(empty)" for null/blank, the trimmed
/// text (truncated) for strings, a coarse marker for references/objects.
fn fmt_change_value(v: &Option<serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => "(empty)".to_string(),
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                "(empty)".to_string()
            } else if looks_like_uuid(t) {
                "(reference)".to_string()
            } else if t.chars().count() > 160 {
                format!("{}…", t.chars().take(160).collect::<String>())
            } else {
                t.to_string()
            }
        }
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(_) => "(updated)".to_string(),
    }
}

/// MAPPS-289: sortable columns on the ticket list. Mirrors the
/// `ContactSortKey` pattern in `contacts.rs`: an enum tracks which
/// column is active, paired with a `SortDirection`, and the table
/// reorders client-side over the already-filtered set.
#[derive(Clone, Copy, PartialEq)]
enum TicketSortKey {
    Ticket,
    Company,
    Status,
    Priority,
    Assigned,
    Updated,
}

fn ticket_sort_dir_for(
    current: &Option<(TicketSortKey, SortDirection)>,
    key: TicketSortKey,
) -> Option<SortDirection> {
    current.and_then(|(k, dir)| if k == key { Some(dir) } else { None })
}

fn toggle_ticket_sort(
    current: &mut Signal<Option<(TicketSortKey, SortDirection)>>,
    key: TicketSortKey,
) {
    let next = match *current.read() {
        Some((k, SortDirection::Ascending)) if k == key => Some((key, SortDirection::Descending)),
        Some((k, SortDirection::Descending)) if k == key => Some((key, SortDirection::Ascending)),
        _ => Some((key, SortDirection::Ascending)),
    };
    current.set(next);
}

/// Ticket list page
#[component]
pub fn TicketListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);
    let mut priority_filter = use_signal(String::new);
    // MAPPS-289: sortable-column state. Default sort matches the existing
    // list query (`?sort=-updated_at`) so the first paint is unchanged.
    let mut sort = use_signal(|| Some((TicketSortKey::Updated, SortDirection::Descending)));
    // MAPPS-290: page-scoped bulk selection. The header `SelectAllHeader`
    // toggles every visible row in/out; per-row `SelectRowCell` toggles
    // single rows; the `BulkActionsBar` renders the verb buttons when
    // non-empty and clears itself when a verb fires.
    let mut selection = use_bulk_selection();
    // MAPPS-310: confirm-before-delete for the bulk delete action.
    // `None` = no dialog open; `Some(snapshot)` = dialog open against
    // the snapshotted selection (we freeze the id list at click time
    // so a user un-checking a row mid-dialog doesn't smuggle past the
    // confirmation prompt). Other destructive surfaces in the app
    // (Companies / Contracts / Assets detail) gate on `ConfirmDialog`;
    // the bulk path bypassed it before this fix.
    let mut bulk_delete_confirm = use_signal::<Option<Vec<String>>>(|| None);
    let mut bulk_delete_running = use_signal(|| false);

    // MAPPS-295: source the status-filter options from the tenant's
    // configured `ticket_statuses` table instead of a hand-rolled slug
    // list. The list previously hardcoded `new/open/in_progress/pending/
    // resolved/closed`, while the ticket-detail inline-edit dropdown
    // already fetches `/tickets/statuses` (PMS-359) - so a tenant whose
    // workflow includes "Waiting on Client / Waiting on Vendor /
    // Scheduled" saw those on the detail page but couldn't filter by
    // them on the list, and the list offered a "Pending" the records
    // never carried. Source-of-truth is whichever set the server hands
    // back. Empty fetch (offline / 403) falls back to no options so the
    // filter just renders the "All Statuses" placeholder, which is
    // strictly better than fabricating slugs.
    let status_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketStatus>>("/tickets/statuses")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    // Same progressive-enablement pattern as the companies page: try
    // the live backend first, fall back to the seeded demo rows so the
    // page stays demoable when the route isn't deployed yet or the
    // user is signed out.
    // MAPPS-249: a company context card's "View All" lands here with
    // `?company_id=<uuid>`. When present, scope the fetch to that company so the
    // list shows only its tickets and every row stays inside the same company.
    let mut tickets_resource = use_resource(|| async {
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server returns, and so a failed load stays distinguishable
        // from an empty one (the third tuple element records the failure).
        let _reachable = crate::hooks::use_server_reachable();
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteTicket>::new(), TicketSource::Demo, false),
        };
        let mut path = String::from("/tickets");
        if let Some(company_id) = crate::utils::url::current_query_param("company_id") {
            path.push_str(&format!("?company_id={company_id}"));
        }
        match crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteTicket>>(&path, &token)
            .await
        {
            Ok(page) => (page.data, TicketSource::Backend, false),
            // MAPPS-357: keep the demo fallback for a 4xx while reachable, but
            // flag the failure so an outage (failed while unreachable) can render
            // ContentUnavailable below instead of a misleading demo list.
            Err(_) => (Vec::new(), TicketSource::Demo, true),
        }
    });

    let resource_snapshot = tickets_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_tickets, source, fetch_failed) = match &*resource_snapshot {
        Some((rows, source, failed)) => (rows.clone(), *source, *failed),
        None => (Vec::new(), TicketSource::Demo, false),
    };

    // MAPPS-357: the ticket list is this page's PRIMARY resource. A failed load
    // while the server is flagged down is an outage, not an empty/demo list, so
    // render the honest unavailable state (which keeps the nav + banner) instead
    // of demo rows. A fetch that fails while the server is still reachable (a
    // 4xx) keeps the demo-rows fallback below. Writes are blocked while down;
    // `can_mutate` disables the bulk-delete control.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Tickets".to_string() }
        };
    }

    // Apply search + status/priority filters to the loaded set client-side.
    // The controls previously only updated signals and never narrowed the
    // list (MAPPS-154); filtering here makes them functional. Status uses
    // the tenant's server-side name verbatim (MAPPS-295); priority still
    // routes through the humanize helper because its options remain the
    // hardcoded four-level slug set.
    let search_term = search.read().trim().to_lowercase();
    let status_name_sel = status_filter.read().clone();
    let priority_label_sel = match priority_filter.read().as_str() {
        "" => String::new(),
        raw => humanize_priority(raw),
    };
    let mut filtered_tickets: Vec<RemoteTicket> = remote_tickets
        .iter()
        .filter(|t| {
            if !search_term.is_empty() {
                let assigned = t.assigned_to_name.as_deref().unwrap_or("");
                let haystack = format!(
                    "{} {} {} {}",
                    t.ticket_number, t.title, t.company_name, assigned
                )
                .to_lowercase();
                if !haystack.contains(&search_term) {
                    return false;
                }
            }
            if !status_name_sel.is_empty() && !t.status.name.eq_ignore_ascii_case(&status_name_sel)
            {
                return false;
            }
            if !priority_label_sel.is_empty()
                && humanize_priority(&t.priority.name) != priority_label_sel
            {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    // MAPPS-289: client-side sort over the filtered set. The list query
    // already asks the server for `?sort=-updated_at`, so the default
    // sort signal matches that and the first paint is unchanged; a
    // header click re-orders without an extra round trip.
    {
        let snap = *sort.read();
        if let Some((key, dir)) = snap {
            filtered_tickets.sort_by(|a, b| {
                let ord = match key {
                    TicketSortKey::Ticket => a.ticket_number.cmp(&b.ticket_number),
                    TicketSortKey::Company => a.company_name.cmp(&b.company_name),
                    TicketSortKey::Status => a.status.name.cmp(&b.status.name),
                    TicketSortKey::Priority => a.priority.name.cmp(&b.priority.name),
                    TicketSortKey::Assigned => a
                        .assigned_to_name
                        .as_deref()
                        .unwrap_or("")
                        .cmp(b.assigned_to_name.as_deref().unwrap_or("")),
                    TicketSortKey::Updated => a.updated_at.cmp(&b.updated_at),
                };
                match dir {
                    SortDirection::Ascending => ord,
                    SortDirection::Descending => ord.reverse(),
                }
            });
        }
    }

    // MAPPS-295: build the Status filter options from the tenant's actual
    // status set. A still-loading or empty resource just shows the "All
    // Statuses" placeholder option.
    let tenant_statuses = status_resource.read_unchecked().clone().unwrap_or_default();
    let mut status_options = vec![SelectOption::new("", "All Statuses")];
    for s in tenant_statuses.iter() {
        status_options.push(SelectOption::new(s.name.clone(), s.name.clone()));
    }

    let priority_options = vec![
        SelectOption::new("", "All Priorities"),
        SelectOption::new("critical", "Critical"),
        SelectOption::new("high", "High"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("low", "Low"),
    ];

    rsx! {
        AppLayout { title: "Tickets",
            PageHeader {
                title: "Tickets",
                subtitle: "Manage support tickets and service requests",
                actions: rsx! {
                    Link {
                        to: Route::TicketNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Ticket"
                        }
                    }
                },
            }

            // MAPPS-321: surface the active company scope when the user
            // arrived via a "View all" link from CompanyDetail. The
            // fetch above already narrows the list to ?company_id=;
            // without this chip the user has no signal that the list
            // is already scoped (the header reads a plain "Tickets").
            crate::components::ContextFilterBanner {
                scope: crate::components::ContextFilterScope::Tickets,
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        SearchInput {
                            value: search.read().clone(),
                            placeholder: "Search tickets...",
                            oninput: move |e: FormEvent| search.set(e.value()),
                        }
                    }
                    div { class: "flex gap-4",
                        Select {
                            name: "status",
                            options: status_options,
                            value: status_filter.read().clone(),
                            placeholder: "Status",
                            onchange: move |e: FormEvent| status_filter.set(e.value()),
                        }
                        Select {
                            name: "priority",
                            options: priority_options,
                            value: priority_filter.read().clone(),
                            placeholder: "Priority",
                            onchange: move |e: FormEvent| priority_filter.set(e.value()),
                        }
                    }
                }
            }

            if source == TicketSource::Demo && !is_loading {
                div {
                    class: "mb-3 text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                    "Backend tickets API not reachable - showing demo rows."
                }
            }

            // MAPPS-290: bulk actions bar. Renders only when at least one
            // row is selected. The Delete verb issues parallel DELETE
            // calls and clears the selection on completion. Adding more
            // verbs (bulk assign, set priority) follows the same shape.
            BulkActionsBar {
                selection,
                label: "ticket".to_string(),
                Button {
                    variant: ButtonVariant::Danger,
                    // MAPPS-357: block bulk delete while the server is unreachable.
                    disabled: bulk_delete_running() || !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        // MAPPS-310: stash the snapshot + open the
                        // confirmation dialog. The actual delete fanout
                        // runs from the dialog's confirm handler so an
                        // accidental click is recoverable.
                        let ids: Vec<String> = selection.read().iter().cloned().collect();
                        if !ids.is_empty() {
                            bulk_delete_confirm.set(Some(ids));
                        }
                    },
                    "Delete selected"
                }
            }

            // MAPPS-310: confirmation dialog for the bulk delete. The
            // pending-id list lives in `bulk_delete_confirm`; the
            // onconfirm handler runs the same join_all delete fanout
            // the inline onclick used before this fix, then clears
            // the selection and restarts the resource.
            {
                let pending = bulk_delete_confirm.read().clone();
                let pending_count = pending.as_ref().map(|v| v.len()).unwrap_or(0);
                let dialog_message = format!(
                    "Delete {pending_count} selected ticket(s)? Notes, attachments, and time entries on these tickets are also removed. This cannot be undone."
                );
                let confirm_text = format!("Delete {pending_count} ticket(s)");
                rsx! {
                    crate::components::ConfirmDialog {
                        open: pending.is_some(),
                        title: "Delete selected tickets".to_string(),
                        message: dialog_message,
                        confirm_text,
                        cancel_text: "Cancel".to_string(),
                        destructive: true,
                        loading: bulk_delete_running(),
                        onconfirm: move |_| {
                            let Some(ids) = bulk_delete_confirm.read().clone() else { return };
                            if ids.is_empty() || bulk_delete_running() { return; }
                            bulk_delete_running.set(true);
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    use futures_util::future::join_all;
                                    let futs = ids.iter().map(|id| {
                                        let path = format!("/tickets/{id}");
                                        async move {
                                            crate::hooks::fetch::api::delete_authed(&path).await
                                        }
                                    });
                                    let results = join_all(futs).await;
                                    let failures = results.iter().filter(|r| r.is_err()).count();
                                    if failures == 0 {
                                        crate::hooks::toast::push_toast(
                                            crate::components::AlertType::Success,
                                            format!("Deleted {} ticket(s).", ids.len()),
                                        );
                                    } else {
                                        crate::hooks::toast::push_toast(
                                            crate::components::AlertType::Error,
                                            format!("Deleted {} of {}; {} failed.", ids.len() - failures, ids.len(), failures),
                                        );
                                    }
                                }
                                clear_selection(&mut selection);
                                tickets_resource.restart();
                                bulk_delete_confirm.set(None);
                                bulk_delete_running.set(false);
                            });
                        },
                        oncancel: move |_| {
                            if !bulk_delete_running() {
                                bulk_delete_confirm.set(None);
                            }
                        },
                    }
                }
            }

            // Ticket table
            DataTable {
                loading: is_loading,
                total_items: if source == TicketSource::Backend { filtered_tickets.len() } else { 5 },
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    striped: true,
                    TableHead {
                        TableRow {
                            // MAPPS-290: select-all checkbox for the visible page.
                            SelectAllHeader {
                                selection,
                                ids: filtered_tickets.iter().map(|t| t.id.to_string()).collect::<Vec<_>>(),
                            }
                            {
                                let sort_snap = *sort.read();
                                rsx! {
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Ticket),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Ticket),
                                        "Ticket"
                                    }
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Company),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Company),
                                        "Company"
                                    }
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Status),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Status),
                                        "Status"
                                    }
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Priority),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Priority),
                                        "Priority"
                                    }
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Assigned),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Assigned),
                                        "Assigned To"
                                    }
                                    TableHeader {
                                        sortable: true,
                                        sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Updated),
                                        onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Updated),
                                        "Updated"
                                    }
                                }
                            }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 6, rows: 5 }
                    } else if source == TicketSource::Backend && filtered_tickets.is_empty() {
                        if remote_tickets.is_empty() {
                            // PMS-354: helpful empty state with a primary CTA,
                            // matching the Contracts reference pattern.
                            TableEmpty {
                                columns: 6,
                                title: "No tickets yet".to_string(),
                                description: "Create your first ticket to start tracking support work."
                                    .to_string(),
                                actions: rsx! {
                                    Link {
                                        to: Route::TicketNew {},
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                            "New Ticket"
                                        }
                                    }
                                },
                            }
                        } else {
                            // Filtered to nothing: MAPPS-291 adds a one-click
                            // "Clear filters" affordance so the user does not
                            // have to find every filter control and reset
                            // each one to recover. Resets the three signals
                            // the toolbar above mounts.
                            TableEmpty {
                                columns: 6,
                                title: "No tickets match your filters".to_string(),
                                description: "Adjust the filters above, or clear them to see every ticket again.".to_string(),
                                actions: rsx! {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| {
                                            search.set(String::new());
                                            status_filter.set(String::new());
                                            priority_filter.set(String::new());
                                        },
                                        "Clear filters"
                                    }
                                },
                            }
                        }
                    } else {
                        TableBody {
                            if source == TicketSource::Backend {
                                for ticket in filtered_tickets.iter().cloned() {
                                    TicketRow {
                                        key: "{ticket.id}",
                                        id: ticket.id.to_string(),
                                        number: ticket.ticket_number,
                                        title: ticket.title,
                                        company: ticket.company_name,
                                        status: humanize_ticket_status(&ticket.status.name),
                                        priority: humanize_priority(&ticket.priority.name),
                                        assigned_to: ticket.assigned_to_name.unwrap_or_else(|| "Unassigned".to_string()),
                                        updated: relative_time(ticket.updated_at),
                                        // MAPPS-290: hand the page-scoped
                                        // selection signal down so each
                                        // row's first cell renders a
                                        // checkbox bound to it.
                                        selection: Some(selection),
                                    }
                                }
                            } else {
                                TicketRow {
                                    id: "1",
                                    number: "TKT-1234",
                                    title: "Email server not responding",
                                    company: "Acme Corp",
                                    status: "Open",
                                    priority: "High",
                                    assigned_to: "John Smith",
                                    updated: "5 min ago",
                                }
                                TicketRow {
                                    id: "2",
                                    number: "TKT-1233",
                                    title: "New user setup request",
                                    company: "TechStart Inc",
                                    status: "In Progress",
                                    priority: "Medium",
                                    assigned_to: "Jane Doe",
                                    updated: "1 hour ago",
                                }
                                TicketRow {
                                    id: "3",
                                    number: "TKT-1232",
                                    title: "Printer configuration for new office",
                                    company: "Global Widgets",
                                    status: "Pending",
                                    priority: "Low",
                                    assigned_to: "Unassigned",
                                    updated: "2 hours ago",
                                }
                                TicketRow {
                                    id: "4",
                                    number: "TKT-1231",
                                    title: "VPN connection issues for remote workers",
                                    company: "Acme Corp",
                                    status: "Open",
                                    priority: "Critical",
                                    assigned_to: "John Smith",
                                    updated: "3 hours ago",
                                }
                                TicketRow {
                                    id: "5",
                                    number: "TKT-1230",
                                    title: "Software license renewal required",
                                    company: "TechStart Inc",
                                    status: "Resolved",
                                    priority: "Medium",
                                    assigned_to: "Jane Doe",
                                    updated: "1 day ago",
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
struct TicketRowProps {
    id: String,
    number: String,
    title: String,
    company: String,
    status: String,
    priority: String,
    assigned_to: String,
    updated: String,
    /// MAPPS-290: optional bulk-selection signal. When `Some`, the row
    /// renders a `SelectRowCell` as its first cell wired to this signal;
    /// the demo rows pass `None` so the no-backend fallback table still
    /// fits the same column shape via a hidden first cell.
    #[props(default)]
    selection: Option<BulkSelection>,
}

#[component]
fn TicketRow(props: TicketRowProps) -> Element {
    let status_variant = ticket_status_badge(&props.status);

    let priority_variant = match props.priority.as_str() {
        "Critical" => BadgeVariant::Red,
        "High" => BadgeVariant::Red,
        "Medium" => BadgeVariant::Yellow,
        "Low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::TicketDetail { id: id.clone() }); },
            // MAPPS-290: per-row checkbox in the first column. The cell
            // stops propagation so toggling the checkbox doesn't also
            // navigate to the detail page.
            if let Some(selection) = props.selection {
                SelectRowCell { selection, id: props.id.clone() }
            } else {
                // Demo rows: keep the column shape consistent with the
                // header by rendering an inert cell.
                TableCell { class: "w-10", "" }
            }
            TableCell {
                div {
                    Link {
                        to: Route::TicketDetail { id: props.id.clone() },
                        class: "font-medium text-accent hover:opacity-90",
                        "{props.number}"
                    }
                    p { class: "text-muted text-sm truncate max-w-xs", "{props.title}" }
                }
            }
            TableCell { "{props.company}" }
            TableCell {
                Badge { variant: status_variant, "{props.status}" }
            }
            TableCell {
                Badge { variant: priority_variant, "{props.priority}" }
            }
            TableCell {
                if props.assigned_to == "Unassigned" {
                    span { class: "text-subtle italic", "Unassigned" }
                } else {
                    span { "{props.assigned_to}" }
                }
            }
            TableCell { class: "text-muted",
                "{props.updated}"
            }
        }
    }
}

/// MAPPS-207: optional company prefill carried on the New Ticket URL
/// (`/tickets/new?company_id=<uuid>&company_name=<name>`). The company
/// detail "New Ticket" affordance links here so the form opens with the
/// company already selected.
#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyPrefill {
    id: String,
    name: String,
}

fn read_company_prefill_from_url() -> CompanyPrefill {
    #[cfg(feature = "web")]
    {
        if let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) {
            if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                let id = params.get("company_id").unwrap_or_default();
                let name = params.get("company_name").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return CompanyPrefill { id, name };
                }
            }
        }
    }
    CompanyPrefill::default()
}

/// PMS-482: KB-article prefill captured off the URL when the
/// ticket-new page is reached via "Open ticket about this article"
/// from a KB article. Only the `id` is needed for the create body's
/// `source_kb_article_id`; `title` + `url` are folded into the
/// title and description signals so the user lands on a pre-filled
/// form they can immediately edit and submit.
#[derive(Clone, Debug, Default, PartialEq)]
struct KbArticlePrefill {
    id: String,
    title: String,
    url: String,
}

fn read_kb_prefill_from_url() -> KbArticlePrefill {
    #[cfg(feature = "web")]
    {
        if let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) {
            if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                let id = params.get("from_kb_article").unwrap_or_default();
                let title = params.get("from_kb_title").unwrap_or_default();
                let url = params.get("from_kb_url").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return KbArticlePrefill { id, title, url };
                }
            }
        }
    }
    KbArticlePrefill::default()
}

/// New ticket page
///
/// MAPPS-357: this is a create form, so there is no primary *entity* resource
/// whose failure means "no content" - the only fetches (types / categories /
/// users / priorities) are SECONDARY dropdown lookups that keep degrading to a
/// default. So the page does not swap in `ContentUnavailable`; instead the form
/// stays mounted (the user can keep composing) and the Create submit is disabled
/// while the server is unreachable so a write cannot silently fail.
#[component]
pub fn TicketNewPage() -> Element {
    // MAPPS-207: seed the company from the URL when linked from a company.
    let prefill = use_signal(read_company_prefill_from_url);
    let prefill = prefill.read().clone();
    // PMS-482: seed title + description from the source KB article when
    // the user clicked "Open ticket about this article". The article id
    // rides into the create body as `source_kb_article_id`.
    let kb_prefill = use_signal(read_kb_prefill_from_url);
    let kb_prefill = kb_prefill.read().clone();
    let kb_article_id_for_body = kb_prefill.id.clone();

    let initial_title = kb_prefill.title.clone();
    let initial_description = if kb_prefill.title.is_empty() {
        String::new()
    } else {
        let link = if kb_prefill.url.is_empty() {
            String::new()
        } else {
            format!("\nLink: {}", kb_prefill.url)
        };
        format!("Article: {}{}", kb_prefill.title, link)
    };
    let mut title = use_signal(|| initial_title);
    let mut description = use_signal(|| initial_description);
    // The company field holds a real company UUID (string) plus its human
    // name, both fed by the CompanyPicker. The old hardcoded "1"/"2"/"3"
    // Select submitted non-UUID ids that fell back to the nil UUID, so the
    // create always failed against the server (MAPPS-122).
    let mut company_id = use_signal(|| prefill.id.clone());
    let mut company_name = use_signal(|| prefill.name.clone());
    // MAPPS-207: optional contact association. Scoped to the selected
    // company via the ContactPicker's `company_filter`.
    let mut contact_id = use_signal(String::new);
    let mut contact_name = use_signal(String::new);
    // PMS-358: priority is a tenant-scoped lookup whose IDs are UUIDs. The
    // signal stores the selected UUID as a string (empty = "use tenant
    // default"). The hardcoded ["critical", "high", "medium", "low"]
    // string options the form used previously never landed in the request
    // body at all (the submit handler did not read the signal), and even if
    // they had they would not match the server's `priority_id: Option<Uuid>`
    // contract. Fetch the tenant's priorities on mount, default to the row
    // flagged `is_default = true`, and bind the Select's value to the UUID.
    let mut priority_id = use_signal(String::new);
    // PMS-344: optional asset association. Empty signals = no asset.
    // Both id and human name are tracked so the AssetPicker can render
    // its "selected chip" state without an extra fetch.
    let mut asset_id = use_signal(String::new);
    let mut asset_name = use_signal(String::new);
    // MAPPS-296: capture every common field at create time instead of
    // forcing the user to follow up with an edit. Type and Category are
    // tenant-scoped lookups (`/tickets/types` and `/tickets/categories`),
    // Assignee comes from `/auth/users` (already used by the calendar /
    // dispatch surfaces), and the Due Date stamps `scheduled_end` so
    // SLA / dispatch view it on creation.
    let mut type_id = use_signal(String::new);
    let mut category_id = use_signal(String::new);
    let mut assigned_to_id = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field server validation message routed next to the Title input
    // (MAPPS-210). Cleared on each submit and on edit.
    let mut title_error = use_signal(String::new);
    // PMS-518: per-field error for the now-enforced required Description.
    let mut description_error = use_signal(String::new);
    // MAPPS-322: per-field error for the required Company, routed into the
    // CompanyPicker's own inline slot instead of the form-level banner.
    let mut company_error = use_signal(String::new);

    // MAPPS-296: tenant lookups for Type + Category + Assignee. Each
    // hits its own endpoint and falls back to an empty list on a 403 /
    // network drop so the form mounts even when the lookups are
    // unavailable; the matching signal stays empty and the server
    // applies its defaults.
    let types_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketLookup>>(
            "/tickets/types?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let categories_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketLookup>>(
            "/tickets/categories?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let users_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteUserLookup>>(
            "/auth/users?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });

    let type_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "(none)")];
        if let Some(rows) = types_resource.read().as_ref() {
            for r in rows.iter() {
                opts.push(SelectOption::new(r.id.to_string(), r.name.clone()));
            }
        }
        opts
    };
    let category_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "(none)")];
        if let Some(rows) = categories_resource.read().as_ref() {
            for r in rows.iter() {
                opts.push(SelectOption::new(r.id.to_string(), r.name.clone()));
            }
        }
        opts
    };
    let assignee_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "Unassigned")];
        if let Some(rows) = users_resource.read().as_ref() {
            for u in rows.iter() {
                let name = if u.full_name.trim().is_empty() {
                    u.email.clone()
                } else {
                    u.full_name.clone()
                };
                opts.push(SelectOption::new(u.id.to_string(), name));
            }
        }
        opts
    };

    // Fetch the tenant's ticket priorities on mount. The Paginated envelope
    // matches the server's PaginatedResponse wire shape; meta is ignored
    // here (lookup tables are short enough to fit one page).
    let priorities_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketPriority>>(
            "/tickets/priorities",
        )
        .await
        .map(|p| p.data)
        .unwrap_or_default()
    });

    // Once priorities load, seed the signal with the tenant's default row
    // (or the first row if none is flagged default). use_effect re-runs
    // every time the resource transitions to Ready, but a non-empty
    // priority_id signal short-circuits so an in-progress user selection
    // is never clobbered by a late-arriving fetch.
    use_effect(move || {
        if !priority_id.read().is_empty() {
            return;
        }
        if let Some(rows) = priorities_resource.read().as_ref() {
            let chosen = rows
                .iter()
                .find(|p| p.is_default)
                .or_else(|| rows.first())
                .map(|p| p.id.to_string())
                .unwrap_or_default();
            if !chosen.is_empty() {
                priority_id.set(chosen);
            }
        }
    });

    let priority_options: Vec<SelectOption> = match priorities_resource.read().as_ref() {
        Some(rows) => rows
            .iter()
            .map(|p| SelectOption::new(p.id.to_string(), p.name.clone()))
            .collect(),
        // While the fetch is in flight, render an empty single-option
        // placeholder so the Select component does not blow up; the
        // effect above will populate the real options on the next tick.
        None => vec![SelectOption::new("", "Loading…")],
    };

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        error.set(String::new());

        // PMS-518: validate every required field through the shared FormGuard,
        // so all failures surface at once (each in its own inline slot) and the
        // first invalid field is focused. Generalises the PMS-514 fix; each
        // `field` call also clears a stale message when the value now passes.
        //
        // MAPPS-281: Title/Description are trimmed so a whitespace-only value
        // does not satisfy the (inert) native `required` and reach the server.
        let mut guard = FormGuard::new();

        let title_v = title.read().trim().to_string();
        title_error.set(guard.field("title", &title_v, "Title", &[Rule::Required]));

        // PMS-518: Description is now enforced (it carried the asterisk but was
        // never validated). The server accepts an empty body, so this is a
        // purely client-side rule, applied here and on blur via the field's
        // `rules`.
        let description_v = description.read().trim().to_string();
        description_error.set(guard.field(
            "description",
            &description_v,
            "Description",
            &[Rule::Required],
        ));

        // MAPPS-322: Company is required. Route the failure into the picker's
        // own inline error slot (it now takes an `error:` prop) instead of the
        // form-level banner, and focus its search input. `Rule::Uuid` guards a
        // malformed id; in practice the picker only ever yields a real UUID or
        // an empty string, so the surfaced message is `Rule::Required`'s
        // "Company is required."
        company_error.set(guard.field(
            "company_search",
            company_id.read().as_str(),
            "Company",
            &[Rule::Required, Rule::Uuid],
        ));

        if guard.blocked() {
            is_submitting.set(false);
            return;
        }
        // Past the guard: every required field is valid, so the company UUID
        // parses (re-bound here without an unwrap/expect).
        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().as_str()).ok() else {
            is_submitting.set(false);
            return;
        };

        // PMS-358: send the selected priority UUID. An empty string means
        // the priorities fetch was still in flight; let the server apply
        // its default rather than blocking the submit.
        let priority_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(priority_id.read().as_str()).ok();

        // PMS-344: asset is optional. Empty string = not picked.
        let asset_uuid: Option<uuid::Uuid> = uuid::Uuid::parse_str(asset_id.read().as_str()).ok();

        // MAPPS-207: contact is optional. Empty string = not picked.
        let contact_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(contact_id.read().as_str()).ok();

        // MAPPS-296: optional type / category / assignee. Each empty
        // signal sends `null` so the server falls back to its default
        // (no enforced type / category / assignee).
        let type_uuid: Option<uuid::Uuid> = uuid::Uuid::parse_str(type_id.read().as_str()).ok();
        let category_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(category_id.read().as_str()).ok();
        let assignee_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(assigned_to_id.read().as_str()).ok();
        // MAPPS-296: Due Date stamps `scheduled_end` (the server-side
        // SLA / dispatch consume the same field). The date input only
        // emits `YYYY-MM-DD`, so we land it at 23:59 UTC on the chosen
        // day - "due by end of day" is the intuitive read for "Due
        // Date" on a ticket.
        let due_value = due_date.read().clone();
        let scheduled_end: Option<String> = if due_value.trim().is_empty() {
            None
        } else {
            Some(format!("{}T23:59:00Z", due_value.trim()))
        };

        // Title + Description are already validated and captured (trimmed)
        // above; send the trimmed Description (now a required, non-empty value).
        // PMS-482: clone the captured KB id into a per-call binding so
        // the FnMut submit handler can be called more than once.
        let kb_article_id = kb_article_id_for_body.clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // PMS-482: stamp `source_kb_article_id` when the
                // page was reached from a KB article. Parsed
                // defensively (the prefill already validated it as
                // a UUID, but a hand-edited URL could still slip
                // through) and folded into the body as Null when
                // absent so the server uses its default.
                let kb_article_uuid: serde_json::Value = match uuid::Uuid::parse_str(&kb_article_id)
                {
                    Ok(u) => serde_json::Value::String(u.to_string()),
                    Err(_) => serde_json::Value::Null,
                };
                let body = serde_json::json!({
                    "title": title_v,
                    // MAPPS-322: Description is required and validated non-empty
                    // above, so send the (trimmed) string verbatim. The old
                    // "collapse empty to null" branch made the asterisk a lie:
                    // it let a blank description through as `null`.
                    "description": description_v,
                    "company_id": company_uuid,
                    "contact_id": contact_uuid,
                    "priority_id": priority_uuid,
                    "asset_id": asset_uuid,
                    // MAPPS-296: new fields. The server ignores `null`
                    // and uses its own defaults; this captures every
                    // field a dispatcher needs at creation instead of
                    // forcing a follow-up edit.
                    "type_id": type_uuid,
                    "category_id": category_uuid,
                    "assigned_to_id": assignee_uuid,
                    "scheduled_end": scheduled_end,
                    // PMS-482: KB-article provenance.
                    "source_kb_article_id": kb_article_uuid,
                });

                #[derive(serde::Deserialize)]
                struct CreatedTicket {
                    id: uuid::Uuid,
                }

                match crate::hooks::fetch::api::post_authed_typed::<CreatedTicket, _>(
                    "/tickets", &body,
                )
                .await
                {
                    Ok(created) => {
                        // MAPPS-293: confirming success toast.
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Ticket created.",
                        );
                        navigator.push(Route::TicketDetail {
                            id: created.id.to_string(),
                        });
                    }
                    Err(err) => {
                        // Surface the failure in the form and keep it
                        // mounted so the user can retry without losing
                        // their text. Route a server-flagged Title
                        // validation message next to that input; otherwise
                        // fall back to the general user-facing message
                        // (MAPPS-210).
                        if let Some(msg) = err.field_message("title") {
                            title_error.set(msg);
                        } else if let Some(msg) = err.field_message("description") {
                            // MAPPS-322: a server-side description validation
                            // failure (empty / over-cap) lands inline.
                            description_error.set(msg);
                        } else if let Some(msg) = err.field_message("company_id") {
                            // MAPPS-322: route the company validation failure
                            // next to the picker, same as title/description.
                            company_error.set(msg);
                        } else {
                            error.set(format!("Could not create ticket: {}", err.user_message()));
                        }
                    }
                }
            }

            is_submitting.set(false);
        });
    };

    let picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };

    let contact_company_filter: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };
    let picker_contact_selected_id: Option<String> =
        if uuid::Uuid::parse_str(contact_id.read().as_str()).is_ok() {
            Some(contact_id.read().clone())
        } else {
            None
        };

    // MAPPS-357: gate the create submit while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    rsx! {
        AppLayout { title: "New Ticket",
            PageHeader {
                title: "New Ticket",
                subtitle: "Create a new support ticket",
                breadcrumbs: rsx! {
                    crate::components::Breadcrumbs {
                        items: vec![
                            crate::components::BreadcrumbItem {
                                label: "Tickets".to_string(),
                                route: Some(Route::TicketList {}),
                            },
                            crate::components::BreadcrumbItem {
                                label: "New Ticket".to_string(),
                                route: None,
                            },
                        ],
                    }
                },
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: handle_submit,

                    if !error.read().is_empty() {
                        div {
                            class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                            "{error.read()}"
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Input {
                            name: "title",
                            label: "Title",
                            placeholder: "Brief description of the issue",
                            required: true,
                            // Server caps the title at 500 chars; mirror it
                            // client-side as a UX nicety (MAPPS-210).
                            maxlength: 500,
                            rules: vec![Rule::Required],
                            error: title_error.read().clone(),
                            value: title.read().clone(),
                            oninput: move |e: FormEvent| {
                                title_error.set(String::new());
                                title.set(e.value());
                            },
                        }

                        crate::components::CompanyPicker {
                            value: company_name.read().clone(),
                            selected_id: picker_selected_id,
                            required: true,
                            // MAPPS-322: surface the required-company error
                            // inline on the picker, cleared on select/clear
                            // like the title/description fields.
                            error: company_error.read().clone(),
                            // PMS-352: opt this picker into the inline
                            // "+ Create new company" affordance so a
                            // first-time technician on a tenant with zero
                            // companies can finish the New Ticket flow
                            // without leaving the form to seed a company.
                            allow_inline_create: true,
                            onselect: move |(id, name): (String, String)| {
                                company_error.set(String::new());
                                company_id.set(id);
                                company_name.set(name);
                                // Contacts are scoped to a company; clear
                                // any prior pick when the company changes.
                                contact_id.set(String::new());
                                contact_name.set(String::new());
                            },
                            onclear: move |_| {
                                company_error.set(String::new());
                                company_id.set(String::new());
                                company_name.set(String::new());
                                contact_id.set(String::new());
                                contact_name.set(String::new());
                            },
                        }

                        // MAPPS-207: optional contact on create, scoped to
                        // the selected company. Sends the server's
                        // `contact_id` field on TicketCreateRequest; empty
                        // signal sends null (no contact).
                        crate::components::ContactPicker {
                            value: contact_name.read().clone(),
                            selected_id: picker_contact_selected_id,
                            label: "Contact".to_string(),
                            company_filter: contact_company_filter,
                            // MAPPS-276: opt this picker into the inline
                            // "+ Create new contact" affordance so the New
                            // Ticket flow doesn't dead-end when the calling
                            // user isn't in the company's contacts yet. The
                            // picker inherits the form's selected company
                            // via `company_filter`, so the new contact
                            // lands attached to the right company.
                            allow_inline_create: true,
                            onselect: move |(id, name): (String, String)| {
                                contact_id.set(id);
                                contact_name.set(name);
                            },
                            onclear: move |_| {
                                contact_id.set(String::new());
                                contact_name.set(String::new());
                            },
                        }
                    }

                    Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Provide detailed information about the issue...",
                        rows: 6,
                        required: true,
                        rules: vec![Rule::Required],
                        error: description_error.read().clone(),
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| {
                            description_error.set(String::new());
                            description.set(e.value());
                        },
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "priority",
                            label: "Priority",
                            options: priority_options,
                            value: priority_id.read().clone(),
                            onchange: move |e: FormEvent| priority_id.set(e.value()),
                        }

                        // PMS-344: optional asset on create. Wires to the
                        // server's `asset_id` field on TicketCreateRequest;
                        // empty signal sends null (no asset).
                        {
                            let picker_asset_selected_id: Option<String> =
                                if uuid::Uuid::parse_str(asset_id.read().as_str()).is_ok() {
                                    Some(asset_id.read().clone())
                                } else {
                                    None
                                };
                            rsx! {
                                crate::components::AssetPicker {
                                    value: asset_name.read().clone(),
                                    selected_id: picker_asset_selected_id,
                                    onselect: move |(id, name): (String, String)| {
                                        asset_id.set(id);
                                        asset_name.set(name);
                                    },
                                    onclear: move |_| {
                                        asset_id.set(String::new());
                                        asset_name.set(String::new());
                                    },
                                }
                            }
                        }
                    }

                    // MAPPS-296: richer create form. Type + Category +
                    // Assignee + Due Date so the dispatcher captures
                    // everything a service-desk ticket needs at
                    // creation, instead of opening the new ticket and
                    // immediately editing in four more fields.
                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "type",
                            label: "Type",
                            options: type_options,
                            value: type_id.read().clone(),
                            onchange: move |e: FormEvent| type_id.set(e.value()),
                        }
                        Select {
                            name: "category",
                            label: "Category",
                            options: category_options,
                            value: category_id.read().clone(),
                            onchange: move |e: FormEvent| category_id.set(e.value()),
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "assigned_to",
                            label: "Assigned To",
                            options: assignee_options,
                            value: assigned_to_id.read().clone(),
                            onchange: move |e: FormEvent| assigned_to_id.set(e.value()),
                        }
                        crate::components::DateField {
                            name: "due_date",
                            label: "Due Date".to_string(),
                            value: due_date.read().clone(),
                            help: "Stamps the ticket's scheduled-end so SLA + dispatch view it on creation.".to_string(),
                            oninput: move |e: FormEvent| due_date.set(e.value()),
                        }
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::TicketList {},
                            Button {
                                variant: ButtonVariant::Secondary,
                                "Cancel"
                            }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            // MAPPS-357: block create while the server is unreachable.
                            disabled: !can_mutate,
                            title: (!can_mutate).then(|| "Can't create a ticket while the server is unreachable".to_string()),
                            "Create Ticket"
                        }
                    }
                }
            }
        }
    }
}

/// Ticket detail page
#[derive(Props, Clone, PartialEq)]
pub struct TicketDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn TicketDetailPage(props: TicketDetailPageProps) -> Element {
    let mut show_note_modal = use_signal(|| false);
    let mut note_type = use_signal(|| "internal".to_string());
    let mut note_content = use_signal(String::new);
    // PMS-518: inline error for the now-enforced required note Content,
    // surfaced in the textarea's own slot by the FormGuard in the Add Note
    // modal's submit handler.
    let mut note_content_error = use_signal(String::new);
    let mut note_submitting = use_signal(|| false);
    let mut note_error = use_signal(String::new);
    let ticket_id_for_note = props.id.clone();

    // Drive the whole page off the real ticket, its notes, and its time
    // entries. Each resource yields `Option<Option<T>>`: `None` while the
    // fetch is in flight, `Some(None)` on failure / no token.
    let id_for_ticket = props.id.clone();
    let ticket_resource = use_resource(move || {
        let id = id_for_ticket.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: the ticket entity is this page's PRIMARY resource.
            // Subscribe to reachability so it auto-refetches on reconnect, and
            // keep `.ok()` (Option) so a failed load stays distinguishable from
            // a real "ticket not found" 404.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<RemoteTicketDetail>(&format!("/tickets/{id}"))
                .await
                .ok()
        }
    });
    let id_for_notes = props.id.clone();
    let notes_resource = use_resource(move || {
        let id = id_for_notes.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteNote>>(&format!(
                "/tickets/{id}/notes"
            ))
            .await
            .ok()
        }
    });
    let id_for_time = props.id.clone();
    let time_resource = use_resource(move || {
        let id = id_for_time.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>(&format!(
                "/time-entries?ticket_id={id}"
            ))
            .await
            .ok()
        }
    });
    let id_for_history = props.id.clone();
    let history_resource = use_resource(move || {
        let id = id_for_history.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<HistoryEntry>>(&format!(
                "/audit-log/entity/tickets/{id}"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<UserOpt>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    // PMS-359: the tenant's ticket statuses + priorities, fetched once
    // and reused by the three inline editors on the sidebar. Same
    // Paginated envelope the New Ticket form's priorities fetch uses
    // (PMS-358).
    let statuses_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketStatus>>("/tickets/statuses")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let priorities_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTicketPriority>>(
            "/tickets/priorities",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    // PMS-359: per-field error message surfaced inline below the editor.
    // One signal is enough since at most one of the three editors fires
    // at a time and we always clear before the next attempt.
    let mut field_error = use_signal(String::new);

    // PMS-182 description edit state. MAPPS-188 folds the title into the
    // same modal, so the edit form now carries both `e_title` and `e_desc`.
    let mut editing_desc = use_signal(|| false);
    let mut e_title = use_signal(String::new);
    let mut e_desc = use_signal(String::new);
    // PMS-518: per-field inline errors for the Edit Ticket modal's required
    // Title + Description, surfaced in each field's own slot by the FormGuard
    // in `on_save`.
    let mut e_title_error = use_signal(String::new);
    let mut e_desc_error = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    let id_for_save = props.id.clone();

    // MAPPS-313: delete-ticket affordance on the detail page. The
    // existing list bulk-delete (MAPPS-310) and the detail-page
    // Delete here use the same `ConfirmDialog` shape; success
    // toasts and navigates back to the list.
    let mut confirming_ticket_delete = use_signal(|| false);
    let mut deleting_ticket = use_signal(|| false);
    let mut delete_ticket_error = use_signal(String::new);
    let delete_nav = use_navigator();
    let id_for_delete = props.id.clone();

    // MAPPS-198: keep the failure state distinct from the in-flight one.
    // Each resource yields `Option<Option<T>>`: `None` while in flight,
    // `Some(None)` on a failed fetch (e.g. a 404 for a missing/deleted/
    // cross-tenant ticket). `.flatten()` collapses both to `None`, so on its
    // own it cannot tell loading from a 404 and the page hangs on "Loading…"
    // forever. Capture the failure before flattening and short-circuit to a
    // "Ticket not found" state, mirroring the explicit `Some(None)` arm on the
    // invoice/contract/company detail pages.
    let ticket_snapshot = ticket_resource.read_unchecked().clone();
    let ticket_fetch_failed = matches!(ticket_snapshot, Some(None));
    let ticket = ticket_snapshot.flatten();
    // MAPPS-357: split the failed-fetch case. A failure while the server is
    // flagged down is an outage - render the honest ContentUnavailable state
    // (which keeps the nav + banner and auto-recovers on reconnect) instead of
    // the misleading "Ticket not found". A failure while the server is still
    // reachable is a real 404 (deleted / cross-tenant / bad link) and keeps the
    // existing not-found body below. Writes are blocked while down; `can_mutate`
    // disables every mutating control on the page.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if ticket_fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket".to_string() }
        };
    }
    if ticket_fetch_failed {
        return rsx! {
            AppLayout { title: "Ticket not found",
                PageHeader { title: "Ticket not found" }
                Card {
                    div { class: "py-8 text-center",
                        p {
                            class: "text-sm text-red-600 dark:text-red-300 mb-2",
                            "Ticket not found. It may have been deleted, or the link may be incorrect."
                        }
                        Link {
                            to: Route::TicketList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to tickets"
                        }
                    }
                }
            }
        };
    }
    let history = history_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    // PMS-359: lookups for the inline editors. Empty until the fetches
    // land; the renderer falls back to a "Loading…" badge in that window.
    let statuses = statuses_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let priorities = priorities_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    // "Edited" marker for the description: most recent history entry that
    // touched the description column.
    let desc_edited = history
        .iter()
        .find(|e| e.action == "update" && e.changed_fields.iter().any(|f| f == "description"))
        .map(|e| {
            let who = actor_name(&users, &e.user_id);
            let when = fmt_datetime(e.timestamp);
            if who == "-" {
                format!("Edited {when}")
            } else {
                format!("Edited {when} by {who}")
            }
        });
    // Prefer the ticket's human label (number + title) over the raw UUID. Fall
    // back to "Ticket <id>" only if the title is missing, "Loading…" in flight.
    let header_title = match ticket.as_ref() {
        Some(t) if !t.title.trim().is_empty() => {
            if t.ticket_number.trim().is_empty() {
                t.title.clone()
            } else {
                format!("{}: {}", t.ticket_number, t.title)
            }
        }
        Some(_) => format!("Ticket {}", props.id),
        None => "Loading…".to_string(),
    };
    let notes: Vec<RemoteNote> = notes_resource
        .read_unchecked()
        .clone()
        .flatten()
        .map(|p| p.data)
        .unwrap_or_default();
    let note_count = notes.len();
    let time_entries: Vec<RemoteTimeEntry> = time_resource
        .read_unchecked()
        .clone()
        .flatten()
        .map(|p| p.data)
        .unwrap_or_default();
    let total_minutes: i32 = time_entries.iter().map(|e| e.duration_minutes).sum();
    let total_hours_label = format!("{:.1} hours", total_minutes as f64 / 60.0);

    // PMS-362: carry the ticket into the Log Time flow so the work-item picker
    // opens preselected. A plain <a href> (not a routed Link) because the
    // TimeEntryNew route declares no query params, so a Link would strip
    // `?ticket_id=`; the router still intercepts the same-origin anchor click.
    let log_time_href = format!("/time/new?ticket_id={}", props.id);

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        // MAPPS-357: block adding a note while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't add a note while the server is unreachable".to_string()),
                        onclick: move |_| {
                            note_error.set(String::new());
                            show_note_modal.set(true);
                        },
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Note"
                    }
                    a {
                        href: "{log_time_href}",
                        Button {
                            variant: ButtonVariant::Primary,
                            ClockIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Log Time"
                        }
                    }
                    // MAPPS-313: per-ticket Delete affordance, matching
                    // the pattern on Company / Contract / Asset detail.
                    Button {
                        variant: ButtonVariant::Danger,
                        // MAPPS-357: block delete while the server is unreachable.
                        disabled: deleting_ticket() || !can_mutate,
                        title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                        onclick: move |_| {
                            delete_ticket_error.set(String::new());
                            confirming_ticket_delete.set(true);
                        },
                        "Delete"
                    }
                },
            }
            // MAPPS-313: confirm-before-delete for the ticket. Success
            // toasts, navigates back to the list. Failure surfaces the
            // server message inline in the dialog so the user can
            // retry without losing their place.
            {
                let ticket_label = ticket
                    .as_ref()
                    .map(|t| {
                        if t.ticket_number.trim().is_empty() {
                            t.title.clone()
                        } else {
                            format!("{} - {}", t.ticket_number, t.title)
                        }
                    })
                    .unwrap_or_else(|| "this ticket".to_string());
                let id_for_confirm = id_for_delete.clone();
                rsx! {
                    crate::components::ConfirmDialog {
                        open: confirming_ticket_delete(),
                        title: "Delete ticket".to_string(),
                        message: {
                            let mut msg = format!(
                                "Delete {ticket_label}? Notes, attachments, and time entries on this ticket are also removed. This cannot be undone."
                            );
                            if !delete_ticket_error.read().is_empty() {
                                msg.push_str(&format!("\n\n{}", delete_ticket_error.read()));
                            }
                            msg
                        },
                        confirm_text: "Delete ticket".to_string(),
                        cancel_text: "Cancel".to_string(),
                        destructive: true,
                        loading: deleting_ticket(),
                        oncancel: move |_| {
                            if !deleting_ticket() {
                                confirming_ticket_delete.set(false);
                                delete_ticket_error.set(String::new());
                            }
                        },
                        onconfirm: move |_| {
                            if deleting_ticket() { return; }
                            deleting_ticket.set(true);
                            delete_ticket_error.set(String::new());
                            let id = id_for_confirm.clone();
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let path = format!("/tickets/{id}");
                                    match crate::hooks::fetch::api::delete_authed(&path).await {
                                        Ok(()) => {
                                            crate::hooks::toast::push_toast(
                                                crate::components::AlertType::Success,
                                                "Ticket deleted.",
                                            );
                                            confirming_ticket_delete.set(false);
                                            delete_nav.push(Route::TicketList {});
                                        }
                                        Err(err) => {
                                            delete_ticket_error.set(format!("Could not delete ticket: {err}"));
                                        }
                                    }
                                }
                                #[cfg(not(feature = "web"))]
                                let _ = &id;
                                deleting_ticket.set(false);
                            });
                        },
                    }
                }
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Description (real ticket description, editable - PMS-182)
                    {
                        let ticket_loaded = ticket.is_some();
                        let cur_desc = ticket
                            .as_ref()
                            .and_then(|t| t.description.clone())
                            .unwrap_or_default();
                        // MAPPS-188: seed the title field from the saved ticket
                        // so the edit modal opens with the current title too.
                        let cur_title = ticket
                            .as_ref()
                            .map(|t| t.title.clone())
                            .unwrap_or_default();
                        let open_edit = move |_| {
                            e_title.set(cur_title.clone());
                            e_desc.set(cur_desc.clone());
                            e_error.set(String::new());
                            editing_desc.set(true);
                        };
                        let marker = desc_edited.clone();
                        rsx! {
                            Card {
                                title: "Description",
                                actions: if ticket_loaded {
                                    Some(rsx! {
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            // MAPPS-357: block the edit modal while the server is down.
                                            disabled: !can_mutate,
                                            title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                                            onclick: open_edit,
                                            PencilIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                            "Edit"
                                        }
                                    })
                                } else {
                                    None
                                },
                                if let Some(t) = ticket.as_ref() {
                                    if let Some(desc) = t.description.as_ref().filter(|d| !d.trim().is_empty()) {
                                        // PMS-309: render Markdown (sanitized). PMS-348:
                                        // task-list checkboxes are clickable - toggling
                                        // flips the source marker and persists.
                                        {
                                            let desc_src = desc.clone();
                                            let tid = props.id.clone();
                                            rsx! {
                                                crate::components::Markdown {
                                                    content: desc.clone(),
                                                    // MAPPS-357: task-list checkboxes PUT on
                                                    // toggle, so drop interactivity while the
                                                    // server is unreachable (on_toggle is a
                                                    // no-op when not interactive) to block the
                                                    // silent-fail write.
                                                    interactive: can_mutate,
                                                    on_toggle: move |i: usize| {
                                                        let Some(new_desc) =
                                                            crate::utils::markdown::toggle_task(&desc_src, i)
                                                        else {
                                                            return;
                                                        };
                                                        let tid = tid.clone();
                                                        let mut tr = ticket_resource;
                                                        let mut hr = history_resource;
                                                        spawn(async move {
                                                            let body = serde_json::json!({ "description": new_desc });
                                                            match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                                                                    &format!("/tickets/{tid}"),
                                                                    &body,
                                                                )
                                                                .await
                                                            {
                                                                Ok(_) => {
                                                                    tr.restart();
                                                                    hr.restart();
                                                                }
                                                                Err(e) => {
                                                                    crate::hooks::push_toast(
                                                                        crate::components::AlertType::Error,
                                                                        format!("Could not update checklist: {e}"),
                                                                    );
                                                                    tr.restart();
                                                                }
                                                            }
                                                        });
                                                    },
                                                }
                                            }
                                        }
                                    } else {
                                        p { class: "text-sm text-subtle italic", "No description provided." }
                                    }
                                    if let Some(m) = marker {
                                        p { class: "text-xs text-subtle italic mt-3", "{m}" }
                                    }
                                } else {
                                    p { class: "text-sm text-subtle", "Loading…" }
                                }
                            }
                        }
                    }

                    // PMS-486: ticket-detail Approvals section. Self-contained
                    // component owns its own fetch, modal state, and refresh
                    // cycle; rendered above the activity timeline so the
                    // open approval requests are immediately visible.
                    ApprovalsSection { entity_id: props.id.clone() }

                    // Activity timeline (real ticket notes; there is no audit
                    // feed yet, so status / assignment events do not appear).
                    Card { title: "Activity",
                        if note_count == 0 {
                            p { class: "text-sm text-subtle italic",
                                "No activity yet. Notes added to this ticket will appear here."
                            }
                        } else {
                            div { class: "flow-root",
                                ul { class: "-mb-8",
                                    for (i , n) in notes.iter().enumerate() {
                                        TimelineItem {
                                            user: if n.created_by_name.is_empty() { "Someone".to_string() } else { n.created_by_name.clone() },
                                            action: if n.note_type == "internal" { "added an internal note".to_string() } else { "added a note".to_string() },
                                            time: fmt_datetime(n.created_at),
                                            content: Some(n.content.clone()),
                                            is_last: i + 1 == note_count,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    Card { title: "Details",
                        if let Some(t) = ticket.as_ref() {
                            dl { class: "space-y-4",
                                // PMS-359: inline Status / Priority / Assigned To editors.
                                // Each renders a native Select bound to the
                                // currently-saved id; onchange fires a PUT
                                // /tickets/{id} with the matching field and
                                // refreshes the ticket + history resources on
                                // success so the change-history pane records
                                // the edit alongside any prior description edits.
                                {
                                    // Statuses Select. Empty options list (still
                                    // fetching) falls back to a single "Loading…"
                                    // entry so the component does not collapse.
                                    let current_status = t
                                        .status
                                        .id
                                        .map(|u| u.to_string())
                                        .unwrap_or_default();
                                    let mut status_options: Vec<SelectOption> = statuses
                                        .iter()
                                        .map(|s| SelectOption::new(s.id.to_string(), s.name.clone()))
                                        .collect();
                                    if status_options.is_empty() {
                                        status_options.push(SelectOption::new("", "Loading…"));
                                    }
                                    let save_id = props.id.clone();
                                    let mut tr = ticket_resource;
                                    let mut hr = history_resource;
                                    let onchange = move |e: FormEvent| {
                                        let Ok(new_id) = uuid::Uuid::parse_str(&e.value()) else {
                                            return;
                                        };
                                        let save_id = save_id.clone();
                                        spawn(async move {
                                            field_error.set(String::new());
                                            let body =
                                                serde_json::json!({ "status_id": new_id });
                                            match crate::hooks::fetch::api::put_authed::<
                                                serde_json::Value,
                                                _,
                                            >(
                                                &format!("/tickets/{save_id}"), &body
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    tr.restart();
                                                    hr.restart();
                                                }
                                                Err(err) => {
                                                    field_error.set(format!(
                                                        "Could not update status: {err}"
                                                    ));
                                                }
                                            }
                                        });
                                    };
                                    rsx! {
                                        DetailItem {
                                            label: "Status",
                                            value: rsx! {
                                                Select {
                                                    name: "status_id",
                                                    label: "",
                                                    options: status_options,
                                                    value: current_status,
                                                    // MAPPS-357: this Select PUTs on change; block it while down.
                                                    disabled: !can_mutate,
                                                    onchange,
                                                }
                                            },
                                        }
                                    }
                                }
                                {
                                    let current_priority = t
                                        .priority
                                        .id
                                        .map(|u| u.to_string())
                                        .unwrap_or_default();
                                    let mut priority_options: Vec<SelectOption> = priorities
                                        .iter()
                                        .map(|p| SelectOption::new(p.id.to_string(), p.name.clone()))
                                        .collect();
                                    if priority_options.is_empty() {
                                        priority_options.push(SelectOption::new("", "Loading…"));
                                    }
                                    let save_id = props.id.clone();
                                    let mut tr = ticket_resource;
                                    let mut hr = history_resource;
                                    let onchange = move |e: FormEvent| {
                                        let Ok(new_id) = uuid::Uuid::parse_str(&e.value()) else {
                                            return;
                                        };
                                        let save_id = save_id.clone();
                                        spawn(async move {
                                            field_error.set(String::new());
                                            let body =
                                                serde_json::json!({ "priority_id": new_id });
                                            match crate::hooks::fetch::api::put_authed::<
                                                serde_json::Value,
                                                _,
                                            >(
                                                &format!("/tickets/{save_id}"), &body
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    tr.restart();
                                                    hr.restart();
                                                }
                                                Err(err) => {
                                                    field_error.set(format!(
                                                        "Could not update priority: {err}"
                                                    ));
                                                }
                                            }
                                        });
                                    };
                                    rsx! {
                                        DetailItem {
                                            label: "Priority",
                                            value: rsx! {
                                                Select {
                                                    name: "priority_id",
                                                    label: "",
                                                    options: priority_options,
                                                    value: current_priority,
                                                    // MAPPS-357: this Select PUTs on change; block it while down.
                                                    disabled: !can_mutate,
                                                    onchange,
                                                }
                                            },
                                        }
                                    }
                                }
                                {
                                    // Assignee uses the same users list the change-
                                    // history viewer consumes. Empty value = unassigned,
                                    // which serialises to JSON null so the server
                                    // clears assigned_to_id.
                                    let current_assignee = t
                                        .assigned_to_id
                                        .map(|u| u.to_string())
                                        .unwrap_or_default();
                                    let mut user_options: Vec<SelectOption> =
                                        vec![SelectOption::new("", "Unassigned")];
                                    for u in users.iter() {
                                        let label = if u.full_name.trim().is_empty() {
                                            u.id.to_string()
                                        } else {
                                            u.full_name.clone()
                                        };
                                        user_options.push(SelectOption::new(u.id.to_string(), label));
                                    }
                                    let save_id = props.id.clone();
                                    let mut tr = ticket_resource;
                                    let mut hr = history_resource;
                                    let onchange = move |e: FormEvent| {
                                        let raw = e.value();
                                        let new_id: Option<uuid::Uuid> = if raw.is_empty() {
                                            None
                                        } else {
                                            match uuid::Uuid::parse_str(&raw) {
                                                Ok(u) => Some(u),
                                                Err(_) => return,
                                            }
                                        };
                                        let save_id = save_id.clone();
                                        spawn(async move {
                                            field_error.set(String::new());
                                            let body =
                                                serde_json::json!({ "assigned_to_id": new_id });
                                            match crate::hooks::fetch::api::put_authed::<
                                                serde_json::Value,
                                                _,
                                            >(
                                                &format!("/tickets/{save_id}"), &body
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    tr.restart();
                                                    hr.restart();
                                                }
                                                Err(err) => {
                                                    field_error.set(format!(
                                                        "Could not update assignee: {err}"
                                                    ));
                                                }
                                            }
                                        });
                                    };
                                    rsx! {
                                        DetailItem {
                                            label: "Assigned To",
                                            value: rsx! {
                                                Select {
                                                    name: "assigned_to_id",
                                                    label: "",
                                                    options: user_options,
                                                    value: current_assignee,
                                                    // MAPPS-357: this Select PUTs on change; block it while down.
                                                    disabled: !can_mutate,
                                                    onchange,
                                                }
                                            },
                                        }
                                    }
                                }
                                if !field_error.read().is_empty() {
                                    p { class: "text-xs text-red-600 dark:text-red-400",
                                        "{field_error}"
                                    }
                                }
                                // PMS-344: Asset row with inline AssetPicker.
                                // Selecting an asset fires PUT /tickets/{id}
                                // with `asset_id`, the same shape the inline
                                // status/priority/assignee editors above use.
                                // Clearing the picker sends `null` so the
                                // server unsets the association.
                                {
                                    let current_asset_id = t.asset_id.map(|u| u.to_string());
                                    let current_asset_name =
                                        t.asset_name.clone().unwrap_or_default();
                                    let save_id = props.id.clone();
                                    let mut tr = ticket_resource;
                                    let mut hr = history_resource;
                                    let put_asset = move |new_id: Option<uuid::Uuid>| {
                                        let save_id = save_id.clone();
                                        spawn(async move {
                                            field_error.set(String::new());
                                            let body = serde_json::json!({
                                                "asset_id": new_id,
                                            });
                                            match crate::hooks::fetch::api::put_authed::<
                                                serde_json::Value,
                                                _,
                                            >(
                                                &format!("/tickets/{save_id}"),
                                                &body,
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    tr.restart();
                                                    hr.restart();
                                                }
                                                Err(err) => {
                                                    field_error.set(format!(
                                                        "Could not update asset: {err}"
                                                    ));
                                                }
                                            }
                                        });
                                    };
                                    let put_asset_for_select = put_asset.clone();
                                    let put_asset_for_clear = put_asset.clone();
                                    rsx! {
                                        DetailItem {
                                            label: "Asset",
                                            value: rsx! {
                                                // MAPPS-357: the AssetPicker PUTs on
                                                // select/clear but exposes no `disabled`
                                                // prop, so it cannot be gated from this
                                                // file (unlike the Selects above). A write
                                                // it fires while down surfaces the inline
                                                // "Could not update asset" error rather
                                                // than silently succeeding; gating it needs
                                                // a `disabled` prop on the shared component.
                                                crate::components::AssetPicker {
                                                    // PMS-344 follow-up
                                                    // (layout): suppress
                                                    // the picker's own
                                                    // label here because
                                                    // DetailItem already
                                                    // renders "Asset" on
                                                    // the left, matching
                                                    // how the inline
                                                    // Status/Priority/
                                                    // Assignee Select
                                                    // editors mount.
                                                    label: String::new(),
                                                    value: current_asset_name,
                                                    selected_id: current_asset_id,
                                                    onselect: move |(id, _name): (String, String)| {
                                                        if let Ok(uid) = uuid::Uuid::parse_str(&id) {
                                                            put_asset_for_select(Some(uid));
                                                        }
                                                    },
                                                    onclear: move |_| {
                                                        put_asset_for_clear(None);
                                                    },
                                                }
                                            },
                                        }
                                    }
                                }
                                if !t.company_name.is_empty() {
                                    DetailItem {
                                        label: "Company",
                                        value: rsx! {
                                            if let Some(cid) = t.company_id {
                                                Link {
                                                    to: Route::CompanyDetail { id: cid.to_string() },
                                                    class: "text-accent hover:opacity-90",
                                                    "{t.company_name}"
                                                }
                                            } else {
                                                span { "{t.company_name}" }
                                            }
                                        },
                                    }
                                }
                                if let Some(contact) = t.contact_name.as_ref().filter(|s| !s.is_empty()) {
                                    DetailItem { label: "Contact", value: rsx!(span { "{contact}" }) }
                                }
                                if !t.queue_name.is_empty() {
                                    DetailItem { label: "Queue", value: rsx!(span { "{t.queue_name}" }) }
                                }
                                {
                                    let created = if t.created_by_name.is_empty() {
                                        fmt_datetime(t.created_at)
                                    } else {
                                        format!("{} by {}", fmt_datetime(t.created_at), t.created_by_name)
                                    };
                                    rsx! {
                                        DetailItem { label: "Created", nowrap: true, value: rsx!(span { "{created}" }) }
                                    }
                                }
                                if let Some((variant , label)) = t.sla_status.badge() {
                                    DetailItem { label: "SLA Status", value: rsx!(Badge { variant, "{label}" }) }
                                }
                                if let Some(due) = t.sla_due_date.map(format_sla_due) {
                                    DetailItem { label: "SLA Due", nowrap: true, value: rsx!(span { "{due}" }) }
                                }
                            }
                        } else {
                            p { class: "text-sm text-subtle", "Loading…" }
                        }
                    }

                    // Time entries (real, summed from /time-entries?ticket_id=)
                    Card { title: "Time Logged",
                        div { class: "space-y-3",
                            div { class: "flex justify-between items-center",
                                span { class: "text-sm text-muted", "Total Time" }
                                span { class: "text-lg font-semibold", "{total_hours_label}" }
                            }
                            if time_entries.is_empty() {
                                p { class: "text-sm text-subtle italic", "No time logged yet." }
                            } else {
                                div { class: "space-y-2 text-sm text-muted",
                                    for e in time_entries.iter() {
                                        div {
                                            div { class: "flex justify-between gap-2",
                                                span { "{e.date} · {e.duration_minutes} min" }
                                                if e.is_billable {
                                                    span { class: "text-green-600 dark:text-green-400", "billable" }
                                                }
                                            }
                                            if let Some(note) = e.notes.as_ref().filter(|s| !s.is_empty()) {
                                                p { class: "text-xs text-subtle truncate", "{note}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Change history (PMS-182) - field-level edits to the ticket.
                    Card { title: "Change History",
                        if history.is_empty() {
                            p { class: "text-sm text-subtle italic", "No edits yet." }
                        } else {
                            div { class: "space-y-3 text-sm",
                                for e in history.iter().take(20) {
                                    {
                                        let label = action_label(&e.action);
                                        let fields = fields_label(&e.changed_fields);
                                        let who = actor_name(&users, &e.user_id);
                                        let when = fmt_datetime(e.timestamp);
                                        rsx! {
                                            div { class: "flex justify-between gap-2",
                                                div { class: "min-w-0",
                                                    p { class: "text-content",
                                                        if fields.is_empty() {
                                                            "{label}"
                                                        } else {
                                                            "{label}: {fields}"
                                                        }
                                                    }
                                                    if who != "-" {
                                                        p { class: "text-xs text-subtle", "by {who}" }
                                                    }
                                                    // PMS-204: actual before/after content.
                                                    for c in e.changes.iter() {
                                                        {
                                                            let old = fmt_change_value(&c.old);
                                                            let new = fmt_change_value(&c.new);
                                                            let fname = title_field(&c.field);
                                                            if old == "(reference)" && new == "(reference)" {
                                                                rsx! {}
                                                            } else {
                                                                rsx! {
                                                                    p { class: "text-xs text-muted mt-1",
                                                                        span { class: "font-medium", "{fname}: " }
                                                                        span { class: "line-through text-subtle", "{old}" }
                                                                        " → "
                                                                        span { "{new}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                span { class: "text-subtle whitespace-nowrap", "{when}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // PMS-182 description edit modal.
            {
                let mut ticket_res = ticket_resource;
                let mut history_res = history_resource;
                let save_id = id_for_save.clone();
                let on_save = move |_| {
                    if e_submitting() {
                        return;
                    }
                    e_error.set(String::new());
                    // PMS-518: validate the required Title + Description through
                    // the shared FormGuard so both failures surface inline at
                    // once and the first invalid field is focused. The ids match
                    // each field component's `name` prop. MAPPS-188: Title is
                    // still trimmed (the server validates length >= 1) so a
                    // whitespace-only value never reaches the PUT.
                    let mut guard = FormGuard::new();
                    let title_v = e_title().trim().to_string();
                    e_title_error
                        .set(guard.field("edit-title", &title_v, "Title", &[Rule::Required]));
                    let desc_v = e_desc().trim().to_string();
                    e_desc_error.set(guard.field(
                        "edit-description",
                        &desc_v,
                        "Description",
                        &[Rule::Required],
                    ));
                    if guard.blocked() {
                        return;
                    }
                    let save_id = save_id.clone();
                    spawn(async move {
                        e_submitting.set(true);
                        e_error.set(String::new());
                        // MAPPS-322: send the trimmed, guard-validated values.
                        // `desc_v` is already non-empty (the FormGuard above
                        // blocks a blank edit), so an edit can no longer blank
                        // an existing description.
                        let body = serde_json::json!({
                            "title": title_v,
                            "description": desc_v,
                        });
                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                            &format!("/tickets/{save_id}"),
                            &body,
                        )
                        .await
                        {
                            Ok(_) => {
                                e_submitting.set(false);
                                editing_desc.set(false);
                                ticket_res.restart();
                                history_res.restart();
                            }
                            Err(err) => {
                                e_submitting.set(false);
                                e_error.set(err);
                            }
                        }
                    });
                };
                rsx! {
                    Modal {
                        open: editing_desc(),
                        title: "Edit Ticket",
                        size: crate::components::ModalSize::Large,
                        onclose: move |_| editing_desc.set(false),
                        footer: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| editing_desc.set(false),
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: e_submitting(),
                                // MAPPS-357: block the save PUT while the server is down.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                                onclick: on_save,
                                "Save Changes"
                            }
                        },
                        div { class: "space-y-3",
                            if !e_error().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{e_error}" }
                            }
                            // MAPPS-188: title is now editable alongside the
                            // description (previously description-only).
                            crate::components::Input {
                                name: "edit-title",
                                label: "Title",
                                required: true,
                                rules: vec![Rule::Required],
                                error: e_title_error.read().clone(),
                                value: "{e_title}",
                                oninput: move |e: FormEvent| {
                                    e_title_error.set(String::new());
                                    e_title.set(e.value());
                                },
                            }
                            Textarea {
                                name: "edit-description",
                                label: "Description",
                                rows: 8,
                                required: true,
                                rules: vec![Rule::Required],
                                error: e_desc_error.read().clone(),
                                value: "{e_desc}",
                                oninput: move |e: FormEvent| {
                                    e_desc_error.set(String::new());
                                    e_desc.set(e.value());
                                },
                            }
                        }
                    }
                }
            }

            // Add note modal
            Modal {
                open: *show_note_modal.read(),
                title: "Add Note",
                size: crate::components::ModalSize::Medium,
                onclose: move |_| show_note_modal.set(false),
                footer: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| show_note_modal.set(false),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *note_submitting.read(),
                        // MAPPS-357: block the add-note POST while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't add a note while the server is unreachable".to_string()),
                        onclick: move |_| {
                            note_error.set(String::new());
                            // PMS-518: validate the required Content through the
                            // shared FormGuard before submitting so the failure
                            // lands in the textarea's own inline slot and the
                            // field is focused. Runs before `note_submitting` is
                            // set, so the bail path leaves it untouched.
                            let mut guard = FormGuard::new();
                            let content_v = note_content.read().clone();
                            note_content_error.set(guard.field(
                                "content",
                                content_v.trim(),
                                "Content",
                                &[Rule::Required],
                            ));
                            if guard.blocked() {
                                return;
                            }
                            note_submitting.set(true);
                            let id = ticket_id_for_note.clone();
                            let type_v = note_type.read().clone();
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let body = serde_json::json!({
                                        "note_type": type_v,
                                        "content": content_v,
                                    });
                                    let path = format!("/tickets/{id}/notes");
                                    match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(&path, &body).await {
                                        Ok(_) => {
                                            note_content.set(String::new());
                                            note_error.set(String::new());
                                            show_note_modal.set(false);
                                            // Refresh the Activity feed so the new note shows.
                                            let mut nr = notes_resource;
                                            nr.restart();
                                        }
                                        Err(err) => {
                                            note_error.set(format!("Could not add note: {err}"));
                                        }
                                    }
                                }
                                note_submitting.set(false);
                            });
                        },
                        "Add Note"
                    }
                },
                div { class: "space-y-4",
                    if !note_error.read().is_empty() {
                        div {
                            class: "rounded-md bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 px-3 py-2 text-sm text-red-700 dark:text-red-300",
                            "{note_error}"
                        }
                    }
                    Select {
                        name: "note_type",
                        label: "Note Type",
                        options: vec![
                            SelectOption::new("internal", "Internal Note"),
                            SelectOption::new("public", "Public Note (visible to customer)"),
                        ],
                        value: note_type.read().clone(),
                        onchange: move |e: FormEvent| note_type.set(e.value()),
                    }
                    Textarea {
                        name: "content",
                        label: "Content",
                        placeholder: "Enter your note...",
                        rows: 4,
                        required: true,
                        rules: vec![Rule::Required],
                        error: note_content_error.read().clone(),
                        value: note_content.read().clone(),
                        oninput: move |e: FormEvent| {
                            note_content_error.set(String::new());
                            note_content.set(e.value());
                        },
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DetailItemProps {
    label: String,
    value: Element,
    /// Keep the value on a single line (no wrapping). Used for Created and
    /// SLA Due, whose timestamps would otherwise wrap onto a second line
    /// (PMS-181).
    #[props(default = false)]
    nowrap: bool,
}

#[component]
fn DetailItem(props: DetailItemProps) -> Element {
    // `flex-1 min-w-0` on dd so the value cell grows to fill the row
    // after the (shrink-0) label, instead of being sized to its content.
    // The Select-based inline editors stay visually unchanged because
    // their content is small and stays right-anchored by `text-right`,
    // but the AssetPicker chip (PMS-344) can now `w-full` itself into
    // the dd cell and truncate its long asset name / uuid inline rather
    // than escaping the row and rendering on top of the next field.
    // `items-start` (instead of `items-baseline`) keeps a multi-line
    // value cell (chip + buttons) aligned to the label's top, not its
    // baseline.
    let dd_class = if props.nowrap {
        "text-sm text-content text-right whitespace-nowrap flex-1 min-w-0"
    } else {
        "text-sm text-content text-right flex-1 min-w-0"
    };
    rsx! {
        div { class: "flex justify-between items-start gap-3",
            dt { class: "text-sm text-muted flex-shrink-0 pt-0.5", "{props.label}" }
            dd { class: "{dd_class}", {props.value} }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimelineItemProps {
    user: String,
    action: String,
    time: String,
    content: Option<String>,
    is_last: bool,
}

#[component]
fn TimelineItem(props: TimelineItemProps) -> Element {
    rsx! {
        li {
            div { class: "relative pb-8",
                if !props.is_last {
                    span {
                        class: "absolute left-4 top-4 -ml-px h-full w-0.5 bg-surface-2",
                        aria_hidden: "true",
                    }
                }
                div { class: "relative flex space-x-3",
                    div {
                        span { class: "h-8 w-8 rounded-full bg-accent-100 dark:bg-accent-900 flex items-center justify-center ring-8 ring-surface",
                            UserCircleIcon { size: IconSize::Small, class: "text-accent".to_string() }
                        }
                    }
                    div { class: "flex min-w-0 flex-1 justify-between space-x-4 pt-1.5",
                        div {
                            p { class: "text-sm text-muted",
                                span { class: "font-medium text-content", "{props.user}" }
                                " {props.action}"
                            }
                            if let Some(content) = &props.content {
                                div { class: "mt-2 text-sm text-content bg-surface-2 rounded-md p-3",
                                    "{content}"
                                }
                            }
                        }
                        div { class: "whitespace-nowrap text-right text-sm text-muted",
                            "{props.time}"
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// PMS-486: Approvals section on the ticket-detail page.
// ============================================================================

#[derive(Clone, Debug, Deserialize)]
struct TicketApprovalRow {
    id: uuid::Uuid,
    #[serde(default)]
    state: String,
    #[serde(default)]
    requested_by_name: Option<String>,
    #[serde(default)]
    approver_user_name: Option<String>,
    #[serde(default)]
    approver_role: Option<String>,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    decision_notes: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    requested_at: Option<DateTime<Utc>>,
    #[serde(default)]
    decided_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
struct UserPickerRow {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
}

/// PMS-675: the approvals surface is polymorphic server-side
/// (`target` = ticket | change_request | quote | time_entry), so this
/// section is parameterised by entity rather than forked per entity.
/// The defaults keep it a drop-in for its original ticket call site.
#[derive(Props, Clone, PartialEq)]
pub struct ApprovalsSectionProps {
    /// The parent entity's id.
    pub entity_id: String,
    /// URL segment the approvals hang off: `tickets`, `quotes`, ...
    #[props(default = "tickets".to_string())]
    pub entity_segment: String,
    /// Noun used in the empty / error copy ("this ticket", "this quote").
    #[props(default = "ticket".to_string())]
    pub entity_noun: String,
}

#[component]
pub fn ApprovalsSection(props: ApprovalsSectionProps) -> Element {
    let mut version = use_signal(|| 0u32);
    let mut show_request = use_signal(|| false);
    let mut approver_user_id = use_signal(String::new);
    let mut approver_role = use_signal(String::new);
    let mut request_notes = use_signal(String::new);
    let mut request_submitting = use_signal(|| false);
    let mut request_error = use_signal(String::new);

    let id_for_list = props.entity_id.clone();
    let segment_for_list = props.entity_segment.clone();
    let approvals_resource = use_resource(move || {
        let id = id_for_list.clone();
        let segment = segment_for_list.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _v = version.read();
            crate::hooks::fetch::api::get_authed::<Vec<TicketApprovalRow>>(&format!(
                "/{segment}/{id}/approvals"
            ))
            .await
            .ok()
        }
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<UserPickerRow>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let snap = approvals_resource.read_unchecked();
    let rows: Vec<TicketApprovalRow> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    // MAPPS-357: this is a SECONDARY section embedded in the ticket-detail page,
    // not a routed page - the parent already swaps in ContentUnavailable when the
    // ticket entity fails to load, so a failed approvals fetch keeps its inline
    // "could not load" message rather than blanking the page. But its write
    // controls (Request approval) still get disabled while the server is down so
    // a request cannot silently fail.
    let can_mutate = crate::hooks::use_can_mutate();

    let ticket_for_submit = props.entity_id.clone();
    let segment_for_submit = props.entity_segment.clone();
    let on_submit = move |_| {
        let user_id = approver_user_id.read().trim().to_string();
        let role = approver_role.read().trim().to_string();
        let notes = request_notes.read().trim().to_string();
        // PMS-518: the approver is an XOR rule (exactly one of a specific user
        // OR a role). Neither side owns an inline error slot, so a violation
        // goes to the form-level banner; `note_invalid` blocks the submit and
        // focuses the approver picker. Runs before `request_submitting` is set,
        // so the bail path leaves it untouched.
        let mut guard = FormGuard::new();
        if user_id.is_empty() && role.is_empty() {
            request_error.set("Pick an approver or enter a role.".to_string());
            guard.note_invalid(Some("approver_user_id"));
        }
        if !user_id.is_empty() && !role.is_empty() {
            request_error.set("Pick either an approver or a role, not both.".to_string());
            guard.note_invalid(Some("approver_user_id"));
        }
        if guard.blocked() {
            return;
        }
        let ticket = ticket_for_submit.clone();
        // Cloned per invocation, like `ticket` above: the closure is an
        // `onclick` handler and must stay `FnMut`, so it cannot move the
        // captured segment out of itself.
        let segment = segment_for_submit.clone();
        request_submitting.set(true);
        request_error.set(String::new());
        spawn(async move {
            let mut body = serde_json::Map::new();
            if !user_id.is_empty() {
                if let Ok(u) = uuid::Uuid::parse_str(&user_id) {
                    body.insert(
                        "approver_user_id".to_string(),
                        serde_json::Value::String(u.to_string()),
                    );
                }
            }
            if !role.is_empty() {
                body.insert("approver_role".to_string(), serde_json::Value::String(role));
            }
            if !notes.is_empty() {
                body.insert("notes".to_string(), serde_json::Value::String(notes));
            }
            let json = serde_json::Value::Object(body);
            match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                &format!("/{segment}/{ticket}/approvals"),
                &json,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Approval requested");
                    show_request.set(false);
                    approver_user_id.set(String::new());
                    approver_role.set(String::new());
                    request_notes.set(String::new());
                    version += 1;
                }
                Err(e) => {
                    request_error.set(format!("Could not create approval: {e}"));
                }
            }
            request_submitting.set(false);
        });
    };

    let mut user_options: Vec<SelectOption> = users
        .iter()
        .map(|u| {
            let label = if u.name.trim().is_empty() {
                u.email.clone()
            } else {
                u.name.clone()
            };
            SelectOption::new(u.id.to_string(), label)
        })
        .collect();
    user_options.insert(0, SelectOption::new("", "- Pick approver -"));

    rsx! {
        Card { title: "Approvals",
            div { class: "flex justify-end mb-3",
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block requesting an approval while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                    onclick: move |_| show_request.set(true),
                    "Request approval"
                }
            }
            if loading {
                p { class: "text-sm text-subtle italic", "Loading approvals..." }
            } else if fetch_failed {
                p { class: "text-sm text-red-600 dark:text-red-300", "Could not load approvals for this {props.entity_noun}." }
            } else if rows.is_empty() {
                p { class: "text-sm text-subtle italic", "No approvals requested on this {props.entity_noun} yet." }
            } else {
                ul { class: "space-y-3",
                    for row in rows.iter().cloned() {
                        {
                            let key = row.id.to_string();
                            let state_variant = match row.state.as_str() {
                                "approved" => BadgeVariant::Green,
                                "rejected" => BadgeVariant::Red,
                                _ => BadgeVariant::Yellow,
                            };
                            let approver_label = match (row.approver_user_name.clone(), row.approver_role.clone()) {
                                (Some(n), _) if !n.trim().is_empty() => format!("To: {n}"),
                                (_, Some(r)) if !r.trim().is_empty() => format!("Role: {r}"),
                                _ => "(unassigned)".to_string(),
                            };
                            let requester = row.requested_by_name.clone().unwrap_or_default();
                            let when = row
                                .requested_at
                                .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                .unwrap_or_default();
                            let decided = row
                                .decided_at
                                .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                .unwrap_or_default();
                            let notes = row.notes.clone().unwrap_or_default();
                            let decision = row.decision.clone().unwrap_or_default();
                            let decision_notes = row.decision_notes.clone().unwrap_or_default();
                            rsx! {
                                li { key: "{key}", class: "rounded border border-line p-3",
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        Badge { variant: state_variant, "{row.state}" }
                                        span { class: "text-sm text-content", "{approver_label}" }
                                    }
                                    if !requester.is_empty() {
                                        p { class: "text-xs text-subtle mt-1", "Requested by {requester}" }
                                    }
                                    if !when.is_empty() {
                                        p { class: "text-xs text-subtle", "Requested {when}" }
                                    }
                                    if !notes.is_empty() {
                                        p { class: "text-sm text-muted mt-2 whitespace-pre-wrap", "{notes}" }
                                    }
                                    if !decision.is_empty() {
                                        p { class: "text-xs text-subtle mt-2",
                                            "Decision: " strong { "{decision}" }
                                            if !decided.is_empty() { " on {decided}" }
                                        }
                                    }
                                    if !decision_notes.is_empty() {
                                        p { class: "text-sm text-muted mt-1 whitespace-pre-wrap italic",
                                            "{decision_notes}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            open: *show_request.read(),
            title: "Request approval",
            size: crate::components::ModalSize::Medium,
            onclose: move |_| show_request.set(false),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| show_request.set(false),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *request_submitting.read(),
                    // MAPPS-357: block the approval-request POST while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                    onclick: on_submit,
                    "Request"
                }
            },
            div { class: "space-y-4",
                if !request_error().is_empty() {
                    div { class: "rounded-md bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 px-3 py-2 text-sm text-red-700 dark:text-red-300",
                        "{request_error}"
                    }
                }
                p { class: "text-xs text-subtle",
                    "Pick a specific approver OR enter a role. The server requires exactly one."
                }
                Select {
                    name: "approver_user_id",
                    label: "Approver",
                    options: user_options,
                    value: approver_user_id.read().clone(),
                    onchange: move |e: FormEvent| approver_user_id.set(e.value()),
                }
                crate::components::Input {
                    name: "approver_role",
                    label: "Approver role (optional)",
                    value: "{approver_role}",
                    placeholder: "e.g. manager, finance",
                    oninput: move |e: FormEvent| approver_role.set(e.value()),
                }
                Textarea {
                    name: "request_notes",
                    label: "Notes (optional)",
                    rows: 3,
                    value: "{request_notes}",
                    oninput: move |e: FormEvent| request_notes.set(e.value()),
                }
            }
        }
    }
}

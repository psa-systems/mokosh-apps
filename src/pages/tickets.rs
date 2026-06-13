//! Ticket pages

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    ticket_status_badge, AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ClockIcon,
    DataTable, IconSize, Modal, PageHeader, PencilIcon, PlusIcon, SearchInput, Select,
    SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow, Textarea, UserCircleIcon,
};
use crate::utils::Paginated;
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
    assigned_to_name: Option<String>,
    #[serde(default)]
    created_by_name: String,
    created_at: DateTime<Utc>,
    #[serde(default)]
    sla_due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    sla_status: SlaStatus,
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
    #[serde(default)]
    duration_minutes: i64,
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

/// Badge colour for a humanized priority label.
fn priority_badge_variant(label: &str) -> BadgeVariant {
    match label {
        "Critical" | "High" => BadgeVariant::Red,
        "Medium" => BadgeVariant::Yellow,
        "Low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
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
fn title_field(f: &str) -> String {
    let mut s = f.replace('_', " ");
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

/// Ticket list page
#[component]
pub fn TicketListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);
    let mut priority_filter = use_signal(String::new);

    // Same progressive-enablement pattern as the companies page: try
    // the live backend first, fall back to the seeded demo rows so the
    // page stays demoable when the route isn't deployed yet or the
    // user is signed out.
    let tickets_resource = use_resource(|| async {
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteTicket>::new(), TicketSource::Demo),
        };
        match crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteTicket>>("/tickets", &token)
            .await
        {
            Ok(page) => (page.data, TicketSource::Backend),
            Err(_) => (Vec::new(), TicketSource::Demo),
        }
    });

    let resource_snapshot = tickets_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_tickets, source) = match &*resource_snapshot {
        Some((rows, source)) => (rows.clone(), *source),
        None => (Vec::new(), TicketSource::Demo),
    };

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("open", "Open"),
        SelectOption::new("in_progress", "In Progress"),
        SelectOption::new("pending", "Pending"),
        SelectOption::new("resolved", "Resolved"),
        SelectOption::new("closed", "Closed"),
    ];

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

            // Ticket table
            DataTable {
                loading: is_loading,
                total_items: if source == TicketSource::Backend { remote_tickets.len() } else { 5 },
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Ticket" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { sortable: true, "Status" }
                            TableHeader { sortable: true, "Priority" }
                            TableHeader { sortable: true, "Assigned To" }
                            TableHeader { sortable: true, "Updated" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 6, rows: 5 }
                    } else if source == TicketSource::Backend && remote_tickets.is_empty() {
                        TableEmpty { columns: 6, message: "No tickets yet.".to_string() }
                    } else {
                        TableBody {
                            if source == TicketSource::Backend {
                                for ticket in remote_tickets.iter().cloned() {
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
            TableCell {
                div {
                    Link {
                        to: Route::TicketDetail { id: props.id.clone() },
                        class: "font-medium text-blue-600 hover:text-blue-500",
                        "{props.number}"
                    }
                    p { class: "text-gray-500 text-sm truncate max-w-xs", "{props.title}" }
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
                    span { class: "text-gray-400 italic", "Unassigned" }
                } else {
                    span { "{props.assigned_to}" }
                }
            }
            TableCell { class: "text-gray-500",
                "{props.updated}"
            }
        }
    }
}

/// New ticket page
#[component]
pub fn TicketNewPage() -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    // The company field holds a real company UUID (string) plus its human
    // name, both fed by the CompanyPicker. The old hardcoded "1"/"2"/"3"
    // Select submitted non-UUID ids that fell back to the nil UUID, so the
    // create always failed against the server (MAPPS-122).
    let mut company_id = use_signal(String::new);
    let mut company_name = use_signal(String::new);
    let mut priority = use_signal(|| "medium".to_string());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let priority_options = vec![
        SelectOption::new("critical", "Critical"),
        SelectOption::new("high", "High"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("low", "Low"),
    ];

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        error.set(String::new());

        // The CompanyPicker only ever reports real company UUIDs, but a
        // user can submit before picking one. Validate up front and bail
        // with a visible message instead of POSTing the nil UUID.
        let Ok(company_uuid) = uuid::Uuid::parse_str(company_id.read().as_str()) else {
            error.set("Please pick a company first.".to_string());
            is_submitting.set(false);
            return;
        };

        // Snapshot signals so the spawn doesn't need to read them.
        let title_v = title.read().clone();
        let description_v = description.read().clone();
        let _priority_v = priority.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({
                    "title": title_v,
                    "description": if description_v.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(description_v) },
                    "company_id": company_uuid,
                });

                #[derive(serde::Deserialize)]
                struct CreatedTicket {
                    id: uuid::Uuid,
                }

                match crate::hooks::fetch::api::post_authed::<CreatedTicket, _>("/tickets", &body)
                    .await
                {
                    Ok(created) => {
                        navigator.push(Route::TicketDetail {
                            id: created.id.to_string(),
                        });
                    }
                    Err(err) => {
                        // Surface the failure in the form and keep it
                        // mounted so the user can retry without losing
                        // their text.
                        error.set(format!("Could not create ticket: {err}"));
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

    rsx! {
        AppLayout { title: "New Ticket",
            PageHeader {
                title: "New Ticket",
                subtitle: "Create a new support ticket",
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
                            value: title.read().clone(),
                            oninput: move |e: FormEvent| title.set(e.value()),
                        }

                        crate::components::CompanyPicker {
                            value: company_name.read().clone(),
                            selected_id: picker_selected_id,
                            required: true,
                            onselect: move |(id, name): (String, String)| {
                                company_id.set(id);
                                company_name.set(name);
                            },
                            onclear: move |_| {
                                company_id.set(String::new());
                                company_name.set(String::new());
                            },
                        }
                    }

                    Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Provide detailed information about the issue...",
                        rows: 6,
                        required: true,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "priority",
                            label: "Priority",
                            options: priority_options,
                            value: priority.read().clone(),
                            onchange: move |e: FormEvent| priority.set(e.value()),
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

    // PMS-182 description edit state.
    let mut editing_desc = use_signal(|| false);
    let mut e_desc = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    let id_for_save = props.id.clone();

    let ticket = ticket_resource.read_unchecked().clone().flatten();
    let history = history_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
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
    let total_minutes: i64 = time_entries.iter().map(|e| e.duration_minutes).sum();
    let total_hours_label = format!("{:.1} hours", total_minutes as f64 / 60.0);

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            note_error.set(String::new());
                            show_note_modal.set(true);
                        },
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Note"
                    }
                    Link {
                        to: Route::TimeEntryNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            ClockIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Log Time"
                        }
                    }
                },
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
                        let open_edit = move |_| {
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
                                        p { class: "text-gray-700 dark:text-gray-300 whitespace-pre-wrap", "{desc}" }
                                    } else {
                                        p { class: "text-sm text-gray-400 italic", "No description provided." }
                                    }
                                    if let Some(m) = marker {
                                        p { class: "text-xs text-gray-400 italic mt-3", "{m}" }
                                    }
                                } else {
                                    p { class: "text-sm text-gray-400", "Loading…" }
                                }
                            }
                        }
                    }

                    // Activity timeline (real ticket notes; there is no audit
                    // feed yet, so status / assignment events do not appear).
                    Card { title: "Activity",
                        if note_count == 0 {
                            p { class: "text-sm text-gray-400 italic",
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
                                {
                                    let status_label = humanize_ticket_status(&t.status.name);
                                    rsx! {
                                        DetailItem {
                                            label: "Status",
                                            value: rsx!(Badge { variant: ticket_status_badge(&status_label), "{status_label}" }),
                                        }
                                    }
                                }
                                {
                                    let priority_label = humanize_priority(&t.priority.name);
                                    rsx! {
                                        DetailItem {
                                            label: "Priority",
                                            value: rsx!(Badge { variant: priority_badge_variant(&priority_label), "{priority_label}" }),
                                        }
                                    }
                                }
                                {
                                    let assigned = t
                                        .assigned_to_name
                                        .clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| "Unassigned".to_string());
                                    rsx! {
                                        DetailItem { label: "Assigned To", value: rsx!(span { "{assigned}" }) }
                                    }
                                }
                                if !t.company_name.is_empty() {
                                    DetailItem {
                                        label: "Company",
                                        value: rsx! {
                                            if let Some(cid) = t.company_id {
                                                Link {
                                                    to: Route::CompanyDetail { id: cid.to_string() },
                                                    class: "text-blue-600 hover:text-blue-500",
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
                            p { class: "text-sm text-gray-400", "Loading…" }
                        }
                    }

                    // Time entries (real, summed from /time-entries?ticket_id=)
                    Card { title: "Time Logged",
                        div { class: "space-y-3",
                            div { class: "flex justify-between items-center",
                                span { class: "text-sm text-gray-500", "Total Time" }
                                span { class: "text-lg font-semibold", "{total_hours_label}" }
                            }
                            if time_entries.is_empty() {
                                p { class: "text-sm text-gray-400 italic", "No time logged yet." }
                            } else {
                                div { class: "space-y-2 text-sm text-gray-500",
                                    for e in time_entries.iter() {
                                        div {
                                            div { class: "flex justify-between gap-2",
                                                span { "{e.date} · {e.duration_minutes} min" }
                                                if e.is_billable {
                                                    span { class: "text-green-600 dark:text-green-400", "billable" }
                                                }
                                            }
                                            if let Some(note) = e.notes.as_ref().filter(|s| !s.is_empty()) {
                                                p { class: "text-xs text-gray-400 truncate", "{note}" }
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
                            p { class: "text-sm text-gray-400 italic", "No edits yet." }
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
                                                    p { class: "text-gray-700 dark:text-gray-300",
                                                        if fields.is_empty() {
                                                            "{label}"
                                                        } else {
                                                            "{label}: {fields}"
                                                        }
                                                    }
                                                    if who != "-" {
                                                        p { class: "text-xs text-gray-400", "by {who}" }
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
                                                                    p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                                                        span { class: "font-medium", "{fname}: " }
                                                                        span { class: "line-through text-gray-400", "{old}" }
                                                                        " → "
                                                                        span { "{new}" }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                span { class: "text-gray-400 whitespace-nowrap", "{when}" }
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
                    let save_id = save_id.clone();
                    spawn(async move {
                        e_submitting.set(true);
                        e_error.set(String::new());
                        let body = serde_json::json!({ "description": e_desc() });
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
                        title: "Edit Description",
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
                                onclick: on_save,
                                "Save Changes"
                            }
                        },
                        div { class: "space-y-3",
                            if !e_error().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{e_error}" }
                            }
                            Textarea {
                                name: "edit-description",
                                label: "Description",
                                rows: 8,
                                value: "{e_desc}",
                                oninput: move |e: FormEvent| e_desc.set(e.value()),
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
                        onclick: move |_| {
                            note_error.set(String::new());
                            note_submitting.set(true);
                            let id = ticket_id_for_note.clone();
                            let type_v = note_type.read().clone();
                            let content_v = note_content.read().clone();
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    if content_v.trim().is_empty() {
                                        note_error.set("Note content cannot be empty.".to_string());
                                        note_submitting.set(false);
                                        return;
                                    }
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
                        value: note_content.read().clone(),
                        oninput: move |e: FormEvent| note_content.set(e.value()),
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
    let dd_class = if props.nowrap {
        "text-sm text-gray-900 dark:text-white text-right whitespace-nowrap"
    } else {
        "text-sm text-gray-900 dark:text-white text-right"
    };
    rsx! {
        div { class: "flex justify-between items-baseline gap-3",
            dt { class: "text-sm text-gray-500 dark:text-gray-400 flex-shrink-0", "{props.label}" }
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
                        class: "absolute left-4 top-4 -ml-px h-full w-0.5 bg-gray-200 dark:bg-gray-700",
                        aria_hidden: "true",
                    }
                }
                div { class: "relative flex space-x-3",
                    div {
                        span { class: "h-8 w-8 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center ring-8 ring-white dark:ring-gray-800",
                            UserCircleIcon { size: IconSize::Small, class: "text-blue-600 dark:text-blue-400".to_string() }
                        }
                    }
                    div { class: "flex min-w-0 flex-1 justify-between space-x-4 pt-1.5",
                        div {
                            p { class: "text-sm text-gray-500 dark:text-gray-400",
                                span { class: "font-medium text-gray-900 dark:text-white", "{props.user}" }
                                " {props.action}"
                            }
                            if let Some(content) = &props.content {
                                div { class: "mt-2 text-sm text-gray-700 dark:text-gray-300 bg-gray-50 dark:bg-gray-800 rounded-md p-3",
                                    "{content}"
                                }
                            }
                        }
                        div { class: "whitespace-nowrap text-right text-sm text-gray-500 dark:text-gray-400",
                            "{props.time}"
                        }
                    }
                }
            }
        }
    }
}

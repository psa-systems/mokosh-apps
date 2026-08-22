//! Dashboard page, wired to the reports + list APIs.

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    ticket_status_badge, use_page_title, Badge, BadgeVariant, Card, ClockIcon, FolderIcon,
    PageHeader, StatCard, StatCardTone, Table, TableBody, TableCell, TableEmptyRow, TableHead,
    TableHeader, TableRow, TicketIcon,
};
use crate::modules::calendar::DispatchResponse;
use crate::utils::Paginated;
use crate::Route;

/// `GET /api/v1/reports/dashboard`.
#[derive(Clone, Debug, Default, Deserialize)]
struct DashboardReport {
    #[serde(default)]
    open_by_priority: Vec<Bucket>,
    #[serde(default)]
    sla_warnings: i64,
    #[serde(default)]
    sla_breached: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct Bucket {
    #[serde(default)]
    label: String,
    #[serde(default)]
    count: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Summary {
    #[serde(default)]
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DashTicket {
    id: uuid::Uuid,
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: Summary,
    #[serde(default)]
    priority: Summary,
}

#[derive(Clone, Debug, Deserialize)]
struct DashTimeEntry {
    date: NaiveDate,
    #[serde(default)]
    duration_minutes: i64,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct DashProject {
    #[serde(default)]
    status: String,
}

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

fn priority_badge(name: &str) -> BadgeVariant {
    match name.to_lowercase().as_str() {
        "critical" | "high" => BadgeVariant::Red,
        "medium" => BadgeVariant::Yellow,
        "low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    }
}

/// Main dashboard page component
#[component]
pub fn DashboardPage() -> Element {
    // MAPPS-351: the dashboard report is this page's PRIMARY resource. All
    // four loads go through `use_remote_resource`, which (a) preserves a
    // failed fetch instead of swallowing it to an empty default, and (b)
    // subscribes to the server-reachable flag so every resource
    // auto-refetches the instant the MAPPS-333 recovery poll flips the
    // server back. If the primary is `Unavailable` (failed while the server
    // is down) we render the honest ContentUnavailable body instead of an
    // empty, misleading dashboard. (These hooks run unconditionally and in
    // a fixed order before any early return, per the rules of hooks.)
    let report_data = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<DashboardReport>("/reports/dashboard").await
    });
    let tickets = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Paginated<DashTicket>>("/tickets")
            .await
            .map(|p| p.data)
    })
    .value_or_default();
    let time_entries = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Paginated<DashTimeEntry>>("/time-entries")
            .await
            .map(|p| p.data)
    })
    .value_or_default();
    let projects = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Paginated<DashProject>>("/projects")
            .await
            .map(|p| p.data)
    })
    .value_or_default();

    // MAPPS-366: set the tab title unconditionally, BEFORE the unavailable
    // early return (rules of hooks). The persistent AppShell reads it.
    use_page_title("Dashboard");
    if report_data.is_unavailable() {
        // Already on the dashboard, so no self-referential "Go to dashboard".
        return rsx! {
            crate::components::ContentUnavailable { title: "Dashboard".to_string(), show_dashboard_link: false }
        };
    }
    // MAPPS-404: capture the loading state before value_or_default() consumes
    // the resource, so the body below can gate on the primary report's first
    // fetch and show a skeleton instead of zeroed stats / empty tables.
    let is_loading = report_data.is_loading();
    let report = report_data.value_or_default();

    // Stat values.
    let open_tickets: i64 = report.open_by_priority.iter().map(|b| b.count).sum();
    let active_projects = projects.iter().filter(|p| p.status == "active").count();
    let week_start = monday_of_week(Utc::now().date_naive());
    let week_minutes: i64 = time_entries
        .iter()
        .filter(|e| e.date >= week_start)
        .map(|e| e.duration_minutes)
        .sum();
    let hours_week = format!("{:.1}", week_minutes as f64 / 60.0);
    let open_label = open_tickets.to_string();
    let projects_label = active_projects.to_string();
    let breached_label = report.sla_breached.to_string();

    let recent_tickets: Vec<&DashTicket> = tickets.iter().take(5).collect();
    let recent_time: Vec<&DashTimeEntry> = time_entries.iter().take(5).collect();

    rsx! {
        PageHeader {
            title: "Dashboard",
            subtitle: "Welcome back! Here's what's happening today.",
                // MAPPS-256: entry point into the full-screen TV view,
                // shown only when the user has enabled it in settings.
                actions: rsx! {
                    if crate::hooks::tv_view::is_enabled() {
                        Link {
                            to: Route::DashboardTv {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Open TV view"
                        }
                    }
                },
            }

            if is_loading {
                // MAPPS-404: while the primary dashboard report is still
                // loading, show a shape-matching skeleton (stat-row + content
                // cards) instead of zeroed StatCards ("0") and empty
                // "No ... yet." tables that then pop to real data once the
                // fetch resolves. Reuses the shared shimmer style from
                // components/skeleton.rs so it matches the other loading views.
                div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-8",
                    for _ in 0..4 {
                        div { class: "rounded-lg border border-line bg-surface p-6 space-y-4",
                            div { class: "h-4 w-1/2 bg-surface-2 rounded animate-pulse" }
                            div { class: "h-6 w-1/3 bg-surface-2 rounded animate-pulse" }
                        }
                    }
                }
                div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                    for _ in 0..4 {
                        crate::components::DetailSkeleton {}
                    }
                }
            } else {
            // Stats row
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-8",
                StatCard {
                    label: "Open Tickets",
                    value: "{open_label}",
                    icon: rsx!(TicketIcon { class: "h-6 w-6".to_string() }),
                }
                StatCard {
                    label: "Hours This Week",
                    value: "{hours_week}",
                    icon: rsx!(ClockIcon { class: "h-6 w-6".to_string() }),
                }
                StatCard {
                    label: "Active Projects",
                    value: "{projects_label}",
                    icon: rsx!(FolderIcon { class: "h-6 w-6".to_string() }),
                }
                StatCard {
                    label: "SLA Breached",
                    value: "{breached_label}",
                    icon_tone: StatCardTone::Danger,
                    icon: rsx!(TicketIcon { class: "h-6 w-6".to_string() }),
                }
            }

            // Main content grid
            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                // Recent tickets
                Card {
                    title: "Recent Tickets",
                    actions: rsx! {
                        Link {
                            to: Route::TicketList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "View all"
                        }
                    },
                    padding: false,
                    Table {
                        TableHead {
                            TableRow {
                                TableHeader { "Ticket" }
                                TableHeader { "Status" }
                                TableHeader { "Priority" }
                            }
                        }
                        TableBody {
                            if recent_tickets.is_empty() {
                                // MAPPS-388: centered across the table.
                                TableEmptyRow { columns: 3, class: "text-subtle italic", "No tickets yet." }
                            } else {
                                for t in recent_tickets.iter() {
                                    {
                                        let sv = ticket_status_badge(&t.status.name);
                                        let pv = priority_badge(&t.priority.name);
                                        let tid = t.id.to_string();
                                        let snap = if t.status.name.is_empty() { "-".to_string() } else { t.status.name.clone() };
                                        let pnap = if t.priority.name.is_empty() { "-".to_string() } else { t.priority.name.clone() };
                                        rsx! {
                                            TableRow {
                                                key: "{tid}",
                                                TableCell {
                                                    Link {
                                                        to: Route::TicketDetail { id: tid.clone() },
                                                        class: "block",
                                                        span { class: "font-medium text-accent", "{t.ticket_number}" }
                                                        p { class: "text-muted text-xs truncate max-w-xs", "{t.title}" }
                                                    }
                                                }
                                                TableCell { Badge { variant: sv, "{snap}" } }
                                                TableCell { Badge { variant: pv, "{pnap}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Open tickets by priority
                Card { title: "Open Tickets by Priority",
                    if report.open_by_priority.is_empty() {
                        p { class: "text-sm text-subtle italic", "No open tickets." }
                    } else {
                        div { class: "space-y-3",
                            for b in report.open_by_priority.iter() {
                                {
                                    let pv = priority_badge(&b.label);
                                    let count = b.count.to_string();
                                    rsx! {
                                        div { class: "flex items-center justify-between",
                                            Badge { variant: pv, "{b.label}" }
                                            span { class: "font-medium text-content", "{count}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // SLA status
                Card { title: "SLA Status",
                    div { class: "space-y-4",
                        div { class: "flex justify-between items-center p-3 rounded-lg bg-yellow-100 dark:bg-yellow-900/20",
                            span { class: "text-sm font-medium text-yellow-800 dark:text-yellow-200", "At risk" }
                            span { class: "text-lg font-bold text-yellow-800 dark:text-yellow-200", "{report.sla_warnings}" }
                        }
                        div { class: "flex justify-between items-center p-3 rounded-lg bg-red-100 dark:bg-red-900/20",
                            span { class: "text-sm font-medium text-red-700 dark:text-red-200", "Breached" }
                            span { class: "text-lg font-bold text-red-700 dark:text-red-200", "{report.sla_breached}" }
                        }
                    }
                }

                // Recent time entries
                Card {
                    title: "Recent Time Entries",
                    actions: rsx! {
                        Link {
                            to: Route::TimeEntryList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "View all"
                        }
                    },
                    padding: false,
                    Table {
                        TableHead {
                            TableRow {
                                TableHeader { "Date" }
                                TableHeader { "Description" }
                                TableHeader { "Hours" }
                            }
                        }
                        TableBody {
                            if recent_time.is_empty() {
                                // MAPPS-388: centered across the table.
                                TableEmptyRow { columns: 3, class: "text-subtle italic", "No time logged yet." }
                            } else {
                                for e in recent_time.iter() {
                                    {
                                        let hrs = crate::utils::duration::fmt_duration(e.duration_minutes);
                                        let note = e
                                            .notes
                                            .clone()
                                            .filter(|s| !s.trim().is_empty())
                                            .unwrap_or_else(|| "(no description)".to_string());
                                        let date = e.date.to_string();
                                        rsx! {
                                            TableRow {
                                                TableCell { class: "text-muted", "{date}" }
                                                TableCell { "{note}" }
                                                TableCell { class: "font-medium", "{hrs}" }
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
}

// ============================================================================
// MAPPS-256: full-screen, team-scoped wall-monitor "TV view".
// ============================================================================

/// Minimal user row from `GET /api/v1/auth/users`, used to resolve an
/// appointment's `assigned_to_id` to a technician name on the board. The
/// endpoint is server-gated to Admin / Manager; lower roles get an empty
/// list and the board falls back to a short id.
#[derive(Clone, Debug, Default, Deserialize)]
struct TvUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

impl TvUser {
    fn display_name(&self) -> String {
        if !self.full_name.trim().is_empty() {
            return self.full_name.clone();
        }
        let joined = format!("{} {}", self.first_name, self.last_name);
        let joined = joined.trim();
        if joined.is_empty() {
            "Unknown".to_string()
        } else {
            joined.to_string()
        }
    }
}

/// Empty dispatch payload used as the loading / error fallback so the
/// board renders an empty table rather than blanking out.
fn empty_dispatch() -> DispatchResponse {
    DispatchResponse {
        appointments: Vec::new(),
        availability: Vec::new(),
        time_off: Vec::new(),
        on_call: Vec::new(),
    }
}

/// First 8 chars of a uuid, for when a technician name can't be resolved.
fn short_id(id: &uuid::Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// Full-screen team-scoped TV view for wall monitors (MAPPS-256).
///
/// Renders WITHOUT `AppLayout`: a bare full-bleed container with the
/// essential board (today's technician appointments + open ticket
/// status), no `TopBar` / `Sidebar` / banners / `ToastRoot` and no action
/// buttons. Data is team-scoped via the PMS-406 parameters: the ticket
/// list uses `?team_id=<uuid>` when a single team is pinned in settings,
/// otherwise the server-resolved `?my_teams=true` union, so the board
/// never shows tickets outside the viewer's team(s). The KPI aggregate
/// (`/reports/dashboard`) is team-scoped only when a single team is
/// pinned (the endpoint scopes to one team, not a union). Every resource
/// re-runs on org switch (`active_tenant_generation()`) and on a 30s
/// timer so an unattended monitor stays current.
#[component]
pub fn DashboardTvPage() -> Element {
    let auth = crate::hooks::use_auth();

    // Periodic refresh for the unattended wall monitor. Bumps a counter
    // every 30s; each resource reads it so it re-fetches. Mirrors the
    // token-refresh loop in hooks/auth.rs.
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            #[cfg(feature = "web")]
            {
                crate::platform::timer::sleep_ms(30_000).await;
                tick += 1;
            }
            #[cfg(not(feature = "web"))]
            {
                break;
            }
        }
    });

    // Team scope, read at render so a settings change applies on remount.
    // Empty string = the "my teams" union; a uuid = that single team.
    let team = crate::hooks::tv_view::selected_team();

    // Tickets, team-scoped (PMS-406). `my_teams=true` resolves the
    // caller's teams server-side so no other team's tickets are shipped.
    let tickets_resource = {
        let team = team.clone();
        use_resource(move || {
            let team = team.clone();
            async move {
                let _gen = crate::hooks::fetch::active_tenant_generation();
                let _tick = tick();
                let path = if team.is_empty() {
                    "/tickets?my_teams=true".to_string()
                } else {
                    format!("/tickets?team_id={team}")
                };
                crate::hooks::fetch::api::get_authed::<Paginated<DashTicket>>(&path)
                    .await
                    .ok()
                    .map(|p| p.data)
                    .unwrap_or_default()
            }
        })
    };

    // KPI aggregate. Team-scoped only when a single team is pinned.
    let report_resource = {
        let team = team.clone();
        use_resource(move || {
            let team = team.clone();
            async move {
                let _gen = crate::hooks::fetch::active_tenant_generation();
                let _tick = tick();
                let path = if team.is_empty() {
                    "/reports/dashboard".to_string()
                } else {
                    format!("/reports/dashboard?team_id={team}")
                };
                crate::hooks::fetch::api::get_authed::<DashboardReport>(&path)
                    .await
                    .ok()
                    .unwrap_or_default()
            }
        })
    };

    // Today's dispatch (technicians, locations, appointments).
    let dispatch_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _tick = tick();
        #[cfg(feature = "web")]
        {
            let from = Utc::now()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
            let to = from + Duration::days(1);
            let path = format!(
                "/dispatch?from={}&to={}",
                from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                to.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            );
            crate::hooks::fetch::api::get_authed::<DispatchResponse>(&path)
                .await
                .ok()
                .unwrap_or_else(empty_dispatch)
        }
        #[cfg(not(feature = "web"))]
        {
            empty_dispatch()
        }
    });

    // Technician names. Gated to Admin / Manager server-side; an empty
    // list just falls the board back to short ids.
    let users_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _tick = tick();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_all_authed::<TvUser>("/auth/users")
                .await
                .unwrap_or_else(|e| {
                    // Best-effort: the board falls back to short ids.
                    tracing::warn!("technician-name load failed: {e}");
                    Vec::new()
                })
        }
        #[cfg(not(feature = "web"))]
        {
            Vec::<TvUser>::new()
        }
    });

    let tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let report = report_resource.read_unchecked().clone().unwrap_or_default();
    let dispatch = dispatch_resource
        .read_unchecked()
        .clone()
        .unwrap_or_else(empty_dispatch);
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    let users_map: std::collections::HashMap<uuid::Uuid, String> =
        users.iter().map(|u| (u.id, u.display_name())).collect();

    let open_tickets: i64 = report.open_by_priority.iter().map(|b| b.count).sum();
    let scope_label = if team.is_empty() {
        "All my teams".to_string()
    } else {
        format!("Team {}", short_id_str(&team))
    };
    let org_name = auth
        .read()
        .active_org_name()
        .unwrap_or("Live board")
        .to_string();

    let mut appointments = dispatch.appointments.clone();
    appointments.sort_by_key(|a| a.start_time);

    rsx! {
        // Bare, chrome-less full-bleed container. No AppLayout, so no
        // TopBar / Sidebar / banners / ToastRoot mount above it.
        div { class: "h-screen w-screen bg-app text-content overflow-hidden flex flex-col p-6",
            // Header: org + scope, no buttons.
            div { class: "flex items-end justify-between mb-5",
                div {
                    h1 { class: "text-4xl font-bold tracking-tight", "{org_name}" }
                    p { class: "text-lg text-muted mt-1", "Dispatch & ticket board" }
                }
                div { class: "text-right",
                    div { class: "text-sm uppercase tracking-wide text-subtle", "Scope" }
                    div { class: "text-2xl font-semibold", "{scope_label}" }
                }
            }

            // KPI strip.
            div { class: "grid grid-cols-3 gap-5 mb-6",
                div { class: "rounded-xl bg-surface p-5",
                    div { class: "text-sm uppercase tracking-wide text-subtle", "Open Tickets" }
                    div { class: "text-5xl font-bold mt-1", "{open_tickets}" }
                }
                div { class: "rounded-xl bg-surface p-5",
                    div { class: "text-sm uppercase tracking-wide text-subtle", "SLA At Risk" }
                    div { class: "text-5xl font-bold mt-1 text-yellow-600 dark:text-yellow-400", "{report.sla_warnings}" }
                }
                div { class: "rounded-xl bg-surface p-5",
                    div { class: "text-sm uppercase tracking-wide text-subtle", "SLA Breached" }
                    div { class: "text-5xl font-bold mt-1 text-red-600 dark:text-red-400", "{report.sla_breached}" }
                }
            }

            // Body: technicians/appointments on the left, ticket status on
            // the right. One essential table each, no actions.
            div { class: "grid grid-cols-2 gap-6 flex-1 overflow-hidden",
                div { class: "rounded-xl bg-surface p-4 flex flex-col overflow-hidden",
                    h2 { class: "text-xl font-semibold mb-3", "Today's Appointments" }
                    div { class: "overflow-y-auto",
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Technician" }
                                    TableHeader { "Appointment" }
                                    TableHeader { "Location" }
                                    TableHeader { "Time" }
                                    TableHeader { "Status" }
                                }
                            }
                            TableBody {
                                if appointments.is_empty() {
                                    TableRow {
                                        TableCell { class: "text-subtle italic", "No appointments scheduled today." }
                                    }
                                } else {
                                    for a in appointments.iter() {
                                        {
                                            let tech = users_map
                                                .get(&a.assigned_to_id)
                                                .cloned()
                                                .unwrap_or_else(|| short_id(&a.assigned_to_id));
                                            let loc = a
                                                .location
                                                .clone()
                                                .filter(|s| !s.trim().is_empty())
                                                .unwrap_or_else(|| "-".to_string());
                                            let time = format!(
                                                "{}-{}",
                                                a.start_time.format("%H:%M"),
                                                a.end_time.format("%H:%M")
                                            );
                                            let status = if a.status.is_empty() {
                                                "-".to_string()
                                            } else {
                                                a.status.clone()
                                            };
                                            let aid = a.id.to_string();
                                            rsx! {
                                                TableRow { key: "{aid}",
                                                    TableCell { class: "font-medium", "{tech}" }
                                                    TableCell { "{a.title}" }
                                                    TableCell { class: "text-muted", "{loc}" }
                                                    TableCell { class: "text-muted whitespace-nowrap", "{time}" }
                                                    TableCell { "{status}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "rounded-xl bg-surface p-4 flex flex-col overflow-hidden",
                    h2 { class: "text-xl font-semibold mb-3", "Ticket Status" }
                    div { class: "overflow-y-auto",
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Ticket" }
                                    TableHeader { "Title" }
                                    TableHeader { "Status" }
                                }
                            }
                            TableBody {
                                if tickets.is_empty() {
                                    TableRow {
                                        TableCell { class: "text-subtle italic", "No tickets for this scope." }
                                    }
                                } else {
                                    for t in tickets.iter() {
                                        {
                                            let sv = ticket_status_badge(&t.status.name);
                                            let snap = if t.status.name.is_empty() {
                                                "-".to_string()
                                            } else {
                                                t.status.name.clone()
                                            };
                                            let tid = t.id.to_string();
                                            rsx! {
                                                TableRow { key: "{tid}",
                                                    TableCell { class: "font-medium", "{t.ticket_number}" }
                                                    TableCell { class: "truncate max-w-md", "{t.title}" }
                                                    TableCell { Badge { variant: sv, "{snap}" } }
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
    }
}

/// First 8 chars of a uuid string (already a string in the pref).
fn short_id_str(id: &str) -> String {
    id.chars().take(8).collect()
}

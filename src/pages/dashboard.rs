//! Dashboard page, wired to the reports + list APIs.

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Card, ClockIcon, FolderIcon, PageHeader, StatCard, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow, TicketIcon,
};
use crate::Route;

#[derive(Clone, Debug, Deserialize)]
struct Paginated<T> {
    data: Vec<T>,
}

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

fn status_badge(name: &str) -> BadgeVariant {
    match name.to_lowercase().as_str() {
        "open" => BadgeVariant::Blue,
        "in_progress" | "in progress" => BadgeVariant::Yellow,
        "resolved" => BadgeVariant::Green,
        "closed" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    }
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
    let report_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<DashboardReport>("/reports/dashboard")
            .await
            .ok()
            .unwrap_or_default()
    });
    let tickets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<DashTicket>>("/tickets")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let time_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<DashTimeEntry>>("/time-entries")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let projects_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<DashProject>>("/projects")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let report = report_resource.read_unchecked().clone().unwrap_or_default();
    let tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let time_entries = time_resource.read_unchecked().clone().unwrap_or_default();
    let projects = projects_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

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
        AppLayout { title: "Dashboard",
            PageHeader {
                title: "Dashboard",
                subtitle: "Welcome back! Here's what's happening today.",
            }

            // Stats row
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-8",
                StatCard {
                    label: "Open Tickets",
                    value: "{open_label}",
                    icon: rsx!(TicketIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "Hours This Week",
                    value: "{hours_week}",
                    icon: rsx!(ClockIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "Active Projects",
                    value: "{projects_label}",
                    icon: rsx!(FolderIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "SLA Breached",
                    value: "{breached_label}",
                    icon: rsx!(TicketIcon { class: "h-6 w-6 text-red-600".to_string() }),
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
                            class: "text-sm text-blue-600 hover:text-blue-500",
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
                                TableRow {
                                    TableCell { class: "text-gray-400 italic", "No tickets yet." }
                                }
                            } else {
                                for t in recent_tickets.iter() {
                                    {
                                        let sv = status_badge(&t.status.name);
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
                                                        span { class: "font-medium text-blue-600", "{t.ticket_number}" }
                                                        p { class: "text-gray-500 text-xs truncate max-w-xs", "{t.title}" }
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
                        p { class: "text-sm text-gray-400 italic", "No open tickets." }
                    } else {
                        div { class: "space-y-3",
                            for b in report.open_by_priority.iter() {
                                {
                                    let pv = priority_badge(&b.label);
                                    let count = b.count.to_string();
                                    rsx! {
                                        div { class: "flex items-center justify-between",
                                            Badge { variant: pv, "{b.label}" }
                                            span { class: "font-medium text-gray-900 dark:text-white", "{count}" }
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
                            class: "text-sm text-blue-600 hover:text-blue-500",
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
                                TableRow {
                                    TableCell { class: "text-gray-400 italic", "No time logged yet." }
                                }
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
                                                TableCell { class: "text-gray-500", "{date}" }
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

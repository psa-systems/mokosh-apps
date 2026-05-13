//! Dashboard page

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Card, ClockIcon, CurrencyIcon, FolderIcon, PageHeader,
    StatCard, Table, TableBody, TableCell, TableHead, TableHeader, TableRow, TicketIcon,
};
use crate::Route;

/// Main dashboard page component
#[component]
pub fn DashboardPage() -> Element {
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
                    value: "24",
                    change: "+3",
                    change_positive: false,
                    icon: rsx!(TicketIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "Hours This Week",
                    value: "32.5",
                    change: "+8.5",
                    change_positive: true,
                    icon: rsx!(ClockIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "Active Projects",
                    value: "8",
                    icon: rsx!(FolderIcon { class: "h-6 w-6 text-blue-600".to_string() }),
                }
                StatCard {
                    label: "Pending Invoices",
                    value: "$12,450",
                    icon: rsx!(CurrencyIcon { class: "h-6 w-6 text-blue-600".to_string() }),
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
                            RecentTicketRow {
                                number: "TKT-1234",
                                title: "Email server not responding",
                                status: "Open",
                                priority: "High",
                            }
                            RecentTicketRow {
                                number: "TKT-1233",
                                title: "New user setup request",
                                status: "In Progress",
                                priority: "Medium",
                            }
                            RecentTicketRow {
                                number: "TKT-1232",
                                title: "Printer configuration",
                                status: "Pending",
                                priority: "Low",
                            }
                            RecentTicketRow {
                                number: "TKT-1231",
                                title: "VPN connection issues",
                                status: "Open",
                                priority: "High",
                            }
                            RecentTicketRow {
                                number: "TKT-1230",
                                title: "Software license renewal",
                                status: "In Progress",
                                priority: "Medium",
                            }
                        }
                    }
                }

                // Upcoming tasks
                Card {
                    title: "Today's Schedule",
                    actions: rsx! {
                        Link {
                            to: Route::Calendar {},
                            class: "text-sm text-blue-600 hover:text-blue-500",
                            "View calendar"
                        }
                    },
                    div { class: "space-y-4",
                        ScheduleItem {
                            time: "9:00 AM",
                            title: "Onsite: Acme Corp",
                            description: "Server maintenance",
                        }
                        ScheduleItem {
                            time: "11:30 AM",
                            title: "Call: TechStart Inc",
                            description: "Quarterly review meeting",
                        }
                        ScheduleItem {
                            time: "2:00 PM",
                            title: "Remote: Global Widgets",
                            description: "Network troubleshooting",
                        }
                        ScheduleItem {
                            time: "4:00 PM",
                            title: "Internal",
                            description: "Team standup",
                        }
                    }
                }

                // SLA warnings
                Card {
                    title: "SLA Warnings",
                    div { class: "space-y-3",
                        SlaWarningItem {
                            ticket: "TKT-1234",
                            company: "Acme Corp",
                            time_remaining: "45 min",
                            level: "warning",
                        }
                        SlaWarningItem {
                            ticket: "TKT-1228",
                            company: "TechStart Inc",
                            time_remaining: "2 hours",
                            level: "info",
                        }
                        SlaWarningItem {
                            ticket: "TKT-1225",
                            company: "Global Widgets",
                            time_remaining: "Breached",
                            level: "danger",
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
                            TimeEntryRow {
                                date: "Today",
                                description: "TKT-1234: Email troubleshooting",
                                hours: "1.5",
                            }
                            TimeEntryRow {
                                date: "Today",
                                description: "TKT-1233: User onboarding",
                                hours: "2.0",
                            }
                            TimeEntryRow {
                                date: "Yesterday",
                                description: "Project: Network upgrade",
                                hours: "3.0",
                            }
                            TimeEntryRow {
                                date: "Yesterday",
                                description: "TKT-1230: License research",
                                hours: "0.5",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RecentTicketRowProps {
    number: String,
    title: String,
    status: String,
    priority: String,
}

#[component]
fn RecentTicketRow(props: RecentTicketRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Open" => BadgeVariant::Blue,
        "In Progress" => BadgeVariant::Yellow,
        "Pending" => BadgeVariant::Gray,
        "Resolved" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    let priority_variant = match props.priority.as_str() {
        "Critical" => BadgeVariant::Red,
        "High" => BadgeVariant::Red,
        "Medium" => BadgeVariant::Yellow,
        "Low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();
    let id = props.number.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::TicketDetail { id: id.clone() }); },
            TableCell {
                div {
                    span { class: "font-medium text-blue-600", "{props.number}" }
                    p { class: "text-gray-500 text-xs truncate max-w-xs", "{props.title}" }
                }
            }
            TableCell {
                Badge { variant: status_variant, "{props.status}" }
            }
            TableCell {
                Badge { variant: priority_variant, "{props.priority}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ScheduleItemProps {
    time: String,
    title: String,
    description: String,
}

#[component]
fn ScheduleItem(props: ScheduleItemProps) -> Element {
    rsx! {
        div { class: "flex items-start space-x-3",
            div { class: "flex-shrink-0 w-20 text-sm text-gray-500",
                "{props.time}"
            }
            div { class: "flex-1 min-w-0",
                p { class: "text-sm font-medium text-gray-900 dark:text-white truncate",
                    "{props.title}"
                }
                p { class: "text-sm text-gray-500 dark:text-gray-400",
                    "{props.description}"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SlaWarningItemProps {
    ticket: String,
    company: String,
    time_remaining: String,
    level: String,
}

#[component]
fn SlaWarningItem(props: SlaWarningItemProps) -> Element {
    // Audit P2-11/P2-12: previous palette had dark-red text on dark-red
    // bg in dark mode (and same for yellow), failing WCAG AA. Lighten
    // the foreground so the contrast ratio against the dark-tinted bg
    // crosses the 4.5:1 normal-text threshold.
    let (bg_class, text_class) = match props.level.as_str() {
        "danger" => (
            "bg-red-100 dark:bg-red-900/20",
            "text-red-700 dark:text-red-200",
        ),
        "warning" => (
            "bg-yellow-100 dark:bg-yellow-900/20",
            "text-yellow-800 dark:text-yellow-200",
        ),
        _ => (
            "bg-blue-100 dark:bg-blue-900/20",
            "text-blue-700 dark:text-blue-200",
        ),
    };

    let navigator = use_navigator();
    let id = props.ticket.clone();

    rsx! {
        div {
            class: "flex items-center justify-between p-3 rounded-lg cursor-pointer hover:opacity-80 transition-opacity {bg_class}",
            onclick: move |_| { navigator.push(Route::TicketDetail { id: id.clone() }); },
            div {
                p { class: "text-sm font-medium text-gray-900 dark:text-white",
                    "{props.ticket}"
                }
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "{props.company}"
                }
            }
            span { class: "text-sm font-medium {text_class}",
                "{props.time_remaining}"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimeEntryRowProps {
    date: String,
    description: String,
    hours: String,
}

#[component]
fn TimeEntryRow(props: TimeEntryRowProps) -> Element {
    // Description is shaped like "TKT-1234: Email troubleshooting" on
    // the dashboard's mock data. If we can pull the leading TKT- token
    // out, navigate straight to that ticket; otherwise fall back to
    // the full time-entry list.
    let navigator = use_navigator();
    let target = match props.description.split_once(':') {
        Some((token, _)) if token.starts_with("TKT-") => Route::TicketDetail {
            id: token.to_string(),
        },
        _ => Route::TimeEntryList {},
    };

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(target.clone()); },
            TableCell { class: "text-gray-500",
                "{props.date}"
            }
            TableCell {
                "{props.description}"
            }
            TableCell { class: "font-medium",
                "{props.hours}h"
            }
        }
    }
}

//! Reports pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ChartIcon, IconSize, PageHeader,
};
use crate::Route;

/// Reports home page
#[component]
pub fn ReportsPage() -> Element {
    rsx! {
        AppLayout { title: "Reports",
            PageHeader {
                title: "Reports",
                subtitle: "Analytics and business intelligence",
            }

            // Report categories
            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
                ReportCategory {
                    title: "Service Desk",
                    description: "Ticket metrics, SLA performance, and support analytics",
                    reports: vec![
                        ("ticket-volume", "Ticket Volume"),
                        ("sla-performance", "SLA Performance"),
                        ("resolution-time", "Resolution Time"),
                        ("tech-performance", "Technician Performance"),
                    ],
                }
                ReportCategory {
                    title: "Time & Billing",
                    description: "Time tracking, utilization, and billing reports",
                    reports: vec![
                        ("utilization", "Technician Utilization"),
                        ("billable-hours", "Billable Hours"),
                        ("timesheet-summary", "Timesheet Summary"),
                    ],
                }
                ReportCategory {
                    title: "Financial",
                    description: "Revenue, invoicing, and profitability reports",
                    reports: vec![
                        ("revenue", "Revenue Summary"),
                        ("ar-aging", "A/R Aging"),
                        ("profitability", "Client Profitability"),
                    ],
                }
                ReportCategory {
                    title: "Projects",
                    description: "Project status, budget tracking, and progress reports",
                    reports: vec![
                        ("project-status", "Project Status"),
                        ("budget-tracking", "Budget vs Actual"),
                        ("milestone-tracking", "Milestone Tracking"),
                    ],
                }
                ReportCategory {
                    title: "Clients",
                    description: "Client activity, asset, and contract reports",
                    reports: vec![
                        ("client-summary", "Client Summary"),
                        ("asset-inventory", "Asset Inventory"),
                        ("contract-renewals", "Contract Renewals"),
                    ],
                }
                ReportCategory {
                    title: "Custom Reports",
                    description: "Build your own custom reports",
                    reports: vec![
                        ("report-builder", "Report Builder"),
                    ],
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ReportCategoryProps {
    title: String,
    description: String,
    reports: Vec<(&'static str, &'static str)>,
}

#[component]
fn ReportCategory(props: ReportCategoryProps) -> Element {
    rsx! {
        Card {
            h3 { class: "text-lg font-medium text-gray-900 dark:text-white mb-2",
                "{props.title}"
            }
            p { class: "text-sm text-gray-500 dark:text-gray-400 mb-4",
                "{props.description}"
            }
            ul { class: "space-y-2",
                for (report_type, name) in props.reports.iter() {
                    li {
                        Link {
                            to: Route::ReportDetail { report_type: report_type.to_string() },
                            class: "flex items-center text-sm text-blue-600 hover:text-blue-500",
                            ChartIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}

/// Report detail page
#[derive(Props, Clone, PartialEq)]
pub struct ReportDetailPageProps {
    pub report_type: String,
}

#[component]
pub fn ReportDetailPage(props: ReportDetailPageProps) -> Element {
    let report_title = match props.report_type.as_str() {
        "ticket-volume" => "Ticket Volume Report",
        "sla-performance" => "SLA Performance Report",
        "utilization" => "Technician Utilization Report",
        "revenue" => "Revenue Summary Report",
        _ => "Report",
    };

    rsx! {
        AppLayout { title: report_title,
            PageHeader {
                title: report_title,
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Export PDF" }
                    Button { variant: ButtonVariant::Secondary, "Export CSV" }
                    Button { variant: ButtonVariant::Primary, "Schedule" }
                },
            }

            // Date range filter
            Card { class: "mb-6",
                div { class: "flex flex-wrap gap-4 items-center",
                    // P2-14: Dioxus 0.7 doesn't render `selected:` on a
                    // bare `option`, so the user saw a blank input.
                    // Drive the displayed text via the parent select's
                    // `value:` attribute (which Dioxus 0.7 does honor).
                    div { class: "flex items-center space-x-2",
                        label { class: "text-sm text-gray-500", "Date Range:" }
                        select {
                            class: "rounded-md border-gray-300 text-sm",
                            value: "30",
                            option { value: "7", "Last 7 days" }
                            option { value: "30", "Last 30 days" }
                            option { value: "month", "This Month" }
                            option { value: "last-month", "Last Month" }
                            option { value: "quarter", "This Quarter" }
                            option { value: "custom", "Custom" }
                        }
                    }
                    div { class: "flex items-center space-x-2",
                        label { class: "text-sm text-gray-500", "Group By:" }
                        select {
                            class: "rounded-md border-gray-300 text-sm",
                            value: "week",
                            option { value: "day", "Day" }
                            option { value: "week", "Week" }
                            option { value: "month", "Month" }
                        }
                    }
                    Button { variant: ButtonVariant::Secondary, "Apply Filters" }
                }
            }

            // Report content (placeholder)
            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                Card { title: "Summary",
                    div { class: "grid grid-cols-2 gap-4",
                        div { class: "text-center p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
                            p { class: "text-sm text-gray-500", "Total Tickets" }
                            p { class: "text-3xl font-bold text-gray-900 dark:text-white", "342" }
                        }
                        div { class: "text-center p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
                            p { class: "text-sm text-gray-500", "Avg. Resolution Time" }
                            p { class: "text-3xl font-bold text-gray-900 dark:text-white", "4.2h" }
                        }
                        div { class: "text-center p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
                            p { class: "text-sm text-gray-500", "SLA Compliance" }
                            p { class: "text-3xl font-bold text-green-600", "94%" }
                        }
                        div { class: "text-center p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
                            p { class: "text-sm text-gray-500", "Customer Satisfaction" }
                            p { class: "text-3xl font-bold text-blue-600", "4.7/5" }
                        }
                    }
                }

                Card { title: "Trend",
                    ChartComingSoon { caption: "Ticket volume over time" }
                }

                Card { title: "By Status",
                    ChartComingSoon { caption: "Tickets by status" }
                }

                Card { title: "By Priority",
                    ChartComingSoon { caption: "Tickets by priority" }
                }
            }

            // Data table
            Card { title: "Detailed Data", class: "mt-6",
                div { class: "overflow-x-auto",
                    table { class: "min-w-full divide-y divide-gray-200 dark:divide-gray-700",
                        thead { class: "bg-gray-50 dark:bg-gray-800",
                            tr {
                                th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase", "Week" }
                                th { class: "px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase", "New" }
                                th { class: "px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase", "Resolved" }
                                th { class: "px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase", "Avg. Time" }
                                th { class: "px-6 py-3 text-right text-xs font-medium text-gray-500 uppercase", "SLA %" }
                            }
                        }
                        tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
                            tr {
                                td { class: "px-6 py-4 text-sm text-gray-900 dark:text-white", "Dec 30 - Jan 5" }
                                td { class: "px-6 py-4 text-sm text-right", "85" }
                                td { class: "px-6 py-4 text-sm text-right", "78" }
                                td { class: "px-6 py-4 text-sm text-right", "3.8h" }
                                td { class: "px-6 py-4 text-sm text-right text-green-600", "96%" }
                            }
                            tr {
                                td { class: "px-6 py-4 text-sm text-gray-900 dark:text-white", "Jan 6 - Jan 12" }
                                td { class: "px-6 py-4 text-sm text-right", "92" }
                                td { class: "px-6 py-4 text-sm text-right", "88" }
                                td { class: "px-6 py-4 text-sm text-right", "4.1h" }
                                td { class: "px-6 py-4 text-sm text-right text-green-600", "94%" }
                            }
                            tr {
                                td { class: "px-6 py-4 text-sm text-gray-900 dark:text-white", "Jan 13 - Jan 19" }
                                td { class: "px-6 py-4 text-sm text-right", "78" }
                                td { class: "px-6 py-4 text-sm text-right", "71" }
                                td { class: "px-6 py-4 text-sm text-right", "4.5h" }
                                td { class: "px-6 py-4 text-sm text-right text-yellow-600", "89%" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ChartComingSoonProps {
    caption: &'static str,
}

#[component]
fn ChartComingSoon(props: ChartComingSoonProps) -> Element {
    rsx! {
        div { class: "h-64 flex flex-col items-center justify-center gap-2 rounded-md border border-dashed border-gray-300 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/40",
            ChartIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
            p { class: "text-sm font-medium text-gray-600 dark:text-gray-300", "Charts coming soon" }
            p { class: "text-xs text-gray-500 dark:text-gray-400", "{props.caption}" }
        }
    }
}

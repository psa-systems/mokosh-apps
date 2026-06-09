//! Reports pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Card, ChartIcon, IconSize, PageHeader};
use crate::Route;

#[derive(Clone, Debug, Deserialize)]
struct Bucket {
    #[serde(default)]
    label: String,
    #[serde(default)]
    count: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct IdCount {
    #[serde(default)]
    count: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TicketsReport {
    #[serde(default)]
    closed_total: i64,
    #[serde(default)]
    opened_by_status: Vec<Bucket>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TimeReport {
    #[serde(default)]
    minutes_by_user: Vec<IdCount>,
    #[serde(default)]
    minutes_by_work_type: Vec<IdCount>,
}

#[derive(Clone, Debug, Deserialize)]
struct AgingBucket {
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    total: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct BillingReport {
    #[serde(default)]
    invoiced: String,
    #[serde(default)]
    paid: String,
    #[serde(default)]
    outstanding: String,
    #[serde(default)]
    aging: Vec<AgingBucket>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DashReport {
    #[serde(default)]
    sla_warnings: i64,
    #[serde(default)]
    sla_breached: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ProjectsReport {
    #[serde(default)]
    by_status: Vec<Bucket>,
    #[serde(default)]
    budget_hours: String,
    #[serde(default)]
    budget_amount: String,
    #[serde(default)]
    actual_hours: String,
    #[serde(default)]
    actual_amount: String,
    #[serde(default)]
    tasks_total: i64,
    #[serde(default)]
    tasks_completed: i64,
    #[serde(default)]
    overdue: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ClientsReport {
    #[serde(default)]
    companies_total: i64,
    #[serde(default)]
    companies_active: i64,
    #[serde(default)]
    assets_total: i64,
    #[serde(default)]
    assets_by_type: Vec<Bucket>,
    #[serde(default)]
    warranty_expiring_90d: i64,
    #[serde(default)]
    contracts_active: i64,
    #[serde(default)]
    contracts_renewing_90d: i64,
}

/// Parse a `Decimal`-as-string money/hours field for display.
fn pf(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

/// Which backend report a `report_type` maps to. The reports landing page
/// lists more report types than the backend (PMS-93) implements; the
/// unmapped ones render an honest "not available yet" state.
#[derive(Clone, Copy, PartialEq)]
enum ReportKind {
    Tickets,
    Time,
    Billing,
    Projects,
    Clients,
    Unsupported,
}

fn report_kind(report_type: &str) -> ReportKind {
    match report_type {
        "ticket-volume" | "sla-performance" | "resolution-time" | "tech-performance" => {
            ReportKind::Tickets
        }
        "utilization" | "billable-hours" | "timesheet-summary" => ReportKind::Time,
        "revenue" | "ar-aging" | "profitability" => ReportKind::Billing,
        "project-status" | "budget-tracking" | "milestone-tracking" => ReportKind::Projects,
        "client-summary" | "asset-inventory" | "contract-renewals" => ReportKind::Clients,
        _ => ReportKind::Unsupported,
    }
}

/// Normalised view the detail page renders, built from whichever backend
/// report the `report_type` maps to.
#[derive(Clone, Debug, Default)]
struct ReportView {
    supported: bool,
    summary: Vec<(String, String)>,
    breakdown_title: String,
    breakdown: Vec<(String, String)>,
}

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
        "resolution-time" => "Resolution Time Report",
        "tech-performance" => "Technician Performance Report",
        "utilization" => "Technician Utilization Report",
        "billable-hours" => "Billable Hours Report",
        "timesheet-summary" => "Timesheet Summary Report",
        "revenue" => "Revenue Summary Report",
        "ar-aging" => "A/R Aging Report",
        "profitability" => "Client Profitability Report",
        "project-status" => "Project Status Report",
        "budget-tracking" => "Budget vs Actual Report",
        "milestone-tracking" => "Milestone Tracking Report",
        "client-summary" => "Client Summary Report",
        "asset-inventory" => "Asset Inventory Report",
        "contract-renewals" => "Contract Renewals Report",
        _ => "Report",
    };

    let report_type = props.report_type.clone();
    let view_resource = use_resource(move || {
        let rt = report_type.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            build_view(&rt).await
        }
    });
    let view = view_resource.read_unchecked().clone();
    let is_loading = view.is_none();
    let view = view.unwrap_or_default();

    rsx! {
        AppLayout { title: report_title,
            PageHeader { title: report_title, subtitle: "Live figures from the reports service" }

            if is_loading {
                Card { p { class: "text-sm text-gray-400", "Loading report…" } }
            } else if !view.supported {
                Card {
                    p { class: "text-sm text-gray-500 dark:text-gray-400",
                        "This report isn't available yet. The reports service powers the ticket, time, billing, project, and client reports. The custom report builder is planned but not implemented."
                    }
                }
            } else {
                div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                    Card { title: "Summary",
                        if view.summary.is_empty() {
                            p { class: "text-sm text-gray-400 italic", "No data for this period." }
                        } else {
                            div { class: "grid grid-cols-2 gap-4",
                                for (label , value) in view.summary.iter() {
                                    div { class: "text-center p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
                                        p { class: "text-sm text-gray-500", "{label}" }
                                        p { class: "text-3xl font-bold text-gray-900 dark:text-white", "{value}" }
                                    }
                                }
                            }
                        }
                    }

                    Card { title: "Trend",
                        ChartComingSoon { caption: "Time-series charts are coming soon" }
                    }
                }

                if !view.breakdown.is_empty() {
                    Card { title: "{view.breakdown_title}", class: "mt-6",
                        div { class: "overflow-x-auto",
                            table { class: "min-w-full divide-y divide-gray-200 dark:divide-gray-700",
                                tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
                                    for (k , v) in view.breakdown.iter() {
                                        tr {
                                            td { class: "px-6 py-3 text-sm text-gray-900 dark:text-white", "{k}" }
                                            td { class: "px-6 py-3 text-sm text-right font-medium", "{v}" }
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

/// Fetch the backend report a `report_type` maps to and normalise it into a
/// `ReportView`. Each fetch is best-effort (`.ok()`); billing requires a
/// manager role, so a lower-privilege session yields an empty billing view.
async fn build_view(report_type: &str) -> ReportView {
    match report_kind(report_type) {
        ReportKind::Tickets => {
            let tickets = crate::hooks::fetch::api::get_authed::<TicketsReport>("/reports/tickets")
                .await
                .unwrap_or_default();
            let dash = crate::hooks::fetch::api::get_authed::<DashReport>("/reports/dashboard")
                .await
                .unwrap_or_default();
            let open: i64 = tickets.opened_by_status.iter().map(|b| b.count).sum();
            ReportView {
                supported: true,
                summary: vec![
                    ("Tickets closed".into(), tickets.closed_total.to_string()),
                    ("Opened (period)".into(), open.to_string()),
                    ("SLA at-risk".into(), dash.sla_warnings.to_string()),
                    ("SLA breached".into(), dash.sla_breached.to_string()),
                ],
                breakdown_title: "Opened by status".into(),
                breakdown: tickets
                    .opened_by_status
                    .iter()
                    .map(|b| (b.label.clone(), b.count.to_string()))
                    .collect(),
            }
        }
        ReportKind::Time => {
            let time = crate::hooks::fetch::api::get_authed::<TimeReport>("/reports/time")
                .await
                .unwrap_or_default();
            let total_min: i64 = time.minutes_by_user.iter().map(|u| u.count).sum();
            ReportView {
                supported: true,
                summary: vec![
                    (
                        "Total hours".into(),
                        format!("{:.1}", total_min as f64 / 60.0),
                    ),
                    (
                        "Contributors".into(),
                        time.minutes_by_user.len().to_string(),
                    ),
                    (
                        "Work types".into(),
                        time.minutes_by_work_type.len().to_string(),
                    ),
                ],
                breakdown_title: String::new(),
                breakdown: Vec::new(),
            }
        }
        ReportKind::Billing => {
            let billing = crate::hooks::fetch::api::get_authed::<BillingReport>("/reports/billing")
                .await
                .unwrap_or_default();
            ReportView {
                supported: true,
                summary: vec![
                    ("Invoiced".into(), format!("${}", billing.invoiced)),
                    ("Paid".into(), format!("${}", billing.paid)),
                    ("Outstanding".into(), format!("${}", billing.outstanding)),
                ],
                breakdown_title: "A/R aging".into(),
                breakdown: billing
                    .aging
                    .iter()
                    .map(|a| (a.bucket.clone(), format!("${}", a.total)))
                    .collect(),
            }
        }
        ReportKind::Projects => {
            let p = crate::hooks::fetch::api::get_authed::<ProjectsReport>("/reports/projects")
                .await
                .unwrap_or_default();
            let tasks = format!("{}/{}", p.tasks_completed, p.tasks_total);
            ReportView {
                supported: true,
                summary: vec![
                    ("Budget hours".into(), format!("{:.1}", pf(&p.budget_hours))),
                    ("Actual hours".into(), format!("{:.1}", pf(&p.actual_hours))),
                    ("Budget $".into(), format!("${:.0}", pf(&p.budget_amount))),
                    ("Actual $".into(), format!("${:.0}", pf(&p.actual_amount))),
                    ("Tasks done".into(), tasks),
                    ("Overdue".into(), p.overdue.to_string()),
                ],
                breakdown_title: "Projects by status".into(),
                breakdown: p
                    .by_status
                    .iter()
                    .map(|b| (b.label.clone(), b.count.to_string()))
                    .collect(),
            }
        }
        ReportKind::Clients => {
            let c = crate::hooks::fetch::api::get_authed::<ClientsReport>("/reports/clients")
                .await
                .unwrap_or_default();
            let companies = format!("{}/{}", c.companies_active, c.companies_total);
            ReportView {
                supported: true,
                summary: vec![
                    ("Active companies".into(), companies),
                    ("Assets".into(), c.assets_total.to_string()),
                    ("Warranty < 90d".into(), c.warranty_expiring_90d.to_string()),
                    ("Active contracts".into(), c.contracts_active.to_string()),
                    (
                        "Renewing < 90d".into(),
                        c.contracts_renewing_90d.to_string(),
                    ),
                ],
                breakdown_title: "Assets by type".into(),
                breakdown: c
                    .assets_by_type
                    .iter()
                    .map(|b| (b.label.clone(), b.count.to_string()))
                    .collect(),
            }
        }
        ReportKind::Unsupported => ReportView::default(),
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

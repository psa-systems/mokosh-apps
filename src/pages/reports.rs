//! Reports pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, BarChart, BarChartDatum, Button, ButtonVariant, Card, ChartIcon, IconSize,
    PageHeader, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::utils::money::format_money_str;
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

// --- Custom report builder (PMS-180) ----------------------------------------

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct FieldSchema {
    key: String,
    label: String,
}

/// One source in the builder catalog (`GET /reports/custom/schema`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct SourceSchema {
    key: String,
    label: String,
    #[serde(default)]
    dimensions: Vec<FieldSchema>,
    #[serde(default)]
    measures: Vec<FieldSchema>,
    #[serde(default)]
    filters: Vec<FieldSchema>,
    #[serde(default)]
    has_date_range: bool,
}

/// Request body sent to `POST /reports/custom`.
#[derive(Clone, Debug, serde::Serialize)]
struct CustomSpec {
    source: String,
    dimensions: Vec<String>,
    measures: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    limit: i64,
}

/// Generic columns / rows / totals envelope returned by the builder.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct CustomResult {
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default)]
    rows: Vec<Vec<Option<String>>>,
    #[serde(default)]
    totals: std::collections::BTreeMap<String, String>,
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

/// MAPPS-641: the server report an export downloads, by kind. The sixteen
/// report pages here read five server reports between them, so an export is
/// the server's report rather than the page's slice of it; the button says
/// which. `None` for a kind the server does not serve.
fn export_key(kind: ReportKind) -> Option<&'static str> {
    match kind {
        ReportKind::Tickets => Some("tickets"),
        ReportKind::Time => Some("time"),
        ReportKind::Billing => Some("billing"),
        ReportKind::Projects => Some("projects"),
        ReportKind::Clients => Some("clients"),
        ReportKind::Unsupported => None,
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
    use_page_title("Reports");
    // MAPPS-357: N/A - this landing page is not data-driven. It renders a
    // static, hard-coded catalog of report categories with no fetch, so there
    // is no primary resource that can fail and no write control to gate.
    rsx! {
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
            h3 { class: "text-lg font-medium text-content mb-2",
                "{props.title}"
            }
            p { class: "text-sm text-muted mb-4",
                "{props.description}"
            }
            ul { class: "space-y-2",
                for (report_type, name) in props.reports.iter() {
                    li {
                        Link {
                            to: Route::ReportDetail { report_type: report_type.to_string() },
                            class: "flex items-center text-sm text-accent hover:opacity-90",
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
        "report-builder" => "Report Builder",
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
    use_page_title(report_title);

    // MAPPS-357: the normalised report view is this page's PRIMARY resource.
    // It flows through `use_remote_resource`, which preserves a failed fetch
    // (instead of the old `.unwrap_or_default()` that made an outage look like
    // an empty report) and auto-refetches on reconnect. `build_view` returns a
    // raw `Result`; a failure while the server is still reachable (e.g. a
    // role-gated billing 403) degrades to an empty view inside `build_view`, so
    // only a genuine outage reaches `Unavailable`. The hook reads
    // `active_tenant_generation` internally, so the explicit subscription the
    // old hand-rolled resource carried is no longer needed here.
    //
    // MAPPS-377: call this hook BEFORE the report-builder early return below so
    // hook order stays stable when the route param flips this instance between
    // the builder and a fixed report. `build_view` is a pure no-op
    // (ReportKind::Unsupported -> default) for the builder type, so hoisting it
    // fires no network for that path.
    let report_type = props.report_type.clone();
    let view_data = crate::hooks::use_remote_resource(move || {
        let rt = report_type.clone();
        async move { build_view(&rt).await }
    });

    // The custom report builder is its own interactive surface, not a
    // fixed report.
    if props.report_type == "report-builder" {
        return rsx! {
            PageHeader {
                title: "Custom Report Builder",
                subtitle: "Build a report from your own data",
            }
            CustomReportBuilder {}
        };
    }

    let is_loading = view_data.is_loading();
    if view_data.is_unavailable() {
        return rsx! {
            crate::components::ContentUnavailable { title: report_title.to_string() }
        };
    }
    let view = view_data.value_or_default();

    rsx! {
        PageHeader {
            title: report_title,
            subtitle: "Live figures from the reports service",
            actions: rsx! {
                // MAPPS-641: CSV and PDF of the server report this page reads
                // (PMS-876). Both carry the report's own gate server-side, so
                // a finance-only report answers a technician's click with the
                // role message rather than a file.
                if view.supported {
                    if let Some(key) = export_key(report_kind(&props.report_type)) {
                        crate::components::DownloadButton {
                            path: format!("/reports/{key}/export?format=csv"),
                            fallback_name: format!("{key}.csv"),
                            what: format!("the {key} report as CSV"),
                            label: "Download CSV".to_string(),
                            title: format!("The {key} report, every row, as CSV."),
                        }
                        crate::components::DownloadButton {
                            path: format!("/reports/{key}/export?format=pdf"),
                            fallback_name: format!("{key}.pdf"),
                            what: format!("the {key} report as PDF"),
                            label: "Download PDF".to_string(),
                            title: format!("The {key} report as a PDF, rendered now."),
                        }
                    }
                }
            },
        }

        if is_loading {
            crate::components::DetailSkeleton {} // PMS-353
        } else if !view.supported {
            Card {
                p { class: "text-sm text-muted",
                    "This report isn't available yet. The reports service powers the ticket, time, billing, project, and client reports. The custom report builder is planned but not implemented."
                }
            }
        } else {
            Card { title: "Summary",
                if view.summary.is_empty() {
                    p { class: "text-sm text-subtle italic", "No data for this period." }
                } else {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4",
                        for (label , value) in view.summary.iter() {
                            div { class: "text-center p-4 bg-app rounded-lg",
                                p { class: "text-sm text-muted", "{label}" }
                                p { class: "text-3xl font-bold text-content", "{value}" }
                            }
                        }
                    }
                }
            }

            if !view.breakdown.is_empty() {
                // MAPPS-297: render a chart alongside the table
                // whenever the breakdown values parse as numbers.
                // Currency strings (`$1,234.56`) and bare integers
                // both parse via `parse_chartable_value`; rows that
                // do not parse are dropped from the chart but kept
                // in the table so nothing is lost.
                {
                    let chart_data = chart_data_from_breakdown(&view.breakdown);
                    if !chart_data.is_empty() {
                        rsx! {
                            Card { title: "{view.breakdown_title}", class: "mt-6",
                                BarChart { data: chart_data, one_decimal: false }
                                div { class: "mt-4",
                                    Table {
                                        TableBody {
                                            for (k , v) in view.breakdown.iter() {
                                                TableRow {
                                                    TableCell { "{k}" }
                                                    TableCell { class: "text-right font-medium", "{v}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            Card { title: "{view.breakdown_title}", class: "mt-6",
                                Table {
                                    TableBody {
                                        for (k , v) in view.breakdown.iter() {
                                            TableRow {
                                                TableCell { "{k}" }
                                                TableCell { class: "text-right font-medium", "{v}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // MAPPS-297: every report ships at least one chart.
            // When the breakdown is empty (Time / others) but the
            // summary card has numeric metrics, render those as a
            // bar chart so the AC holds for every report type.
            if view.breakdown.is_empty() {
                {
                    let summary_chart = chart_data_from_breakdown(&view.summary);
                    if !summary_chart.is_empty() {
                        rsx! {
                            Card { title: "Summary chart".to_string(), class: "mt-6",
                                BarChart { data: summary_chart, one_decimal: true }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
        }
    }
}

/// Try to parse a breakdown value into a number for the chart.
/// Accepts plain ints (`42`), decimals (`12.5`), currency
/// (`$1,234.56`, `1,234.56 USD`), and percentages (`87%`). Returns
/// `None` for free-form strings so the chart silently drops rows that
/// don't parse rather than crashing on them.
fn parse_chartable_value(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '$' | ',' | '%' | ' '))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let numeric: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    numeric.parse::<f64>().ok()
}

fn chart_data_from_breakdown(rows: &[(String, String)]) -> Vec<BarChartDatum> {
    rows.iter()
        .filter_map(|(label, value)| {
            parse_chartable_value(value).map(|v| BarChartDatum {
                label: label.clone(),
                value: v,
            })
        })
        .collect()
}

/// MAPPS-357: fetch a report payload for `build_view`, degrading a failure
/// that happened while the server is still reachable (a role-gated 403, a
/// one-off decode error) to the type default - exactly the prior best-effort
/// `.unwrap_or_default()` behavior - while propagating a failure caused by an
/// unreachable server so the primary resource becomes `Unavailable` and the
/// page renders the honest outage state instead of a report full of zeros.
async fn fetch_or_default<T>(path: &str) -> Result<T, String>
where
    T: Default + serde::de::DeserializeOwned,
{
    match crate::hooks::fetch::api::get_authed::<T>(path).await {
        Ok(v) => Ok(v),
        // Reachable failure: preserve the old degrade-to-default behavior.
        Err(_) if crate::hooks::use_server_reachable() => Ok(T::default()),
        // Server unreachable: surface the outage to the caller.
        Err(e) => Err(e),
    }
}

/// Fetch the backend report a `report_type` maps to and normalise it into a
/// `ReportView`. MAPPS-357: returns `Err` only when the PRIMARY fetch fails
/// while the server is unreachable, so the page can surface an outage; a
/// failure while the server is still reachable degrades to the type default
/// via `fetch_or_default`. Billing requires a manager role, so a
/// lower-privilege session still yields an empty (but supported) billing view.
async fn build_view(report_type: &str) -> Result<ReportView, String> {
    match report_kind(report_type) {
        ReportKind::Tickets => {
            let tickets = fetch_or_default::<TicketsReport>("/reports/tickets").await?;
            // Secondary KPI aggregate: degrade to zeros on any failure (during
            // a real outage the primary fetch above already returned early).
            let dash = crate::hooks::fetch::api::get_authed::<DashReport>("/reports/dashboard")
                .await
                .unwrap_or_default();
            let open: i64 = tickets.opened_by_status.iter().map(|b| b.count).sum();
            Ok(ReportView {
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
            })
        }
        ReportKind::Time => {
            let time = fetch_or_default::<TimeReport>("/reports/time").await?;
            let total_min: i64 = time.minutes_by_user.iter().map(|u| u.count).sum();
            Ok(ReportView {
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
            })
        }
        ReportKind::Billing => {
            let billing = fetch_or_default::<BillingReport>("/reports/billing").await?;
            Ok(ReportView {
                supported: true,
                summary: vec![
                    ("Invoiced".into(), format_money_str(&billing.invoiced)),
                    ("Paid".into(), format_money_str(&billing.paid)),
                    ("Outstanding".into(), format_money_str(&billing.outstanding)),
                ],
                breakdown_title: "A/R aging".into(),
                breakdown: billing
                    .aging
                    .iter()
                    .map(|a| (a.bucket.clone(), format_money_str(&a.total)))
                    .collect(),
            })
        }
        ReportKind::Projects => {
            let p = fetch_or_default::<ProjectsReport>("/reports/projects").await?;
            let tasks = format!("{}/{}", p.tasks_completed, p.tasks_total);
            Ok(ReportView {
                supported: true,
                summary: vec![
                    ("Budget hours".into(), format!("{:.1}", pf(&p.budget_hours))),
                    ("Actual hours".into(), format!("{:.1}", pf(&p.actual_hours))),
                    ("Budget $".into(), format_money_str(&p.budget_amount)),
                    ("Actual $".into(), format_money_str(&p.actual_amount)),
                    ("Tasks done".into(), tasks),
                    ("Overdue".into(), p.overdue.to_string()),
                ],
                breakdown_title: "Projects by status".into(),
                breakdown: p
                    .by_status
                    .iter()
                    .map(|b| (b.label.clone(), b.count.to_string()))
                    .collect(),
            })
        }
        ReportKind::Clients => {
            let c = fetch_or_default::<ClientsReport>("/reports/clients").await?;
            let companies = format!("{}/{}", c.companies_active, c.companies_total);
            Ok(ReportView {
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
            })
        }
        ReportKind::Unsupported => Ok(ReportView::default()),
    }
}

/// Interactive custom report builder. Fetches the whitelisted catalog from
/// the backend, lets the user pick a source + dimensions + measures (+ an
/// optional date range), then POSTs the spec and renders the returned
/// columns / rows / totals table. No raw SQL is ever exposed (PMS-180).
#[component]
fn CustomReportBuilder() -> Element {
    // MAPPS-357: N/A for the ContentUnavailable retrofit. This is a private
    // helper component (not a routed `*Page`), and it already surfaces an
    // explicit failure state: the `load_err` branch below renders "Could not
    // load the report catalog. The reports service may be unavailable." when
    // the schema fetch fails, so an outage does not read as an empty builder.
    // "Run report" POSTs a query spec (a read that computes a report, not a
    // create/update/delete), so it is intentionally left enabled like a
    // refresh control rather than gated on `can_mutate`.
    let schema_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Vec<SourceSchema>>("/reports/custom/schema").await
    });

    let mut source = use_signal(String::new);
    let mut dims = use_signal(Vec::<String>::new);
    let mut measures = use_signal(Vec::<String>::new);
    let mut from = use_signal(String::new);
    let mut to = use_signal(String::new);
    let mut result = use_signal(|| Option::<Result<CustomResult, String>>::None);
    let mut running = use_signal(|| false);

    let schema_state = schema_resource.read_unchecked().clone();

    // Default the source to the first catalog entry once it loads.
    use_effect(move || {
        if let Some(Ok(list)) = schema_resource.read_unchecked().as_ref() {
            if source.peek().is_empty() {
                if let Some(first) = list.first() {
                    source.set(first.key.clone());
                }
            }
        }
    });

    let sources: Vec<SourceSchema> = match &schema_state {
        Some(Ok(list)) => list.clone(),
        _ => Vec::new(),
    };
    let load_err = matches!(&schema_state, Some(Err(_)));
    let loading = schema_state.is_none();
    let current = sources.iter().find(|s| s.key == source()).cloned();

    let run = move |_| {
        let src = source();
        let d = dims();
        let m = measures();
        if src.is_empty() || m.is_empty() {
            return;
        }
        let f = from();
        let t = to();
        running.set(true);
        spawn(async move {
            let spec = CustomSpec {
                source: src,
                dimensions: d,
                measures: m,
                from: (!f.is_empty()).then_some(f),
                to: (!t.is_empty()).then_some(t),
                limit: 500,
            };
            let res =
                crate::hooks::fetch::api::post_authed::<CustomResult, _>("/reports/custom", &spec)
                    .await;
            result.set(Some(res));
            running.set(false);
        });
    };

    let can_run = !measures().is_empty() && !running();

    rsx! {
        if load_err {
            Card {
                p { class: "text-sm text-red-600 dark:text-red-400",
                    "Could not load the report catalog. The reports service may be unavailable."
                }
            }
        } else if loading {
            crate::components::DetailSkeleton {} // PMS-353
        } else {
            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Builder controls
                Card { title: "Build",
                    div { class: "space-y-5",
                        // Source
                        div {
                            label { class: "block text-sm font-medium text-content mb-1",
                                "Data source"
                            }
                            select {
                                class: "w-full rounded-md border border-line bg-surface px-3 py-2 text-sm",
                                onchange: move |e| {
                                    source.set(e.value());
                                    dims.set(Vec::new());
                                    measures.set(Vec::new());
                                    result.set(None);
                                },
                                for s in sources.iter() {
                                    option {
                                        value: "{s.key}",
                                        selected: s.key == source(),
                                        "{s.label}"
                                    }
                                }
                            }
                        }

                        if let Some(cur) = current.as_ref() {
                            // Dimensions
                            div {
                                p { class: "text-sm font-medium text-content mb-2",
                                    "Group by"
                                }
                                div { class: "space-y-1",
                                    for d in cur.dimensions.iter() {
                                        {
                                            let key = d.key.clone();
                                            let checked = dims().contains(&key);
                                            rsx! {
                                                label { class: "flex items-center gap-2 text-sm text-content",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked,
                                                        onchange: move |_| {
                                                            let mut v = dims();
                                                            if v.contains(&key) { v.retain(|x| x != &key); }
                                                            else { v.push(key.clone()); }
                                                            dims.set(v);
                                                        },
                                                    }
                                                    "{d.label}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Measures
                            div {
                                p { class: "text-sm font-medium text-content mb-2",
                                    "Measures"
                                }
                                div { class: "space-y-1",
                                    for m in cur.measures.iter() {
                                        {
                                            let key = m.key.clone();
                                            let checked = measures().contains(&key);
                                            rsx! {
                                                label { class: "flex items-center gap-2 text-sm text-content",
                                                    input {
                                                        r#type: "checkbox",
                                                        checked,
                                                        onchange: move |_| {
                                                            let mut v = measures();
                                                            if v.contains(&key) { v.retain(|x| x != &key); }
                                                            else { v.push(key.clone()); }
                                                            measures.set(v);
                                                        },
                                                    }
                                                    "{m.label}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Optional date range
                            if cur.has_date_range {
                                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                                    // MAPPS-204: shared DateField so the report
                                    // range filters match every other date input.
                                    crate::components::DateField {
                                        name: "report-from",
                                        label: "From",
                                        value: "{from}",
                                        oninput: move |e: FormEvent| from.set(e.value()),
                                    }
                                    crate::components::DateField {
                                        name: "report-to",
                                        label: "To",
                                        value: "{to}",
                                        oninput: move |e: FormEvent| to.set(e.value()),
                                    }
                                }
                            }
                        }

                        Button {
                            variant: ButtonVariant::Primary,
                            class: "w-full".to_string(),
                            loading: running(),
                            disabled: !can_run,
                            title: (measures().is_empty())
                                .then(|| "Select at least one measure to run the report.".to_string()),
                            onclick: run,
                            "Run report"
                        }
                        p { class: "text-xs text-subtle",
                            "Pick a source and at least one measure. Grouping is optional."
                        }
                    }
                }

                // Results
                div { class: "lg:col-span-2",
                    Card { title: "Result",
                        match result() {
                            None => rsx! {
                                p { class: "text-sm text-subtle italic",
                                    "Configure the report on the left and run it to see results."
                                }
                            },
                            Some(Err(e)) => rsx! {
                                p { class: "text-sm text-red-600 dark:text-red-400", "Report failed: {e}" }
                            },
                            Some(Ok(r)) if r.rows.is_empty() => rsx! {
                                p { class: "text-sm text-subtle italic", "No rows for this selection." }
                            },
                            Some(Ok(r)) => {
                                let totals: Vec<(String, String)> =
                                    r.totals.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                                rsx! {
                                    Table {
                                        TableHead {
                                            TableRow {
                                                for col in r.columns.iter() {
                                                    TableHeader { "{col}" }
                                                }
                                            }
                                        }
                                        TableBody {
                                            for row in r.rows.iter() {
                                                TableRow {
                                                    for cell in row.iter() {
                                                        {
                                                            let v = cell.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "-".to_string());
                                                            rsx! {
                                                                TableCell { "{v}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if !totals.is_empty() {
                                        div { class: "mt-4 flex flex-wrap gap-4",
                                            for (k , v) in totals.iter() {
                                                div { class: "rounded-md bg-app px-3 py-2",
                                                    span { class: "text-xs text-muted", "Total {k}: " }
                                                    span { class: "text-sm font-semibold text-content", "{v}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod export_tests {
    use super::{export_key, report_kind};

    /// Every page that renders a server report can export that same report,
    /// and the one placeholder page cannot.
    #[test]
    fn every_supported_report_page_exports_the_server_report_it_reads() {
        assert_eq!(export_key(report_kind("ticket-volume")), Some("tickets"));
        assert_eq!(export_key(report_kind("billable-hours")), Some("time"));
        assert_eq!(export_key(report_kind("ar-aging")), Some("billing"));
        assert_eq!(export_key(report_kind("budget-tracking")), Some("projects"));
        assert_eq!(export_key(report_kind("client-summary")), Some("clients"));
        assert_eq!(export_key(report_kind("report-builder")), None);
    }
}

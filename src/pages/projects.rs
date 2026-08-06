//! Project pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, ErrorBanner,
    IconSize, Input, Modal, OverflowActions, PageHeader, PencilIcon, PlusIcon, SearchInput, Select,
    SelectOption, StatCard, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
    Textarea,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

/// A project (`GET /api/v1/projects`). Money/hours are decoded with a
/// number-or-string tolerant reader because the server's `Decimal` wire
/// form depends on rust_decimal's serde feature set.
#[derive(Clone, Debug, Deserialize)]
struct RemoteProject {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    project_manager_id: Option<uuid::Uuid>,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    target_end_date: Option<String>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    budget_hours: Option<f64>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    budget_amount: Option<f64>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    actual_hours: Option<f64>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    actual_amount: Option<f64>,
}

/// A company, used to resolve `company_id` to a name and to populate the
/// New Project picker.
#[derive(Clone, Debug, Deserialize)]
struct CompanyOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// A task (`GET /api/v1/projects/:id/tasks`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteTask {
    id: uuid::Uuid,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status_id: Option<uuid::Uuid>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    assigned_to_id: Option<uuid::Uuid>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    estimated_hours: Option<f64>,
    // Approved-only hours (PMS-51). MAPPS-167: `logged_hours` is all
    // non-rejected logged time; we show both so logged time is visible
    // before approval.
    #[serde(default, deserialize_with = "de_flex_f64")]
    actual_hours: Option<f64>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    logged_hours: Option<f64>,
    #[serde(default)]
    due_date: Option<String>,
}

/// One change-history entry (`GET /audit-log/entity/tasks/:id`, PMS-184).
/// `changes` carries the before/after values of the touched columns (PMS-204).
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
    timestamp: chrono::DateTime<chrono::Utc>,
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

/// A per-tenant task status (`GET /api/v1/task-statuses`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteTaskStatus {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_completed: bool,
}

/// A user, used to resolve `assigned_to_id` / `project_manager_id` to a name.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
}

/// Resolve a task status id to a (badge colour, label). Completed
/// statuses are green; everything else is blue (in-flight).
fn task_status_badge(
    statuses: &[RemoteTaskStatus],
    id: &Option<uuid::Uuid>,
) -> (BadgeVariant, String) {
    match id.and_then(|sid| statuses.iter().find(|s| s.id == sid)) {
        Some(s) if s.is_completed => (BadgeVariant::Green, s.name.clone()),
        Some(s) => (BadgeVariant::Blue, s.name.clone()),
        None => (BadgeVariant::Gray, "Unknown".to_string()),
    }
}

/// Resolve a user id to a display name; "Unassigned" when none.
fn user_name(users: &[RemoteUser], id: &Option<uuid::Uuid>) -> String {
    match id {
        Some(uid) => users
            .iter()
            .find(|u| &u.id == uid)
            .map(|u| u.full_name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        None => "Unassigned".to_string(),
    }
}

/// Resolve a history actor id to a display name; "" when unknown (the caller
/// then omits the "by ..." suffix rather than printing a UUID). PMS-184.
fn actor_name(users: &[RemoteUser], id: &Option<uuid::Uuid>) -> String {
    match id {
        Some(uid) => users
            .iter()
            .find(|u| &u.id == uid)
            .map(|u| u.full_name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_default(),
        None => String::new(),
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
/// (`project_manager_id`, `client_company_id`). The audit log records
/// the raw column name, so without trimming the suffix the
/// change-history feed reads "Project manager id" / "Client company id".
/// Strip the trailing `_id` first so future FK fields render cleanly
/// without a per-column allow-list.
fn title_field(f: &str) -> String {
    let trimmed = f.strip_suffix("_id").unwrap_or(f);
    let mut s = trimmed.replace('_', " ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    s
}

/// A 36-char hyphenated UUID, not worth showing as before/after text.
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
            } else if let Ok(d) = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d") {
                // PMS-317: show dates the way the rest of the app does
                // ("Mar 1, 2026"), not the raw yyyy-mm-dd the audit stores.
                d.format("%b %-d, %Y").to_string()
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

/// "Feb 28, 2025 15:04" for a history timestamp.
fn fmt_history_dt(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%b %-d, %Y %H:%M").to_string()
}

/// Validate an optional `yyyy-mm-dd` date field (PMS-317 / PMS-346). Blank is
/// allowed (`Ok`). A non-empty value must parse as a full calendar date within
/// a sane year range. The Due Date inputs are native `<input type="date">`,
/// which only ever emit a valid date or empty, so the real gap this guards is
/// an out-of-range year (e.g. `0007`); the inputs carry matching min/max so
/// the picker rejects it natively too. `label` names the field in the message.
fn validate_opt_date(raw: &str, label: &str) -> Result<(), String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(());
    }
    let d = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("{label} must be a valid date."))?;
    let min = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let max = chrono::NaiveDate::from_ymd_opt(2100, 12, 31).unwrap();
    if d < min || d > max {
        return Err(format!("{label} must be between 2000 and 2100."));
    }
    Ok(())
}

/// Insert a date field, sending `null` (leaves the column unchanged under the
/// server's COALESCE update) when the input is blank. PMS-184 edit forms.
fn insert_opt_date(body: &mut serde_json::Map<String, serde_json::Value>, key: &str, v: &str) {
    let v = v.trim();
    body.insert(
        key.to_string(),
        if v.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!(v)
        },
    );
}

/// Insert a numeric field as a JSON number, or `null` when blank/unparseable.
/// Max length for a project name. Mirrors the server's PMS-324 cap so the
/// client rejects over-long names inline instead of waiting for a 422.
const PROJECT_NAME_MAX: usize = 80;

/// Maximum project budget amount (MAPPS-212). Mirrors the server's
/// `DECIMAL(14, 2)` budget columns (12 integer digits) so an over-large amount
/// is rejected inline with a field message instead of overflowing opaquely
/// server-side. Assumed precision; revise if the server column differs.
const BUDGET_AMOUNT_MAX: f64 = 999_999_999_999.99;

/// Maximum project description length (MAPPS-212). Caps the textarea client-side
/// so over-long text is blocked inline instead of failing later server-side.
/// Assumed to match the server's project description column; revise if it differs.
const PROJECT_DESCRIPTION_MAX: usize = 2000;

/// Validate a project name (MAPPS-176): required, trimmed, at most
/// [`PROJECT_NAME_MAX`] characters. Returns the trimmed name or an inline
/// error message for the Name field.
fn validate_project_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("Project name is required.".to_string());
    }
    if name.chars().count() > PROJECT_NAME_MAX {
        return Err(format!(
            "Project name must be {PROJECT_NAME_MAX} characters or fewer."
        ));
    }
    Ok(name.to_string())
}

/// Validate an optional budget field (MAPPS-176). Blank -> `Ok(None)`.
/// Otherwise it must be a non-negative number with at most two decimal places
/// (matching the server's `DECIMAL(_, 2)` budget columns). Returns the parsed
/// value or an inline error message for that field. `label` names the field in
/// the message (e.g. "Budget amount").
fn validate_budget(raw: &str, label: &str) -> Result<Option<f64>, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let value: f64 = s
        .parse()
        .map_err(|_| format!("{label} must be a number."))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{label} must not be negative."));
    }
    if value > BUDGET_AMOUNT_MAX {
        return Err(format!("{label} must be at most {BUDGET_AMOUNT_MAX:.2}."));
    }
    if let Some((_, frac)) = s.split_once('.') {
        if frac.trim_end_matches('0').len() > 2 {
            return Err(format!("{label} must have at most 2 decimal places."));
        }
    }
    Ok(Some(value))
}

/// Validate the optional Budget Hours field (MAPPS-212). Blank -> `Ok(None)`.
/// Accepts decimal hours or `H:MM` (reusing the Log Time parser) and requires a
/// value greater than zero. Distinguishes a well-formed but out-of-range value
/// (e.g. `-9`, `0`) from genuinely non-numeric input, so a negative number is
/// no longer misreported as "must be a number".
fn validate_budget_hours(raw: &str) -> Result<Option<f64>, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    match crate::utils::duration::parse_input_to_hours(s) {
        Some(h) if h > 0.0 => Ok(Some(h)),
        // The parser accepts 0, but a budget of zero hours is out of range.
        Some(_) => Err("Budget hours must be greater than 0.".to_string()),
        // The parser rejects negatives by returning `None`; a value that parses
        // once its leading '-' is stripped is a negative number (out of range),
        // not malformed.
        None if is_negative_duration(s) => Err("Budget hours must be greater than 0.".to_string()),
        None => Err("Budget hours must be a number (2.5) or H:MM (1:30).".to_string()),
    }
}

/// True when `s` is a well-formed duration prefixed with a minus sign (e.g.
/// `-9`, `-2.5`, `-1:30`): a negative number rather than malformed input.
fn is_negative_duration(s: &str) -> bool {
    s.strip_prefix('-')
        .map(str::trim)
        .and_then(crate::utils::duration::parse_input_to_hours)
        .is_some()
}

/// Insert a UUID-bearing field as its string form, or `null` when blank.
fn insert_opt_uuid(body: &mut serde_json::Map<String, serde_json::Value>, key: &str, v: &str) {
    let v = v.trim();
    body.insert(
        key.to_string(),
        if v.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!(v)
        },
    );
}

/// One-decimal hours, or "-" when absent.
fn fmt_hours(v: Option<f64>) -> String {
    match v {
        Some(n) => format!("{n:.1}"),
        None => "-".to_string(),
    }
}

/// Deserialize an optional `Decimal`-ish field that may arrive as a JSON
/// number, a string, or null.
fn de_flex_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.parse().ok(),
        _ => None,
    })
}

/// (badge colour, label) for a project status.
fn status_badge(status: &str) -> (BadgeVariant, &'static str) {
    match status {
        "active" => (BadgeVariant::Green, "Active"),
        "on_hold" => (BadgeVariant::Yellow, "On Hold"),
        "completed" => (BadgeVariant::Blue, "Completed"),
        "cancelled" => (BadgeVariant::Gray, "Cancelled"),
        "planning" => (BadgeVariant::Gray, "Planning"),
        _ => (BadgeVariant::Gray, "Unknown"),
    }
}

// Money formatting is centralized in `crate::utils::money` (MAPPS-197).
use crate::utils::money::format_money_f64;

/// "Feb 28, 2025" from an ISO date string; raw string on parse failure,
/// "-" when absent.
fn fmt_date(s: &Option<String>) -> String {
    match s {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map(|nd| nd.format("%b %-d, %Y").to_string())
            .unwrap_or_else(|_| d.clone()),
        None => "-".to_string(),
    }
}

/// Project list page
#[component]
pub fn ProjectListPage() -> Element {
    use_page_title("Projects");
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("planning", "Planning"),
        SelectOption::new("active", "Active"),
        SelectOption::new("on_hold", "On Hold"),
        SelectOption::new("completed", "Completed"),
        SelectOption::new("cancelled", "Cancelled"),
    ];

    // MAPPS-249: scope to one company when a context card's "View All" passes
    // `?company_id=<uuid>`.
    // MAPPS-357: `/projects` is this page's PRIMARY resource; via
    // use_remote_resource a failed load while the server is down renders the
    // honest unavailable state below instead of an empty grid with zero stat
    // counts, and auto-refetches on reconnect. The query-param subscription
    // stays inside the fetcher.
    let projects_resource = crate::hooks::use_remote_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let mut path = String::from("/projects");
        if let Some(company_id) = crate::utils::url::current_query_param("company_id") {
            path.push_str(&format!("?company_id={company_id}"));
        }
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteProject>>(&path)
            .await
            .map(|p| p.data)
    });
    // Company names are a SECONDARY lookup: a missing list just renders
    // "Unknown company", so it keeps degrading to a default rather than
    // gating the whole page on an outage.
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    // MAPPS-357: primary resource unavailable (failed while the server is
    // flagged down) -> explicit outage body. Placed after every hook.
    if projects_resource.is_unavailable() {
        return rsx! {
            crate::components::ContentUnavailable { title: "Projects".to_string() }
        };
    }
    let is_loading = projects_resource.is_loading();
    let projects: Vec<RemoteProject> = projects_resource.value_or_default();
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let company_name = |id: &Option<uuid::Uuid>| -> String {
        match id {
            Some(cid) => companies
                .iter()
                .find(|c| &c.id == cid)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Unknown company".to_string()),
            None => "No company".to_string(),
        }
    };

    // Stat cards computed from the fetched projects (no hardcoded totals).
    let active = projects.iter().filter(|p| p.status == "active").count();
    let on_hold = projects.iter().filter(|p| p.status == "on_hold").count();
    let completed = projects.iter().filter(|p| p.status == "completed").count();
    let total_value: f64 = projects.iter().filter_map(|p| p.budget_amount).sum();
    // Normalize negative zero (and tiny negatives that round to zero) so the card
    // shows "$0" instead of "$-0".
    let total_value = if total_value.round() == 0.0 {
        0.0
    } else {
        total_value
    };
    // PMS-365: route through the shared formatter so the stat matches the
    // project card / budget panel ($1,234.00) instead of a bare $1234.
    let total_value_label = format_money_f64(Some(total_value));

    // Client-side search + status filter.
    let needle = search.read().to_lowercase();
    let sf = status_filter.read().clone();
    let filtered: Vec<&RemoteProject> = projects
        .iter()
        .filter(|p| sf.is_empty() || p.status == sf)
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .collect();

    rsx! {
        PageHeader {
            title: "Projects",
            subtitle: "Manage projects and track progress",
            actions: rsx! {
                Link {
                    to: Route::ProjectNew {},
                    Button {
                        variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Project"
                    }
                }
            },
        }

        // MAPPS-321: scope indicator.
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Projects,
        }

        // Stats
        div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-6",
            StatCard { label: "Active Projects", value: "{active}" }
            StatCard { label: "On Hold", value: "{on_hold}" }
            StatCard { label: "Completed", value: "{completed}" }
            StatCard { label: "Total Budget", value: "{total_value_label}" }
        }

        // MAPPS-388: de-boxed. Search + type controls sit directly on the
        // page; the surrounding Card was much larger than the controls it held.
        div { class: "mb-6",
            div { class: "flex flex-col sm:flex-row gap-4",
                div { class: "flex-1",
                    SearchInput {
                        value: search.read().clone(),
                        placeholder: "Search projects…",
                        oninput: move |e: FormEvent| search.set(e.value()),
                    }
                }
                Select {
                    name: "status",
                    options: status_options,
                    value: status_filter.read().clone(),
                    onchange: move |e: FormEvent| status_filter.set(e.value()),
                }
            }
        }

        if is_loading {
            // PMS-353: card-grid skeleton matching the populated layout,
            // instead of a bare "Loading projects…" line.
            crate::components::CardGridSkeleton {}
        } else if filtered.is_empty() {
            if projects.is_empty() {
                crate::components::EmptyState {
                    title: "No projects yet".to_string(),
                    description: "Create your first project to plan and track work.".to_string(),
                    actions: rsx! {
                        Link {
                            to: Route::ProjectNew {},
                            Button {
                                variant: ButtonVariant::Primary,
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Project"
                            }
                        }
                    },
                }
            } else {
                // MAPPS-291 "Clear filters" affordance on the projects list.
                crate::components::EmptyState {
                    title: "No projects match the current filters".to_string(),
                    description: "Adjust the filters above, or clear them to see every project again.".to_string(),
                    actions: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                search.set(String::new());
                                status_filter.set(String::new());
                            },
                            "Clear filters"
                        }
                    },
                }
            }
        } else {
            // Project cards
            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
                for p in filtered.iter() {
                    {
                        let (variant, label) = status_badge(&p.status);
                        let cname = company_name(&p.company_id);
                        let util = match (p.actual_amount, p.budget_amount) {
                            (Some(a), Some(b)) if b > 0.0 => {
                                Some(((a / b) * 100.0).clamp(0.0, 100.0).round() as u32)
                            }
                            _ => None,
                        };
                        let bar_color = match util {
                            Some(u) if u >= 90 => "bg-red-600",
                            Some(u) if u >= 75 => "bg-yellow-500",
                            Some(_) => "bg-green-600",
                            None => "bg-gray-400", // theme-guard-allow: neutral status-bar fill, sibling of red/yellow/green
                        };
                        let due = fmt_date(&p.target_end_date);
                        let budget = format_money_f64(p.budget_amount);
                        let pid = p.id.to_string();
                        rsx! {
                            Link {
                                key: "{pid}",
                                to: Route::ProjectDetail { id: pid.clone() },
                                Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                                    div { class: "flex items-start justify-between mb-4",
                                        div {
                                            h3 { class: "text-lg font-medium text-content",
                                                "{p.name}"
                                            }
                                            p { class: "text-sm text-muted",
                                                "{cname}"
                                            }
                                        }
                                        Badge { variant, "{label}" }
                                    }

                                    // Budget utilization (actual vs budget)
                                    div { class: "mb-4",
                                        div { class: "flex justify-between text-sm mb-1",
                                            span { class: "text-muted", "Budget used" }
                                            if let Some(u) = util {
                                                span { class: "font-medium text-content", "{u}%" }
                                            } else {
                                                span { class: "text-subtle", "n/a" }
                                            }
                                        }
                                        div { class: "w-full bg-surface-2 rounded-full h-2",
                                            div {
                                                class: "{bar_color} h-2 rounded-full transition-all",
                                                style: "width: {util.unwrap_or(0)}%",
                                            }
                                        }
                                    }

                                    // Footer info
                                    div { class: "flex justify-between text-sm",
                                        div {
                                            span { class: "text-muted", "Due: " }
                                            span { class: "text-content", "{due}" }
                                        }
                                        div {
                                            span { class: "text-muted", "Budget: " }
                                            span { class: "font-medium text-content", "{budget}" }
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

/// New project page
#[component]
pub fn ProjectNewPage() -> Element {
    use_page_title("New Project");
    let mut name = use_signal(String::new);
    // MAPPS-300: pre-fill `company` from the URL so the Company detail
    // "New Project" CTA lands on a form already scoped to that company.
    let mut company =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    // PMS-367: `company` holds the selected company UUID; CompanyPicker reports
    // the display name back here so the autocomplete renders the chosen company.
    let mut company_name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut budget_amount = use_signal(String::new);
    let mut budget_hours = use_signal(String::new);
    // PMS-361: New Project lacked the status / dates / manager fields the
    // Edit modal exposes, so every project was born as "planning" and
    // needed a follow-up edit. These signals make Create field-symmetric
    // with Edit; "planning" stays the default to preserve existing
    // behaviour for users who don't change the dropdown.
    let mut status = use_signal(|| "planning".to_string());
    let mut project_manager = use_signal(String::new);
    let mut start_date = use_signal(String::new);
    let mut target_end_date = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-176).
    let mut name_err = use_signal(String::new);
    let mut amount_err = use_signal(String::new);
    let mut hours_err = use_signal(String::new);

    // PMS-367: company is chosen via the shared autocomplete CompanyPicker
    // (same component as Ticket/Contact/Asset/Contract), which fetches and
    // filters its own list, so this form no longer builds a native Select.
    let company_picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company.read().as_str()).is_ok() {
            Some(company.read().clone())
        } else {
            None
        };

    // MAPPS-357: N/A because this is a blank create form with no PRIMARY
    // fetched entity to gate on. `users_resource` below is a SECONDARY
    // dropdown lookup (Project Manager options) that keeps degrading to a
    // default. There is no honest "content" to swap for ContentUnavailable;
    // instead the write control (Create Project) is disabled while the
    // server is unreachable.
    // PMS-361: users list for the Project Manager Select. Same endpoint
    // and shape the Edit modal uses on the detail page.
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteUser>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let mut manager_options = vec![SelectOption::new("", "Unassigned")];
    manager_options.extend(
        users
            .iter()
            .map(|u| SelectOption::new(u.id.to_string(), u.full_name.clone())),
    );

    // PMS-361: same status set the Edit modal offers, kept inline here
    // because there is no tenant-configurable project-status lookup.
    let status_options = vec![
        SelectOption::new("planning", "Planning"),
        SelectOption::new("active", "Active"),
        SelectOption::new("on_hold", "On Hold"),
        SelectOption::new("completed", "Completed"),
        SelectOption::new("cancelled", "Cancelled"),
    ];

    let err = error.read().clone();
    // MAPPS-357: block the create submit while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    rsx! {
        PageHeader {
            title: "New Project",
            subtitle: "Create a new project",
            // MAPPS-294: every create form gets a breadcrumb trail back
            // to its parent list so the user can bail out without
            // hitting the browser back button (which would otherwise
            // trigger the MAPPS-292 unsaved-changes guard on a dirty
            // form for no good reason).
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: vec![
                        crate::components::BreadcrumbItem {
                            label: "Projects".to_string(),
                            route: Some(Route::ProjectList {}),
                        },
                        crate::components::BreadcrumbItem {
                            label: "New Project".to_string(),
                            route: None,
                        },
                    ],
                }
            },
        }

        Card {
            form {
                class: "space-y-6",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    error.set(String::new());
                    name_err.set(String::new());
                    amount_err.set(String::new());
                    hours_err.set(String::new());
                    let company_id = company.read().clone();
                    let desc = description.read().clone();
                    // MAPPS-284: collect every client-validation failure on
                    // the same submit instead of early-returning on the
                    // first one. The user previously had to fix one error,
                    // resubmit, then discover the next; now the form
                    // reports every offending field at once. Per-field
                    // signals still drive the inline error rendering and
                    // the styled red border.
                    let project_name_res = validate_project_name(&name.read());
                    let amount_res = validate_budget(&budget_amount.read(), "Budget amount");
                    let hours_res = validate_budget_hours(&budget_hours.read());
                    let mut hit_blank = false;
                    if let Err(ref msg) = project_name_res {
                        name_err.set(msg.clone());
                        hit_blank = true;
                    }
                    if let Err(ref msg) = amount_res {
                        amount_err.set(msg.clone());
                        hit_blank = true;
                    }
                    if let Err(ref msg) = hours_res {
                        hours_err.set(msg.clone());
                        hit_blank = true;
                    }
                    if hit_blank {
                        // PMS-518: the per-field validators above already
                        // populated every inline slot; the shared guard only
                        // adds focus-first, landing on the first invalid field
                        // in field order (Name, then Budget Amount, then Hours).
                        let mut guard = FormGuard::new();
                        guard.note_invalid(Some(if project_name_res.is_err() {
                            "name"
                        } else if amount_res.is_err() {
                            "budget_amount"
                        } else {
                            "budget_hours"
                        }));
                        guard.blocked();
                        return;
                    }
                    let project_name = project_name_res.unwrap();
                    let amount = amount_res.unwrap();
                    let hours: Option<f64> = hours_res.unwrap();
                    is_submitting.set(true);
                    // PMS-361: snapshot the new fields so the spawn
                    // doesn't reach back into the signal layer. Status
                    // is always sent (defaulting to "planning" if the
                    // user somehow cleared the Select); the rest go
                    // in only when non-empty so the server doesn't see
                    // an empty-string masquerading as a UUID/date.
                    let status_v = {
                        let s = status.read().trim().to_string();
                        if s.is_empty() {
                            "planning".to_string()
                        } else {
                            s
                        }
                    };
                    let manager_v = project_manager.read().trim().to_string();
                    let start_v = start_date.read().trim().to_string();
                    let end_v = target_end_date.read().trim().to_string();
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            let mut body = serde_json::json!({
                                "name": project_name,
                                "description": desc,
                                "status": status_v,
                            });
                            if !company_id.is_empty() {
                                body["company_id"] = serde_json::json!(company_id);
                            }
                            if let Some(a) = amount {
                                body["budget_amount"] = serde_json::json!(a);
                            }
                            if let Some(h) = hours {
                                body["budget_hours"] = serde_json::json!(h);
                            }
                            if !manager_v.is_empty() {
                                body["project_manager_id"] =
                                    serde_json::json!(manager_v);
                            }
                            if !start_v.is_empty() {
                                body["start_date"] = serde_json::json!(start_v);
                            }
                            if !end_v.is_empty() {
                                body["target_end_date"] = serde_json::json!(end_v);
                            }
                            match crate::hooks::fetch::api::post_authed_typed::<
                                    serde_json::Value,
                                    _,
                                >("/projects", &body)
                                .await
                            {
                                Ok(_) => {
                                    dioxus::prelude::navigator().push(Route::ProjectList {});
                                }
                                Err(err) => {
                                    // MAPPS-265: route server-side field errors
                                    // onto their inline fields so the cue
                                    // persists after a failed submit; unmatched
                                    // fields or a non-422 failure fall back to
                                    // the top-of-form banner.
                                    let fields = err.field_errors();
                                    if fields.is_empty() {
                                        error
                                            .set(format!(
                                                "Could not create project: {}",
                                                err.user_message(),
                                            ));
                                    } else {
                                        let mut leftover = Vec::new();
                                        for fe in fields {
                                            match fe.field.as_str() {
                                                "name" => name_err.set(fe.message.clone()),
                                                "budget_amount" => {
                                                    amount_err.set(fe.message.clone())
                                                }
                                                "budget_hours" => {
                                                    hours_err.set(fe.message.clone())
                                                }
                                                _ => leftover.push(fe.message.clone()),
                                            }
                                        }
                                        if !leftover.is_empty() {
                                            error.set(leftover.join("; "));
                                        }
                                    }
                                }
                            }
                        }
                        is_submitting.set(false);
                    });
                },

                if !err.is_empty() {
                    ErrorBanner { "{err}" }
                }

                crate::components::Input {
                    name: "name",
                    label: "Project Name",
                    placeholder: "Enter project name",
                    required: true,
                    value: name.read().clone(),
                    error: name_err(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }

                // PMS-367: autocomplete CompanyPicker (with inline create),
                // matching the Ticket/Contact/Asset/Contract forms. Company
                // is optional on a project, so `required` is false.
                crate::components::CompanyPicker {
                    value: company_name.read().clone(),
                    selected_id: company_picker_selected_id,
                    required: false,
                    allow_inline_create: true,
                    onselect: move |(id, name): (String, String)| {
                        company.set(id);
                        company_name.set(name);
                    },
                    onclear: move |_| {
                        company.set(String::new());
                        company_name.set(String::new());
                    },
                }

                crate::components::Textarea {
                    name: "description",
                    label: "Description",
                    placeholder: "Project description…",
                    rows: 4,
                    maxlength: PROJECT_DESCRIPTION_MAX as i64,
                    value: description.read().clone(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }

                // PMS-361: status + project manager row, matching the
                // Edit modal's layout so the Create flow doesn't force
                // an immediate follow-up edit just to set these.
                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "status",
                        label: "Status",
                        options: status_options,
                        value: status.read().clone(),
                        onchange: move |e: FormEvent| status.set(e.value()),
                    }
                    Select {
                        name: "project_manager_id",
                        label: "Project Manager",
                        options: manager_options.clone(),
                        value: project_manager.read().clone(),
                        onchange: move |e: FormEvent| project_manager.set(e.value()),
                    }
                }

                // PMS-361: start + target end dates. Optional; blank
                // leaves them unset on the server.
                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "start_date",
                        label: "Start Date",
                        value: start_date.read().clone(),
                        oninput: move |e: FormEvent| start_date.set(e.value()),
                    }
                    crate::components::DateField {
                        name: "target_end_date",
                        label: "Target End Date",
                        value: target_end_date.read().clone(),
                        oninput: move |e: FormEvent| target_end_date.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "budget_amount",
                        label: "Budget Amount ($)",
                        r#type: "number",
                        min: "0".to_string(),
                        max: BUDGET_AMOUNT_MAX.to_string(),
                        placeholder: "0.00",
                        value: budget_amount.read().clone(),
                        error: amount_err(),
                        oninput: move |e: FormEvent| budget_amount.set(e.value()),
                    }
                    crate::components::Input {
                        name: "budget_hours",
                        label: "Budget Hours",
                        // Free-text so H:MM (e.g. "1:30") can be typed; a
                        // type="number" input blocks the colon. PMS-340.
                        r#type: "text",
                        placeholder: "2, 2.5, or 1:30",
                        help: "Decimal hours or H:MM.",
                        value: budget_hours.read().clone(),
                        error: hours_err(),
                        oninput: move |e: FormEvent| budget_hours.set(e.value()),
                    }
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: Route::ProjectList {},
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        // MAPPS-357: no writes while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create a project while the server is unreachable".to_string()),
                        "Create Project"
                    }
                }
            }
        }
    }
}

/// Project detail page
#[derive(Props, Clone, PartialEq)]
pub struct ProjectDetailPageProps {
    pub id: String,
}

#[component]
pub fn ProjectDetailPage(props: ProjectDetailPageProps) -> Element {
    let id_for_project = props.id.clone();
    // MAPPS-357: the fetched project is this detail page's PRIMARY resource
    // (tasks / statuses / users / history are secondary lookups that keep
    // degrading to defaults). Kept as a hand-rolled use_resource because the
    // edit / checklist flows call `.restart()`; `.ok()` preserves a failed
    // load so an outage renders ContentUnavailable instead of a blank detail.
    // Subscribe to reachability so it auto-refetches on reconnect.
    let project_resource = use_resource(move || {
        let id = id_for_project.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<RemoteProject>(&format!("/projects/{id}"))
                .await
                .ok()
        }
    });
    let id_for_tasks = props.id.clone();
    let mut tasks_resource = use_resource(move || {
        let id = id_for_tasks.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteTask>>(&format!(
                "/projects/{id}/tasks"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let statuses_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTaskStatus>>("/task-statuses")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteUser>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    // PMS-184 project-edit modal state.
    let mut show_proj_modal = use_signal(|| false);
    let mut pe_name = use_signal(String::new);
    let mut pe_description = use_signal(String::new);
    let mut pe_status = use_signal(String::new);
    let mut pe_start = use_signal(String::new);
    let mut pe_due = use_signal(String::new);
    let mut pe_budget_amount = use_signal(String::new);
    let mut pe_budget_hours = use_signal(String::new);
    let mut pe_manager = use_signal(String::new);
    let mut pe_submitting = use_signal(|| false);
    let mut pe_error = use_signal(String::new);
    // Per-field inline validation errors for the edit modal (MAPPS-176).
    let mut pe_name_err = use_signal(String::new);
    let mut pe_amount_err = use_signal(String::new);
    let mut pe_hours_err = use_signal(String::new);

    // PMS-184 task-edit modal state. `selected_task` is `Some` while the
    // modal is open for that task; the form and per-task history live in the
    // shared `TaskEditModal` component (MAPPS-165).
    let mut selected_task = use_signal(|| None::<RemoteTask>);
    // PMS-205: the project's own change history (who/when + before/after).
    let id_for_proj_history = props.id.clone();
    let project_history_resource = use_resource(move || {
        let id = id_for_proj_history.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<HistoryEntry>>(&format!(
                "/audit-log/entity/projects/{id}"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });

    let snapshot = project_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    // MAPPS-357: `Some(None)` means the project fetch failed (distinct from
    // `None` still-loading). Used for the outage early return after all hooks.
    let fetch_failed = matches!(&snapshot, Some(None));
    let project = snapshot.flatten();
    // MAPPS-245: a cancelled project does not accept new items. Gate the
    // add-task control once the project has loaded, using the same literal
    // the status badge/filter/edit selects use ("cancelled").
    let is_cancelled = project
        .as_ref()
        .map(|p| p.status == "cancelled")
        .unwrap_or(false);
    let tasks = tasks_resource.read_unchecked().clone().unwrap_or_default();
    let statuses = statuses_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let project_history = project_history_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    // "Edited" marker for the project Overview: most recent recorded edit.
    let project_edited = project_history
        .iter()
        .find(|e| e.action == "update")
        .map(|e| {
            let who = actor_name(&users, &e.user_id);
            let when = fmt_history_dt(e.timestamp);
            if who.is_empty() {
                format!("Edited {when}")
            } else {
                format!("Edited {when} by {who}")
            }
        });
    let proj_status_options = vec![
        SelectOption::new("planning", "Planning"),
        SelectOption::new("active", "Active"),
        SelectOption::new("on_hold", "On Hold"),
        SelectOption::new("completed", "Completed"),
        SelectOption::new("cancelled", "Cancelled"),
    ];

    // Add Task modal state.
    let mut show_task_modal = use_signal(|| false);
    let mut t_title = use_signal(String::new);
    let mut t_status = use_signal(String::new);
    let mut t_priority = use_signal(|| "medium".to_string());
    let mut t_estimated = use_signal(String::new);
    let mut t_due = use_signal(String::new);
    let mut t_assignee = use_signal(String::new);
    let mut t_submitting = use_signal(|| false);
    let mut t_error = use_signal(String::new);
    // Per-field inline validation slots for the Add Task modal (PMS-518).
    let mut t_title_err = use_signal(String::new);
    let mut t_status_err = use_signal(String::new);

    let mut status_options = vec![SelectOption::new("", "Select a status")];
    status_options.extend(
        statuses
            .iter()
            .map(|s| SelectOption::new(s.id.to_string(), s.name.clone())),
    );
    let priority_options = vec![
        SelectOption::new("low", "Low"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("high", "High"),
        SelectOption::new("critical", "Critical"),
    ];
    let mut assignee_options = vec![SelectOption::new("", "Unassigned")];
    assignee_options.extend(
        users
            .iter()
            .map(|u| SelectOption::new(u.id.to_string(), u.full_name.clone())),
    );
    let t_err = t_error.read().clone();
    let id_for_create = props.id.clone();

    // MAPPS-158: detail-page Delete, wired to the existing
    // `DELETE /projects/{id}` endpoint (parity with Company/Contract).
    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let id_for_delete = props.id.clone();

    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // actual DELETE fires from `on_confirm_delete` when confirmed.
    let mut confirming_delete = use_signal(|| false);

    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = id_for_delete.clone();
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/projects/{id}");
                if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                    navigator.push(Route::ProjectList {});
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let header_title = match project.as_ref() {
        Some(p) if !p.name.trim().is_empty() => p.name.clone(),
        Some(_) => format!("Project {}", props.id),
        None if is_loading => "Loading…".to_string(),
        None => "Project".to_string(),
    };
    use_page_title(header_title.clone());

    // MAPPS-357: primary resource failed while the server is flagged down ->
    // honest outage body (keeps nav + banner) instead of the "could not load"
    // card. Placed after every hook so the hook order stays fixed. A failure
    // while still reachable (a 4xx) keeps the inline card below. `can_mutate`
    // disables the Add Task / Edit / Delete / save controls while down.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: header_title.clone() }
        };
    }

    // Progress from task completion (per-tenant statuses flag the "done"
    // columns via is_completed).
    let total_tasks = tasks.len();
    let completed_tasks = tasks
        .iter()
        .filter(|t| {
            t.status_id
                .and_then(|sid| statuses.iter().find(|s| s.id == sid))
                .map(|s| s.is_completed)
                .unwrap_or(false)
        })
        .count();
    let progress = if total_tasks > 0 {
        (completed_tasks * 100 / total_tasks) as u32
    } else {
        0
    };

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete project".to_string(),
            message: "Delete this project? This cannot be undone.".to_string(),
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
        PageHeader {
            title: "{header_title}",
            actions: rsx! {
              // MAPPS-267: collapse the Edit/Delete/Add cluster into a
              // `...` menu on narrow screens instead of letting four
              // buttons overflow the header row.
              OverflowActions {
                Link {
                    to: Route::ProjectTasks { id: props.id.clone() },
                    Button { variant: ButtonVariant::Secondary, "View Tasks" }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-245: disable on a cancelled project so the Add
                    // Task modal cannot be opened; the native tooltip and the
                    // banner below explain why (MAPPS-217 pattern).
                    // MAPPS-357: also disable while the server is unreachable.
                    disabled: is_cancelled || !can_mutate,
                    title: if is_cancelled {
                        Some("This project is cancelled and does not accept new tasks.".to_string())
                    } else if !can_mutate {
                        Some("Can't add a task while the server is unreachable".to_string())
                    } else {
                        None
                    },
                    onclick: move |_| {
                        if is_cancelled {
                            return;
                        }
                        t_error.set(String::new());
                        show_task_modal.set(true);
                    },
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "Add Task"
                }
                if let Some(p) = project.clone() {
                    Button {
                        variant: ButtonVariant::Secondary,
                        // MAPPS-357: no edits while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                        onclick: move |_| {
                            pe_name.set(p.name.clone());
                            pe_description.set(p.description.clone().unwrap_or_default());
                            pe_status.set(p.status.clone());
                            pe_start.set(p.start_date.clone().unwrap_or_default());
                            pe_due.set(p.target_end_date.clone().unwrap_or_default());
                            pe_budget_amount
                                .set(p.budget_amount.map(|v| v.to_string()).unwrap_or_default());
                            pe_budget_hours.set(
                                p.budget_hours
                                    .map(crate::utils::duration::fmt_input_hours)
                                    .unwrap_or_default(),
                            );
                            pe_manager.set(
                                p.project_manager_id.map(|v| v.to_string()).unwrap_or_default(),
                            );
                            pe_error.set(String::new());
                            show_proj_modal.set(true);
                        },
                        PencilIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Edit"
                    }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    loading: *deleting.read(),
                    // MAPPS-357: no deletes while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        if !*deleting.read() {
                            confirming_delete.set(true);
                        }
                    },
                    "Delete"
                }
              }
            },
        }

        if is_loading {
            crate::components::DetailSkeleton {} // PMS-353
        } else if project.is_none() {
            Card {
                p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                    "Could not load this project."
                }
            }
        } else {
            {
                let p = project.clone().unwrap();
                let (status_variant, status_label) = status_badge(&p.status);
                let pm = user_name(&users, &p.project_manager_id);
                let spent = p.actual_amount.unwrap_or(0.0);
                let budget = p.budget_amount.unwrap_or(0.0);
                let remaining = budget - spent;
                let util = if budget > 0.0 {
                    ((spent / budget) * 100.0).clamp(0.0, 100.0).round() as u32
                } else {
                    0
                };
                let logged_h = p.actual_hours.unwrap_or(0.0);
                let remaining_h = p.budget_hours.unwrap_or(0.0) - logged_h;
                let description = p.description.clone().filter(|d| !d.trim().is_empty());
                rsx! {
                    div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                        // Main content
                        div { class: "lg:col-span-2 space-y-6",
                            Card {
                                title: "Overview",
                                // MAPPS-245: explain why the Add Task control is
                                // disabled so the state reads as intentional.
                                if is_cancelled {
                                    div { class: "mb-4 px-3 py-2 rounded-md text-sm bg-surface-2 text-muted",
                                        "This project is cancelled. Adding tasks is disabled."
                                    }
                                }
                                if let Some(d) = description {
                                    // PMS-309: render Markdown (sanitized). PMS-348:
                                    // task-list checkboxes are clickable - toggling
                                    // flips the source marker and persists.
                                    {
                                        let d_src = d.clone();
                                        let pid = props.id.clone();
                                        rsx! {
                                            crate::components::Markdown {
                                                content: d,
                                                // MAPPS-357: toggling a checklist item persists a
                                                // PUT, so make the checkboxes non-interactive while
                                                // the server is unreachable (no silent failed write).
                                                interactive: can_mutate,
                                                on_toggle: move |i: usize| {
                                                    let Some(new_desc) =
                                                        crate::utils::markdown::toggle_task(&d_src, i)
                                                    else {
                                                        return;
                                                    };
                                                    let pid = pid.clone();
                                                    let mut pr = project_resource;
                                                    let mut phr = project_history_resource;
                                                    spawn(async move {
                                                        let body = serde_json::json!({ "description": new_desc });
                                                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                                                                &format!("/projects/{pid}"),
                                                                &body,
                                                            )
                                                            .await
                                                        {
                                                            Ok(_) => {
                                                                pr.restart();
                                                                phr.restart();
                                                            }
                                                            Err(e) => {
                                                                crate::hooks::push_toast(
                                                                    crate::components::AlertType::Error,
                                                                    format!("Could not update checklist: {e}"),
                                                                );
                                                                pr.restart();
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
                                if let Some(m) = project_edited.clone() {
                                    p { class: "text-xs text-subtle italic mt-3", "{m}" }
                                }
                            }
                            Card { title: "Tasks",
                                if tasks.is_empty() {
                                    p { class: "text-sm text-subtle italic", "No tasks yet." }
                                } else {
                                    div { class: "space-y-3",
                                        for t in tasks.iter() {
                                            {
                                                let (tv, tl) = task_status_badge(&statuses, &t.status_id);
                                                let who = user_name(&users, &t.assigned_to_id);
                                                // MAPPS-205: surface logged vs approved vs
                                                // estimated hours on each task here in the
                                                // project view, mirroring the task overview, so
                                                // logged time is reflected on the task without
                                                // opening the edit modal. logged = all
                                                // non-rejected time (PMS-329); approved = the
                                                // approval-gated total (PMS-51); est = estimate.
                                                let logged_h = fmt_hours(t.logged_hours);
                                                let approved_h = fmt_hours(t.actual_hours);
                                                let est_h = fmt_hours(t.estimated_hours);
                                                // Clicking a row opens the task in the edit modal.
                                                let task = t.clone();
                                                let open_task = move |_| selected_task.set(Some(task.clone()));
                                                rsx! {
                                                    div {
                                                        class: "flex items-center justify-between p-3 bg-surface rounded-lg cursor-pointer hover:bg-surface-2 transition-colors",
                                                        onclick: open_task,
                                                        div {
                                                            p { class: "font-medium text-content", "{t.title}" }
                                                            p { class: "text-sm text-muted", "{who}" }
                                                        }
                                                        div { class: "flex items-center gap-4",
                                                            div { class: "text-right",
                                                                div { class: "text-sm font-medium text-content whitespace-nowrap",
                                                                    "Logged {logged_h} h"
                                                                }
                                                                div { class: "text-xs text-muted whitespace-nowrap",
                                                                    "Approved {approved_h} h · Est {est_h} h"
                                                                }
                                                            }
                                                            Badge { variant: tv, "{tl}" }
                                                        }
                                                    }
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
                                dl { class: "space-y-4",
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Status" }
                                        dd { Badge { variant: status_variant, "{status_label}" } }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Progress" }
                                        dd { class: "text-sm font-medium",
                                            "{progress}% ({completed_tasks}/{total_tasks} tasks)"
                                        }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Start Date" }
                                        dd { class: "text-sm", "{fmt_date(&p.start_date)}" }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Due Date" }
                                        dd { class: "text-sm", "{fmt_date(&p.target_end_date)}" }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Project Manager" }
                                        dd { class: "text-sm", "{pm}" }
                                    }
                                }
                            }
                            Card { title: "Budget",
                                div { class: "space-y-3",
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Total Budget" }
                                        span { class: "font-medium", "{format_money_f64(p.budget_amount)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Spent" }
                                        span { class: "font-medium text-green-600", "{format_money_f64(p.actual_amount)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Remaining" }
                                        span { class: "font-medium", "{format_money_f64(Some(remaining))}" }
                                    }
                                    div { class: "w-full bg-surface-2 rounded-full h-2 mt-2",
                                        div { class: "bg-green-600 h-2 rounded-full", style: "width: {util}%" }
                                    }
                                }
                            }
                            Card { title: "Time",
                                div { class: "space-y-3",
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Estimated" }
                                        span { class: "font-medium", "{fmt_hours(p.budget_hours)} h" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Logged" }
                                        span { class: "font-medium", "{fmt_hours(p.actual_hours)} h" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Remaining" }
                                        span { class: "font-medium", "{remaining_h:.1} h" }
                                    }
                                }
                            }
                            // PMS-205: the project's own change history.
                            Card { title: "Change History",
                                if project_history.is_empty() {
                                    p { class: "text-sm text-subtle italic", "No edits yet." }
                                } else {
                                    div { class: "space-y-3 text-sm",
                                        for e in project_history.iter().take(20) {
                                            {
                                                let label = action_label(&e.action);
                                                let fields = fields_label(&e.changed_fields);
                                                let who = actor_name(&users, &e.user_id);
                                                let when = fmt_history_dt(e.timestamp);
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
                                                            if !who.is_empty() {
                                                                p { class: "text-xs text-subtle", "by {who}" }
                                                            }
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
                }
            }
        }

        Modal {
            open: *show_task_modal.read(),
            title: "Add Task",
            size: crate::components::ModalSize::Medium,
            onclose: move |_| show_task_modal.set(false),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| show_task_modal.set(false),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *t_submitting.read(),
                    // MAPPS-357: no task creation while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't create a task while the server is unreachable".to_string()),
                    onclick: move |_| {
                        t_error.set(String::new());
                        // PMS-518: validate Title + Status through the shared
                        // guard so both surface at once in their own inline
                        // slots and the first invalid is focused, instead of
                        // the old single top-of-modal banner that showed only
                        // the first failure.
                        let mut guard = FormGuard::new();
                        let title = t_title.read().trim().to_string();
                        t_title_err
                            .set(guard.field("task_title", &title, "Title", &[Rule::Required]));
                        let status_id = t_status.read().clone();
                        t_status_err.set(guard.field(
                            "task_status",
                            &status_id,
                            "Status",
                            &[Rule::Required],
                        ));
                        if guard.blocked() {
                            return;
                        }
                        let priority = t_priority.read().clone();
                        let est_raw = t_estimated.read().trim().to_string();
                        let due = t_due.read().clone();
                        let assignee = t_assignee.read().clone();
                        // Reject a partial/invalid due date (PMS-317).
                        if let Err(e) = validate_opt_date(&due, "Due date") {
                            t_error.set(e);
                            return;
                        }
                        // Accept decimal hours or H:MM (PMS-319), reusing
                        // the Log Time parser; estimated_hours is stored as
                        // fractional hours, so parse straight to hours.
                        let est: Option<f64> = if est_raw.is_empty() {
                            None
                        } else {
                            match crate::utils::duration::parse_input_to_hours(&est_raw) {
                                Some(h) => Some(h),
                                None => {
                                    t_error.set(
                                        "Estimated hours must be a number (2.5) or H:MM (1:30)."
                                            .to_string(),
                                    );
                                    return;
                                }
                            }
                        };
                        let id = id_for_create.clone();
                        t_submitting.set(true);
                        let mut tr = tasks_resource;
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let mut body = serde_json::json!({
                                    "title": title,
                                    "status_id": status_id,
                                    "priority": priority,
                                });
                                if let Some(e) = est {
                                    body["estimated_hours"] = serde_json::json!(e);
                                }
                                if !due.is_empty() {
                                    body["due_date"] = serde_json::json!(due);
                                }
                                if !assignee.is_empty() {
                                    body["assigned_to_id"] = serde_json::json!(assignee);
                                }
                                let path = format!("/projects/{id}/tasks");
                                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                        &path,
                                        &body,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        t_title.set(String::new());
                                        t_estimated.set(String::new());
                                        t_due.set(String::new());
                                        t_assignee.set(String::new());
                                        show_task_modal.set(false);
                                        tr.restart();
                                    }
                                    Err(e) => {
                                        t_error.set(format!("Could not create task: {e}"));
                                    }
                                }
                            }
                            t_submitting.set(false);
                        });
                    },
                    "Create Task"
                }
            },
            div { class: "space-y-4",
                if !t_err.is_empty() {
                    ErrorBanner { "{t_err}" }
                }
                crate::components::Input {
                    name: "task_title",
                    label: "Title",
                    placeholder: "Task title",
                    required: true,
                    rules: vec![Rule::Required],
                    error: t_title_err.read().clone(),
                    value: t_title.read().clone(),
                    oninput: move |e: FormEvent| {
                        t_title_err.set(String::new());
                        t_title.set(e.value());
                    },
                }
                Select {
                    name: "task_status",
                    label: "Status",
                    required: true,
                    options: status_options,
                    rules: vec![Rule::Required],
                    error: t_status_err.read().clone(),
                    value: t_status.read().clone(),
                    onchange: move |e: FormEvent| {
                        t_status_err.set(String::new());
                        t_status.set(e.value());
                    },
                }
                Select {
                    name: "task_priority",
                    label: "Priority",
                    options: priority_options,
                    value: t_priority.read().clone(),
                    onchange: move |e: FormEvent| t_priority.set(e.value()),
                }
                Select {
                    name: "task_assignee",
                    label: "Assignee",
                    options: assignee_options,
                    value: t_assignee.read().clone(),
                    onchange: move |e: FormEvent| t_assignee.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "task_est",
                        label: "Estimated Hours",
                        // Free-text so H:MM (e.g. "1:30") can be typed; a
                        // type="number" input blocks the colon. PMS-319.
                        r#type: "text",
                        placeholder: "2, 2.5, or 1:30",
                        help: "Decimal hours or H:MM.",
                        value: t_estimated.read().clone(),
                        oninput: move |e: FormEvent| t_estimated.set(e.value()),
                    }
                    crate::components::DateField {
                        name: "task_due",
                        label: "Due Date",
                        value: t_due.read().clone(),
                        oninput: move |e: FormEvent| t_due.set(e.value()),
                    }
                }
            }
        }

        // PMS-184 project-edit modal.
        {
            let mut proj_res = project_resource;
            let mut proj_hist_res = project_history_resource;
            let save_id = props.id.clone();
            let on_save = move |_| {
                if pe_submitting() {
                    return;
                }
                pe_error.set(String::new());
                pe_name_err.set(String::new());
                pe_amount_err.set(String::new());
                pe_hours_err.set(String::new());
                // Per-field client validation mirrors the server rules (MAPPS-176).
                // PMS-518: accumulate every failure (each into its own inline
                // slot) instead of returning on the first, then add focus-first
                // via the shared guard. The bespoke validators stay - they
                // parse-and-return the values the request body needs.
                let name_res = validate_project_name(&pe_name());
                let amount_res = validate_budget(&pe_budget_amount(), "Budget amount");
                // Budget hours: accept decimal or H:MM (PMS-340). Blank
                // leaves it unset; out-of-range or malformed values error
                // inline with distinct messages (MAPPS-212).
                let hours_res = validate_budget_hours(&pe_budget_hours());
                let mut hit = false;
                if let Err(ref msg) = name_res {
                    pe_name_err.set(msg.clone());
                    hit = true;
                }
                if let Err(ref msg) = amount_res {
                    pe_amount_err.set(msg.clone());
                    hit = true;
                }
                if let Err(ref msg) = hours_res {
                    pe_hours_err.set(msg.clone());
                    hit = true;
                }
                if hit {
                    let mut guard = FormGuard::new();
                    guard.note_invalid(Some(if name_res.is_err() {
                        "pe-name"
                    } else if amount_res.is_err() {
                        "pe-budget-amount"
                    } else {
                        "pe-budget-hours"
                    }));
                    guard.blocked();
                    return;
                }
                let project_name = name_res.unwrap();
                let amount = amount_res.unwrap();
                let hours: Option<f64> = hours_res.unwrap();
                let save_id = save_id.clone();
                spawn(async move {
                    pe_submitting.set(true);
                    let mut body = serde_json::Map::new();
                    body.insert("name".into(), serde_json::json!(project_name));
                    body.insert("description".into(), serde_json::json!(pe_description()));
                    body.insert("status".into(), serde_json::json!(pe_status()));
                    insert_opt_date(&mut body, "start_date", &pe_start());
                    insert_opt_date(&mut body, "target_end_date", &pe_due());
                    body.insert(
                        "budget_amount".into(),
                        amount.map_or(serde_json::Value::Null, |a| serde_json::json!(a)),
                    );
                    body.insert(
                        "budget_hours".into(),
                        hours.map_or(serde_json::Value::Null, |h| serde_json::json!(h)),
                    );
                    insert_opt_uuid(&mut body, "project_manager_id", &pe_manager());
                    let body = serde_json::Value::Object(body);
                    match crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                        &format!("/projects/{save_id}"),
                        &body,
                    )
                    .await
                    {
                        Ok(_) => {
                            pe_submitting.set(false);
                            show_proj_modal.set(false);
                            proj_res.restart();
                            proj_hist_res.restart();
                        }
                        Err(err) => {
                            pe_submitting.set(false);
                            // MAPPS-265: route server-side field errors onto
                            // their inline fields so the cue persists after a
                            // failed submit; unmatched fields or a non-422
                            // failure fall back to the modal's banner.
                            let fields = err.field_errors();
                            if fields.is_empty() {
                                pe_error.set(err.user_message());
                            } else {
                                let mut leftover = Vec::new();
                                for fe in fields {
                                    match fe.field.as_str() {
                                        "name" => pe_name_err.set(fe.message.clone()),
                                        "budget_amount" => {
                                            pe_amount_err.set(fe.message.clone())
                                        }
                                        "budget_hours" => {
                                            pe_hours_err.set(fe.message.clone())
                                        }
                                        _ => leftover.push(fe.message.clone()),
                                    }
                                }
                                if !leftover.is_empty() {
                                    pe_error.set(leftover.join("; "));
                                }
                            }
                        }
                    }
                });
            };
            let mut pm_options = vec![SelectOption::new("", "Unassigned")];
            pm_options.extend(
                users
                    .iter()
                    .map(|u| SelectOption::new(u.id.to_string(), u.full_name.clone())),
            );
            rsx! {
                Modal {
                    open: show_proj_modal(),
                    title: "Edit Project",
                    size: crate::components::ModalSize::Large,
                    onclose: move |_| show_proj_modal.set(false),
                    footer: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| show_proj_modal.set(false),
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: pe_submitting(),
                            // MAPPS-357: no save while the server is down.
                            disabled: !can_mutate,
                            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                            onclick: on_save,
                            "Save Changes"
                        }
                    },
                    div { class: "space-y-4",
                        if !pe_error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{pe_error}" }
                        }
                        Input {
                            name: "pe-name",
                            label: "Name",
                            required: true,
                            value: "{pe_name}",
                            error: pe_name_err(),
                            oninput: move |e: FormEvent| pe_name.set(e.value()),
                        }
                        Textarea {
                            name: "pe-description",
                            label: "Description",
                            rows: 4,
                            maxlength: PROJECT_DESCRIPTION_MAX as i64,
                            value: "{pe_description}",
                            oninput: move |e: FormEvent| pe_description.set(e.value()),
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Select {
                                name: "pe-status",
                                label: "Status",
                                options: proj_status_options.clone(),
                                value: "{pe_status}",
                                onchange: move |e: FormEvent| pe_status.set(e.value()),
                            }
                            Select {
                                name: "pe-manager",
                                label: "Project Manager",
                                options: pm_options.clone(),
                                value: "{pe_manager}",
                                onchange: move |e: FormEvent| pe_manager.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            crate::components::DateField {
                                name: "pe-start",
                                label: "Start Date",
                                value: "{pe_start}",
                                oninput: move |e: FormEvent| pe_start.set(e.value()),
                            }
                            crate::components::DateField {
                                name: "pe-due",
                                label: "Target End Date",
                                value: "{pe_due}",
                                oninput: move |e: FormEvent| pe_due.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Input {
                                name: "pe-budget-amount",
                                label: "Budget Amount",
                                r#type: "number",
                                min: "0".to_string(),
                                max: BUDGET_AMOUNT_MAX.to_string(),
                                value: "{pe_budget_amount}",
                                error: pe_amount_err(),
                                oninput: move |e: FormEvent| pe_budget_amount.set(e.value()),
                            }
                            Input {
                                name: "pe-budget-hours",
                                label: "Budget Hours",
                                // Free-text for H:MM input (PMS-340).
                                r#type: "text",
                                placeholder: "2, 2.5, or 1:30",
                                help: "Decimal hours or H:MM.",
                                value: "{pe_budget_hours}",
                                error: pe_hours_err(),
                                oninput: move |e: FormEvent| pe_budget_hours.set(e.value()),
                            }
                        }
                    }
                }
            }
        }

        // Task edit modal (MAPPS-165): the shared TaskEditModal is mounted
        // only while a task is selected; it owns the form + change history.
        if let Some(task) = selected_task() {
            TaskEditModal {
                task,
                statuses: statuses.clone(),
                users: users.clone(),
                onclose: move |_| selected_task.set(None),
                onsaved: move |_| {
                    selected_task.set(None);
                    tasks_resource.restart();
                },
                ondeleted: move |_| {
                    selected_task.set(None);
                    tasks_resource.restart();
                },
            }
        }
    }
}

/// Project tasks page
#[derive(Props, Clone, PartialEq)]
pub struct ProjectTasksPageProps {
    pub id: String,
}

#[component]
pub fn ProjectTasksPage(props: ProjectTasksPageProps) -> Element {
    use_page_title("Project Tasks");
    let id_for_tasks = props.id.clone();
    // MAPPS-357: the task list is this page's PRIMARY resource (statuses /
    // users are secondary lookups that keep degrading to defaults). Kept as a
    // hand-rolled use_resource because TaskEditModal's onsaved/ondeleted call
    // `.restart()`; `.ok()` preserves a failed load so an outage renders
    // ContentUnavailable instead of an empty task table. Subscribe to
    // reachability so it auto-refetches on reconnect.
    let mut tasks_resource = use_resource(move || {
        let id = id_for_tasks.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteTask>>(&format!(
                "/projects/{id}/tasks"
            ))
            .await
            .ok()
            .map(|p| p.data)
        }
    });
    let statuses_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTaskStatus>>("/task-statuses")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteUser>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let snapshot = tasks_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let tasks: Vec<RemoteTask> = snapshot.flatten().unwrap_or_default();
    let statuses = statuses_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let total = tasks.len();

    // MAPPS-165: click-to-edit a task via the shared modal.
    let mut selected_task = use_signal(|| None::<RemoteTask>);

    // MAPPS-357: a failed task load while the server is flagged down is an
    // outage, not an empty project - render the honest unavailable state
    // (which keeps nav + banner) instead of an empty table. A failure while
    // still reachable (a 4xx) keeps the inline "Could not load tasks" row
    // below. Placed after every hook so the hook order stays fixed. Task
    // edits are gated inside TaskEditModal via its own `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    if load_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Project Tasks".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Project Tasks",
            subtitle: "Tasks for this project",
            actions: rsx! {
                Link {
                    to: Route::ProjectDetail { id: props.id.clone() },
                    Button { variant: ButtonVariant::Secondary, "Back to Project" }
                }
            },
        }

        DataTable {
            total_items: total,
            current_page: 1,
            per_page: if total == 0 { 25 } else { total },
            columns: 5,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Task" }
                        TableHeader { "Status" }
                        TableHeader { "Assigned To" }
                        TableHeader { "Due Date" }
                        TableHeader { "Hours" }
                    }
                }
                TableBody {
                    if is_loading {
                        TableRow { TableCell { class: "text-subtle", "Loading…" } }
                    } else if load_failed {
                        TableRow {
                            TableCell { class: "text-yellow-600 dark:text-yellow-400",
                                "Could not load tasks."
                            }
                        }
                    } else if tasks.is_empty() {
                        TableRow {
                            TableCell { class: "text-subtle italic", "No tasks yet." }
                        }
                    } else {
                        for t in tasks.iter() {
                            {
                                let (tv, tl) = task_status_badge(&statuses, &t.status_id);
                                let who = user_name(&users, &t.assigned_to_id);
                                let due = fmt_date(&t.due_date);
                                // Logged = all non-rejected time (PMS-329),
                                // visible before approval; approved = the
                                // approval-gated total; est = the estimate.
                                let logged_h = fmt_hours(t.logged_hours);
                                let approved_h = fmt_hours(t.actual_hours);
                                let est_h = fmt_hours(t.estimated_hours);
                                let unassigned = t.assigned_to_id.is_none();
                                let task = t.clone();
                                rsx! {
                                    TableRow {
                                        clickable: true,
                                        onclick: move |_| selected_task.set(Some(task.clone())),
                                        TableCell { "{t.title}" }
                                        TableCell { Badge { variant: tv, "{tl}" } }
                                        TableCell {
                                            if unassigned {
                                                span { class: "text-subtle italic", "Unassigned" }
                                            } else {
                                                "{who}"
                                            }
                                        }
                                        TableCell { "{due}" }
                                        TableCell {
                                            div { class: "whitespace-nowrap font-medium", "Logged {logged_h} h" }
                                            div {
                                                class: "text-xs text-muted whitespace-nowrap",
                                                "Approved {approved_h} h · Est {est_h} h"
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

        if let Some(task) = selected_task() {
            TaskEditModal {
                task,
                statuses: statuses.clone(),
                users: users.clone(),
                onclose: move |_| selected_task.set(None),
                onsaved: move |_| {
                    selected_task.set(None);
                    tasks_resource.restart();
                },
                ondeleted: move |_| {
                    selected_task.set(None);
                    tasks_resource.restart();
                },
            }
        }
    }
}

// ============================================================================
// Shared task-edit modal (MAPPS-165)
//
// Extracted from ProjectDetailPage so both the project detail page and the
// project tasks overview can open the same click-to-edit experience. Mounted
// only while a task is selected (the caller wraps it in `if let Some(task)`),
// so it is always `open`. Seeds its own form signals from the `task` prop,
// fetches that task's change history, PUTs `/tasks/{id}`, and reports back via
// `onsaved` (the caller refreshes its task list and clears the selection).
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct TaskEditModalProps {
    task: RemoteTask,
    statuses: Vec<RemoteTaskStatus>,
    users: Vec<RemoteUser>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
    ondeleted: EventHandler<()>,
}

#[component]
fn TaskEditModal(props: TaskEditModalProps) -> Element {
    let task = props.task.clone();
    let tid = task.id;
    let statuses = props.statuses.clone();
    let users = props.users.clone();
    let onclose = props.onclose;
    let onsaved = props.onsaved;
    let ondeleted = props.ondeleted;

    // MAPPS-357: this shared modal is opened from both the project detail and
    // tasks pages; gate its Save / Delete writes on reachability so a task
    // edit cannot be submitted into a downed server. History is a secondary
    // lookup that keeps degrading to a default.
    let can_mutate = crate::hooks::use_can_mutate();

    let mut te_title = use_signal(|| task.title.clone());
    let mut te_description = use_signal(|| task.description.clone().unwrap_or_default());
    let mut te_status = use_signal(|| task.status_id.map(|v| v.to_string()).unwrap_or_default());
    let mut te_priority = use_signal(|| {
        task.priority
            .clone()
            .unwrap_or_else(|| "medium".to_string())
    });
    let mut te_assignee = use_signal(|| {
        task.assigned_to_id
            .map(|v| v.to_string())
            .unwrap_or_default()
    });
    let mut te_estimated = use_signal(|| {
        // Pre-fill in the same preference-aware shape the Log Time field uses
        // (PMS-319), so an estimate set as 1.5h shows as "1:30" / "1.5".
        task.estimated_hours
            .map(crate::utils::duration::fmt_input_hours)
            .unwrap_or_default()
    });
    let mut te_due = use_signal(|| task.due_date.clone().unwrap_or_default());
    let mut te_submitting = use_signal(|| false);
    let mut te_error = use_signal(String::new);
    // Per-field inline validation slot for Title (PMS-518).
    let mut te_title_err = use_signal(String::new);

    let history_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<HistoryEntry>>(&format!(
            "/audit-log/entity/tasks/{tid}"
        ))
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let task_history = history_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let task_edited = task_history.iter().find(|e| e.action == "update").map(|e| {
        let who = actor_name(&users, &e.user_id);
        let when = fmt_history_dt(e.timestamp);
        if who.is_empty() {
            format!("Edited {when}")
        } else {
            format!("Edited {when} by {who}")
        }
    });

    let mut task_status_opts = vec![SelectOption::new("", "Select a status")];
    task_status_opts.extend(
        statuses
            .iter()
            .map(|s| SelectOption::new(s.id.to_string(), s.name.clone())),
    );
    let task_priority_opts = vec![
        SelectOption::new("low", "Low"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("high", "High"),
        SelectOption::new("critical", "Critical"),
    ];
    let mut task_assignee_opts = vec![SelectOption::new("", "Unassigned")];
    task_assignee_opts.extend(
        users
            .iter()
            .map(|u| SelectOption::new(u.id.to_string(), u.full_name.clone())),
    );

    let on_save = move |_| {
        if te_submitting() {
            return;
        }
        // PMS-518: Title through the shared guard so its failure shows in the
        // field's own inline slot (and focuses) instead of the top-of-modal banner.
        let mut guard = FormGuard::new();
        let title_v = te_title().trim().to_string();
        te_title_err.set(guard.field("te-title", &title_v, "Title", &[Rule::Required]));
        if guard.blocked() {
            return;
        }
        // Reject a partial/invalid due date (PMS-317) before submit.
        if let Err(e) = validate_opt_date(&te_due(), "Due date") {
            te_error.set(e);
            return;
        }
        // estimated_hours: accept decimal or H:MM (PMS-319). Empty clears it;
        // an unparseable value is a hard error here, before submit, rather
        // than silently sending null (which used to wipe the estimate).
        let est_raw = te_estimated().trim().to_string();
        let est_json = if est_raw.is_empty() {
            serde_json::Value::Null
        } else {
            match crate::utils::duration::parse_input_to_hours(&est_raw) {
                Some(h) => serde_json::json!(h),
                None => {
                    te_error
                        .set("Estimated hours must be a number (2.5) or H:MM (1:30).".to_string());
                    return;
                }
            }
        };
        spawn(async move {
            te_submitting.set(true);
            te_error.set(String::new());
            let mut body = serde_json::Map::new();
            body.insert("title".into(), serde_json::json!(te_title().trim()));
            body.insert("description".into(), serde_json::json!(te_description()));
            body.insert("priority".into(), serde_json::json!(te_priority()));
            insert_opt_uuid(&mut body, "status_id", &te_status());
            insert_opt_uuid(&mut body, "assigned_to_id", &te_assignee());
            body.insert("estimated_hours".into(), est_json);
            insert_opt_date(&mut body, "due_date", &te_due());
            let body = serde_json::Value::Object(body);
            match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                &format!("/tasks/{tid}"),
                &body,
            )
            .await
            {
                Ok(_) => {
                    te_submitting.set(false);
                    onsaved.call(());
                }
                Err(err) => {
                    te_submitting.set(false);
                    te_error.set(err);
                }
            }
        });
    };

    // MAPPS-237: Delete affordance for project tasks. The Danger button in the
    // footer opens the styled ConfirmDialog; the actual `DELETE /tasks/{id}`
    // fires from `on_confirm_delete` when confirmed (parity with the
    // project/asset detail-page delete).
    let mut te_deleting = use_signal(|| false);
    let mut confirming_delete = use_signal(|| false);

    let on_confirm_delete = move |_: ()| {
        if te_deleting() {
            return;
        }
        te_deleting.set(true);
        te_error.set(String::new());
        spawn(async move {
            match crate::hooks::fetch::api::delete_authed(&format!("/tasks/{tid}")).await {
                Ok(()) => {
                    te_deleting.set(false);
                    confirming_delete.set(false);
                    ondeleted.call(());
                }
                Err(err) => {
                    te_deleting.set(false);
                    confirming_delete.set(false);
                    te_error.set(err);
                }
            }
        });
    };

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete task".to_string(),
            message: "Delete this task? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: te_deleting(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !te_deleting() {
                    confirming_delete.set(false);
                }
            },
        }
        Modal {
            open: true,
            title: "Edit Task",
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Danger,
                    loading: te_deleting(),
                    // MAPPS-357: no deletes while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        if !te_deleting() {
                            confirming_delete.set(true);
                        }
                    },
                    "Delete"
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: te_submitting(),
                    // MAPPS-357: no save while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                    onclick: on_save,
                    "Save Changes"
                }
            },
            div { class: "space-y-4",
                if !te_error().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{te_error}" }
                }
                if let Some(m) = task_edited {
                    p { class: "text-xs text-subtle italic", "{m}" }
                }
                Input {
                    name: "te-title",
                    label: "Title",
                    required: true,
                    rules: vec![Rule::Required],
                    error: te_title_err.read().clone(),
                    value: "{te_title}",
                    oninput: move |e: FormEvent| {
                        te_title_err.set(String::new());
                        te_title.set(e.value());
                    },
                }
                Textarea {
                    name: "te-description",
                    label: "Description",
                    rows: 4,
                    value: "{te_description}",
                    oninput: move |e: FormEvent| te_description.set(e.value()),
                }
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                    Select {
                        name: "te-status",
                        label: "Status",
                        options: task_status_opts.clone(),
                        value: "{te_status}",
                        onchange: move |e: FormEvent| te_status.set(e.value()),
                    }
                    Select {
                        name: "te-priority",
                        label: "Priority",
                        options: task_priority_opts.clone(),
                        value: "{te_priority}",
                        onchange: move |e: FormEvent| te_priority.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                    Select {
                        name: "te-assignee",
                        label: "Assignee",
                        options: task_assignee_opts.clone(),
                        value: "{te_assignee}",
                        onchange: move |e: FormEvent| te_assignee.set(e.value()),
                    }
                    Input {
                        name: "te-estimated",
                        label: "Estimated Hours",
                        // Free-text so H:MM can be typed (PMS-319).
                        r#type: "text",
                        placeholder: "2, 2.5, or 1:30",
                        help: "Decimal hours or H:MM.",
                        value: "{te_estimated}",
                        oninput: move |e: FormEvent| te_estimated.set(e.value()),
                    }
                }
                crate::components::DateField {
                    name: "te-due",
                    label: "Due Date",
                    value: "{te_due}",
                    oninput: move |e: FormEvent| te_due.set(e.value()),
                }

                // Change history for this task.
                div { class: "border-t border-line pt-3",
                    p { class: "text-sm font-medium text-content mb-2", "Change History" }
                    if task_history.is_empty() {
                        p { class: "text-sm text-subtle italic", "No edits yet." }
                    } else {
                        div { class: "space-y-2 text-sm max-h-48 overflow-y-auto",
                            for e in task_history.iter().take(20) {
                                {
                                    let label = action_label(&e.action);
                                    let fields = fields_label(&e.changed_fields);
                                    let who = actor_name(&users, &e.user_id);
                                    let when = fmt_history_dt(e.timestamp);
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
                                                if !who.is_empty() {
                                                    p { class: "text-xs text-subtle", "by {who}" }
                                                }
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
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{
        validate_budget, validate_budget_hours, validate_project_name, BUDGET_AMOUNT_MAX,
        PROJECT_NAME_MAX,
    };

    #[test]
    fn name_required_and_capped() {
        assert!(validate_project_name("   ").is_err());
        assert_eq!(validate_project_name("  Acme  ").unwrap(), "Acme");
        assert!(validate_project_name(&"x".repeat(PROJECT_NAME_MAX)).is_ok());
        assert!(validate_project_name(&"x".repeat(PROJECT_NAME_MAX + 1)).is_err());
    }

    #[test]
    fn budget_optional_nonneg_two_dp() {
        // Blank -> None.
        assert_eq!(validate_budget("", "Budget amount").unwrap(), None);
        assert_eq!(validate_budget("  ", "Budget amount").unwrap(), None);
        // Valid numbers.
        assert_eq!(
            validate_budget("500", "Budget amount").unwrap(),
            Some(500.0)
        );
        assert_eq!(validate_budget("8.5", "Budget hours").unwrap(), Some(8.5));
        assert_eq!(validate_budget("8.50", "Budget hours").unwrap(), Some(8.5));
        // Rejected: non-numeric, negative, more than 2 decimal places.
        assert!(validate_budget("Bobby Tables", "Budget amount").is_err());
        assert!(validate_budget("-1", "Budget amount").is_err());
        assert!(validate_budget("1.234", "Budget hours").is_err());
    }

    #[test]
    fn budget_amount_rejects_out_of_magnitude() {
        // The documented maximum is accepted; anything larger is rejected with a
        // field message before it can overflow server-side (MAPPS-212).
        assert_eq!(
            validate_budget(&format!("{BUDGET_AMOUNT_MAX:.2}"), "Budget amount").unwrap(),
            Some(BUDGET_AMOUNT_MAX)
        );
        assert!(validate_budget("1000000000000", "Budget amount").is_err());
        // The 41-digit value from the repro must not pass.
        assert!(validate_budget(&"9".repeat(41), "Budget amount").is_err());
    }

    #[test]
    fn budget_hours_distinguishes_out_of_range_from_non_numeric() {
        // Blank -> None; well-formed positive values pass.
        assert_eq!(validate_budget_hours("").unwrap(), None);
        assert_eq!(validate_budget_hours("  ").unwrap(), None);
        assert_eq!(validate_budget_hours("2.5").unwrap(), Some(2.5));
        assert_eq!(validate_budget_hours("1:30").unwrap(), Some(1.5));

        // Negative / zero are out of range, not "not a number" (the MAPPS-212 bug).
        let neg = validate_budget_hours("-9").unwrap_err();
        assert!(neg.contains("greater than 0"), "got: {neg}");
        assert!(validate_budget_hours("-1:30")
            .unwrap_err()
            .contains("greater than 0"));
        assert!(validate_budget_hours("0")
            .unwrap_err()
            .contains("greater than 0"));

        // Genuinely non-numeric input still reports the format message.
        let bad = validate_budget_hours("abc").unwrap_err();
        assert!(bad.contains("must be a number"), "got: {bad}");
    }
}

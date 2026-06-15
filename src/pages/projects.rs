//! Project pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, Input, Modal,
    PageHeader, PencilIcon, PlusIcon, SearchInput, Select, SelectOption, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow, Textarea,
};
use crate::utils::Paginated;
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
fn title_field(f: &str) -> String {
    let mut s = f.replace('_', " ");
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

/// Validate an optional `yyyy-mm-dd` date field (PMS-317). Blank is allowed
/// (`Ok`). A non-empty value must parse as a full calendar date, so a partial
/// entry (e.g. a month with no day/year) is rejected before submit instead of
/// being sent on. `label` names the field in the message.
fn validate_opt_date(raw: &str, label: &str) -> Result<(), String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(());
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| format!("{label} must be a valid date."))
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
    if let Some((_, frac)) = s.split_once('.') {
        if frac.trim_end_matches('0').len() > 2 {
            return Err(format!("{label} must have at most 2 decimal places."));
        }
    }
    Ok(Some(value))
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

/// Whole-dollar money, or "-" when absent.
fn fmt_money(v: Option<f64>) -> String {
    match v {
        Some(n) => format!("${n:.0}"),
        None => "-".to_string(),
    }
}

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

    let projects_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteProject>>("/projects")
            .await
            .ok()
            .map(|p| p.data)
    });
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let snapshot = projects_resource.read_unchecked().clone();
    // `None` while loading; `Some(None)` on fetch failure; `Some(Some(rows))`.
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let projects: Vec<RemoteProject> = snapshot.flatten().unwrap_or_default();
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
    let total_value_label = format!("${total_value:.0}");

    // Client-side search + status filter.
    let needle = search.read().to_lowercase();
    let sf = status_filter.read().clone();
    let filtered: Vec<&RemoteProject> = projects
        .iter()
        .filter(|p| sf.is_empty() || p.status == sf)
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .collect();

    rsx! {
        AppLayout { title: "Projects",
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

            // Stats
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Active Projects" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "{active}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "On Hold" }
                    p { class: "text-2xl font-bold text-yellow-600", "{on_hold}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Completed" }
                    p { class: "text-2xl font-bold text-green-600", "{completed}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Total Budget" }
                    p { class: "text-2xl font-bold text-blue-600", "{total_value_label}" }
                }
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        SearchInput {
                            value: search.read().clone(),
                            placeholder: "Search projects...",
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
                Card { p { class: "text-sm text-gray-400", "Loading projects…" } }
            } else if load_failed {
                Card {
                    p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                        "Could not load projects from the server."
                    }
                }
            } else if filtered.is_empty() {
                Card {
                    p { class: "text-sm text-gray-400 italic",
                        if projects.is_empty() {
                            "No projects yet. Create one to get started."
                        } else {
                            "No projects match the current filters."
                        }
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
                                None => "bg-gray-400",
                            };
                            let due = fmt_date(&p.target_end_date);
                            let budget = fmt_money(p.budget_amount);
                            let pid = p.id.to_string();
                            rsx! {
                                Link {
                                    key: "{pid}",
                                    to: Route::ProjectDetail { id: pid.clone() },
                                    Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                                        div { class: "flex items-start justify-between mb-4",
                                            div {
                                                h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                                                    "{p.name}"
                                                }
                                                p { class: "text-sm text-gray-500 dark:text-gray-400",
                                                    "{cname}"
                                                }
                                            }
                                            Badge { variant, "{label}" }
                                        }

                                        // Budget utilization (actual vs budget)
                                        div { class: "mb-4",
                                            div { class: "flex justify-between text-sm mb-1",
                                                span { class: "text-gray-500 dark:text-gray-400", "Budget used" }
                                                if let Some(u) = util {
                                                    span { class: "font-medium text-gray-900 dark:text-white", "{u}%" }
                                                } else {
                                                    span { class: "text-gray-400", "n/a" }
                                                }
                                            }
                                            div { class: "w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2",
                                                div {
                                                    class: "{bar_color} h-2 rounded-full transition-all",
                                                    style: "width: {util.unwrap_or(0)}%",
                                                }
                                            }
                                        }

                                        // Footer info
                                        div { class: "flex justify-between text-sm",
                                            div {
                                                span { class: "text-gray-500 dark:text-gray-400", "Due: " }
                                                span { class: "text-gray-900 dark:text-white", "{due}" }
                                            }
                                            div {
                                                span { class: "text-gray-500 dark:text-gray-400", "Budget: " }
                                                span { class: "font-medium text-gray-900 dark:text-white", "{budget}" }
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

/// New project page
#[component]
pub fn ProjectNewPage() -> Element {
    let mut name = use_signal(String::new);
    let mut company = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut budget_amount = use_signal(String::new);
    let mut budget_hours = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-176).
    let mut name_err = use_signal(String::new);
    let mut amount_err = use_signal(String::new);
    let mut hours_err = use_signal(String::new);

    // Real company picker from the live companies list.
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let mut company_options = vec![SelectOption::new("", "No company")];
    company_options.extend(
        companies
            .iter()
            .map(|c| SelectOption::new(c.id.to_string(), c.name.clone())),
    );

    let err = error.read().clone();

    rsx! {
        AppLayout { title: "New Project",
            PageHeader {
                title: "New Project",
                subtitle: "Create a new project",
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
                        // Per-field client validation mirrors the server rules (MAPPS-176).
                        let project_name = match validate_project_name(&name.read()) {
                            Ok(n) => n,
                            Err(msg) => {
                                name_err.set(msg);
                                return;
                            }
                        };
                        let amount = match validate_budget(&budget_amount.read(), "Budget amount") {
                            Ok(v) => v,
                            Err(msg) => {
                                amount_err.set(msg);
                                return;
                            }
                        };
                        // Budget hours is a duration: accept decimal or H:MM
                        // (PMS-340), reusing the Log Time parser. Blank leaves
                        // it unset; an unparseable value errors inline.
                        let hours: Option<f64> = {
                            let raw = budget_hours.read().trim().to_string();
                            if raw.is_empty() {
                                None
                            } else {
                                match crate::utils::duration::parse_input_to_hours(&raw) {
                                    Some(h) => Some(h),
                                    None => {
                                        hours_err.set(
                                            "Budget hours must be a number (2.5) or H:MM (1:30)."
                                                .to_string(),
                                        );
                                        return;
                                    }
                                }
                            }
                        };
                        is_submitting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let mut body = serde_json::json!({
                                    "name": project_name,
                                    "description": desc,
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
                                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                        "/projects",
                                        &body,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::ProjectList {});
                                    }
                                    Err(e) => {
                                        error.set(format!("Could not create project: {e}"));
                                    }
                                }
                            }
                            is_submitting.set(false);
                        });
                    },

                    if !err.is_empty() {
                        div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                            p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                        }
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

                    Select {
                        name: "company",
                        label: "Company",
                        options: company_options,
                        value: company.read().clone(),
                        placeholder: "Select a company",
                        onchange: move |e: FormEvent| company.set(e.value()),
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Project description...",
                        rows: 4,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Input {
                            name: "budget_amount",
                            label: "Budget Amount ($)",
                            r#type: "number",
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
                            placeholder: "2.5 or 1:30",
                            help: "Decimal hours (2.5) or H:MM (1:30).",
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
                            "Create Project"
                        }
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
    let project_resource = use_resource(move || {
        let id = id_for_project.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
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
    let project = snapshot.flatten();
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

    let header_title = match project.as_ref() {
        Some(p) if !p.name.trim().is_empty() => p.name.clone(),
        Some(_) => format!("Project {}", props.id),
        None if is_loading => "Loading…".to_string(),
        None => "Project".to_string(),
    };

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
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Link {
                        to: Route::ProjectTasks { id: props.id.clone() },
                        Button { variant: ButtonVariant::Secondary, "View Tasks" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| {
                            t_error.set(String::new());
                            show_task_modal.set(true);
                        },
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Task"
                    }
                    if let Some(p) = project.clone() {
                        Button {
                            variant: ButtonVariant::Secondary,
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
                        onclick: move |_| {
                            let id = id_for_delete.clone();
                            deleting.set(true);
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let confirmed = web_sys::window()
                                        .and_then(|w| {
                                            w.confirm_with_message(
                                                "Delete this project? This cannot be undone.",
                                            )
                                            .ok()
                                        })
                                        .unwrap_or(false);
                                    if confirmed {
                                        let path = format!("/projects/{id}");
                                        if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                                            navigator.push(Route::ProjectList {});
                                        }
                                    }
                                }
                                deleting.set(false);
                            });
                        },
                        "Delete"
                    }
                },
            }

            if is_loading {
                Card { p { class: "text-sm text-gray-400", "Loading project…" } }
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
                                    if let Some(d) = description {
                                        p { class: "text-gray-700 dark:text-gray-300 whitespace-pre-wrap", "{d}" }
                                    } else {
                                        p { class: "text-sm text-gray-400 italic", "No description provided." }
                                    }
                                    if let Some(m) = project_edited.clone() {
                                        p { class: "text-xs text-gray-400 italic mt-3", "{m}" }
                                    }
                                }
                                Card { title: "Tasks",
                                    if tasks.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No tasks yet." }
                                    } else {
                                        div { class: "space-y-3",
                                            for t in tasks.iter() {
                                                {
                                                    let (tv, tl) = task_status_badge(&statuses, &t.status_id);
                                                    let who = user_name(&users, &t.assigned_to_id);
                                                    // Clicking a row opens the task in the edit modal.
                                                    let task = t.clone();
                                                    let open_task = move |_| selected_task.set(Some(task.clone()));
                                                    rsx! {
                                                        div {
                                                            class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800 rounded-lg cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors",
                                                            onclick: open_task,
                                                            div {
                                                                p { class: "font-medium text-gray-900 dark:text-white", "{t.title}" }
                                                                p { class: "text-sm text-gray-500 dark:text-gray-400", "{who}" }
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

                            // Sidebar
                            div { class: "space-y-6",
                                Card { title: "Details",
                                    dl { class: "space-y-4",
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Status" }
                                            dd { Badge { variant: status_variant, "{status_label}" } }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Progress" }
                                            dd { class: "text-sm font-medium",
                                                "{progress}% ({completed_tasks}/{total_tasks} tasks)"
                                            }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Start Date" }
                                            dd { class: "text-sm", "{fmt_date(&p.start_date)}" }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Due Date" }
                                            dd { class: "text-sm", "{fmt_date(&p.target_end_date)}" }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Project Manager" }
                                            dd { class: "text-sm", "{pm}" }
                                        }
                                    }
                                }
                                Card { title: "Budget",
                                    div { class: "space-y-3",
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Total Budget" }
                                            span { class: "font-medium", "{fmt_money(p.budget_amount)}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Spent" }
                                            span { class: "font-medium text-green-600", "{fmt_money(p.actual_amount)}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Remaining" }
                                            span { class: "font-medium", "${remaining:.0}" }
                                        }
                                        div { class: "w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2 mt-2",
                                            div { class: "bg-green-600 h-2 rounded-full", style: "width: {util}%" }
                                        }
                                    }
                                }
                                Card { title: "Time",
                                    div { class: "space-y-3",
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Estimated" }
                                            span { class: "font-medium", "{fmt_hours(p.budget_hours)} h" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Logged" }
                                            span { class: "font-medium", "{fmt_hours(p.actual_hours)} h" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Remaining" }
                                            span { class: "font-medium", "{remaining_h:.1} h" }
                                        }
                                    }
                                }
                                // PMS-205: the project's own change history.
                                Card { title: "Change History",
                                    if project_history.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No edits yet." }
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
                                                                p { class: "text-gray-700 dark:text-gray-300",
                                                                    if fields.is_empty() {
                                                                        "{label}"
                                                                    } else {
                                                                        "{label}: {fields}"
                                                                    }
                                                                }
                                                                if !who.is_empty() {
                                                                    p { class: "text-xs text-gray-400", "by {who}" }
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
                        onclick: move |_| {
                            t_error.set(String::new());
                            let title = t_title.read().trim().to_string();
                            let status_id = t_status.read().clone();
                            let priority = t_priority.read().clone();
                            let est_raw = t_estimated.read().trim().to_string();
                            let due = t_due.read().clone();
                            let assignee = t_assignee.read().clone();
                            if title.is_empty() {
                                t_error.set("Task title is required.".to_string());
                                return;
                            }
                            if status_id.is_empty() {
                                t_error.set("Please pick a status.".to_string());
                                return;
                            }
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
                        div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                            p { class: "text-sm text-red-600 dark:text-red-400", "{t_err}" }
                        }
                    }
                    crate::components::Input {
                        name: "task_title",
                        label: "Title",
                        placeholder: "Task title",
                        required: true,
                        value: t_title.read().clone(),
                        oninput: move |e: FormEvent| t_title.set(e.value()),
                    }
                    Select {
                        name: "task_status",
                        label: "Status",
                        options: status_options,
                        value: t_status.read().clone(),
                        onchange: move |e: FormEvent| t_status.set(e.value()),
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
                            placeholder: "2.5 or 1:30",
                            help: "Decimal hours (2.5) or H:MM (1:30).",
                            value: t_estimated.read().clone(),
                            oninput: move |e: FormEvent| t_estimated.set(e.value()),
                        }
                        crate::components::Input {
                            name: "task_due",
                            label: "Due Date",
                            r#type: "date",
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
                    let project_name = match validate_project_name(&pe_name()) {
                        Ok(n) => n,
                        Err(msg) => {
                            pe_name_err.set(msg);
                            return;
                        }
                    };
                    let amount = match validate_budget(&pe_budget_amount(), "Budget amount") {
                        Ok(v) => v,
                        Err(msg) => {
                            pe_amount_err.set(msg);
                            return;
                        }
                    };
                    // Budget hours: accept decimal or H:MM (PMS-340). Blank
                    // leaves it unset; an unparseable value errors inline.
                    let hours: Option<f64> = {
                        let raw = pe_budget_hours();
                        let raw = raw.trim();
                        if raw.is_empty() {
                            None
                        } else {
                            match crate::utils::duration::parse_input_to_hours(raw) {
                                Some(h) => Some(h),
                                None => {
                                    pe_hours_err.set(
                                        "Budget hours must be a number (2.5) or H:MM (1:30)."
                                            .to_string(),
                                    );
                                    return;
                                }
                            }
                        }
                    };
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
                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
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
                                pe_error.set(err);
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
                                value: "{pe_description}",
                                oninput: move |e: FormEvent| pe_description.set(e.value()),
                            }
                            div { class: "grid grid-cols-2 gap-4",
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
                            div { class: "grid grid-cols-2 gap-4",
                                Input {
                                    name: "pe-start",
                                    label: "Start Date",
                                    r#type: "date",
                                    value: "{pe_start}",
                                    oninput: move |e: FormEvent| pe_start.set(e.value()),
                                }
                                Input {
                                    name: "pe-due",
                                    label: "Target End Date",
                                    r#type: "date",
                                    value: "{pe_due}",
                                    oninput: move |e: FormEvent| pe_due.set(e.value()),
                                }
                            }
                            div { class: "grid grid-cols-2 gap-4",
                                Input {
                                    name: "pe-budget-amount",
                                    label: "Budget Amount",
                                    r#type: "number",
                                    value: "{pe_budget_amount}",
                                    error: pe_amount_err(),
                                    oninput: move |e: FormEvent| pe_budget_amount.set(e.value()),
                                }
                                Input {
                                    name: "pe-budget-hours",
                                    label: "Budget Hours",
                                    // Free-text for H:MM input (PMS-340).
                                    r#type: "text",
                                    placeholder: "2.5 or 1:30",
                                    help: "Decimal hours (2.5) or H:MM (1:30).",
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
                }
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

    rsx! {
        AppLayout { title: "Project Tasks",
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
                            TableRow { TableCell { class: "text-gray-400", "Loading…" } }
                        } else if load_failed {
                            TableRow {
                                TableCell { class: "text-yellow-600 dark:text-yellow-400",
                                    "Could not load tasks."
                                }
                            }
                        } else if tasks.is_empty() {
                            TableRow {
                                TableCell { class: "text-gray-400 italic", "No tasks yet." }
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
                                                    span { class: "text-gray-400 italic", "Unassigned" }
                                                } else {
                                                    "{who}"
                                                }
                                            }
                                            TableCell { "{due}" }
                                            TableCell {
                                                div { class: "whitespace-nowrap font-medium", "Logged {logged_h} h" }
                                                div {
                                                    class: "text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap",
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
                }
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
}

#[component]
fn TaskEditModal(props: TaskEditModalProps) -> Element {
    let task = props.task.clone();
    let tid = task.id;
    let statuses = props.statuses.clone();
    let users = props.users.clone();
    let onclose = props.onclose;
    let onsaved = props.onsaved;

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
        if te_title().trim().is_empty() {
            te_error.set("Task title is required.".to_string());
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

    rsx! {
        Modal {
            open: true,
            title: "Edit Task",
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: te_submitting(),
                    onclick: on_save,
                    "Save Changes"
                }
            },
            div { class: "space-y-4",
                if !te_error().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{te_error}" }
                }
                if let Some(m) = task_edited {
                    p { class: "text-xs text-gray-400 italic", "{m}" }
                }
                Input {
                    name: "te-title",
                    label: "Title",
                    required: true,
                    value: "{te_title}",
                    oninput: move |e: FormEvent| te_title.set(e.value()),
                }
                Textarea {
                    name: "te-description",
                    label: "Description",
                    rows: 4,
                    value: "{te_description}",
                    oninput: move |e: FormEvent| te_description.set(e.value()),
                }
                div { class: "grid grid-cols-2 gap-4",
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
                div { class: "grid grid-cols-2 gap-4",
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
                        placeholder: "2.5 or 1:30",
                        help: "Decimal hours (2.5) or H:MM (1:30).",
                        value: "{te_estimated}",
                        oninput: move |e: FormEvent| te_estimated.set(e.value()),
                    }
                }
                Input {
                    name: "te-due",
                    label: "Due Date",
                    r#type: "date",
                    value: "{te_due}",
                    oninput: move |e: FormEvent| te_due.set(e.value()),
                }

                // Change history for this task.
                div { class: "border-t border-gray-200 dark:border-gray-700 pt-3",
                    p { class: "text-sm font-medium text-gray-700 dark:text-gray-300 mb-2", "Change History" }
                    if task_history.is_empty() {
                        p { class: "text-sm text-gray-400 italic", "No edits yet." }
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
                                                p { class: "text-gray-700 dark:text-gray-300",
                                                    if fields.is_empty() {
                                                        "{label}"
                                                    } else {
                                                        "{label}: {fields}"
                                                    }
                                                }
                                                if !who.is_empty() {
                                                    p { class: "text-xs text-gray-400", "by {who}" }
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
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{validate_budget, validate_project_name, PROJECT_NAME_MAX};

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
}

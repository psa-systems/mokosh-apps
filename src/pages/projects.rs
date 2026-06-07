//! Project pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, SearchInput, Select, SelectOption, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::Route;

/// `PaginatedResponse<T>` envelope (`{ "data": [...], "meta": {...} }`);
/// serde drops `meta`.
#[derive(Clone, Debug, Deserialize)]
struct Paginated<T> {
    data: Vec<T>,
}

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
#[derive(Clone, Debug, Deserialize)]
struct RemoteTask {
    #[serde(default)]
    title: String,
    #[serde(default)]
    status_id: Option<uuid::Uuid>,
    #[serde(default)]
    assigned_to_id: Option<uuid::Uuid>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    estimated_hours: Option<f64>,
    #[serde(default, deserialize_with = "de_flex_f64")]
    actual_hours: Option<f64>,
    #[serde(default)]
    due_date: Option<String>,
}

/// A per-tenant task status (`GET /api/v1/task-statuses`).
#[derive(Clone, Debug, Deserialize)]
struct RemoteTaskStatus {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_completed: bool,
}

/// A user, used to resolve `assigned_to_id` / `project_manager_id` to a name.
#[derive(Clone, Debug, Deserialize)]
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
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

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
                        let project_name = name.read().trim().to_string();
                        let company_id = company.read().clone();
                        let desc = description.read().clone();
                        if project_name.is_empty() {
                            error.set("Please enter a project name.".to_string());
                            return;
                        }
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
    let tasks_resource = use_resource(move || {
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

    let snapshot = project_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let project = snapshot.flatten();
    let tasks = tasks_resource.read_unchecked().clone().unwrap_or_default();
    let statuses = statuses_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

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
                                Card { title: "Overview",
                                    if let Some(d) = description {
                                        p { class: "text-gray-700 dark:text-gray-300 whitespace-pre-wrap", "{d}" }
                                    } else {
                                        p { class: "text-sm text-gray-400 italic", "No description provided." }
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
                                                    rsx! {
                                                        div { class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800 rounded-lg",
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
                            }
                        }
                    }
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
    let tasks_resource = use_resource(move || {
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

    rsx! {
        AppLayout { title: "Project Tasks",
            PageHeader { title: "Project Tasks", subtitle: "Tasks for this project" }

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
                                    let hours = format!(
                                        "{} / {}",
                                        fmt_hours(t.actual_hours),
                                        fmt_hours(t.estimated_hours),
                                    );
                                    let unassigned = t.assigned_to_id.is_none();
                                    rsx! {
                                        TableRow {
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
                                            TableCell { "{hours}" }
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

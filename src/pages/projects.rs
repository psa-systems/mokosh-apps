//! Project pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, SearchInput, Select, SelectOption, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::Route;

/// Project list page
#[component]
pub fn ProjectListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("active", "Active"),
        SelectOption::new("on_hold", "On Hold"),
        SelectOption::new("completed", "Completed"),
    ];

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
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "8" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "On Hold" }
                    p { class: "text-2xl font-bold text-yellow-600", "2" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Completed (YTD)" }
                    p { class: "text-2xl font-bold text-green-600", "15" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Total Value" }
                    p { class: "text-2xl font-bold text-blue-600", "$125,000" }
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

            // Project cards
            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
                ProjectCard {
                    id: "1",
                    name: "Network Infrastructure Upgrade",
                    company: "Acme Corp",
                    status: "Active",
                    progress: 65,
                    due_date: "Feb 28, 2025",
                    budget: "$45,000",
                }
                ProjectCard {
                    id: "2",
                    name: "Office 365 Migration",
                    company: "TechStart Inc",
                    status: "Active",
                    progress: 30,
                    due_date: "Mar 15, 2025",
                    budget: "$12,000",
                }
                ProjectCard {
                    id: "3",
                    name: "Security Audit & Remediation",
                    company: "Global Widgets",
                    status: "On Hold",
                    progress: 15,
                    due_date: "Apr 30, 2025",
                    budget: "$28,000",
                }
                ProjectCard {
                    id: "4",
                    name: "VoIP Phone System",
                    company: "Acme Corp",
                    status: "Active",
                    progress: 80,
                    due_date: "Jan 31, 2025",
                    budget: "$18,000",
                }
                ProjectCard {
                    id: "5",
                    name: "Backup Solution Implementation",
                    company: "TechStart Inc",
                    status: "Active",
                    progress: 45,
                    due_date: "Feb 15, 2025",
                    budget: "$8,500",
                }
                ProjectCard {
                    id: "6",
                    name: "New Office Setup",
                    company: "Global Widgets",
                    status: "Active",
                    progress: 90,
                    due_date: "Jan 20, 2025",
                    budget: "$15,000",
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProjectCardProps {
    id: String,
    name: String,
    company: String,
    status: String,
    progress: u32,
    due_date: String,
    budget: String,
}

#[component]
fn ProjectCard(props: ProjectCardProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Active" => BadgeVariant::Green,
        "On Hold" => BadgeVariant::Yellow,
        "Completed" => BadgeVariant::Blue,
        _ => BadgeVariant::Gray,
    };

    let progress_color = if props.progress >= 75 {
        "bg-green-600"
    } else if props.progress >= 50 {
        "bg-blue-600"
    } else if props.progress >= 25 {
        "bg-yellow-500"
    } else {
        "bg-gray-400"
    };

    rsx! {
        Link {
            to: Route::ProjectDetail { id: props.id.clone() },
            Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                div { class: "flex items-start justify-between mb-4",
                    div {
                        h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{props.name}"
                        }
                        p { class: "text-sm text-gray-500 dark:text-gray-400",
                            "{props.company}"
                        }
                    }
                    Badge { variant: status_variant, "{props.status}" }
                }

                // Progress bar
                div { class: "mb-4",
                    div { class: "flex justify-between text-sm mb-1",
                        span { class: "text-gray-500 dark:text-gray-400", "Progress" }
                        span { class: "font-medium text-gray-900 dark:text-white", "{props.progress}%" }
                    }
                    div { class: "w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2",
                        div {
                            class: "{progress_color} h-2 rounded-full transition-all",
                            style: "width: {props.progress}%",
                        }
                    }
                }

                // Footer info
                div { class: "flex justify-between text-sm",
                    div {
                        span { class: "text-gray-500 dark:text-gray-400", "Due: " }
                        span { class: "text-gray-900 dark:text-white", "{props.due_date}" }
                    }
                    div {
                        span { class: "text-gray-500 dark:text-gray-400", "Budget: " }
                        span { class: "font-medium text-gray-900 dark:text-white", "{props.budget}" }
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

    let company_options = vec![
        SelectOption::new("1", "Acme Corp"),
        SelectOption::new("2", "TechStart Inc"),
        SelectOption::new("3", "Global Widgets"),
    ];

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
                        is_submitting.set(true);
                        // P1-04: mock 1s "submit" so the spinner doesn't
                        // get stuck. Real POST lands when server F7-style
                        // module exists for projects.
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use gloo_timers::future::TimeoutFuture;
                                TimeoutFuture::new(1000).await;
                            }
                            is_submitting.set(false);
                            dioxus::prelude::navigator().push(Route::ProjectList {});
                        });
                    },

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
                        required: true,
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
    let header_title = format!("Project {}", props.id);
    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Link {
                        to: Route::ProjectTasks { id: props.id.clone() },
                        Button {
                            variant: ButtonVariant::Secondary,
                            "View Tasks"
                        }
                    }
                    // F5: Add Task here was decorative (no onclick, no
                    // server projects module yet). Hidden until the
                    // server lands; live wiring tracked under PMC-39.
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Overview
                    Card { title: "Overview",
                        p { class: "text-gray-700 dark:text-gray-300",
                            "Complete upgrade of network infrastructure including new switches, "
                            "firewall replacement, and wireless access points for all three floors."
                        }
                    }

                    // Tasks summary
                    Card { title: "Tasks",
                        div { class: "space-y-3",
                            TaskItem {
                                name: "Site survey and documentation",
                                status: "Completed",
                                assignee: "John Smith",
                            }
                            TaskItem {
                                name: "Hardware procurement",
                                status: "Completed",
                                assignee: "Jane Doe",
                            }
                            TaskItem {
                                name: "Core switch installation",
                                status: "In Progress",
                                assignee: "John Smith",
                            }
                            TaskItem {
                                name: "Access point deployment",
                                status: "Pending",
                                assignee: "Unassigned",
                            }
                            TaskItem {
                                name: "Testing and documentation",
                                status: "Pending",
                                assignee: "Unassigned",
                            }
                        }
                    }

                    // Recent activity
                    Card { title: "Recent Activity",
                        div { class: "space-y-3 text-sm",
                            ActivityItem {
                                user: "John Smith",
                                action: "completed task 'Hardware procurement'",
                                time: "2 hours ago",
                            }
                            ActivityItem {
                                user: "Jane Doe",
                                action: "added 4.0 hours to 'Core switch installation'",
                                time: "3 hours ago",
                            }
                            ActivityItem {
                                user: "System",
                                action: "project progress updated to 65%",
                                time: "1 day ago",
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    // Status card
                    Card { title: "Details",
                        dl { class: "space-y-4",
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Status" }
                                dd { Badge { variant: BadgeVariant::Green, "Active" } }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Progress" }
                                dd { class: "text-sm font-medium", "65%" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Start Date" }
                                dd { class: "text-sm", "Dec 1, 2024" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Due Date" }
                                dd { class: "text-sm", "Feb 28, 2025" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Project Manager" }
                                dd { class: "text-sm", "Jane Doe" }
                            }
                        }
                    }

                    // Budget
                    Card { title: "Budget",
                        div { class: "space-y-3",
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Total Budget" }
                                span { class: "font-medium", "$45,000" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Spent" }
                                span { class: "font-medium text-green-600", "$28,500" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Remaining" }
                                span { class: "font-medium", "$16,500" }
                            }
                            // Progress bar
                            div { class: "w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2 mt-2",
                                div { class: "bg-green-600 h-2 rounded-full", style: "width: 63%" }
                            }
                        }
                    }

                    // Time
                    Card { title: "Time",
                        div { class: "space-y-3",
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Estimated" }
                                span { class: "font-medium", "120 hours" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Logged" }
                                span { class: "font-medium", "78 hours" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Remaining" }
                                span { class: "font-medium", "42 hours" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TaskItemProps {
    name: String,
    status: String,
    assignee: String,
}

#[component]
fn TaskItem(props: TaskItemProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Completed" => BadgeVariant::Green,
        "In Progress" => BadgeVariant::Blue,
        _ => BadgeVariant::Gray,
    };

    rsx! {
        div { class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div {
                p { class: "font-medium text-gray-900 dark:text-white", "{props.name}" }
                p { class: "text-sm text-gray-500 dark:text-gray-400", "{props.assignee}" }
            }
            Badge { variant: status_variant, "{props.status}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ActivityItemProps {
    user: String,
    action: String,
    time: String,
}

#[component]
fn ActivityItem(props: ActivityItemProps) -> Element {
    rsx! {
        div { class: "flex justify-between",
            p { class: "text-gray-700 dark:text-gray-300",
                span { class: "font-medium", "{props.user}" }
                " {props.action}"
            }
            span { class: "text-gray-500 dark:text-gray-400 whitespace-nowrap ml-4", "{props.time}" }
        }
    }
}

/// Project tasks page
#[derive(Props, Clone, PartialEq)]
pub struct ProjectTasksPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn ProjectTasksPage(props: ProjectTasksPageProps) -> Element {
    let header_title = format!("Project {} - Tasks", props.id);
    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                // F5: Add Task here was decorative (no onclick, no
                // server projects module yet). Hidden until the server
                // lands; reopen PMC-39 alongside that backend story.
            }

            DataTable {
                total_items: 5,
                current_page: 1,
                per_page: 25,
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
                        TableRow {
                            TableCell { "Site survey and documentation" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Completed" } }
                            TableCell { "John Smith" }
                            TableCell { "Dec 15, 2024" }
                            TableCell { "8 / 8" }
                        }
                        TableRow {
                            TableCell { "Hardware procurement" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Completed" } }
                            TableCell { "Jane Doe" }
                            TableCell { "Dec 30, 2024" }
                            TableCell { "12 / 10" }
                        }
                        TableRow {
                            TableCell { "Core switch installation" }
                            TableCell { Badge { variant: BadgeVariant::Blue, "In Progress" } }
                            TableCell { "John Smith" }
                            TableCell { "Jan 31, 2025" }
                            TableCell { "24 / 40" }
                        }
                        TableRow {
                            TableCell { "Access point deployment" }
                            TableCell { Badge { variant: BadgeVariant::Gray, "Pending" } }
                            TableCell { class: "text-gray-400 italic", "Unassigned" }
                            TableCell { "Feb 15, 2025" }
                            TableCell { "0 / 32" }
                        }
                        TableRow {
                            TableCell { "Testing and documentation" }
                            TableCell { Badge { variant: BadgeVariant::Gray, "Pending" } }
                            TableCell { class: "text-gray-400 italic", "Unassigned" }
                            TableCell { "Feb 28, 2025" }
                            TableCell { "0 / 16" }
                        }
                    }
                }
            }
        }
    }
}

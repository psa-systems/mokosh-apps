//! Ticket pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ClockIcon, DataTable, IconSize,
    Modal, PageHeader, PlusIcon, SearchInput, Select, SelectOption, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow, Textarea, UserCircleIcon,
};
use crate::Route;

/// Ticket list page
#[component]
pub fn TicketListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);
    let mut priority_filter = use_signal(String::new);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("open", "Open"),
        SelectOption::new("in_progress", "In Progress"),
        SelectOption::new("pending", "Pending"),
        SelectOption::new("resolved", "Resolved"),
        SelectOption::new("closed", "Closed"),
    ];

    let priority_options = vec![
        SelectOption::new("", "All Priorities"),
        SelectOption::new("critical", "Critical"),
        SelectOption::new("high", "High"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("low", "Low"),
    ];

    rsx! {
        AppLayout { title: "Tickets",
            PageHeader {
                title: "Tickets",
                subtitle: "Manage support tickets and service requests",
                actions: rsx! {
                    Link {
                        to: Route::TicketNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Ticket"
                        }
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        SearchInput {
                            value: search.read().clone(),
                            placeholder: "Search tickets...",
                            oninput: move |e: FormEvent| search.set(e.value()),
                        }
                    }
                    div { class: "flex gap-4",
                        Select {
                            name: "status",
                            options: status_options,
                            value: status_filter.read().clone(),
                            placeholder: "Status",
                            onchange: move |e: FormEvent| status_filter.set(e.value()),
                        }
                        Select {
                            name: "priority",
                            options: priority_options,
                            value: priority_filter.read().clone(),
                            placeholder: "Priority",
                            onchange: move |e: FormEvent| priority_filter.set(e.value()),
                        }
                    }
                }
            }

            // Ticket table
            DataTable {
                total_items: 50,
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Ticket" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { sortable: true, "Status" }
                            TableHeader { sortable: true, "Priority" }
                            TableHeader { sortable: true, "Assigned To" }
                            TableHeader { sortable: true, "Updated" }
                        }
                    }
                    TableBody {
                        TicketRow {
                            id: "1",
                            number: "TKT-1234",
                            title: "Email server not responding",
                            company: "Acme Corp",
                            status: "Open",
                            priority: "High",
                            assigned_to: "John Smith",
                            updated: "5 min ago",
                        }
                        TicketRow {
                            id: "2",
                            number: "TKT-1233",
                            title: "New user setup request",
                            company: "TechStart Inc",
                            status: "In Progress",
                            priority: "Medium",
                            assigned_to: "Jane Doe",
                            updated: "1 hour ago",
                        }
                        TicketRow {
                            id: "3",
                            number: "TKT-1232",
                            title: "Printer configuration for new office",
                            company: "Global Widgets",
                            status: "Pending",
                            priority: "Low",
                            assigned_to: "Unassigned",
                            updated: "2 hours ago",
                        }
                        TicketRow {
                            id: "4",
                            number: "TKT-1231",
                            title: "VPN connection issues for remote workers",
                            company: "Acme Corp",
                            status: "Open",
                            priority: "Critical",
                            assigned_to: "John Smith",
                            updated: "3 hours ago",
                        }
                        TicketRow {
                            id: "5",
                            number: "TKT-1230",
                            title: "Software license renewal required",
                            company: "TechStart Inc",
                            status: "Resolved",
                            priority: "Medium",
                            assigned_to: "Jane Doe",
                            updated: "1 day ago",
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketRowProps {
    id: String,
    number: String,
    title: String,
    company: String,
    status: String,
    priority: String,
    assigned_to: String,
    updated: String,
}

#[component]
fn TicketRow(props: TicketRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Open" => BadgeVariant::Blue,
        "In Progress" => BadgeVariant::Yellow,
        "Pending" => BadgeVariant::Gray,
        "Resolved" => BadgeVariant::Green,
        "Closed" => BadgeVariant::Gray,
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
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::TicketDetail { id: id.clone() }); },
            TableCell {
                div {
                    Link {
                        to: Route::TicketDetail { id: props.id.clone() },
                        class: "font-medium text-blue-600 hover:text-blue-500",
                        "{props.number}"
                    }
                    p { class: "text-gray-500 text-sm truncate max-w-xs", "{props.title}" }
                }
            }
            TableCell { "{props.company}" }
            TableCell {
                Badge { variant: status_variant, "{props.status}" }
            }
            TableCell {
                Badge { variant: priority_variant, "{props.priority}" }
            }
            TableCell {
                if props.assigned_to == "Unassigned" {
                    span { class: "text-gray-400 italic", "Unassigned" }
                } else {
                    span { "{props.assigned_to}" }
                }
            }
            TableCell { class: "text-gray-500",
                "{props.updated}"
            }
        }
    }
}

/// New ticket page
#[component]
pub fn TicketNewPage() -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut company = use_signal(String::new);
    let mut priority = use_signal(|| "medium".to_string());
    let mut is_submitting = use_signal(|| false);

    let company_options = vec![
        SelectOption::new("1", "Acme Corp"),
        SelectOption::new("2", "TechStart Inc"),
        SelectOption::new("3", "Global Widgets"),
    ];

    let priority_options = vec![
        SelectOption::new("critical", "Critical"),
        SelectOption::new("high", "High"),
        SelectOption::new("medium", "Medium"),
        SelectOption::new("low", "Low"),
    ];

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);

        // Snapshot signals so the spawn doesn't need to read them.
        let title_v = title.read().clone();
        let description_v = description.read().clone();
        let company_v = company.read().clone();
        let _priority_v = priority.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // The Select still ships hardcoded "1"/"2"/"3" placeholders
                // until the company dropdown is wired to /api/v1/contacts/companies
                // (tracked under the contacts story). Parse as Uuid; non-UUID
                // values fall back to `nil()` so the POST exercises the wire
                // and the server returns a typed validation error we can
                // surface to the user via the toast.
                let company_id = uuid::Uuid::parse_str(&company_v).unwrap_or_else(|_| uuid::Uuid::nil());
                let body = serde_json::json!({
                    "title": title_v,
                    "description": if description_v.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(description_v) },
                    "company_id": company_id,
                });

                #[derive(serde::Deserialize)]
                struct CreatedTicket { id: uuid::Uuid }

                match crate::hooks::fetch::api::post_authed::<CreatedTicket, _>("/tickets", &body).await {
                    Ok(created) => {
                        navigator.push(Route::TicketDetail { id: created.id.to_string() });
                    }
                    Err(err) => {
                        // The toast surface lands with the API client
                        // story; until then surface the failure via the
                        // browser console and keep the form mounted so
                        // the user can retry without losing their text.
                        web_sys::console::error_1(
                            &format!("Could not create ticket: {err}").into(),
                        );
                    }
                }
            }

            is_submitting.set(false);
        });
    };

    rsx! {
        AppLayout { title: "New Ticket",
            PageHeader {
                title: "New Ticket",
                subtitle: "Create a new support ticket",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: handle_submit,

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Input {
                            name: "title",
                            label: "Title",
                            placeholder: "Brief description of the issue",
                            required: true,
                            value: title.read().clone(),
                            oninput: move |e: FormEvent| title.set(e.value()),
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
                    }

                    Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Provide detailed information about the issue...",
                        rows: 6,
                        required: true,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "priority",
                            label: "Priority",
                            options: priority_options,
                            value: priority.read().clone(),
                            onchange: move |e: FormEvent| priority.set(e.value()),
                        }
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::TicketList {},
                            Button {
                                variant: ButtonVariant::Secondary,
                                "Cancel"
                            }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Create Ticket"
                        }
                    }
                }
            }
        }
    }
}

/// Ticket detail page
#[derive(Props, Clone, PartialEq)]
pub struct TicketDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn TicketDetailPage(props: TicketDetailPageProps) -> Element {
    let mut show_note_modal = use_signal(|| false);
    let mut note_type = use_signal(|| "internal".to_string());
    let mut note_content = use_signal(String::new);
    let mut note_submitting = use_signal(|| false);
    let header_title = format!("Ticket {}", props.id);
    let ticket_id_for_note = props.id.clone();

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| show_note_modal.set(true),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Note"
                    }
                    Link {
                        to: Route::TimeEntryNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            ClockIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Log Time"
                        }
                    }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Description
                    Card { title: "Description",
                        p { class: "text-gray-700 dark:text-gray-300",
                            "Users are reporting that they cannot send or receive emails. "
                            "The Exchange server appears to be unresponsive. "
                            "This started occurring around 9:00 AM today."
                        }
                    }

                    // Activity timeline
                    Card { title: "Activity",
                        div { class: "flow-root",
                            ul { class: "-mb-8",
                                TimelineItem {
                                    user: "John Smith",
                                    action: "added a note",
                                    time: "10 minutes ago",
                                    content: Some("Restarted the Exchange services, monitoring for stability.".to_string()),
                                    is_last: false,
                                }
                                TimelineItem {
                                    user: "System",
                                    action: "changed status to In Progress",
                                    time: "30 minutes ago",
                                    content: None,
                                    is_last: false,
                                }
                                TimelineItem {
                                    user: "Jane Doe",
                                    action: "assigned ticket to John Smith",
                                    time: "1 hour ago",
                                    content: None,
                                    is_last: false,
                                }
                                TimelineItem {
                                    user: "Customer Portal",
                                    action: "created ticket",
                                    time: "2 hours ago",
                                    content: None,
                                    is_last: true,
                                }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    // Status card
                    Card { title: "Details",
                        dl { class: "space-y-4",
                            DetailItem { label: "Status", value: rsx!(Badge { variant: BadgeVariant::Blue, "Open" }) }
                            DetailItem { label: "Priority", value: rsx!(Badge { variant: BadgeVariant::Red, "High" }) }
                            DetailItem { label: "Assigned To", value: rsx!(span { "John Smith" }) }
                            DetailItem { label: "Company", value: rsx!(
                                Link { to: Route::CompanyDetail { id: "1".to_string() }, class: "text-blue-600 hover:text-blue-500", "Acme Corp" }
                            ) }
                            DetailItem { label: "Contact", value: rsx!(span { "Bob Johnson" }) }
                            DetailItem { label: "Queue", value: rsx!(span { "Support" }) }
                            DetailItem { label: "Created", value: rsx!(span { "Jan 15, 2025 9:15 AM" }) }
                            DetailItem { label: "SLA Due", value: rsx!(
                                span { class: "text-yellow-600 font-medium", "45 minutes remaining" }
                            ) }
                        }
                    }

                    // Time entries
                    Card { title: "Time Logged",
                        div { class: "space-y-3",
                            div { class: "flex justify-between items-center",
                                span { class: "text-sm text-gray-500", "Total Time" }
                                span { class: "text-lg font-semibold", "1.5 hours" }
                            }
                            div { class: "text-sm text-gray-500",
                                p { "John Smith - 1.0h" }
                                p { "Jane Doe - 0.5h" }
                            }
                        }
                    }

                    // Related items
                    Card { title: "Related",
                        div { class: "space-y-2 text-sm",
                            p {
                                span { class: "text-gray-500", "Contract: " }
                                Link { to: Route::ContractDetail { id: "1".to_string() }, class: "text-blue-600 hover:text-blue-500",
                                    "Managed Services Agreement"
                                }
                            }
                            p {
                                span { class: "text-gray-500", "Asset: " }
                                Link { to: Route::AssetDetail { id: "1".to_string() }, class: "text-blue-600 hover:text-blue-500",
                                    "Exchange Server 01"
                                }
                            }
                        }
                    }
                }
            }

            // Add note modal
            Modal {
                open: *show_note_modal.read(),
                title: "Add Note",
                size: crate::components::ModalSize::Medium,
                onclose: move |_| show_note_modal.set(false),
                footer: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| show_note_modal.set(false),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *note_submitting.read(),
                        onclick: move |_| {
                            note_submitting.set(true);
                            let id = ticket_id_for_note.clone();
                            let type_v = note_type.read().clone();
                            let content_v = note_content.read().clone();
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    if content_v.trim().is_empty() {
                                        web_sys::console::warn_1(&"Note content empty; not posting.".into());
                                        note_submitting.set(false);
                                        return;
                                    }
                                    let body = serde_json::json!({
                                        "note_type": type_v,
                                        "content": content_v,
                                    });
                                    let path = format!("/tickets/{id}/notes");
                                    match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(&path, &body).await {
                                        Ok(_) => {
                                            note_content.set(String::new());
                                            show_note_modal.set(false);
                                        }
                                        Err(err) => {
                                            web_sys::console::error_1(
                                                &format!("Could not add note: {err}").into(),
                                            );
                                        }
                                    }
                                }
                                note_submitting.set(false);
                            });
                        },
                        "Add Note"
                    }
                },
                div { class: "space-y-4",
                    Select {
                        name: "note_type",
                        label: "Note Type",
                        options: vec![
                            SelectOption::new("internal", "Internal Note"),
                            SelectOption::new("public", "Public Note (visible to customer)"),
                        ],
                        value: note_type.read().clone(),
                        onchange: move |e: FormEvent| note_type.set(e.value()),
                    }
                    Textarea {
                        name: "content",
                        label: "Content",
                        placeholder: "Enter your note...",
                        rows: 4,
                        value: note_content.read().clone(),
                        oninput: move |e: FormEvent| note_content.set(e.value()),
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DetailItemProps {
    label: String,
    value: Element,
}

#[component]
fn DetailItem(props: DetailItemProps) -> Element {
    rsx! {
        div { class: "flex justify-between",
            dt { class: "text-sm text-gray-500 dark:text-gray-400", "{props.label}" }
            dd { class: "text-sm text-gray-900 dark:text-white", {props.value} }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimelineItemProps {
    user: String,
    action: String,
    time: String,
    content: Option<String>,
    is_last: bool,
}

#[component]
fn TimelineItem(props: TimelineItemProps) -> Element {
    rsx! {
        li {
            div { class: "relative pb-8",
                if !props.is_last {
                    span {
                        class: "absolute left-4 top-4 -ml-px h-full w-0.5 bg-gray-200 dark:bg-gray-700",
                        aria_hidden: "true",
                    }
                }
                div { class: "relative flex space-x-3",
                    div {
                        span { class: "h-8 w-8 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center ring-8 ring-white dark:ring-gray-800",
                            UserCircleIcon { size: IconSize::Small, class: "text-blue-600 dark:text-blue-400".to_string() }
                        }
                    }
                    div { class: "flex min-w-0 flex-1 justify-between space-x-4 pt-1.5",
                        div {
                            p { class: "text-sm text-gray-500 dark:text-gray-400",
                                span { class: "font-medium text-gray-900 dark:text-white", "{props.user}" }
                                " {props.action}"
                            }
                            if let Some(content) = &props.content {
                                div { class: "mt-2 text-sm text-gray-700 dark:text-gray-300 bg-gray-50 dark:bg-gray-800 rounded-md p-3",
                                    "{content}"
                                }
                            }
                        }
                        div { class: "whitespace-nowrap text-right text-sm text-gray-500 dark:text-gray-400",
                            "{props.time}"
                        }
                    }
                }
            }
        }
    }
}

//! Client portal pages

use dioxus::prelude::*;

use crate::components::{
    Badge, BadgeVariant, BookIcon, Button, ButtonVariant, Card, CurrencyIcon, IconSize, PlusIcon,
    PortalLayout, SearchInput, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// Portal home page
#[component]
pub fn PortalHomePage() -> Element {
    rsx! {
        PortalLayout { title: "Home",
            // Welcome section
            div { class: "mb-8",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-white",
                    "Welcome back, Bob"
                }
                p { class: "text-gray-500 dark:text-gray-400 mt-1",
                    "Here's what's happening with your account."
                }
            }

            // Quick stats
            div { class: "grid grid-cols-1 md:grid-cols-3 gap-6 mb-8",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500", "Open Tickets" }
                    p { class: "text-3xl font-bold text-blue-600", "3" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500", "Pending Invoices" }
                    p { class: "text-3xl font-bold text-yellow-600", "1" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500", "Outstanding Balance" }
                    p { class: "text-3xl font-bold text-gray-900 dark:text-white", "$2,500" }
                }
            }

            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                // Recent tickets
                Card {
                    title: "Recent Tickets",
                    actions: rsx! {
                        Link {
                            to: Route::PortalTicketList {},
                            class: "text-sm text-blue-600 hover:text-blue-500",
                            "View all"
                        }
                    },
                    div { class: "space-y-3",
                        PortalTicketItem {
                            number: "TKT-1234",
                            title: "Email server not responding",
                            status: "In Progress",
                            updated: "5 min ago",
                        }
                        PortalTicketItem {
                            number: "TKT-1231",
                            title: "VPN connection issues",
                            status: "Open",
                            updated: "3 hours ago",
                        }
                        PortalTicketItem {
                            number: "TKT-1228",
                            title: "New user setup request",
                            status: "Pending",
                            updated: "1 day ago",
                        }
                    }
                }

                // Recent invoices
                Card {
                    title: "Recent Invoices",
                    actions: rsx! {
                        Link {
                            to: Route::PortalInvoiceList {},
                            class: "text-sm text-blue-600 hover:text-blue-500",
                            "View all"
                        }
                    },
                    div { class: "space-y-3",
                        PortalInvoiceItem {
                            number: "INV-2025-001",
                            date: "Jan 1, 2025",
                            amount: "$2,500.00",
                            status: "Pending",
                        }
                        PortalInvoiceItem {
                            number: "INV-2024-012",
                            date: "Dec 1, 2024",
                            amount: "$2,500.00",
                            status: "Paid",
                        }
                    }
                }
            }

            // Quick actions
            div { class: "mt-8",
                h2 { class: "text-lg font-medium text-gray-900 dark:text-white mb-4",
                    "Quick Actions"
                }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                    Link {
                        to: Route::PortalTicketNew {},
                        class: "flex items-center p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg hover:bg-blue-100 dark:hover:bg-blue-900/40 transition-colors",
                        PlusIcon { class: "h-6 w-6 text-blue-600 mr-3".to_string() }
                        span { class: "font-medium text-blue-900 dark:text-blue-100", "Submit New Ticket" }
                    }
                    Link {
                        to: Route::PortalKB {},
                        class: "flex items-center p-4 bg-green-50 dark:bg-green-900/20 rounded-lg hover:bg-green-100 dark:hover:bg-green-900/40 transition-colors",
                        BookIcon { class: "h-6 w-6 text-green-600 mr-3".to_string() }
                        span { class: "font-medium text-green-900 dark:text-green-100", "Browse Knowledge Base" }
                    }
                    Link {
                        to: Route::PortalInvoiceList {},
                        class: "flex items-center p-4 bg-purple-50 dark:bg-purple-900/20 rounded-lg hover:bg-purple-100 dark:hover:bg-purple-900/40 transition-colors",
                        CurrencyIcon { class: "h-6 w-6 text-purple-600 mr-3".to_string() }
                        span { class: "font-medium text-purple-900 dark:text-purple-100", "Pay Invoice" }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalTicketItemProps {
    number: String,
    title: String,
    status: String,
    updated: String,
}

#[component]
fn PortalTicketItem(props: PortalTicketItemProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Open" => BadgeVariant::Blue,
        "In Progress" => BadgeVariant::Yellow,
        "Pending" => BadgeVariant::Gray,
        "Resolved" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    rsx! {
        div { class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div {
                div { class: "flex items-center",
                    span { class: "font-medium text-blue-600", "{props.number}" }
                    Badge { variant: status_variant, class: "ml-2", "{props.status}" }
                }
                p { class: "text-sm text-gray-600 dark:text-gray-400 mt-1", "{props.title}" }
            }
            span { class: "text-xs text-gray-400", "{props.updated}" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalInvoiceItemProps {
    number: String,
    date: String,
    amount: String,
    status: String,
}

#[component]
fn PortalInvoiceItem(props: PortalInvoiceItemProps) -> Element {
    let status_variant = if props.status == "Paid" {
        BadgeVariant::Green
    } else {
        BadgeVariant::Yellow
    };

    rsx! {
        div { class: "flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div {
                span { class: "font-medium text-gray-900 dark:text-white", "{props.number}" }
                p { class: "text-sm text-gray-500", "{props.date}" }
            }
            div { class: "text-right",
                span { class: "font-medium text-gray-900 dark:text-white", "{props.amount}" }
                div { class: "mt-1",
                    Badge { variant: status_variant, "{props.status}" }
                }
            }
        }
    }
}

/// Portal ticket list page
#[component]
pub fn PortalTicketListPage() -> Element {
    rsx! {
        // Title is rendered once below alongside the "New Ticket"
        // action button (P1-10 dedup).
        PortalLayout {
            div { class: "flex items-center justify-between mb-6",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-white", "My Tickets" }
                Link {
                    to: Route::PortalTicketNew {},
                    Button {
                        variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Ticket"
                    }
                }
            }

            Card { padding: false,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Ticket" }
                            TableHeader { "Status" }
                            TableHeader { "Updated" }
                        }
                    }
                    TableBody {
                        PortalTicketRow {
                            id: "1234",
                            number: "TKT-1234",
                            subject: "Email server not responding",
                            status: "In Progress",
                            updated: "5 min ago",
                        }
                        PortalTicketRow {
                            id: "1231",
                            number: "TKT-1231",
                            subject: "VPN connection issues",
                            status: "Open",
                            updated: "3 hours ago",
                        }
                        PortalTicketRow {
                            id: "1228",
                            number: "TKT-1228",
                            subject: "New user setup request",
                            status: "Pending",
                            updated: "1 day ago",
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalTicketRowProps {
    id: String,
    number: String,
    subject: String,
    status: String,
    updated: String,
}

#[component]
fn PortalTicketRow(props: PortalTicketRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Open" => BadgeVariant::Blue,
        "In Progress" => BadgeVariant::Yellow,
        "Pending" => BadgeVariant::Gray,
        "Resolved" => BadgeVariant::Green,
        "Closed" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    };
    let navigator = use_navigator();
    let id_for_click = props.id.clone();
    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| {
                navigator.push(Route::PortalTicketDetail { id: id_for_click.clone() });
            },
            TableCell {
                div {
                    Link {
                        to: Route::PortalTicketDetail { id: props.id.clone() },
                        class: "font-medium text-blue-600 hover:text-blue-500",
                        "{props.number}"
                    }
                    p { class: "text-sm text-gray-500", "{props.subject}" }
                }
            }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-gray-500", "{props.updated}" }
        }
    }
}

/// Portal new ticket page
#[component]
pub fn PortalTicketNewPage() -> Element {
    let mut subject = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut priority = use_signal(|| "medium".to_string());
    let mut is_submitting = use_signal(|| false);

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);

        // Snapshot signals so the spawn doesn't borrow them across await.
        let subject_v = subject.read().clone();
        let description_v = description.read().clone();
        let priority_v = priority.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                if subject_v.trim().is_empty() {
                    web_sys::console::warn_1(&"Subject empty; not submitting.".into());
                    is_submitting.set(false);
                    return;
                }
                // Wire to the expected portal-scoped ticket-create path.
                // TODO(portal-api): the server-side portal tickets
                // endpoint (server F6) may not be deployed yet. Until it
                // is, this POST exercises the wire and surfaces the
                // server's response; we intentionally do NOT fake a
                // success. The contact/company are resolved server-side
                // from the portal session, so the body carries only the
                // fields the portal user controls.
                let body = serde_json::json!({
                    "subject": subject_v,
                    "description": if description_v.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(description_v)
                    },
                    "priority": priority_v,
                });

                #[derive(serde::Deserialize)]
                struct CreatedPortalTicket {
                    id: String,
                }

                match crate::hooks::fetch::api::post_authed::<CreatedPortalTicket, _>(
                    "/portal/tickets",
                    &body,
                )
                .await
                {
                    Ok(created) => {
                        navigator.push(Route::PortalTicketDetail { id: created.id });
                    }
                    Err(err) => {
                        // Keep the form mounted so the user can retry
                        // without losing their text; surface the failure
                        // via the console until the portal toast surface
                        // lands.
                        web_sys::console::error_1(
                            &format!("Could not submit ticket: {err}").into(),
                        );
                    }
                }
            }

            is_submitting.set(false);
        });
    };

    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-6", "Submit a Ticket" }

            Card {
                form {
                    class: "space-y-6",
                    // F2: real submit handler. POSTs to the portal tickets
                    // endpoint; prevent_default stops the browser's GET
                    // default-submit from leaking fields into the URL.
                    onsubmit: handle_submit,

                    crate::components::Input {
                        name: "subject",
                        label: "Subject",
                        placeholder: "Brief description of your issue",
                        required: true,
                        value: subject.read().clone(),
                        oninput: move |e: FormEvent| subject.set(e.value()),
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Please provide as much detail as possible...",
                        rows: 6,
                        required: true,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    crate::components::Select {
                        name: "priority",
                        label: "Priority",
                        options: vec![
                            crate::components::SelectOption::new("low", "Low - General question or minor issue"),
                            crate::components::SelectOption::new("medium", "Medium - Issue affecting work but has workaround"),
                            crate::components::SelectOption::new("high", "High - Significant impact, no workaround"),
                            crate::components::SelectOption::new("critical", "Critical - Complete outage or data loss"),
                        ],
                        value: priority.read().clone(),
                        onchange: move |e: FormEvent| priority.set(e.value()),
                    }

                    // Real file input (PMC-98). The previous drop-zone
                    // was decorative - no <input type=file> meant the
                    // click target did nothing and dropping a file did
                    // nothing. A native multi-file input is the cheapest
                    // honest thing here; styled drag-and-drop with
                    // upload progress can land alongside the portal
                    // attachments endpoint.
                    div {
                        label {
                            r#for: "attachments",
                            class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Attachments"
                        }
                        input {
                            id: "attachments",
                            name: "attachments",
                            r#type: "file",
                            multiple: true,
                            class: "block w-full text-sm text-gray-700 dark:text-gray-300 file:mr-3 file:py-2 file:px-3 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-blue-50 file:text-blue-700 dark:file:bg-blue-900 dark:file:text-blue-200 hover:file:bg-blue-100",
                        }
                        p { class: "mt-1 text-xs text-gray-500", "Up to 10MB per file. Uploads activate once the portal attachments endpoint ships." }
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::PortalTicketList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Submit Ticket"
                        }
                    }
                }
            }
        }
    }
}

/// Portal ticket detail page
#[derive(Props, Clone, PartialEq)]
pub struct PortalTicketDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn PortalTicketDetailPage(props: PortalTicketDetailPageProps) -> Element {
    let header_title = format!("Ticket {}", props.id);

    let mut reply = use_signal(String::new);
    let mut reply_submitting = use_signal(|| false);
    let ticket_id_for_reply = props.id.clone();

    let send_reply = move |_| {
        reply_submitting.set(true);
        let id = ticket_id_for_reply.clone();
        let reply_v = reply.read().clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                if reply_v.trim().is_empty() {
                    web_sys::console::warn_1(&"Reply empty; not sending.".into());
                    reply_submitting.set(false);
                    return;
                }
                // Wire to the expected portal-scoped reply path.
                // TODO(portal-api): the portal ticket-reply endpoint
                // (server F6) may not be live yet. We POST to the
                // expected path and surface any failure rather than
                // faking a success or silently dropping the reply.
                let body = serde_json::json!({ "content": reply_v });
                let path = format!("/portal/tickets/{id}/replies");
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => {
                        reply.set(String::new());
                    }
                    Err(err) => {
                        web_sys::console::error_1(&format!("Could not send reply: {err}").into());
                    }
                }
            }
            reply_submitting.set(false);
        });
    };

    rsx! {
        PortalLayout { title: "{header_title}",
            div { class: "mb-6",
                Link {
                    to: Route::PortalTicketList {},
                    class: "text-sm text-blue-600 hover:text-blue-500",
                    "Back to tickets"
                }
            }

            Card {
                div { class: "flex items-start justify-between mb-6",
                    div {
                        h1 { class: "text-xl font-bold text-gray-900 dark:text-white",
                            "{header_title}"
                        }
                        div { class: "flex items-center mt-2 space-x-4",
                            Badge { variant: BadgeVariant::Yellow, "In Progress" }
                            span { class: "text-sm text-gray-500", "Created: Jan 15, 2025" }
                        }
                    }
                }

                div { class: "prose dark:prose-invert max-w-none mb-6",
                    p {
                        "Users are reporting that they cannot send or receive emails. "
                        "The issue started around 9:00 AM this morning."
                    }
                }

                // Updates
                h3 { class: "font-medium text-gray-900 dark:text-white mb-4", "Updates" }
                div { class: "space-y-4",
                    UpdateItem {
                        author: "Support Team",
                        time: "10 min ago",
                        content: "We've identified the issue and are working on a fix. The Exchange services have been restarted and we're monitoring for stability.",
                        is_staff: true,
                    }
                    UpdateItem {
                        author: "You",
                        time: "2 hours ago",
                        content: "Users are still unable to send emails. This is affecting the entire office.",
                        is_staff: false,
                    }
                }

                // Reply form (F2: real signal-bound textarea + POST).
                div { class: "mt-6 pt-6 border-t border-gray-200 dark:border-gray-700",
                    h4 { class: "font-medium text-gray-900 dark:text-white mb-3", "Add Reply" }
                    crate::components::Textarea {
                        name: "reply",
                        placeholder: "Type your reply...",
                        rows: 3,
                        value: reply.read().clone(),
                        oninput: move |e: FormEvent| reply.set(e.value()),
                    }
                    div { class: "mt-3 flex justify-end",
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: *reply_submitting.read(),
                            onclick: send_reply,
                            "Send Reply"
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct UpdateItemProps {
    author: String,
    time: String,
    content: String,
    is_staff: bool,
}

#[component]
fn UpdateItem(props: UpdateItemProps) -> Element {
    let bg_class = if props.is_staff {
        "bg-blue-50 dark:bg-blue-900/20"
    } else {
        "bg-gray-50 dark:bg-gray-800"
    };

    rsx! {
        div { class: "p-4 rounded-lg {bg_class}",
            div { class: "flex items-center justify-between mb-2",
                div { class: "flex items-center",
                    span { class: "font-medium text-gray-900 dark:text-white", "{props.author}" }
                    if props.is_staff {
                        Badge { variant: BadgeVariant::Blue, class: "ml-2", "Staff" }
                    }
                }
                span { class: "text-sm text-gray-500", "{props.time}" }
            }
            p { class: "text-gray-700 dark:text-gray-300", "{props.content}" }
        }
    }
}

/// Portal invoice list page
#[component]
pub fn PortalInvoiceListPage() -> Element {
    // F2: the "View" buttons navigate to the invoice detail route.
    // `navigator` is Copy, so each onclick closure captures its own
    // copy and pushes the route for that row's invoice number.
    let navigator = use_navigator();

    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-6", "Invoices" }

            Card { padding: false,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Invoice" }
                            TableHeader { "Date" }
                            TableHeader { "Amount" }
                            TableHeader { "Status" }
                            TableHeader { "" }
                        }
                    }
                    TableBody {
                        TableRow {
                            TableCell { class: "font-medium", "INV-2025-001" }
                            TableCell { "Jan 1, 2025" }
                            TableCell { class: "font-medium", "$2,500.00" }
                            TableCell { Badge { variant: BadgeVariant::Yellow, "Pending" } }
                            // Audit P1-07: "Pay Now" button was decorative
                            // (no onclick, no payment integration). Hidden
                            // until the portal payments flow ships.
                            TableCell {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| { navigator.push(Route::PortalInvoiceDetail { id: "INV-2025-001".to_string() }); },
                                    "View"
                                }
                            }
                        }
                        TableRow {
                            TableCell { class: "font-medium", "INV-2024-012" }
                            TableCell { "Dec 1, 2024" }
                            TableCell { class: "font-medium", "$2,500.00" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                            TableCell {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| { navigator.push(Route::PortalInvoiceDetail { id: "INV-2024-012".to_string() }); },
                                    "View"
                                }
                            }
                        }
                        TableRow {
                            TableCell { class: "font-medium", "INV-2024-011" }
                            TableCell { "Nov 1, 2024" }
                            TableCell { class: "font-medium", "$2,500.00" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                            TableCell {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| { navigator.push(Route::PortalInvoiceDetail { id: "INV-2024-011".to_string() }); },
                                    "View"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Portal invoice detail page
#[derive(Props, Clone, PartialEq)]
pub struct PortalInvoiceDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn PortalInvoiceDetailPage(props: PortalInvoiceDetailPageProps) -> Element {
    let header_title = format!("Invoice {}", props.id);
    rsx! {
        PortalLayout { title: "{header_title}",
            div { class: "mb-6",
                Link {
                    to: Route::PortalInvoiceList {},
                    class: "text-sm text-blue-600 hover:text-blue-500",
                    "Back to invoices"
                }
            }

            // Minimum-viable read-only invoice view (PMC-96). Once the
            // portal billing endpoint ships, replace the placeholder
            // bill-to / line items / payment status with real fetched
            // data. The shape mirrors InvoiceDetailPage for consistency.
            Card {
                div { class: "flex justify-between items-start mb-6",
                    div {
                        h2 { class: "text-2xl font-bold text-gray-900 dark:text-white",
                            "{header_title}"
                        }
                        p { class: "text-sm text-gray-500", "Issued Jan 5, 2025 - Due Feb 4, 2025" }
                    }
                    Badge { variant: BadgeVariant::Yellow, "Pending" }
                }

                div { class: "grid grid-cols-2 gap-6 mb-6",
                    div {
                        h3 { class: "text-xs font-medium text-gray-500 uppercase mb-1", "Bill To" }
                        p { class: "font-medium", "Acme Corp" }
                        p { class: "text-sm text-gray-600 dark:text-gray-400", "Bob Johnson" }
                        p { class: "text-sm text-gray-600 dark:text-gray-400", "bob@acme.com" }
                    }
                    div {
                        h3 { class: "text-xs font-medium text-gray-500 uppercase mb-1", "Amount Due" }
                        p { class: "text-3xl font-bold text-gray-900 dark:text-white", "$1,850.00" }
                    }
                }

                table { class: "min-w-full text-sm mb-6",
                    thead { class: "border-b border-gray-200 dark:border-gray-700",
                        tr {
                            th { class: "text-left py-2 text-gray-500", "Description" }
                            th { class: "text-right py-2 text-gray-500", "Qty" }
                            th { class: "text-right py-2 text-gray-500", "Unit Price" }
                            th { class: "text-right py-2 text-gray-500", "Total" }
                        }
                    }
                    tbody {
                        tr {
                            td { class: "py-3", "Managed Services - January 2025" }
                            td { class: "py-3 text-right", "1" }
                            td { class: "py-3 text-right", "$1,500.00" }
                            td { class: "py-3 text-right", "$1,500.00" }
                        }
                        tr {
                            td { class: "py-3", "Additional support hours" }
                            td { class: "py-3 text-right", "3.5" }
                            td { class: "py-3 text-right", "$100.00" }
                            td { class: "py-3 text-right", "$350.00" }
                        }
                    }
                }

                div { class: "border-t border-gray-200 dark:border-gray-700 pt-4 text-right",
                    p { class: "text-sm text-gray-500", "Total Due" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "$1,850.00" }
                }

                p { class: "mt-6 text-xs text-gray-500",
                    "Online payment activates once the portal billing endpoint ships."
                }
            }
        }
    }
}

/// Portal knowledge base page
#[component]
pub fn PortalKBPage() -> Element {
    let mut search = use_signal(String::new);

    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-6", "Knowledge Base" }

            Card { class: "mb-6",
                SearchInput {
                    value: search.read().clone(),
                    placeholder: "Search articles...",
                    oninput: move |e: FormEvent| search.set(e.value()),
                }
            }

            // Popular articles
            Card { title: "Popular Articles",
                div { class: "space-y-3",
                    PortalArticleItem {
                        id: "portal-reset-password",
                        title: "How to Reset Your Password",
                        category: "Getting Started",
                    }
                    PortalArticleItem {
                        id: "portal-vpn-troubleshooting",
                        title: "Troubleshooting VPN Connection Issues",
                        category: "Troubleshooting",
                    }
                    PortalArticleItem {
                        id: "portal-mfa-setup",
                        title: "Setting Up Multi-Factor Authentication",
                        category: "Security",
                    }
                    PortalArticleItem {
                        id: "portal-email-mobile",
                        title: "Email Setup Guide for Mobile Devices",
                        category: "How-To Guides",
                    }
                    PortalArticleItem {
                        id: "portal-support-best-practices",
                        title: "Requesting IT Support - Best Practices",
                        category: "Getting Started",
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalArticleItemProps {
    id: String,
    title: String,
    category: String,
}

#[component]
fn PortalArticleItem(props: PortalArticleItemProps) -> Element {
    rsx! {
        Link {
            to: Route::KBArticleDetail { id: props.id.clone() },
            class: "block p-3 -mx-3 hover:bg-gray-50 dark:hover:bg-gray-800 rounded-lg transition-colors",
            h4 { class: "font-medium text-gray-900 dark:text-white", "{props.title}" }
            p { class: "text-sm text-gray-500 mt-1", "{props.category}" }
        }
    }
}

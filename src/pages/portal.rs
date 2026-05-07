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
                        TableRow {
                            TableCell {
                                div {
                                    span { class: "font-medium text-blue-600", "TKT-1234" }
                                    p { class: "text-sm text-gray-500", "Email server not responding" }
                                }
                            }
                            TableCell { Badge { variant: BadgeVariant::Yellow, "In Progress" } }
                            TableCell { class: "text-gray-500", "5 min ago" }
                        }
                        TableRow {
                            TableCell {
                                div {
                                    span { class: "font-medium text-blue-600", "TKT-1231" }
                                    p { class: "text-sm text-gray-500", "VPN connection issues" }
                                }
                            }
                            TableCell { Badge { variant: BadgeVariant::Blue, "Open" } }
                            TableCell { class: "text-gray-500", "3 hours ago" }
                        }
                        TableRow {
                            TableCell {
                                div {
                                    span { class: "font-medium text-blue-600", "TKT-1228" }
                                    p { class: "text-sm text-gray-500", "New user setup request" }
                                }
                            }
                            TableCell { Badge { variant: BadgeVariant::Gray, "Pending" } }
                            TableCell { class: "text-gray-500", "1 day ago" }
                        }
                    }
                }
            }
        }
    }
}

/// Portal new ticket page
#[component]
pub fn PortalTicketNewPage() -> Element {
    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-6", "Submit a Ticket" }

            Card {
                form {
                    class: "space-y-6",
                    // Without an explicit handler the browser default-submits
                    // as GET, leaking subject/description/priority into the
                    // URL and blanking the SPA. Stop that until a real
                    // /portal/tickets POST endpoint exists (server F6).
                    onsubmit: move |e: FormEvent| { e.prevent_default(); },

                    crate::components::Input {
                        name: "subject",
                        label: "Subject",
                        placeholder: "Brief description of your issue",
                        required: true,
                        oninput: |_| {},
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "Please provide as much detail as possible...",
                        rows: 6,
                        required: true,
                        oninput: |_| {},
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
                        value: "medium".to_string(),
                        onchange: |_| {},
                    }

                    // File upload placeholder
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Attachments"
                        }
                        div { class: "mt-1 flex justify-center px-6 pt-5 pb-6 border-2 border-gray-300 dark:border-gray-600 border-dashed rounded-md",
                            div { class: "space-y-1 text-center",
                                p { class: "text-sm text-gray-600 dark:text-gray-400",
                                    "Drag and drop files here, or click to select"
                                }
                                p { class: "text-xs text-gray-500", "Up to 10MB per file" }
                            }
                        }
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::PortalTicketList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
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
    rsx! {
        PortalLayout { title: "Ticket Detail",
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
                            "TKT-1234: Email server not responding"
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

                // Reply form
                div { class: "mt-6 pt-6 border-t border-gray-200 dark:border-gray-700",
                    h4 { class: "font-medium text-gray-900 dark:text-white mb-3", "Add Reply" }
                    crate::components::Textarea {
                        name: "reply",
                        placeholder: "Type your reply...",
                        rows: 3,
                        oninput: |_| {},
                    }
                    div { class: "mt-3 flex justify-end",
                        Button { variant: ButtonVariant::Primary, "Send Reply" }
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
                            TableCell { "" }
                        }
                        TableRow {
                            TableCell { class: "font-medium", "INV-2024-012" }
                            TableCell { "Dec 1, 2024" }
                            TableCell { class: "font-medium", "$2,500.00" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                            TableCell {
                                Button { variant: ButtonVariant::Secondary, "View" }
                            }
                        }
                        TableRow {
                            TableCell { class: "font-medium", "INV-2024-011" }
                            TableCell { "Nov 1, 2024" }
                            TableCell { class: "font-medium", "$2,500.00" }
                            TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                            TableCell {
                                Button { variant: ButtonVariant::Secondary, "View" }
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
    rsx! {
        PortalLayout { title: "Invoice Detail",
            div { class: "mb-6",
                Link {
                    to: Route::PortalInvoiceList {},
                    class: "text-sm text-blue-600 hover:text-blue-500",
                    "Back to invoices"
                }
            }

            Card {
                // Invoice content would go here - similar to main invoice detail
                p { class: "text-gray-500", "Invoice details would be displayed here." }
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
                        title: "How to Reset Your Password",
                        category: "Getting Started",
                    }
                    PortalArticleItem {
                        title: "Troubleshooting VPN Connection Issues",
                        category: "Troubleshooting",
                    }
                    PortalArticleItem {
                        title: "Setting Up Multi-Factor Authentication",
                        category: "Security",
                    }
                    PortalArticleItem {
                        title: "Email Setup Guide for Mobile Devices",
                        category: "How-To Guides",
                    }
                    PortalArticleItem {
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
    title: String,
    category: String,
}

#[component]
fn PortalArticleItem(props: PortalArticleItemProps) -> Element {
    rsx! {
        a {
            class: "block p-3 -mx-3 hover:bg-gray-50 dark:hover:bg-gray-800 rounded-lg transition-colors",
            h4 { class: "font-medium text-gray-900 dark:text-white", "{props.title}" }
            p { class: "text-sm text-gray-500 mt-1", "{props.category}" }
        }
    }
}

//! Client portal pages

use dioxus::prelude::*;

use crate::components::{
    invoice_status_badge, ticket_status_badge, Badge, BadgeVariant, BookIcon, Button,
    ButtonVariant, Card, CurrencyIcon, IconSize, PlusIcon, PortalLayout, SearchInput, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// Portal home page
#[component]
pub fn PortalHomePage() -> Element {
    rsx! {
        PortalLayout { title: "Home",
            // Welcome section
            div { class: "mb-8",
                h1 { class: "text-2xl font-bold text-content",
                    "Welcome back, Bob"
                }
                p { class: "text-muted mt-1",
                    "Here's what's happening with your account."
                }
            }

            // Quick stats
            div { class: "grid grid-cols-1 md:grid-cols-3 gap-6 mb-8",
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "Open Tickets" }
                    p { class: "text-3xl font-bold text-accent", "3" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "Pending Invoices" }
                    p { class: "text-3xl font-bold text-yellow-600", "1" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-muted", "Outstanding Balance" }
                    p { class: "text-3xl font-bold text-content", "$2,500" }
                }
            }

            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                // Recent tickets
                Card {
                    title: "Recent Tickets",
                    actions: rsx! {
                        Link {
                            to: Route::PortalTicketList {},
                            class: "text-sm text-accent hover:opacity-90",
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
                            class: "text-sm text-accent hover:opacity-90",
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
                h2 { class: "text-lg font-medium text-content mb-4",
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
    let status_variant = ticket_status_badge(&props.status);

    rsx! {
        div { class: "flex items-center justify-between p-3 bg-surface-2 rounded-lg",
            div {
                div { class: "flex items-center",
                    span { class: "font-medium text-accent", "{props.number}" }
                    Badge { variant: status_variant, class: "ml-2", "{props.status}" }
                }
                p { class: "text-sm text-muted mt-1", "{props.title}" }
            }
            span { class: "text-xs text-subtle", "{props.updated}" }
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
        div { class: "flex items-center justify-between p-3 bg-surface-2 rounded-lg",
            div {
                span { class: "font-medium text-content", "{props.number}" }
                p { class: "text-sm text-muted", "{props.date}" }
            }
            div { class: "text-right",
                span { class: "font-medium text-content", "{props.amount}" }
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
                h1 { class: "text-2xl font-bold text-content", "My Tickets" }
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
    let status_variant = ticket_status_badge(&props.status);
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
                        class: "font-medium text-accent hover:opacity-90",
                        "{props.number}"
                    }
                    p { class: "text-sm text-muted", "{props.subject}" }
                }
            }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-muted", "{props.updated}" }
        }
    }
}

/// Portal new ticket page
#[component]
pub fn PortalTicketNewPage() -> Element {
    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-content mb-6", "Submit a Ticket" }

            // Honest "coming soon" state. The previous form's inputs were
            // dead (no-op oninput handlers) and there is no `POST
            // /portal/tickets` endpoint to submit to, so the page never
            // created a ticket. Rather than present a form that silently
            // discards input, tell the user the flow is not available yet
            // and point them at a working channel.
            Card {
                div { class: "py-12 text-center",
                    h2 { class: "text-lg font-medium text-content mb-2",
                        "Coming soon"
                    }
                    p { class: "text-sm text-muted mb-6 max-w-md mx-auto",
                        "Submitting tickets from the portal is not available yet. In the meantime, please contact your account team to open a request."
                    }
                    Link {
                        to: Route::PortalTicketList {},
                        Button { variant: ButtonVariant::Secondary, "Back to tickets" }
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

/// Subset of the server portal ticket response (`GET
/// /api/v1/portal/tickets/{id}`) the detail page renders. Mirrors the
/// agent-side `RemoteTicketDetail` shape; serde drops unknown fields and
/// every field defaults so a thinner portal payload still decodes.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalTicketDetail {
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    status: PortalSummary,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A `{ id, name }`-style lookup value (status/priority). Only `name` is
/// rendered here.
#[derive(Clone, Debug, PartialEq, Default, serde::Deserialize)]
struct PortalSummary {
    #[serde(default)]
    name: String,
}

#[component]
pub fn PortalTicketDetailPage(props: PortalTicketDetailPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let ticket_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<PortalTicketDetail>(&format!(
                "/portal/tickets/{id}"
            ))
            .await
            .ok()
        }
    });

    let snap = ticket_resource.read_unchecked();
    let header_title = match &*snap {
        Some(Some(t)) if !t.ticket_number.is_empty() => format!("Ticket {}", t.ticket_number),
        _ => format!("Ticket {}", props.id),
    };

    rsx! {
        PortalLayout { title: "{header_title}",
            div { class: "mb-6",
                Link {
                    to: Route::PortalTicketList {},
                    class: "text-sm text-accent hover:opacity-90",
                    "Back to tickets"
                }
            }

            match &*snap {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load ticket." }
                            Link {
                                to: Route::PortalTicketList {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to tickets"
                            }
                        }
                    }
                },
                Some(Some(ticket)) => {
                    let status_label = if ticket.status.name.is_empty() {
                        "Unknown".to_string()
                    } else {
                        ticket.status.name.clone()
                    };
                    let created = ticket
                        .created_at
                        .map(|d| d.format("Created %b %-d, %Y").to_string())
                        .unwrap_or_default();
                    let subject = if ticket.title.is_empty() {
                        header_title.clone()
                    } else {
                        ticket.title.clone()
                    };
                    let description = ticket.description.clone().unwrap_or_default();
                    rsx! {
                        Card {
                            div { class: "flex items-start justify-between mb-6",
                                div {
                                    h1 { class: "text-xl font-bold text-content",
                                        "{subject}"
                                    }
                                    div { class: "flex items-center mt-2 space-x-4",
                                        Badge { variant: BadgeVariant::Yellow, "{status_label}" }
                                        if !created.is_empty() {
                                            span { class: "text-sm text-muted", "{created}" }
                                        }
                                    }
                                }
                            }

                            if description.is_empty() {
                                p { class: "text-sm text-muted", "No description provided." }
                            } else {
                                div { class: "prose dark:prose-invert max-w-none",
                                    p { "{description}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Portal invoice list page
#[component]
pub fn PortalInvoiceListPage() -> Element {
    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-content mb-6", "Invoices" }

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

/// Subset of the server portal invoice response (`GET
/// /api/v1/portal/invoices/{id}`, company-scoped per PMS-25) the detail
/// page renders. Mirrors the agent-side `InvoiceDetail`; amounts arrive
/// as strings and every field defaults so a thinner payload still decodes.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalInvoiceDetail {
    #[serde(default)]
    invoice_number: String,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    invoice_date: Option<String>,
    #[serde(default)]
    due_date: Option<String>,
    #[serde(default)]
    subtotal: String,
    #[serde(default)]
    total: String,
    #[serde(default)]
    balance_due: String,
    #[serde(default)]
    lines: Option<Vec<PortalInvoiceLine>>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalInvoiceLine {
    #[serde(default)]
    description: String,
    #[serde(default)]
    quantity: String,
    #[serde(default)]
    unit_price: String,
    #[serde(default)]
    total: String,
}

/// Render an amount string as currency, defaulting blanks to `$0.00`.
fn portal_money(raw: &str) -> String {
    if raw.trim().is_empty() {
        "$0.00".to_string()
    } else {
        format!("${raw}")
    }
}

#[component]
pub fn PortalInvoiceDetailPage(props: PortalInvoiceDetailPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let invoice_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<PortalInvoiceDetail>(&format!(
                "/portal/invoices/{id}"
            ))
            .await
            .ok()
        }
    });

    let snap = invoice_resource.read_unchecked();
    let header_title = match &*snap {
        Some(Some(inv)) if !inv.invoice_number.is_empty() => {
            format!("Invoice {}", inv.invoice_number)
        }
        _ => format!("Invoice {}", props.id),
    };

    rsx! {
        PortalLayout { title: "{header_title}",
            div { class: "mb-6",
                Link {
                    to: Route::PortalInvoiceList {},
                    class: "text-sm text-accent hover:opacity-90",
                    "Back to invoices"
                }
            }

            match &*snap {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load invoice." }
                            Link {
                                to: Route::PortalInvoiceList {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to invoices"
                            }
                        }
                    }
                },
                Some(Some(inv)) => {
                    let status_raw = if inv.status.is_empty() {
                        "pending"
                    } else {
                        inv.status.as_str()
                    };
                    let (status_variant, status_label) = invoice_status_badge(status_raw);
                    let issued = inv.invoice_date.clone().unwrap_or_default();
                    let due = inv.due_date.clone().unwrap_or_default();
                    let company_name = inv
                        .company_name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "-".to_string());
                    let amount_due = portal_money(&inv.balance_due);
                    let subtotal = portal_money(&inv.subtotal);
                    let total = portal_money(&inv.total);
                    let lines = inv.lines.clone().unwrap_or_default();
                    rsx! {
                        Card {
                            div { class: "flex justify-between items-start mb-6",
                                div {
                                    h2 { class: "text-2xl font-bold text-content",
                                        "{header_title}"
                                    }
                                    if !issued.is_empty() || !due.is_empty() {
                                        p { class: "text-sm text-muted", "Issued {issued} - Due {due}" }
                                    }
                                }
                                Badge { variant: status_variant, "{status_label}" }
                            }

                            div { class: "grid grid-cols-2 gap-6 mb-6",
                                div {
                                    h3 { class: "text-xs font-medium text-muted uppercase mb-1", "Bill To" }
                                    p { class: "font-medium", "{company_name}" }
                                }
                                div {
                                    h3 { class: "text-xs font-medium text-muted uppercase mb-1", "Amount Due" }
                                    p { class: "text-3xl font-bold text-content", "{amount_due}" }
                                }
                            }

                            table { class: "min-w-full text-sm mb-6",
                                thead { class: "border-b border-line",
                                    tr {
                                        th { class: "text-left py-2 text-muted", "Description" }
                                        th { class: "text-right py-2 text-muted", "Qty" }
                                        th { class: "text-right py-2 text-muted", "Unit Price" }
                                        th { class: "text-right py-2 text-muted", "Total" }
                                    }
                                }
                                tbody {
                                    if lines.is_empty() {
                                        tr {
                                            td { class: "py-3 text-muted", colspan: "4", "No line items." }
                                        }
                                    } else {
                                        for (idx, line) in lines.iter().enumerate() {
                                            tr { key: "{idx}",
                                                td { class: "py-3", "{line.description}" }
                                                td { class: "py-3 text-right", "{line.quantity}" }
                                                td { class: "py-3 text-right", {portal_money(&line.unit_price)} }
                                                td { class: "py-3 text-right", {portal_money(&line.total)} }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "border-t border-line pt-4 text-right space-y-1",
                                p { class: "text-sm text-muted", "Subtotal {subtotal}" }
                                p { class: "text-2xl font-bold text-content", "Total {total}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Server-side paginated envelope (`PaginatedResponse<KbArticleResponse>`)
/// for the portal KB feed. Only the fields the portal renders are pulled;
/// serde drops the rest.
#[derive(Clone, Debug, serde::Deserialize)]
struct PortalKbFeed {
    data: Vec<PortalKbArticle>,
}

/// `KbArticleResponse` subset for the portal reader. Mirrors the agent
/// `KbArticle` DTO but local to the portal page (the portal feed reuses
/// the same server response type).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalKbArticle {
    id: uuid::Uuid,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: Option<String>,
}

/// Portal knowledge base page.
///
/// Fetches `GET /api/v1/portal/kb`, the portal-scoped feed. Auth differs
/// from the agent KB pages: the server mounts this under a separate
/// `/api/v1/portal` router guarded by `portal_auth_middleware`, whose
/// identity is the authenticated *contacts* row (not a `users` row) and
/// which scopes results to published, portal-visible articles for the
/// caller's company. The SPA only holds the OIDC bearer token, so we send
/// it via `get_authed`; the portal middleware is the server-side gate.
/// (Other portal pages here are still demo and do not fetch yet, so there
/// is no portal-token helper to mirror; `get_authed` is the available
/// path.)
#[component]
pub fn PortalKBPage() -> Element {
    let mut search = use_signal(String::new);

    let feed_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<PortalKbFeed>("/portal/kb?page=1&per_page=100")
            .await
            .ok()
    });

    let snap = feed_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));

    let search_text = search.read().trim().to_lowercase();
    let articles: Vec<PortalKbArticle> = match &*snap {
        Some(Some(feed)) => feed
            .data
            .iter()
            .filter(|a| {
                search_text.is_empty()
                    || a.title.to_lowercase().contains(&search_text)
                    || a.summary
                        .as_deref()
                        .map(|s| s.to_lowercase().contains(&search_text))
                        .unwrap_or(false)
            })
            .cloned()
            .collect(),
        _ => Vec::new(),
    };

    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-content mb-6", "Knowledge Base" }

            Card { class: "mb-6",
                SearchInput {
                    value: search.read().clone(),
                    placeholder: "Search articles...",
                    oninput: move |e: FormEvent| search.set(e.value()),
                }
            }

            Card { title: "Articles",
                if fetch_failed {
                    div { class: "py-8 text-center text-sm text-red-600 dark:text-red-300",
                        "Could not load articles. Refresh the page to retry."
                    }
                } else if is_loading {
                    div { class: "space-y-3",
                        for _ in 0..4 {
                            div { class: "h-10 bg-surface-2 rounded animate-pulse" }
                        }
                    }
                } else if articles.is_empty() {
                    div { class: "py-8 text-center text-sm text-muted",
                        if search_text.is_empty() {
                            "No articles available yet."
                        } else {
                            "No articles match your search."
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        for article in articles.iter().cloned() {
                            PortalArticleItem {
                                key: "{article.id}",
                                id: article.id.to_string(),
                                title: article.title,
                                summary: article.summary.unwrap_or_default(),
                            }
                        }
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
    summary: String,
}

#[component]
fn PortalArticleItem(props: PortalArticleItemProps) -> Element {
    rsx! {
        Link {
            to: Route::KBArticleDetail { id: props.id.clone() },
            class: "block p-3 -mx-3 hover:bg-surface-2 rounded-lg transition-colors",
            h4 { class: "font-medium text-content", "{props.title}" }
            if !props.summary.is_empty() {
                p { class: "text-sm text-muted mt-1", "{props.summary}" }
            }
        }
    }
}

//! Client portal pages

use dioxus::prelude::*;

use crate::components::{
    invoice_status_badge, ticket_status_badge, Badge, BadgeVariant, BookIcon, Button,
    ButtonVariant, Card, CurrencyIcon, IconSize, PlusIcon, PortalLayout, SearchInput, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// MAPPS-357: portal-native "server unreachable" body.
///
/// The shared [`crate::components::ContentUnavailable`] mounts the internal
/// agent `AppLayout` (sidebar + admin nav), which would leak internal chrome
/// to a portal (client) user during an outage. This mirrors its copy and
/// self-healing posture but stays inside [`PortalLayout`] so the client-facing
/// header/footer are preserved. Like its sibling it is non-blocking: the page
/// re-runs and repopulates on its own once the MAPPS-333 recovery poll flips
/// the reachability flag back (the resource subscribes to it).
#[component]
fn PortalUnavailable(title: String) -> Element {
    rsx! {
        PortalLayout { title: "{title}",
            Card {
                div {
                    role: "status",
                    aria_live: "polite",
                    class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                    h3 { class: "text-base font-medium text-content", "Can't load this page" }
                    p { class: "mt-2 text-sm text-muted",
                        "The server is unreachable. This page will refresh on its own once the connection is back."
                    }
                    Link {
                        to: Route::PortalHome {},
                        class: "mt-6 inline-flex items-center rounded-md bg-surface-2 px-3 py-2 text-sm font-medium text-content hover:opacity-90",
                        "Go to portal home"
                    }
                }
            }
        }
    }
}

/// Portal home page
#[component]
pub fn PortalHomePage() -> Element {
    // MAPPS-357: N/A because this page fetches nothing. Every stat, ticket,
    // and invoice below is static demo copy, so there is no primary resource
    // that could fail during an outage and no mutating control to disable.
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
    // MAPPS-357: N/A because the ticket rows below are static demo data (no
    // fetch), so there is no primary resource to gate on an outage. The "New
    // Ticket" control is a plain navigation Link, not a mutation, so it stays
    // enabled.
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
    // MAPPS-357: N/A because this is a static "coming soon" page. It has no
    // fetch and no working submit (the form was removed), so there is no
    // primary resource to gate and no mutating control to disable.
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
            // MAPPS-357: subscribe to reachability so the ticket auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_portal_authed::<PortalTicketDetail>(&format!(
                "/portal/tickets/{id}"
            ))
            .await
            .ok()
        }
    });

    let snap = ticket_resource.read_unchecked();
    // `Some(None)` = fetch failed; `Some(Some(_))` = loaded; `None` = loading.
    let fetch_failed = matches!(*snap, Some(None));
    let header_title = match &*snap {
        Some(Some(t)) if !t.ticket_number.is_empty() => format!("Ticket {}", t.ticket_number),
        _ => format!("Ticket {}", props.id),
    };

    // MAPPS-357: the ticket is this page's PRIMARY resource. A failed load
    // while the server is flagged down is an outage, not a missing ticket, so
    // render the honest unavailable state (which keeps the nav + banner)
    // instead of the "Could not load ticket" card. A 4xx that fails while the
    // server is still reachable keeps that inline card below.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            PortalUnavailable { title: header_title.clone() }
        };
    }

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

                        // PMS-480: comments thread + reply form. The
                        // server filters internal / resolution /
                        // time_entry notes server-side (only `public`
                        // reaches the portal endpoint), so the SPA
                        // does not have to filter again.
                        PortalTicketComments { ticket_id: props.id.clone() }
                    }
                }
            }
        }
    }
}

// PMS-480: portal comments surface ----------------------------------------

/// One public note as the portal endpoint returns it. The shape
/// mirrors the server's `TicketNoteResponse` projection but stays
/// permissive (every field `#[serde(default)]`) so a thinner payload
/// still decodes.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalNote {
    id: uuid::Uuid,
    #[serde(default)]
    content: String,
    #[serde(default)]
    created_by_name: Option<String>,
    /// PMS-468 / PMS-449 phase 2: when populated, the note was
    /// authored by a portal contact. The portal UI uses
    /// `is_some()` to render "You" vs "Agent" attribution without
    /// the lossy name-equality heuristic the spec described.
    #[serde(default)]
    created_by_contact_id: Option<uuid::Uuid>,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Props, Clone, PartialEq)]
struct PortalTicketCommentsProps {
    ticket_id: String,
}

#[component]
fn PortalTicketComments(props: PortalTicketCommentsProps) -> Element {
    let id_for_fetch = props.ticket_id.clone();
    let id_for_post = props.ticket_id.clone();

    // Empty input + submit state. Resource refetches when `version`
    // is bumped (set after a successful POST so the new comment
    // shows without a page reload).
    let mut draft = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut version = use_signal(|| 0u32);

    let mut notes_resource = use_resource(use_reactive!(|id_for_fetch| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _v = version.read();
        // MAPPS-357: subscribe to reachability so the thread auto-refetches
        // the instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_portal_authed::<crate::utils::Paginated<PortalNote>>(
            &format!("/portal/tickets/{id_for_fetch}/notes?page=1&per_page=200"),
        )
        .await
        .map(|p| p.data)
        .ok()
    }));

    // `Some(None)` = fetch failed; `Some(Some(rows))` = fetched, possibly empty.
    let snap = notes_resource.read_unchecked();
    let rows: Vec<PortalNote> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));

    // MAPPS-357: block the reply POST while the server is unreachable so a
    // click cannot silently fail. This is a comments panel embedded in the
    // ticket detail page, so its notes fetch stays a secondary resource (it
    // degrades to the inline "Could not load comments" line rather than
    // swapping the whole page to ContentUnavailable).
    let can_mutate = crate::hooks::use_can_mutate();

    let handle_submit = move |_| {
        if *submitting.read() {
            return;
        }
        let content_v = draft.read().trim().to_string();
        if content_v.is_empty() {
            error.set("Reply cannot be empty.".to_string());
            return;
        }
        submitting.set(true);
        error.set(String::new());
        let id = id_for_post.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({ "content": content_v });
                match crate::hooks::fetch::api::post_portal_authed_typed::<PortalNote, _>(
                    &format!("/portal/tickets/{id}/notes"),
                    &body,
                )
                .await
                {
                    Ok(_) => {
                        draft.set(String::new());
                        version += 1;
                        notes_resource.restart();
                    }
                    Err(e) => error.set(format!("Could not send reply: {e}")),
                }
            }
            submitting.set(false);
        });
    };

    rsx! {
        Card { class: "mt-6",
            h2 { class: "text-lg font-semibold text-content mb-4", "Conversation" }

            if loading {
                p { class: "text-sm text-muted", "Loading comments..." }
            } else if fetch_failed {
                p { class: "text-sm text-red-600 dark:text-red-300", "Could not load comments." }
            } else if rows.is_empty() {
                p { class: "text-sm text-muted mb-4",
                    "No comments yet. Send the first reply below."
                }
            } else {
                ul { class: "space-y-4 mb-6",
                    for note in rows.iter().cloned() {
                        {
                            let key = note.id.to_string();
                            let is_customer = note.created_by_contact_id.is_some();
                            let author = note
                                .created_by_name
                                .clone()
                                .filter(|s| !s.trim().is_empty())
                                .unwrap_or_else(|| {
                                    if is_customer { "You".to_string() } else { "Agent".to_string() }
                                });
                            let when = note
                                .created_at
                                .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                .unwrap_or_default();
                            let (badge, label) = if is_customer {
                                (BadgeVariant::Blue, "Customer")
                            } else {
                                (BadgeVariant::Gray, "Agent")
                            };
                            rsx! {
                                li { key: "{key}", class: "rounded-md border border-line bg-surface-2 p-4",
                                    div { class: "flex items-center justify-between mb-2 gap-2",
                                        div { class: "flex items-center gap-2",
                                            span { class: "font-medium text-content text-sm", "{author}" }
                                            Badge { variant: badge, "{label}" }
                                        }
                                        if !when.is_empty() {
                                            span { class: "text-xs text-subtle", "{when}" }
                                        }
                                    }
                                    p { class: "text-sm text-content whitespace-pre-wrap",
                                        "{note.content}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Send-reply form. Submit button is disabled while the
            // POST is in flight to prevent the customer from double-
            // submitting the same comment.
            div { class: "border-t border-line pt-4",
                h3 { class: "text-sm font-medium text-content mb-2", "Send Reply" }
                if !error().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "{error}" }
                }
                textarea {
                    class: "w-full rounded-md border border-line bg-surface text-content p-2 text-sm focus:border-accent focus:ring-accent",
                    rows: 4,
                    placeholder: "Type your message...",
                    value: "{draft}",
                    oninput: move |e: FormEvent| draft.set(e.value()),
                }
                div { class: "mt-3 flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        // MAPPS-357: also disabled while the server is unreachable.
                        disabled: *submitting.read() || !can_mutate,
                        title: (!can_mutate).then(|| "Can't send a reply while the server is unreachable".to_string()),
                        onclick: handle_submit,
                        if *submitting.read() { "Sending..." } else { "Send Reply" }
                    }
                }
            }
        }
    }
}

/// Portal invoice list page
#[component]
pub fn PortalInvoiceListPage() -> Element {
    // MAPPS-357: N/A because the invoice rows below are static demo data (no
    // fetch), so there is no primary resource to gate on an outage. The "View"
    // buttons are decorative (no onclick / no payment flow), not mutations.
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

/// Render an amount string as currency. Routes through the shared
/// `format_money_str` helper (`src/utils/money.rs`) so portal invoice
/// amounts get the same grouped-thousands + two-decimals format every
/// other surface uses, instead of the previous `${raw}` concatenation
/// that produced `$60000.00` on a server payload of `"60000.00"`
/// (MAPPS-272).
fn portal_money(raw: &str) -> String {
    crate::utils::money::format_money_str(raw)
}

#[component]
pub fn PortalInvoiceDetailPage(props: PortalInvoiceDetailPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let invoice_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the invoice auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_portal_authed::<PortalInvoiceDetail>(&format!(
                "/portal/invoices/{id}"
            ))
            .await
            .ok()
        }
    });

    let snap = invoice_resource.read_unchecked();
    // `Some(None)` = fetch failed; `Some(Some(_))` = loaded; `None` = loading.
    let fetch_failed = matches!(*snap, Some(None));
    let header_title = match &*snap {
        Some(Some(inv)) if !inv.invoice_number.is_empty() => {
            format!("Invoice {}", inv.invoice_number)
        }
        _ => format!("Invoice {}", props.id),
    };

    // MAPPS-357: the invoice is this page's PRIMARY resource. A failed load
    // while the server is flagged down is an outage, not a missing invoice, so
    // render the honest unavailable state (which keeps the nav + banner)
    // instead of the "Could not load invoice" card. A 4xx that fails while the
    // server is still reachable keeps that inline card below.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            PortalUnavailable { title: header_title.clone() }
        };
    }

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

                            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-6 mb-6",
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
/// caller's company. MAPPS-395: the request carries the portal session
/// token from `/portal/login`, never the agent bearer, which that
/// middleware rejects on its `typ` claim.
#[component]
pub fn PortalKBPage() -> Element {
    let mut search = use_signal(String::new);

    let feed_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the feed auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_portal_authed::<PortalKbFeed>(
            "/portal/kb?page=1&per_page=100",
        )
        .await
        .ok()
    });

    let snap = feed_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));

    // MAPPS-357: the article feed is this page's PRIMARY resource. A failed
    // load while the server is flagged down is an outage, not an empty KB, so
    // render the honest unavailable state (which keeps the nav + banner)
    // instead of "No articles available yet." A 4xx that fails while the
    // server is still reachable keeps the inline "Could not load articles"
    // message below. The search box is a client-side filter, not a mutation.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            PortalUnavailable { title: "Knowledge Base".to_string() }
        };
    }

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

// ============================================================================
// PMS-675: client quote sign-off.
//
// The client-facing half of the PMS-673 flow. Reads are already scoped
// server-side to the signed-in contact's company and to quotes that were
// actually issued, so this surface does no filtering of its own; anything
// it can fetch, it is allowed to show.
// ============================================================================

use crate::modules::quotes::{status as quote_status, PortalQuoteDecisionRequest, QuoteResponse};
use crate::utils::money::format_money;
use crate::utils::Paginated;

/// Colour for a client-facing quote status. Narrower than the staff
/// palette because the internal states never reach the portal.
fn portal_quote_variant(status: &str) -> BadgeVariant {
    match status {
        "sent" => BadgeVariant::Orange,
        "accepted" => BadgeVariant::Green,
        "declined" => BadgeVariant::Red,
        "expired" => BadgeVariant::Gray,
        "converted" => BadgeVariant::Purple,
        _ => BadgeVariant::Gray,
    }
}

#[component]
pub fn PortalQuoteListPage() -> Element {
    let quotes_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_portal_authed::<Paginated<QuoteResponse>>(
            "/portal/quotes?page=1&per_page=50",
        )
        .await
        .ok()
    });

    let snap = quotes_resource.read_unchecked();
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<QuoteResponse> = match &*snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! { PortalUnavailable { title: "Quotes".to_string() } };
    }

    rsx! {
        PortalLayout { title: "Quotes",
            div { class: "mb-6",
                h1 { class: "text-2xl font-semibold text-content", "Quotes" }
                p { class: "text-sm text-subtle mt-1",
                    "Quotes we have sent you, and what you decided."
                }
            }

            if fetch_failed {
                Card {
                    p { class: "text-sm text-red-600 dark:text-red-300",
                        "Could not load your quotes. Refresh the page to retry."
                    }
                }
            } else if loading {
                Card { p { class: "text-sm text-subtle italic", "Loading quotes..." } }
            } else if rows.is_empty() {
                Card {
                    p { class: "text-sm text-subtle italic",
                        "You have no quotes yet. When we send one, it will appear here."
                    }
                }
            } else {
                Card {
                    Table {
                        TableHead {
                            TableRow {
                                TableHeader { "Quote" }
                                TableHeader { "Total" }
                                TableHeader { "Valid until" }
                                TableHeader { "Status" }
                            }
                        }
                        TableBody {
                            for quote in rows.iter().cloned() {
                                TableRow { key: "{quote.id}",
                                    TableCell {
                                        Link {
                                            to: Route::PortalQuoteDetail { id: quote.id.to_string() },
                                            class: "text-accent hover:opacity-90",
                                            "{quote.quote_number.clone().unwrap_or_else(|| \"Quote\".to_string())}"
                                        }
                                        div { class: "text-xs text-subtle", "{quote.title}" }
                                    }
                                    TableCell { class: "font-medium", "{format_money(quote.total)}" }
                                    TableCell {
                                        "{quote.valid_until.map(|d| d.format(\"%b %-d, %Y\").to_string()).unwrap_or_else(|| \"No expiry\".to_string())}"
                                    }
                                    TableCell {
                                        Badge { variant: portal_quote_variant(&quote.status),
                                            "{quote_status::label(&quote.status)}"
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

#[derive(Props, Clone, PartialEq)]
pub struct PortalQuoteDetailPageProps {
    pub id: String,
}

#[component]
pub fn PortalQuoteDetailPage(props: PortalQuoteDetailPageProps) -> Element {
    let mut version = use_signal(|| 0u32);
    let mut notes = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    // Accepting is a commercial commitment the client cannot take back
    // from here, so it goes through an explicit confirm step rather than
    // firing on the first click.
    let mut confirming = use_signal(|| Option::<bool>::None);

    let id_for_resource = props.id.clone();
    let quote_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            let _v = version.read();
            crate::hooks::fetch::api::get_portal_authed::<QuoteResponse>(&format!(
                "/portal/quotes/{id}"
            ))
            .await
            .ok()
        }
    });

    let snap = quote_resource.read_unchecked();
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let quote: Option<QuoteResponse> = match &*snap {
        Some(Some(q)) => Some(q.clone()),
        _ => None,
    };

    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! { PortalUnavailable { title: "Quote".to_string() } };
    }

    let quote_id = props.id.clone();
    let mut decide = move |accept: bool| {
        let qid = quote_id.clone();
        let note_text = notes.read().trim().to_string();
        submitting.set(true);
        error.set(String::new());
        spawn(async move {
            let action = if accept { "accept" } else { "decline" };
            let body = PortalQuoteDecisionRequest {
                notes: (!note_text.is_empty()).then_some(note_text),
            };
            match crate::hooks::fetch::api::post_portal_authed::<QuoteResponse, _>(
                &format!("/portal/quotes/{qid}/{action}"),
                &body,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        if accept {
                            "Quote accepted"
                        } else {
                            "Quote declined"
                        },
                    );
                    confirming.set(None);
                    version += 1;
                }
                Err(e) => error.set(format!("Could not record your decision: {e}")),
            }
            submitting.set(false);
        });
    };

    rsx! {
        PortalLayout { title: "Quote",
            div { class: "mb-6",
                Link {
                    to: Route::PortalQuoteList {},
                    class: "text-sm text-accent hover:opacity-90",
                    "Back to quotes"
                }
            }

            if fetch_failed {
                Card {
                    p { class: "text-sm text-red-600 dark:text-red-300",
                        "Could not load this quote."
                    }
                }
            } else if loading {
                Card { p { class: "text-sm text-subtle italic", "Loading quote..." } }
            } else if let Some(q) = quote.clone() {
                {
                    let st = q.status.clone();
                    let awaiting = quote_status::awaiting_client(&st);
                    rsx! {
                        Card { class: "mb-6",
                            div { class: "flex items-start justify-between gap-4 mb-4",
                                div {
                                    h1 { class: "text-xl font-semibold text-content",
                                        "{q.quote_number.clone().unwrap_or_else(|| \"Quote\".to_string())}"
                                    }
                                    p { class: "text-sm text-subtle", "{q.title}" }
                                }
                                Badge { variant: portal_quote_variant(&st),
                                    "{quote_status::label(&st)}"
                                }
                            }

                            if let Some(desc) = q.description.clone().filter(|s| !s.is_empty()) {
                                p { class: "text-sm whitespace-pre-wrap mb-4", "{desc}" }
                            }

                            Table {
                                TableHead {
                                    TableRow {
                                        TableHeader { "Description" }
                                        TableHeader { "Qty" }
                                        TableHeader { "Unit price" }
                                        TableHeader { "Total" }
                                    }
                                }
                                TableBody {
                                    for line in q.lines.clone().unwrap_or_default() {
                                        TableRow { key: "{line.id}",
                                            TableCell { "{line.description}" }
                                            TableCell { "{line.quantity}" }
                                            TableCell { "{format_money(line.unit_price)}" }
                                            TableCell { class: "font-medium", "{format_money(line.total)}" }
                                        }
                                    }
                                }
                            }

                            div { class: "mt-4 flex flex-col items-end gap-1 text-sm",
                                div { "Subtotal: " span { class: "font-medium", "{format_money(q.subtotal)}" } }
                                div { "Tax: " span { class: "font-medium", "{format_money(q.tax_amount)}" } }
                                div { class: "text-base",
                                    "Total: " span { class: "font-semibold", "{format_money(q.total)}" }
                                }
                            }

                            if let Some(valid) = q.valid_until {
                                p { class: "mt-3 text-xs text-subtle",
                                    "Valid until {valid.format(\"%b %-d, %Y\")}."
                                }
                            }
                        }

                        if awaiting {
                            Card { title: "Your decision",
                                if !error.read().is_empty() {
                                    p { class: "mb-3 text-sm text-red-600 dark:text-red-300", "{error}" }
                                }
                                textarea {
                                    class: "w-full rounded-md border border-default bg-surface p-2 text-sm",
                                    rows: 3,
                                    placeholder: "Anything you want to add (optional)",
                                    value: "{notes}",
                                    oninput: move |e| notes.set(e.value()),
                                }
                                match *confirming.read() {
                                    None => rsx! {
                                        div { class: "mt-3 flex gap-2",
                                            Button {
                                                variant: ButtonVariant::Primary,
                                                disabled: *submitting.read(),
                                                onclick: move |_| confirming.set(Some(true)),
                                                "Accept quote"
                                            }
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                disabled: *submitting.read(),
                                                onclick: move |_| confirming.set(Some(false)),
                                                "Decline"
                                            }
                                        }
                                    },
                                    Some(accept) => rsx! {
                                        div { class: "mt-3",
                                            p { class: "text-sm mb-2",
                                                if accept {
                                                    "Accepting this quote authorises the work at this price. This cannot be undone here."
                                                } else {
                                                    "Decline this quote? You can talk to us if you would like it revised."
                                                }
                                            }
                                            div { class: "flex gap-2",
                                                Button {
                                                    variant: if accept { ButtonVariant::Primary } else { ButtonVariant::Danger },
                                                    disabled: *submitting.read(),
                                                    onclick: move |_| decide(accept),
                                                    if accept { "Yes, accept" } else { "Yes, decline" }
                                                }
                                                Button {
                                                    variant: ButtonVariant::Secondary,
                                                    disabled: *submitting.read(),
                                                    onclick: move |_| confirming.set(None),
                                                    "Go back"
                                                }
                                            }
                                        }
                                    },
                                }
                            }
                        } else {
                            // Decided, expired, or already converted: show the
                            // outcome rather than controls that would 409.
                            Card { title: "Status",
                                p { class: "text-sm",
                                    match st.as_str() {
                                        "accepted" => "You accepted this quote. We will be in touch about scheduling the work.",
                                        "declined" => "You declined this quote.",
                                        "expired" => "This quote passed its valid-until date. Contact us if you would still like to go ahead.",
                                        "converted" => "This quote has been turned into a project and the work is underway.",
                                        _ => "No action is needed from you right now.",
                                    }
                                }
                                if let Some(when) = q.decided_at {
                                    p { class: "mt-1 text-xs text-subtle",
                                        "Decided {when.format(\"%b %-d, %Y\")}."
                                    }
                                }
                                if let Some(n) = q.decision_notes.clone().filter(|s| !s.is_empty()) {
                                    p { class: "mt-2 text-sm", "Your note: {n}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

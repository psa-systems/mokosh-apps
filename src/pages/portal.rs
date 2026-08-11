//! Client portal pages

use dioxus::prelude::*;

use crate::components::{
    invoice_status_badge, Badge, BadgeVariant, BookIcon, Button, ButtonVariant, Card, CurrencyIcon,
    IconSize, PlusIcon, PortalLayout, SearchInput, Table, TableBody, TableCell, TableEmptyRow,
    TableHead, TableHeader, TableRow,
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

/// PMS-729 phase 2 §7 slice A / I17: server DTO for the four home cards.
///
/// Mirrors mokosh-server `PortalDashboardResponse`. Every field is decoded
/// so a future card can render without a wire-shape change. The SPA never
/// pretends to know a metric that is not present: an absent
/// `next_invoice_due` renders the "nothing outstanding" state, not a
/// zero-dollar placeholder.
#[derive(Clone, Default, PartialEq, serde::Deserialize)]
struct DashboardPayload {
    #[serde(default)]
    tickets_by_priority: Vec<PriorityBucket>,
    #[serde(default)]
    next_invoice_due: Option<NextInvoiceDue>,
    #[serde(default)]
    open_quotes_awaiting_decision: i64,
    #[serde(default)]
    recent_activity: Vec<ActivityRow>,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
struct PriorityBucket {
    name: String,
    color: String,
    count: i64,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
struct NextInvoiceDue {
    id: String,
    invoice_number: String,
    total: String,
    balance_due: String,
    due_date: chrono::NaiveDate,
    currency: String,
}

#[derive(Clone, PartialEq, serde::Deserialize)]
struct ActivityRow {
    kind: String,
    entity_id: String,
    label: String,
    summary: String,
    at: chrono::DateTime<chrono::Utc>,
}

/// Portal home page. PMS-729 phase 2 §7 slice A / I17: renders the four
/// dashboard cards D17 pins for phase 2.
#[component]
pub fn PortalHomePage() -> Element {
    use crate::hooks::classify_remote;

    crate::components::use_page_title("Home");
    let server_reachable = crate::hooks::use_server_reachable();
    let dashboard = use_resource(move || async move {
        // Subscribe to the tenant-generation signal so a tenant switch (or
        // an auth flip) re-fetches without any per-page glue.
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_portal_authed::<DashboardPayload>("/portal/dashboard").await
    });
    let outcome: Option<Result<DashboardPayload, String>> = dashboard.read_unchecked().clone();
    let state = classify_remote(outcome, server_reachable);

    rsx! {
        PortalLayout { title: "Home".to_string(),
            match state {
                crate::hooks::RemoteData::Loading => rsx! {
                    Card {
                        p { class: "text-sm text-muted py-4 text-center", "Loading your dashboard..." }
                    }
                },
                crate::hooks::RemoteData::Unavailable => rsx! {
                    Card {
                        p { class: "text-sm text-muted py-4 text-center",
                            "The server is unreachable. Your dashboard will appear once the connection is back."
                        }
                    }
                },
                crate::hooks::RemoteData::Ready(payload) => rsx! {
                    DashboardCards { payload: payload.clone() }
                },
            }

            // Quick actions stay under the cards regardless of whether the
            // fetch succeeded; they are pure navigation Links, not data.
            div { class: "mt-8",
                h2 { class: "text-lg font-medium text-content mb-4",
                    "Quick actions"
                }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                    Link {
                        to: Route::PortalTicketNew {},
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        PlusIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Submit new ticket" }
                    }
                    Link {
                        to: Route::PortalKB {},
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        BookIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Browse knowledge base" }
                    }
                    Link {
                        to: Route::PortalInvoiceList {},
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        CurrencyIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Pay an invoice" }
                    }
                }
            }
        }
    }
}

/// Painted grid of the four dashboard cards. Kept as a separate
/// component so the parent stays a thin state machine and the card
/// layout can be tuned without diffing the whole page.
#[derive(Props, Clone, PartialEq)]
struct DashboardCardsProps {
    payload: DashboardPayload,
}

#[component]
fn DashboardCards(props: DashboardCardsProps) -> Element {
    let p = &props.payload;
    let total_open_tickets: i64 = p.tickets_by_priority.iter().map(|b| b.count).sum();
    rsx! {
        div { class: "grid grid-cols-1 md:grid-cols-2 gap-6 mb-8",
            // Card 1: open tickets by priority.
            Card {
                div { class: "flex items-center justify-between mb-3",
                    h2 { class: "text-lg font-medium text-content", "Open tickets" }
                    span { class: "text-2xl font-bold text-accent", "{total_open_tickets}" }
                }
                if p.tickets_by_priority.is_empty() {
                    p { class: "text-sm text-muted", "No priorities configured yet." }
                } else {
                    ul { class: "space-y-1",
                        for bucket in p.tickets_by_priority.iter().cloned() {
                            li { class: "flex items-center justify-between text-sm",
                                span { class: "flex items-center gap-2 text-content",
                                    // A colored dot from the priority's own hex.
                                    // Inline style is the only place branding /
                                    // priority hex leaks in, matching the agent
                                    // side's `PriorityBadge` convention.
                                    span {
                                        class: "inline-block h-2.5 w-2.5 rounded-full",
                                        style: "background-color: {bucket.color};",
                                    }
                                    "{bucket.name}"
                                }
                                span { class: "font-medium text-content", "{bucket.count}" }
                            }
                        }
                    }
                }
            }

            // Card 2: next invoice due.
            Card {
                h2 { class: "text-lg font-medium text-content mb-3", "Next invoice due" }
                if let Some(inv) = p.next_invoice_due.as_ref() {
                    div { class: "space-y-2 text-sm text-content",
                        div { class: "flex items-center justify-between",
                            Link {
                                to: Route::PortalInvoiceDetail { id: inv.id.clone() },
                                class: "text-accent hover:opacity-80 font-medium",
                                "#{inv.invoice_number}"
                            }
                            span { class: "font-medium",
                                "{inv.balance_due} {inv.currency}"
                            }
                        }
                        p { class: "text-xs text-muted",
                            "Due {inv.due_date} (total {inv.total} {inv.currency})"
                        }
                    }
                } else {
                    p { class: "text-sm text-muted", "Nothing outstanding right now." }
                }
            }

            // Card 3: open quotes.
            Card {
                div { class: "flex items-center justify-between mb-3",
                    h2 { class: "text-lg font-medium text-content", "Quotes awaiting decision" }
                    span { class: "text-2xl font-bold text-accent",
                        "{p.open_quotes_awaiting_decision}"
                    }
                }
                if p.open_quotes_awaiting_decision == 0 {
                    p { class: "text-sm text-muted",
                        "No quotes waiting on you."
                    }
                } else {
                    // Quote list lives at a different route in the agent
                    // shell today; portal quote list is a phase 2 §7 follow
                    // up (D8). For now the card gets a subtle affordance so
                    // the customer knows there is something to look at
                    // without a broken link.
                    p { class: "text-sm text-muted",
                        "Check the notifications inbox once quote review lands in the portal."
                    }
                }
            }

            // Card 4: recent activity.
            Card {
                h2 { class: "text-lg font-medium text-content mb-3", "Recent activity" }
                if p.recent_activity.is_empty() {
                    p { class: "text-sm text-muted", "Nothing new yet." }
                } else {
                    ul { class: "space-y-2",
                        for row in p.recent_activity.iter().cloned() {
                            ActivityRowView { row }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ActivityRowViewProps {
    row: ActivityRow,
}

#[component]
fn ActivityRowView(props: ActivityRowViewProps) -> Element {
    let row = &props.row;
    let target: Route = match row.kind.as_str() {
        "ticket" => Route::PortalTicketDetail {
            id: row.entity_id.clone(),
        },
        _ => Route::PortalHome {},
    };
    let when = crate::utils::datetime::format_user_datetime(row.at, None);
    rsx! {
        li { class: "border-l-2 border-accent pl-3",
            Link {
                to: target,
                class: "block text-sm font-medium text-content hover:text-accent truncate",
                "{row.label}"
            }
            div { class: "text-xs text-muted",
                span { "{row.summary}" }
                span { class: "ml-1", " - {when}" }
            }
        }
    }
}

/// Portal ticket list page
#[component]
pub fn PortalTicketListPage() -> Element {
    // MAPPS-414: the hardcoded demo ticket rows were removed so no fabricated
    // tickets reach a real client (the same defect MAPPS-403 fixed on
    // PortalHomePage). Until the portal ticket-list endpoint is wired, the page
    // shows an honest empty state.
    //
    // Eventual fix: fetch the portal ticket-list endpoint via `use_resource` +
    // `get_portal_authed` + `use_server_reachable`, mirroring
    // `PortalInvoiceDetailPage`, then render the rows and the outage-aware
    // states from the live payload (gated off now, wire later, ref MAPPS-414).
    //
    // MAPPS-357: N/A while the page fetches nothing. The "New Ticket" control
    // is a plain navigation Link, not a mutation, so it stays enabled.
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

            // Honest empty state in place of the removed demo rows: no
            // fabricated tickets until the portal API lands (matches the
            // MAPPS-403 PortalHomePage placeholder style).
            Card {
                p { class: "text-sm text-muted", "No tickets yet." }
            }
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
                    // MAPPS-409: machine-readable form for the `<time>` wrapper.
                    let created_iso = ticket
                        .created_at
                        .map(|d| d.to_rfc3339())
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
                                    h2 { class: "text-xl font-bold text-content",
                                        "{subject}"
                                    }
                                    div { class: "flex items-center mt-2 space-x-4",
                                        Badge { variant: BadgeVariant::Yellow, "{status_label}" }
                                        if !created.is_empty() {
                                            span { class: "text-sm text-muted",
                                                time { datetime: "{created_iso}", "{created}" }
                                            }
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

                        // PMS-729 phase 2 §7 slice A / I10: SLA card.
                        // Fetches its own resource so a broken SLA
                        // fetch does not block the ticket body or the
                        // comments thread. Hidden entirely when the
                        // server reports `not_applicable` and no due
                        // dates - a portal customer on a plan that
                        // does not carry SLA policies does not need
                        // a "not applicable" placeholder card.
                        PortalTicketSlaCard { ticket_id: props.id.clone() }

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

// PMS-729 phase 2 §7 slice A / I10: SLA visibility on portal ticket ---------

/// Wire shape for `GET /portal/tickets/{id}/sla`. Every datetime field
/// is `Option<_>` so a ticket with no SLA policy at all decodes cleanly
/// (server sends every column back regardless).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
struct PortalSlaPayload {
    #[serde(default)]
    sla_due_date: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    first_response_due: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    first_response_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    resolution_due: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_sla_status")]
    status: String,
    #[serde(default)]
    status_name: String,
}

fn default_sla_status() -> String {
    "not_applicable".to_string()
}

#[derive(Props, Clone, PartialEq)]
struct PortalTicketSlaCardProps {
    ticket_id: String,
}

#[component]
fn PortalTicketSlaCard(props: PortalTicketSlaCardProps) -> Element {
    let id_for_fetch = props.ticket_id.clone();
    let sla = use_resource(move || {
        let id = id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_portal_authed::<PortalSlaPayload>(&format!(
                "/portal/tickets/{id}/sla"
            ))
            .await
            .ok()
        }
    });

    let snap = sla.read_unchecked();
    let Some(Some(payload)) = &*snap else {
        // Loading or fetch failed: no card at all, mirroring the
        // "hide when there is nothing to say" posture the plan doc
        // asks for.
        return rsx! { {} };
    };

    // Nothing to render when both legs are absent AND the ticket is
    // not applicable. The customer's MSP has not attached an SLA
    // policy to the ticket; a "not applicable" chip alone reads as
    // noise here.
    let has_any_target = payload.first_response_due.is_some()
        || payload.resolution_due.is_some()
        || payload.sla_due_date.is_some();
    if !has_any_target {
        return rsx! { {} };
    }

    let (badge_variant, badge_label) = match payload.status.as_str() {
        "on_track" => (BadgeVariant::Green, "On track"),
        "warning" => (BadgeVariant::Yellow, "At risk"),
        "breached" => (BadgeVariant::Red, "Breached"),
        _ => (BadgeVariant::Gray, "Not applicable"),
    };

    let fmt = |t: Option<chrono::DateTime<chrono::Utc>>| -> String {
        t.map(|d| crate::utils::datetime::format_user_datetime(d, None))
            .unwrap_or_else(|| "Not set".to_string())
    };

    rsx! {
        Card {
            div { class: "flex items-center justify-between mb-3",
                h3 { class: "text-lg font-medium text-content", "Service level" }
                Badge { variant: badge_variant, "{badge_label}" }
            }
            dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-2 text-sm",
                if payload.first_response_due.is_some() || payload.first_response_at.is_some() {
                    dt { class: "text-muted", "First response target" }
                    dd { class: "text-content", "{fmt(payload.first_response_due)}" }
                    dt { class: "text-muted", "First response actual" }
                    dd { class: "text-content",
                        if payload.first_response_at.is_some() {
                            "{fmt(payload.first_response_at)}"
                        } else {
                            "Not responded yet"
                        }
                    }
                }
                if payload.resolution_due.is_some() || payload.resolved_at.is_some() {
                    dt { class: "text-muted", "Resolution target" }
                    dd { class: "text-content", "{fmt(payload.resolution_due)}" }
                    dt { class: "text-muted", "Resolved" }
                    dd { class: "text-content",
                        if payload.resolved_at.is_some() {
                            "{fmt(payload.resolved_at)}"
                        } else {
                            "Not resolved yet"
                        }
                    }
                }
                if !payload.status_name.is_empty() {
                    dt { class: "text-muted", "Current status" }
                    dd { class: "text-content", "{payload.status_name}" }
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

    // PMS-729 phase 2 §7 slice B / I2: files selected for the next
    // reply. Cleared on successful submit. Storing the whole `FileList`
    // as a `Signal<Option<web_sys::FileList>>` costs one Rc clone but
    // keeps the file handles alive across the async submit closure.
    #[cfg(feature = "web")]
    let mut selected_files: Signal<Option<web_sys::FileList>> = use_signal(|| None);
    #[cfg(feature = "web")]
    let selected_file_count = selected_files
        .read()
        .as_ref()
        .map(|f| f.length())
        .unwrap_or(0);
    #[cfg(not(feature = "web"))]
    let selected_file_count: u32 = 0;

    let id_for_submit = id_for_post.clone();
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
        let id = id_for_submit.clone();
        #[cfg(feature = "web")]
        let files_snapshot = selected_files.read().clone();
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
                    Ok(note) => {
                        // PMS-729 phase 2 §7 slice B / I2: upload each
                        // selected file to the freshly created note.
                        // A partial failure (one file 400s but the
                        // others succeed) surfaces to the customer so
                        // they know to retry; the note itself stays
                        // posted so the conversation is never lost.
                        let mut upload_errors: Vec<String> = Vec::new();
                        if let Some(files) = &files_snapshot {
                            for i in 0..files.length() {
                                let Some(file) = files.item(i) else {
                                    continue;
                                };
                                let form = match web_sys::FormData::new() {
                                    Ok(f) => f,
                                    Err(_) => {
                                        upload_errors.push(
                                            "browser refused to build the upload form".to_string(),
                                        );
                                        continue;
                                    }
                                };
                                let name = file.name();
                                if form
                                    .append_with_blob_and_filename("file", &file, &name)
                                    .is_err()
                                {
                                    upload_errors
                                        .push(format!("could not attach {name} to the upload"));
                                    continue;
                                }
                                let path =
                                    format!("/portal/tickets/{id}/notes/{}/attachments", note.id);
                                if let Err(err) =
                                    crate::hooks::fetch::api::post_portal_authed_multipart::<
                                        serde_json::Value,
                                    >(&path, &form)
                                    .await
                                {
                                    upload_errors.push(format!("{name}: {}", err.user_message()));
                                }
                            }
                        }
                        draft.set(String::new());
                        selected_files.set(None);
                        version += 1;
                        notes_resource.restart();
                        if !upload_errors.is_empty() {
                            error.set(format!(
                                "Reply sent, but some attachments failed: {}",
                                upload_errors.join("; ")
                            ));
                        }
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
                p { class: "text-sm text-muted", "Loading comments…" }
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
                            // MAPPS-409: machine-readable form for the `<time>` wrapper.
                            let when_iso = note
                                .created_at
                                .map(|d| d.to_rfc3339())
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
                                            span { class: "text-xs text-muted",
                                                time { datetime: "{when_iso}", "{when}" }
                                            }
                                        }
                                    }
                                    p { class: "text-sm text-content whitespace-pre-wrap",
                                        "{note.content}"
                                    }
                                    // PMS-729 phase 2 §7 slice B / I2:
                                    // per-note attachment list. Fetches
                                    // its own resource so a slow list
                                    // does not block the conversation
                                    // rendering.
                                    PortalNoteAttachments {
                                        ticket_id: id_for_post.clone(),
                                        note_id: note.id.to_string(),
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
                    placeholder: "Type your message…",
                    value: "{draft}",
                    oninput: move |e: FormEvent| draft.set(e.value()),
                }

                // PMS-729 phase 2 §7 slice B / I2: file attachments
                // for the next reply. `multiple` lets the customer
                // upload every relevant screenshot in one go; the
                // browser natively renders the "N files selected"
                // affordance so the SPA only needs to add a hint
                // underneath.
                div { class: "mt-3",
                    label {
                        class: "block text-xs font-medium text-content mb-1",
                        r#for: "portal-attachment-input",
                        "Attach files (optional)"
                    }
                    input {
                        id: "portal-attachment-input",
                        r#type: "file",
                        multiple: true,
                        class: "block w-full text-xs text-content file:mr-3 file:rounded file:border-0 file:bg-accent file:px-3 file:py-1.5 file:text-on-accent hover:file:opacity-90",
                        disabled: *submitting.read() || !can_mutate,
                        onchange: move |_e: FormEvent| {
                            #[cfg(feature = "web")]
                            {
                                use wasm_bindgen::JsCast;
                                // The onchange event payload from Dioxus does not
                                // surface `HTMLInputElement.files` directly, so
                                // reach through `web_sys` on the actual DOM node.
                                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                    if let Some(el) = doc.get_element_by_id("portal-attachment-input") {
                                        if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                            selected_files.set(input.files());
                                        }
                                    }
                                }
                            }
                        },
                    }
                    if selected_file_count > 0 {
                        p { class: "mt-1 text-xs text-muted",
                            "{selected_file_count} file(s) will be attached to your reply."
                        }
                    }
                }
                div { class: "mt-3 flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        // MAPPS-357: also disabled while the server is unreachable.
                        disabled: *submitting.read() || !can_mutate,
                        loading: *submitting.read(),
                        title: (!can_mutate).then(|| "Can't send a reply while the server is unreachable".to_string()),
                        onclick: handle_submit,
                        "Send Reply"
                    }
                }
            }
        }
    }
}

/// Portal invoice list page
#[component]
pub fn PortalInvoiceListPage() -> Element {
    // MAPPS-414: the hardcoded demo invoice rows (with fabricated amounts and
    // decorative "View" buttons) were removed so no fake invoices reach a real
    // client (the same defect MAPPS-403 fixed on PortalHomePage). Until the
    // portal invoice-list endpoint is wired, the page shows an honest empty
    // state.
    //
    // Eventual fix: fetch the portal invoice-list endpoint via `use_resource` +
    // `get_portal_authed` + `use_server_reachable`, mirroring
    // `PortalInvoiceDetailPage`, then render the rows and the outage-aware
    // states from the live payload (gated off now, wire later, ref MAPPS-414).
    //
    // MAPPS-357: N/A while the page fetches nothing.
    rsx! {
        // P1-10 dedup: title rendered once below.
        PortalLayout {
            h1 { class: "text-2xl font-bold text-content mb-6", "Invoices" }

            // Honest empty state in place of the removed demo rows: no
            // fabricated invoices until the portal API lands (matches the
            // MAPPS-403 PortalHomePage placeholder style).
            Card {
                p { class: "text-sm text-muted", "No invoices yet." }
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
        PortalLayout {
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
                                    h2 { class: "text-xl font-bold text-content",
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

                            div { class: "mb-6",
                                Table {
                                    TableHead {
                                        TableRow {
                                            TableHeader { "Description" }
                                            TableHeader { class: "text-right", "Qty" }
                                            TableHeader { class: "text-right", "Unit Price" }
                                            TableHeader { class: "text-right", "Total" }
                                        }
                                    }
                                    TableBody {
                                        if lines.is_empty() {
                                            TableEmptyRow { columns: 4, class: "text-muted", "No line items." }
                                        } else {
                                            for (idx, line) in lines.iter().enumerate() {
                                                TableRow { key: "{idx}",
                                                    TableCell { "{line.description}" }
                                                    TableCell { class: "text-right", "{line.quantity}" }
                                                    TableCell { class: "text-right", {portal_money(&line.unit_price)} }
                                                    TableCell { class: "text-right", {portal_money(&line.total)} }
                                                }
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

                        // PMS-729 phase 2 §7 slice A / I11: payment
                        // history card. Fetches its own resource so a
                        // broken payment fetch does not block the
                        // rest of the invoice detail; hidden entirely
                        // on an empty ledger so the layout stays
                        // clean for a brand-new invoice.
                        PortalInvoicePaymentsCard { invoice_id: props.id.clone() }
                    }
                }
            }
        }
    }
}

// PMS-729 phase 2 §7 slice A / I11: payment history on invoice detail -------

/// One payment row as `GET /portal/invoices/{id}/payments` returns it.
/// `amount` arrives as the same decimal string the invoice detail uses,
/// so `portal_money` renders it identically.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalPaymentRow {
    id: uuid::Uuid,
    #[serde(default)]
    payment_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    payment_method: String,
    #[serde(default)]
    reference_number: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
struct PortalPaymentsPayload {
    #[serde(default)]
    payments: Vec<PortalPaymentRow>,
    #[serde(default)]
    total: i64,
}

/// Human label for the payment_method enum values the server sends.
/// Matches the seed's `payments.payment_method` CHECK constraint values.
fn payment_method_label(raw: &str) -> &'static str {
    match raw {
        "check" => "Check",
        "credit_card" => "Credit card",
        "ach" => "ACH",
        "wire" => "Wire transfer",
        "cash" => "Cash",
        _ => "Other",
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalInvoicePaymentsCardProps {
    invoice_id: String,
}

#[component]
fn PortalInvoicePaymentsCard(props: PortalInvoicePaymentsCardProps) -> Element {
    let id_for_fetch = props.invoice_id.clone();
    let payments = use_resource(move || {
        let id = id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_portal_authed::<PortalPaymentsPayload>(&format!(
                "/portal/invoices/{id}/payments"
            ))
            .await
            .ok()
        }
    });

    let snap = payments.read_unchecked();
    let Some(Some(payload)) = &*snap else {
        // Loading or fetch failed: no card at all. The invoice body
        // above already carries the balance due, which is the primary
        // signal.
        return rsx! { {} };
    };
    if payload.total == 0 {
        // Empty ledger renders nothing. A first-time invoice does not
        // need a placeholder card.
        return rsx! { {} };
    }

    rsx! {
        Card {
            div { class: "mt-6",
                h3 { class: "text-lg font-medium text-content mb-3", "Payment history" }
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Method" }
                            TableHeader { "Reference" }
                            TableHeader { class: "text-right", "Amount" }
                        }
                    }
                    TableBody {
                        for row in payload.payments.iter().cloned() {
                            TableRow { key: "{row.id}",
                                TableCell {
                                    if let Some(date) = row.payment_date {
                                        "{date}"
                                    } else {
                                        "-"
                                    }
                                }
                                TableCell { "{payment_method_label(&row.payment_method)}" }
                                TableCell {
                                    if let Some(reference) = &row.reference_number {
                                        "{reference}"
                                    } else {
                                        "-"
                                    }
                                }
                                TableCell { class: "text-right", {portal_money(&row.amount)} }
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
                    placeholder: "Search articles…",
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
        PortalLayout {
            div { class: "mb-6",
                h1 { class: "text-2xl font-bold text-content", "Quotes" }
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
                Card { p { class: "text-sm text-subtle italic", "Loading quotes…" } }
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
                Card { p { class: "text-sm text-subtle italic", "Loading quote…" } }
            } else if let Some(q) = quote.clone() {
                {
                    let st = q.status.clone();
                    let awaiting = quote_status::awaiting_client(&st);
                    rsx! {
                        Card { class: "mb-6",
                            div { class: "flex items-start justify-between gap-4 mb-4",
                                div {
                                    h2 { class: "text-xl font-bold text-content",
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

// PMS-729 phase 2 §7 slice B / I2: portal ticket-note attachments -----------

/// One attachment as `GET /portal/tickets/{tid}/notes/{nid}/attachments`
/// returns it. Mirrors the server `AttachmentResponse`; permissive so a
/// thinner payload still decodes.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalAttachment {
    id: uuid::Uuid,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    file_size: i64,
    #[serde(default)]
    created_by_contact_id: Option<uuid::Uuid>,
}

#[derive(Props, Clone, PartialEq)]
struct PortalNoteAttachmentsProps {
    ticket_id: String,
    note_id: String,
}

#[component]
fn PortalNoteAttachments(props: PortalNoteAttachmentsProps) -> Element {
    let ticket_id = props.ticket_id.clone();
    let note_id = props.note_id.clone();
    let fetch_ticket = ticket_id.clone();
    let fetch_note = note_id.clone();
    let list = use_resource(move || {
        let ticket = fetch_ticket.clone();
        let note = fetch_note.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_portal_authed::<Vec<PortalAttachment>>(&format!(
                "/portal/tickets/{ticket}/notes/{note}/attachments"
            ))
            .await
            .ok()
        }
    });
    let snap = list.read_unchecked();
    let Some(Some(rows)) = &*snap else {
        // Loading or the endpoint refused: no chrome, no placeholder.
        return rsx! { {} };
    };
    if rows.is_empty() {
        return rsx! { {} };
    }
    rsx! {
        div { class: "mt-3 border-t border-line pt-2",
            p { class: "text-xs font-medium text-muted mb-1", "Attachments" }
            ul { class: "space-y-1",
                for att in rows.iter().cloned() {
                    li { class: "text-xs",
                        PortalAttachmentLink {
                            ticket_id: ticket_id.clone(),
                            note_id: note_id.clone(),
                            attachment: att,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalAttachmentLinkProps {
    ticket_id: String,
    note_id: String,
    attachment: PortalAttachment,
}

/// Download button that pulls the file with the portal bearer and
/// synthesises a browser download. The endpoint sits behind
/// `RequirePortalAuth`, so a plain `<a href>` is a guaranteed 401
/// (the bearer only lives in WASM memory).
#[component]
fn PortalAttachmentLink(props: PortalAttachmentLinkProps) -> Element {
    let mut downloading = use_signal(|| false);
    let mut err = use_signal(String::new);
    let att = props.attachment.clone();
    let ticket_id = props.ticket_id.clone();
    let note_id = props.note_id.clone();
    let size_kb = ((att.file_size as f64) / 1024.0).max(1.0).ceil() as i64;
    let file_name = att.file_name.clone();
    let att_id = att.id.to_string();
    let click_ticket = ticket_id.clone();
    let click_note = note_id.clone();
    let click_att_id = att_id.clone();
    let click_name = file_name.clone();
    rsx! {
        button {
            r#type: "button",
            class: "text-accent hover:opacity-80 disabled:opacity-60",
            disabled: *downloading.read(),
            onclick: move |_| {
                if *downloading.peek() {
                    return;
                }
                downloading.set(true);
                err.set(String::new());
                let ticket = click_ticket.clone();
                let note = click_note.clone();
                let aid = click_att_id.clone();
                let name = click_name.clone();
                spawn(async move {
                    #[cfg(feature = "web")]
                    {
                        let path = format!(
                            "/portal/tickets/{ticket}/notes/{note}/attachments/{aid}"
                        );
                        match crate::hooks::fetch::api::get_portal_authed_bytes(&path).await {
                            Ok((bytes, filename)) => {
                                let fname = filename.unwrap_or_else(|| name.clone());
                                if let Err(e) =
                                    crate::utils::download::save_bytes_as_file(&bytes, &fname)
                                {
                                    err.set(e);
                                }
                            }
                            Err(e) => err.set(e),
                        }
                    }
                    downloading.set(false);
                });
            },
            if *downloading.read() { "Downloading…" } else { "{file_name} ({size_kb} KB)" }
        }
        if !err().is_empty() {
            span { class: "ml-2 text-red-600 dark:text-red-300", "{err}" }
        }
    }
}

// PMS-729 phase 2 §7 slice A / I14: portal search page --------------------

/// Response body of `GET /portal/search`. Every field defaults so a
/// partial payload still decodes.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
struct PortalSearchPayload {
    #[serde(default)]
    tickets: Vec<PortalSearchHit>,
    #[serde(default)]
    invoices: Vec<PortalSearchHit>,
    #[serde(default)]
    quotes: Vec<PortalSearchHit>,
    #[serde(default)]
    kb_articles: Vec<PortalSearchHit>,
    #[serde(default)]
    counts: PortalSearchCounts,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
struct PortalSearchCounts {
    #[serde(default)]
    tickets: i64,
    #[serde(default)]
    invoices: i64,
    #[serde(default)]
    quotes: i64,
    #[serde(default)]
    kb_articles: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalSearchHit {
    id: uuid::Uuid,
    #[serde(default)]
    label: String,
    #[serde(default)]
    secondary: Option<String>,
}

/// Portal search page. Query lives in the URL as `?q=...` so a
/// customer can bookmark or share a search. Fires on every keystroke
/// (server trims + returns empty for a blank query, so an empty box
/// safely results in an empty page rather than a 400).
#[component]
pub fn PortalSearchPage() -> Element {
    let initial_q = crate::utils::url::current_query_param("q").unwrap_or_default();
    let mut query = use_signal(|| initial_q.clone());

    // Read the current input for the fetch closure. Storing in a
    // `Memo` so a keystroke re-runs the resource without re-rendering
    // every other subtree.
    let q_signal = query;
    let results = use_resource(move || {
        let q = q_signal.read().clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            if q.trim().is_empty() {
                return Ok(PortalSearchPayload::default());
            }
            let encoded = js_sys::encode_uri_component(&q)
                .as_string()
                .unwrap_or_default();
            crate::hooks::fetch::api::get_portal_authed::<PortalSearchPayload>(&format!(
                "/portal/search?q={encoded}"
            ))
            .await
        }
    });

    let snap = results.read_unchecked();
    let payload = match &*snap {
        None => None,
        Some(Ok(payload)) => Some(payload.clone()),
        Some(Err(_)) => Some(PortalSearchPayload::default()),
    };
    let is_loading = snap.is_none();

    rsx! {
        PortalLayout { title: "Search".to_string(),
            div { class: "mb-6 max-w-xl",
                label {
                    class: "block text-sm font-medium text-content mb-1",
                    r#for: "portal-search-input",
                    "Find a ticket, invoice, quote, or article"
                }
                input {
                    id: "portal-search-input",
                    r#type: "search",
                    class: "block w-full rounded-md border border-line bg-surface px-3 py-2 text-content focus:outline-none focus:ring-2 focus:ring-accent",
                    value: "{query.read()}",
                    placeholder: "Search across your account",
                    oninput: move |e: FormEvent| query.set(e.value()),
                }
            }

            if is_loading {
                Card {
                    p { class: "text-sm text-muted py-3 text-center", "Searching..." }
                }
            } else if let Some(payload) = payload {
                if query.read().trim().is_empty() {
                    Card {
                        p { class: "text-sm text-muted py-3 text-center",
                            "Type a query above to search your tickets, invoices, quotes, and knowledge-base articles."
                        }
                    }
                } else if payload.counts.tickets == 0
                    && payload.counts.invoices == 0
                    && payload.counts.quotes == 0
                    && payload.counts.kb_articles == 0
                {
                    Card {
                        p { class: "text-sm text-muted py-3 text-center",
                            "No matches. Try a different word or check your spelling."
                        }
                    }
                } else {
                    PortalSearchSections { payload }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalSearchSectionsProps {
    payload: PortalSearchPayload,
}

#[component]
fn PortalSearchSections(props: PortalSearchSectionsProps) -> Element {
    let p = &props.payload;
    rsx! {
        div { class: "space-y-6",
            if !p.tickets.is_empty() {
                PortalSearchSection {
                    title: "Tickets".to_string(),
                    hits: p.tickets.clone(),
                    total: p.counts.tickets,
                    kind: "ticket".to_string(),
                }
            }
            if !p.invoices.is_empty() {
                PortalSearchSection {
                    title: "Invoices".to_string(),
                    hits: p.invoices.clone(),
                    total: p.counts.invoices,
                    kind: "invoice".to_string(),
                }
            }
            if !p.quotes.is_empty() {
                PortalSearchSection {
                    title: "Quotes".to_string(),
                    hits: p.quotes.clone(),
                    total: p.counts.quotes,
                    kind: "quote".to_string(),
                }
            }
            if !p.kb_articles.is_empty() {
                PortalSearchSection {
                    title: "Knowledge base".to_string(),
                    hits: p.kb_articles.clone(),
                    total: p.counts.kb_articles,
                    kind: "kb".to_string(),
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalSearchSectionProps {
    title: String,
    hits: Vec<PortalSearchHit>,
    total: i64,
    kind: String,
}

#[component]
fn PortalSearchSection(props: PortalSearchSectionProps) -> Element {
    let showing = props.hits.len() as i64;
    let more = props.total.saturating_sub(showing);
    rsx! {
        Card {
            div { class: "flex items-center justify-between mb-3",
                h2 { class: "text-lg font-medium text-content", "{props.title}" }
                span { class: "text-sm text-muted",
                    if more > 0 { "{showing} of {props.total}" } else { "{props.total}" }
                }
            }
            ul { class: "space-y-2",
                for hit in props.hits.iter().cloned() {
                    PortalSearchHitRow { hit, kind: props.kind.clone() }
                }
            }
            if more > 0 {
                p { class: "mt-3 text-xs text-muted",
                    "{more} more result(s) not shown. Narrow the search to see them."
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalSearchHitRowProps {
    hit: PortalSearchHit,
    kind: String,
}

#[component]
fn PortalSearchHitRow(props: PortalSearchHitRowProps) -> Element {
    let id = props.hit.id.to_string();
    let target = match props.kind.as_str() {
        "ticket" => Route::PortalTicketDetail { id: id.clone() },
        "invoice" => Route::PortalInvoiceDetail { id: id.clone() },
        "quote" => Route::PortalQuoteDetail { id: id.clone() },
        _ => Route::PortalKB {},
    };
    rsx! {
        li { class: "border-l-2 border-accent pl-3",
            Link {
                to: target,
                class: "block text-sm font-medium text-content hover:text-accent",
                "{props.hit.label}"
            }
            if let Some(sub) = &props.hit.secondary {
                p { class: "text-xs text-muted", "{sub}" }
            }
        }
    }
}

// PMS-729 phase 2 §7 slice B / I8: authenticated portal forms ---------------

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalFormListItem {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// `GET /portal/forms` list page. Empty state renders honestly when
/// the MSP has published no portal-visible form.
#[component]
pub fn PortalFormListPage() -> Element {
    let list = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_portal_authed::<Vec<PortalFormListItem>>("/portal/forms")
            .await
            .ok()
    });
    let snap = list.read_unchecked();
    let rows: Vec<PortalFormListItem> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loading = snap.is_none();

    rsx! {
        PortalLayout { title: "Forms".to_string(),
            if loading {
                Card {
                    p { class: "text-sm text-muted py-4 text-center", "Loading forms..." }
                }
            } else if rows.is_empty() {
                Card {
                    p { class: "text-sm text-muted py-4 text-center",
                        "No forms have been published to the portal yet."
                    }
                }
            } else {
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for form in rows.iter().cloned() {
                        Link {
                            key: "{form.id}",
                            to: Route::PortalFormDetail { id: form.id.to_string() },
                            class: "block rounded-lg border border-line bg-surface p-4 hover:border-accent transition-colors",
                            h3 { class: "text-base font-semibold text-content mb-1",
                                "{form.name}"
                            }
                            if let Some(desc) = &form.description {
                                p { class: "text-sm text-muted", "{desc}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Client-facing view of a single form as `GET /portal/forms/{id}`
/// returns it. Field types are stringly-serialised so the SPA renders
/// them without pulling in the whole agent-side FieldType enum.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalFormDetail {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    fields: Vec<PortalFormField>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalFormField {
    #[serde(default)]
    name: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    help_text: Option<String>,
    #[serde(default)]
    field_type: String,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    min_length: Option<i32>,
    #[serde(default)]
    max_length: Option<i32>,
    #[serde(default)]
    options: Option<Vec<String>>,
}

#[derive(Props, Clone, PartialEq)]
pub struct PortalFormDetailPageProps {
    pub id: String,
}

#[component]
pub fn PortalFormDetailPage(props: PortalFormDetailPageProps) -> Element {
    let id_for_fetch = props.id.clone();
    let form_resource = use_resource(move || {
        let id = id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_portal_authed::<PortalFormDetail>(&format!(
                "/portal/forms/{id}"
            ))
            .await
            .ok()
        }
    });
    let snap = form_resource.read_unchecked();
    let form: Option<PortalFormDetail> = match &*snap {
        Some(Some(f)) => Some(f.clone()),
        _ => None,
    };

    let mut answers = use_signal(std::collections::HashMap::<String, String>::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut receipt = use_signal(String::new);
    let form_id = props.id.clone();

    rsx! {
        PortalLayout { title: form.as_ref().map(|f| f.name.clone()).unwrap_or_else(|| "Form".to_string()),
            div { class: "mb-6",
                Link {
                    to: Route::PortalFormList {},
                    class: "text-sm text-accent hover:opacity-90",
                    "Back to forms"
                }
            }
            match form {
                None => rsx! {
                    Card {
                        p { class: "text-sm text-muted py-4 text-center",
                            "Loading form..."
                        }
                    }
                },
                Some(f) if !receipt().is_empty() => {
                    let ticket = receipt();
                    rsx! {
                        Card {
                            div { class: "text-center py-6",
                                h2 { class: "text-lg font-semibold text-content", "Thanks - we got it." }
                                p { class: "mt-2 text-sm text-muted",
                                    "Your submission opened ticket "
                                    span { class: "font-medium text-content", "#{ticket}." }
                                }
                                Link {
                                    to: Route::PortalTicketList {},
                                    class: "mt-4 inline-block text-sm text-accent hover:opacity-90",
                                    "View your tickets"
                                }
                            }
                        }
                        // Prevent an accidental re-submit by hiding the
                        // form after the receipt is shown.
                        {
                            let _ = f;
                            rsx! { {} }
                        }
                    }
                }
                Some(f) => rsx! {
                    Card {
                        h2 { class: "text-xl font-bold text-content mb-2", "{f.name}" }
                        if let Some(desc) = &f.description {
                            p { class: "text-sm text-muted mb-4", "{desc}" }
                        }
                        div { class: "space-y-4",
                            for field in f.fields.iter().cloned() {
                                PortalFormFieldRow {
                                    field: field.clone(),
                                    value: answers.read().get(&field.name).cloned().unwrap_or_default(),
                                    onchange: {
                                        let name = field.name.clone();
                                        move |v: String| {
                                            let mut map = answers.read().clone();
                                            map.insert(name.clone(), v);
                                            answers.set(map);
                                        }
                                    },
                                }
                            }
                        }
                        if !error().is_empty() {
                            p { class: "mt-4 text-sm text-red-600 dark:text-red-300", "{error}" }
                        }
                        div { class: "mt-6 flex justify-end",
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: *submitting.read(),
                                loading: *submitting.read(),
                                onclick: {
                                    let form_id = form_id.clone();
                                    move |_| {
                                        if *submitting.read() {
                                            return;
                                        }
                                        submitting.set(true);
                                        error.set(String::new());
                                        let payload: serde_json::Value = serde_json::Value::Object(
                                            answers.read().iter()
                                                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                                                .collect()
                                        );
                                        let body = serde_json::json!({ "payload": payload });
                                        let form_id = form_id.clone();
                                        spawn(async move {
                                            #[cfg(feature = "web")]
                                            {
                                                match crate::hooks::fetch::api::post_portal_authed_typed::<serde_json::Value, _>(
                                                    &format!("/portal/forms/{form_id}/submit"),
                                                    &body,
                                                ).await {
                                                    Ok(resp) => {
                                                        let num = resp.get("ticket_number")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("")
                                                            .to_string();
                                                        receipt.set(num);
                                                    }
                                                    Err(e) => error.set(e.user_message()),
                                                }
                                            }
                                            submitting.set(false);
                                        });
                                    }
                                },
                                "Submit"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalFormFieldRowProps {
    field: PortalFormField,
    value: String,
    onchange: EventHandler<String>,
}

#[component]
fn PortalFormFieldRow(props: PortalFormFieldRowProps) -> Element {
    let f = &props.field;
    let field_type = f.field_type.as_str();
    let value = props.value.clone();
    let field_name = f.name.clone();
    let onchange = props.onchange;
    let label_class = "block text-sm font-medium text-content mb-1";
    let input_class = "block w-full rounded-md border border-line bg-surface px-3 py-2 text-content focus:outline-none focus:ring-2 focus:ring-accent";
    rsx! {
        div {
            label { class: "{label_class}", r#for: "{field_name}",
                "{f.label}"
                if f.is_required {
                    span { class: "text-red-600 dark:text-red-300 ml-1", "*" }
                }
            }
            if let Some(help) = &f.help_text {
                p { class: "text-xs text-muted mb-1", "{help}" }
            }
            match field_type {
                "textarea" => rsx! {
                    textarea {
                        id: "{field_name}",
                        class: "{input_class}",
                        rows: 4,
                        value: "{value}",
                        oninput: move |e: FormEvent| onchange.call(e.value()),
                    }
                },
                "select" => {
                    let options = f.options.clone().unwrap_or_default();
                    rsx! {
                        select {
                            id: "{field_name}",
                            class: "{input_class}",
                            value: "{value}",
                            onchange: move |e: FormEvent| onchange.call(e.value()),
                            option { value: "", disabled: true, selected: value.is_empty(), "Choose..." }
                            for opt in options {
                                option { value: "{opt}", "{opt}" }
                            }
                        }
                    }
                },
                "checkbox" => rsx! {
                    input {
                        id: "{field_name}",
                        r#type: "checkbox",
                        class: "h-4 w-4 rounded border-line text-accent focus:ring-accent",
                        checked: value == "true",
                        onchange: move |e: FormEvent| {
                            onchange.call(if e.value() == "true" { "true".to_string() } else { "false".to_string() })
                        },
                    }
                },
                "date" => rsx! {
                    input {
                        id: "{field_name}",
                        r#type: "date",
                        class: "{input_class}",
                        value: "{value}",
                        oninput: move |e: FormEvent| onchange.call(e.value()),
                    }
                },
                _ => rsx! {
                    input {
                        id: "{field_name}",
                        r#type: "text",
                        class: "{input_class}",
                        value: "{value}",
                        oninput: move |e: FormEvent| onchange.call(e.value()),
                    }
                },
            }
        }
    }
}

//! Client portal pages

use dioxus::prelude::*;

use crate::components::{
    invoice_status_badge, Badge, BadgeVariant, BannerTone, BookIcon, Button, ButtonVariant, Card,
    CurrencyIcon, IconSize, PlusIcon, PortalLayout, SearchInput, StatusBanner, Table, TableBody,
    TableCell, TableEmptyRow, TableHead, TableHeader, TableRow,
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
    // MAPPS-403: the fabricated "Welcome back, Bob" name, the demo stat cards
    // (Open Tickets / Pending Invoices / Outstanding Balance), and the demo
    // ticket / invoice lists were removed so no invented counts or fake
    // TKT-/INV- rows reach a real client. Wiring these blocks to the portal
    // API is backend-dependent and deferred (per the MAPPS-403 decision);
    // until then the page shows only honest, non-fabricated content.
    //
    // MAPPS-357: N/A because this page still fetches nothing. With only a
    // static greeting, a neutral placeholder, and navigation Links below,
    // there is no primary resource that could fail during an outage and no
    // mutating control to disable.
    rsx! {
        PortalLayout {
            // Welcome section. The portal identity is a `contacts` row behind
            // the portal session token, not the agent `use_auth` CurrentUser,
            // so no signed-in display name is available client-side here; the
            // greeting stays generic rather than inventing one.
            div { class: "mb-8",
                h1 { class: "text-2xl font-bold text-content",
                    "Welcome back"
                }
                p { class: "text-muted mt-1",
                    "Here's what's happening with your account."
                }
            }

            // Neutral placeholder in place of the removed demo stats and
            // lists: no fabricated numbers or rows until the portal API lands.
            Card {
                p { class: "text-sm text-muted",
                    "Your tickets and invoices will appear here."
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
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        PlusIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Submit New Ticket" }
                    }
                    Link {
                        to: Route::PortalKB {},
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        BookIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Browse Knowledge Base" }
                    }
                    Link {
                        to: Route::PortalInvoiceList {},
                        class: "flex items-center p-4 bg-accent-50 dark:bg-accent-900/20 rounded-lg hover:bg-accent-100 dark:hover:bg-accent-900/40 transition-colors",
                        CurrencyIcon { class: "h-6 w-6 text-accent mr-3".to_string() }
                        span { class: "font-medium text-accent-900 dark:text-accent-100", "Pay Invoice" }
                    }
                }
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
        // MAPPS-528: read every page of the thread. The old `per_page=200`
        // was clamped to 100 by the server, so a long thread lost its oldest
        // notes with nothing on screen to say so.
        crate::hooks::fetch::api::get_all_portal_authed::<PortalNote>(&format!(
            "/portal/tickets/{id_for_fetch}/notes"
        ))
        .await
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
                                    // MAPPS-610: agents now write notes with a
                                    // Markdown toolbar, so a public note reaches
                                    // a customer as Markdown. Rendered plain, it
                                    // would show them the asterisks.
                                    //
                                    // `mentions: false`: a contact has no
                                    // business resolving staff handles, and
                                    // `/auth/users` is manager-gated, so the
                                    // lookup would fail anyway.
                                    crate::components::Markdown {
                                        content: note.content.clone(),
                                        mentions: false,
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
                    // MAPPS-582: raw textarea, so it strips the invisibles
                    // itself rather than inheriting it from `Textarea`.
                    oninput: move |e: FormEvent| {
                        draft.set(crate::utils::text::strip_invisible(&e.value()))
                    },
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

/// `PayInvoiceResponse` from mokosh-server (`src/modules/billing/models.rs`),
/// the body of `POST /api/v1/portal/invoices/{id}/pay`. The URL is a hosted
/// checkout session the server minted with the tenant's own gateway
/// credentials; the SPA can only follow it (MAPPS-523).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalPayResponse {
    #[serde(default)]
    checkout_url: String,
}

/// Statuses the server refuses to mint a checkout session for
/// (`create_invoice_checkout_session`: `Void | WrittenOff` is a 409). The Pay
/// control is hidden for them rather than offered and then rejected.
const UNPAYABLE_INVOICE_STATUSES: &[&str] = &["void", "written_off"];

/// Does this invoice still owe money? `balance_due` arrives as a decimal
/// string and defaults to empty on a thinner payload, so an unreadable value
/// hides the control (the server would refuse anyway) and is logged rather
/// than passed off as a zero balance.
fn has_outstanding_balance(invoice_id: &str, balance_due: &str) -> bool {
    let raw = balance_due.trim();
    if raw.is_empty() {
        return false;
    }
    match raw.parse::<f64>() {
        Ok(amount) => amount > 0.0,
        Err(e) => {
            tracing::warn!(
                "portal invoice {invoice_id}: balance_due {raw:?} is not a number ({e}); \
                 hiding the Pay control"
            );
            false
        }
    }
}

/// The pay endpoint's refusals are specific and actionable ("Invoice INV-0007
/// cannot be paid in status 'void'", "No active payment provider is configured
/// for this account"), so they are shown verbatim. `user_message()` would
/// replace a 404 with "The requested resource was not found." and the caller
/// would learn nothing (MAPPS-523).
#[cfg(feature = "web")]
fn pay_refusal_message(e: &crate::hooks::fetch::api::ApiError) -> String {
    use crate::hooks::fetch::api::ApiError;
    match e {
        ApiError::Status { message, .. } if !message.trim().is_empty() => {
            message.trim().to_string()
        }
        other => other.user_message(),
    }
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

    // MAPPS-523: Pay Now. `paying` blocks a double mint while the POST is in
    // flight; `pay_error` carries the server's own refusal text.
    let mut paying = use_signal(|| false);
    let mut pay_error = use_signal(String::new);
    // The server's `success_url` is this page with `?paid=1`
    // (mokosh-server `portal/routes.rs`, `pay_invoice`). The banner is a
    // receipt for the redirect, not the invoice's payment state: the webhook
    // reconciles that, and it may not have landed yet.
    let payment_received = crate::utils::url::current_query_param("paid").as_deref() == Some("1");
    // MAPPS-357: block the mint while the server is unreachable so a click
    // cannot silently fail.
    let can_mutate = crate::hooks::use_can_mutate();

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

    let id_for_pay = props.id.clone();
    let handle_pay = move |_| {
        if *paying.read() {
            return;
        }
        paying.set(true);
        pay_error.set(String::new());
        let id = id_for_pay.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_portal_authed_typed::<PortalPayResponse, _>(
                    &format!("/portal/invoices/{id}/pay"),
                    &serde_json::json!({}),
                )
                .await
                {
                    // `safe_href` keeps a `javascript:` / `data:` URL out of a
                    // navigation even though this one comes from our own
                    // server (MAPPS-149's allowlist, same rule).
                    Ok(resp) => match crate::utils::url::safe_href(&resp.checkout_url) {
                        Some(url) => {
                            if let Err(e) = crate::platform::location::set_href(&url) {
                                pay_error.set(format!("Could not open the payment page: {e}"));
                            }
                        }
                        None => pay_error.set(
                            "The payment page address was not one this app can open.".to_string(),
                        ),
                    },
                    Err(e) => pay_error.set(pay_refusal_message(&e)),
                }
            }
            paying.set(false);
        });
    };

    rsx! {
        PortalLayout {
            div { class: "mb-6",
                Link {
                    to: Route::PortalInvoiceList {},
                    class: "text-sm text-accent hover:opacity-90",
                    "Back to invoices"
                }
            }

            if payment_received {
                StatusBanner { tone: BannerTone::Success, class: "mb-6",
                    "Payment received. Thank you. It can take a moment to show against this invoice."
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
                    // MAPPS-523: offer Pay Now only where the server would
                    // actually mint a checkout session, and never directly
                    // under the receipt banner: the webhook has not
                    // necessarily cleared `balance_due` yet, so a second
                    // "Pay {amount}" there reads as an unpaid invoice and
                    // invites paying twice.
                    let can_pay = !payment_received
                        && !UNPAYABLE_INVOICE_STATUSES.contains(&status_raw)
                        && has_outstanding_balance(&props.id, &inv.balance_due);
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

                            // MAPPS-523: the control the "Pay Now" invoice
                            // email has been asking clients to use. The
                            // checkout session is minted server-side; this
                            // only follows the URL it returns.
                            if can_pay {
                                div { class: "border-t border-line mt-6 pt-4",
                                    if !pay_error().is_empty() {
                                        StatusBanner { tone: BannerTone::Error, class: "mb-3", "{pay_error}" }
                                    }
                                    div { class: "flex justify-end",
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            disabled: *paying.read() || !can_mutate,
                                            loading: *paying.read(),
                                            title: (!can_mutate).then(|| "Can't start a payment while the server is unreachable".to_string()),
                                            onclick: handle_pay,
                                            CurrencyIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                            "Pay {amount_due}"
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
        crate::hooks::fetch::api::get_all_portal_authed::<PortalKbArticle>("/portal/kb")
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
        crate::hooks::fetch::api::get_all_portal_authed::<QuoteResponse>("/portal/quotes")
            .await
            .ok()
    });

    let snap = quotes_resource.read_unchecked();
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<QuoteResponse> = match &*snap {
        Some(Some(rows)) => rows.clone(),
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
                                    class: "w-full rounded-md border border-line bg-surface p-2 text-sm",
                                    rows: 3,
                                    placeholder: "Anything you want to add (optional)",
                                    value: "{notes}",
                                    // MAPPS-582: raw textarea; see the reply
                                    // draft above.
                                    oninput: move |e: FormEvent| {
                                        notes.set(crate::utils::text::strip_invisible(&e.value()))
                                    },
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

/// MAPPS-523: the Pay control's gate. `PortalInvoiceDetailPage` renders the
/// button only where mokosh-server would actually mint a checkout session, so
/// the two conditions it splits on are pinned here rather than left to a
/// visual check. The component itself needs a running virtual DOM and a portal
/// session to render, which is why the predicate is a free function.
#[cfg(test)]
mod pay_control_gate {
    use super::{has_outstanding_balance, UNPAYABLE_INVOICE_STATUSES};

    const ID: &str = "2f1c2f1e-0000-4000-8000-00000000abcd";

    #[test]
    fn only_a_positive_balance_is_payable() {
        assert!(has_outstanding_balance(ID, "1200.00"));
        assert!(has_outstanding_balance(ID, " 0.01 "));
        assert!(!has_outstanding_balance(ID, "0.00"));
        assert!(!has_outstanding_balance(ID, "-5.00"));
    }

    #[test]
    fn an_unreadable_balance_hides_the_control() {
        // `balance_due` is `#[serde(default)]`, so a thinner payload leaves it
        // empty. Neither that nor junk may read as "money is owed".
        assert!(!has_outstanding_balance(ID, ""));
        assert!(!has_outstanding_balance(ID, "   "));
        assert!(!has_outstanding_balance(ID, "n/a"));
    }

    /// `create_invoice_checkout_session` (mokosh-server
    /// `src/modules/billing/service.rs`) 409s on exactly `Void | WrittenOff`.
    /// A status this list misses would be offered and then refused.
    #[test]
    fn the_terminal_statuses_match_the_servers_refusal() {
        assert_eq!(UNPAYABLE_INVOICE_STATUSES, &["void", "written_off"]);
        for payable in ["draft", "pending", "sent", "overdue", "partially_paid"] {
            assert!(
                !UNPAYABLE_INVOICE_STATUSES.contains(&payable),
                "{payable} is payable server-side and must keep the control"
            );
        }
    }
}

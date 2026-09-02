//! Credit notes (MAPPS-638): the correction path PMS-953 added to the server,
//! given a UI.
//!
//! An issued invoice cannot be edited, because the customer holds a copy. A
//! credit note is the only correction the product offers, and until this
//! module it was reachable only by calling the API directly. Three pieces:
//!
//! - the list (`/credit-notes`) and the detail (`/credit-notes/:id`), routed
//!   pages beside the invoice pages;
//! - [`CreditNoteFormModal`], the create form, mounted from the invoice detail
//!   in `billing.rs`, because a credit note is always about one invoice and
//!   the server refuses one without;
//! - the cards the invoice detail shows: the credit notes raised against it.
//!
//! Conventions follow `billing.rs`: page-local `Deserialize` structs with
//! `#[serde(default)]` on every field, money as the server's decimal strings
//! rendered through `format_money_str`, and every rule the server enforces
//! mirrored in the form so it reads as a field error rather than a 400.
//! [`credit_note_math`] holds the arithmetic as pure functions, because the
//! host-side `cargo test --lib` has no browser to drive the form with.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    credit_note_status_badge, use_page_title, Badge, Button, ButtonVariant, Card, DataTable,
    ErrorBanner, PageHeader, Select, SelectOption, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
};
use crate::utils::money::format_money_str;
use crate::utils::Paginated;
use crate::Route;

/// Rows per page for the list.
const PER_PAGE: usize = 25;

/// `CreditNoteResponse`. `lines` is `Some` on `GET /credit-notes/{id}` and
/// `None` on the list rollup, like the invoice.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct RemoteCreditNote {
    pub(crate) id: uuid::Uuid,
    #[serde(default)]
    pub(crate) credit_note_number: String,
    #[serde(default)]
    pub(crate) company_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub(crate) company_name: Option<String>,
    #[serde(default)]
    pub(crate) invoice_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub(crate) invoice_number: Option<String>,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) issue_date: Option<String>,
    #[serde(default)]
    pub(crate) reason: String,
    #[serde(default)]
    pub(crate) subtotal: String,
    #[serde(default)]
    pub(crate) tax_amount: String,
    #[serde(default)]
    pub(crate) total: String,
    #[serde(default)]
    pub(crate) currency: Option<String>,
    #[serde(default)]
    pub(crate) notes: Option<String>,
    #[serde(default)]
    pub(crate) voided_at: Option<String>,
    #[serde(default)]
    pub(crate) lines: Option<Vec<RemoteCreditNoteLine>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct RemoteCreditNoteLine {
    pub(crate) id: uuid::Uuid,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) quantity: String,
    #[serde(default)]
    pub(crate) unit_price: String,
    #[serde(default)]
    pub(crate) total: String,
}

/// The slice of `InvoiceResponse` the credit-note detail needs: enough to say
/// what was credited against what, and what is left owing now.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteInvoiceBalance {
    #[serde(default)]
    invoice_number: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    total: String,
    #[serde(default)]
    amount_paid: String,
    #[serde(default)]
    amount_credited: String,
    #[serde(default)]
    balance_due: String,
}

// ============================================================================
// Arithmetic, kept pure so it can be tested off-web
// ============================================================================

/// The rules the server enforces in `create_credit_note`, mirrored here so the
/// form can refuse before the request is sent. Every function takes the
/// decimal strings the fields hold and answers with the same message shape
/// the field would show.
pub(crate) mod credit_note_math {
    use rust_decimal::Decimal;
    use std::str::FromStr;

    /// What can still be credited: the invoice total less what is already
    /// credited. Deliberately NOT less what has been paid, matching the
    /// server: a paid invoice can be credited in full, which is exactly the
    /// case where the customer is owed money back. `None` when either number
    /// does not parse, which the caller reports rather than treating as zero.
    pub(crate) fn remaining_to_credit(
        invoice_total: &str,
        amount_credited: &str,
    ) -> Option<Decimal> {
        let total = Decimal::from_str(invoice_total.trim()).ok()?;
        let credited = Decimal::from_str(amount_credited.trim()).ok()?;
        Some((total - credited).max(Decimal::ZERO))
    }

    /// A line's amount, or `None` when either field is not a positive number.
    /// Strictly positive on both, as the server requires: the document as a
    /// whole is the credit, so a negative or zero line inside it is a charge
    /// hidden in a credit.
    pub(crate) fn line_amount(quantity: &str, unit_price: &str) -> Option<Decimal> {
        let qty = Decimal::from_str(quantity.trim()).ok()?;
        let price = Decimal::from_str(unit_price.trim()).ok()?;
        if qty <= Decimal::ZERO || price <= Decimal::ZERO {
            return None;
        }
        Some(qty * price)
    }

    /// Subtotal over lines that parse. A line that does not parse contributes
    /// nothing here and is reported on its own field by the form.
    pub(crate) fn subtotal(lines: &[(String, String)]) -> Decimal {
        lines.iter().filter_map(|(q, p)| line_amount(q, p)).sum()
    }

    /// The tax field: blank is zero, anything else must parse and not be
    /// negative. `Err` carries the message the field shows.
    pub(crate) fn tax(raw: &str) -> Result<Decimal, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Decimal::ZERO);
        }
        let value = Decimal::from_str(trimmed).map_err(|_| "Tax must be a number".to_string())?;
        if value < Decimal::ZERO {
            return Err("Tax cannot be negative".to_string());
        }
        Ok(value)
    }

    /// Why the document as a whole cannot be sent, if it cannot. The two
    /// document-level rules the server has beyond the per-field ones: a
    /// credit note must credit something, and not more than is left.
    pub(crate) fn document_error(total: Decimal, remaining: Decimal) -> Option<String> {
        if total <= Decimal::ZERO {
            return Some("A credit note must credit something: add at least one line with a positive amount.".to_string());
        }
        if total > remaining {
            return Some(format!(
                "This credit note comes to {} but only {} is left to credit on this invoice.",
                crate::utils::money::format_money(total),
                crate::utils::money::format_money(remaining)
            ));
        }
        None
    }
}

// ============================================================================
// List
// ============================================================================

/// Credit note list page. GET `/credit-notes` with an optional status filter,
/// server-paginated. Finance-gated like every billing page.
#[component]
pub fn CreditNoteListPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Credit Notes");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Credit Notes" } };
    }

    rsx! { CreditNoteListBody {} }
}

#[component]
fn CreditNoteListBody() -> Element {
    let mut status_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("issued", "Issued"),
        SelectOption::new("void", "Void"),
    ];

    let status_text = status_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let status_for_resource = status_text.clone();
    let notes_resource = use_resource(move || {
        let status = status_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back.
            let _reachable = crate::hooks::use_server_reachable();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/credit-notes?page={current_page}&per_page={PER_PAGE}");
            if !status.is_empty() {
                path.push_str(&format!("&status={status}"));
            }
            crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteCreditNote>>(&path, &token)
                .await
                .ok()
        }
    });

    let snap = notes_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteCreditNote>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Credit Notes".to_string() }
        };
    }

    let filtered = !status_text.is_empty();

    rsx! {
        PageHeader {
            title: "Credit Notes",
            subtitle: "Corrections issued against sent invoices",
            actions: rsx! {
                Link {
                    to: Route::InvoiceList {},
                    Button { variant: ButtonVariant::Secondary, "Invoices" }
                }
            },
        }

        // No "New credit note" here on purpose: a credit note is raised from
        // the invoice it corrects, on that invoice's page, because the server
        // refuses one without an invoice and choosing the invoice is the form.
        p { class: "mb-4 text-sm text-muted",
            "To raise a credit note, open the sent invoice it corrects and choose Create Credit Note."
        }

        Card { class: "mb-4",
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                Select {
                    name: "credit_note_status",
                    label: "Status",
                    options: status_options,
                    value: status_text.clone(),
                    onchange: move |e: FormEvent| {
                        status_filter.set(e.value());
                        page.set(1);
                    },
                }
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load credit notes. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 6,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Number" }
                        TableHeader { "Invoice" }
                        TableHeader { "Date" }
                        TableHeader { "Reason" }
                        TableHeader { class: "text-right", "Total" }
                        TableHeader { "Status" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 5 }
                } else if rows.is_empty() {
                    if filtered {
                        TableEmpty {
                            columns: 6,
                            message: "No credit notes match this filter.".to_string(),
                        }
                    } else {
                        TableEmpty {
                            columns: 6,
                            title: "No credit notes yet".to_string(),
                            description: "A credit note corrects an invoice that has already been sent. Open that invoice to raise one.".to_string(),
                            actions: rsx! {
                                Link {
                                    to: Route::InvoiceList {},
                                    Button { variant: ButtonVariant::Primary, "Go to Invoices" }
                                }
                            },
                        }
                    }
                } else {
                    TableBody {
                        for note in rows.iter().cloned() {
                            CreditNoteRow { key: "{note.id}", note }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CreditNoteRow(note: RemoteCreditNote) -> Element {
    let (status_variant, status_label) = credit_note_status_badge(&note.status);
    let navigator = use_navigator();
    let id = note.id.to_string();
    let id_for_click = id.clone();
    let invoice_id = note.invoice_id.map(|i| i.to_string());
    let invoice_number = note.invoice_number.clone().unwrap_or_default();
    let date = note.issue_date.clone().unwrap_or_default();
    let reason = one_line(&note.reason, 80);
    let total = format_money_str(&note.total);

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::CreditNoteDetail { id: id_for_click.clone() }); },
            TableCell {
                Link {
                    to: Route::CreditNoteDetail { id: id.clone() },
                    class: "font-medium text-accent hover:opacity-90",
                    "{note.credit_note_number}"
                }
            }
            TableCell {
                if let Some(iid) = invoice_id.clone() {
                    Link {
                        to: Route::InvoiceDetail { id: iid },
                        class: "text-accent hover:opacity-90",
                        // Stop the row navigation so the click lands on the
                        // invoice rather than the credit note.
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        if invoice_number.is_empty() { "View invoice" } else { "{invoice_number}" }
                    }
                } else {
                    span { class: "text-subtle", "-" }
                }
            }
            TableCell {
                if date.is_empty() {
                    span { class: "text-subtle", "-" }
                } else {
                    "{date}"
                }
            }
            TableCell { class: "max-w-xs",
                span { class: "block truncate", title: "{note.reason}", "{reason}" }
            }
            TableCell { class: "text-right font-medium", "{total}" }
            TableCell { Badge { variant: status_variant, "{status_label}" } }
        }
    }
}

/// `voided_at` as the server sends it (RFC 3339), in the user's own format
/// and zone; the raw string if it does not parse.
fn format_voided_at(raw: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => {
            crate::utils::datetime::format_user_datetime(dt.with_timezone(&chrono::Utc), None)
        }
        Err(_) => raw.to_string(),
    }
}

/// The first line of a reason, cut to `max` characters for a table cell. The
/// full text is on the cell's `title` and on the detail page.
fn one_line(text: &str, max: usize) -> String {
    let first = text.lines().next().unwrap_or("").trim();
    if first.chars().count() <= max {
        return first.to_string();
    }
    let cut: String = first.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

// ============================================================================
// Detail
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct CreditNoteDetailPageProps {
    pub id: String,
}

/// Credit note detail. GET `/credit-notes/{id}` with lines, plus the invoice
/// it corrects so the balance it changed is on the same page.
#[component]
pub fn CreditNoteDetailPage(props: CreditNoteDetailPageProps) -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);
    use_page_title("Credit Note");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Credit Note" } };
    }
    rsx! { CreditNoteDetailBody { id: props.id.clone() } }
}

#[component]
fn CreditNoteDetailBody(id: String) -> Element {
    let id_for_resource = id.clone();
    let mut note_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<RemoteCreditNote>(&format!("/credit-notes/{id}"))
                .await
                .ok()
        }
    });

    let snap = note_resource.read_unchecked();
    let note = match &*snap {
        Some(Some(n)) => Some(n.clone()),
        _ => None,
    };
    let invoice_id = note
        .as_ref()
        .and_then(|n| n.invoice_id)
        .map(|i| i.to_string())
        .unwrap_or_default();

    // The corrected invoice, fetched once the note says which one. Restarted
    // alongside the note after a void, so the balance on this page moves.
    let invoice_for_resource = invoice_id.clone();
    let mut invoice_resource = use_resource(move || {
        let invoice_id = invoice_for_resource.clone();
        async move {
            if invoice_id.is_empty() {
                return None;
            }
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<RemoteInvoiceBalance>(&format!(
                "/invoices/{invoice_id}"
            ))
            .await
            .ok()
        }
    });

    let mut busy = use_signal(|| false);
    let mut action_error = use_signal(String::new);
    let mut confirming_void = use_signal(|| false);
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();

    let header_title = match &note {
        Some(n) => format!("Credit Note {}", n.credit_note_number),
        None => "Credit Note".to_string(),
    };
    use_page_title(&header_title);

    let status = note.as_ref().map(|n| n.status.clone()).unwrap_or_default();
    let is_issued = status == "issued";

    let id_for_void = id.clone();
    let on_confirm_void = move |_: ()| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        let path = format!("/credit-notes/{id_for_void}/void");
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    &path,
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Credit note voided",
                        );
                        note_resource.restart();
                        invoice_resource.restart();
                    }
                    Err(err) => action_error.set(format!("Could not void credit note: {err}")),
                }
            }
            busy.set(false);
            confirming_void.set(false);
        });
    };

    let id_for_pdf = id.clone();
    let number_for_pdf = note
        .as_ref()
        .map(|n| n.credit_note_number.clone())
        .unwrap_or_default();
    let mut downloading = use_signal(|| false);
    let on_download = move |_| {
        if *downloading.read() {
            return;
        }
        downloading.set(true);
        action_error.set(String::new());
        let path = format!("/credit-notes/{id_for_pdf}/pdf");
        let fallback = if number_for_pdf.is_empty() {
            "credit-note.pdf".to_string()
        } else {
            format!("{number_for_pdf}.pdf")
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::get_authed_bytes(&path).await {
                    Ok((bytes, name)) => {
                        let filename = name.unwrap_or(fallback);
                        if let Err(err) =
                            crate::utils::download::save_bytes_as_file(&bytes, &filename)
                        {
                            action_error.set(format!("Could not save the PDF: {err}"));
                        }
                    }
                    Err(err) => action_error.set(format!("Could not download the PDF: {err}")),
                }
            }
            downloading.set(false);
        });
    };

    let fetch_failed = matches!(*snap, Some(None));
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Credit Note".to_string() }
        };
    }

    let act_err = action_error.read().clone();
    let invoice_snap = invoice_resource.read_unchecked();
    let invoice = match &*invoice_snap {
        Some(Some(inv)) => Some(inv.clone()),
        _ => None,
    };

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_void(),
            title: "Void credit note".to_string(),
            message: "Void this credit note? Its amount stops counting against the invoice, so the invoice's balance due goes back up by that amount. The credit note is kept on record and cannot be reinstated.".to_string(),
            confirm_text: "Void".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *busy.read(),
            onconfirm: on_confirm_void,
            oncancel: move |_| {
                if !*busy.read() {
                    confirming_void.set(false);
                }
            },
        }
        PageHeader {
            title: "{header_title}",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Credit Notes", Route::CreditNoteList {}, &header_title),
                }
            },
            actions: rsx! {
                if note.is_some() {
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *downloading.read(),
                        onclick: on_download,
                        "Download PDF"
                    }
                }
                if is_issued {
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *busy.read(),
                        // MAPPS-357: block voiding while the server is down.
                        disabled: !can_mutate,
                        title: if can_mutate {
                            "Voids this credit note and keeps it on record. The invoice's balance due goes back up.".to_string()
                        } else {
                            "Can't void while the server is unreachable".to_string()
                        },
                        onclick: move |_| {
                            if !*busy.read() {
                                confirming_void.set(true);
                            }
                        },
                        "Void"
                    }
                }
            },
        }

        // A credit note is issued or voided, never edited, for the reason the
        // invoice it corrects has no edit: the customer holds a copy of both.
        if let Some(n) = note.as_ref() {
            div {
                class: "mb-3 text-xs text-muted bg-surface-2 border border-line rounded-md px-3 py-2",
                if n.status == "void" {
                    "This credit note has been voided and is kept on record. Its amount no longer counts against the invoice."
                } else {
                    "This credit note was issued when it was created and is a finalized record. It can't be edited; if it is wrong, void it and raise another."
                }
            }
        }

        if !act_err.is_empty() {
            ErrorBanner { class: "mb-3", "{act_err}" }
        }

        match &*snap {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(None) => rsx! {
                Card {
                    div { class: "py-8 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load credit note." }
                        Link {
                            to: Route::CreditNoteList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to credit notes"
                        }
                    }
                }
            },
            Some(Some(n)) => {
                let (status_variant, status_label) = credit_note_status_badge(&n.status);
                let lines = n.lines.clone().unwrap_or_default();
                let currency = n.currency.clone().unwrap_or_default();
                let notes = n.notes.clone().unwrap_or_default();
                let issue_date = n.issue_date.clone().unwrap_or_default();
                let voided_at = n.voided_at.clone().unwrap_or_default();
                let subtotal = format_money_str(&n.subtotal);
                let tax_amount = format_money_str(&n.tax_amount);
                let total = format_money_str(&n.total);
                let company_id = n.company_id.map(|c| c.to_string());
                let company_name = n
                    .company_name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "View company".to_string());
                let invoice_number = n
                    .invoice_number
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "View invoice".to_string());
                rsx! {
                    div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                        div { class: "lg:col-span-2",
                            Card {
                                div { class: "flex justify-between mb-8",
                                    div {
                                        h2 { class: "text-2xl font-bold text-content", "CREDIT NOTE" }
                                        p { class: "text-muted", "{n.credit_note_number}" }
                                    }
                                    div { class: "text-right",
                                        div { class: "mb-2",
                                            span { class: "text-sm text-muted", "Issue Date: " }
                                            span { class: "font-medium",
                                                if issue_date.is_empty() { "-" } else { "{issue_date}" }
                                            }
                                        }
                                        if !invoice_id.is_empty() {
                                            div {
                                                span { class: "text-sm text-muted", "Corrects: " }
                                                Link {
                                                    to: Route::InvoiceDetail { id: invoice_id.clone() },
                                                    class: "font-medium text-accent hover:opacity-90",
                                                    "{invoice_number}"
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6 text-sm",
                                    h3 { class: "font-medium text-muted mb-1", "Reason" }
                                    p { class: "text-content whitespace-pre-line", "{n.reason}" }
                                }

                                Table {
                                    TableHead {
                                        TableRow {
                                            TableHeader { "Description" }
                                            TableHeader { class: "text-right", "Qty" }
                                            TableHeader { class: "text-right", "Unit Price" }
                                            TableHeader { class: "text-right", "Amount" }
                                        }
                                    }
                                    if lines.is_empty() {
                                        TableEmpty { columns: 4, message: "This credit note has no line items.".to_string() }
                                    } else {
                                        TableBody {
                                            for line in lines.iter().cloned() {
                                                TableRow { key: "{line.id}",
                                                    TableCell { "{line.description}" }
                                                    TableCell { class: "text-right", "{line.quantity}" }
                                                    TableCell { class: "text-right", "{format_money_str(&line.unit_price)}" }
                                                    TableCell { class: "text-right font-medium", "{format_money_str(&line.total)}" }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mt-8 border-t border-line pt-4",
                                    div { class: "flex justify-end",
                                        div { class: "w-64 space-y-2",
                                            div { class: "flex justify-between",
                                                span { class: "text-muted", "Subtotal" }
                                                span { "{subtotal}" }
                                            }
                                            div { class: "flex justify-between",
                                                span { class: "text-muted", "Tax" }
                                                span { "{tax_amount}" }
                                            }
                                            div { class: "flex justify-between text-lg font-bold pt-2 border-t border-line",
                                                span { "Total credited" }
                                                span { "{total}" }
                                            }
                                        }
                                    }
                                }

                                if !notes.is_empty() {
                                    div { class: "mt-6 text-sm",
                                        h3 { class: "font-medium text-muted mb-1", "Notes" }
                                        p { class: "text-content whitespace-pre-line", "{notes}" }
                                    }
                                }
                            }
                        }

                        div { class: "space-y-6",
                            Card { title: "Status",
                                div { class: "space-y-4",
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-muted", "Status" }
                                        Badge { variant: status_variant, "{status_label}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Total credited" }
                                        span { class: "text-lg font-bold", "{total}" }
                                    }
                                    if !voided_at.is_empty() {
                                        div { class: "flex justify-between",
                                            span { class: "text-muted", "Voided" }
                                            span { class: "font-medium", "{format_voided_at(&voided_at)}" }
                                        }
                                    }
                                }
                            }

                            // The invoice's balance, on this page, so a void
                            // is seen to do something.
                            Card { title: "Invoice",
                                if let Some(inv) = invoice.as_ref() {
                                    {
                                        let (inv_variant, inv_label) = crate::components::invoice_status_badge(&inv.status);
                                        rsx! {
                                            div { class: "space-y-4",
                                                div { class: "flex justify-between items-center",
                                                    span { class: "text-muted", "Invoice" }
                                                    Link {
                                                        to: Route::InvoiceDetail { id: invoice_id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{inv.invoice_number}"
                                                    }
                                                }
                                                div { class: "flex justify-between items-center",
                                                    span { class: "text-muted", "Status" }
                                                    Badge { variant: inv_variant, "{inv_label}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted", "Total" }
                                                    span { class: "font-medium", "{format_money_str(&inv.total)}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted", "Paid" }
                                                    span { class: "font-medium", "{format_money_str(&inv.amount_paid)}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted", "Credited" }
                                                    span { class: "font-medium", "{format_money_str(&inv.amount_credited)}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-muted", "Balance Due" }
                                                    span { class: "text-lg font-bold", "{format_money_str(&inv.balance_due)}" }
                                                }
                                            }
                                        }
                                    }
                                } else if invoice_id.is_empty() {
                                    p { class: "text-sm text-muted", "This credit note is not linked to an invoice." }
                                } else {
                                    p { class: "text-sm text-muted", "Loading the invoice…" }
                                }
                            }

                            Card { title: "Details",
                                dl { class: "space-y-3 text-sm",
                                    if !currency.is_empty() {
                                        div { class: "flex justify-between",
                                            dt { class: "text-muted", "Currency" }
                                            dd { "{currency}" }
                                        }
                                    }
                                    if let Some(cid) = company_id.clone() {
                                        div { class: "flex justify-between",
                                            dt { class: "text-muted", "Company" }
                                            dd {
                                                Link {
                                                    to: Route::CompanyDetail { id: cid.clone() },
                                                    class: "text-accent hover:opacity-90",
                                                    "{company_name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::credit_note_math::*;
    use super::one_line;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    /// The server's cap is total minus credited and ignores what was paid: a
    /// paid invoice can still be credited in full.
    #[test]
    fn remaining_ignores_what_was_paid() {
        assert_eq!(remaining_to_credit("100.00", "0"), Some(d("100.00")));
        assert_eq!(remaining_to_credit("100.00", "30.00"), Some(d("70.00")));
        assert_eq!(remaining_to_credit("100.00", "100.00"), Some(Decimal::ZERO));
        // Over-credited cannot happen server-side, but a stale page could
        // show it; clamp rather than offer a negative cap.
        assert_eq!(remaining_to_credit("100.00", "120.00"), Some(Decimal::ZERO));
        assert_eq!(remaining_to_credit("", "0"), None);
        assert_eq!(remaining_to_credit("abc", "0"), None);
    }

    /// Both factors strictly positive, the server's rule: a zero or negative
    /// line is a charge hidden inside a credit.
    #[test]
    fn a_line_must_be_strictly_positive_on_both_sides() {
        assert_eq!(line_amount("2", "10.50"), Some(d("21.00")));
        assert_eq!(line_amount("0", "10"), None);
        assert_eq!(line_amount("1", "0"), None);
        assert_eq!(line_amount("-1", "10"), None);
        assert_eq!(line_amount("1", "-10"), None);
        assert_eq!(line_amount("", "10"), None);
        assert_eq!(line_amount("x", "10"), None);
    }

    #[test]
    fn subtotal_skips_lines_that_do_not_parse() {
        let lines = vec![
            ("1".to_string(), "10".to_string()),
            ("".to_string(), "10".to_string()),
            ("2".to_string(), "2.25".to_string()),
        ];
        assert_eq!(subtotal(&lines), d("14.50"));
    }

    #[test]
    fn tax_is_blank_or_a_non_negative_number() {
        assert_eq!(tax(""), Ok(Decimal::ZERO));
        assert_eq!(tax("  "), Ok(Decimal::ZERO));
        assert_eq!(tax("1.50"), Ok(d("1.50")));
        assert_eq!(tax("0"), Ok(Decimal::ZERO));
        assert!(tax("-1").is_err());
        assert!(tax("abc").is_err());
    }

    /// The two document-level rules: credit something, and not more than is
    /// left. Exactly the remaining amount is allowed, because that is how an
    /// invoice is voided by credit.
    #[test]
    fn the_document_must_credit_something_and_not_more_than_is_left() {
        assert!(document_error(Decimal::ZERO, d("100")).is_some());
        assert!(document_error(d("-5"), d("100")).is_some());
        assert!(document_error(d("100"), d("100")).is_none());
        assert!(document_error(d("50"), d("100")).is_none());
        let over = document_error(d("100.01"), d("100")).expect("over the cap");
        assert!(over.contains("left to credit"), "{over}");
    }

    #[test]
    fn a_reason_is_cut_to_one_line_for_the_table() {
        assert_eq!(one_line("short", 80), "short");
        assert_eq!(one_line("first line\nsecond", 80), "first line");
        let long = "a".repeat(100);
        let cut = one_line(&long, 80);
        assert_eq!(cut.chars().count(), 81);
        assert!(cut.ends_with('…'));
    }
}

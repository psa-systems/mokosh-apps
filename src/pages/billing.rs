//! Billing pages: invoices, payments, tax rates, and payment-gateway
//! config. Wired to the real `/api/v1` billing endpoints.
//!
//! Conventions mirror `src/pages/contacts.rs`:
//!   - page-local `Deserialize` structs (serde drops unknown fields, so
//!     they can grow without breaking decoding);
//!   - `#[serde(default)]` on every optional;
//!   - `active_tenant_generation()` read inside each `use_resource`
//!     closure so an org switch / token swap re-fetches;
//!   - list fetches require an access token up front; detail/mutation use
//!     the auto-authed wrappers;
//!   - loading / empty / error states match the contacts pages.
//!
//! Money: the server serialises `rust_decimal::Decimal` as a JSON string
//! (the crate's default serde impl), so every amount is mirrored as
//! `String` and rendered with a leading `$` via [`money`].

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize,
    InformationIcon, Modal, ModalSize, PageHeader, PlusIcon, Select, SelectOption, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::utils::Paginated;
use crate::Route;

/// Rows per page for the paginated billing list views.
const PER_PAGE: usize = 25;

// Money formatting is centralized in `crate::utils::money` (MAPPS-197).
// `format_money_str` parses the server's decimal string and renders it with
// grouped thousands + two decimals, matching projects and contracts.
use crate::utils::money::format_money_str;

/// Map the server's snake_case `InvoiceStatus` tag to a title-case label.
/// Unknown values fall through unchanged so future statuses don't vanish.
fn humanize_invoice_status(raw: &str) -> String {
    match raw {
        "draft" => "Draft".to_string(),
        "pending" => "Pending".to_string(),
        "sent" => "Sent".to_string(),
        "paid" => "Paid".to_string(),
        "partially_paid" => "Partially Paid".to_string(),
        "void" => "Void".to_string(),
        "written_off" => "Written Off".to_string(),
        other => other.to_string(),
    }
}

/// Badge colour for an invoice status tag.
fn invoice_status_variant(raw: &str) -> BadgeVariant {
    match raw {
        "paid" => BadgeVariant::Green,
        "sent" | "pending" => BadgeVariant::Blue,
        "partially_paid" => BadgeVariant::Yellow,
        "void" | "written_off" => BadgeVariant::Red,
        _ => BadgeVariant::Gray,
    }
}

/// Map the server's snake_case `PaymentMethod` tag to a readable label.
fn humanize_payment_method(raw: &str) -> String {
    match raw {
        "check" => "Check".to_string(),
        "credit_card" => "Credit Card".to_string(),
        "ach" => "ACH Transfer".to_string(),
        "wire" => "Wire Transfer".to_string(),
        "cash" => "Cash".to_string(),
        "other" => "Other".to_string(),
        other => other.to_string(),
    }
}

/// Build the `serde_json` null-or-string helper used by the create forms
/// (matches `contacts::optional_string`).
fn optional_string(value: &str) -> serde_json::Value {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

// ============================================================================
// Invoices
// ============================================================================

/// Subset of `InvoiceResponse` rendered in the list rollup. `lines` is
/// omitted on the list endpoint, so it is not modelled here.
/// A company option for the billing company pickers and for resolving a
/// `company_id` to a display name (PMS-186). Sourced from
/// `GET /api/v1/contacts/companies`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CompanyOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// Load the tenant's companies for the billing pickers (PMS-186).
/// Best-effort: an empty list on error so a form still renders.
async fn load_companies() -> Vec<CompanyOption> {
    crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>("/contacts/companies")
        .await
        .map(|p| p.data)
        .unwrap_or_default()
}

/// Build `[("", placeholder), (id, name), ...]` select options from a
/// loaded company list.
fn company_select_options(companies: &[CompanyOption], placeholder: &str) -> Vec<SelectOption> {
    let mut opts = vec![SelectOption::new("", placeholder)];
    opts.extend(
        companies
            .iter()
            .map(|c| SelectOption::new(c.id.to_string(), c.name.clone())),
    );
    opts
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteInvoice {
    id: uuid::Uuid,
    #[serde(default)]
    invoice_number: String,
    /// Resolved company display name (PMS-186); the client never shows the
    /// raw `company_id` UUID.
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    invoice_date: Option<String>,
    #[serde(default)]
    due_date: Option<String>,
    #[serde(default)]
    total: String,
    #[serde(default)]
    balance_due: String,
}

/// Shared "no finance permission" state rendered by every billing list page
/// when the current user lacks the `can_manage_billing` role set
/// (super_admin / admin / finance). A friendly locked state rather than a
/// bare error sentence (MAPPS-133): an icon, a clear heading, who has
/// access, and the viewer's current role for context.
#[component]
fn NoFinancePermission(title: String) -> Element {
    let auth = crate::hooks::use_auth();
    let role = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.as_str().to_string())
        .unwrap_or_default();
    rsx! {
        AppLayout { title: "{title}",
            PageHeader { title: "{title}" }
            Card {
                div { class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                    div { class: "mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-gray-100 dark:bg-gray-800",
                        InformationIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
                    }
                    h3 { class: "text-base font-medium text-gray-900 dark:text-white",
                        "Billing access required"
                    }
                    p { class: "mt-2 text-sm text-gray-500 dark:text-gray-400",
                        "Invoices and payments are restricted to administrator and finance roles. Ask an administrator to grant you access."
                    }
                    if !role.is_empty() {
                        p { class: "mt-4 text-xs text-gray-400 dark:text-gray-500",
                            "Your current role: {role}"
                        }
                    }
                }
            }
        }
    }
}

/// Invoice list page. GET `/invoices` with optional company / status
/// filters, server-paginated.
#[component]
pub fn InvoiceListPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    if !has_finance {
        return rsx! { NoFinancePermission { title: "Invoices" } };
    }

    let mut company_filter = use_signal(String::new);
    let mut status_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        load_companies().await
    });
    let company_options = company_select_options(
        &companies_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
        "All companies",
    );

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("draft", "Draft"),
        SelectOption::new("pending", "Pending"),
        SelectOption::new("sent", "Sent"),
        SelectOption::new("partially_paid", "Partially Paid"),
        SelectOption::new("paid", "Paid"),
        SelectOption::new("void", "Void"),
        SelectOption::new("written_off", "Written Off"),
    ];

    let company_text = company_filter.read().trim().to_string();
    let status_text = status_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let company_for_resource = company_text.clone();
    let status_for_resource = status_text.clone();
    let invoices_resource = use_resource(move || {
        let company = company_for_resource.clone();
        let status = status_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/invoices?page={current_page}&per_page={PER_PAGE}");
            // `company` is a UUID filter (`InvoiceFilter.company_id`). Only
            // send it when it parses, otherwise the server 422s.
            if uuid::Uuid::parse_str(&company).is_ok() {
                path.push_str(&format!("&company_id={company}"));
            }
            if !status.is_empty() {
                path.push_str(&format!("&status={status}"));
            }
            crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteInvoice>>(&path, &token)
                .await
                .ok()
        }
    });

    let snap = invoices_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteInvoice>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !company_text.is_empty() || !status_text.is_empty();

    rsx! {
        AppLayout { title: "Invoices",
            PageHeader {
                title: "Invoices",
                subtitle: "Manage customer invoices and billing",
                actions: rsx! {
                    Link {
                        to: Route::TaxRateList {},
                        Button { variant: ButtonVariant::Secondary, "Tax Rates" }
                    }
                    Link {
                        to: Route::PaymentGatewayConfig {},
                        Button { variant: ButtonVariant::Secondary, "Gateways" }
                    }
                    Link {
                        to: Route::InvoiceNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Invoice"
                        }
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        Select {
                            name: "company_id",
                            options: company_options,
                            value: company_filter.read().clone(),
                            onchange: move |e: FormEvent| {
                                company_filter.set(e.value());
                                page.set(1);
                            },
                        }
                    }
                    Select {
                        name: "status",
                        options: status_options,
                        value: status_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            status_filter.set(e.value());
                            page.set(1);
                        },
                    }
                }
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load invoices. Refresh the page to retry."
                }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 7,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Invoice" }
                            TableHeader { "Company" }
                            TableHeader { "Date" }
                            TableHeader { "Due Date" }
                            TableHeader { class: "text-right", "Total" }
                            TableHeader { class: "text-right", "Balance" }
                            TableHeader { "Status" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 7, rows: 5 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 7,
                            message: if has_filters {
                                "No invoices match your filters.".to_string()
                            } else {
                                "No invoices yet. Click New Invoice to create one.".to_string()
                            },
                        }
                    } else {
                        TableBody {
                            for invoice in rows.iter().cloned() {
                                InvoiceRow {
                                    key: "{invoice.id}",
                                    id: invoice.id.to_string(),
                                    number: invoice.invoice_number,
                                    company: invoice.company_name.clone().unwrap_or_default(),
                                    date: invoice.invoice_date.unwrap_or_default(),
                                    due_date: invoice.due_date.unwrap_or_default(),
                                    total: format_money_str(&invoice.total),
                                    balance: format_money_str(&invoice.balance_due),
                                    status: invoice.status,
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
struct InvoiceRowProps {
    id: String,
    number: String,
    company: String,
    date: String,
    due_date: String,
    total: String,
    balance: String,
    status: String,
}

#[component]
fn InvoiceRow(props: InvoiceRowProps) -> Element {
    let status_label = humanize_invoice_status(&props.status);
    let status_variant = invoice_status_variant(&props.status);
    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::InvoiceDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::InvoiceDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.number}"
                }
            }
            TableCell {
                if props.company.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.company}"
                }
            }
            TableCell {
                if props.date.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.date}"
                }
            }
            TableCell {
                if props.due_date.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.due_date}"
                }
            }
            TableCell { class: "text-right font-medium", "{props.total}" }
            TableCell { class: "text-right", "{props.balance}" }
            TableCell { Badge { variant: status_variant, "{status_label}" } }
        }
    }
}

/// Full `InvoiceResponse` for the detail page, including line items.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct InvoiceDetail {
    #[serde(default)]
    invoice_number: String,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    billing_contact_id: Option<uuid::Uuid>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    invoice_date: Option<String>,
    #[serde(default)]
    due_date: Option<String>,
    // MAPPS-170/PMS-333: payment terms are now a lookup FK; the response
    // carries the id (for editing) and the joined name (for display).
    #[serde(default)]
    payment_term_id: Option<String>,
    #[serde(default)]
    payment_term_name: Option<String>,
    #[serde(default)]
    subtotal: String,
    #[serde(default)]
    tax_amount: String,
    #[serde(default)]
    discount_amount: String,
    #[serde(default)]
    total: String,
    #[serde(default)]
    amount_paid: String,
    #[serde(default)]
    balance_due: String,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    po_number: Option<String>,
    #[serde(default)]
    lines: Option<Vec<InvoiceLine>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct InvoiceLine {
    id: uuid::Uuid,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quantity: String,
    #[serde(default)]
    unit_price: String,
    #[serde(default)]
    total: String,
}

/// Invoice detail page. GET `/invoices/{id}` with `lines` populated.
#[derive(Props, Clone, PartialEq)]
pub struct InvoiceDetailPageProps {
    pub id: String,
}

#[component]
pub fn InvoiceDetailPage(props: InvoiceDetailPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let mut invoice_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<InvoiceDetail>(&format!("/invoices/{id}"))
                .await
                .ok()
        }
    });

    // MAPPS-158: detail-page lifecycle actions. `PUT /invoices/{id}`
    // freezes header/line/status edits once an invoice leaves
    // draft/pending (`InvoiceStatus::is_frozen`), so Edit, Send and Void are
    // surfaced only while the invoice is editable. Record Payment
    // (`POST /payments`) is offered whenever a balance can still be
    // collected. The backend exposes no route to delete, un-send, or email an
    // invoice, and editing line items requires a credit note (out of scope),
    // so those actions are intentionally not surfaced here.
    let mut show_edit = use_signal(|| false);
    let mut show_payment = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut action_error = use_signal(String::new);

    let snap = invoice_resource.read_unchecked();
    let invoice = match &*snap {
        Some(Some(inv)) => Some(inv.clone()),
        _ => None,
    };
    let header_title = match &invoice {
        Some(inv) => format!("Invoice {}", inv.invoice_number),
        None => "Invoice".to_string(),
    };
    let status = invoice
        .as_ref()
        .map(|i| i.status.clone())
        .unwrap_or_default();
    let editable = matches!(status.as_str(), "draft" | "pending");
    let collectible = matches!(status.as_str(), "pending" | "sent" | "partially_paid");
    let pay_company_id = invoice
        .as_ref()
        .and_then(|i| i.company_id)
        .map(|c| c.to_string())
        .unwrap_or_default();
    let id_for_send = props.id.clone();
    let id_for_void = props.id.clone();
    let act_err = action_error.read().clone();

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    if editable {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                action_error.set(String::new());
                                show_edit.set(true);
                            },
                            "Edit"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: *busy.read(),
                            onclick: move |_| {
                                if *busy.read() {
                                    return;
                                }
                                busy.set(true);
                                action_error.set(String::new());
                                let path = format!("/invoices/{id_for_send}");
                                spawn(async move {
                                    #[cfg(feature = "web")]
                                    {
                                        let body = serde_json::json!({ "status": "sent" });
                                        match crate::hooks::fetch::api::put_authed::<
                                            serde_json::Value,
                                            _,
                                        >(&path, &body)
                                            .await
                                        {
                                            Ok(_) => invoice_resource.restart(),
                                            Err(err) => action_error
                                                .set(format!("Could not send invoice: {err}")),
                                        }
                                    }
                                    busy.set(false);
                                });
                            },
                            "Send"
                        }
                    }
                    if collectible {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                action_error.set(String::new());
                                show_payment.set(true);
                            },
                            "Record Payment"
                        }
                    }
                    if editable {
                        Button {
                            variant: ButtonVariant::Danger,
                            loading: *busy.read(),
                            onclick: move |_| {
                                if *busy.read() {
                                    return;
                                }
                                busy.set(true);
                                action_error.set(String::new());
                                let path = format!("/invoices/{id_for_void}");
                                spawn(async move {
                                    #[cfg(feature = "web")]
                                    {
                                        let confirmed = web_sys::window()
                                            .and_then(|w| {
                                                w.confirm_with_message(
                                                    "Void this invoice? This cannot be undone.",
                                                )
                                                .ok()
                                            })
                                            .unwrap_or(false);
                                        if confirmed {
                                            let body = serde_json::json!({ "status": "void" });
                                            match crate::hooks::fetch::api::put_authed::<
                                                serde_json::Value,
                                                _,
                                            >(&path, &body)
                                                .await
                                            {
                                                Ok(_) => invoice_resource.restart(),
                                                Err(err) => action_error
                                                    .set(format!("Could not void invoice: {err}")),
                                            }
                                        }
                                    }
                                    busy.set(false);
                                });
                            },
                            "Void"
                        }
                    }
                },
            }

            if !act_err.is_empty() {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "{act_err}"
                }
            }

            match &*snap {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading invoice..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load invoice." }
                            Link {
                                to: Route::InvoiceList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to invoices"
                            }
                        }
                    }
                },
                Some(Some(inv)) => {
                    let status_label = humanize_invoice_status(&inv.status);
                    let status_variant = invoice_status_variant(&inv.status);
                    let lines = inv.lines.clone().unwrap_or_default();
                    let currency = inv.currency.clone().unwrap_or_default();
                    let notes = inv.notes.clone();
                    let po_number = inv.po_number.clone();
                    // Joined display name; the editor is seeded from the FK id.
                    let payment_terms = inv.payment_term_name.clone();
                    let company_id = inv.company_id.map(|c| c.to_string());
                    let company_name = inv
                        .company_name
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "View company".to_string());
                    let billing_contact_id = inv.billing_contact_id.map(|c| c.to_string());
                    let invoice_date = inv.invoice_date.clone().unwrap_or_default();
                    let due_date = inv.due_date.clone().unwrap_or_default();
                    let subtotal = format_money_str(&inv.subtotal);
                    let tax_amount = format_money_str(&inv.tax_amount);
                    let discount_amount = format_money_str(&inv.discount_amount);
                    let total = format_money_str(&inv.total);
                    let amount_paid = format_money_str(&inv.amount_paid);
                    let balance_due = format_money_str(&inv.balance_due);
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            div { class: "lg:col-span-2",
                                Card {
                                    // Header
                                    div { class: "flex justify-between mb-8",
                                        div {
                                            h2 { class: "text-2xl font-bold text-gray-900 dark:text-white", "INVOICE" }
                                            p { class: "text-gray-500", "{inv.invoice_number}" }
                                        }
                                        div { class: "text-right",
                                            div { class: "mb-2",
                                                span { class: "text-sm text-gray-500", "Invoice Date: " }
                                                span { class: "font-medium",
                                                    if invoice_date.is_empty() { "-" } else { "{invoice_date}" }
                                                }
                                            }
                                            div { class: "mb-2",
                                                span { class: "text-sm text-gray-500", "Due Date: " }
                                                span { class: "font-medium",
                                                    if due_date.is_empty() { "-" } else { "{due_date}" }
                                                }
                                            }
                                            if let Some(terms) = payment_terms.clone() {
                                                if !terms.is_empty() {
                                                    div {
                                                        span { class: "text-sm text-gray-500", "Terms: " }
                                                        span { class: "font-medium", "{terms}" }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Line items
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
                                            TableEmpty { columns: 4, message: "This invoice has no line items.".to_string() }
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

                                    // Totals
                                    div { class: "mt-8 border-t border-gray-200 dark:border-gray-700 pt-4",
                                        div { class: "flex justify-end",
                                            div { class: "w-64 space-y-2",
                                                div { class: "flex justify-between",
                                                    span { class: "text-gray-500", "Subtotal" }
                                                    span { "{subtotal}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-gray-500", "Tax" }
                                                    span { "{tax_amount}" }
                                                }
                                                div { class: "flex justify-between",
                                                    span { class: "text-gray-500", "Discount" }
                                                    span { "{discount_amount}" }
                                                }
                                                div { class: "flex justify-between text-lg font-bold pt-2 border-t border-gray-200 dark:border-gray-700",
                                                    span { "Total" }
                                                    span { "{total}" }
                                                }
                                            }
                                        }
                                    }

                                    if let Some(notes) = notes.clone() {
                                        if !notes.is_empty() {
                                            div { class: "mt-6 text-sm",
                                                h3 { class: "font-medium text-gray-500 mb-1", "Notes" }
                                                p { class: "text-gray-700 dark:text-gray-300 whitespace-pre-line", "{notes}" }
                                            }
                                        }
                                    }
                                }
                            }

                            // Sidebar
                            div { class: "space-y-6",
                                Card { title: "Status",
                                    div { class: "space-y-4",
                                        div { class: "flex justify-between items-center",
                                            span { class: "text-gray-500", "Status" }
                                            Badge { variant: status_variant, "{status_label}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-500", "Total" }
                                            span { class: "font-medium", "{total}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-500", "Paid" }
                                            span { class: "font-medium text-green-600", "{amount_paid}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-500", "Balance Due" }
                                            span { class: "text-lg font-bold", "{balance_due}" }
                                        }
                                    }
                                }

                                Card { title: "Details",
                                    dl { class: "space-y-3 text-sm",
                                        if !currency.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-gray-500", "Currency" }
                                                dd { "{currency}" }
                                            }
                                        }
                                        if let Some(po) = po_number.clone() {
                                            if !po.is_empty() {
                                                div { class: "flex justify-between",
                                                    dt { class: "text-gray-500", "PO Number" }
                                                    dd { "{po}" }
                                                }
                                            }
                                        }
                                        if let Some(cid) = company_id.clone() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-gray-500", "Company" }
                                                dd {
                                                    Link {
                                                        to: Route::CompanyDetail { id: cid.clone() },
                                                        class: "text-blue-600 hover:text-blue-500",
                                                        "{company_name}"
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(bcid) = billing_contact_id.clone() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-gray-500", "Billing Contact" }
                                                dd {
                                                    Link {
                                                        to: Route::ContactDetail { id: bcid.clone() },
                                                        class: "text-blue-600 hover:text-blue-500",
                                                        "View contact"
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

            if *show_edit.read() {
                if let Some(inv) = invoice.clone() {
                    InvoiceEditModal {
                        id: props.id.clone(),
                        invoice_date: inv.invoice_date.clone().unwrap_or_default(),
                        due_date: inv.due_date.clone().unwrap_or_default(),
                        payment_term_id: inv.payment_term_id.clone().unwrap_or_default(),
                        po_number: inv.po_number.clone().unwrap_or_default(),
                        notes: inv.notes.clone().unwrap_or_default(),
                        onclose: move |_| show_edit.set(false),
                        onsaved: move |_| {
                            show_edit.set(false);
                            invoice_resource.restart();
                        },
                    }
                }
            }

            if *show_payment.read() {
                RecordPaymentModal {
                    company_id: pay_company_id.clone(),
                    invoice_id: props.id.clone(),
                    onclose: move |_| show_payment.set(false),
                    onsaved: move |_| {
                        show_payment.set(false);
                        invoice_resource.restart();
                    },
                }
            }
        }
    }
}

/// New invoice page. Two paths: a manual single-line invoice (POST
/// `/invoices`) and "generate from time entries" (POST
/// `/invoices/from-time-entries`). Both take a company UUID; the manual
/// path also takes dates and one line item.
#[component]
pub fn InvoiceNewPage() -> Element {
    let mut company_id = use_signal(String::new);
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        load_companies().await
    });
    let company_options = company_select_options(
        &companies_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
        "Select a company",
    );
    let mut invoice_date = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut po_number = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut line_description = use_signal(String::new);
    let mut line_quantity = use_signal(|| "1".to_string());
    let mut line_unit_price = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut is_generating = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field messages so a bad value is flagged at the field rather than
    // surfaced only as the generic 422 banner (MAPPS-214).
    let mut due_date_error = use_signal(String::new);
    let mut quantity_error = use_signal(String::new);
    let mut unit_price_error = use_signal(String::new);

    let navigator = use_navigator();

    // Manual create: POST /invoices with a single service line.
    let handle_create = move |e: FormEvent| {
        e.prevent_default();
        if *is_submitting.read() || *is_generating.read() {
            return;
        }
        error.set(String::new());
        due_date_error.set(String::new());
        quantity_error.set(String::new());
        unit_price_error.set(String::new());

        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().trim()).ok() else {
            error.set("A valid company ID (UUID) is required.".to_string());
            return;
        };
        let inv_date = invoice_date.read().trim().to_string();
        let due = due_date.read().trim().to_string();
        if inv_date.is_empty() || due.is_empty() {
            error.set("Invoice date and due date are required.".to_string());
            return;
        }
        // Dates come from the native date picker as ISO `YYYY-MM-DD`, so a
        // lexicographic compare is a correct date order check.
        if due < inv_date {
            due_date_error.set("Due date must be on or after the invoice date.".to_string());
            return;
        }
        let description = line_description.read().trim().to_string();
        if description.is_empty() {
            error.set("A line item description is required.".to_string());
            return;
        }
        let quantity = line_quantity.read().trim().to_string();
        let unit_price = line_unit_price.read().trim().to_string();
        if quantity.is_empty() || unit_price.is_empty() {
            error.set("Line item quantity and unit price are required.".to_string());
            return;
        }
        // Reject negatives (and non-numeric input) at the field. The native
        // `min="0"` already blocks this, but validate here too so the message
        // is explicit and the bad value never reaches the 422 path.
        match quantity.parse::<f64>() {
            Ok(q) if q >= 0.0 => {}
            Ok(_) => {
                quantity_error.set("Quantity cannot be negative.".to_string());
                return;
            }
            Err(_) => {
                quantity_error.set("Enter a valid quantity.".to_string());
                return;
            }
        }
        match unit_price.parse::<f64>() {
            Ok(p) if p >= 0.0 => {}
            Ok(_) => {
                unit_price_error.set("Unit price cannot be negative.".to_string());
                return;
            }
            Err(_) => {
                unit_price_error.set("Enter a valid unit price.".to_string());
                return;
            }
        }

        is_submitting.set(true);
        let body = serde_json::json!({
            "company_id": company_uuid,
            "invoice_date": inv_date,
            "due_date": due,
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
            "lines": [{
                "line_type": "service",
                "description": description,
                // Quantities/prices are decimals; the server parses the
                // string into `rust_decimal::Decimal`.
                "quantity": quantity,
                "unit_price": unit_price,
                "sort_order": 0,
            }],
        });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct InvoiceId {
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed::<InvoiceId, _>("/invoices", &body)
                    .await
                {
                    Ok(inv) => {
                        navigator.push(Route::InvoiceDetail {
                            id: inv.id.to_string(),
                        });
                    }
                    Err(err) => {
                        error.set(format!("Could not create invoice: {err}"));
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    // Generate from time entries: POST /invoices/from-time-entries.
    // Sweeps every eligible billable entry for the company (no
    // time_entry_ids => all eligible).
    let handle_generate = move |_| {
        if *is_submitting.read() || *is_generating.read() {
            return;
        }
        error.set(String::new());
        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().trim()).ok() else {
            error.set(
                "A valid company ID (UUID) is required to generate from time entries.".to_string(),
            );
            return;
        };

        is_generating.set(true);
        let inv_date = optional_string(&invoice_date.read());
        let due = optional_string(&due_date.read());
        let body = serde_json::json!({
            "company_id": company_uuid,
            "invoice_date": inv_date,
            "due_date": due,
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
        });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct InvoiceId {
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed::<InvoiceId, _>(
                    "/invoices/from-time-entries",
                    &body,
                )
                .await
                {
                    Ok(inv) => {
                        navigator.push(Route::InvoiceDetail {
                            id: inv.id.to_string(),
                        });
                    }
                    Err(err) => {
                        error.set(format!("Could not generate invoice: {err}"));
                    }
                }
            }
            is_generating.set(false);
        });
    };

    rsx! {
        AppLayout { title: "New Invoice",
            PageHeader {
                title: "New Invoice",
                subtitle: "Create an invoice manually or generate one from billable time entries",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: handle_create,

                    if !error.read().is_empty() {
                        div {
                            class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                            "{error.read()}"
                        }
                    }

                    Select {
                        name: "company_id",
                        label: "Company",
                        options: company_options,
                        required: true,
                        placeholder: "Select a company",
                        value: company_id.read().clone(),
                        onchange: move |e: FormEvent| company_id.set(e.value()),
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::DateField {
                            name: "invoice_date",
                            label: "Invoice Date",
                            required: true,
                            value: invoice_date.read().clone(),
                            oninput: move |e: FormEvent| invoice_date.set(e.value()),
                        }
                        crate::components::DateField {
                            name: "due_date",
                            label: "Due Date",
                            required: true,
                            value: due_date.read().clone(),
                            error: due_date_error.read().clone(),
                            oninput: move |e: FormEvent| {
                                due_date_error.set(String::new());
                                due_date.set(e.value());
                            },
                        }
                    }

                    crate::components::Input {
                        name: "po_number",
                        label: "PO Number",
                        maxlength: 100,
                        value: po_number.read().clone(),
                        oninput: move |e: FormEvent| po_number.set(e.value()),
                    }

                    div {
                        h3 { class: "text-sm font-medium text-gray-700 dark:text-gray-300 mb-3", "Line Item" }
                        div { class: "grid grid-cols-1 gap-3 sm:grid-cols-[1fr_100px_140px]",
                            crate::components::Input {
                                name: "line_description",
                                label: "Description",
                                required: true,
                                maxlength: 1000,
                                placeholder: "What was delivered",
                                value: line_description.read().clone(),
                                oninput: move |e: FormEvent| line_description.set(e.value()),
                            }
                            crate::components::Input {
                                name: "line_quantity",
                                label: "Quantity",
                                r#type: "number",
                                required: true,
                                step: "0.01",
                                min: "0",
                                placeholder: "Qty",
                                value: line_quantity.read().clone(),
                                error: quantity_error.read().clone(),
                                oninput: move |e: FormEvent| {
                                    quantity_error.set(String::new());
                                    line_quantity.set(e.value());
                                },
                            }
                            crate::components::Input {
                                name: "line_unit_price",
                                label: "Unit Price",
                                r#type: "number",
                                required: true,
                                step: "0.01",
                                min: "0",
                                placeholder: "0.00",
                                value: line_unit_price.read().clone(),
                                error: unit_price_error.read().clone(),
                                oninput: move |e: FormEvent| {
                                    unit_price_error.set(String::new());
                                    line_unit_price.set(e.value());
                                },
                            }
                        }
                        p { class: "mt-2 text-xs text-gray-500",
                            "Manual invoices start with a single service line. Add more lines by editing the invoice after it is created."
                        }
                    }

                    crate::components::Textarea {
                        name: "notes",
                        label: "Notes",
                        placeholder: "Internal notes (not shown to the customer)",
                        rows: 3,
                        maxlength: "2000",
                        value: notes.read().clone(),
                        oninput: move |e: FormEvent| notes.set(e.value()),
                    }

                    div { class: "flex flex-wrap justify-end gap-3",
                        Link {
                            to: Route::InvoiceList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "button",
                            variant: ButtonVariant::Secondary,
                            loading: *is_generating.read(),
                            onclick: handle_generate,
                            "Generate from Time Entries"
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Create Invoice"
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Payments
// ============================================================================

/// Subset of `PaymentResponse` rendered in the list.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemotePayment {
    id: uuid::Uuid,
    #[serde(default)]
    invoice_id: Option<uuid::Uuid>,
    #[serde(default)]
    invoice_number: Option<String>,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    payment_date: Option<String>,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    payment_method: String,
    #[serde(default)]
    reference_number: Option<String>,
}

/// Payment list page. GET `/payments`, server-paginated, plus a
/// record-payment modal (POST `/payments`) and per-row delete (DELETE
/// `/payments/{id}`).
#[component]
pub fn PaymentListPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    if !has_finance {
        return rsx! { NoFinancePermission { title: "Payments" } };
    }

    let mut page = use_signal(|| 1usize);
    let mut recording = use_signal(|| false);
    // Bumped after a create/delete to force the resource to re-fetch.
    let mut reload = use_signal(|| 0u64);

    let current_page = (*page.read()).max(1);
    let reload_token = *reload.read();
    let mut payments_resource = use_resource(move || {
        let _reload = reload_token;
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let path = format!("/payments?page={current_page}&per_page={PER_PAGE}");
            crate::hooks::fetch::api::get_with_auth::<Paginated<RemotePayment>>(&path, &token)
                .await
                .ok()
        }
    });

    let snap = payments_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemotePayment>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Payments",
            PageHeader {
                title: "Payments",
                subtitle: "Track customer payments",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| recording.set(true),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Record Payment"
                    }
                },
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load payments. Refresh the page to retry."
                }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 7,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Company" }
                            TableHeader { "Invoice" }
                            TableHeader { "Method" }
                            TableHeader { "Reference" }
                            TableHeader { class: "text-right", "Amount" }
                            TableHeader { class: "text-right", "Actions" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 7, rows: 5 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 7,
                            message: "No payments recorded yet. Click Record Payment to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for payment in rows.iter().cloned() {
                                PaymentRow {
                                    key: "{payment.id}",
                                    id: payment.id.to_string(),
                                    company: payment.company_name.clone().unwrap_or_default(),
                                    invoice_id: payment.invoice_id.map(|i| i.to_string()).unwrap_or_default(),
                                    invoice_number: payment.invoice_number.clone().unwrap_or_default(),
                                    date: payment.payment_date.unwrap_or_default(),
                                    method: humanize_payment_method(&payment.payment_method),
                                    reference: payment.reference_number.unwrap_or_default(),
                                    amount: format_money_str(&payment.amount),
                                    on_deleted: move |_| { reload += 1; },
                                }
                            }
                        }
                    }
                }
            }

            if *recording.read() {
                RecordPaymentModal {
                    onclose: move |_| recording.set(false),
                    onsaved: move |_| {
                        recording.set(false);
                        payments_resource.restart();
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PaymentRowProps {
    id: String,
    company: String,
    invoice_id: String,
    invoice_number: String,
    date: String,
    method: String,
    reference: String,
    amount: String,
    on_deleted: EventHandler<()>,
}

#[component]
fn PaymentRow(props: PaymentRowProps) -> Element {
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let on_deleted = props.on_deleted;
    let delete_id = props.id.clone();
    // Prefer the human invoice number; fall back to a generic label if the
    // payment is applied but the number could not be resolved.
    let invoice_label = if props.invoice_number.is_empty() {
        "View invoice".to_string()
    } else {
        props.invoice_number.clone()
    };
    rsx! {
        TableRow {
            TableCell {
                if props.date.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.date}"
                }
            }
            TableCell {
                if props.company.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.company}"
                }
            }
            TableCell {
                if props.invoice_id.is_empty() {
                    span { class: "text-gray-400", "Unapplied" }
                } else {
                    Link {
                        to: Route::InvoiceDetail { id: props.invoice_id.clone() },
                        class: "font-medium text-blue-600 hover:text-blue-500",
                        "{invoice_label}"
                    }
                }
            }
            TableCell { "{props.method}" }
            TableCell {
                if props.reference.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.reference}"
                }
            }
            TableCell { class: "text-right font-medium text-green-600", "{props.amount}" }
            TableCell { class: "text-right",
                Button {
                    variant: ButtonVariant::Danger,
                    loading: *deleting.read(),
                    onclick: move |_| {
                        let id = delete_id.clone();
                        deleting.set(true);
                        error.set(String::new());
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let confirmed = web_sys::window()
                                    .and_then(|w| {
                                        w.confirm_with_message(
                                            "Delete this payment? The linked invoice balance will be restored.",
                                        )
                                        .ok()
                                    })
                                    .unwrap_or(false);
                                if confirmed {
                                    let path = format!("/payments/{id}");
                                    match crate::hooks::fetch::api::delete_authed(&path).await {
                                        Ok(()) => on_deleted.call(()),
                                        Err(err) => {
                                            error.set(format!("Could not delete payment: {err}"))
                                        }
                                    }
                                }
                            }
                            deleting.set(false);
                        });
                    },
                    "Delete"
                }
                if !error.read().is_empty() {
                    p { class: "mt-1 text-xs text-red-600", "{error.read()}" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RecordPaymentModalProps {
    // MAPPS-158: optional seeds so the invoice detail page can pre-fill the
    // company and invoice. Default to empty for the standalone Payments view.
    #[props(default)]
    company_id: String,
    #[props(default)]
    invoice_id: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RecordPaymentModal(props: RecordPaymentModalProps) -> Element {
    let mut company_id = use_signal(|| props.company_id.clone());
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        load_companies().await
    });
    let company_options = company_select_options(
        &companies_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
        "Select a company",
    );
    let mut invoice_id = use_signal(|| props.invoice_id.clone());
    let mut payment_date = use_signal(String::new);
    let mut amount = use_signal(String::new);
    let mut payment_method = use_signal(|| "check".to_string());
    let mut reference_number = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let method_options = vec![
        SelectOption::new("check", "Check"),
        SelectOption::new("credit_card", "Credit Card"),
        SelectOption::new("ach", "ACH Transfer"),
        SelectOption::new("wire", "Wire Transfer"),
        SelectOption::new("cash", "Cash"),
        SelectOption::new("other", "Other"),
    ];

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());

        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().trim()).ok() else {
            error.set("A valid company ID (UUID) is required.".to_string());
            return;
        };
        let date = payment_date.read().trim().to_string();
        if date.is_empty() {
            error.set("Payment date is required.".to_string());
            return;
        }
        let amt = amount.read().trim().to_string();
        if amt.is_empty() {
            error.set("Amount is required.".to_string());
            return;
        }
        // invoice_id is optional (an unapplied payment is allowed). Only
        // send it when it parses.
        let invoice_value = match uuid::Uuid::parse_str(invoice_id.read().trim()) {
            Ok(id) => serde_json::Value::String(id.to_string()),
            Err(_) => serde_json::Value::Null,
        };

        saving.set(true);
        let body = serde_json::json!({
            "company_id": company_uuid,
            "invoice_id": invoice_value,
            "payment_date": date,
            "amount": amt,
            "payment_method": payment_method.read().clone(),
            "reference_number": optional_string(&reference_number.read()),
            "notes": optional_string(&notes.read()),
        });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    "/payments",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not record payment: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let footer = rsx! {
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            "Record Payment"
        }
    };

    rsx! {
        Modal {
            open: true,
            title: "Record Payment",
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                Select {
                    name: "payment_company_id",
                    label: "Company",
                    options: company_options,
                    required: true,
                    placeholder: "Select a company",
                    value: company_id.read().clone(),
                    onchange: move |e: FormEvent| company_id.set(e.value()),
                }
                crate::components::Input {
                    name: "payment_invoice_id",
                    label: "Invoice ID (UUID, optional)",
                    help: "Leave blank for an unapplied payment.",
                    value: invoice_id.read().clone(),
                    oninput: move |e: FormEvent| invoice_id.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "payment_date",
                        label: "Payment Date",
                        required: true,
                        value: payment_date.read().clone(),
                        oninput: move |e: FormEvent| payment_date.set(e.value()),
                    }
                    crate::components::Input {
                        name: "payment_amount",
                        label: "Amount",
                        r#type: "number",
                        required: true,
                        value: amount.read().clone(),
                        oninput: move |e: FormEvent| amount.set(e.value()),
                    }
                }
                Select {
                    name: "payment_method",
                    label: "Method",
                    options: method_options,
                    value: payment_method.read().clone(),
                    onchange: move |e: FormEvent| payment_method.set(e.value()),
                }
                crate::components::Input {
                    name: "payment_reference",
                    label: "Reference Number",
                    value: reference_number.read().clone(),
                    oninput: move |e: FormEvent| reference_number.set(e.value()),
                }
                crate::components::Textarea {
                    name: "payment_notes",
                    label: "Notes",
                    rows: 2,
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct InvoiceEditModalProps {
    id: String,
    invoice_date: String,
    due_date: String,
    /// Current payment-term FK (PMS-333), empty string when unset.
    payment_term_id: String,
    po_number: String,
    notes: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

/// A payment-term option for the invoice dropdown (`GET /payment-terms`).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PaymentTermOpt {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_active: bool,
}

/// MAPPS-158: edit a draft/pending invoice's header fields. Wired to
/// `PUT /invoices/{id}`; line items are left untouched (the request omits
/// `lines`, so the server keeps the existing set). The backend rejects the
/// PUT once the invoice is frozen, so this modal is only opened for editable
/// invoices.
#[component]
fn InvoiceEditModal(props: InvoiceEditModalProps) -> Element {
    let mut invoice_date = use_signal(|| props.invoice_date.clone());
    let mut due_date = use_signal(|| props.due_date.clone());
    let mut payment_term_id = use_signal(|| props.payment_term_id.clone());
    let mut po_number = use_signal(|| props.po_number.clone());
    let mut notes = use_signal(|| props.notes.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Payment-term options from the settings-managed lookup (PMS-333). Only
    // active terms are offered; the entry keeps its current term even if that
    // term was later deactivated (it stays selected because we seed by id).
    let terms_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<PaymentTermOpt>>(
            "/payment-terms?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let current_term = payment_term_id.read().clone();
    // The server PUT does `payment_term_id = COALESCE($x, payment_term_id)`, so
    // a null cannot clear a term that is already set. Only offer the "no term"
    // option when the invoice has none yet; once set, the user can switch terms
    // but not blank it (the server cannot express a clear through this PUT).
    let mut term_options = Vec::new();
    if current_term.is_empty() {
        term_options.push(SelectOption::new("", "No payment term"));
    }
    if let Some(terms) = &*terms_resource.read_unchecked() {
        for t in terms.iter() {
            // Keep an inactive term visible only if it is the one currently set.
            if t.is_active || t.id.to_string() == current_term {
                term_options.push(SelectOption::new(t.id.to_string(), t.name.clone()));
            }
        }
    }

    let onclose = props.onclose;
    let onsaved = props.onsaved;
    let invoice_id = props.id.clone();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());
        let inv_date = invoice_date.read().trim().to_string();
        let due = due_date.read().trim().to_string();
        if inv_date.is_empty() || due.is_empty() {
            error.set("Invoice date and due date are required.".to_string());
            return;
        }
        saving.set(true);
        let path = format!("/invoices/{invoice_id}");
        let body = serde_json::json!({
            "invoice_date": inv_date,
            "due_date": due,
            "payment_term_id": optional_string(&payment_term_id.read()),
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
        });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save invoice: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let footer = rsx! {
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            "Save"
        }
    };

    rsx! {
        Modal {
            open: true,
            title: "Edit Invoice",
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "invoice_date",
                        label: "Invoice Date",
                        required: true,
                        value: invoice_date.read().clone(),
                        oninput: move |e: FormEvent| invoice_date.set(e.value()),
                    }
                    crate::components::DateField {
                        name: "due_date",
                        label: "Due Date",
                        required: true,
                        value: due_date.read().clone(),
                        oninput: move |e: FormEvent| due_date.set(e.value()),
                    }
                }
                Select {
                    name: "payment_term_id",
                    label: "Payment Terms",
                    options: term_options,
                    value: payment_term_id.read().clone(),
                    onchange: move |e: FormEvent| payment_term_id.set(e.value()),
                }
                crate::components::Input {
                    name: "po_number",
                    label: "PO Number",
                    value: po_number.read().clone(),
                    oninput: move |e: FormEvent| po_number.set(e.value()),
                }
                crate::components::Textarea {
                    name: "invoice_notes",
                    label: "Notes",
                    rows: 3,
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
    }
}

// ============================================================================
// Tax rates
// ============================================================================

/// `TaxRateResponse`. `rate` is a decimal string.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteTaxRate {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    rate: String,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_active: bool,
}

/// Tax-rate management view. GET/POST `/tax-rates`, PUT/DELETE
/// `/tax-rates/{id}`. Create/edit happen in a modal.
#[component]
pub fn TaxRateListPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    if !has_finance {
        return rsx! { NoFinancePermission { title: "Tax Rates" } };
    }

    let mut page = use_signal(|| 1usize);
    // `Some` => the create/edit modal is open with this state.
    let mut editing = use_signal(|| None::<TaxRateFormState>);

    let current_page = (*page.read()).max(1);
    let mut tax_rates_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        let path = format!("/tax-rates?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteTaxRate>>(&path, &token)
            .await
            .ok()
    });

    let snap = tax_rates_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteTaxRate>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Tax Rates",
            PageHeader {
                title: "Tax Rates",
                subtitle: "Manage tax rates applied to invoices",
                actions: rsx! {
                    Link {
                        to: Route::InvoiceList {},
                        Button { variant: ButtonVariant::Secondary, "Back to Invoices" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing.set(Some(TaxRateFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Tax Rate"
                    }
                },
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load tax rates. Refresh the page to retry."
                }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 4,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { class: "text-right", "Rate" }
                            TableHeader { "Default" }
                            TableHeader { "Active" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 4, rows: 4 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 4,
                            message: "No tax rates yet. Click New Tax Rate to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for rate in rows.iter().cloned() {
                                {
                                    let key = rate.id.to_string();
                                    let edit_state = TaxRateFormState::from_existing(&rate);
                                    let rate_label = rate.rate.clone();
                                    let is_default = rate.is_default;
                                    let is_active = rate.is_active;
                                    let name = rate.name.clone();
                                    rsx! {
                                        TableRow { key: "{key}", clickable: true,
                                            onclick: move |_| editing.set(Some(edit_state.clone())),
                                            TableCell {
                                                span { class: "font-medium text-blue-600", "{name}" }
                                            }
                                            TableCell { class: "text-right", "{rate_label}%" }
                                            TableCell {
                                                if is_default {
                                                    Badge { variant: BadgeVariant::Blue, "Default" }
                                                }
                                            }
                                            TableCell {
                                                if is_active {
                                                    Badge { variant: BadgeVariant::Green, "Active" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Inactive" }
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

            if let Some(state) = editing.read().clone() {
                TaxRateFormModal {
                    state,
                    onclose: move |_| editing.set(None),
                    onsaved: move |_| {
                        editing.set(None);
                        tax_rates_resource.restart();
                    },
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TaxRateFormState {
    /// `Some` => editing an existing rate.
    id: Option<String>,
    name: String,
    rate: String,
    is_default: bool,
    is_active: bool,
}

impl TaxRateFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            rate: String::new(),
            is_default: false,
            is_active: true,
        }
    }

    fn from_existing(r: &RemoteTaxRate) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            rate: r.rate.clone(),
            is_default: r.is_default,
            is_active: r.is_active,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TaxRateFormModalProps {
    state: TaxRateFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TaxRateFormModal(props: TaxRateFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let modal_title = if is_edit {
        "Edit Tax Rate"
    } else {
        "New Tax Rate"
    };

    let mut name = use_signal(|| initial.name.clone());
    let mut rate = use_signal(|| initial.rate.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut is_active = use_signal(|| initial.is_active);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        if rate.read().trim().is_empty() {
            error.set("Rate is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            // Server parses the rate string into `rust_decimal::Decimal`.
            "rate": rate.read().trim(),
            "is_default": *is_default.read(),
            "is_active": *is_active.read(),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), String> = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/tax-rates",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/tax-rates/{id}");
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save tax rate: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this tax rate? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/tax-rates/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete tax rate: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    let footer = rsx! {
        if is_edit {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                onclick: handle_delete,
                "Delete"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create Tax Rate" }
        }
    };

    rsx! {
        Modal {
            open: true,
            title: modal_title,
            size: ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                crate::components::Input {
                    name: "tax_rate_name",
                    label: "Name",
                    placeholder: "e.g. US-CA or Standard VAT",
                    required: true,
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
                crate::components::Input {
                    name: "tax_rate_rate",
                    label: "Rate (%)",
                    r#type: "number",
                    placeholder: "e.g. 8.25",
                    required: true,
                    value: rate.read().clone(),
                    oninput: move |e: FormEvent| rate.set(e.value()),
                }
                crate::components::Checkbox {
                    name: "tax_rate_is_default",
                    label: "Default rate",
                    checked: *is_default.read(),
                    help: "Used when no specific jurisdiction matches.",
                    onchange: move |_| {
                        let next = !*is_default.read();
                        is_default.set(next);
                    },
                }
                crate::components::Checkbox {
                    name: "tax_rate_is_active",
                    label: "Active",
                    checked: *is_active.read(),
                    onchange: move |_| {
                        let next = !*is_active.read();
                        is_active.set(next);
                    },
                }
            }
        }
    }
}

// ============================================================================
// Payment gateway config
// ============================================================================

/// `PaymentGatewayConfigResponse`. `config` is arbitrary decrypted JSON;
/// the view edits it as raw JSON text.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteGateway {
    id: uuid::Uuid,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    is_test_mode: bool,
    #[serde(default)]
    config: serde_json::Value,
}

/// Payment-gateway config view. GET `/payment-gateways` (paginated) and
/// PUT `/payment-gateways` (upsert by provider). Editing happens inline
/// in a modal that posts the whole config back.
#[component]
pub fn PaymentGatewayConfigPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    if !has_finance {
        return rsx! { NoFinancePermission { title: "Payment Gateways" } };
    }

    let mut editing = use_signal(|| None::<GatewayFormState>);

    let mut gateways_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        // One row per provider; a single page covers every configured
        // gateway.
        crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteGateway>>(
            "/payment-gateways?page=1&per_page=100",
            &token,
        )
        .await
        .ok()
    });

    let snap = gateways_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<RemoteGateway> = match &*snap {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    rsx! {
        AppLayout { title: "Payment Gateways",
            PageHeader {
                title: "Payment Gateways",
                subtitle: "Configure payment gateway integrations",
                actions: rsx! {
                    Link {
                        to: Route::InvoiceList {},
                        Button { variant: ButtonVariant::Secondary, "Back to Invoices" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing.set(Some(GatewayFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Configure Gateway"
                    }
                },
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load payment gateways. Refresh the page to retry."
                }
            }

            Card { padding: false,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Provider" }
                            TableHeader { "Mode" }
                            TableHeader { "Active" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 3, rows: 3 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 3,
                            message: "No payment gateways configured. Click Configure Gateway to add one.".to_string(),
                        }
                    } else {
                        TableBody {
                            for gateway in rows.iter().cloned() {
                                {
                                    let key = gateway.id.to_string();
                                    let edit_state = GatewayFormState::from_existing(&gateway);
                                    let provider_label = humanize_provider(&gateway.provider);
                                    let is_test = gateway.is_test_mode;
                                    let is_active = gateway.is_active;
                                    rsx! {
                                        TableRow { key: "{key}", clickable: true,
                                            onclick: move |_| editing.set(Some(edit_state.clone())),
                                            TableCell {
                                                span { class: "font-medium text-blue-600", "{provider_label}" }
                                            }
                                            TableCell {
                                                if is_test {
                                                    Badge { variant: BadgeVariant::Yellow, "Test" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Green, "Live" }
                                                }
                                            }
                                            TableCell {
                                                if is_active {
                                                    Badge { variant: BadgeVariant::Green, "Active" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Inactive" }
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

            if let Some(state) = editing.read().clone() {
                GatewayFormModal {
                    state,
                    onclose: move |_| editing.set(None),
                    onsaved: move |_| {
                        editing.set(None);
                        gateways_resource.restart();
                    },
                }
            }
        }
    }
}

fn humanize_provider(raw: &str) -> String {
    match raw {
        "stripe" => "Stripe".to_string(),
        "authorize_net" => "Authorize.Net".to_string(),
        "paypal" => "PayPal".to_string(),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct GatewayFormState {
    /// `true` when this state was built from an existing row (provider is
    /// then read-only).
    existing: bool,
    provider: String,
    is_active: bool,
    is_test_mode: bool,
    /// Pretty-printed JSON config for the textarea.
    config_json: String,
}

impl GatewayFormState {
    fn new() -> Self {
        Self {
            existing: false,
            provider: "stripe".to_string(),
            is_active: false,
            is_test_mode: true,
            config_json: "{}".to_string(),
        }
    }

    fn from_existing(g: &RemoteGateway) -> Self {
        let config_json =
            serde_json::to_string_pretty(&g.config).unwrap_or_else(|_| "{}".to_string());
        Self {
            existing: true,
            provider: g.provider.clone(),
            is_active: g.is_active,
            is_test_mode: g.is_test_mode,
            config_json,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct GatewayFormModalProps {
    state: GatewayFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn GatewayFormModal(props: GatewayFormModalProps) -> Element {
    let initial = props.state.clone();
    let provider_locked = initial.existing;
    let modal_title = if provider_locked {
        "Edit Gateway"
    } else {
        "Configure Gateway"
    };

    let mut provider = use_signal(|| initial.provider.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut is_test_mode = use_signal(|| initial.is_test_mode);
    let mut config_json = use_signal(|| initial.config_json.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let provider_options = vec![
        SelectOption::new("stripe", "Stripe"),
        SelectOption::new("authorize_net", "Authorize.Net"),
        SelectOption::new("paypal", "PayPal"),
    ];

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());
        // The config must be valid JSON; the server stores it encrypted.
        let parsed_config: serde_json::Value = match serde_json::from_str(&config_json.read()) {
            Ok(v) => v,
            Err(e) => {
                error.set(format!("Config must be valid JSON: {e}"));
                return;
            }
        };
        saving.set(true);
        let body = serde_json::json!({
            "provider": provider.read().clone(),
            "is_active": *is_active.read(),
            "is_test_mode": *is_test_mode.read(),
            "config": parsed_config,
        });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    "/payment-gateways",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save gateway: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_provider = initial.provider.clone();
    let handle_delete = move |_| {
        if !provider_locked || *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        let provider = delete_provider.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Remove this gateway configuration?")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/payment-gateways/{provider}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not remove gateway: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    let footer = rsx! {
        if provider_locked {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                onclick: handle_delete,
                "Remove"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            "Save Gateway"
        }
    };

    rsx! {
        Modal {
            open: true,
            title: modal_title,
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                Select {
                    name: "gateway_provider",
                    label: "Provider",
                    options: provider_options,
                    value: provider.read().clone(),
                    disabled: provider_locked,
                    onchange: move |e: FormEvent| provider.set(e.value()),
                }
                crate::components::Checkbox {
                    name: "gateway_is_active",
                    label: "Active",
                    checked: *is_active.read(),
                    onchange: move |_| {
                        let next = !*is_active.read();
                        is_active.set(next);
                    },
                }
                crate::components::Checkbox {
                    name: "gateway_is_test_mode",
                    label: "Test mode",
                    checked: *is_test_mode.read(),
                    help: "Use the provider's sandbox credentials.",
                    onchange: move |_| {
                        let next = !*is_test_mode.read();
                        is_test_mode.set(next);
                    },
                }
                crate::components::Textarea {
                    name: "gateway_config",
                    label: "Config (JSON)",
                    placeholder: "{{ \"api_key\": \"...\" }}",
                    rows: 8,
                    help: "Provider credentials. Stored encrypted at rest.",
                    value: config_json.read().clone(),
                    oninput: move |e: FormEvent| config_json.set(e.value()),
                }
            }
        }
    }
}

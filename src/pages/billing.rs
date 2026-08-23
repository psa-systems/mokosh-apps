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
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;

use crate::components::{
    invoice_status_badge, use_page_title, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant,
    Card, DataTable, ErrorBanner, IconSize, InformationIcon, MailIcon, Modal, ModalSize,
    PageHeader, PlusIcon, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

/// Rows per page for the paginated billing list views.
const PER_PAGE: usize = 25;

// Money formatting is centralized in `crate::utils::money` (MAPPS-197).
// `format_money_str` parses the server's decimal string and renders it with
// grouped thousands + two decimals, matching projects and contracts.
use crate::utils::money::format_money_str;

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
    crate::hooks::fetch::api::get_all_authed::<CompanyOption>("/contacts/companies")
        .await
        .unwrap_or_default()
}

/// Load the tenant's tax rates for the invoice pickers (MAPPS-192). Reuses the
/// `RemoteTaxRate` model from the Tax Rates settings view. Best-effort: an
/// empty list on error so a form still renders.
async fn load_tax_rates() -> Vec<RemoteTaxRate> {
    crate::hooks::fetch::api::get_all_authed::<RemoteTaxRate>("/tax-rates")
        .await
        .unwrap_or_else(|e| {
            // Best-effort: the form still renders without the picker, but the
            // failure is logged rather than read as "this tenant has no rates".
            tracing::warn!("tax-rate load failed: {e}");
            Vec::new()
        })
}

/// Build `[("", "No tax"), (id, "name (rate%)"), ...]` select options from a
/// loaded tax-rate list, keeping only active rates (MAPPS-192).
fn tax_rate_select_options(rates: &[RemoteTaxRate]) -> Vec<SelectOption> {
    let mut opts = vec![SelectOption::new("", "No tax")];
    opts.extend(
        rates.iter().filter(|r| r.is_active).map(|r| {
            SelectOption::new(r.id.to_string(), format!("{} ({}%)", r.name, r.rate.trim()))
        }),
    );
    opts
}

/// Compute a tax amount from a line subtotal and a selected tax rate
/// (MAPPS-192). `rate_id` is matched against `rates`; an empty id, an unknown
/// id, or an unparseable subtotal/rate yields an empty string (no tax). Rates
/// are stored as a percentage (PMS-339), so tax = subtotal * rate / 100,
/// rounded to two decimals.
fn computed_tax_amount(rates: &[RemoteTaxRate], rate_id: &str, subtotal: &str) -> String {
    if rate_id.is_empty() {
        return String::new();
    }
    let Some(rate) = rates.iter().find(|r| r.id.to_string() == rate_id) else {
        return String::new();
    };
    let rate_pct = Decimal::from_str(rate.rate.trim()).unwrap_or_default();
    let sub = Decimal::from_str(subtotal.trim()).unwrap_or_default();
    ((sub * rate_pct) / Decimal::from(100))
        .round_dp(2)
        .to_string()
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

/// Load a single company's invoices for the Record Payment picker (MAPPS-191).
/// Hits `GET /invoices?company_id=<id>` (the `InvoiceFilter.company_id`
/// filter). Best-effort: an empty list on error so the picker still renders
/// the "(Unapplied payment)" choice.
async fn load_company_invoices(company_id: uuid::Uuid) -> Vec<RemoteInvoice> {
    let path = format!("/invoices?company_id={company_id}");
    crate::hooks::fetch::api::get_all_authed::<RemoteInvoice>(&path)
        .await
        .unwrap_or_else(|e| {
            // Best-effort: the picker still offers "(Unapplied payment)".
            tracing::warn!("invoice load failed for company {company_id}: {e}");
            Vec::new()
        })
}

/// Build the Record Payment invoice options (MAPPS-191): a leading explicit
/// "(Unapplied payment)" blank choice followed by each invoice keyed by UUID
/// and labelled with its human number, amount, and status. Selecting from this
/// list can only ever yield a valid UUID or the explicit blank, so the old
/// silent bad-UUID -> unapplied coercion path is gone.
fn invoice_select_options(invoices: &[RemoteInvoice]) -> Vec<SelectOption> {
    let mut opts = vec![SelectOption::new("", "(Unapplied payment)")];
    opts.extend(invoices.iter().map(|inv| {
        let label = format!("{} - {} ({})", inv.invoice_number, inv.total, inv.status);
        SelectOption::new(inv.id.to_string(), label)
    }));
    opts
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
    use_page_title(title.clone());
    rsx! {
        PageHeader { title: "{title}" }
        Card {
            div { class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                div { class: "mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-surface-2",
                    InformationIcon { size: IconSize::Large, class: "text-subtle".to_string() }
                }
                h3 { class: "text-base font-medium text-content",
                    "Billing access required"
                }
                p { class: "mt-2 text-sm text-muted",
                    "Invoices and payments are restricted to administrator and finance roles. Ask an administrator to grant you access."
                }
                if !role.is_empty() {
                    p { class: "mt-4 text-xs text-subtle",
                        "Your current role: {role}"
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

    use_page_title("Invoices");
    if !has_finance {
        return rsx! { NoFinancePermission { title: "Invoices" } };
    }

    rsx! { InvoiceListBody {} }
}

#[component]
fn InvoiceListBody() -> Element {
    // MAPPS-249: seed the company filter from `?company_id=<uuid>` so a context
    // card's "View All" lands here scoped to that company.
    let mut company_filter =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
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
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
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
    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (which keeps the
    // nav + banner) instead of an empty invoices table. A fetch that fails while
    // still reachable (a 4xx) keeps the inline banner below. This page's only
    // controls are navigation Links + filters (no inline mutations), so no
    // `can_mutate` gating is needed here.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Invoices".to_string() }
        };
    }
    let has_filters = !company_text.is_empty() || !status_text.is_empty();

    rsx! {
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

        // MAPPS-321: scope indicator.
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Invoices,
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
            ErrorBanner { class: "mb-3", "Could not load invoices. Refresh the page to retry." }
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
                } else if rows.is_empty() && has_filters {
                    // MAPPS-291 "Clear filters" affordance on the
                    // invoices list.
                    TableEmpty {
                        columns: 7,
                        title: "No invoices match your filters".to_string(),
                        description: "Adjust the filters above, or clear them to see every invoice again.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| {
                                    company_filter.set(String::new());
                                    status_filter.set(String::new());
                                },
                                "Clear filters"
                            }
                        },
                    }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 7,
                        title: "No invoices yet".to_string(),
                        description: "Create your first invoice to start billing customers.".to_string(),
                        actions: rsx! {
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
    let (status_variant, status_label) = invoice_status_badge(&props.status);
    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::InvoiceDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::InvoiceDetail { id: props.id.clone() },
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.number}"
                }
            }
            TableCell {
                if props.company.is_empty() {
                    span { class: "text-subtle", "-" }
                } else {
                    "{props.company}"
                }
            }
            TableCell {
                if props.date.is_empty() {
                    span { class: "text-subtle", "-" }
                } else {
                    "{props.date}"
                }
            }
            TableCell {
                if props.due_date.is_empty() {
                    span { class: "text-subtle", "-" }
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
    /// Line type as stored (e.g. `service`, `time_entry`). Carried through the
    /// edit modal so re-saving keeps a time/product line's original type
    /// instead of flattening every line to `service` (MAPPS-234).
    #[serde(default)]
    line_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quantity: String,
    #[serde(default)]
    unit_price: String,
    #[serde(default)]
    total: String,
}

/// MAPPS-539: the invoice "Pay Now" mail is built in mokosh-server's billing
/// service, not by a notification rule, so `POST /notifications/preview`
/// renders nothing for it. Same position as the quote and invite sends; the
/// modal says so rather than leaving the operator to read "nothing will be
/// sent" and believe it. MAPPS-489 moves those onto the dispatcher.
const INVOICE_SEND_PREVIEW_NOTE: &str = "The invoice email is built into the server rather than by a notification rule, so there is nothing to render yet. The billing contact is still emailed a link to view and pay the invoice.";

/// The path the invoice **send** transition writes to (MAPPS-539).
///
/// A named helper rather than an inline `format!`, because it is what
/// `scripts/check-email-affordance.sh` keys on. The URL shape cannot be the
/// key: `PUT /invoices/{id}` is the general invoice update, shared here with
/// Edit and Void and again with the invoice delete in `src/pages/contacts.rs`,
/// none of which send anything. This symbol exists because the call sends
/// email, so it matches the send and nothing else, and goes on matching if the
/// button moves to another file.
///
/// The email is `notify_invoice_pay_now` on the server, fired on the first
/// transition into `sent`.
fn invoice_send_path(id: &str) -> String {
    format!("/invoices/{id}")
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
            // MAPPS-357: subscribe to reachability so the invoice auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
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
    use_page_title(&header_title);
    let status = invoice
        .as_ref()
        .map(|i| i.status.clone())
        .unwrap_or_default();
    let editable = matches!(status.as_str(), "draft" | "pending");
    let collectible = matches!(status.as_str(), "pending" | "sent" | "partially_paid");
    // PMS-580: a frozen invoice (sent and beyond) is a finalized financial
    // record. There is no edit / cancel / void once sent; correction goes
    // through a credit note (not yet built). Spell that out inline so the
    // missing actions read as intentional rather than broken. Draft / pending
    // show nothing here (their actions, including Void, are available above).
    let frozen_note = match status.as_str() {
        "sent" | "partially_paid" => Some(
            "This invoice has been sent and is now a finalized record. It can't be edited, cancelled, or voided. Record a payment to collect the balance; corrections are made with a credit note.",
        ),
        "paid" => Some(
            "This invoice is paid and finalized. It can't be edited or voided; corrections are made with a credit note.",
        ),
        "void" => Some("This invoice has been voided and is kept on record. It can't be edited or reinstated."),
        "written_off" => Some("This invoice has been written off and is kept on record. It can't be edited or reinstated."),
        _ => None,
    };
    let pay_company_id = invoice
        .as_ref()
        .and_then(|i| i.company_id)
        .map(|c| c.to_string())
        .unwrap_or_default();
    let id_for_send = props.id.clone();
    let id_for_void = props.id.clone();
    let act_err = action_error.read().clone();

    // MAPPS-189: the Void button opens the styled ConfirmDialog; the void
    // PUT fires from `on_confirm_void` once the user confirms.
    let mut confirming_void = use_signal(|| false);
    let on_confirm_void = move |_: ()| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        let path = format!("/invoices/{id_for_void}");
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({ "status": "void" });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => invoice_resource.restart(),
                    Err(err) => action_error.set(format!("Could not void invoice: {err}")),
                }
            }
            busy.set(false);
            confirming_void.set(false);
        });
    };

    // MAPPS-357: primary resource is the fetched invoice (`/invoices/{id}`). A
    // failed load while the server is flagged down is an outage, not a missing
    // invoice - render the honest unavailable state (keeps nav + banner). A
    // failure while still reachable (a 404 / 4xx) keeps the inline "Could not
    // load invoice" card below. Writes (Edit / Send / Record Payment / Void)
    // are blocked while down via `can_mutate`.
    let fetch_failed = matches!(*snap, Some(None));
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Invoice".to_string() }
        };
    }

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_void(),
            title: "Void invoice".to_string(),
            message: "Void this invoice? This cannot be undone.".to_string(),
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
            // PMS-746: a route back to the list, matching ContractDetailPage.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Invoices", Route::InvoiceList {}, &header_title),
                }
            },
            actions: rsx! {
                if editable {
                    Button {
                        variant: ButtonVariant::Secondary,
                        // MAPPS-357: block edits while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                        onclick: move |_| {
                            action_error.set(String::new());
                            show_edit.set(true);
                        },
                        "Edit"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *busy.read(),
                        // MAPPS-357: block sending while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't send while the server is unreachable".to_string()),
                        onclick: move |_| {
                            if *busy.read() {
                                return;
                            }
                            busy.set(true);
                            action_error.set(String::new());
                            let path = invoice_send_path(&id_for_send);
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
                        MailIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Send"
                    }
                    // MAPPS-539: Send emails the client, so it carries the two
                    // affordances every other send trigger does. The preview
                    // never gates the send; it sits beside it.
                    crate::components::EmailPreview {
                        event_type: "billing.invoice_pay_now".to_string(),
                        context: serde_json::json!({
                            "invoice_number": invoice
                                .as_ref()
                                .map(|i| i.invoice_number.clone())
                                .unwrap_or_default(),
                            "company_name": invoice
                                .as_ref()
                                .and_then(|i| i.company_name.clone())
                                .unwrap_or_default(),
                            "total": invoice
                                .as_ref()
                                .map(|i| i.total.clone())
                                .unwrap_or_default(),
                            "due_date": invoice
                                .as_ref()
                                .and_then(|i| i.due_date.clone())
                                .unwrap_or_default(),
                        }),
                        empty_note: INVOICE_SEND_PREVIEW_NOTE.to_string(),
                    }
                }
                if collectible {
                    Button {
                        variant: ButtonVariant::Secondary,
                        // MAPPS-357: block recording a payment while down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't record a payment while the server is unreachable".to_string()),
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
                        // MAPPS-357: block voiding while the server is down.
                        // PMS-580: clarify that Void is the pre-send back-out.
                        disabled: !can_mutate,
                        title: "Voids this draft invoice and keeps it on record. Once an invoice is sent it can no longer be voided.".to_string(),
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

        // PMS-580: explain why a finalized invoice exposes no edit / cancel /
        // void actions.
        if let Some(note) = frozen_note {
            div {
                class: "mb-3 text-xs text-muted bg-surface-2 border border-line rounded-md px-3 py-2",
                "{note}"
            }
        }

        // MAPPS-539: Send is a one-way door that emails the client, and the
        // button alone cannot say so. The conditions are the server's: it
        // skips the mail when no payment gateway is connected, when the
        // invoice has no billing contact, or when that contact has no address
        // on file, so promise it only where it holds.
        if editable {
            p { class: "mb-3 text-xs text-subtle",
                "Sending emails the billing contact a link to view and pay this invoice, if a payment gateway is connected and the contact has an email address. Use Preview email to read it first."
            }
        }

        if !act_err.is_empty() {
            ErrorBanner { class: "mb-3", "{act_err}" }
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
                            to: Route::InvoiceList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to invoices"
                        }
                    }
                }
            },
            Some(Some(inv)) => {
                let (status_variant, status_label) = invoice_status_badge(&inv.status);
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
                                        h2 { class: "text-2xl font-bold text-content", "INVOICE" }
                                        p { class: "text-muted", "{inv.invoice_number}" }
                                    }
                                    div { class: "text-right",
                                        div { class: "mb-2",
                                            span { class: "text-sm text-muted", "Invoice Date: " }
                                            span { class: "font-medium",
                                                if invoice_date.is_empty() { "-" } else { "{invoice_date}" }
                                            }
                                        }
                                        div { class: "mb-2",
                                            span { class: "text-sm text-muted", "Due Date: " }
                                            span { class: "font-medium",
                                                if due_date.is_empty() { "-" } else { "{due_date}" }
                                            }
                                        }
                                        if let Some(terms) = payment_terms.clone() {
                                            if !terms.is_empty() {
                                                div {
                                                    span { class: "text-sm text-muted", "Terms: " }
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
                                            div { class: "flex justify-between",
                                                span { class: "text-muted", "Discount" }
                                                span { "{discount_amount}" }
                                            }
                                            div { class: "flex justify-between text-lg font-bold pt-2 border-t border-line",
                                                span { "Total" }
                                                span { "{total}" }
                                            }
                                        }
                                    }
                                }

                                if let Some(notes) = notes.clone() {
                                    if !notes.is_empty() {
                                        div { class: "mt-6 text-sm",
                                            h3 { class: "font-medium text-muted mb-1", "Notes" }
                                            p { class: "text-content whitespace-pre-line", "{notes}" }
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
                                        span { class: "text-muted", "Status" }
                                        Badge { variant: status_variant, "{status_label}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Total" }
                                        span { class: "font-medium", "{total}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Paid" }
                                        span { class: "font-medium text-green-600 dark:text-green-400", "{amount_paid}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Balance Due" }
                                        span { class: "text-lg font-bold", "{balance_due}" }
                                    }
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
                                    if let Some(po) = po_number.clone() {
                                        if !po.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-muted", "PO Number" }
                                                dd { "{po}" }
                                            }
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
                                    if let Some(bcid) = billing_contact_id.clone() {
                                        div { class: "flex justify-between",
                                            dt { class: "text-muted", "Billing Contact" }
                                            dd {
                                                Link {
                                                    to: Route::ContactDetail { id: bcid.clone() },
                                                    class: "text-accent hover:opacity-90",
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
                    lines: inv.lines.clone().unwrap_or_default(),
                    tax_amount: inv.tax_amount.clone(),
                    discount_amount: inv.discount_amount.clone(),
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

/// New invoice page. Two paths: a manual single-line invoice (POST
/// `/invoices`) and "generate from time entries" (POST
/// `/invoices/from-time-entries`). Both take a company UUID; the manual
/// path also takes dates and one line item.
#[component]
pub fn InvoiceNewPage() -> Element {
    // MAPPS-300: pre-fill `company_id` from the URL so the Company detail
    // "New Invoice" CTA lands on a form already scoped to that company.
    let mut company_id =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    let mut company_name = use_signal(String::new);
    let mut invoice_date = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut po_number = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut line_description = use_signal(String::new);
    let mut line_quantity = use_signal(|| "1".to_string());
    let mut line_unit_price = use_signal(String::new);
    let mut tax_rate_id = use_signal(String::new);
    // `None` => follow the rate-computed tax; `Some` => a manual override.
    let mut tax_override = use_signal(|| None::<String>);
    let mut discount_amount = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut is_generating = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field messages so a bad value is flagged at the field rather than
    // surfaced only as the generic 422 banner (MAPPS-214).
    let mut due_date_error = use_signal(String::new);
    let mut quantity_error = use_signal(String::new);
    let mut unit_price_error = use_signal(String::new);
    // PMS-518: per-field slots for the fields that previously shared the banner.
    let mut invoice_date_error = use_signal(String::new);
    let mut line_description_error = use_signal(String::new);
    // PMS-579: company validation now renders inline under the CompanyPicker
    // (which forwards it to its wrapped Input) instead of the form-level banner,
    // matching every other required field. Shared by both submit paths.
    let mut company_error = use_signal(String::new);

    let tax_rates_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        load_tax_rates().await
    });
    let tax_rate_options = tax_rate_select_options(
        &tax_rates_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
    );
    // Tax computed from the selected rate and the single line's subtotal
    // (qty * unit price); recomputes as either input changes. A manual edit to
    // the Tax field overrides it until another rate is picked.
    let computed_tax = use_memo(move || {
        let qty = Decimal::from_str(line_quantity.read().trim()).unwrap_or_default();
        let price = Decimal::from_str(line_unit_price.read().trim()).unwrap_or_default();
        let subtotal = (qty * price).to_string();
        let rates = tax_rates_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default();
        computed_tax_amount(&rates, &tax_rate_id.read(), &subtotal)
    });

    let navigator = use_navigator();
    // MAPPS-357: this is a create form, not a data-driven view - there is no
    // primary fetched entity whose failure would blank the page, so no
    // ContentUnavailable is warranted (the tax-rate list is a secondary lookup
    // that degrades to an empty dropdown). We still block the write controls
    // (Create / Generate) while the server is unreachable via `can_mutate`.
    let can_mutate = crate::hooks::use_can_mutate();

    // Manual create: POST /invoices with a single service line.
    let handle_create = move |e: FormEvent| {
        e.prevent_default();
        if *is_submitting.read() || *is_generating.read() {
            return;
        }
        error.set(String::new());

        // PMS-518: validate every required field through the shared FormGuard so
        // all failures surface at once (each in its own inline slot) and the first
        // invalid field is focused, instead of the previous first-failure-returns
        // chain that masked every field after the first.
        let mut guard = FormGuard::new();

        // PMS-579: surface a missing company inline under the picker (red
        // outline + message), no "UUID" wording. `note_invalid` with the
        // picker's input id keeps it in the first-invalid focus order.
        let company_uuid = uuid::Uuid::parse_str(company_id.read().trim()).ok();
        if company_uuid.is_none() {
            company_error.set("Company is required.".to_string());
            guard.note_invalid(Some("company_search"));
        } else {
            company_error.set(String::new());
        }

        let inv_date = invoice_date.read().trim().to_string();
        let due = due_date.read().trim().to_string();
        invoice_date_error.set(guard.field(
            "invoice_date",
            &inv_date,
            "Invoice date",
            &[Rule::Required],
        ));
        due_date_error.set(guard.field("due_date", &due, "Due date", &[Rule::Required]));
        // Cross-field order check, only meaningful once both dates are present.
        // Dates come from the native picker as ISO `YYYY-MM-DD`, so a lexicographic
        // compare is a correct order check. Overrides the per-field slot set above.
        if !inv_date.is_empty() && !due.is_empty() && due < inv_date {
            due_date_error.set("Due date must be on or after the invoice date.".to_string());
            guard.note_invalid(Some("due_date"));
        }

        let description = line_description.read().trim().to_string();
        line_description_error.set(guard.field(
            "line_description",
            &description,
            "Description",
            &[Rule::Required],
        ));

        // Quantity / unit price: required, numeric, non-negative. Rule::Number
        // gives the canonical "must not be negative." / "must be a number."
        // messages; Required catches the blank case (Number skips blank).
        let quantity = line_quantity.read().trim().to_string();
        let unit_price = line_unit_price.read().trim().to_string();
        let money_rules = [
            Rule::Required,
            Rule::Number {
                min: Some(0.0),
                max: None,
                max_decimals: None,
            },
        ];
        quantity_error.set(guard.field("line_quantity", &quantity, "Quantity", &money_rules));
        unit_price_error.set(guard.field(
            "line_unit_price",
            &unit_price,
            "Unit price",
            &money_rules,
        ));

        if guard.blocked() {
            return;
        }
        // Past the guard: company is present.
        let Some(company_uuid) = company_uuid else {
            return;
        };

        is_submitting.set(true);
        // Effective tax: a manual override if present, else the rate-computed
        // value. Both tax and discount are optional; an empty string sends
        // null so the server keeps its 0 default (existing flow unchanged).
        let tax_str = tax_override
            .read()
            .clone()
            .unwrap_or_else(|| computed_tax.read().clone());
        let body = serde_json::json!({
            "company_id": company_uuid,
            "invoice_date": inv_date,
            "due_date": due,
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
            "tax_amount": optional_string(&tax_str),
            "discount_amount": optional_string(&discount_amount.read()),
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
        // PMS-579: same inline company error as the manual create path.
        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().trim()).ok() else {
            company_error.set("Company is required.".to_string());
            return;
        };
        company_error.set(String::new());

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

    let tax_value = tax_override
        .read()
        .clone()
        .unwrap_or_else(|| computed_tax.read().clone());

    // PMS-367 AC1: company chosen via the shared autocomplete CompanyPicker.
    let company_picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };

    use_page_title("New Invoice");

    rsx! {
        PageHeader {
            title: "New Invoice",
            subtitle: "Create an invoice manually or generate one from billable time entries",
        }

        Card {
            form {
                class: "space-y-6",
                onsubmit: handle_create,

                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }

                crate::components::CompanyPicker {
                    value: company_name.read().clone(),
                    selected_id: company_picker_selected_id,
                    required: true,
                    allow_inline_create: true,
                    // PMS-579: inline field-level error instead of the banner.
                    error: company_error.read().clone(),
                    onselect: move |(id, name): (String, String)| {
                        company_id.set(id);
                        company_name.set(name);
                        company_error.set(String::new());
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                    },
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "invoice_date",
                        label: "Invoice Date",
                        required: true,
                        rules: vec![Rule::Required],
                        error: invoice_date_error.read().clone(),
                        value: invoice_date.read().clone(),
                        oninput: move |e: FormEvent| {
                            invoice_date_error.set(String::new());
                            invoice_date.set(e.value());
                        },
                    }
                    crate::components::DateField {
                        name: "due_date",
                        label: "Due Date",
                        required: true,
                        rules: vec![Rule::Required],
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
                    h3 { class: "text-sm font-medium text-content mb-3", "Line Item" }
                    div { class: "grid grid-cols-1 gap-3 sm:grid-cols-[1fr_100px_140px]",
                        crate::components::Input {
                            name: "line_description",
                            label: "Description",
                            required: true,
                            maxlength: 1000,
                            rules: vec![Rule::Required],
                            error: line_description_error.read().clone(),
                            placeholder: "What was delivered",
                            value: line_description.read().clone(),
                            oninput: move |e: FormEvent| {
                                line_description_error.set(String::new());
                                line_description.set(e.value());
                            },
                        }
                        crate::components::Input {
                            name: "line_quantity",
                            label: "Quantity",
                            r#type: "number",
                            required: true,
                            step: "0.01",
                            min: "0",
                            rules: vec![
                                Rule::Required,
                                Rule::Number { min: Some(0.0), max: None, max_decimals: None },
                            ],
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
                            rules: vec![
                                Rule::Required,
                                Rule::Number { min: Some(0.0), max: None, max_decimals: None },
                            ],
                            placeholder: "0.00",
                            value: line_unit_price.read().clone(),
                            error: unit_price_error.read().clone(),
                            oninput: move |e: FormEvent| {
                                unit_price_error.set(String::new());
                                line_unit_price.set(e.value());
                            },
                        }
                    }
                    p { class: "mt-2 text-xs text-muted",
                        "Manual invoices start with a single service line. Add more lines by editing the invoice after it is created."
                    }
                }

                div {
                    h3 { class: "text-sm font-medium text-content mb-3", "Tax & Discount" }
                    div { class: "grid grid-cols-1 gap-3 sm:grid-cols-3",
                        Select {
                            name: "tax_rate_id",
                            label: "Tax Rate",
                            options: tax_rate_options,
                            value: tax_rate_id.read().clone(),
                            onchange: move |e: FormEvent| {
                                tax_rate_id.set(e.value());
                                // Re-follow the computed value for the new rate.
                                tax_override.set(None);
                            },
                        }
                        crate::components::Input {
                            name: "tax_amount",
                            label: "Tax",
                            r#type: "number",
                            step: "0.01".to_string(),
                            min: "0".to_string(),
                            placeholder: "0.00",
                            help: "Auto-computed from the tax rate; edit to override.",
                            value: tax_value.clone(),
                            oninput: move |e: FormEvent| tax_override.set(Some(e.value())),
                        }
                        crate::components::Input {
                            name: "discount_amount",
                            label: "Discount",
                            r#type: "number",
                            step: "0.01".to_string(),
                            min: "0".to_string(),
                            placeholder: "0.00",
                            value: discount_amount.read().clone(),
                            oninput: move |e: FormEvent| discount_amount.set(e.value()),
                        }
                    }
                }

                crate::components::Textarea {
                    name: "notes",
                    label: "Notes",
                    placeholder: "Internal notes (not shown to the customer)",
                    rows: 3,
                    maxlength: 2000,
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
                        // MAPPS-357: block generation while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't generate an invoice while the server is unreachable".to_string()),
                        onclick: handle_generate,
                        "Generate from Time Entries"
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        // MAPPS-357: block creation while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create an invoice while the server is unreachable".to_string()),
                        "Create Invoice"
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
    company_id: uuid::Uuid,
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
    #[serde(default)]
    notes: Option<String>,
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

    use_page_title("Payments");
    if !has_finance {
        return rsx! { NoFinancePermission { title: "Payments" } };
    }

    rsx! { PaymentListBody {} }
}

#[component]
fn PaymentListBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut recording = use_signal(|| false);
    // Some(payment) while the edit modal is open, seeded from that row (MAPPS-235).
    let mut editing = use_signal(|| None::<RemotePayment>);
    // Bumped after a create/delete to force the resource to re-fetch.
    let mut reload = use_signal(|| 0u64);

    let current_page = (*page.read()).max(1);
    let reload_token = *reload.read();
    let mut payments_resource = use_resource(move || {
        let _reload = reload_token;
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
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

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty ledger - render the honest unavailable state instead of an
    // empty payments table. A failure while still reachable (a 4xx) keeps the
    // inline banner below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Payments".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Payments",
            subtitle: "Track customer payments",
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block recording a payment while down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't record a payment while the server is unreachable".to_string()),
                    onclick: move |_| recording.set(true),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "Record Payment"
                }
            },
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load payments. Refresh the page to retry." }
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
                        title: "No payments yet".to_string(),
                        description: "Record a payment to track what your customers have paid.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                // MAPPS-357: block recording a payment while down.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't record a payment while the server is unreachable".to_string()),
                                onclick: move |_| recording.set(true),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "Record Payment"
                            }
                        },
                    }
                } else {
                    TableBody {
                        for payment in rows.iter().cloned() {
                            {
                                let edit_payment = payment.clone();
                                rsx! {
                                    PaymentRow {
                                        key: "{payment.id}",
                                        id: payment.id.to_string(),
                                        company: payment.company_name.clone().unwrap_or_default(),
                                        invoice_id: payment.invoice_id.map(|i| i.to_string()).unwrap_or_default(),
                                        invoice_number: payment.invoice_number.clone().unwrap_or_default(),
                                        date: payment.payment_date.clone().unwrap_or_default(),
                                        method: humanize_payment_method(&payment.payment_method),
                                        reference: payment.reference_number.clone().unwrap_or_default(),
                                        amount: format_money_str(&payment.amount),
                                        on_edit: move |_| editing.set(Some(edit_payment.clone())),
                                        on_deleted: move |_| { reload += 1; },
                                    }
                                }
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

        if let Some(p) = editing.read().clone() {
            RecordPaymentModal {
                payment_id: p.id.to_string(),
                company_id: p.company_id.to_string(),
                invoice_id: p.invoice_id.map(|i| i.to_string()).unwrap_or_default(),
                payment_date: p.payment_date.clone().unwrap_or_default(),
                amount: p.amount.clone(),
                payment_method: p.payment_method.clone(),
                reference_number: p.reference_number.clone().unwrap_or_default(),
                notes: p.notes.clone().unwrap_or_default(),
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    payments_resource.restart();
                },
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
    on_edit: EventHandler<()>,
    on_deleted: EventHandler<()>,
}

#[component]
fn PaymentRow(props: PaymentRowProps) -> Element {
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block edit / delete on this row while the server is down.
    let can_mutate = crate::hooks::use_can_mutate();
    let on_edit = props.on_edit;
    let on_deleted = props.on_deleted;
    let delete_id = props.id.clone();
    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // DELETE fires from `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/payments/{id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => on_deleted.call(()),
                    Err(err) => error.set(format!("Could not delete payment: {err}")),
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };
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
                    span { class: "text-subtle", "-" }
                } else {
                    "{props.date}"
                }
            }
            TableCell {
                if props.company.is_empty() {
                    span { class: "text-subtle", "-" }
                } else {
                    "{props.company}"
                }
            }
            TableCell {
                if props.invoice_id.is_empty() {
                    span { class: "text-subtle", "Unapplied" }
                } else {
                    Link {
                        to: Route::InvoiceDetail { id: props.invoice_id.clone() },
                        class: "font-medium text-accent hover:opacity-90",
                        "{invoice_label}"
                    }
                }
            }
            TableCell { "{props.method}" }
            TableCell {
                if props.reference.is_empty() {
                    span { class: "text-subtle", "-" }
                } else {
                    "{props.reference}"
                }
            }
            TableCell { class: "text-right font-medium text-green-600 dark:text-green-400", "{props.amount}" }
            TableCell { class: "text-right",
                div { class: "inline-flex gap-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: *deleting.read() || !can_mutate,
                        title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                        onclick: move |_| on_edit.call(()),
                        "Edit"
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *deleting.read(),
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                        onclick: move |_| {
                            if !*deleting.read() {
                                confirming_delete.set(true);
                            }
                        },
                        "Delete"
                    }
                }
                if !error.read().is_empty() {
                    p { class: "mt-1 text-xs text-red-600 dark:text-red-400", "{error.read()}" }
                }
            }
            crate::components::ConfirmDialog {
                open: confirming_delete(),
                title: "Delete payment".to_string(),
                message: "Delete this payment? The linked invoice balance will be restored."
                    .to_string(),
                confirm_text: "Delete".to_string(),
                cancel_text: "Cancel".to_string(),
                destructive: true,
                loading: *deleting.read(),
                onconfirm: on_confirm_delete,
                oncancel: move |_| {
                    if !*deleting.read() {
                        confirming_delete.set(false);
                    }
                },
            }
        }
    }
}

// Field caps for the Record Payment form's free-text inputs (MAPPS-215).
// Mirror the mokosh-server column limits so over-long input is blocked inline
// (via `maxlength`) instead of failing later as an opaque 422; the server
// stays the source of truth.
const PAYMENT_REFERENCE_MAX: usize = 100;
const PAYMENT_NOTES_MAX: usize = 2000;

/// Upper bound for the Amount field (MAPPS-215). Comfortably inside `Decimal`'s
/// range while ruling out absurd magnitudes, so such input is caught with a
/// clear "out of range" message rather than a misleading parse error.
const PAYMENT_AMOUNT_MAX: i64 = 10_000_000_000;

#[derive(Props, Clone, PartialEq)]
struct RecordPaymentModalProps {
    // MAPPS-235: when set, the modal edits this existing payment (PUT
    // /payments/{id}) instead of creating one (POST /payments). The other
    // fields below seed the form for that edit.
    #[props(default)]
    payment_id: String,
    // MAPPS-158: optional seeds so the invoice detail page can pre-fill the
    // company and invoice. Default to empty for the standalone Payments view.
    #[props(default)]
    company_id: String,
    #[props(default)]
    invoice_id: String,
    #[props(default)]
    payment_date: String,
    #[props(default)]
    amount: String,
    #[props(default)]
    payment_method: String,
    #[props(default)]
    reference_number: String,
    #[props(default)]
    notes: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RecordPaymentModal(props: RecordPaymentModalProps) -> Element {
    let mut company_id = use_signal(|| props.company_id.clone());
    let mut company_name = use_signal(String::new);
    let mut invoice_id = use_signal(|| props.invoice_id.clone());
    // MAPPS-191: invoice picker options for the selected company. Reading
    // `company_id` inside the resource subscribes it, so the list re-fetches
    // whenever the chosen company changes.
    let invoices_resource = use_resource(move || {
        let company = company_id.read().trim().to_string();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            match uuid::Uuid::parse_str(&company) {
                Ok(cid) => load_company_invoices(cid).await,
                Err(_) => Vec::new(),
            }
        }
    });
    let invoice_options = invoice_select_options(
        &invoices_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
    );
    let mut payment_date = use_signal(|| props.payment_date.clone());
    let mut amount = use_signal(|| props.amount.clone());
    // Seed the method from the edited payment, falling back to the create
    // default when this is a fresh record (MAPPS-235).
    let seed_method = props.payment_method.clone();
    let mut payment_method = use_signal(|| {
        if seed_method.is_empty() {
            "check".to_string()
        } else {
            seed_method.clone()
        }
    });
    let mut reference_number = use_signal(|| props.reference_number.clone());
    let mut notes = use_signal(|| props.notes.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block the save while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    // Per-field inline validation errors (MAPPS-215): shown beneath the field
    // they belong to instead of collapsing into the single form-level banner.
    let mut amount_err = use_signal(String::new);
    let mut invoice_err = use_signal(String::new);
    // PMS-518: Payment Date gets its own slot too (previously banner-only).
    let mut payment_date_err = use_signal(String::new);

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
    // Empty => create (POST); set => edit that payment (PUT). MAPPS-235.
    let payment_id = props.payment_id.clone();
    let is_edit = !payment_id.is_empty();

    // PMS-367 AC1: company chosen via the shared autocomplete CompanyPicker.
    let company_picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());
        amount_err.set(String::new());
        invoice_err.set(String::new());
        payment_date_err.set(String::new());

        // PMS-518: accumulate every failure through the shared FormGuard so all
        // problems surface at once (each in its own inline slot) and the first
        // invalid field is focused, instead of the previous chain of early
        // returns. The bespoke Amount / Invoice parses stay because their typed
        // results (the Decimal-validated string and the invoice UUID) feed the
        // request body and the existence check below; the guard only adds
        // focus-first.
        let mut guard = FormGuard::new();

        // The CompanyPicker has no inline slot, so its failure goes to the
        // form-level banner; `note_invalid` still blocks the submit.
        let company_uuid = uuid::Uuid::parse_str(company_id.read().trim()).ok();
        if company_uuid.is_none() {
            error.set("A valid company ID (UUID) is required.".to_string());
            guard.note_invalid(None);
        }

        let date = payment_date.read().trim().to_string();
        payment_date_err.set(guard.field("payment_date", &date, "Payment date", &[Rule::Required]));

        // Amount: required, strictly positive, at most 2 decimals, in range.
        // `min`/`step` on the field block most bad input in the browser; this
        // re-checks on submit so a pasted or scripted value can't slip a
        // negative/zero or sub-cent amount past the form.
        let amt = {
            let s = amount.read().trim().to_string();
            if s.is_empty() {
                amount_err.set("Amount is required.".to_string());
                guard.note_invalid(Some("payment_amount"));
                String::new()
            } else {
                match s.parse::<Decimal>() {
                    Ok(d) if d <= Decimal::ZERO => {
                        amount_err.set("Amount must be greater than zero.".to_string());
                        guard.note_invalid(Some("payment_amount"));
                        String::new()
                    }
                    Ok(d) if d.scale() > 2 => {
                        amount_err.set("Amount must have at most 2 decimal places.".to_string());
                        guard.note_invalid(Some("payment_amount"));
                        String::new()
                    }
                    Ok(d) if d > Decimal::from(PAYMENT_AMOUNT_MAX) => {
                        amount_err.set("Amount is out of range.".to_string());
                        guard.note_invalid(Some("payment_amount"));
                        String::new()
                    }
                    Ok(_) => s,
                    Err(_) => {
                        amount_err.set("Amount must be a number.".to_string());
                        guard.note_invalid(Some("payment_amount"));
                        String::new()
                    }
                }
            }
        };

        // Invoice ID is optional (an unapplied payment is allowed). When
        // present it must be a valid UUID; existence is confirmed below before
        // the payment is recorded. A malformed value is no longer silently
        // dropped (which produced an unintended unapplied payment).
        let invoice_uuid = {
            let raw = invoice_id.read().trim().to_string();
            if raw.is_empty() {
                None
            } else {
                match uuid::Uuid::parse_str(&raw) {
                    Ok(id) => Some(id),
                    Err(_) => {
                        invoice_err.set(
                            "Invoice ID must be a valid UUID, or leave it blank for an unapplied payment."
                                .to_string(),
                        );
                        guard.note_invalid(Some("payment_invoice_id"));
                        None
                    }
                }
            }
        };

        if guard.blocked() {
            return;
        }
        // Past the guard: company is present.
        let Some(company_uuid) = company_uuid else {
            return;
        };

        saving.set(true);
        let invoice_value = match invoice_uuid {
            Some(id) => serde_json::Value::String(id.to_string()),
            None => serde_json::Value::Null,
        };
        let body = serde_json::json!({
            "company_id": company_uuid,
            "invoice_id": invoice_value,
            "payment_date": date,
            "amount": amt,
            "payment_method": payment_method.read().clone(),
            "reference_number": optional_string(&reference_number.read()),
            "notes": optional_string(&notes.read()),
        });
        // Fresh owned copy so this multi-call handler does not move the
        // captured id into the spawned future.
        let payment_id = payment_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                // For an applied payment, confirm the invoice exists (and is
                // visible to this tenant) before recording, so a well-formed
                // but unknown ID gets a field message instead of an opaque
                // server FK error (MAPPS-215).
                if let Some(id) = invoice_uuid {
                    if crate::hooks::fetch::api::get_authed::<InvoiceDetail>(&format!(
                        "/invoices/{id}"
                    ))
                    .await
                    .is_err()
                    {
                        invoice_err.set("No invoice found with that ID.".to_string());
                        saving.set(false);
                        return;
                    }
                }
                let result = if is_edit {
                    crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                        &format!("/payments/{payment_id}"),
                        &body,
                    )
                    .await
                } else {
                    crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/payments",
                        &body,
                    )
                    .await
                };
                match result {
                    Ok(_) => onsaved.call(()),
                    Err(err) => {
                        let verb = if is_edit { "save" } else { "record" };
                        error.set(format!("Could not {verb} payment: {err}"));
                    }
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
            // MAPPS-357: block the save while the server is unreachable.
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Record Payment" }
        }
    };

    rsx! {
        Modal {
            open: true,
            title: if is_edit { "Edit Payment" } else { "Record Payment" },
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }
                crate::components::CompanyPicker {
                    value: company_name.read().clone(),
                    selected_id: company_picker_selected_id,
                    required: true,
                    allow_inline_create: true,
                    onselect: move |(id, name): (String, String)| {
                        // Switching companies invalidates any previously picked
                        // invoice; clear it so a stale UUID can't be submitted.
                        company_id.set(id);
                        company_name.set(name);
                        invoice_id.set(String::new());
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                        invoice_id.set(String::new());
                    },
                }
                Select {
                    name: "payment_invoice_id",
                    label: "Invoice",
                    help: "Select a company first. Choose (Unapplied payment) to leave this payment unapplied.",
                    options: invoice_options,
                    value: invoice_id.read().clone(),
                    error: invoice_err(),
                    onchange: move |e: FormEvent| invoice_id.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "payment_date",
                        label: "Payment Date",
                        required: true,
                        rules: vec![Rule::Required],
                        error: payment_date_err(),
                        value: payment_date.read().clone(),
                        oninput: move |e: FormEvent| {
                            payment_date_err.set(String::new());
                            payment_date.set(e.value());
                        },
                    }
                    crate::components::Input {
                        name: "payment_amount",
                        label: "Amount",
                        r#type: "number",
                        // `min`/`step` make the browser reject non-positive and
                        // sub-cent amounts; submit-time validation re-checks.
                        min: "0.01".to_string(),
                        step: "0.01".to_string(),
                        required: true,
                        value: amount.read().clone(),
                        error: amount_err(),
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
                    maxlength: PAYMENT_REFERENCE_MAX as i64,
                    value: reference_number.read().clone(),
                    oninput: move |e: FormEvent| reference_number.set(e.value()),
                }
                crate::components::Textarea {
                    name: "payment_notes",
                    label: "Notes",
                    rows: 2,
                    maxlength: PAYMENT_NOTES_MAX as i64,
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
    }
}

/// A line-item row being edited in the invoice edit modal (MAPPS-234). Amounts
/// stay as strings (mirroring the server's decimal-as-string wire format) and
/// are validated/parsed on save. `line_type` is preserved from the existing
/// line; new rows default to `service`.
#[derive(Clone, Debug, PartialEq, Default)]
struct EditableLine {
    line_type: String,
    description: String,
    quantity: String,
    unit_price: String,
    // PMS-518: per-field inline validation messages, populated on submit so each
    // failing line flags its own field instead of collapsing into one banner.
    // The message travels with the line through add/remove, staying aligned.
    description_err: String,
    quantity_err: String,
    unit_price_err: String,
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
    /// Current line items, seeded into the editable line table (MAPPS-234).
    /// The tax-rate calculation derives its subtotal from these (MAPPS-192).
    lines: Vec<InvoiceLine>,
    /// Current tax amount, seeded as the editable Tax field (MAPPS-192).
    tax_amount: String,
    /// Current discount amount, seeded as the editable Discount field (MAPPS-192).
    discount_amount: String,
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
/// `PUT /invoices/{id}`. MAPPS-234: the modal also renders an editable line-item
/// table and sends `lines`, so line items can be added, removed, and corrected
/// after creation (the server replaces the set transactionally and recomputes
/// the subtotal). The backend rejects the PUT once the invoice is frozen, so
/// this modal is only opened for editable invoices.
#[component]
fn InvoiceEditModal(props: InvoiceEditModalProps) -> Element {
    let mut invoice_date = use_signal(|| props.invoice_date.clone());
    let mut due_date = use_signal(|| props.due_date.clone());
    let mut payment_term_id = use_signal(|| props.payment_term_id.clone());
    let mut po_number = use_signal(|| props.po_number.clone());
    let mut notes = use_signal(|| props.notes.clone());
    let mut lines = use_signal(|| {
        props
            .lines
            .iter()
            .map(|l| EditableLine {
                line_type: if l.line_type.is_empty() {
                    "service".to_string()
                } else {
                    l.line_type.clone()
                },
                description: l.description.clone(),
                quantity: l.quantity.clone(),
                unit_price: l.unit_price.clone(),
                ..EditableLine::default()
            })
            .collect::<Vec<_>>()
    });
    let mut tax_rate_id = use_signal(String::new);
    // Seed the override with the invoice's current tax so it shows on open;
    // picking a rate clears it back to the rate-computed value (MAPPS-192).
    let mut tax_override = use_signal(|| Some(props.tax_amount.clone()));
    let mut discount_amount = use_signal(|| props.discount_amount.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block the save while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    // PMS-518: per-field inline slots for the required dates (previously a single
    // shared banner). Line-item errors live on each `EditableLine` row.
    let mut invoice_date_err = use_signal(String::new);
    let mut due_date_err = use_signal(String::new);

    // Tax-rate picker (MAPPS-192): compute tax from the stored line subtotal and
    // the selected rate; the Tax field stays editable as a manual override.
    let tax_rates_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        load_tax_rates().await
    });
    let tax_rate_options = tax_rate_select_options(
        &tax_rates_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
    );
    // Subtotal follows the edited lines so picking a tax rate recomputes against
    // the current line set (MAPPS-234), not the subtotal the invoice opened with.
    let line_subtotal = use_memo(move || {
        lines
            .read()
            .iter()
            .map(|l| {
                let qty = Decimal::from_str(l.quantity.trim()).unwrap_or_default();
                let price = Decimal::from_str(l.unit_price.trim()).unwrap_or_default();
                qty * price
            })
            .sum::<Decimal>()
            .to_string()
    });
    let computed_tax = use_memo(move || {
        let rates = tax_rates_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default();
        computed_tax_amount(&rates, &tax_rate_id.read(), &line_subtotal.read())
    });

    // Payment-term options from the settings-managed lookup (PMS-333). Only
    // active terms are offered; the entry keeps its current term even if that
    // term was later deactivated (it stays selected because we seed by id).
    let terms_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<PaymentTermOpt>("/payment-terms")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: the entry keeps its current term either way.
                tracing::warn!("payment-term load failed: {e}");
                Vec::new()
            })
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

        // PMS-518: accumulate every failure through the shared FormGuard so all
        // problems surface at once (the dates in their own inline slots, each
        // line in its own per-field slots) and the first invalid field is
        // focused, instead of the previous first-failure-returns chain.
        let mut guard = FormGuard::new();

        let inv_date = invoice_date.read().trim().to_string();
        let due = due_date.read().trim().to_string();
        invoice_date_err.set(guard.field(
            "invoice_date",
            &inv_date,
            "Invoice date",
            &[Rule::Required],
        ));
        due_date_err.set(guard.field("due_date", &due, "Due date", &[Rule::Required]));

        // Validate the line items and build the request set (MAPPS-234). An
        // invoice must keep at least one line; each line needs a description and
        // a non-negative numeric quantity and unit price (mirrors the create
        // form's checks so a bad value never reaches the 422 path).
        let rows = lines.read().clone();
        if rows.is_empty() {
            // The empty-set rule has no per-line slot; surface it on the banner.
            error.set("An invoice must have at least one line item.".to_string());
            guard.note_invalid(None);
        }
        // Quantity / unit price share the create form's money rules: required,
        // numeric, non-negative. The trimmed strings still feed the body verbatim.
        let money_rules = [
            Rule::Required,
            Rule::Number {
                min: Some(0.0),
                max: None,
                max_decimals: None,
            },
        ];
        let mut lines_json = Vec::with_capacity(rows.len());
        for (idx, line) in rows.iter().enumerate() {
            let description = line.description.trim().to_string();
            let quantity = line.quantity.trim().to_string();
            let unit_price = line.unit_price.trim().to_string();
            let description_err = guard.field(
                &format!("line_description_{idx}"),
                &description,
                "Description",
                &[Rule::Required],
            );
            let quantity_err = guard.field(
                &format!("line_quantity_{idx}"),
                &quantity,
                "Quantity",
                &money_rules,
            );
            let unit_price_err = guard.field(
                &format!("line_unit_price_{idx}"),
                &unit_price,
                "Unit price",
                &money_rules,
            );
            {
                let mut w = lines.write();
                w[idx].description_err = description_err;
                w[idx].quantity_err = quantity_err;
                w[idx].unit_price_err = unit_price_err;
            }
            let line_type = if line.line_type.trim().is_empty() {
                "service"
            } else {
                line.line_type.trim()
            };
            lines_json.push(serde_json::json!({
                "line_type": line_type,
                "description": description,
                // Decimal strings; the server parses into `rust_decimal::Decimal`.
                "quantity": quantity,
                "unit_price": unit_price,
                "sort_order": idx as i32,
            }));
        }

        if guard.blocked() {
            return;
        }
        saving.set(true);
        let path = format!("/invoices/{invoice_id}");
        // Effective tax: a manual override if present, else the rate-computed
        // value. Empty sends null, so the server COALESCE keeps the current
        // amount (existing zero-tax invoices stay unchanged unless edited).
        let tax_str = tax_override
            .read()
            .clone()
            .unwrap_or_else(|| computed_tax.read().clone());
        let body = serde_json::json!({
            "invoice_date": inv_date,
            "due_date": due,
            "payment_term_id": optional_string(&payment_term_id.read()),
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
            "tax_amount": optional_string(&tax_str),
            "discount_amount": optional_string(&discount_amount.read()),
            // Replace the line set (server deletes + reinserts transactionally
            // and recomputes the subtotal). MAPPS-234.
            "lines": lines_json,
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
            // MAPPS-357: block the save while the server is unreachable.
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
            onclick: handle_save,
            "Save"
        }
    };

    let tax_value = tax_override
        .read()
        .clone()
        .unwrap_or_else(|| computed_tax.read().clone());

    rsx! {
        Modal {
            open: true,
            title: "Edit Invoice",
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "invoice_date",
                        label: "Invoice Date",
                        required: true,
                        rules: vec![Rule::Required],
                        error: invoice_date_err(),
                        value: invoice_date.read().clone(),
                        oninput: move |e: FormEvent| {
                            invoice_date_err.set(String::new());
                            invoice_date.set(e.value());
                        },
                    }
                    crate::components::DateField {
                        name: "due_date",
                        label: "Due Date",
                        required: true,
                        rules: vec![Rule::Required],
                        error: due_date_err(),
                        value: due_date.read().clone(),
                        oninput: move |e: FormEvent| {
                            due_date_err.set(String::new());
                            due_date.set(e.value());
                        },
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
                // Line items: add / remove / edit (MAPPS-234). The set is sent
                // on save and replaces the invoice's lines server-side.
                div {
                    div { class: "flex items-center justify-between mb-3",
                        h3 { class: "text-sm font-medium text-content", "Line Items" }
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| {
                                lines
                                    .write()
                                    .push(EditableLine {
                                        line_type: "service".to_string(),
                                        ..EditableLine::default()
                                    });
                            },
                            "Add line"
                        }
                    }
                    if lines.read().is_empty() {
                        p { class: "text-sm text-muted",
                            "No line items. Add at least one before saving."
                        }
                    }
                    div { class: "space-y-3",
                        for (idx , line) in lines.read().clone().into_iter().enumerate() {
                            div {
                                key: "{idx}",
                                class: "grid grid-cols-1 gap-3 sm:grid-cols-[1fr_90px_120px_auto] sm:items-end",
                                crate::components::Input {
                                    name: "line_description_{idx}",
                                    label: "Description",
                                    required: true,
                                    maxlength: 1000,
                                    placeholder: "What was delivered",
                                    rules: vec![Rule::Required],
                                    error: line.description_err.clone(),
                                    value: line.description.clone(),
                                    oninput: move |e: FormEvent| {
                                        let mut w = lines.write();
                                        w[idx].description = e.value();
                                        w[idx].description_err = String::new();
                                    },
                                }
                                crate::components::Input {
                                    name: "line_quantity_{idx}",
                                    label: "Qty",
                                    r#type: "number",
                                    required: true,
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "Qty",
                                    rules: vec![
                                        Rule::Required,
                                        Rule::Number {
                                            min: Some(0.0),
                                            max: None,
                                            max_decimals: None,
                                        },
                                    ],
                                    error: line.quantity_err.clone(),
                                    value: line.quantity.clone(),
                                    oninput: move |e: FormEvent| {
                                        let mut w = lines.write();
                                        w[idx].quantity = e.value();
                                        w[idx].quantity_err = String::new();
                                    },
                                }
                                crate::components::Input {
                                    name: "line_unit_price_{idx}",
                                    label: "Unit Price",
                                    r#type: "number",
                                    required: true,
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "0.00",
                                    rules: vec![
                                        Rule::Required,
                                        Rule::Number {
                                            min: Some(0.0),
                                            max: None,
                                            max_decimals: None,
                                        },
                                    ],
                                    error: line.unit_price_err.clone(),
                                    value: line.unit_price.clone(),
                                    oninput: move |e: FormEvent| {
                                        let mut w = lines.write();
                                        w[idx].unit_price = e.value();
                                        w[idx].unit_price_err = String::new();
                                    },
                                }
                                Button {
                                    variant: ButtonVariant::Ghost,
                                    onclick: move |_| {
                                        lines.write().remove(idx);
                                    },
                                    "Remove"
                                }
                            }
                        }
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                    Select {
                        name: "tax_rate_id",
                        label: "Tax Rate",
                        options: tax_rate_options,
                        value: tax_rate_id.read().clone(),
                        onchange: move |e: FormEvent| {
                            tax_rate_id.set(e.value());
                            tax_override.set(None);
                        },
                    }
                    crate::components::Input {
                        name: "tax_amount",
                        label: "Tax",
                        r#type: "number",
                        step: "0.01".to_string(),
                        min: "0".to_string(),
                        placeholder: "0.00",
                        help: "Auto-computed from the tax rate; edit to override.",
                        value: tax_value.clone(),
                        oninput: move |e: FormEvent| tax_override.set(Some(e.value())),
                    }
                    crate::components::Input {
                        name: "discount_amount",
                        label: "Discount",
                        r#type: "number",
                        step: "0.01".to_string(),
                        min: "0".to_string(),
                        placeholder: "0.00",
                        value: discount_amount.read().clone(),
                        oninput: move |e: FormEvent| discount_amount.set(e.value()),
                    }
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

    use_page_title("Tax Rates");
    if !has_finance {
        return rsx! { NoFinancePermission { title: "Tax Rates" } };
    }

    rsx! { TaxRateListBody {} }
}

#[component]
fn TaxRateListBody() -> Element {
    let mut page = use_signal(|| 1usize);
    // `Some` => the create/edit modal is open with this state.
    let mut editing = use_signal(|| None::<TaxRateFormState>);

    let current_page = (*page.read()).max(1);
    let mut tax_rates_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
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

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state instead of an
    // empty tax-rate table. A failure while still reachable (a 4xx) keeps the
    // inline banner below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Tax Rates".to_string() }
        };
    }

    rsx! {
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
                    // MAPPS-357: block creating a tax rate while down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't add a tax rate while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TaxRateFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Tax Rate"
                }
            },
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load tax rates. Refresh the page to retry." }
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
                        title: "No tax rates yet".to_string(),
                        description: "Add a tax rate to apply it to your invoices.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                // MAPPS-357: block creating a tax rate while down.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't add a tax rate while the server is unreachable".to_string()),
                                onclick: move |_| editing.set(Some(TaxRateFormState::new())),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Tax Rate"
                            }
                        },
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
                                            span { class: "font-medium text-accent", "{name}" }
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
    // MAPPS-357: block save / delete while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    // PMS-518: per-field inline slots, previously presence-only on the banner.
    let mut name_err = use_signal(String::new);
    let mut rate_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());

        // PMS-518: report both fields at once (each in its own inline slot) and
        // focus the first invalid, instead of bailing on the first presence miss.
        // The rate string still feeds the body verbatim; the server reparses it.
        let mut guard = FormGuard::new();
        let name_v = name.read().trim().to_string();
        let rate_v = rate.read().trim().to_string();
        name_err.set(guard.field(
            "tax_rate_name",
            &name_v,
            "Name",
            &[Rule::Required, Rule::MaxLen(100)],
        ));
        rate_err.set(guard.field(
            "tax_rate_rate",
            &rate_v,
            "Rate",
            &[
                Rule::Required,
                Rule::Number {
                    min: Some(0.0),
                    max: Some(100.0),
                    max_decimals: Some(2),
                },
            ],
        ));
        if guard.blocked() {
            return;
        }

        saving.set(true);
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
    let can_delete = delete_id.is_some();
    // MAPPS-189: Delete opens the styled ConfirmDialog; the DELETE runs
    // from `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let handle_delete = move |_| {
        if !can_delete || *saving.read() || *deleting.read() {
            return;
        }
        confirming_delete.set(true);
    };
    let on_confirm_delete = move |_: ()| {
        let Some(id) = delete_id.clone() else { return };
        if *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/tax-rates/{id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not delete tax rate: {err}")),
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let footer = rsx! {
        if is_edit {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                // MAPPS-357: block the delete while the server is unreachable.
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
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
            // MAPPS-357: block the save while the server is unreachable.
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
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
                    ErrorBanner { "{error.read()}" }
                }
                crate::components::Input {
                    name: "tax_rate_name",
                    label: "Name",
                    placeholder: "e.g. US-CA or Standard VAT",
                    required: true,
                    // Mirror the server cap (`UpsertTaxRateRequest::name`,
                    // `length(max = 100)`) so the client rejects over-long names.
                    maxlength: 100,
                    rules: vec![Rule::Required, Rule::MaxLen(100)],
                    error: name_err(),
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| {
                        name_err.set(String::new());
                        name.set(e.value());
                    },
                }
                crate::components::Input {
                    name: "tax_rate_rate",
                    label: "Rate (%)",
                    r#type: "number",
                    placeholder: "e.g. 8.25",
                    required: true,
                    // Mirror the Ticket Priorities SLA-multiplier field
                    // (MAPPS-220): a 2-decimal percentage bounded to 0..=100, so
                    // `8.25` is accepted while negatives and >100% are rejected.
                    step: "0.01".to_string(),
                    min: "0".to_string(),
                    max: "100".to_string(),
                    rules: vec![
                        Rule::Required,
                        Rule::Number {
                            min: Some(0.0),
                            max: Some(100.0),
                            max_decimals: Some(2),
                        },
                    ],
                    error: rate_err(),
                    value: rate.read().clone(),
                    oninput: move |e: FormEvent| {
                        rate_err.set(String::new());
                        rate.set(e.value());
                    },
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
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete tax rate".to_string(),
            message: "Delete this tax rate? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
    }
}

// ============================================================================
// Payment gateway config
// ============================================================================

/// `PaymentGatewayConfigResponse`. The secret is write-only (PMS-342): the
/// server never returns the stored credential, only `configured` (whether a
/// key is on file). MAPPS-363: the view enters the key masked and write-only,
/// showing configured/not-configured rather than the plaintext.
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
    configured: bool,
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

    use_page_title("Payment Gateways");
    if !has_finance {
        return rsx! { NoFinancePermission { title: "Payment Gateways" } };
    }

    rsx! { PaymentGatewayConfigBody {} }
}

#[component]
fn PaymentGatewayConfigBody() -> Element {
    let mut editing = use_signal(|| None::<GatewayFormState>);

    let mut gateways_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_all_with_auth::<RemoteGateway>("/payment-gateways", &token)
            .await
            .ok()
    });

    let snap = gateways_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<RemoteGateway> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty config - render the honest unavailable state instead of an
    // empty gateway table. A failure while still reachable (a 4xx) keeps the
    // inline banner below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Payment Gateways".to_string() }
        };
    }

    rsx! {
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
                    // MAPPS-357: block configuring a gateway while down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't configure a gateway while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(GatewayFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "Configure Gateway"
                }
            },
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3",
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
                        TableHeader { "Credentials" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 3 }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 4,
                        title: "No payment gateways yet".to_string(),
                        description: "Configure a gateway to accept online payments.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                // MAPPS-357: block configuring a gateway while down.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't configure a gateway while the server is unreachable".to_string()),
                                onclick: move |_| editing.set(Some(GatewayFormState::new())),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "Configure Gateway"
                            }
                        },
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
                                let configured = gateway.configured;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{provider_label}" }
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
                                        TableCell {
                                            if configured {
                                                Badge { variant: BadgeVariant::Green, "Configured" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "Not configured" }
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
    /// Whether a secret is already stored server-side (MAPPS-363). Drives the
    /// "Configured" badge and lets the key field be left blank on edit to keep
    /// the existing secret.
    configured: bool,
}

impl GatewayFormState {
    fn new() -> Self {
        Self {
            existing: false,
            provider: "stripe".to_string(),
            is_active: false,
            is_test_mode: true,
            configured: false,
        }
    }

    fn from_existing(g: &RemoteGateway) -> Self {
        Self {
            existing: true,
            provider: g.provider.clone(),
            is_active: g.is_active,
            is_test_mode: g.is_test_mode,
            configured: g.configured,
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

    // Whether a secret is already stored (MAPPS-363): drives the status badge
    // and lets the key field be left blank on edit to keep the existing secret.
    let configured = initial.configured;
    let mut provider = use_signal(|| initial.provider.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut is_test_mode = use_signal(|| initial.is_test_mode);
    // MAPPS-363: the API key is write-only. It always starts blank (the server
    // never returns the stored secret); a blank value on save keeps the
    // existing key.
    let mut api_key = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block save / remove while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    // Inline slot for the key field, routed off the form-level banner.
    let mut key_err = use_signal(String::new);

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
        key_err.set(String::new());

        // MAPPS-363: the key is write-only. Send `config` only when the admin
        // typed a key; a blank field keeps the existing secret (PMS-342
        // omit-to-keep). A first-time gateway (no secret yet) must supply one -
        // the server rejects a create with no `config` (400), so guard here for
        // a field-level message instead.
        let key = api_key.read().trim().to_string();
        if key.is_empty() && !configured {
            key_err.set("An API key is required to configure this gateway.".to_string());
            return;
        }
        saving.set(true);
        let mut body = serde_json::json!({
            "provider": provider.read().clone(),
            "is_active": *is_active.read(),
            "is_test_mode": *is_test_mode.read(),
        });
        if !key.is_empty() {
            body["config"] = serde_json::json!({ "api_key": key });
        }
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                    "/payment-gateways",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => {
                        error.set(format!("Could not save gateway: {}", err.user_message()))
                    }
                }
            }
            saving.set(false);
        });
    };

    let delete_provider = initial.provider.clone();
    // MAPPS-189: Remove opens the styled ConfirmDialog; the DELETE runs
    // from `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let handle_delete = move |_| {
        if !provider_locked || *saving.read() || *deleting.read() {
            return;
        }
        confirming_delete.set(true);
    };
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        let provider = delete_provider.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/payment-gateways/{provider}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not remove gateway: {err}")),
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let footer = rsx! {
        if provider_locked {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                // MAPPS-357: block the remove while the server is unreachable.
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't remove while the server is unreachable".to_string()),
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
            // MAPPS-357: block the save while the server is unreachable.
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
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
                    ErrorBanner { "{error.read()}" }
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
                div { class: "space-y-1",
                    div { class: "flex items-center gap-2",
                        label {
                            r#for: "gateway_api_key",
                            class: "block text-sm font-medium text-content",
                            "API key"
                        }
                        if configured {
                            Badge { variant: BadgeVariant::Green, "Configured" }
                        } else {
                            Badge { variant: BadgeVariant::Gray, "Not configured" }
                        }
                    }
                    crate::components::Input {
                        name: "gateway_api_key",
                        r#type: "password",
                        placeholder: if configured {
                            "Leave blank to keep the current key".to_string()
                        } else {
                            "Enter the provider API key".to_string()
                        },
                        required: !configured,
                        help: "Stored encrypted at rest. It is write-only and never shown again.",
                        error: key_err(),
                        value: api_key.read().clone(),
                        oninput: move |e: FormEvent| {
                            key_err.set(String::new());
                            api_key.set(e.value());
                        },
                    }
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Remove gateway".to_string(),
            message: "Remove this gateway configuration?".to_string(),
            confirm_text: "Remove".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
    }
}

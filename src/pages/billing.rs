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
    clear_on_edit, invoice_status_badge, use_page_title, Badge, BadgeVariant, Button, ButtonSize,
    ButtonVariant, Card, DataTable, ErrorBanner, IconSize, InformationIcon, MailIcon, Modal,
    ModalSize, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
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
pub(crate) fn humanize_payment_method(raw: &str) -> String {
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
    crate::hooks::fetch::list_or_empty(
        "billing company picker option",
        crate::hooks::fetch::api::get_all_authed::<CompanyOption>("/contacts/companies").await,
    )
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

/// The tenant's default rate, the one the server applies when a body names
/// none (PMS-1029): active and flagged default.
fn default_tax_rate(rates: &[RemoteTaxRate]) -> Option<&RemoteTaxRate> {
    rates.iter().find(|r| r.is_default && r.is_active)
}

/// The label of the empty picker entry (MAPPS-712). Picking nothing sends no
/// rate, and the server then applies the tenant's default, so the entry says
/// which rate that is; a tenant with no default really gets no tax.
fn unset_tax_rate_label(rates: &[RemoteTaxRate]) -> String {
    match default_tax_rate(rates) {
        Some(r) => format!("Default: {} ({}%)", r.name, r.rate.trim()),
        None => "No tax".to_string(),
    }
}

/// Build `[("", default-or-no-tax), (id, "name (rate%)"), ...]` select options
/// from a loaded tax-rate list, keeping only active rates (MAPPS-192).
fn tax_rate_select_options(rates: &[RemoteTaxRate]) -> Vec<SelectOption> {
    let mut opts = vec![SelectOption::new("", unset_tax_rate_label(rates))];
    opts.extend(
        rates.iter().filter(|r| r.is_active).map(|r| {
            SelectOption::new(r.id.to_string(), format!("{} ({}%)", r.name, r.rate.trim()))
        }),
    );
    opts
}

/// Compute a tax amount from a taxable line subtotal and a selected tax rate
/// (MAPPS-192). `rate_id` is matched against `rates`; an empty id is the
/// tenant's default rate, the one the server applies (MAPPS-712); an unknown
/// id, no default, or an unparseable subtotal/rate yields an empty string (no
/// tax). Rates are stored as a percentage (PMS-339), so tax = subtotal * rate
/// / 100, rounded to two decimals. A PREVIEW only: the server's figure is the
/// one shown once the invoice is saved.
fn computed_tax_amount(rates: &[RemoteTaxRate], rate_id: &str, subtotal: &str) -> String {
    let rate = if rate_id.is_empty() {
        default_tax_rate(rates)
    } else {
        rates.iter().find(|r| r.id.to_string() == rate_id)
    };
    let Some(rate) = rate else {
        return String::new();
    };
    let rate_pct = Decimal::from_str(rate.rate.trim()).unwrap_or_default();
    let sub = Decimal::from_str(subtotal.trim()).unwrap_or_default();
    // Half away from zero, the server's rounding (PMS-1029), so the preview
    // and the saved figure agree on a half cent.
    ((sub * rate_pct) / Decimal::from(100))
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero)
        .to_string()
}

/// The tax half of an invoice body (MAPPS-712). The server derives the tax
/// from a rate over the taxable lines (PMS-1029), and a given amount wins and
/// records no rate, so a body names ONE of the two or neither.
#[derive(Debug, PartialEq)]
struct TaxBody {
    rate_id: serde_json::Value,
    amount: serde_json::Value,
}

/// An override the user typed sends `tax_amount` and no rate; a picked rate
/// sends `tax_rate_id` and no amount; nothing picked sends neither, and the
/// server applies the tenant's default on create and keeps the recorded rate
/// on edit. A blank override is not one: clearing the field goes back to the
/// rate.
fn tax_body_fields(rate_id: &str, override_amount: Option<&str>) -> TaxBody {
    let override_amount = override_amount.map(str::trim).filter(|a| !a.is_empty());
    match override_amount {
        Some(amount) => TaxBody {
            rate_id: serde_json::Value::Null,
            amount: serde_json::Value::String(amount.to_string()),
        },
        None => TaxBody {
            rate_id: optional_string(rate_id),
            amount: serde_json::Value::Null,
        },
    }
}

/// The totals row label: `Tax (13%)` when the invoice recorded a rate, the
/// way the document prints it (PMS-1029), and a bare `Tax` when it carries a
/// given amount or none. The percent drops its trailing zeros (`13.0000` is
/// `13`, `7.25` stays).
fn tax_label(rate: Option<&str>) -> String {
    match rate
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .and_then(|r| Decimal::from_str(r).ok())
    {
        Some(pct) => format!("Tax ({}%)", pct.normalize()),
        None => "Tax".to_string(),
    }
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
    /// MAPPS-764 / PMS-1173: who the invoice was addressed to. `None` against
    /// a server that predates the field, and on an invoice that names nobody,
    /// which the list renders the same way: there is no one to point at.
    #[serde(default)]
    billing_contact_name: Option<String>,
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
    /// PMS-1037 (MAPPS-728): derived by the server in the tenant's day, so
    /// the page never computes overdue from the browser's clock.
    #[serde(default)]
    is_overdue: bool,
    #[serde(default)]
    days_overdue: i64,
}

/// MAPPS-728: the badge text for an overdue invoice. The day count is the
/// server's (`days_overdue`, PMS-1037), never derived here.
pub(crate) fn overdue_label(days: i64) -> String {
    match days {
        1 => "Overdue (1 day)".to_string(),
        n => format!("Overdue ({n} days)"),
    }
}

/// MAPPS-728: the `overdue` query the list sends for the filter's three
/// states: everything, overdue only, not overdue.
pub(crate) fn overdue_query(filter: &str) -> Option<&'static str> {
    match filter {
        "true" => Some("&overdue=true"),
        "false" => Some("&overdue=false"),
        _ => None,
    }
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
pub(crate) fn NoFinancePermission(title: String) -> Element {
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
    // mokosh-contact-login prompt 006: a contact with `invoices:read`
    // reaches the list too. `use_capability` returns true for staff
    // and platform sessions unconditionally, so this predicate stays
    // true for the pre-pivot personas and additionally lets the
    // capable contact through.
    let contact_can_read = crate::hooks::capabilities::use_capability("invoices:read");

    use_page_title("Invoices");
    if !has_finance && !contact_can_read {
        return rsx! { NoFinancePermission { title: "Invoices" } };
    }

    rsx! { InvoiceListBody {} }
}

#[component]
fn InvoiceListBody() -> Element {
    // mokosh-contact-login prompt 006: staff-only list actions
    // (New / Tax rates / Gateways). Contact-facing "Pay now" does
    // not exist in the codebase today; skipped gracefully.
    let staff_only =
        crate::hooks::capabilities::use_capability(crate::hooks::capabilities::STAFF_ONLY);
    // MAPPS-249: seed the company filter from `?company_id=<uuid>` so a context
    // card's "View All" lands here scoped to that company.
    let mut company_filter =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    let mut status_filter = use_signal(String::new);
    // MAPPS-728: the overdue filter, sent as `overdue=true|false` (PMS-1037).
    let mut overdue_filter = use_signal(String::new);
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

    // MAPPS-670 (mokosh-invoices P1e): the server hides drafts from a
    // Contact caller, so the dropdown drops the option too - offering
    // it would let the portal filter to an always-empty list.
    let mut status_options = vec![SelectOption::new("", "All Statuses")];
    if staff_only {
        status_options.push(SelectOption::new("draft", "Draft"));
    }
    status_options.extend([
        SelectOption::new("pending", "Pending"),
        SelectOption::new("sent", "Sent"),
        SelectOption::new("partially_paid", "Partially Paid"),
        SelectOption::new("paid", "Paid"),
        SelectOption::new("void", "Void"),
        SelectOption::new("written_off", "Written Off"),
    ]);

    let company_text = company_filter.read().trim().to_string();
    let status_text = status_filter.read().clone();
    let overdue_text = overdue_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let company_for_resource = company_text.clone();
    let status_for_resource = status_text.clone();
    let overdue_for_resource = overdue_text.clone();
    let invoices_resource = use_resource(move || {
        let company = company_for_resource.clone();
        let status = status_for_resource.clone();
        let overdue = overdue_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
            // MAPPS-604: allow either staff or contact bearer; the server
            // `RequireCallerContext` filters `company_id` to
            // `contact.company_scope()` when the caller is a Contact,
            // so a contact sees only their own invoices without touching
            // this fetch path.
            if crate::hooks::fetch::api::current_access_token().is_none()
                && !crate::hooks::fetch::api::has_contact_session()
            {
                return None;
            }
            let mut path = format!("/invoices?page={current_page}&per_page={PER_PAGE}");
            // `company` is a UUID filter (`InvoiceFilter.company_id`). Only
            // send it when it parses, otherwise the server 422s.
            if uuid::Uuid::parse_str(&company).is_ok() {
                path.push_str(&format!("&company_id={company}"));
            }
            if !status.is_empty() {
                path.push_str(&format!("&status={status}"));
            }
            if let Some(q) = overdue_query(&overdue) {
                path.push_str(q);
            }
            crate::hooks::fetch::api::get_authed_any::<Paginated<RemoteInvoice>>(&path)
                .await
                .inspect_err(|e| tracing::error!("invoice list load failed: {e}"))
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
    let has_filters =
        !company_text.is_empty() || !status_text.is_empty() || !overdue_text.is_empty();

    rsx! {
        PageHeader {
            title: "Invoices",
            subtitle: "Manage customer invoices and billing",
            actions: rsx! {
                if staff_only {
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
                // MAPPS-728: overdue is the server's call (PMS-1037), so the
                // chip is a query filter rather than a client-side sort.
                Select {
                    name: "overdue",
                    options: vec![
                        SelectOption::new("", "Overdue and current"),
                        SelectOption::new("true", "Overdue only"),
                        SelectOption::new("false", "Not overdue"),
                    ],
                    value: overdue_filter.read().clone(),
                    onchange: move |e: FormEvent| {
                        overdue_filter.set(e.value());
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
                        TableHeader {
                            if staff_only { "Company" } else { "Billed to" }
                        }
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
                                    overdue_filter.set(String::new());
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
                            if staff_only {
                                Link {
                                    to: Route::InvoiceNew {},
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                        "New Invoice"
                                    }
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
                                // MAPPS-764: on the contact plane the company
                                // is the same on every row, so the column
                                // answers "is this one mine" instead.
                                company: if staff_only {
                                    invoice.company_name.clone().unwrap_or_default()
                                } else {
                                    invoice.billing_contact_name.clone().unwrap_or_default()
                                },
                                date: invoice.invoice_date.unwrap_or_default(),
                                due_date: invoice.due_date.unwrap_or_default(),
                                total: format_money_str(&invoice.total),
                                balance: format_money_str(&invoice.balance_due),
                                status: invoice.status,
                                overdue_days: invoice.is_overdue.then_some(invoice.days_overdue),
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
    /// MAPPS-764: the Company column for staff, and who the invoice is billed
    /// to for a customer. One column, because on the contact plane every
    /// invoice belongs to the SAME company, so that value carries no
    /// information there while "is this one mine" is the only question the
    /// list has to answer.
    company: String,
    date: String,
    due_date: String,
    total: String,
    balance: String,
    status: String,
    /// MAPPS-728: `Some(days)` when the server says the invoice is overdue.
    #[props(default)]
    overdue_days: Option<i64>,
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
            TableCell {
                div { class: "flex flex-wrap items-center gap-1",
                    Badge { variant: status_variant, "{status_label}" }
                    if let Some(days) = props.overdue_days {
                        Badge { variant: BadgeVariant::Orange, "{overdue_label(days)}" }
                    }
                }
            }
        }
    }
}

/// MAPPS-735: one payment on the invoice, as `GET /invoices/{id}/payments`
/// (server PMS-1088) lists it. The amounts are decimal strings, the way
/// every money field on this page arrives.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteLedgerPayment {
    #[serde(default)]
    id: Option<uuid::Uuid>,
    #[serde(default)]
    payment_date: String,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    payment_method: String,
    #[serde(default)]
    reference_number: Option<String>,
}

/// MAPPS-735: one refund against a payment on the invoice.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteLedgerRefund {
    #[serde(default)]
    id: Option<uuid::Uuid>,
    #[serde(default)]
    payment_id: Option<uuid::Uuid>,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    created_at: String,
}

/// MAPPS-735: the ledger behind `amount_paid` and `balance_due`, newest
/// first, with the server's own sums so the card adds nothing. Dual-plane:
/// a contact reads its own company's invoice on the contact bearer, and
/// nothing internal (gateway ids, fees, notes) is in it, so one struct
/// serves both sessions.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct RemoteInvoiceLedger {
    #[serde(default)]
    payments: Vec<RemoteLedgerPayment>,
    #[serde(default)]
    refunds: Vec<RemoteLedgerRefund>,
    #[serde(default)]
    total_paid: String,
    #[serde(default)]
    total_refunded: String,
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
    /// MAPPS-768 / PMS-1173: who the invoice is billed to, by name. The
    /// contact plane gets no link to the staff contact page, so the name is
    /// the whole answer there. `None` against a server that predates it.
    #[serde(default)]
    billing_contact_name: Option<String>,
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
    /// MAPPS-638: what has been credited back, so the page can say what is
    /// left to credit and offer a credit note only while something is.
    #[serde(default)]
    amount_credited: String,
    #[serde(default)]
    balance_due: String,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    po_number: Option<String>,
    /// PMS-992: who the invoice went to and when, or nothing when it was
    /// marked sent without emailing.
    #[serde(default)]
    emailed_at: Option<String>,
    #[serde(default)]
    emailed_to: Option<String>,
    /// PMS-1029: the rate the tax was derived from, frozen on the invoice;
    /// both absent when the tax was a given amount.
    #[serde(default)]
    tax_rate_id: Option<uuid::Uuid>,
    /// The percent as the server's decimal string, e.g. `13.0000`.
    #[serde(default)]
    tax_rate: Option<String>,
    /// PMS-1037 (MAPPS-728): the server's overdue call, in the tenant's day.
    #[serde(default)]
    is_overdue: bool,
    #[serde(default)]
    days_overdue: i64,
    /// PMS-1036 (MAPPS-727): the write-off, when there was one. The amount
    /// is the balance at that moment, frozen; `written_off_by_name` is the
    /// server's joined display name and absent from an older server.
    #[serde(default)]
    written_off_at: Option<String>,
    #[serde(default)]
    written_off_by_name: Option<String>,
    #[serde(default)]
    write_off_reason: Option<String>,
    #[serde(default)]
    write_off_amount: Option<String>,
    #[serde(default)]
    lines: Option<Vec<InvoiceLine>>,
}

/// MAPPS-727: whether the server would accept a write-off. PMS-1036 moves a
/// `sent` or `partially_paid` invoice to `written_off` and refuses every
/// other status with a 409, so the button is absent rather than disabled
/// everywhere else: a control that can never work should not be on the page.
pub(crate) fn can_write_off(status: &str) -> bool {
    matches!(status, "sent" | "partially_paid")
}

/// MAPPS-727: the Details row for a written-off invoice: the amount, the
/// date part of the timestamp, and who, when the server named them.
pub(crate) fn write_off_line(amount: &str, at: Option<&str>, by: Option<&str>) -> String {
    let mut line = amount.to_string();
    if let Some(date) = at
        .map(|a| a.chars().take(10).collect::<String>())
        .filter(|d| !d.is_empty())
    {
        line.push_str(&format!(" on {date}"));
    }
    if let Some(who) = by.map(str::trim).filter(|w| !w.is_empty()) {
        line.push_str(&format!(" by {who}"));
    }
    line
}

/// MAPPS-735: a payment method as the Payments card labels it: the
/// server's snake_case token with the underscores spaced and the first
/// letter raised (`credit_card` to "Credit card"). An empty token reads
/// as "Payment" rather than as a blank.
pub(crate) fn payment_method_label(method: &str) -> String {
    let words = method.trim().replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Payment".to_string(),
    }
}

/// The row text for a payment: the method, the date, and the reference
/// when the server sent a non-blank one.
pub(crate) fn payment_line(date: &str, method: &str, reference: Option<&str>) -> String {
    let mut line = payment_method_label(method);
    let date = date.trim();
    if !date.is_empty() {
        line.push_str(&format!(" on {date}"));
    }
    if let Some(r) = reference.map(str::trim).filter(|r| !r.is_empty()) {
        line.push_str(&format!(" (ref {r})"));
    }
    line
}

/// The row text for a refund: the date part of its timestamp.
pub(crate) fn refund_line(created_at: &str) -> String {
    let date: String = created_at.trim().chars().take(10).collect();
    if date.is_empty() {
        "Refunded".to_string()
    } else {
        format!("Refunded on {date}")
    }
}

/// True for a decimal string the server uses for "nothing": empty, or
/// zero at any scale (`0`, `0.00`).
pub(crate) fn is_zero_amount(raw: &str) -> bool {
    let t = raw.trim();
    t.is_empty() || t.chars().all(|c| c == '0' || c == '.')
}

/// PMS-1004: the Details row for a sent invoice. The address, and the date
/// part of the timestamp when there is one; the time of day says nothing an
/// operator acts on.
pub(crate) fn emailed_line(to: &str, at: Option<&str>) -> String {
    match at.map(|a| a.chars().take(10).collect::<String>()) {
        Some(date) if !date.is_empty() => format!("{to} on {date}"),
        _ => to.to_string(),
    }
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
    /// MAPPS-640: the catalog product this line sells, when it names one.
    /// Carried through the edit modal so re-saving keeps the reference; the
    /// price is never read back through it.
    #[serde(default)]
    product_id: Option<uuid::Uuid>,
    /// PMS-1029: whether the rate applies to this line. Defaulted so an older
    /// server reads taxable, the server's own default.
    #[serde(default = "default_true")]
    is_taxable: bool,
}

fn default_true() -> bool {
    true
}

/// MAPPS-642: what Send mails, described from the server's own template.
///
/// The invoice "Pay Now" mail is built in mokosh-server's billing service
/// (`notify_invoice_pay_now`, PMS-711 and PMS-761), not by a notification
/// rule, so `POST /notifications/preview` renders nothing for it. MAPPS-539
/// put a note under the empty response; an operator read "nothing will be
/// sent" as "email is not configured" and filed MAPPS-642. So the page now
/// mirrors the template here and names the server's three conditions as
/// blockers when the page can see they fail: a billing contact, an email on
/// that contact, and a connected payment gateway. The two the page cannot
/// see (a configured mailer and a portal origin) are deployment settings and
/// are named as such in the body text.
///
/// `contact_email` is `None` for no billing contact, `Some(None)` for a
/// contact with no address, `Some(Some(_))` otherwise; `gateway` is `None`
/// while the check has not answered.
pub(crate) fn invoice_pay_now_preview(
    org_name: &str,
    invoice_number: &str,
    balance_due: &str,
    currency: &str,
    due_date: &str,
    contact_email: Option<Option<&str>>,
    gateway: Option<bool>,
) -> crate::components::BuiltinEmail {
    let org = if org_name.trim().is_empty() {
        "Your organisation"
    } else {
        org_name.trim()
    };
    let currency = if currency.trim().is_empty() {
        "USD"
    } else {
        currency.trim()
    };
    // MAPPS-663: the two real blockers are the server's 409s (PMS-992). The
    // gateway is not one since PMS-991: the invoice goes as a PDF regardless,
    // and the pay link is the part that needs a gateway.
    let mut blockers = Vec::new();
    let mut notes = Vec::new();
    let recipient = match contact_email {
        None => {
            blockers.push(
                "This invoice has no billing contact and the company has no default one, so Send is refused. Set one with Edit, or pick one on the company."
                    .to_string(),
            );
            "The billing contact (none set)".to_string()
        }
        Some(None) => {
            blockers.push(
                "The billing contact has no email address on file, so Send is refused. Add one on the contact."
                    .to_string(),
            );
            "The billing contact (no email address on file)".to_string()
        }
        Some(Some(email)) => email.to_string(),
    };
    let pay_link = match gateway {
        Some(true) => true,
        Some(false) => {
            notes.push(
                "No payment gateway is connected, so the email carries no Pay Now link. Connect one under Settings, Payment Gateways."
                    .to_string(),
            );
            false
        }
        None => {
            notes.push(
                "Could not check whether a payment gateway is connected; the Pay Now link is included only when one is."
                    .to_string(),
            );
            false
        }
    };
    // Mirrors the server's `compose_invoice_sent` (PMS-991): the same
    // paragraphs in the same order, the pay paragraph only with a gateway.
    let pay = if pay_link {
        "Review the invoice and pay online here:\n\n{{portal_link}}\n\n"
    } else {
        ""
    };
    let subject = if pay_link {
        format!("Invoice {invoice_number} from {org} is ready to pay")
    } else {
        format!("Invoice {invoice_number} from {org}")
    };
    let mut unresolved = Vec::new();
    if pay_link {
        unresolved.push("portal_link".to_string());
    }
    unresolved.push("contact_line".to_string());
    crate::components::BuiltinEmail {
        recipient,
        subject,
        body: format!(
            "{org} has sent you an invoice.\n\n\
             Invoice: {invoice_number}\n\
             Amount due: {balance_due} {currency}\n\
             Due: {due_date}\n\n\
             The invoice is attached as {invoice_number}.pdf.\n\n\
             {pay}\
             {{{{contact_line}}}}"
        ),
        unresolved,
        blockers,
        notes,
    }
}

/// The billing contact: the address for the preview's recipient line, and
/// the name for the editor's picker chip (PMS-1004).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteContactEmail {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

impl RemoteContactEmail {
    fn email(&self) -> Option<String> {
        self.email.clone().filter(|e| !e.trim().is_empty())
    }

    fn display_name(&self) -> String {
        format!("{} {}", self.first_name.trim(), self.last_name.trim())
            .trim()
            .to_string()
    }
}

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
/// MAPPS-643: what the page says in place of Edit, Cancel and Void once an
/// invoice is sent. One sentence per state, in the reader's order: what
/// the invoice is now, why nothing on it can change, then what they can do
/// from this page. The earlier wording opened with "finalized record", said
/// "cancelled, or voided" for one thing, and put the actions last.
///
/// The actions named are the ones on this page: Record Payment and Write
/// off in the header (write-off is finance only, so it is offered as a
/// possibility rather than a button), and the Credit Notes card below. A
/// written-off invoice's late payment is a recovery (PMS-1036), so that
/// state says so instead of "cannot be reinstated".
pub(crate) fn locked_invoice_note(status: &str) -> Option<&'static str> {
    match status {
        "sent" => Some(
            "This invoice was sent to your customer, so it is locked: nothing on it can change. Record a payment when it is paid, or write it off if it never will be. To correct it, issue a credit note from the Credit Notes card below.",
        ),
        "partially_paid" => Some(
            "This invoice is partly paid and locked: nothing on it can change. Record the rest as it arrives, or write off what will not be paid. To correct it, issue a credit note from the Credit Notes card below.",
        ),
        "paid" => Some(
            "This invoice is paid and locked: nothing on it can change. To refund or correct it, issue a credit note from the Credit Notes card below.",
        ),
        "void" => Some(
            "This invoice was voided. It stays on record as it was and cannot be reinstated; raise a new invoice instead.",
        ),
        "written_off" => Some(
            "This invoice was written off as unpaid. It stays on record as it was; a payment that arrives later is recorded as a recovery.",
        ),
        _ => None,
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct InvoiceDetailPageProps {
    pub id: String,
}

/// MAPPS-666 (mokosh-invoices P1a): SPA shadow of the server's
/// `InvoicePaymentReadinessResponse`. Serde defaults everywhere so a
/// server that predates P1a decodes to an all-off shape and the Pay
/// button stays hidden.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct PaymentReadiness {
    #[serde(default)]
    gateway_ready: bool,
    #[serde(default)]
    button_label: Option<String>,
    /// MAPPS-771 / PMS-1179: every way this invoice can be paid. Empty against
    /// a server that predates the choice, which is why `button_label` is still
    /// read: `pay_options` falls back to it.
    #[serde(default)]
    providers: Vec<RemotePaymentProvider>,
    #[serde(default)]
    invoice_payable: bool,
    #[serde(default)]
    balance_due_display: String,
}

/// MAPPS-771 / PMS-1179: one way the customer can pay, as the server sends it.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub(crate) struct RemotePaymentProvider {
    #[serde(default)]
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) label: String,
}

/// MAPPS-771: one Pay button.
///
/// `provider` is `None` only against a server that predates PMS-1179, where
/// the pay request names nothing and the server resolves its single active
/// gateway - which is exactly what every client did before the choice existed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PayOption {
    pub(crate) provider: Option<String>,
    pub(crate) label: String,
}

/// MAPPS-771: what to render right now, including before readiness lands.
///
/// An empty list would mean no button at all while the readiness fetch is in
/// flight, which is a button that appears late rather than one that is
/// disabled - so the pending state keeps its single unnamed button, exactly
/// what this page rendered before the choice existed.
pub(crate) fn pay_options_for_render(
    choices: &[PayOption],
    fallback_label: &str,
) -> Vec<PayOption> {
    if choices.is_empty() {
        return vec![PayOption {
            provider: None,
            label: fallback_label.to_string(),
        }];
    }
    choices.to_vec()
}

/// MAPPS-771: the Pay buttons this invoice offers.
///
/// One per provider the tenant has connected, in the order the server sends
/// them, each labelled by the server (the MSP's own override where they set
/// one). The client never names a provider itself, so adding a third one is a
/// server change alone.
///
/// A server that predates PMS-1179 sends no list and one `button_label`; that
/// becomes a single option naming no provider, which is the request every
/// client sent before the choice existed. No labels at all means nothing to
/// offer, and the caller renders no button rather than one that cannot work.
pub(crate) fn pay_options(
    providers: &[RemotePaymentProvider],
    button_label: Option<&str>,
) -> Vec<PayOption> {
    let named: Vec<PayOption> = providers
        .iter()
        .filter(|p| !p.provider.trim().is_empty() && !p.label.trim().is_empty())
        .map(|p| PayOption {
            provider: Some(p.provider.clone()),
            label: p.label.clone(),
        })
        .collect();
    if !named.is_empty() {
        return named;
    }
    match button_label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => vec![PayOption {
            provider: None,
            label: label.to_string(),
        }],
        None => Vec::new(),
    }
}

/// MAPPS-668 (mokosh-invoices P1c): body sent to POST /invoices/{id}/pay.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct PayInvoiceBody {
    success_url: String,
    cancel_url: String,
    /// MAPPS-771: which provider the customer pressed. Omitted entirely when
    /// there is nothing to choose between, so the request stays byte-identical
    /// to what the server has always accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
}

/// MAPPS-668: what the server returns from POST /invoices/{id}/pay.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct PayInvoiceResp {
    #[serde(default)]
    checkout_url: String,
}

#[component]
pub fn InvoiceDetailPage(props: InvoiceDetailPageProps) -> Element {
    // mokosh-contact-login prompt 006: Edit / Send / Void / Record
    // Payment are staff-only. Contact-facing "Pay now" does not
    // exist in the codebase today; skipped gracefully.
    let staff_only =
        crate::hooks::capabilities::use_capability(crate::hooks::capabilities::STAFF_ONLY);
    // MAPPS-727: writing off is finance only on the server (PMS-1036), so
    // the action is gated on the role and not only on the staff plane; a
    // technician sees no button rather than a 403.
    let has_finance = crate::hooks::use_auth()
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);
    // MAPPS-607: PMS-936 exposes `GET /invoices/{id}/pdf` behind the
    // `invoices:download_pdf` cap. Staff / platform sessions bypass
    // unconditionally via `use_capability`, so this button renders for
    // them without a role grant.
    let can_download_pdf = crate::hooks::capabilities::use_capability("invoices:download_pdf");
    // MAPPS-668 (P1c): the Pay Now cap. Any caller with this can pay a
    // payable invoice; the button also gates on the readiness fetch
    // and the invoice status, so a contact without the cap sees nothing
    // and a contact with the cap on a Draft sees an inline note instead
    // of a button.
    // MAPPS-707: Pay Now is contact-plane only. `use_capability` bypasses
    // the cap check for staff/platform sessions (so admins can see every
    // read affordance), which is right for Download PDF and wrong for Pay
    // Now - clicking it as a staff caller would mint a hosted-checkout
    // session for the admin's own browser. Require BOTH a live contact
    // session AND the cap so the button only renders on the portal plane.
    let can_pay = crate::hooks::capabilities::use_is_contact_session()
        && crate::hooks::capabilities::use_capability("invoices:pay");
    let _pdf_downloading = use_signal(|| false);
    let pdf_error = use_signal(String::new);
    let _id_for_pdf = props.id.clone();
    let id_for_resource = props.id.clone();
    // MAPPS-669 (P1d): post-checkout splash flag. Read `?paid=1` off
    // the boot-time search snapshot (the MAPPS-664 fix: the Dioxus
    // router strips the query on mount, so live window.location.search
    // is empty by the time this component renders).
    //
    // MAPPS-707: only the contact plane can pay, so only the contact
    // plane can land back with `?paid=1`. Gating the flag on
    // `use_is_contact_session` keeps the polling loop and the splash
    // arm off the staff detail page even if a staff caller manually
    // types the query string, so the same predicate that hides the
    // button hides its post-checkout artefacts too.
    let is_paid_landing = {
        #[cfg(feature = "app")]
        {
            let search = crate::modules::oidc::initial_search();
            let params = crate::utils::url::QueryString::parse(&search);
            params.get("paid").as_deref() == Some("1")
                && crate::hooks::capabilities::use_is_contact_session()
        }
        #[cfg(not(feature = "app"))]
        {
            false
        }
    };
    // MAPPS-669: polling state. When `is_paid_landing`, restart the
    // invoice resource every 2s until status = 'paid' or the 30s
    // budget elapses.
    let mut poll_tick = use_signal(|| 0u32);
    let id_for_readiness = props.id.clone();
    let readiness_resource = use_resource(move || {
        let id = id_for_readiness.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed_any::<PaymentReadiness>(&format!(
                "/invoices/{id}/payment-readiness"
            ))
            .await
            .inspect_err(|e| tracing::error!("payment readiness load failed for invoice {id}: {e}"))
            .ok()
        }
    });
    let mut invoice_resource = use_resource(move || {
        let id = id_for_resource.clone();
        // MAPPS-669: read the tick so a poll_tick write restarts this
        // resource. The value itself is unused; the read is the
        // subscribe.
        let _tick = poll_tick();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the invoice auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
            // MAPPS-665: pair with the invoice-list fetch at line 329
            // which already uses `get_authed_any` (contact-preferred
            // bearer). The detail fetch had stayed on the staff-only
            // `get_authed`, so a portal contact clicking through from
            // their list got a bearerless 401 and the "Could not load
            // invoice" card. The server's dual-plane get_invoice at
            // src/modules/billing/routes.rs:410 already gates the
            // contact branch on `invoices:read` + Company-scope 404,
            // so contact-first bearer selection is safe.
            crate::hooks::fetch::api::get_authed_any::<InvoiceDetail>(&format!("/invoices/{id}"))
                .await
                .inspect_err(|e| tracing::error!("invoice detail load failed for {id}: {e}"))
                .ok()
        }
    });

    // MAPPS-638: the credit notes raised against this invoice. Restarted with
    // the invoice after one is issued, so the card and the balance agree.
    let id_for_notes = props.id.clone();
    let mut credit_notes_resource = use_resource(move || {
        let id = id_for_notes.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::pages::credit_notes::load_invoice_credit_notes(&id).await
        }
    });

    // MAPPS-735: the payments and refunds behind the balance (server
    // PMS-1088). `get_authed_any` for the same reason the invoice uses it:
    // a contact reads its own invoice on the contact bearer. Reading the
    // invoice resource here subscribes this one to it, so every restart
    // the page already does (a recorded payment, a credit note, a
    // write-off, the paid-landing poll) refreshes the ledger with the
    // balance. `Some(None)` on a failure hides the card: the staff arm
    // carries the finance gate inline, so a technician is refused with a
    // 403 and must see no card rather than an error, and the summary
    // above already carries the balance.
    let id_for_ledger = props.id.clone();
    let ledger_resource = use_resource(move || {
        let id = id_for_ledger.clone();
        let _invoice = invoice_resource.read().clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed_any::<RemoteInvoiceLedger>(&format!(
                "/invoices/{id}/payments"
            ))
            .await
            .inspect_err(|e| tracing::warn!("invoice ledger load failed for {id}: {e}"))
            .ok()
        }
    });

    // MAPPS-158: detail-page lifecycle actions. `PUT /invoices/{id}`
    // freezes header/line/status edits once an invoice leaves
    // draft/pending (`InvoiceStatus::is_frozen`), so Edit, Send and Void are
    // surfaced only while the invoice is editable. Record Payment
    // (`POST /payments`) is offered whenever a balance can still be
    // collected. The backend exposes no route to delete, un-send, or email an
    // invoice, and editing line items requires a credit note (MAPPS-638, below),
    // so those actions are intentionally not surfaced here.
    let mut show_edit = use_signal(|| false);
    let mut show_payment = use_signal(|| false);
    let mut show_credit_note = use_signal(|| false);
    // MAPPS-727: the write-off dialog, its required reason, and its own
    // error so a refusal lands beside the button that produced it.
    let mut show_write_off = use_signal(|| false);
    let mut write_off_reason = use_signal(String::new);
    let mut write_off_error = use_signal(String::new);
    let id_for_write_off = props.id.clone();
    let mut busy = use_signal(|| false);
    let mut action_error = use_signal(String::new);
    // MAPPS-672: which template to render the draft preview under. Empty
    // means the tenant's own setting (no `?template=` sent), matching the
    // preview before this control existed. PMS-1006 honours `?template=`
    // only while the invoice is still editable.
    let mut preview_template = use_signal(String::new);
    // MAPPS-668 (P1c): Pay Now click state. `pay_saving` gates the button
    // while the checkout-session round trip is in flight; `pay_error`
    // surfaces a failure inline the way `action_error` and `pdf_error`
    // do.
    let mut pay_saving = use_signal(|| false);
    let mut pay_error = use_signal(String::new);
    let id_for_pay = props.id.clone();

    // MAPPS-669 (P1d): drive the post-checkout poll. When the browser
    // lands with `?paid=1` and the invoice still reads as unpaid,
    // increment `poll_tick` every 2s for up to 30s so the invoice
    // resource restarts until the Stripe webhook has landed the
    // payment. The loop always runs; it no-ops when `is_paid_landing`
    // is false so the hook count stays stable.
    use_future(move || async move {
        if !is_paid_landing {
            return;
        }
        for _ in 0..15 {
            crate::platform::timer::sleep_ms(2_000).await;
            poll_tick.with_mut(|t| *t += 1);
        }
    });

    let snap = invoice_resource.read_unchecked();
    let invoice = match &*snap {
        Some(Some(inv)) => Some(inv.clone()),
        _ => None,
    };
    // MAPPS-642: the two conditions of the pay-now email the page can check,
    // so Preview email can say whether Send will mail anyone. Both hooks sit
    // before the early returns below (MAPPS-602).
    // MAPPS-644: the invoice is read INSIDE the closure. Capturing the id
    // outside froze the value from the first render, before the invoice had
    // loaded, so the read never ran and the preview always reported the
    // contact as having no address.
    let contact_resource = use_resource(move || async move {
        let id = invoice_resource
            .read_unchecked()
            .clone()
            .flatten()
            .and_then(|i| i.billing_contact_id)?;
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<RemoteContactEmail>(&format!(
            "/contacts/contacts/{id}"
        ))
        .await
        // Best-effort: the email preview falls back to no billing address.
        .inspect_err(|e| tracing::warn!("invoice billing contact load failed for {id}: {e}"))
        .ok()
    });
    let gateway_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // "no gateway is live" and "the gateway list did not load" both hide
        // the pay affordance, so each says which it is.
        match crate::hooks::fetch::api::get_all_authed::<RemoteGateway>("/payment-gateways").await {
            Ok(list) => {
                let live = list.iter().any(|g| g.is_active && g.configured);
                if !live {
                    tracing::info!(
                        "payment gateway load succeeded and none is active and configured"
                    );
                }
                Some(live)
            }
            Err(e) => {
                tracing::error!("payment gateway load failed, hiding the pay affordance: {e}");
                None
            }
        }
    });

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
    // MAPPS-727: PMS-1036 writes off a sent or partially paid invoice.
    let write_offable = can_write_off(status.as_str());
    // MAPPS-638: a credit note corrects a frozen invoice, and only while
    // something is left to credit: the total less what is already credited,
    // and NOT less what was paid, because a paid invoice can be credited in
    // full (that is the case where the customer is owed money back). Absent
    // rather than disabled on a draft: the server refuses to credit a
    // document that can still be edited, and a control that can never work
    // should not be on the page.
    let frozen = matches!(
        status.as_str(),
        "sent" | "partially_paid" | "paid" | "void" | "written_off"
    );
    let remaining_to_credit = invoice
        .as_ref()
        .and_then(|i| {
            crate::pages::credit_notes::credit_note_math::remaining_to_credit(
                &i.total,
                &i.amount_credited,
            )
        })
        .unwrap_or(Decimal::ZERO);
    let creditable = frozen && remaining_to_credit > Decimal::ZERO;
    // PMS-580: a frozen invoice (sent and beyond) is a finalized financial
    // record. There is no edit / cancel / void once sent; correction goes
    // through a credit note (MAPPS-638, the Credit Notes card). Say so inline so the
    // missing actions read as intentional rather than broken. Draft / pending
    // show nothing here (their actions, including Void, are available above).
    let frozen_note = locked_invoice_note(status.as_str());
    let pay_company_id = invoice
        .as_ref()
        .and_then(|i| i.company_id)
        .map(|c| c.to_string())
        .unwrap_or_default();
    let id_for_send = props.id.clone();
    let id_for_void = props.id.clone();
    let act_err = action_error.read().clone();

    // PMS-1004: a Send the server refused with a 409 (no billing contact, or
    // one with no address, PMS-992). The reason, kept apart from
    // `action_error` because it gets a panel with the ways out rather than a
    // bare banner: pick the contact here, or mark the invoice sent without
    // emailing (`skip_email`, the server's explicit path for an invoice
    // delivered another way).
    let mut send_blocked = use_signal(|| None::<String>);
    let mut confirming_skip = use_signal(|| false);
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
            #[cfg(feature = "app")]
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

    // MAPPS-727: the write-off POST fires from the dialog once a reason is
    // typed. A refusal (a 409 naming the status, a 403) stays in the dialog.
    let mut on_confirm_write_off = move |_: ()| {
        if *busy.read() {
            return;
        }
        let reason = write_off_reason.read().trim().to_string();
        if reason.is_empty() {
            write_off_error.set("A reason is required.".to_string());
            return;
        }
        busy.set(true);
        write_off_error.set(String::new());
        let path = format!("/invoices/{id_for_write_off}/write-off");
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({ "reason": reason });
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => {
                        write_off_reason.set(String::new());
                        show_write_off.set(false);
                        invoice_resource.restart();
                    }
                    Err(err) => write_off_error.set(format!("Could not write off invoice: {err}")),
                }
            }
            busy.set(false);
        });
    };

    // PMS-1004: the explicit no-email send. The same transition Send makes,
    // with `skip_email`, so the server records that nobody was emailed
    // rather than refusing.
    let id_for_skip = props.id.clone();
    let on_confirm_skip = move |_: ()| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        let path = invoice_send_path(&id_for_skip);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({ "status": "sent", "skip_email": true });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => {
                        send_blocked.set(None);
                        invoice_resource.restart();
                    }
                    Err(err) => action_error.set(format!("Could not mark the invoice sent: {err}")),
                }
            }
            busy.set(false);
            confirming_skip.set(false);
        });
    };
    // PMS-1004: setting the billing contact from the blocked-send panel. A
    // plain field update; the operator then sends again, on purpose.
    let id_for_contact = props.id.clone();
    let on_pick_contact = move |(contact_id, _name): (String, String)| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        let path = format!("/invoices/{id_for_contact}");
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({ "billing_contact_id": contact_id });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => {
                        send_blocked.set(None);
                        invoice_resource.restart();
                    }
                    Err(err) => {
                        action_error.set(format!("Could not set the billing contact: {err}"))
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (path, contact_id);
            }
            busy.set(false);
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
    // MAPPS-668 (P1c): snapshot the readiness fetch into locals so the
    // button, its tooltip, and the empty-state note read one consistent
    // shape. Anything failing (server predates P1a, network hiccup,
    // JSON decode) collapses to the all-off default via serde defaults
    // on `PaymentReadiness`; the button then hides rather than
    // pretending to be ready.
    let readiness_snap = readiness_resource.read_unchecked();
    let readiness = match &*readiness_snap {
        Some(Some(r)) => Some(r.clone()),
        _ => None,
    };
    let gateway_ready = readiness.as_ref().is_some_and(|r| r.gateway_ready);
    let invoice_payable = readiness.as_ref().is_some_and(|r| r.invoice_payable);
    // MAPPS-771: one button per connected provider. An older server sends one
    // label and no list, which becomes a single unnamed option.
    let pay_choices = readiness
        .as_ref()
        .map(|r| pay_options(&r.providers, r.button_label.as_deref()))
        .unwrap_or_default();
    // Kept for the case where readiness has not landed yet: the button still
    // needs words on it, and "Pay Now" is what it said before any of this.
    let pay_fallback_label = "Pay Now".to_string();
    let pay_err = pay_error.read().clone();
    let is_paid = status == "paid";

    rsx! {
        // MAPPS-727: the write-off dialog. A Modal rather than ConfirmDialog
        // because the reason is required and typed here (PMS-1036 records it
        // as the audit trail), and ConfirmDialog carries no field.
        if show_write_off() {
            Modal {
                open: true,
                title: "Write off invoice",
                onclose: move |_| {
                    if !*busy.read() {
                        show_write_off.set(false);
                        write_off_error.set(String::new());
                    }
                },
                footer: rsx! {
                    div { class: "flex-1" }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            if !*busy.read() {
                                show_write_off.set(false);
                                write_off_error.set(String::new());
                            }
                        },
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *busy.read(),
                        disabled: !can_mutate || write_off_reason.read().trim().is_empty(),
                        title: (!can_mutate).then(|| "Can't write off while the server is unreachable".to_string()),
                        onclick: move |_| on_confirm_write_off(()),
                        "Write off"
                    }
                },
                div { class: "space-y-4",
                    if !write_off_error.read().is_empty() {
                        ErrorBanner { "{write_off_error.read()}" }
                    }
                    p { class: "text-sm text-muted",
                        "Your customer owes this balance and will not pay it. The invoice moves to written off and keeps its balance on record as a bad-debt expense; a payment that arrives later is recorded as a recovery. This is not a correction - use a credit note for that."
                    }
                    crate::components::Textarea {
                        name: "write_off_reason",
                        label: "Reason",
                        placeholder: "Why you are not collecting this balance (required)",
                        rows: 3,
                        maxlength: 2000,
                        required: true,
                        value: write_off_reason.read().clone(),
                        oninput: move |e: FormEvent| write_off_reason.set(e.value()),
                    }
                }
            }
        }
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
        crate::components::ConfirmDialog {
            open: confirming_skip(),
            title: "Mark as sent without emailing".to_string(),
            message: "Mark this invoice as sent without emailing it? It becomes a finalized record, exactly as a sent invoice does, and it records that nobody was emailed. Use this when the invoice is delivered another way.".to_string(),
            confirm_text: "Mark as sent".to_string(),
            cancel_text: "Cancel".to_string(),
            loading: *busy.read(),
            onconfirm: on_confirm_skip,
            oncancel: move |_| {
                if !*busy.read() {
                    confirming_skip.set(false);
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
                // MAPPS-668 (P1c): Pay Now, contact-plane only via the
                // `invoices:pay` cap. Gated to render on invoices whose
                // status can still receive a payment (mirrors the
                // server's InvoiceStatus refusal set on
                // `POST /invoices/{id}/pay`). Enabled only when the
                // readiness fetch confirms a live gateway; disabled
                // with a tooltip otherwise so the empty state is
                // legible rather than mystifying. A cap holder on a
                // Draft (still no `invoice_payable`) sees no button at
                // all: those don't have "pay this" as an option, only
                // "wait for a real invoice". P1d spawns the polling
                // loop and the splash below.
                if can_pay && !is_paid {
                    if invoice_payable {
                        // MAPPS-771: one button per provider the tenant has
                        // connected. With one connected this renders exactly
                        // what it always did; with two the customer chooses,
                        // and the choice rides on the pay request.
                        for option in pay_options_for_render(&pay_choices, &pay_fallback_label) {
                            {
                                // Per button, because each closure needs its
                                // own copies: the invoice id and the provider
                                // this button pays through.
                                let id_for_pay = id_for_pay.clone();
                                let chosen_provider = option.provider.clone();
                                let label = option.label.clone();
                                rsx! {
                                    Button {
                                        key: "{label}",
                                        variant: ButtonVariant::Primary,
                                        loading: *pay_saving.read(),
                                        disabled: !can_mutate || !gateway_ready || *pay_saving.read(),
                                        title: if !can_mutate {
                                            Some("Can't start a payment while the server is unreachable".to_string())
                                        } else if !gateway_ready {
                                            Some("Your MSP has not connected an online payment provider yet. Contact them to arrange payment.".to_string())
                                        } else {
                                            None
                                        },
                                        onclick: move |_| {
                                            if *pay_saving.read() {
                                                return;
                                            }
                                            pay_error.set(String::new());
                                            pay_saving.set(true);
                                            let id = id_for_pay.clone();
                                            // MAPPS-771: captured per button, so the
                                            // request names the one that was pressed.
                                            let chosen_provider = chosen_provider.clone();
                                            spawn(async move {
                                                #[cfg(feature = "app")]
                                                {
                                                    let origin = crate::platform::location::origin().unwrap_or_default();
                                                    // MAPPS-762: the provider sends the
                                                    // customer back here after they pay,
                                                    // so the path is taken FROM THE ROUTER
                                                    // rather than typed. It used to read
                                                    // `/portal/invoices/{id}`, a route
                                                    // retired with the customer-portal
                                                    // family, so a successful payment
                                                    // landed the customer on the 404 page
                                                    // holding a charged card. A rename of
                                                    // this route now moves the return URL
                                                    // with it.
                                                    let path = Route::InvoiceDetail { id: id.clone() }.to_string();
                                                    let body = PayInvoiceBody {
                                                        success_url: format!("{origin}{path}?paid=1"),
                                                        cancel_url: format!("{origin}{path}"),
                                                        // MAPPS-771: the button the
                                                        // customer actually pressed.
                                                        provider: chosen_provider.clone(),
                                                    };
                                                    match crate::hooks::fetch::api::post_authed_any_typed::<PayInvoiceResp, _>(
                                                        &format!("/invoices/{id}/pay"),
                                                        &body,
                                                    ).await {
                                                        Ok(resp) if !resp.checkout_url.is_empty() => {
                                                            #[cfg(target_arch = "wasm32")]
                                                            {
                                                                if let Some(win) = web_sys::window() {
                                                                    let _ = win.location().replace(&resp.checkout_url);
                                                                    return;
                                                                }
                                                            }
                                                            #[cfg(not(target_arch = "wasm32"))]
                                                            {
                                                                // Desktop shell has no
                                                                // location redirect; the
                                                                // provider checkout is
                                                                // browser-only, so a
                                                                // portal contact on the
                                                                // desktop app hits this
                                                                // path and is told to open
                                                                // the portal in a browser.
                                                                let _ = &resp;
                                                            }
                                                            pay_error.set(
                                                                "Payment checkout is only available in the web portal. Open your portal in a browser to pay.".to_string(),
                                                            );
                                                        }
                                                        Ok(_) => {
                                                            pay_error.set(
                                                                "Payment provider returned an empty response. Try again.".to_string(),
                                                            );
                                                        }
                                                        Err(err) => {
                                                            pay_error.set(format!(
                                                                "Could not start payment: {}",
                                                                err.user_message()
                                                            ));
                                                        }
                                                    }
                                                }
                                                pay_saving.set(false);
                                            });
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
                // MAPPS-641: the document. A sent invoice's PDF is the bytes
                // stored when it was sent (PMS-959), carrying the identity of
                // that day (PMS-911); a draft renders live and is a preview.
                // The two must not read the same, or a draft PDF becomes a
                // record in somebody's mind. `can_download_pdf` gates the
                // contact plane; staff+platform pass through `use_capability`
                // unconditionally.
                if can_download_pdf {
                    if let Some(inv) = invoice.as_ref() {
                        // MAPPS-672: on a draft, try another template before
                        // committing to it in settings. No live render here
                        // either: `?template=` asks the server to render the
                        // invoice's own data under the named template, which
                        // is PMS-1006's whole point. Nothing is saved by
                        // picking one; the tenant-wide choice is still made
                        // on the organization settings page.
                        if editable {
                            crate::components::Select {
                                name: "invoice_preview_template",
                                label: "Preview template".to_string(),
                                options: vec![
                                    crate::components::SelectOption::new("", "Your current template"),
                                    crate::components::SelectOption::new("classic", "Classic"),
                                    crate::components::SelectOption::new("modern", "Modern"),
                                    crate::components::SelectOption::new("compact", "Compact"),
                                ],
                                value: preview_template(),
                                help: "Shows this draft under another template. Nothing is saved.".to_string(),
                                onchange: move |e: FormEvent| preview_template.set(e.value()),
                            }
                        }
                        crate::components::DownloadButton {
                            path: if editable && !preview_template.read().is_empty() {
                                format!("/invoices/{}/pdf?template={}", props.id, preview_template())
                            } else {
                                format!("/invoices/{}/pdf", props.id)
                            },
                            fallback_name: format!("{}.pdf", inv.invoice_number),
                            what: "the invoice PDF".to_string(),
                            label: if editable { "Preview PDF".to_string() } else { "Download PDF".to_string() },
                            title: if editable {
                                "Shows this draft as it would look now. Nothing is stored until you send it.".to_string()
                            } else {
                                "The invoice as your customer received it. Stored at that moment, so rebranding since does not change it.".to_string()
                            },
                        }
                    }
                }
                if editable && staff_only {
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
                                #[cfg(feature = "app")]
                                {
                                    let body = serde_json::json!({ "status": "sent" });
                                    // Typed, so a 409 can be told from any
                                    // other refusal (PMS-1004).
                                    match crate::hooks::fetch::api::put_authed_typed::<
                                        serde_json::Value,
                                        _,
                                    >(&path, &body)
                                        .await
                                    {
                                        Ok(_) => {
                                            send_blocked.set(None);
                                            invoice_resource.restart();
                                        }
                                        // PMS-1004: a 409 is "nobody to email",
                                        // and it gets the panel with the ways
                                        // out rather than the error banner.
                                        Err(crate::hooks::fetch::api::ApiError::Status {
                                            code: 409,
                                            message,
                                            ..
                                        }) => send_blocked.set(Some(message)),
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
                        // MAPPS-642: the server-built message, with the
                        // conditions under which Send mails nobody.
                        builtin: invoice.as_ref().map(|inv| {
                            let contact_email: Option<Option<String>> = inv
                                .billing_contact_id
                                .map(|_| {
                                    contact_resource
                                        .read_unchecked()
                                        .clone()
                                        .flatten()
                                        .and_then(|c| c.email())
                                });
                            invoice_pay_now_preview(
                                crate::hooks::use_auth()
                                    .read()
                                    .active_org_name()
                                    .unwrap_or_default(),
                                &inv.invoice_number,
                                &inv.balance_due,
                                inv.currency.as_deref().unwrap_or_default(),
                                inv.due_date.as_deref().unwrap_or_default(),
                                contact_email.as_ref().map(|e| e.as_deref()),
                                (*gateway_resource.read_unchecked()).flatten(),
                            )
                        }),
                    }
                }
                if collectible && staff_only {
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
                // MAPPS-727 (PMS-1036): the bad-debt record for a sent
                // balance that will not be paid. Finance only, like the
                // route.
                if write_offable && staff_only && has_finance {
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't write off while the server is unreachable".to_string()),
                        onclick: move |_| {
                            action_error.set(String::new());
                            write_off_error.set(String::new());
                            show_write_off.set(true);
                        },
                        "Write off"
                    }
                }
                // PMS-953 (MAPPS-638): a credit note is the correction path
                // for a frozen invoice. Staff-only per the contact-login
                // stance; contact plane never issues a credit note.
                if creditable && staff_only {
                    Button {
                        variant: ButtonVariant::Secondary,
                        // MAPPS-357: block raising a credit note while down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't raise a credit note while the server is unreachable".to_string()),
                        onclick: move |_| {
                            action_error.set(String::new());
                            show_credit_note.set(true);
                        },
                        "Create Credit Note"
                    }
                }
                if editable && staff_only {
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
        // button alone cannot say so. Since PMS-991 and PMS-992 the rule is
        // the server's: the invoice goes as a PDF to the billing contact, the
        // pay link rides along only with a payment gateway, and a send with
        // nobody to email is refused rather than marked sent (MAPPS-663).
        if editable {
            p { class: "mb-3 text-xs text-subtle",
                "Sending emails the invoice to your customer's billing contact as a PDF, with a link to pay online if a payment gateway is connected. That contact needs an email address. Use Preview email to read it first."
            }
        }

        if !act_err.is_empty() {
            ErrorBanner { class: "mb-3", "{act_err}" }
        }
        // MAPPS-607: PDF download failures land inline just like the
        // action errors above. Kept separate so a lifecycle-action
        // toast and a PDF fetch failure do not fight for the same
        // banner slot.
        if !pdf_error.read().is_empty() {
            ErrorBanner { class: "mb-3", "{pdf_error}" }
        }
        // MAPPS-668 (P1c): checkout-session failures surface on the
        // same page the button lives on. Kept separate from the two
        // above for the same reason.
        if !pay_err.is_empty() {
            ErrorBanner { class: "mb-3", "{pay_err}" }
        }

        // PMS-1004: the server refused Send because there is nobody to email.
        // Its sentence names the company or the contact; the panel offers the
        // ways out it points at, in place, rather than leaving the operator
        // to find them.
        if let Some(reason) = send_blocked.read().clone() {
            crate::components::StatusBanner {
                tone: crate::components::BannerTone::Warning,
                class: "mb-3",
                p { class: "font-medium", "The invoice was not sent." }
                p { class: "mt-1", "{reason}" }
                div { class: "mt-3 space-y-3",
                    crate::components::ContactPicker {
                        value: String::new(),
                        selected_id: None,
                        label: "Billing contact for this invoice".to_string(),
                        company_filter: (!pay_company_id.is_empty()).then(|| pay_company_id.clone()),
                        onselect: on_pick_contact,
                        onclear: move |_| {},
                    }
                    div { class: "flex flex-wrap items-center gap-3",
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            disabled: *busy.read(),
                            onclick: move |_| confirming_skip.set(true),
                            "Mark as sent without emailing"
                        }
                        if !pay_company_id.is_empty() {
                            Link {
                                to: Route::CompanyDetail { id: pay_company_id.clone() },
                                class: "text-sm text-accent hover:opacity-90",
                                "Open the company to set its default billing contact"
                            }
                        }
                    }
                }
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
                            to: Route::InvoiceList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to invoices"
                        }
                    }
                }
            },
            Some(Some(inv)) if is_paid_landing && inv.status != "paid" => rsx! {
                // MAPPS-669 (P1d): post-checkout splash. The Stripe /
                // PayPal webhook writes the payment; until the tick
                // catches up we keep the invoice body hidden so the
                // customer sees "processing", not "still unpaid, click
                // Pay Now again". `use_future` above increments
                // `poll_tick` every 2s for 30s, which restarts the
                // invoice resource; once its status flips to `paid`
                // this arm stops matching and the paid invoice body
                // renders.
                Card {
                    div { class: "py-10 text-center",
                        div { class: "mx-auto mb-3 h-6 w-6 rounded-full border-2 border-accent border-t-transparent animate-spin" }
                        p { class: "text-sm font-medium text-content mb-1", "Processing your payment…" }
                        p { class: "text-xs text-muted",
                            "This usually takes a few seconds. You'll see the paid receipt as soon as your provider confirms."
                        }
                    }
                }
            },
            Some(Some(inv)) => {
                let (status_variant, status_label) = invoice_status_badge(&inv.status);
                let overdue_days = inv.is_overdue.then_some(inv.days_overdue);
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
                // MAPPS-768 / PMS-1173: the name, for the contact plane, which
                // gets no link to follow. Empty against a server that predates
                // the field, and the cell then renders a dash.
                let billing_contact_name = inv.billing_contact_name.clone().unwrap_or_default();
                let emailed = inv
                    .emailed_to
                    .as_deref()
                    .map(|to| emailed_line(to, inv.emailed_at.as_deref()));
                // MAPPS-727: the write-off block, when there was one.
                let write_off = inv.written_off_at.as_deref().map(|at| {
                    write_off_line(
                        &format_money_str(inv.write_off_amount.as_deref().unwrap_or("0")),
                        Some(at),
                        inv.written_off_by_name.as_deref(),
                    )
                });
                let write_off_reason_text = inv.write_off_reason.clone().unwrap_or_default();
                let invoice_date = inv.invoice_date.clone().unwrap_or_default();
                let due_date = inv.due_date.clone().unwrap_or_default();
                let subtotal = format_money_str(&inv.subtotal);
                let tax_amount = format_money_str(&inv.tax_amount);
                let tax_label = tax_label(inv.tax_rate.as_deref());
                let discount_amount = format_money_str(&inv.discount_amount);
                let total = format_money_str(&inv.total);
                let amount_paid = format_money_str(&inv.amount_paid);
                let amount_credited = format_money_str(&inv.amount_credited);
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
                                                    TableCell {
                                                        "{line.description}"
                                                        if !line.is_taxable {
                                                            span { class: "ml-2 text-xs text-muted", "tax exempt" }
                                                        }
                                                    }
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
                                                span { class: "text-muted", "{tax_label}" }
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
                                        div { class: "flex flex-wrap items-center justify-end gap-1",
                                            Badge { variant: status_variant, "{status_label}" }
                                            if let Some(days) = overdue_days {
                                                Badge { variant: BadgeVariant::Orange, "{overdue_label(days)}" }
                                            }
                                        }
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
                                        span { class: "text-muted", "Credited" }
                                        span { class: "font-medium", "{amount_credited}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Balance Due" }
                                        span { class: "text-lg font-bold", "{balance_due}" }
                                    }
                                }
                            }

                            // MAPPS-638: the corrections raised against this
                            // invoice, and the place to raise one.
                            // MAPPS-735: the Payments card. Rendered only once
                            // the ledger has loaded; a refused or failed read
                            // (a technician's 403 from the inline finance gate,
                            // an older server) shows nothing, because the
                            // balance above already says what was paid.
                            {
                                let ledger = ledger_resource.read_unchecked().clone().flatten();
                                rsx! {
                                    if let Some(ledger) = ledger {
                                        Card { title: "Payments",
                                            if ledger.payments.is_empty() {
                                                p { class: "text-sm text-muted", "No payments recorded." }
                                            } else {
                                                ul { class: "space-y-2 text-sm",
                                                    for payment in ledger.payments.iter() {
                                                        li { key: "{payment.id.map(|u| u.to_string()).unwrap_or_default()}", class: "flex justify-between items-center gap-2",
                                                            span { "{payment_line(&payment.payment_date, &payment.payment_method, payment.reference_number.as_deref())}" }
                                                            span { class: "font-medium", "{format_money_str(&payment.amount)}" }
                                                        }
                                                        for refund in ledger.refunds.iter().filter(|r| r.payment_id.is_some() && r.payment_id == payment.id) {
                                                            li { key: "{refund.id.map(|u| u.to_string()).unwrap_or_default()}", class: "flex justify-between items-center gap-2 pl-4 text-muted",
                                                                span { "{refund_line(&refund.created_at)}" }
                                                                span { "-{format_money_str(&refund.amount)}" }
                                                            }
                                                        }
                                                    }
                                                }
                                                p { class: "mt-3 text-sm text-muted",
                                                    "Total paid {format_money_str(&ledger.total_paid)}"
                                                    if !is_zero_amount(&ledger.total_refunded) {
                                                        ", refunded {format_money_str(&ledger.total_refunded)}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            Card { title: "Credit Notes",
                                {
                                    let notes = credit_notes_resource.read_unchecked().clone().unwrap_or_default();
                                    rsx! {
                                        if notes.is_empty() {
                                            p { class: "text-sm text-muted",
                                                if creditable {
                                                    "No credit notes. Use Create Credit Note to correct this invoice."
                                                } else if frozen {
                                                    "No credit notes, and nothing is left to credit."
                                                } else {
                                                    "A credit note can be raised once this invoice has been sent."
                                                }
                                            }
                                        } else {
                                            ul { class: "space-y-2 text-sm",
                                                for note in notes.iter() {
                                                    li { key: "{note.id}", class: "flex justify-between items-center gap-2",
                                                        Link {
                                                            to: Route::CreditNoteDetail { id: note.id.to_string() },
                                                            class: "font-medium text-accent hover:opacity-90",
                                                            "{note.credit_note_number}"
                                                        }
                                                        span { class: "text-muted", "{format_money_str(&note.total)}" }
                                                        {
                                                            let (variant, label) = crate::components::credit_note_status_badge(&note.status);
                                                            rsx! { Badge { variant, "{label}" } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if creditable {
                                            div { class: "mt-3",
                                                Button {
                                                    variant: ButtonVariant::Secondary,
                                                    size: ButtonSize::Small,
                                                    disabled: !can_mutate,
                                                    title: (!can_mutate).then(|| "Can't raise a credit note while the server is unreachable".to_string()),
                                                    onclick: move |_| {
                                                        action_error.set(String::new());
                                                        show_credit_note.set(true);
                                                    },
                                                    "Create Credit Note"
                                                }
                                            }
                                        }
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
                                                // MAPPS-768: same reason as the
                                                // billing contact below. The
                                                // company page is staff-only,
                                                // and it is the customer's own
                                                // company, so the name is all
                                                // there is to say.
                                                if staff_only {
                                                    Link {
                                                        to: Route::CompanyDetail { id: cid.clone() },
                                                        class: "text-accent hover:opacity-90",
                                                        "{company_name}"
                                                    }
                                                } else {
                                                    "{company_name}"
                                                }
                                            }
                                        }
                                    }
                                    if let Some(sent_to) = emailed.clone() {
                                        div { class: "flex justify-between gap-4",
                                            dt { class: "text-muted shrink-0", "Emailed to" }
                                            dd { class: "text-right break-all", "{sent_to}" }
                                        }
                                    }
                                    if let Some(wo) = write_off.clone() {
                                        div { class: "flex justify-between gap-4",
                                            dt { class: "text-muted shrink-0", "Written off" }
                                            dd { class: "text-right", "{wo}" }
                                        }
                                        if !write_off_reason_text.is_empty() {
                                            div { class: "flex justify-between gap-4",
                                                dt { class: "text-muted shrink-0", "Reason" }
                                                dd { class: "text-right whitespace-pre-line", "{write_off_reason_text}" }
                                            }
                                        }
                                    }
                                    // MAPPS-768: a portal customer was offered
                                    // "View contact", which is the STAFF
                                    // contacts page. Following it landed them
                                    // on "Contact not found" with Edit and
                                    // Delete buttons, because the guard's
                                    // staff-only block reads the browser
                                    // pathname in a LAYOUT, and a layout does
                                    // not re-render when a link inside it is
                                    // clicked. The block still catches a typed
                                    // URL; not offering the link is what makes
                                    // it unreachable by hand.
                                    if let Some(bcid) = billing_contact_id.clone() {
                                        div { class: "flex justify-between",
                                            dt { class: "text-muted", "Billing Contact" }
                                            dd {
                                                if staff_only {
                                                    Link {
                                                        to: Route::ContactDetail { id: bcid.clone() },
                                                        class: "text-accent hover:opacity-90",
                                                        "View contact"
                                                    }
                                                } else if !billing_contact_name.is_empty() {
                                                    // The customer already knows who they are;
                                                    // the name is the useful half and the only
                                                    // half they can act on.
                                                    "{billing_contact_name}"
                                                } else {
                                                    span { class: "text-subtle", "-" }
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
                    company_id: inv.company_id.map(|c| c.to_string()).unwrap_or_default(),
                    billing_contact_id: inv.billing_contact_id.map(|c| c.to_string()).unwrap_or_default(),
                    billing_contact_name: contact_resource
                        .read_unchecked()
                        .clone()
                        .flatten()
                        .map(|c| c.display_name())
                        .unwrap_or_default(),
                    invoice_date: inv.invoice_date.clone().unwrap_or_default(),
                    due_date: inv.due_date.clone().unwrap_or_default(),
                    payment_term_id: inv.payment_term_id.clone().unwrap_or_default(),
                    po_number: inv.po_number.clone().unwrap_or_default(),
                    notes: inv.notes.clone().unwrap_or_default(),
                    lines: inv.lines.clone().unwrap_or_default(),
                    tax_rate_id: inv.tax_rate_id.map(|r| r.to_string()).unwrap_or_default(),
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

        // MAPPS-638: the create form. Mounted here and nowhere else, because a
        // credit note is always about one invoice.
        if *show_credit_note.read() {
            if let Some(inv) = invoice.clone() {
                crate::pages::credit_notes::CreditNoteFormModal {
                    invoice_id: props.id.clone(),
                    invoice_number: inv.invoice_number.clone(),
                    invoice_total: inv.total.clone(),
                    amount_credited: inv.amount_credited.clone(),
                    onclose: move |_| show_credit_note.set(false),
                    oncreated: move |_id: String| {
                        show_credit_note.set(false);
                        invoice_resource.restart();
                        credit_notes_resource.restart();
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
    // MAPPS-300: pre-fill `company_id` from the URL so the Company detail
    // "New Invoice" CTA lands on a form already scoped to that company.
    let mut company_id =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    let mut company_name = use_signal(String::new);
    // PMS-1004: the contact the invoice is emailed to. Optional here (the
    // company's default billing contact covers it), scoped to the company.
    let mut billing_contact_id = use_signal(String::new);
    let mut billing_contact_name = use_signal(String::new);
    let mut invoice_date = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    // MAPPS-662: the payment term, seeded with the tenant's default once the
    // lookup loads. The due date may be left blank; the server derives it
    // from the invoice date plus the term's days (PMS-990).
    let mut payment_term_id = use_signal(String::new);
    let mut term_seeded = use_signal(|| false);
    let terms_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<PaymentTermOpt>("/payment-terms")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: with no lookup the server still applies its
                // default term.
                tracing::warn!("payment-term load failed: {e}");
                Vec::new()
            })
    });
    let terms: Vec<PaymentTermOpt> = terms_resource.read_unchecked().clone().unwrap_or_default();
    if !*term_seeded.read() && !terms.is_empty() {
        if let Some(default) = terms.iter().find(|t| t.is_default && t.is_active) {
            payment_term_id.set(default.id.to_string());
        }
        term_seeded.set(true);
    }
    let term_options: Vec<SelectOption> = std::iter::once(SelectOption::new("", "Default term"))
        .chain(
            terms
                .iter()
                .filter(|t| t.is_active)
                .map(|t| SelectOption::new(t.id.to_string(), t.name.clone())),
        )
        .collect();
    let mut po_number = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut line_description = use_signal(String::new);
    let mut line_quantity = use_signal(|| "1".to_string());
    let mut line_unit_price = use_signal(String::new);
    // MAPPS-640: the catalog product the line sells, when one was picked.
    let mut line_product_id = use_signal(String::new);
    let mut line_product_name = use_signal(String::new);
    // MAPPS-712: whether the rate applies to the line. A product line follows
    // the product's own flag, which the server copies and the form shows
    // read-only.
    let mut line_is_taxable = use_signal(|| true);
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
    // Tax previewed from the selected rate (else the tenant's default) and the
    // single line's subtotal (qty * unit price) when the line is taxable;
    // recomputes as any input changes. A manual edit to the Tax field
    // overrides it until another rate is picked. The server's figure is the
    // one shown after save (MAPPS-712).
    let computed_tax = use_memo(move || {
        let qty = Decimal::from_str(line_quantity.read().trim()).unwrap_or_default();
        let price = Decimal::from_str(line_unit_price.read().trim()).unwrap_or_default();
        let subtotal = if *line_is_taxable.read() {
            (qty * price).to_string()
        } else {
            Decimal::ZERO.to_string()
        };
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
        // MAPPS-662: the due date is optional; blank is derived server-side.
        due_date_error.set(String::new());
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
        // MAPPS-712: the rate, or the override, never both; the server derives
        // the amount from the rate over the taxable lines (PMS-1029).
        let tax = tax_body_fields(&tax_rate_id.read(), tax_override.read().as_deref());
        let body = serde_json::json!({
            "company_id": company_uuid,
            "billing_contact_id": optional_string(&billing_contact_id.read()),
            "invoice_date": inv_date,
            // MAPPS-662: blank is null, and the server derives it from the term.
            "due_date": optional_string(&due),
            "payment_term_id": optional_string(&payment_term_id.read()),
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
            "tax_rate_id": tax.rate_id,
            "tax_amount": tax.amount,
            "discount_amount": optional_string(&discount_amount.read()),
            "lines": [{
                "line_type": "service",
                // MAPPS-640: the reference, not the price; the price below is
                // what was picked or typed and is what the line keeps.
                "product_id": optional_string(&line_product_id.read()),
                // Ignored by the server for a product line, which copies the
                // product's flag (PMS-1029).
                "is_taxable": *line_is_taxable.read(),
                "description": description,
                // Quantities/prices are decimals; the server parses the
                // string into `rust_decimal::Decimal`.
                "quantity": quantity,
                "unit_price": unit_price,
                "sort_order": 0,
            }],
        });
        spawn(async move {
            #[cfg(feature = "app")]
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
            "billing_contact_id": optional_string(&billing_contact_id.read()),
            "invoice_date": inv_date,
            "due_date": due,
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
        });
        spawn(async move {
            #[cfg(feature = "app")]
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
                        let changed = *company_id.read() != id;
                        company_id.set(id);
                        company_name.set(name);
                        company_error.set(String::new());
                        if changed {
                            billing_contact_id.set(String::new());
                            billing_contact_name.set(String::new());
                        }
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                        billing_contact_id.set(String::new());
                        billing_contact_name.set(String::new());
                    },
                }
                // PMS-1004: who the invoice is emailed to (PMS-992). Shown
                // once a company is picked, because the search is scoped to
                // it; left empty, the company's default billing contact
                // applies at send.
                if !company_id.read().is_empty() {
                    crate::components::ContactPicker {
                        value: billing_contact_name.read().clone(),
                        selected_id: {
                            let id = billing_contact_id.read().clone();
                            (!id.is_empty()).then_some(id)
                        },
                        label: "Billing Contact".to_string(),
                        company_filter: Some(company_id.read().clone()),
                        onselect: move |(id, name): (String, String)| {
                            billing_contact_id.set(id);
                            billing_contact_name.set(name);
                        },
                        onclear: move |_| {
                            billing_contact_id.set(String::new());
                            billing_contact_name.set(String::new());
                        },
                    }
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
                        help: {
                            // MAPPS-662: say what blank will become, so the
                            // derived date is seen before Create, not after.
                            let selected = payment_term_id.read().clone();
                            let term = terms.iter().find(|t| t.id.to_string() == selected)
                                .or_else(|| terms.iter().find(|t| t.is_default && t.is_active));
                            match (term, derived_due_date(&invoice_date.read(), term.and_then(|t| t.net_days))) {
                                (Some(t), Some(date)) => format!("Leave blank to use {} from the invoice date: {date}.", t.name),
                                (None, Some(date)) => format!("Leave blank for thirty days from the invoice date: {date}."),
                                (Some(t), None) => format!("Leave blank to derive it from {} once the invoice date is set.", t.name),
                                (None, None) => "Leave blank to derive it from the payment term.".to_string(),
                            }
                        },
                        value: due_date.read().clone(),
                        error: due_date_error.read().clone(),
                        oninput: move |e: FormEvent| {
                            due_date_error.set(String::new());
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
                    maxlength: 100,
                    value: po_number.read().clone(),
                    oninput: move |e: FormEvent| po_number.set(e.value()),
                }

                div {
                    h3 { class: "text-sm font-medium text-content mb-3", "Line Item" }
                    // MAPPS-640: pick from the price list. A pick fills the
                    // description and the unit price and keeps the reference;
                    // both stay editable, and the price on the line is what
                    // is charged whatever the catalog says later.
                    div { class: "mb-3",
                        crate::components::ProductPicker {
                            value: line_product_name.read().clone(),
                            selected_id: {
                                let id = line_product_id.read().clone();
                                (!id.is_empty()).then_some(id)
                            },
                            label: "From the price list",
                            onselect: move |picked: crate::components::PickedProduct| {
                                line_product_id.set(picked.id.clone());
                                line_product_name.set(picked.name.clone());
                                line_description_error.set(String::new());
                                line_description.set(picked.name.clone());
                                unit_price_error.set(String::new());
                                line_unit_price.set(picked.unit_price.clone());
                                line_is_taxable.set(picked.is_taxable);
                            },
                            onclear: move |_| {
                                line_product_id.set(String::new());
                                line_product_name.set(String::new());
                                line_is_taxable.set(true);
                            },
                        }
                    }
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
                    div { class: "mt-3",
                        crate::components::Checkbox {
                            name: "line_is_taxable",
                            label: "Taxable",
                            checked: *line_is_taxable.read(),
                            // MAPPS-712: a product line follows the product.
                            disabled: !line_product_id.read().is_empty(),
                            help: if line_product_id.read().is_empty() {
                                "The tax rate applies to this line.".to_string()
                            } else {
                                "Follows the product's own setting in the price list.".to_string()
                            },
                            onchange: move |_| {
                                let next = !*line_is_taxable.read();
                                line_is_taxable.set(next);
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
                            help: "Worked out from your tax rate. Type your own amount to override it; an override is saved as you enter it and records no rate.",
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
                    placeholder: "Internal notes (your customer never sees these)",
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
                .inspect_err(|e| tracing::error!("payment list load failed: {e}"))
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
            #[cfg(feature = "app")]
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
    let amount = use_signal(|| props.amount.clone());
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
            #[cfg(feature = "app")]
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
                    onchange: clear_on_edit(invoice_id, invoice_err),
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
                        oninput: clear_on_edit(amount, amount_err),
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
    /// MAPPS-640: the catalog product this line sells, kept across a re-save.
    product_id: Option<String>,
    /// MAPPS-712: whether the rate applies to the line. Read-only on a
    /// product line, where the server copies the product's flag.
    is_taxable: bool,
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
    /// PMS-1004: the invoice's company, to scope the billing-contact picker.
    company_id: String,
    /// Current billing contact FK and its display name, empty when unset.
    billing_contact_id: String,
    billing_contact_name: String,
    invoice_date: String,
    due_date: String,
    /// Current payment-term FK (PMS-333), empty string when unset.
    payment_term_id: String,
    po_number: String,
    notes: String,
    /// Current line items, seeded into the editable line table (MAPPS-234).
    /// The tax-rate calculation derives its subtotal from these (MAPPS-192).
    lines: Vec<InvoiceLine>,
    /// MAPPS-712: the rate the invoice recorded (PMS-1029), empty when the
    /// tax was a given amount or none.
    tax_rate_id: String,
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
    /// MAPPS-662: seeds the create form's select.
    #[serde(default)]
    is_default: bool,
    /// MAPPS-662: what the term means in days (PMS-990), for the derived
    /// due date hint. `None` for a term with no fixed count.
    #[serde(default)]
    net_days: Option<i64>,
}

/// MAPPS-662: the due date the server will derive when the field is left
/// blank: the invoice date plus the term's days, or plus thirty when the
/// term names no count, which is the server's own fallback (PMS-990).
/// `None` until there is an invoice date to add to.
fn derived_due_date(invoice_date: &str, net_days: Option<i64>) -> Option<String> {
    let date = chrono::NaiveDate::parse_from_str(invoice_date.trim(), "%Y-%m-%d").ok()?;
    let days = net_days.unwrap_or(30);
    let due = date.checked_add_signed(chrono::Duration::days(days))?;
    Some(due.format("%Y-%m-%d").to_string())
}

/// MAPPS-158: edit a draft/pending invoice's header fields. Wired to
/// `PUT /invoices/{id}`. MAPPS-234: the modal also renders an editable line-item
/// table and sends `lines`, so line items can be added, removed, and corrected
/// after creation (the server replaces the set transactionally and recomputes
/// the subtotal). The backend rejects the PUT once the invoice is frozen, so
/// this modal is only opened for editable invoices.
#[component]
fn InvoiceEditModal(props: InvoiceEditModalProps) -> Element {
    let mut billing_contact_id = use_signal(|| props.billing_contact_id.clone());
    let mut billing_contact_name = use_signal(|| props.billing_contact_name.clone());
    let mut invoice_date = use_signal(|| props.invoice_date.clone());
    let mut due_date = use_signal(|| props.due_date.clone());
    // MAPPS-662: whether the operator edited the due date in this session.
    // Untouched, it is not sent, so a term change re-derives it server-side
    // (PMS-990) and an unchanged term keeps it as it was.
    let mut due_touched = use_signal(|| false);
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
                product_id: l.product_id.map(|p| p.to_string()),
                is_taxable: l.is_taxable,
                ..EditableLine::default()
            })
            .collect::<Vec<_>>()
    });
    // MAPPS-712: an invoice carrying a rate follows it, and the server
    // re-derives the amount from that rate when the lines change; one carrying
    // a given amount keeps that amount as its override until a rate is picked,
    // because sending nothing would let the tenant's default replace it.
    let mut tax_rate_id = use_signal(|| props.tax_rate_id.clone());
    let mut tax_override = use_signal(|| {
        props
            .tax_rate_id
            .is_empty()
            .then(|| props.tax_amount.clone())
    });
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
    // Only the taxable lines count (MAPPS-712), as on the server.
    let line_subtotal = use_memo(move || {
        lines
            .read()
            .iter()
            .filter(|l| l.is_taxable)
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
        // MAPPS-662: optional; an untouched date is left to the server.
        due_date_err.set(String::new());

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
                // MAPPS-640: the reference survives a re-save; the price is
                // the line's own.
                "product_id": line.product_id.clone(),
                // Ignored by the server for a product line, which copies the
                // product's flag (PMS-1029).
                "is_taxable": line.is_taxable,
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
        // MAPPS-712: the rate, or the override, never both. Neither leaves the
        // recorded rate in place, and the server re-derives the amount from it
        // over the replaced lines (PMS-1029).
        let tax = tax_body_fields(&tax_rate_id.read(), tax_override.read().as_deref());
        let body = serde_json::json!({
            // PMS-1004: null leaves the contact as it is (the server
            // COALESCEs), so clearing the chip changes nothing on save.
            "billing_contact_id": optional_string(&billing_contact_id.read()),
            "invoice_date": inv_date,
            // MAPPS-662: only what the operator typed. Null leaves the date to
            // the server, which keeps it unless the term changed.
            "due_date": if *due_touched.read() { optional_string(&due) } else { serde_json::Value::Null },
            "payment_term_id": optional_string(&payment_term_id.read()),
            "po_number": optional_string(&po_number.read()),
            "notes": optional_string(&notes.read()),
            "tax_rate_id": tax.rate_id,
            "tax_amount": tax.amount,
            "discount_amount": optional_string(&discount_amount.read()),
            // Replace the line set (server deletes + reinserts transactionally
            // and recomputes the subtotal). MAPPS-234.
            "lines": lines_json,
        });
        spawn(async move {
            #[cfg(feature = "app")]
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
                // PMS-1004: the contact the invoice is emailed to (PMS-992).
                // Scoped to the invoice's company; the server refuses a send
                // with none, so this is where "set a billing contact on the
                // invoice" is done.
                crate::components::ContactPicker {
                    value: billing_contact_name.read().clone(),
                    selected_id: {
                        let id = billing_contact_id.read().clone();
                        (!id.is_empty()).then_some(id)
                    },
                    label: "Billing Contact".to_string(),
                    company_filter: (!props.company_id.is_empty()).then(|| props.company_id.clone()),
                    onselect: move |(id, name): (String, String)| {
                        billing_contact_id.set(id);
                        billing_contact_name.set(name);
                    },
                    onclear: move |_| {
                        billing_contact_id.set(String::new());
                        billing_contact_name.set(String::new());
                    },
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
                        help: "Change the payment term and leave this as it is to take the due date from that term.",
                        error: due_date_err(),
                        value: due_date.read().clone(),
                        oninput: move |e: FormEvent| {
                            due_date_err.set(String::new());
                            due_touched.set(true);
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
                                        is_taxable: true,
                                        ..EditableLine::default()
                                    });
                            },
                            "Add line"
                        }
                    }
                    // MAPPS-640: a line from the price list, prefilled with the
                    // product's name and current price and carrying its id.
                    div { class: "mb-3",
                        crate::components::ProductPicker {
                            value: String::new(),
                            selected_id: None,
                            label: "Add from the price list",
                            placeholder: "Search the price list to add a line…",
                            clear_on_select: true,
                            onselect: move |picked: crate::components::PickedProduct| {
                                lines.write().push(EditableLine {
                                    line_type: "product".to_string(),
                                    description: picked.name.clone(),
                                    quantity: "1".to_string(),
                                    unit_price: picked.unit_price.clone(),
                                    product_id: Some(picked.id.clone()),
                                    is_taxable: picked.is_taxable,
                                    ..EditableLine::default()
                                });
                            },
                            onclear: move |_| {},
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
                                class: "grid grid-cols-1 gap-3 sm:grid-cols-[1fr_90px_120px_auto_auto] sm:items-end",
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
                                crate::components::Checkbox {
                                    name: "line_is_taxable_{idx}",
                                    label: "Taxable",
                                    checked: line.is_taxable,
                                    // MAPPS-712: a product line follows the product.
                                    disabled: line.product_id.is_some(),
                                    onchange: move |_| {
                                        let mut w = lines.write();
                                        w[idx].is_taxable = !w[idx].is_taxable;
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
                        help: "Worked out from your tax rate. Type your own amount to override it; an override is saved as you enter it and records no rate.",
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
            .inspect_err(|e| tracing::error!("tax rate list load failed: {e}"))
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
                                        onclick: {
                                            let edit_state = edit_state.clone();
                                            move |_| editing.set(Some(edit_state.clone()))
                                        },
                                        TableCell {
                                            // MAPPS-569: the row's click opens a modal, so there is no
                                            // route to link to; this cell is the keyboard path instead.
                                            onactivate: move |_| editing.set(Some(edit_state.clone())),
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
            #[cfg(feature = "app")]
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
            #[cfg(feature = "app")]
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
    /// MAPPS-671 (mokosh-invoices P2a): admin-set override for the Pay
    /// Now button label. `None` = fall back to the provider default in
    /// `get_invoice_payment_readiness`.
    #[serde(default)]
    client_display_name: Option<String>,
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
            .inspect_err(|e| tracing::error!("payment gateway list load failed: {e}"))
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
                                        onclick: {
                                            let edit_state = edit_state.clone();
                                            move |_| editing.set(Some(edit_state.clone()))
                                        },
                                        TableCell {
                                            // MAPPS-569: the row's click opens a modal, so there is no
                                            // route to link to; this cell is the keyboard path instead.
                                            onactivate: move |_| editing.set(Some(edit_state.clone())),
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

/// MAPPS-759: the providers this app can configure, which is the server's
/// `billing::provider::SUPPORTED` and NOT the wider set the
/// `payment_gateway_configs.provider` CHECK constraint accepts. The column
/// predates any implementation and still allows `authorize_net`, which this
/// form used to offer: the server refuses to activate one, so choosing it was
/// a 400 the admin could do nothing about.
pub(crate) const CONFIGURABLE_PROVIDERS: &[(&str, &str)] =
    &[("stripe", "Stripe"), ("paypal", "PayPal")];

/// One credential a provider needs, named the way that provider names it.
///
/// MAPPS-759: the form used to ask for a single "API key" and send
/// `{"api_key": ...}`, which no provider reads. Both server-side credential
/// structs are `#[serde(default)]`, so that blob deserialised cleanly into
/// empty strings: the save succeeded, the row reported Configured, and the
/// failure only appeared on the customer's invoice when the provider was built
/// with an empty bearer. The fields are written out here because the shapes are
/// the providers' own and the server parses each one into its own struct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CredentialField {
    /// The key inside the `config` object, exactly as the server deserialises it.
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) placeholder: &'static str,
    pub(crate) help: &'static str,
}

/// What `provider` needs in its `config` blob.
///
/// A provider this build cannot configure gets an empty slice, which is what
/// stops the form writing a credential set for one: an existing
/// `authorize_net` row can still be viewed and removed, and saving it leaves
/// whatever is stored alone.
pub(crate) fn credential_fields(provider: &str) -> &'static [CredentialField] {
    match provider {
        // `StripeCredentials` in mokosh-server's provider/stripe.rs.
        "stripe" => &[
            CredentialField {
                key: "secret_key",
                label: "Secret key",
                placeholder: "sk_test_… or rk_live_…",
                help: "A restricted key is enough. Stripe shows it once, when you create it.",
            },
            CredentialField {
                key: "webhook_secret",
                label: "Webhook signing secret",
                placeholder: "whsec_…",
                help: "From the Stripe webhook endpoint you point at this tenant. Without it a payment is taken and never recorded.",
            },
        ],
        // `PaypalCredentials` in mokosh-server's provider/paypal.rs. `sandbox`
        // is in that struct too and is deliberately not a field here: it
        // follows the Test mode switch above, because two controls for one
        // question is how they come to disagree.
        "paypal" => &[
            CredentialField {
                key: "client_id",
                label: "Client ID",
                placeholder: "The REST app's client ID",
                help: "From the PayPal app under your developer account.",
            },
            CredentialField {
                key: "client_secret",
                label: "Client secret",
                placeholder: "The REST app's secret",
                help: "Shown once when the app's secret is generated.",
            },
            CredentialField {
                key: "webhook_id",
                label: "Webhook ID",
                placeholder: "The webhook's ID, not its URL",
                help: "PayPal verifies a delivery against this ID by calling back, so a wrong one refuses every webhook.",
            },
        ],
        _ => &[],
    }
}

/// The `config` object to send, or `None` to leave the stored credentials alone.
///
/// All-or-nothing, and that is forced by how the server stores them rather than
/// chosen here: `upsert_payment_gateway` serialises this whole object and writes
/// it to the secret provider as ONE value, so a save REPLACES the credential
/// set. Sending one field would wipe the others. All blank therefore means
/// "keep what is stored" (the MAPPS-363 omit-to-keep rule), and anything typed
/// means every field is required, with the ones left empty named in the `Err`
/// so each gets its own message.
pub(crate) fn gateway_config_body(
    provider: &str,
    values: &std::collections::HashMap<String, String>,
    is_test_mode: bool,
) -> Result<Option<serde_json::Value>, Vec<&'static str>> {
    let fields = credential_fields(provider);
    if fields.is_empty() {
        return Ok(None);
    }
    let value_of = |field: &CredentialField| {
        values
            .get(field.key)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    if fields.iter().all(|f| value_of(f).is_empty()) {
        return Ok(None);
    }
    let missing: Vec<&'static str> = fields
        .iter()
        .filter(|f| value_of(f).is_empty())
        .map(|f| f.key)
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let mut config = serde_json::Map::new();
    for field in fields {
        config.insert(field.key.to_string(), serde_json::json!(value_of(field)));
    }
    if provider == "paypal" {
        // The one derived value: PayPal's credential blob carries which API
        // base to talk to, and the admin already answered that above.
        config.insert("sandbox".to_string(), serde_json::json!(is_test_mode));
    }
    Ok(Some(serde_json::Value::Object(config)))
}

/// MAPPS-760: the events a provider has to be subscribed to for this app to
/// hear about a payment.
///
/// Half the setup answer, and not a nice-to-have. An admin who pastes the
/// endpoint URL correctly and subscribes only to the completion event has
/// refunds silently never reach the invoice, which is the same quiet class of
/// failure MAPPS-759 closed: everything looks configured and a number is
/// wrong.
///
/// The sets are the ones mokosh-server's providers act on
/// (`provider/stripe.rs` and `provider/paypal.rs`); anything else is ignored
/// there, so subscribing to more is noise rather than harm.
pub(crate) fn webhook_events(provider: &str) -> &'static [&'static str] {
    match provider {
        "stripe" => &["checkout.session.completed", "charge.refunded"],
        "paypal" => &[
            "CHECKOUT.ORDER.APPROVED",
            "PAYMENT.CAPTURE.COMPLETED",
            "PAYMENT.CAPTURE.REFUNDED",
        ],
        _ => &[],
    }
}

/// MAPPS-765: the provider-side half of setting a gateway up.
///
/// MAPPS-760 put the endpoint URL and the event list on this form, which
/// removed the blocker. It was still not enough to finish unaided: *"without
/// your help i dont know how to navigate through the set destination"*. The
/// sentence it shipped - "Add this endpoint in Stripe, subscribe it to the
/// events below" - asserted an order Stripe's UI does not have (it asks for
/// the events first and the URL last), and named a control that does not
/// exist there ("Add destination" is the button).
///
/// ## Everything in this table names something we do not control
///
/// That is the point of keeping it in ONE place. A provider can rename a
/// screen or redesign a flow whenever it likes - this one did, between
/// MAPPS-760 being written and an admin using it - and instructions that go
/// stale silently are worse than none, because a confident wrong instruction
/// costs more than an absent one. So the split is deliberate:
///
/// - What WE know and control - the endpoint URL, the exact events, which
///   credential goes in which field, that the payload must carry the whole
///   object - is stated on the form, because it is about us and does not go
///   stale.
/// - The provider's own click path is a LINK to their documentation, which
///   they keep current. We give only enough orientation to find the screen.
///
/// A reviewer checking whether this has gone stale reads this table and
/// nothing else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProviderSetup {
    /// Where the setting lives, named loosely enough to survive a redesign
    /// and precisely enough to find. Provider-controlled.
    pub(crate) where_to_look: &'static str,
    /// The provider's own setup documentation. Provider-controlled.
    pub(crate) doc_url: &'static str,
    /// A trap specific to this provider whose failure is SILENT - the setup
    /// looks complete and money goes missing from the record. Empty when the
    /// provider has none.
    pub(crate) silent_trap: &'static str,
}

/// What we can tell an admin about `provider` beyond the URL and the events.
pub(crate) fn provider_setup(provider: &str) -> Option<ProviderSetup> {
    match provider {
        "stripe" => Some(ProviderSetup {
            where_to_look: "In Stripe, open Workbench and the Webhooks tab, then create a destination.",
            doc_url: "https://docs.stripe.com/webhooks",
            // Stripe now offers thin destinations, which deliver an id
            // instead of the object. Our handler reads the object, so a thin
            // destination verifies, answers 200 and records nothing: the
            // customer pays, Stripe reports success, the invoice stays unpaid.
            silent_trap: "Choose the destination that sends the full event data (Stripe calls these snapshot events). A destination that sends only an event ID will be accepted and recorded as delivered, and the payment will never reach the invoice.",
        }),
        "paypal" => Some(ProviderSetup {
            where_to_look: "In the PayPal Developer dashboard, open your app and add a webhook.",
            doc_url: "https://developer.paypal.com/api/rest/webhooks/",
            // The third credential field wants the webhook's id, and the page
            // that creates the webhook shows the URL far more prominently.
            silent_trap: "The Webhook ID field above wants the ID PayPal shows beside the webhook after you save it, not the URL you just pasted in. PayPal verifies every delivery against that ID, so a wrong one refuses all of them.",
        }),
        _ => None,
    }
}

/// One row of `GET /payment-gateways/webhook-endpoints` (PMS-1165).
///
/// `url` is `None` when the deployment sets no `PUBLIC_API_BASE_URL`, which is
/// an operator fix and not something the admin can do in this form, so the two
/// cases are rendered differently.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct RemoteWebhookEndpoint {
    #[serde(default)]
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) url: Option<String>,
}

/// What the form knows about where `provider`'s webhooks should be delivered.
///
/// Three states, each with its own thing to say, because a blank would leave
/// an admin holding a request for a webhook signing secret with no way to act
/// on it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WebhookEndpoint {
    /// The deployment answered with a URL.
    Known(String),
    /// The deployment has no public API base configured.
    NoPublicBase,
    /// This provider has no receiver, or the server predates PMS-1165.
    Unknown,
}

/// Read the endpoint for `provider` out of what the server answered.
///
/// A server that predates PMS-1165 404s the fetch, which reaches here as
/// `None` and reads as [`WebhookEndpoint::Unknown`]: the rest of the form
/// still works, because a missing hint must not take the credential fields
/// down with it.
pub(crate) fn endpoint_for(
    rows: Option<&[RemoteWebhookEndpoint]>,
    provider: &str,
) -> WebhookEndpoint {
    let Some(rows) = rows else {
        return WebhookEndpoint::Unknown;
    };
    match rows.iter().find(|r| r.provider == provider) {
        Some(row) => match row.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            Some(url) => WebhookEndpoint::Known(url.to_string()),
            None => WebhookEndpoint::NoPublicBase,
        },
        None => WebhookEndpoint::Unknown,
    }
}

/// The provider choices to render, given the row being edited.
///
/// The configurable set, plus the row's own provider when this build cannot
/// configure it: the select is disabled on an existing row, and an option that
/// is not in the list renders as an empty control rather than as the name of
/// the gateway the admin came to remove.
pub(crate) fn provider_choices(current: &str) -> Vec<(String, String)> {
    let mut choices: Vec<(String, String)> = CONFIGURABLE_PROVIDERS
        .iter()
        .map(|(id, name)| ((*id).to_string(), (*name).to_string()))
        .collect();
    if !current.is_empty() && !CONFIGURABLE_PROVIDERS.iter().any(|(id, _)| *id == current) {
        choices.push((current.to_string(), humanize_provider(current)));
    }
    choices
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
    /// MAPPS-671 (mokosh-invoices P2a): admin-set override for the Pay
    /// Now button label. Empty string = clear the override (falls back
    /// to the provider default).
    client_display_name: String,
}

impl GatewayFormState {
    fn new() -> Self {
        Self {
            existing: false,
            provider: "stripe".to_string(),
            is_active: false,
            is_test_mode: true,
            configured: false,
            client_display_name: String::new(),
        }
    }

    fn from_existing(g: &RemoteGateway) -> Self {
        Self {
            existing: true,
            provider: g.provider.clone(),
            is_active: g.is_active,
            is_test_mode: g.is_test_mode,
            configured: g.configured,
            client_display_name: g.client_display_name.clone().unwrap_or_default(),
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
    // MAPPS-363 / MAPPS-759: the provider's credentials, keyed by the name the
    // server deserialises. Write-only: they always start blank (the server
    // never returns a stored secret), and leaving every one blank keeps what is
    // stored. Keyed rather than positional so switching provider on a new
    // gateway cannot carry a Stripe key into a PayPal field.
    let mut creds: Signal<std::collections::HashMap<String, String>> =
        use_signal(std::collections::HashMap::new);
    let mut cred_errs: Signal<std::collections::HashMap<String, String>> =
        use_signal(std::collections::HashMap::new);
    // MAPPS-671 (mokosh-invoices P2a): the admin's Pay Now button label.
    // Seeded from the existing row so an edit keeps whatever was set;
    // blank = clear the override on save (server treats empty-string as
    // clear-to-provider-default).
    let mut client_display_name = use_signal(|| initial.client_display_name.clone());
    let mut client_display_name_err = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block save / remove while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    // MAPPS-760 / PMS-1165: where this deployment receives webhooks, answered
    // per provider and independently of whether a gateway is configured. It
    // has to be on screen BEFORE the first save: creating the endpoint in the
    // provider's dashboard is what produces the signing secret this form then
    // demands. A server that predates the endpoint 404s, which reads as
    // `Unknown` and leaves the rest of the form alone.
    let endpoints = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Vec<RemoteWebhookEndpoint>>(
            "/payment-gateways/webhook-endpoints",
        )
        .await
        .inspect_err(|e| tracing::warn!("webhook endpoint lookup failed: {e}"))
        .ok()
    });

    // MAPPS-759: the set the server can actually serve, plus this row's own
    // provider when it is one this build cannot configure.
    let provider_options: Vec<SelectOption> = provider_choices(&provider.read())
        .into_iter()
        .map(|(id, name)| SelectOption::new(id, name))
        .collect();

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());
        cred_errs.set(std::collections::HashMap::new());
        client_display_name_err.set(String::new());

        // MAPPS-759: the credential set, or nothing when every field was left
        // blank (PMS-342 omit-to-keep). A partial fill is refused here rather
        // than sent, because the server replaces the whole stored set.
        let selected = provider.read().clone();
        let config = match gateway_config_body(&selected, &creds.read(), *is_test_mode.read()) {
            Ok(value) => value,
            Err(missing) => {
                let mut errs = std::collections::HashMap::new();
                for key in missing {
                    errs.insert(
                        key.to_string(),
                        "Required. Saving replaces the whole credential set, so every field has to be filled in.".to_string(),
                    );
                }
                cred_errs.set(errs);
                return;
            }
        };
        // A first-time gateway must supply one - the server rejects a create
        // with no `config` (400), so this is a field-level message instead.
        if config.is_none() && !configured {
            let fields = credential_fields(&selected);
            if fields.is_empty() {
                error.set(format!(
                    "{} cannot be configured from this app.",
                    humanize_provider(&selected)
                ));
                return;
            }
            let mut errs = std::collections::HashMap::new();
            for field in fields {
                errs.insert(
                    field.key.to_string(),
                    "Required to configure this gateway.".to_string(),
                );
            }
            cred_errs.set(errs);
            return;
        }
        // MAPPS-671: 64-char cap mirrors the server's validator; catching it
        // here gives a field-level error rather than a form-level 422.
        let cdn = client_display_name.read().clone();
        if cdn.chars().count() > 64 {
            client_display_name_err.set("Button label must be 64 characters or fewer.".to_string());
            return;
        }
        saving.set(true);
        let mut body = serde_json::json!({
            "provider": selected.clone(),
            "is_active": *is_active.read(),
            "is_test_mode": *is_test_mode.read(),
            // MAPPS-671: always send the current value. A trimmed empty
            // string clears the override on the server; a non-empty
            // value sets it. Omitting the field would preserve whatever
            // was previously set, which contradicts the visible form
            // state (the input is empty).
            "client_display_name": cdn.trim(),
        });
        if let Some(config) = config {
            body["config"] = config;
        }
        spawn(async move {
            #[cfg(feature = "app")]
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
            #[cfg(feature = "app")]
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
                // MAPPS-759: the credentials the SELECTED provider needs,
                // named the way that provider names them. One "API key" field
                // fitted neither: the server parses a per-provider blob, and
                // the one this form used to send read as empty strings.
                div { class: "space-y-3",
                    div { class: "flex items-center gap-2",
                        span { class: "block text-sm font-medium text-content", "Credentials" }
                        if configured {
                            Badge { variant: BadgeVariant::Green, "Configured" }
                        } else {
                            Badge { variant: BadgeVariant::Gray, "Not configured" }
                        }
                    }
                    {
                        let selected = provider.read().clone();
                        let fields = credential_fields(&selected);
                        if fields.is_empty() {
                            rsx! {
                                p { class: "text-sm text-muted",
                                    "{humanize_provider(&selected)} cannot be configured from this app. You can remove it here."
                                }
                            }
                        } else {
                            rsx! {
                                for field in fields.iter().copied() {
                                    {
                                        let key = field.key;
                                        rsx! {
                                            crate::components::Input {
                                                key: "{key}",
                                                name: "gateway_cred_{key}",
                                                label: field.label,
                                                r#type: "password",
                                                placeholder: field.placeholder.to_string(),
                                                required: !configured,
                                                help: field.help.to_string(),
                                                error: cred_errs.read().get(key).cloned().unwrap_or_default(),
                                                value: creds.read().get(key).cloned().unwrap_or_default(),
                                                oninput: move |e: FormEvent| {
                                                    cred_errs.write().remove(key);
                                                    creds.write().insert(key.to_string(), e.value());
                                                },
                                            }
                                        }
                                    }
                                }
                                // Said once, under the set, because it is the
                                // rule for the set and not for any one field.
                                p { class: "text-xs text-muted",
                                    if configured {
                                        "Stored encrypted. You will not see these again after you save. Leave them all blank to keep what is stored; filling any one replaces the whole set, so enter all of them."
                                    } else {
                                        "Stored encrypted. You will not see these again after you save."
                                    }
                                }
                            }
                        }
                    }
                }
                // MAPPS-760: the other half of the webhook signing secret
                // above. The endpoint carries this tenant's id, which is not
                // rendered anywhere else in this app, so without this an admin
                // was asked for a secret belonging to a URL they had no
                // supported way to learn.
                {
                    let selected = provider.read().clone();
                    let events = webhook_events(&selected);
                    let snap = endpoints.read_unchecked().clone();
                    let known = endpoint_for(
                        snap.as_ref().and_then(|r| r.as_deref()),
                        &selected,
                    );
                    if events.is_empty() {
                        rsx! {}
                    } else {
                        rsx! {
                            div { class: "space-y-2 rounded-md border border-line bg-surface p-4",
                                span { class: "block text-sm font-medium text-content", "Webhook endpoint" }
                                match known {
                                    WebhookEndpoint::Known(url) => rsx! {
                                        div { class: "flex items-start gap-2",
                                            code {
                                                class: "flex-1 min-w-0 break-all text-xs text-content",
                                                "{url}"
                                            }
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                size: ButtonSize::Small,
                                                onclick: move |_| {
                                                    let u = url.clone();
                                                    #[cfg(target_arch = "wasm32")]
                                                    if let Some(win) = web_sys::window() {
                                                        let _ = win.navigator().clipboard().write_text(&u);
                                                        crate::hooks::toast::push_toast(
                                                            crate::components::AlertType::Success,
                                                            "Webhook endpoint copied to clipboard.".to_string(),
                                                        );
                                                    }
                                                    #[cfg(not(target_arch = "wasm32"))]
                                                    let _ = u;
                                                },
                                                "Copy"
                                            }
                                        }
                                        // MAPPS-765: no order is asserted
                                        // here. The provider decides whether
                                        // it asks for the URL or the events
                                        // first, and Stripe asks for the
                                        // events first, which the sentence
                                        // this replaces got backwards.
                                        p { class: "text-xs text-muted",
                                            "Create a webhook in {humanize_provider(&selected)} for this URL, subscribed to the events below. It will give you a signing secret: paste that into the field above."
                                        }
                                    },
                                    WebhookEndpoint::NoPublicBase => rsx! {
                                        p { class: "text-xs text-muted",
                                            "This deployment has no public API address set, so the endpoint cannot be shown. Whoever runs the server sets PUBLIC_API_BASE_URL."
                                        }
                                    },
                                    WebhookEndpoint::Unknown => rsx! {
                                        p { class: "text-xs text-muted",
                                            "The endpoint for this provider could not be read from the server."
                                        }
                                    },
                                }
                                div {
                                    span { class: "block text-xs text-muted", "Subscribe it to these events:" }
                                    ul { class: "mt-1 space-y-0.5",
                                        for event in events.iter().copied() {
                                            li { key: "{event}",
                                                code { class: "text-xs text-content", "{event}" }
                                            }
                                        }
                                    }
                                    // Said because the cost of missing one is
                                    // invisible: the payment still records and
                                    // the refund never does.
                                    p { class: "mt-1 text-xs text-muted",
                                        "All of them. Without the refund event a refund never reaches the invoice."
                                    }
                                }
                                // MAPPS-765: where to find the screen, the
                                // provider's own documentation, and the one
                                // mistake whose failure is silent. Everything
                                // in this block names something the provider
                                // controls, so it lives in `provider_setup`
                                // where its staleness is findable.
                                if let Some(setup) = provider_setup(&selected) {
                                    div { class: "space-y-1 border-t border-line pt-2",
                                        p { class: "text-xs text-muted", "{setup.where_to_look}" }
                                        if !setup.silent_trap.is_empty() {
                                            p { class: "text-xs text-muted", "{setup.silent_trap}" }
                                        }
                                        a {
                                            href: "{setup.doc_url}",
                                            target: "_blank",
                                            rel: "noopener noreferrer",
                                            class: "text-xs text-accent hover:underline",
                                            "{humanize_provider(&selected)}'s setup guide"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // MAPPS-671 (mokosh-invoices P2a): admin-set override for
                // the Pay Now button label the portal contact sees on
                // their invoice.
                crate::components::Input {
                    name: "gateway_client_display_name",
                    label: "Button label (optional)".to_string(),
                    placeholder: "Pay with card".to_string(),
                    help: "What your customer sees on the Pay Now button. Leave blank for the provider's default.",
                    maxlength: 64_i64,
                    error: client_display_name_err(),
                    value: client_display_name.read().clone(),
                    oninput: move |e: FormEvent| {
                        client_display_name_err.set(String::new());
                        client_display_name.set(e.value());
                    },
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

/// MAPPS-759: the credential set the form sends has to be the one the server
/// parses, and the providers it offers have to be the ones the server can
/// serve. Both drifted, and both drifted silently.
/// MAPPS-765: the provider-side setup guidance.
#[cfg(test)]
mod provider_setup_tests {
    use super::{provider_setup, webhook_events, CONFIGURABLE_PROVIDERS};

    /// Every provider this app can configure has setup guidance, or an admin
    /// meets the provider's dashboard with nothing but a URL - which is where
    /// this started.
    #[test]
    fn every_configurable_provider_has_guidance() {
        for (id, _) in CONFIGURABLE_PROVIDERS {
            let setup = provider_setup(id).unwrap_or_else(|| panic!("{id} has no setup guidance"));
            assert!(!setup.where_to_look.is_empty(), "{id}");
            assert!(setup.doc_url.starts_with("https://"), "{id}");
        }
        assert!(provider_setup("authorize_net").is_none());
    }

    /// The guidance names no button.
    ///
    /// A provider renames its controls whenever it likes - Stripe did, between
    /// the previous copy being written and an admin using it, which is how
    /// that copy came to name a control that does not exist. Orientation to a
    /// screen survives a redesign; a quoted button label does not, and a
    /// confident wrong instruction costs more than an absent one.
    #[test]
    fn the_guidance_does_not_quote_a_button_label() {
        for (id, _) in CONFIGURABLE_PROVIDERS {
            let setup = provider_setup(id).expect("guidance");
            let lowered = setup.where_to_look.to_lowercase();
            for quoted in [
                "click ",
                "press ",
                "\"add",
                "button labelled",
                "button labeled",
            ] {
                assert!(!lowered.contains(quoted), "{id}: {}", setup.where_to_look);
            }
        }
    }

    /// Each provider's silent trap is stated, because that is the mistake an
    /// admin cannot detect: the setup looks complete and money goes missing
    /// from the record.
    #[test]
    fn each_provider_states_its_silent_trap() {
        let stripe = provider_setup("stripe").expect("stripe");
        assert!(
            stripe.silent_trap.to_lowercase().contains("snapshot"),
            "a thin destination records nothing: {}",
            stripe.silent_trap
        );
        let paypal = provider_setup("paypal").expect("paypal");
        let lowered = paypal.silent_trap.to_lowercase();
        assert!(lowered.contains("id"), "{}", paypal.silent_trap);
        assert!(lowered.contains("not the url"), "{}", paypal.silent_trap);
    }

    /// The guidance is the provider-side half of what the events list is the
    /// other half of, so the two have to cover the same providers.
    #[test]
    fn guidance_and_events_cover_the_same_providers() {
        for (id, _) in CONFIGURABLE_PROVIDERS {
            assert_eq!(
                provider_setup(id).is_some(),
                !webhook_events(id).is_empty(),
                "{id} has one half of the setup instructions and not the other"
            );
        }
    }
}

/// MAPPS-762: where a payment provider returns the customer.
#[cfg(test)]
mod checkout_return_tests {
    use crate::Route;

    /// The path comes from the router, so it is whatever the app serves.
    ///
    /// It used to be typed as `/portal/invoices/{id}`, which was retired with
    /// the customer-portal route family: the customer paid, Stripe returned
    /// them, and they landed on the 404 page with a charged card and no
    /// confirmation. Deriving it means a rename moves the return URL too.
    #[test]
    fn the_return_path_is_the_invoice_route_this_app_serves() {
        let id = "2f1c2f1e-0000-4000-8000-00000000abcd";
        let path = Route::InvoiceDetail { id: id.to_string() }.to_string();
        assert_eq!(path, format!("/invoices/{id}"));
        assert!(
            !path.starts_with("/portal/invoices/"),
            "the retired route must never be a return target again: {path}"
        );
    }

    /// `?paid=1` is what the invoice page reads to show the confirmation
    /// splash, so the success URL has to carry it and the cancel URL must not
    /// (a customer who backed out has paid nothing).
    #[test]
    fn success_carries_the_paid_marker_and_cancel_does_not() {
        let path = Route::InvoiceDetail {
            id: "2f1c2f1e-0000-4000-8000-00000000abcd".to_string(),
        }
        .to_string();
        let success = format!("https://msp.example{path}?paid=1");
        let cancel = format!("https://msp.example{path}");
        assert!(success.ends_with("?paid=1"));
        assert!(!cancel.contains("paid="));
        assert!(success.starts_with(&cancel));
    }
}

#[cfg(test)]
mod gateway_credential_tests {
    use super::{
        credential_fields, endpoint_for, gateway_config_body, provider_choices, webhook_events,
        RemoteWebhookEndpoint, WebhookEndpoint, CONFIGURABLE_PROVIDERS,
    };
    use std::collections::HashMap;

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// The keys are the server's, not this page's. `mokosh-server`'s
    /// `StripeCredentials` and `PaypalCredentials` are both
    /// `#[serde(default)]`, so a key it does not know is not an error there -
    /// it reads as an empty string, the row reports Configured, and the
    /// failure waits until a customer presses Pay Now. That is exactly what
    /// `{"api_key": ...}` did, so the names are asserted in full.
    #[test]
    fn the_credential_keys_are_the_ones_the_server_deserialises() {
        let stripe: Vec<&str> = credential_fields("stripe").iter().map(|f| f.key).collect();
        assert_eq!(stripe, vec!["secret_key", "webhook_secret"]);
        let paypal: Vec<&str> = credential_fields("paypal").iter().map(|f| f.key).collect();
        assert_eq!(paypal, vec!["client_id", "client_secret", "webhook_id"]);
        for provider in ["stripe", "paypal"] {
            assert!(
                !credential_fields(provider)
                    .iter()
                    .any(|f| f.key == "api_key"),
                "api_key is the key nothing reads"
            );
        }
    }

    /// A provider this build cannot configure offers no fields, so the form
    /// cannot write a credential set for one. An existing row is still
    /// viewable and removable, which is why this is an empty slice rather than
    /// a panic.
    #[test]
    fn an_unconfigurable_provider_offers_no_fields() {
        assert!(credential_fields("authorize_net").is_empty());
        assert!(credential_fields("").is_empty());
        assert_eq!(
            gateway_config_body("authorize_net", &values(&[("secret_key", "x")]), true),
            Ok(None),
            "and nothing typed can be written for it"
        );
    }

    /// The whole object replaces the stored set, so a fill is all or nothing.
    #[test]
    fn a_partial_fill_names_every_field_left_empty() {
        let partial = values(&[("secret_key", "sk_test_1")]);
        assert_eq!(
            gateway_config_body("stripe", &partial, true),
            Err(vec!["webhook_secret"])
        );
        let partial = values(&[("client_id", "id"), ("webhook_id", "  ")]);
        assert_eq!(
            gateway_config_body("paypal", &partial, true),
            Err(vec!["client_secret", "webhook_id"]),
            "whitespace is not a value"
        );
    }

    /// Every field blank is the MAPPS-363 omit-to-keep case, which is what
    /// lets an admin change Test mode or the button label without retyping a
    /// secret the server never gave back.
    #[test]
    fn all_blank_keeps_what_is_stored() {
        assert_eq!(gateway_config_body("stripe", &values(&[]), true), Ok(None));
        assert_eq!(
            gateway_config_body(
                "stripe",
                &values(&[("secret_key", "   "), ("webhook_secret", "")]),
                true
            ),
            Ok(None)
        );
    }

    /// A complete set is sent trimmed and under the server's own key names,
    /// and PayPal's `sandbox` comes from the Test mode switch rather than from
    /// a second control that could contradict it.
    #[test]
    fn a_complete_set_is_sent_under_the_server_key_names() {
        let stripe = gateway_config_body(
            "stripe",
            &values(&[
                ("secret_key", "  sk_test_1  "),
                ("webhook_secret", "whsec_1"),
            ]),
            true,
        )
        .expect("complete")
        .expect("a config");
        assert_eq!(
            stripe,
            serde_json::json!({"secret_key": "sk_test_1", "webhook_secret": "whsec_1"}),
            "Stripe's blob carries no sandbox flag; the key prefix says which mode it is"
        );

        let paypal_values = values(&[
            ("client_id", "id_1"),
            ("client_secret", "secret_1"),
            ("webhook_id", "wh_1"),
        ]);
        for test_mode in [true, false] {
            let paypal = gateway_config_body("paypal", &paypal_values, test_mode)
                .expect("complete")
                .expect("a config");
            assert_eq!(
                paypal,
                serde_json::json!({
                    "client_id": "id_1",
                    "client_secret": "secret_1",
                    "webhook_id": "wh_1",
                    "sandbox": test_mode,
                })
            );
        }
    }

    /// The picker offers what the server's `provider::SUPPORTED` can serve.
    /// `authorize_net` is in the column's CHECK constraint and in nothing
    /// else, so offering it was a 400 the admin could not act on.
    #[test]
    fn the_picker_offers_only_what_the_server_can_serve() {
        let offered: Vec<&str> = CONFIGURABLE_PROVIDERS.iter().map(|(id, _)| *id).collect();
        assert_eq!(offered, vec!["stripe", "paypal"]);
        let ids: Vec<String> = provider_choices("").into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec!["stripe", "paypal"]);
    }

    /// MAPPS-760: the event sets are the ones the server's providers act on.
    /// Named in full, because the cost of a missing one is invisible: with
    /// only the completion event subscribed, payments record and refunds
    /// silently never reach the invoice.
    #[test]
    fn the_event_lists_are_the_ones_the_server_acts_on() {
        assert_eq!(
            webhook_events("stripe"),
            ["checkout.session.completed", "charge.refunded"]
        );
        assert_eq!(
            webhook_events("paypal"),
            [
                "CHECKOUT.ORDER.APPROVED",
                "PAYMENT.CAPTURE.COMPLETED",
                "PAYMENT.CAPTURE.REFUNDED",
            ]
        );
        for provider in ["stripe", "paypal"] {
            assert!(
                webhook_events(provider)
                    .iter()
                    .any(|e| e.to_lowercase().contains("refund")),
                "{provider} must tell the admin to subscribe to refunds"
            );
        }
        assert!(webhook_events("authorize_net").is_empty());
    }

    /// The three states the form renders differently. A blank would leave the
    /// admin holding a request for a webhook signing secret with nothing to
    /// act on, which is the whole defect.
    #[test]
    fn an_absent_url_is_told_apart_from_an_absent_answer() {
        let rows = vec![
            RemoteWebhookEndpoint {
                provider: "stripe".to_string(),
                url: Some("https://api.example.com/api/v1/stripe/webhooks/x".to_string()),
            },
            RemoteWebhookEndpoint {
                provider: "paypal".to_string(),
                url: None,
            },
        ];
        assert_eq!(
            endpoint_for(Some(&rows), "stripe"),
            WebhookEndpoint::Known("https://api.example.com/api/v1/stripe/webhooks/x".to_string())
        );
        // The server answered, and said it has no public base: an operator
        // fix, not something this form can offer.
        assert_eq!(
            endpoint_for(Some(&rows), "paypal"),
            WebhookEndpoint::NoPublicBase
        );
        // A provider the server did not list at all.
        assert_eq!(
            endpoint_for(Some(&rows), "authorize_net"),
            WebhookEndpoint::Unknown
        );
        // And a server that predates the endpoint: the fetch failed, which
        // must not read as "no public base" and must not take the rest of the
        // form down with it.
        assert_eq!(endpoint_for(None, "stripe"), WebhookEndpoint::Unknown);
    }

    /// A blank string is not a URL. A forwarded-but-unset variable arrives as
    /// `""` (PMS-836), so the server could answer one.
    #[test]
    fn a_blank_url_reads_as_no_public_base() {
        let rows = vec![RemoteWebhookEndpoint {
            provider: "stripe".to_string(),
            url: Some("   ".to_string()),
        }];
        assert_eq!(
            endpoint_for(Some(&rows), "stripe"),
            WebhookEndpoint::NoPublicBase
        );
    }

    /// A row already storing a provider this build cannot configure still
    /// renders its name: the select is disabled on an existing row, and an
    /// option that is not in the list would paint an empty control on the
    /// gateway the admin came to remove.
    #[test]
    fn an_existing_unconfigurable_row_keeps_its_name_in_the_picker() {
        let choices = provider_choices("authorize_net");
        assert_eq!(
            choices
                .last()
                .map(|(id, name)| (id.as_str(), name.as_str())),
            Some(("authorize_net", "Authorize.Net"))
        );
        assert_eq!(choices.len(), CONFIGURABLE_PROVIDERS.len() + 1);
        // And a configurable one is never listed twice.
        assert_eq!(
            provider_choices("stripe").len(),
            CONFIGURABLE_PROVIDERS.len()
        );
    }
}

#[cfg(test)]
mod overdue_tests {
    use super::{overdue_label, overdue_query};

    /// The count is the server's; the label only pluralises it.
    #[test]
    fn the_badge_names_the_day_count() {
        assert_eq!(overdue_label(1), "Overdue (1 day)");
        assert_eq!(overdue_label(14), "Overdue (14 days)");
    }

    /// Three states, two of which reach the wire as PMS-1037's `overdue`.
    #[test]
    fn the_filter_maps_to_the_query_or_nothing() {
        assert_eq!(overdue_query(""), None);
        assert_eq!(overdue_query("true"), Some("&overdue=true"));
        assert_eq!(overdue_query("false"), Some("&overdue=false"));
        assert_eq!(overdue_query("maybe"), None);
    }
}

#[cfg(test)]
mod write_off_tests {
    use super::{can_write_off, write_off_line};

    /// PMS-1036 accepts a sent or partially paid invoice and refuses the
    /// rest with a 409, so the button follows the same rule.
    #[test]
    fn only_a_sent_or_partially_paid_invoice_offers_the_action() {
        assert!(can_write_off("sent"));
        assert!(can_write_off("partially_paid"));
        for status in ["draft", "pending", "paid", "void", "written_off", ""] {
            assert!(!can_write_off(status), "{status}");
        }
    }

    /// The row names the amount, the date part of the timestamp, and the
    /// actor when the server sent one; an older server without the name
    /// reads as amount and date alone.
    #[test]
    fn the_row_says_how_much_when_and_who() {
        assert_eq!(
            write_off_line("$120.00", Some("2026-09-02T14:03:11Z"), Some("Ada Admin")),
            "$120.00 on 2026-09-02 by Ada Admin"
        );
        assert_eq!(
            write_off_line("$120.00", Some("2026-09-02T14:03:11Z"), None),
            "$120.00 on 2026-09-02"
        );
        assert_eq!(write_off_line("$120.00", None, Some("  ")), "$120.00");
    }
}

/// MAPPS-735: the Payments card.
#[cfg(test)]
mod payments_card_tests {
    use super::{
        is_zero_amount, payment_line, payment_method_label, refund_line, RemoteInvoiceLedger,
    };

    #[test]
    fn the_method_label_spaces_and_capitalises_the_token() {
        assert_eq!(payment_method_label("credit_card"), "Credit card");
        assert_eq!(payment_method_label("bank_transfer"), "Bank transfer");
        assert_eq!(payment_method_label("check"), "Check");
        assert_eq!(payment_method_label("  "), "Payment");
    }

    #[test]
    fn the_payment_row_names_method_date_and_reference() {
        assert_eq!(
            payment_line("2026-09-02", "credit_card", Some("ch_123")),
            "Credit card on 2026-09-02 (ref ch_123)"
        );
        assert_eq!(
            payment_line("2026-09-02", "cash", Some("  ")),
            "Cash on 2026-09-02"
        );
        assert_eq!(payment_line("", "cash", None), "Cash");
    }

    #[test]
    fn the_refund_row_takes_the_date_part_of_the_timestamp() {
        assert_eq!(
            refund_line("2026-09-03T10:15:00Z"),
            "Refunded on 2026-09-03"
        );
        assert_eq!(refund_line(""), "Refunded");
    }

    #[test]
    fn zero_at_any_scale_is_nothing_refunded() {
        for raw in ["", "0", "0.00", "0.0000", " 0 "] {
            assert!(is_zero_amount(raw), "{raw:?}");
        }
        for raw in ["0.01", "20", "20.00"] {
            assert!(!is_zero_amount(raw), "{raw:?}");
        }
    }

    /// The route body decodes, newest first as the server sends it, with
    /// the optional reference absent, and an empty ledger decodes too.
    #[test]
    fn decodes_the_route_body() {
        let body = r#"{"invoice_id":"aaaaaaaa-0000-4000-8000-000000000001","currency":"USD","payments":[{"id":"aaaaaaaa-0000-4000-8000-000000000002","payment_date":"2026-09-02","amount":"120.00","payment_method":"credit_card","created_at":"2026-09-02T14:00:00Z"}],"refunds":[{"id":"aaaaaaaa-0000-4000-8000-000000000003","payment_id":"aaaaaaaa-0000-4000-8000-000000000002","amount":"20.00","created_at":"2026-09-03T10:15:00Z"}],"total_paid":"120.00","total_refunded":"20.00"}"#;
        let ledger: RemoteInvoiceLedger = serde_json::from_str(body).expect("decode");
        assert_eq!(ledger.payments.len(), 1);
        assert!(ledger.payments[0].reference_number.is_none());
        assert_eq!(ledger.refunds[0].payment_id, ledger.payments[0].id);
        assert_eq!(ledger.total_refunded, "20.00");

        let empty: RemoteInvoiceLedger = serde_json::from_str(
            r#"{"payments":[],"refunds":[],"total_paid":"0","total_refunded":"0"}"#,
        )
        .expect("decode empty");
        assert!(empty.payments.is_empty());
    }

    /// The ledger is fetched on whichever bearer the session holds, the way
    /// the invoice itself is, so a contact reads it on the contact plane.
    #[test]
    fn the_ledger_is_fetched_on_any_bearer() {
        let src = include_str!("billing.rs");
        let head = &src[..src.find("mod payments_card_tests").expect("this module")];
        assert!(head.contains("get_authed_any::<RemoteInvoiceLedger>"));
        assert!(head.contains("\"/invoices/{id}/payments\""));
    }
}

#[cfg(test)]
mod invoice_preview_tests {
    use super::invoice_pay_now_preview;

    /// The two conditions the server refuses a send on are blockers, a
    /// missing gateway is a note (the invoice still goes, without its pay
    /// link), and a complete setup has neither: the preview never says
    /// "nothing will be sent" over a send that would mail (MAPPS-663).
    #[test]
    fn each_missing_condition_is_named_and_a_complete_setup_has_no_blockers() {
        let ok = invoice_pay_now_preview(
            "Acme MSP",
            "INV-000001",
            "50.00",
            "USD",
            "2026-09-30",
            Some(Some("ap@client.example")),
            Some(true),
        );
        assert!(ok.blockers.is_empty(), "{:?}", ok.blockers);
        assert!(ok.notes.is_empty(), "{:?}", ok.notes);
        assert_eq!(ok.recipient, "ap@client.example");
        assert_eq!(
            ok.subject,
            "Invoice INV-000001 from Acme MSP is ready to pay"
        );
        assert!(ok.body.contains("Amount due: 50.00 USD"));
        assert!(ok.body.contains("attached as INV-000001.pdf"));
        assert!(ok.body.contains("{{portal_link}}"));
        assert_eq!(ok.unresolved, vec!["portal_link", "contact_line"]);

        let no_contact =
            invoice_pay_now_preview("Acme MSP", "INV-1", "1", "", "", None, Some(true));
        assert_eq!(no_contact.blockers.len(), 1);
        assert!(no_contact.blockers[0].contains("no billing contact"));

        let no_email =
            invoice_pay_now_preview("Acme MSP", "INV-1", "1", "", "", Some(None), Some(true));
        assert!(no_email.blockers[0].contains("no email address"));

        let no_gateway = invoice_pay_now_preview(
            "Acme MSP",
            "INV-1",
            "1",
            "",
            "",
            Some(Some("a@b.c")),
            Some(false),
        );
        assert!(no_gateway.blockers.is_empty(), "{:?}", no_gateway.blockers);
        assert!(no_gateway.notes[0].contains("No payment gateway"));
        assert_eq!(no_gateway.subject, "Invoice INV-1 from Acme MSP");
        assert!(
            !no_gateway.body.contains("portal_link"),
            "no gateway, no pay paragraph: {}",
            no_gateway.body
        );
        assert_eq!(no_gateway.unresolved, vec!["contact_line"]);

        let unknown = invoice_pay_now_preview("", "INV-1", "1", "", "", Some(Some("a@b.c")), None);
        assert!(unknown.notes[0].contains("Could not check"));
        assert!(unknown
            .subject
            .starts_with("Invoice INV-1 from Your organisation"));
    }
}

#[cfg(test)]
mod emailed_line_tests {
    use super::emailed_line;

    /// The address and the date it went, and the address alone when the
    /// timestamp is missing or empty.
    #[test]
    fn the_row_says_who_and_when() {
        assert_eq!(
            emailed_line("ap@client.example", Some("2026-09-02T14:03:11Z")),
            "ap@client.example on 2026-09-02"
        );
        assert_eq!(emailed_line("ap@client.example", None), "ap@client.example");
        assert_eq!(
            emailed_line("ap@client.example", Some("")),
            "ap@client.example"
        );
    }
}

#[cfg(test)]
mod derived_due_date_tests {
    use super::derived_due_date;

    /// The hint mirrors the server's rule: invoice date plus the term's days,
    /// thirty when the term names no count, nothing until there is a date.
    #[test]
    fn the_hint_follows_the_servers_rule() {
        assert_eq!(
            derived_due_date("2026-03-01", Some(15)),
            Some("2026-03-16".to_string())
        );
        assert_eq!(
            derived_due_date("2026-03-01", Some(0)),
            Some("2026-03-01".to_string())
        );
        assert_eq!(
            derived_due_date("2026-03-01", None),
            Some("2026-03-31".to_string())
        );
        assert_eq!(derived_due_date("", Some(30)), None);
        assert_eq!(derived_due_date("not a date", Some(30)), None);
    }
}

#[cfg(test)]
mod invoice_tax_tests {
    use super::{
        computed_tax_amount, tax_body_fields, tax_label, unset_tax_rate_label, RemoteTaxRate,
        TaxBody,
    };
    use serde_json::Value;

    fn rate(id: &str, name: &str, pct: &str, is_default: bool, is_active: bool) -> RemoteTaxRate {
        RemoteTaxRate {
            id: uuid::Uuid::parse_str(id).unwrap(),
            name: name.to_string(),
            rate: pct.to_string(),
            is_default,
            is_active,
        }
    }

    const HST: &str = "11111111-1111-4111-8111-111111111111";
    const GST: &str = "22222222-2222-4222-8222-222222222222";

    /// The body names the rate or the override, never both (MAPPS-712): a
    /// typed override sends `tax_amount` alone, a picked rate sends
    /// `tax_rate_id` alone, nothing picked sends neither so the server's
    /// default applies, and a blanked override goes back to the rate.
    #[test]
    fn the_body_names_a_rate_or_an_override_never_both() {
        assert_eq!(
            tax_body_fields(HST, None),
            TaxBody {
                rate_id: Value::String(HST.to_string()),
                amount: Value::Null,
            }
        );
        assert_eq!(
            tax_body_fields(HST, Some("7.00")),
            TaxBody {
                rate_id: Value::Null,
                amount: Value::String("7.00".to_string()),
            }
        );
        assert_eq!(
            tax_body_fields("", Some(" 0 ")),
            TaxBody {
                rate_id: Value::Null,
                amount: Value::String("0".to_string()),
            }
        );
        assert_eq!(
            tax_body_fields("", None),
            TaxBody {
                rate_id: Value::Null,
                amount: Value::Null,
            }
        );
        assert_eq!(
            tax_body_fields(HST, Some("  ")),
            TaxBody {
                rate_id: Value::String(HST.to_string()),
                amount: Value::Null,
            }
        );
    }

    /// The preview follows the server's rule: the picked rate, else the
    /// tenant's active default, else nothing; an inactive default is not one.
    #[test]
    fn the_preview_falls_back_to_the_active_default_rate() {
        let rates = vec![
            rate(HST, "HST", "13.0000", true, true),
            rate(GST, "GST", "5", false, true),
        ];
        assert_eq!(computed_tax_amount(&rates, GST, "100"), "5");
        assert_eq!(computed_tax_amount(&rates, "", "100.50"), "13.07");
        assert_eq!(computed_tax_amount(&rates, "", "0"), "0");
        assert_eq!(unset_tax_rate_label(&rates), "Default: HST (13.0000%)");

        let retired = vec![rate(HST, "HST", "13", true, false)];
        assert_eq!(computed_tax_amount(&retired, "", "100"), "");
        assert_eq!(unset_tax_rate_label(&retired), "No tax");
        assert_eq!(computed_tax_amount(&[], "", "100"), "");
    }

    /// The totals row prints the recorded percent without trailing zeros,
    /// and a bare `Tax` when the invoice carries no rate.
    #[test]
    fn the_label_carries_the_recorded_rate() {
        assert_eq!(tax_label(Some("13.0000")), "Tax (13%)");
        assert_eq!(tax_label(Some("7.25")), "Tax (7.25%)");
        assert_eq!(tax_label(Some("")), "Tax");
        assert_eq!(tax_label(None), "Tax");
        assert_eq!(tax_label(Some("n/a")), "Tax");
    }
}

/// MAPPS-771: which Pay buttons an invoice offers.
#[cfg(test)]
mod pay_option_tests {
    use super::{pay_options, pay_options_for_render, RemotePaymentProvider};

    fn remote(provider: &str, label: &str) -> RemotePaymentProvider {
        RemotePaymentProvider {
            provider: provider.to_string(),
            label: label.to_string(),
        }
    }

    /// Two connected providers give two buttons, in the server's order, each
    /// naming the provider it pays through so the request can say which one
    /// was pressed.
    #[test]
    fn each_connected_provider_gets_its_own_button() {
        let options = pay_options(
            &[
                remote("paypal", "Pay with PayPal"),
                remote("stripe", "Pay with card"),
            ],
            Some("Pay with PayPal"),
        );
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].provider.as_deref(), Some("paypal"));
        assert_eq!(options[0].label, "Pay with PayPal");
        assert_eq!(options[1].provider.as_deref(), Some("stripe"));
    }

    /// A server that predates PMS-1179 sends one label and no list. That
    /// becomes a single button naming NO provider, which is the request every
    /// client sent before the choice existed, so an old server and a new
    /// client still transact.
    #[test]
    fn an_older_server_still_gets_one_working_button() {
        let options = pay_options(&[], Some("Pay with card"));
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].provider, None);
        assert_eq!(options[0].label, "Pay with card");
    }

    /// Nothing to offer is no button, not a button that cannot work.
    #[test]
    fn no_labels_means_no_buttons() {
        assert!(pay_options(&[], None).is_empty());
        assert!(pay_options(&[], Some("   ")).is_empty());
    }

    /// A malformed entry is dropped rather than rendered: a button with no
    /// label is invisible, and one naming no provider would pay through
    /// whichever gateway the server resolved, which is not what the customer
    /// pressed.
    #[test]
    fn an_incomplete_entry_is_not_offered() {
        let options = pay_options(&[remote("", "Pay with card"), remote("paypal", "  ")], None);
        assert!(options.is_empty(), "{options:?}");
    }

    /// Before readiness lands there is still a button, because one that
    /// appears late reads as a broken page; it carries the same words this
    /// page used before any of this.
    #[test]
    fn the_pending_state_keeps_one_button() {
        let rendered = pay_options_for_render(&[], "Pay Now");
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].label, "Pay Now");
        assert_eq!(rendered[0].provider, None);
    }

    /// Once readiness lands, the fallback is not mixed in with the real ones.
    #[test]
    fn resolved_choices_replace_the_fallback() {
        let choices = pay_options(&[remote("stripe", "Pay with card")], None);
        let rendered = pay_options_for_render(&choices, "Pay Now");
        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].provider.as_deref(), Some("stripe"));
        assert_eq!(rendered[0].label, "Pay with card");
    }
}

/// MAPPS-643: the locked-invoice note.
#[cfg(test)]
mod mapps643_locked_note_tests {
    use super::locked_invoice_note;

    /// Every frozen state has a note, a draft has none, and each note leads
    /// with the state, names an action the page offers, and never says
    /// "finalized record" or doubles up "cancelled, or voided".
    #[test]
    fn each_locked_state_says_what_it_is_and_what_can_be_done() {
        for draft in ["draft", "pending", "", "overdue-nonsense"] {
            assert_eq!(locked_invoice_note(draft), None, "{draft}");
        }
        let sent = locked_invoice_note("sent").expect("sent");
        assert!(sent.starts_with("This invoice was sent"));
        assert!(
            sent.contains("Record a payment")
                && sent.contains("write it off")
                && sent.contains("credit note")
        );
        let partly = locked_invoice_note("partially_paid").expect("partially_paid");
        assert!(partly.starts_with("This invoice is partly paid"));
        assert!(partly.contains("Record the rest") && partly.contains("credit note"));
        let paid = locked_invoice_note("paid").expect("paid");
        assert!(
            paid.contains("refund or correct") && !paid.contains("write"),
            "a paid invoice is not written off"
        );
        let void = locked_invoice_note("void").expect("void");
        assert!(void.contains("raise a new invoice"));
        let off = locked_invoice_note("written_off").expect("written_off");
        assert!(
            off.contains("recovery"),
            "PMS-1036: a late payment is a recovery, not refused"
        );
        for note in [sent, partly, paid, void, off] {
            assert!(!note.contains("finalized"), "{note}");
            assert!(!note.contains("cancelled, or voided"), "{note}");
            assert!(note.ends_with('.'), "{note}");
        }
    }
}

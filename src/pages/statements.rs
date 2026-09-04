//! Statements (MAPPS-639): a company's account over a period, the read model
//! PMS-954 added to the server, given a page.
//!
//! A statement is computed from the invoices, payments, refunds and credit
//! notes dated inside the period, with an opening balance carried in from
//! everything before it. The server stores nothing: there is no statement
//! row, no number, and no "issue" action, and this page is careful not to
//! suggest one. The only artefact is the PDF, which is fetched on demand.
//!
//! The period is therefore an input rather than a filter. Changing it changes
//! the opening balance, not just which rows are listed, so the page refetches
//! on every change and shows the arithmetic the server tests as one sum:
//! opening, plus invoiced, plus refunded, minus paid, minus credited, equals
//! closing.
//!
//! Conventions follow `billing.rs` and `credit_notes.rs`: page-local
//! `Deserialize` structs with `#[serde(default)]`, money as the server's
//! decimal strings, and the finance gate every billing page carries. The
//! period presets live in [`period`] as pure functions of a date, because the
//! host-side `cargo test --lib` has no browser.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    invoice_status_badge, use_page_title, Badge, Button, ButtonVariant, Card, ErrorBanner,
    PageHeader, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableRow,
};
use crate::utils::money::format_money_str;
use crate::Route;

/// `StatementResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteStatement {
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    period_start: String,
    #[serde(default)]
    period_end: String,
    #[serde(default)]
    opening_balance: String,
    #[serde(default)]
    invoices: Vec<RemoteStatementInvoice>,
    #[serde(default)]
    payments: Vec<RemoteStatementPayment>,
    #[serde(default)]
    refunds: Vec<RemoteStatementRefund>,
    #[serde(default)]
    credit_notes: Vec<RemoteStatementCredit>,
    #[serde(default)]
    total_invoiced: String,
    #[serde(default)]
    total_paid: String,
    #[serde(default)]
    total_refunded: String,
    #[serde(default)]
    total_credited: String,
    #[serde(default)]
    closing_balance: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteStatementInvoice {
    invoice_id: uuid::Uuid,
    #[serde(default)]
    invoice_number: String,
    #[serde(default)]
    invoice_date: String,
    #[serde(default)]
    due_date: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    total: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteStatementPayment {
    payment_id: uuid::Uuid,
    #[serde(default)]
    payment_date: String,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    payment_method: String,
    #[serde(default)]
    reference_number: Option<String>,
    #[serde(default)]
    invoice_number: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteStatementRefund {
    refund_id: uuid::Uuid,
    #[serde(default)]
    refund_date: String,
    #[serde(default)]
    amount: String,
    #[serde(default)]
    invoice_number: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteStatementCredit {
    credit_note_id: uuid::Uuid,
    #[serde(default)]
    credit_note_number: String,
    #[serde(default)]
    issue_date: String,
    #[serde(default)]
    total: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    invoice_number: Option<String>,
}

/// The one field of a company the picker needs to show a seeded selection.
#[derive(Clone, Debug, Deserialize)]
struct CompanyNameRow {
    #[serde(default)]
    name: String,
}

// ============================================================================
// The period, kept pure so it can be tested off-web
// ============================================================================

/// Presets and validation for the period input. Every function takes the
/// date it needs rather than reading the clock, so the tests can pin a day.
pub(crate) mod period {
    use chrono::{Datelike, Days, Months, NaiveDate};

    /// The preset keys, in the order the select offers them. `custom` is
    /// what the select shows once either date is edited by hand.
    pub(crate) const PRESETS: &[(&str, &str)] = &[
        ("this_month", "This month to date"),
        ("last_month", "Last month"),
        ("last_90_days", "Last 90 days"),
        ("year_to_date", "Year to date"),
        ("custom", "Custom"),
    ];

    /// The default on first load: the current month to date, whose closing
    /// balance is what is owed now.
    pub(crate) const DEFAULT: &str = "this_month";

    /// The inclusive range a preset names on `today`, or `None` for `custom`
    /// and anything unknown.
    pub(crate) fn range(preset: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
        let first_of_month = today.with_day(1)?;
        match preset {
            "this_month" => Some((first_of_month, today)),
            "last_month" => {
                let start = first_of_month.checked_sub_months(Months::new(1))?;
                let end = first_of_month.checked_sub_days(Days::new(1))?;
                Some((start, end))
            }
            "last_90_days" => Some((today.checked_sub_days(Days::new(89))?, today)),
            "year_to_date" => Some((today.with_month(1)?.with_day(1)?, today)),
            _ => None,
        }
    }

    /// `YYYY-MM-DD`, the shape a date field holds and the query carries.
    pub(crate) fn iso(date: NaiveDate) -> String {
        date.format("%Y-%m-%d").to_string()
    }

    /// Why the two dates cannot be sent, if they cannot. Both must parse and
    /// the end must not precede the start; the server's own validation says
    /// the same, this just says it beside the fields.
    pub(crate) fn error(start: &str, end: &str) -> Option<String> {
        let start = NaiveDate::parse_from_str(start.trim(), "%Y-%m-%d").ok();
        let end = NaiveDate::parse_from_str(end.trim(), "%Y-%m-%d").ok();
        match (start, end) {
            (None, _) | (_, None) => Some("Choose a start and an end date.".to_string()),
            (Some(s), Some(e)) if e < s => {
                Some("The end date is before the start date.".to_string())
            }
            _ => None,
        }
    }

    /// The query string for both statement routes.
    pub(crate) fn query(company_id: &str, start: &str, end: &str) -> String {
        format!("company_id={company_id}&period_start={start}&period_end={end}")
    }
}

// ============================================================================
// Page
// ============================================================================

/// Statement page. GET `/statements` for one company over a period.
/// Finance-gated like every billing page.
#[component]
pub fn StatementPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Statement");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Statement" } };
    }

    rsx! { StatementBody {} }
}

#[component]
fn StatementBody() -> Element {
    // Seeded from `?company_id=` so the company page's Statement link lands
    // here already scoped; the name is resolved below for the picker.
    let mut company_id =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    let mut company_name = use_signal(String::new);
    let today = crate::utils::datetime::user_today();
    let (default_start, default_end) =
        period::range(period::DEFAULT, today).unwrap_or((today, today));
    let mut preset = use_signal(|| period::DEFAULT.to_string());
    let mut period_start = use_signal(|| period::iso(default_start));
    let mut period_end = use_signal(|| period::iso(default_end));
    let action_error = use_signal(String::new);

    // The seeded company's name, once, so the picker shows what was chosen.
    let seeded_id = company_id.read().clone();
    let seeded_for_fetch = seeded_id.clone();
    let _name_resource = use_resource(move || {
        let id = seeded_for_fetch.clone();
        async move {
            if id.is_empty() || !company_name.read().is_empty() {
                return;
            }
            let _gen = crate::hooks::fetch::active_tenant_generation();
            if let Ok(row) = crate::hooks::fetch::api::get_authed::<CompanyNameRow>(&format!(
                "/contacts/companies/{id}"
            ))
            .await
            {
                if !row.name.trim().is_empty() {
                    company_name.set(row.name);
                }
            }
        }
    });

    let start_text = period_start.read().clone();
    let end_text = period_end.read().clone();
    let period_err = period::error(&start_text, &end_text);
    let company_text = company_id.read().trim().to_string();
    let ready = !company_text.is_empty() && period_err.is_none();

    let query_for_resource = period::query(&company_text, &start_text, &end_text);
    let ready_for_resource = ready;
    let statement_resource = use_resource(move || {
        let query = query_for_resource.clone();
        async move {
            if !ready_for_resource {
                return None;
            }
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the statement refetches
            // the instant the server comes back.
            let _reachable = crate::hooks::use_server_reachable();
            Some(
                crate::hooks::fetch::api::get_authed::<RemoteStatement>(&format!(
                    "/statements?{query}"
                ))
                .await
                .inspect_err(|e| tracing::error!("statement load failed: {e}"))
                .ok(),
            )
        }
    });

    let reachable = crate::hooks::use_server_reachable();
    let snap = statement_resource.read_unchecked();
    // `None` while loading, `Some(None)` when nothing was asked for,
    // `Some(Some(None))` when the fetch failed, `Some(Some(Some(s)))` on a
    // statement.
    let is_loading = ready && snap.is_none();
    let fetch_failed = matches!(&*snap, Some(Some(None)));
    let statement = match &*snap {
        Some(Some(Some(s))) => Some(s.clone()),
        _ => None,
    };

    // Every hook above every early return (MAPPS-602): the title hook sits
    // here, before the outage exit below, so no render is a hook short.
    let header_title = match &statement {
        Some(s) => match s.company_name.as_deref().filter(|n| !n.trim().is_empty()) {
            Some(name) => format!("Statement for {name}"),
            None => "Statement".to_string(),
        },
        None => "Statement".to_string(),
    };
    use_page_title(&header_title);

    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Statement".to_string() }
        };
    }

    let preset_options: Vec<SelectOption> = period::PRESETS
        .iter()
        .map(|(key, label)| SelectOption::new(*key, *label))
        .collect();
    let picker_selected_id = if company_text.is_empty() {
        None
    } else {
        Some(company_text.clone())
    };

    let query_for_pdf = period::query(&company_text, &start_text, &end_text);
    let pdf_name = format!("statement-{start_text}-{end_text}.pdf");
    let act_err = action_error.read().clone();

    rsx! {
        PageHeader {
            title: "{header_title}",
            subtitle: "A company's account over a period: what was invoiced, paid, refunded and credited, with the balance carried in and out",
            actions: rsx! {
                Link {
                    to: Route::InvoiceList {},
                    Button { variant: ButtonVariant::Secondary, "Invoices" }
                }
                // MAPPS-641: rendered now from current branding, because a
                // statement is computed and stored nowhere (PMS-954). The
                // artefact a client received is whatever PDF somebody sent.
                if statement.is_some() {
                    crate::components::DownloadButton {
                        path: format!("/statements/pdf?{query_for_pdf}"),
                        fallback_name: pdf_name.clone(),
                        what: "the statement PDF".to_string(),
                        variant: ButtonVariant::Primary,
                        title: "Rendered now, from the current branding: a statement is computed, not stored. This is the document to send.".to_string(),
                    }
                }
            },
        }

        // Nothing here is issued or stored. Say so, because a document with
        // totals and a Download button reads as one that was.
        p { class: "mb-4 text-sm text-muted",
            "A statement is computed from the invoices, payments, refunds and credit notes dated in the period, with everything before it folded into the opening balance. It is not stored or numbered: run it again after a backdated payment and it changes, correctly. The PDF is the document to send."
        }

        Card { class: "mb-4",
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4",
                crate::components::CompanyPicker {
                    value: company_name.read().clone(),
                    selected_id: picker_selected_id,
                    required: true,
                    allow_inline_create: false,
                    onselect: move |(id, name): (String, String)| {
                        company_id.set(id);
                        company_name.set(name);
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                    },
                }
                Select {
                    name: "statement_period",
                    label: "Period",
                    options: preset_options,
                    value: preset.read().clone(),
                    onchange: move |e: FormEvent| {
                        let key = e.value();
                        if let Some((start, end)) = period::range(&key, today) {
                            period_start.set(period::iso(start));
                            period_end.set(period::iso(end));
                        }
                        preset.set(key);
                    },
                }
                crate::components::DateField {
                    name: "statement_period_start",
                    label: "From",
                    required: true,
                    value: period_start.read().clone(),
                    oninput: move |e: FormEvent| {
                        period_start.set(e.value());
                        preset.set("custom".to_string());
                    },
                }
                crate::components::DateField {
                    name: "statement_period_end",
                    label: "To",
                    required: true,
                    error: period_err.clone().unwrap_or_default(),
                    value: period_end.read().clone(),
                    oninput: move |e: FormEvent| {
                        period_end.set(e.value());
                        preset.set("custom".to_string());
                    },
                }
            }
        }

        if !act_err.is_empty() {
            ErrorBanner { class: "mb-3", "{act_err}" }
        }
        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load the statement. Check the period and try again." }
        }

        if company_text.is_empty() {
            Card {
                div { class: "py-10 text-center",
                    p { class: "text-sm text-muted", "Choose a company to see its statement." }
                }
            }
        } else if is_loading {
            crate::components::DetailSkeleton {}
        } else if let Some(s) = statement.as_ref() {
            StatementDocument { statement: s.clone() }
        }
    }
}

/// The statement itself: the sum, then the four sections.
#[component]
fn StatementDocument(statement: RemoteStatement) -> Element {
    let s = statement;
    let opening = format_money_str(&s.opening_balance);
    let invoiced = format_money_str(&s.total_invoiced);
    let refunded = format_money_str(&s.total_refunded);
    let paid = format_money_str(&s.total_paid);
    let credited = format_money_str(&s.total_credited);
    let closing = format_money_str(&s.closing_balance);
    let period_label = format!("{} to {}", s.period_start, s.period_end);
    let quiet = s.invoices.is_empty()
        && s.payments.is_empty()
        && s.refunds.is_empty()
        && s.credit_notes.is_empty();

    rsx! {
        div { class: "space-y-6",
            // The identity the server tests, as one sum. Signs are spelled
            // out so a reader can add it up: a refund puts money back on the
            // account, a credit takes it off.
            Card { title: "Balance",
                p { class: "mb-4 text-sm text-muted", "{period_label}" }
                if quiet {
                    p { class: "mb-4 text-sm text-muted",
                        "No activity in this period. The balance carried in is the balance carried out."
                    }
                }
                dl { class: "max-w-md space-y-2 text-sm",
                    div { class: "flex justify-between",
                        dt { class: "text-muted", "Opening balance" }
                        dd { class: "font-medium", "{opening}" }
                    }
                    div { class: "flex justify-between",
                        dt { class: "text-muted", "+ Invoiced" }
                        dd { "{invoiced}" }
                    }
                    div { class: "flex justify-between",
                        dt { class: "text-muted", "+ Refunded" }
                        dd { "{refunded}" }
                    }
                    div { class: "flex justify-between",
                        dt { class: "text-muted", "- Paid" }
                        dd { "{paid}" }
                    }
                    div { class: "flex justify-between",
                        dt { class: "text-muted", "- Credited" }
                        dd { "{credited}" }
                    }
                    div { class: "flex justify-between border-t border-line pt-2 text-lg font-bold",
                        dt { "Closing balance" }
                        dd { "{closing}" }
                    }
                }
            }

            Card { title: "Invoices", class: "overflow-x-auto",
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Invoice" }
                            TableHeader { "Date" }
                            TableHeader { "Due" }
                            TableHeader { "Status" }
                            TableHeader { class: "text-right", "Total" }
                        }
                    }
                    if s.invoices.is_empty() {
                        TableEmpty { columns: 5, message: "No invoices in this period.".to_string() }
                    } else {
                        TableBody {
                            for inv in s.invoices.iter().cloned() {
                                {
                                    let (variant, label) = invoice_status_badge(&inv.status);
                                    rsx! {
                                        TableRow { key: "{inv.invoice_id}",
                                            TableCell {
                                                Link {
                                                    to: Route::InvoiceDetail { id: inv.invoice_id.to_string() },
                                                    class: "font-medium text-accent hover:opacity-90",
                                                    "{inv.invoice_number}"
                                                }
                                            }
                                            TableCell { "{inv.invoice_date}" }
                                            TableCell { "{inv.due_date}" }
                                            TableCell { Badge { variant, "{label}" } }
                                            TableCell { class: "text-right font-medium", "{format_money_str(&inv.total)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Card { title: "Payments", class: "overflow-x-auto",
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Method" }
                            TableHeader { "Reference" }
                            TableHeader { "Applied to" }
                            TableHeader { class: "text-right", "Amount" }
                        }
                    }
                    if s.payments.is_empty() {
                        TableEmpty { columns: 5, message: "No payments in this period.".to_string() }
                    } else {
                        TableBody {
                            for p in s.payments.iter().cloned() {
                                TableRow { key: "{p.payment_id}",
                                    TableCell { "{p.payment_date}" }
                                    TableCell { "{crate::pages::billing::humanize_payment_method(&p.payment_method)}" }
                                    TableCell { {dash_if_empty(p.reference_number.as_deref())} }
                                    TableCell { {dash_if_empty(p.invoice_number.as_deref())} }
                                    TableCell { class: "text-right font-medium", "{format_money_str(&p.amount)}" }
                                }
                            }
                        }
                    }
                }
            }

            Card { title: "Refunds", class: "overflow-x-auto",
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Against" }
                            TableHeader { class: "text-right", "Amount" }
                        }
                    }
                    if s.refunds.is_empty() {
                        TableEmpty { columns: 3, message: "No refunds in this period.".to_string() }
                    } else {
                        TableBody {
                            for r in s.refunds.iter().cloned() {
                                TableRow { key: "{r.refund_id}",
                                    TableCell { "{r.refund_date}" }
                                    TableCell { {dash_if_empty(r.invoice_number.as_deref())} }
                                    TableCell { class: "text-right font-medium", "{format_money_str(&r.amount)}" }
                                }
                            }
                        }
                    }
                }
            }

            Card { title: "Credit Notes", class: "overflow-x-auto",
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Credit note" }
                            TableHeader { "Date" }
                            TableHeader { "Against" }
                            TableHeader { "Reason" }
                            TableHeader { class: "text-right", "Total" }
                        }
                    }
                    if s.credit_notes.is_empty() {
                        TableEmpty { columns: 5, message: "No credit notes in this period.".to_string() }
                    } else {
                        TableBody {
                            for c in s.credit_notes.iter().cloned() {
                                TableRow { key: "{c.credit_note_id}",
                                    TableCell {
                                        Link {
                                            to: Route::CreditNoteDetail { id: c.credit_note_id.to_string() },
                                            class: "font-medium text-accent hover:opacity-90",
                                            "{c.credit_note_number}"
                                        }
                                    }
                                    TableCell { "{c.issue_date}" }
                                    TableCell { {dash_if_empty(c.invoice_number.as_deref())} }
                                    TableCell { class: "max-w-xs",
                                        span { class: "block truncate", title: "{c.reason}", "{c.reason}" }
                                    }
                                    TableCell { class: "text-right font-medium", "{format_money_str(&c.total)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A cell that shows a subtle dash for a missing value, the way the invoice
/// list does, so an empty reference does not read as a rendering slip.
fn dash_if_empty(value: Option<&str>) -> Element {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => rsx! { "{v}" },
        None => rsx! { span { class: "text-subtle", "-" } },
    }
}

#[cfg(test)]
mod tests {
    use super::period::*;
    use chrono::NaiveDate;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Each preset on a day where the arithmetic has an edge to get wrong:
    /// mid-March, so "last month" is February and "last 90 days" crosses the
    /// year boundary.
    #[test]
    fn presets_name_the_expected_inclusive_ranges() {
        let today = day(2026, 3, 15);
        assert_eq!(range("this_month", today), Some((day(2026, 3, 1), today)));
        assert_eq!(
            range("last_month", today),
            Some((day(2026, 2, 1), day(2026, 2, 28)))
        );
        assert_eq!(
            range("last_90_days", today),
            Some((day(2025, 12, 16), today))
        );
        assert_eq!(range("year_to_date", today), Some((day(2026, 1, 1), today)));
        assert_eq!(range("custom", today), None);
        assert_eq!(range("nonsense", today), None);
    }

    /// The first of a month is the day "this month" is a single day and
    /// "last month" is the whole previous one; January's last month is
    /// December of the year before.
    #[test]
    fn presets_hold_on_the_first_of_january() {
        let today = day(2026, 1, 1);
        assert_eq!(range("this_month", today), Some((today, today)));
        assert_eq!(
            range("last_month", today),
            Some((day(2025, 12, 1), day(2025, 12, 31)))
        );
        assert_eq!(range("year_to_date", today), Some((today, today)));
    }

    #[test]
    fn the_default_preset_is_offered() {
        assert!(PRESETS.iter().any(|(key, _)| *key == DEFAULT));
        assert!(range(DEFAULT, day(2026, 6, 10)).is_some());
    }

    #[test]
    fn a_period_must_parse_and_run_forwards() {
        assert_eq!(error("2026-03-01", "2026-03-31"), None);
        assert_eq!(
            error("2026-03-01", "2026-03-01"),
            None,
            "one day is a period"
        );
        assert!(error("2026-03-31", "2026-03-01").is_some());
        assert!(error("", "2026-03-01").is_some());
        assert!(error("2026-03-01", "not a date").is_some());
    }

    #[test]
    fn the_query_carries_the_three_parameters_the_server_reads() {
        assert_eq!(
            query("abc", "2026-03-01", "2026-03-31"),
            "company_id=abc&period_start=2026-03-01&period_end=2026-03-31"
        );
        assert_eq!(iso(day(2026, 3, 5)), "2026-03-05");
    }
}

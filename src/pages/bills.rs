//! The payables ledger (MAPPS-1157, server MAPPS-1100 Phases B and 2).
//!
//! A bill is what the MSP owes a vendor: the opposite direction from an
//! invoice, with its own lifecycle (`draft -> approved -> paid`, or
//! `void`). The server has carried the whole thing since Phase B and
//! nothing called it, so no bill could be entered, and the delegation
//! MAPPS-1155 shipped could not be exercised by a human at all.
//!
//! # Minor units, not decimal strings
//!
//! `bills` stores money in `i64` minor units, where `invoices` serialises a
//! decimal string. The form takes what a person types (`50.00`, `1.25`) and
//! converts; the conversion is a pure function with its own tests, because
//! a rounding slip here is a wrong number in somebody's ledger rather than
//! a cosmetic one.
//!
//! # What a delegated bill shows
//!
//! A bill the provider holds carries `external_provider` and
//! `external_bill_id`. The detail page shows both, because that pair is
//! what a reconciliation reads, and the list shows the provider alone: the
//! question there is "which of these are over there", not "what is their
//! id".
//!
//! # Why approve and void and nothing else
//!
//! Those are the two transitions the server mounts. `paid` is reached by
//! recording a payment, which has no route yet, and a bill cannot be
//! edited after creation. Offering buttons for either would be offering
//! what the build cannot do.

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, DataTable,
    ErrorBanner, IconSize, Modal, PageHeader, PlusIcon, Select, SelectOption, StatusBanner, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::utils::money::format_money_minor;
use crate::utils::Paginated;
use crate::Route;

const PER_PAGE: usize = 25;

/// `Bill`, as `GET /bills` and `GET /bills/{id}` serve it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RemoteBill {
    pub id: Uuid,
    pub vendor_id: Uuid,
    #[serde(default)]
    pub bill_number: Option<String>,
    #[serde(default)]
    pub issue_date: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub subtotal_minor: i64,
    #[serde(default)]
    pub tax_minor: i64,
    #[serde(default)]
    pub total_minor: i64,
    #[serde(default)]
    pub paid_minor: i64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// MAPPS-1155: who holds this bill, when it was delegated.
    #[serde(default)]
    pub external_provider: Option<String>,
    #[serde(default)]
    pub external_bill_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteBillLine {
    id: Uuid,
    #[serde(default)]
    category_id: Option<Uuid>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quantity_hundredths: i64,
    #[serde(default)]
    unit_price_minor: i64,
    #[serde(default)]
    tax_minor: i64,
    #[serde(default)]
    total_minor: i64,
}

/// `BillWithLines`, which the server FLATTENS: the bill's own fields sit
/// at the top level with `lines` beside them, not under a `bill` key
/// (MAPPS-1164). Without the flatten here the whole response fails to
/// deserialise and the detail page renders its skeleton forever.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteBillWithLines {
    #[serde(flatten)]
    bill: RemoteBill,
    #[serde(default)]
    lines: Vec<RemoteBillLine>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct NamedRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// The list path for a page. `GET /bills` takes pagination and nothing
/// else, so there is no filter row offering one.
pub(crate) fn list_path(page: usize) -> String {
    format!("/bills?page={page}&per_page={PER_PAGE}")
}

/// What a person typed as money, in minor units.
///
/// Returns `None` for anything that is not a plain amount with at most two
/// decimals, so the form refuses it rather than truncating somebody's
/// third decimal into a number they did not enter.
pub(crate) fn minor_from_str(raw: &str) -> Option<i64> {
    scaled_from_str(raw, 2)
}

/// What a person typed as a quantity, in hundredths (so `1.25` is 125).
pub(crate) fn hundredths_from_str(raw: &str) -> Option<i64> {
    scaled_from_str(raw, 2)
}

/// Parse a decimal with at most `places` decimals into a scaled integer.
///
/// By hand rather than through `Decimal`: the only inputs are a text field
/// with `type="number"`, the only output is an `i64` the server stores, and
/// a parse that silently accepted `1.239` would put 123 in the ledger and
/// show 1.24 on the screen.
fn scaled_from_str(raw: &str, places: u32) -> Option<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1i64, trimmed),
    };
    let (whole, frac) = match digits.split_once('.') {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if frac.chars().count() > places as usize {
        return None;
    }
    let scale = 10i64.pow(places);
    let whole_v: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut frac_v: i64 = if frac.is_empty() {
        0
    } else {
        frac.parse().ok()?
    };
    // "5" in a two-decimal field is 50 hundredths, not 5.
    for _ in frac.chars().count()..places as usize {
        frac_v *= 10;
    }
    whole_v
        .checked_mul(scale)
        .and_then(|w| w.checked_add(frac_v))
        .map(|v| v * sign)
}

/// The badge for one of the four statuses the server's CHECK allows.
///
/// `paid` is green and `approved` is blue: approved is a commitment and
/// paid is a settled one, and a reader scanning the column wants to tell
/// what still needs money from what does not. An unknown status renders as
/// itself in gray rather than being hidden, because a status this build
/// does not know means the server is ahead of it.
pub(crate) fn status_badge(status: &str) -> (String, BadgeVariant) {
    match status {
        "draft" => ("Draft".to_string(), BadgeVariant::Gray),
        "approved" => ("Approved".to_string(), BadgeVariant::Blue),
        "paid" => ("Paid".to_string(), BadgeVariant::Green),
        "void" => ("Void".to_string(), BadgeVariant::Red),
        other => (other.to_string(), BadgeVariant::Gray),
    }
}

/// Which transitions the server accepts from this status.
///
/// `approve` is draft-only and `void` is draft or approved; a paid or void
/// bill is terminal. Pure, so the buttons and the tests read the same rule.
pub(crate) fn can_approve(status: &str) -> bool {
    status == "draft"
}

pub(crate) fn can_void(status: &str) -> bool {
    status == "draft" || status == "approved"
}

#[component]
pub fn BillListPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Bills");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Bills" } };
    }
    rsx! { BillListBody {} }
}

#[component]
fn BillListBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut creating = use_signal(|| false);

    let current_page = (*page.read()).max(1);
    let mut resource = use_resource(move || {
        // Read inside the closure so the resource subscribes to the page
        // signal itself (MAPPS-847).
        let path = list_path((*page.read()).max(1));
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteBill>>(&path)
                .await
                .inspect_err(|e| tracing::error!("bill list load failed: {e}"))
                .ok()
        }
    });

    // The list serves `vendor_id`, and a column of uuids answers nothing.
    let vendors = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/vendors")
            .await
            .inspect_err(|e| tracing::error!("vendor list load failed: {e}"))
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteBill>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let vendor_names: HashMap<Uuid, String> = vendors
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, v.name))
        .collect();

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    let navigator = use_navigator();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Bills".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Bills",
            subtitle: "What you owe your vendors",
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| creating.set(true),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Bill"
                }
            },
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load bills. Refresh the page to retry." }
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
                        TableHeader { "Vendor" }
                        TableHeader { "Number" }
                        TableHeader { "Issued" }
                        TableHeader { "Due" }
                        TableHeader { class: "text-right", "Total" }
                        TableHeader { "Status" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 6,
                        title: "No bills yet".to_string(),
                        description: "Enter what a vendor billed you, with its lines, and approve it when you have checked it.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                                onclick: move |_| creating.set(true),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Bill"
                            }
                        },
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let id = row.id;
                                let vendor = vendor_names
                                    .get(&row.vendor_id)
                                    .cloned()
                                    .unwrap_or_else(|| "Unknown vendor".to_string());
                                let number = row.bill_number.clone().unwrap_or_default();
                                let issued = row.issue_date.clone();
                                let due = row.due_date.clone().unwrap_or_default();
                                let total = format_money_minor(row.total_minor);
                                let (label, tone) = status_badge(&row.status);
                                let provider = row.external_provider.clone();
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| {
                                            navigator.push(Route::BillDetail { id: id.to_string() });
                                        },
                                        TableCell {
                                            // MAPPS-569: the row navigates, so this cell is the
                                            // keyboard path.
                                            onactivate: move |_| {
                                                navigator.push(Route::BillDetail { id: id.to_string() });
                                            },
                                            span { class: "font-medium text-accent", "{vendor}" }
                                            if let Some(p) = provider.clone() {
                                                Badge { variant: BadgeVariant::Purple, class: "ml-2", "In {p}" }
                                            }
                                        }
                                        TableCell {
                                            if number.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { class: "font-mono text-xs", "{number}" }
                                            }
                                        }
                                        TableCell { "{issued}" }
                                        TableCell {
                                            if due.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { "{due}" }
                                            }
                                        }
                                        TableCell { class: "text-right font-medium", "{total}" }
                                        TableCell {
                                            Badge { variant: tone, "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if creating() {
            NewBillModal {
                onclose: move |_| creating.set(false),
                onsaved: move |_| {
                    creating.set(false);
                    resource.restart();
                },
            }
        }
    }
}

// ============================================================================
// Create
// ============================================================================

/// One line being entered. Strings, because that is what the fields hold
/// until the body is built and the conversion can refuse.
#[derive(Clone, Debug, PartialEq, Default)]
struct LineDraft {
    description: String,
    category_id: String,
    quantity: String,
    unit_price: String,
    tax: String,
}

impl LineDraft {
    fn new() -> Self {
        Self {
            quantity: "1".to_string(),
            ..Default::default()
        }
    }
}

/// The `CreateBillRequest` body, or the first reason it cannot be built.
///
/// Pure and fallible together: every number a person typed is converted
/// here, so a refusal names the line and nothing half-converted reaches the
/// server.
pub(crate) fn bill_body(
    vendor_id: &str,
    bill_number: &str,
    issue_date: &str,
    due_date: &str,
    currency: &str,
    notes: &str,
    lines: &[(String, String, String, String, String)],
) -> Result<serde_json::Value, String> {
    if vendor_id.trim().is_empty() {
        return Err("Pick the vendor this bill is from.".to_string());
    }
    if issue_date.trim().is_empty() {
        return Err("A bill needs the date it was issued.".to_string());
    }
    let currency = currency.trim().to_uppercase();
    if currency.chars().count() != 3 {
        return Err("Currency is a 3-letter code, like USD.".to_string());
    }
    if lines.is_empty() {
        return Err("A bill needs at least one line.".to_string());
    }
    let mut built = Vec::with_capacity(lines.len());
    for (position, (description, category_id, quantity, unit_price, tax)) in
        lines.iter().enumerate()
    {
        let what = if description.trim().is_empty() {
            format!("Line {}", position + 1)
        } else {
            description.trim().to_string()
        };
        if description.trim().is_empty() {
            return Err(format!("{what} needs a description."));
        }
        let quantity_hundredths = hundredths_from_str(quantity)
            .ok_or_else(|| format!("{what}: quantity is an amount with at most 2 decimals."))?;
        let unit_price_minor = minor_from_str(unit_price)
            .ok_or_else(|| format!("{what}: unit price is an amount with at most 2 decimals."))?;
        // Blank tax is none, which is the common case: most vendor lines on
        // an MSP's bills carry tax on the document and not per line.
        let tax_minor = if tax.trim().is_empty() {
            0
        } else {
            minor_from_str(tax)
                .ok_or_else(|| format!("{what}: tax is an amount with at most 2 decimals."))?
        };
        built.push(serde_json::json!({
            "category_id": if category_id.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(category_id.trim().to_string())
            },
            "description": description.trim(),
            "quantity_hundredths": quantity_hundredths,
            "unit_price_minor": unit_price_minor,
            "tax_minor": tax_minor,
            "position": position as i64,
        }));
    }
    Ok(serde_json::json!({
        "vendor_id": vendor_id.trim(),
        "bill_number": if bill_number.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(bill_number.trim().to_string())
        },
        "issue_date": issue_date.trim(),
        "due_date": if due_date.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(due_date.trim().to_string())
        },
        "currency": currency,
        "notes": if notes.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(notes.trim().to_string())
        },
        "lines": built,
    }))
}

#[derive(Props, Clone, PartialEq)]
struct NewBillModalProps {
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn NewBillModal(props: NewBillModalProps) -> Element {
    let mut vendor_id = use_signal(String::new);
    let mut bill_number = use_signal(String::new);
    let mut issue_date = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut currency = use_signal(|| "USD".to_string());
    let mut notes = use_signal(String::new);
    let mut lines = use_signal(|| vec![LineDraft::new()]);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let vendors = use_resource(move || async move {
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/vendors")
            .await
            .inspect_err(|e| tracing::error!("vendor list load failed: {e}"))
            .ok()
    });
    let categories = use_resource(move || async move {
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/expense-categories")
            .await
            .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
            .ok()
    });

    let vendor_rows = vendors
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();
    let category_rows = categories
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();

    let mut vendor_options = vec![SelectOption::new("", "Pick a vendor")];
    vendor_options.extend(
        vendor_rows
            .iter()
            .map(|v| SelectOption::new(v.id.to_string(), v.name.clone())),
    );
    let mut category_options = vec![SelectOption::new("", "No category")];
    category_options.extend(
        category_rows
            .iter()
            .map(|c| SelectOption::new(c.id.to_string(), c.name.clone())),
    );

    // What the server will compute, shown while it is typed: a total that
    // only appears after saving is one nobody can check against the paper
    // bill in front of them.
    let preview: i64 = lines
        .read()
        .iter()
        .map(|l| {
            let q = hundredths_from_str(&l.quantity).unwrap_or(0);
            let p = minor_from_str(&l.unit_price).unwrap_or(0);
            let t = if l.tax.trim().is_empty() {
                0
            } else {
                minor_from_str(&l.tax).unwrap_or(0)
            };
            q.saturating_mul(p) / 100 + t
        })
        .sum();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());
        let drafts: Vec<(String, String, String, String, String)> = lines
            .read()
            .iter()
            .map(|l| {
                (
                    l.description.clone(),
                    l.category_id.clone(),
                    l.quantity.clone(),
                    l.unit_price.clone(),
                    l.tax.clone(),
                )
            })
            .collect();
        let body = match bill_body(
            &vendor_id.read(),
            &bill_number.read(),
            &issue_date.read(),
            &due_date.read(),
            &currency.read(),
            &notes.read(),
            &drafts,
        ) {
            Ok(body) => body,
            Err(message) => {
                error.set(message);
                return;
            }
        };
        saving.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>("/bills", &body)
                    .await
                {
                    Ok(_) => onsaved.call(()),
                    // The delegated path refuses an unmapped vendor and an
                    // unmapped expense category by name (MAPPS-1155), and
                    // that message names the page that fixes it, so it is
                    // shown as it came rather than summarised.
                    Err(err) => error.set(format!("Could not create the bill: {err}")),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = body;
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "New Bill".to_string(),
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *saving.read(),
                    onclick: handle_save,
                    data_testid: "create-bill",
                    "Create Bill"
                }
            },
            div { class: "space-y-4",
                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    Select {
                        name: "bill_vendor",
                        label: "Vendor",
                        required: true,
                        options: vendor_options,
                        value: vendor_id.read().clone(),
                        onchange: move |e: FormEvent| vendor_id.set(e.value()),
                    }
                    crate::components::Input {
                        name: "bill_number",
                        label: "Their bill number",
                        placeholder: "Optional",
                        maxlength: 100,
                        help: "The number on their document, so the two can be matched.",
                        value: bill_number.read().clone(),
                        oninput: move |e: FormEvent| bill_number.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                    crate::components::Input {
                        name: "bill_issue_date",
                        label: "Issued",
                        r#type: "date",
                        required: true,
                        value: issue_date.read().clone(),
                        oninput: move |e: FormEvent| issue_date.set(e.value()),
                    }
                    crate::components::Input {
                        name: "bill_due_date",
                        label: "Due",
                        r#type: "date",
                        help: "Blank means they did not say.",
                        value: due_date.read().clone(),
                        oninput: move |e: FormEvent| due_date.set(e.value()),
                    }
                    crate::components::Input {
                        name: "bill_currency",
                        label: "Currency",
                        required: true,
                        maxlength: 3,
                        value: currency.read().clone(),
                        oninput: move |e: FormEvent| currency.set(e.value()),
                    }
                }
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        p { class: "text-sm font-medium text-content", "Lines" }
                        span { class: "text-sm text-muted", "Total {format_money_minor(preview)}" }
                    }
                    for (index, line) in lines.read().clone().into_iter().enumerate() {
                        div { key: "{index}", class: "grid grid-cols-1 gap-3 rounded-md border border-line p-3 sm:grid-cols-12",
                            div { class: "sm:col-span-4",
                                crate::components::Input {
                                    name: "line_description_{index}",
                                    label: "Description",
                                    required: true,
                                    maxlength: 500,
                                    value: line.description.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].description = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-3",
                                Select {
                                    name: "line_category_{index}",
                                    label: "Category",
                                    options: category_options.clone(),
                                    value: line.category_id.clone(),
                                    onchange: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].category_id = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-1",
                                crate::components::Input {
                                    name: "line_quantity_{index}",
                                    label: "Qty",
                                    r#type: "number",
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    value: line.quantity.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].quantity = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-2",
                                crate::components::Input {
                                    name: "line_unit_price_{index}",
                                    label: "Unit price",
                                    r#type: "number",
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "0.00",
                                    value: line.unit_price.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].unit_price = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-2",
                                crate::components::Input {
                                    name: "line_tax_{index}",
                                    label: "Tax",
                                    r#type: "number",
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "0.00",
                                    value: line.tax.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].tax = e.value());
                                    },
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| lines.with_mut(|ls| ls.push(LineDraft::new())),
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Add line"
                        }
                        // The server refuses a bill with no lines, so the last
                        // one cannot be removed.
                        if lines.read().len() > 1 {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| lines.with_mut(|ls| { ls.pop(); }),
                                "Remove last line"
                            }
                        }
                    }
                }
                crate::components::Textarea {
                    name: "bill_notes",
                    label: "Notes",
                    placeholder: "Optional",
                    rows: 2,
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
    }
}

// ============================================================================
// Detail
// ============================================================================

/// Post one lifecycle transition and reload the bill.
///
/// A free function rather than a closure the two buttons share: a closure
/// that captures signals is not `Copy`, so the second button could not have
/// it. The signals themselves are `Copy`, which is what makes this work.
fn fire_transition(
    bill_id: String,
    action: &'static str,
    mut busy: Signal<bool>,
    mut error: Signal<String>,
    mut confirm_void: Signal<bool>,
    mut resource: Resource<Option<RemoteBillWithLines>>,
) {
    let path = format!("/bills/{bill_id}/{action}");
    busy.set(true);
    error.set(String::new());
    confirm_void.set(false);
    spawn(async move {
        #[cfg(feature = "app")]
        {
            if let Err(e) = crate::hooks::fetch::api::post_authed_no_content(&path).await {
                error.set(format!("Could not {action} the bill: {e}"));
            }
        }
        #[cfg(not(feature = "app"))]
        let _ = path;
        busy.set(false);
        resource.restart();
    });
}

#[component]
pub fn BillDetailPage(id: String) -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Bill");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Bill" } };
    }
    rsx! { BillDetailBody { id } }
}

#[component]
fn BillDetailBody(id: String) -> Element {
    let bill_id = id.clone();
    // Not `mut`: every write goes through `fire_transition`, which takes
    // them by value, and `Signal` is `Copy`.
    let error = use_signal(String::new);
    let busy = use_signal(|| false);
    let mut confirm_void = use_signal(|| false);

    let resource = use_resource(use_reactive(&bill_id, |bill_id| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<RemoteBillWithLines>(&format!("/bills/{bill_id}"))
            .await
            .inspect_err(|e| tracing::error!("bill load failed: {e}"))
            .ok()
    }));

    let vendors = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/vendors")
            .await
            .inspect_err(|e| tracing::error!("vendor list load failed: {e}"))
            .ok()
    });
    let categories = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/expense-categories")
            .await
            .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
            .ok()
    });

    let snap = resource.read_unchecked().clone();
    let data = snap.as_ref().and_then(|o| o.as_ref()).cloned();
    let vendor_names: HashMap<Uuid, String> = vendors
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, v.name))
        .collect();
    let category_names: HashMap<Uuid, String> = categories
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    let can_mutate = crate::hooks::use_can_mutate();

    let approve_id = bill_id.clone();
    let void_id = bill_id.clone();

    rsx! {
        PageHeader {
            title: "Bill",
            subtitle: "What a vendor billed you, and what has been done with it",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Bills", Route::BillList {}, "Bill"),
                }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match data {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(loaded) => {
                let bill = loaded.bill.clone();
                let (label, tone) = status_badge(&bill.status);
                let vendor = vendor_names
                    .get(&bill.vendor_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown vendor".to_string());
                let number = bill.bill_number.clone().unwrap_or_default();
                let due = bill.due_date.clone().unwrap_or_default();
                let notes = bill.notes.clone().unwrap_or_default();
                let provider = bill.external_provider.clone();
                let external_id = bill.external_bill_id.clone();
                let approvable = can_approve(&bill.status) && can_mutate;
                let voidable = can_void(&bill.status) && can_mutate;
                rsx! {
                    Card {
                        title: vendor.clone(),
                        actions: rsx! { Badge { variant: tone, "{label}" } },
                        div { class: "space-y-4",
                            if let (Some(p), Some(external)) = (provider.clone(), external_id.clone()) {
                                StatusBanner { tone: crate::components::BannerTone::Info,
                                    "This bill is held in {p} as {external}. It was raised there when it was created here, and that copy is what your accountant reconciles."
                                }
                            }
                            dl { class: "grid grid-cols-1 gap-4 text-sm sm:grid-cols-4",
                                div {
                                    dt { class: "text-muted", "Their number" }
                                    dd { class: "font-medium text-content",
                                        if number.is_empty() { "-" } else { "{number}" }
                                    }
                                }
                                div {
                                    dt { class: "text-muted", "Issued" }
                                    dd { class: "font-medium text-content", "{bill.issue_date}" }
                                }
                                div {
                                    dt { class: "text-muted", "Due" }
                                    dd { class: "font-medium text-content",
                                        if due.is_empty() { "-" } else { "{due}" }
                                    }
                                }
                                div {
                                    dt { class: "text-muted", "Currency" }
                                    dd { class: "font-medium text-content", "{bill.currency}" }
                                }
                            }
                            if !notes.is_empty() {
                                p { class: "text-sm text-muted", "{notes}" }
                            }
                            div { class: "flex gap-2 pt-2",
                                if approvable {
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        loading: busy(),
                                        onclick: move |_| fire_transition(approve_id.clone(), "approve", busy, error, confirm_void, resource),
                                        data_testid: "approve-bill",
                                        "Approve"
                                    }
                                }
                                if voidable {
                                    Button {
                                        variant: ButtonVariant::Danger,
                                        loading: busy(),
                                        onclick: move |_| confirm_void.set(true),
                                        data_testid: "void-bill",
                                        "Void"
                                    }
                                }
                            }
                            if !approvable && !voidable {
                                p { class: "text-sm text-muted",
                                    "A paid or voided bill is final here. A correction is a new bill, the way an invoice amendment is a new invoice."
                                }
                            }
                            ConfirmDialog {
                                open: confirm_void(),
                                title: "Void this bill?".to_string(),
                                message: "The bill stays on record with its lines and stops counting as owed. It cannot be brought back; enter a new one if it was voided by mistake.".to_string(),
                                confirm_text: "Void".to_string(),
                                destructive: true,
                                loading: busy(),
                                onconfirm: move |_| fire_transition(void_id.clone(), "void", busy, error, confirm_void, resource),
                                oncancel: move |_| confirm_void.set(false),
                            }
                        }
                    }

                    Card { title: "Lines".to_string(), class: "mt-4",
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Description" }
                                    TableHeader { "Category" }
                                    TableHeader { class: "text-right", "Qty" }
                                    TableHeader { class: "text-right", "Unit price" }
                                    TableHeader { class: "text-right", "Tax" }
                                    TableHeader { class: "text-right", "Total" }
                                }
                            }
                            TableBody {
                                for line in loaded.lines.iter().cloned() {
                                    {
                                        let key = line.id.to_string();
                                        let category = line
                                            .category_id
                                            .and_then(|c| category_names.get(&c).cloned())
                                            .unwrap_or_default();
                                        let quantity = format!("{:.2}", line.quantity_hundredths as f64 / 100.0);
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell { "{line.description}" }
                                                TableCell {
                                                    if category.is_empty() {
                                                        span { class: "text-subtle", "-" }
                                                    } else {
                                                        span { "{category}" }
                                                    }
                                                }
                                                TableCell { class: "text-right", "{quantity}" }
                                                TableCell { class: "text-right", "{format_money_minor(line.unit_price_minor)}" }
                                                TableCell { class: "text-right", "{format_money_minor(line.tax_minor)}" }
                                                TableCell { class: "text-right font-medium", "{format_money_minor(line.total_minor)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        dl { class: "mt-4 space-y-1 text-sm",
                            div { class: "flex justify-between",
                                dt { class: "text-muted", "Subtotal" }
                                dd { "{format_money_minor(bill.subtotal_minor)}" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-muted", "Tax" }
                                dd { "{format_money_minor(bill.tax_minor)}" }
                            }
                            div { class: "flex justify-between font-medium",
                                dt { "Total" }
                                dd { "{format_money_minor(bill.total_minor)}" }
                            }
                            if bill.paid_minor > 0 {
                                div { class: "flex justify-between",
                                    dt { class: "text-muted", "Paid" }
                                    dd { "{format_money_minor(bill.paid_minor)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_path_asks_only_for_a_page() {
        assert_eq!(list_path(1), "/bills?page=1&per_page=25");
    }

    /// The detail response is flattened, and this type has to read it
    /// (MAPPS-1164).
    ///
    /// A fixture in the server's actual shape rather than a round trip
    /// through our own `Serialize`: the bug was a client struct that
    /// compiled against nothing, and a test that serialises what it then
    /// deserialises would have passed just as happily.
    #[test]
    fn the_detail_response_is_read_flattened() {
        let body = serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "vendor_id": "33333333-3333-3333-3333-333333333333",
            "bill_number": "VEN-7781",
            "issue_date": "2026-10-23",
            "due_date": "2026-11-23",
            "currency": "USD",
            "subtotal_minor": 7000,
            "tax_minor": 0,
            "total_minor": 7000,
            "paid_minor": 0,
            "status": "draft",
            "notes": null,
            "external_provider": "xero",
            "external_bill_id": "0b9b3e1a-5c7d-4a2f-8e6b-1d3f5a7c9e11",
            "created_at": "2026-10-23T00:00:00Z",
            "updated_at": "2026-10-23T00:00:00Z",
            "lines": [{
                "id": "44444444-4444-4444-4444-444444444444",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "bill_id": "11111111-1111-1111-1111-111111111111",
                "category_id": null,
                "description": "Rack space",
                "quantity_hundredths": 100,
                "unit_price_minor": 5000,
                "tax_minor": 0,
                "total_minor": 5000,
                "position": 0,
                "created_at": "2026-10-23T00:00:00Z",
            }],
        });
        let loaded: RemoteBillWithLines =
            serde_json::from_value(body).expect("the server's own shape deserialises");
        assert_eq!(loaded.bill.total_minor, 7000);
        assert_eq!(loaded.bill.status, "draft");
        assert_eq!(
            loaded.bill.external_provider.as_deref(),
            Some("xero"),
            "and the delegated pair survives the flatten"
        );
        assert_eq!(loaded.lines.len(), 1);
        assert_eq!(loaded.lines[0].description, "Rack space");
    }

    /// A nested body is NOT what the server sends, so reading one would
    /// mean the flatten had been undone.
    #[test]
    fn a_nested_body_is_not_what_this_type_reads() {
        let nested = serde_json::json!({
            "bill": { "id": "11111111-1111-1111-1111-111111111111" },
            "lines": [],
        });
        assert!(
            serde_json::from_value::<RemoteBillWithLines>(nested).is_err(),
            "a `bill` key is the shape MAPPS-1164 removed"
        );
    }

    /// Money a person typed becomes the minor units the server stores.
    ///
    /// The third-decimal case is the one that matters: accepting it would
    /// store a number the screen never showed.
    #[test]
    fn money_converts_to_minor_units_or_is_refused() {
        assert_eq!(minor_from_str("50"), Some(5000));
        assert_eq!(minor_from_str("50.00"), Some(5000));
        assert_eq!(minor_from_str("50.5"), Some(5050), "one decimal is tenths");
        assert_eq!(minor_from_str(" 1234.56 "), Some(123456));
        assert_eq!(minor_from_str(".5"), Some(50));
        assert_eq!(minor_from_str("50.123"), None, "a third decimal is refused");
        assert_eq!(minor_from_str("fifty"), None);
        assert_eq!(minor_from_str(""), None);
        assert_eq!(
            minor_from_str("-5.00"),
            Some(-500),
            "a credit line is signed"
        );
    }

    /// A quantity is hundredths, so 1.25 is 125 and not 1.
    #[test]
    fn a_quantity_is_hundredths() {
        assert_eq!(hundredths_from_str("1"), Some(100));
        assert_eq!(hundredths_from_str("1.25"), Some(125));
        assert_eq!(hundredths_from_str("2.5"), Some(250));
    }

    /// The four statuses the server's CHECK allows each get their own
    /// badge, and an unknown one renders as itself.
    #[test]
    fn every_status_has_a_badge_and_an_unknown_one_shows_itself() {
        assert_eq!(status_badge("draft").0, "Draft");
        assert_eq!(status_badge("approved").0, "Approved");
        assert!(matches!(status_badge("paid").1, BadgeVariant::Green));
        assert!(matches!(status_badge("void").1, BadgeVariant::Red));
        assert_eq!(
            status_badge("partially_paid").0,
            "partially_paid",
            "a status this build does not know means the server is ahead of it"
        );
    }

    /// The transitions offered are the ones the server accepts.
    #[test]
    fn only_the_transitions_the_server_accepts_are_offered() {
        assert!(can_approve("draft"));
        assert!(!can_approve("approved"), "a second approve is a 409");
        assert!(!can_approve("paid"));
        assert!(can_void("draft"));
        assert!(can_void("approved"));
        assert!(!can_void("paid"), "a paid bill is terminal");
        assert!(!can_void("void"));
    }

    fn one_line() -> Vec<(String, String, String, String, String)> {
        vec![(
            "Rack space".to_string(),
            String::new(),
            "1".to_string(),
            "50.00".to_string(),
            String::new(),
        )]
    }

    #[test]
    fn the_body_matches_create_bill_request() {
        let body = bill_body(
            "11111111-1111-1111-1111-111111111111",
            " VEN-1 ",
            "2026-10-23",
            "",
            "usd",
            "",
            &one_line(),
        )
        .expect("the body builds");
        assert_eq!(body["currency"], "USD", "upper-cased for the server");
        assert_eq!(body["bill_number"], "VEN-1");
        assert!(body["due_date"].is_null(), "blank means they did not say");
        assert!(body["notes"].is_null());
        assert_eq!(body["lines"][0]["quantity_hundredths"], 100);
        assert_eq!(body["lines"][0]["unit_price_minor"], 5000);
        assert_eq!(body["lines"][0]["tax_minor"], 0, "blank tax is none");
        assert_eq!(body["lines"][0]["position"], 0);
        assert!(body["lines"][0]["category_id"].is_null());
    }

    /// Every refusal names what to fix, and none of them reaches the
    /// server.
    #[test]
    fn a_body_that_cannot_be_built_says_which_field() {
        let no_vendor = bill_body("", "", "2026-10-23", "", "USD", "", &one_line()).unwrap_err();
        assert!(no_vendor.contains("vendor"), "{no_vendor}");
        let no_date = bill_body("v", "", "", "", "USD", "", &one_line()).unwrap_err();
        assert!(no_date.contains("date"), "{no_date}");
        let bad_currency = bill_body("v", "", "2026-10-23", "", "US", "", &one_line()).unwrap_err();
        assert!(bad_currency.contains("3-letter"), "{bad_currency}");
        let no_lines = bill_body("v", "", "2026-10-23", "", "USD", "", &[]).unwrap_err();
        assert!(no_lines.contains("at least one line"), "{no_lines}");
        let bad_price = bill_body(
            "v",
            "",
            "2026-10-23",
            "",
            "USD",
            "",
            &[(
                "Rack space".to_string(),
                String::new(),
                "1".to_string(),
                "50.123".to_string(),
                String::new(),
            )],
        )
        .unwrap_err();
        assert!(
            bad_price.contains("Rack space") && bad_price.contains("unit price"),
            "the refusal names the line and the field: {bad_price}"
        );
        let no_description = bill_body(
            "v",
            "",
            "2026-10-23",
            "",
            "USD",
            "",
            &[(
                String::new(),
                String::new(),
                "1".to_string(),
                "50.00".to_string(),
                String::new(),
            )],
        )
        .unwrap_err();
        assert!(no_description.contains("Line 1"), "{no_description}");
    }
}

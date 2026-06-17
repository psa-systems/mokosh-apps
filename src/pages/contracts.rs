//! Contract pages, wired to the mokosh-server `/contracts` and
//! `/rate-cards` endpoints. Patterns (resource + `active_tenant_generation`,
//! loading/empty/error states, `serde`-typed request bodies, the minimal
//! query-string encoder) mirror `src/pages/contacts.rs`.
//!
//! Rate cards and their line items are fully editable here (MAPPS-160),
//! finance-gated to match the server's `RequireFinance` write guard.
//! Contract line items are managed from the contract detail page's Line
//! Items card: add via `POST /contracts/{id}/items`, edit via
//! `PUT /contract-items/{id}`, delete via `DELETE /contract-items/{id}`
//! (MAPPS-196).

use dioxus::prelude::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Select, SelectOption, SettingFormModal, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
};
use crate::modules::contracts::{
    ContractHourBalanceResponse, ContractItemResponse, ContractResponse, CreateContractRequest,
    RateCardItemResponse, RateCardResponse, UpdateContractRequest, UpsertRateCardItemRequest,
    UpsertRateCardRequest,
};
use crate::utils::url::urlencoding_minimal;
use crate::utils::Paginated;
use crate::Route;

/// Rows per page for the contract list (mirrors `contacts.rs` PER_PAGE).
const PER_PAGE: usize = 25;

/// Detail sub-lists (items, hour-balance, rate-card items) are not
/// paginated in the UI; request a large page so a single fetch covers
/// every row a contract or rate card realistically has.
const SUBLIST_PER_PAGE: usize = 100;

/// Contract-type enum tags (`managed_services`, `block_hours`,
/// `time_and_materials`, `fixed_price`, ...) to a title-case label. Also maps
/// line-item types (`recurring_service`, `retainer`, `product`, `one_time`).
/// Unknown tags pass through.
fn humanize_contract_type(raw: &str) -> String {
    match raw {
        "managed_services" => "Managed Services".to_string(),
        "block_hours" => "Block Hours".to_string(),
        "time_and_materials" => "Time and Materials".to_string(),
        "fixed_price" => "Fixed Price".to_string(),
        "warranty" => "Warranty".to_string(),
        "recurring_service" => "Recurring Service".to_string(),
        "retainer" => "Retainer".to_string(),
        "product" => "Product".to_string(),
        "one_time" => "One-time".to_string(),
        other => other.to_string(),
    }
}

fn contract_type_variant(raw: &str) -> BadgeVariant {
    match raw {
        "managed_services" | "recurring_service" => BadgeVariant::Blue,
        "block_hours" => BadgeVariant::Purple,
        "fixed_price" => BadgeVariant::Green,
        "time_and_materials" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    }
}

/// Status tags (`draft`, `active`, `expired`, `cancelled`, ...) to a
/// title-case label.
fn humanize_contract_status(raw: &str) -> String {
    match raw {
        "draft" => "Draft".to_string(),
        "active" => "Active".to_string(),
        "expired" => "Expired".to_string(),
        "cancelled" | "canceled" => "Cancelled".to_string(),
        "pending" => "Pending".to_string(),
        other => other.to_string(),
    }
}

/// Title-case a billing-cycle enum for display (PMS-365). Mirrors the
/// `cycle_options` labels in the contract form so the detail panel does not
/// leak the raw lowercase value (e.g. `monthly`).
fn humanize_billing_cycle(raw: &str) -> String {
    match raw {
        "monthly" => "Monthly".to_string(),
        "quarterly" => "Quarterly".to_string(),
        "annually" | "annual" => "Annually".to_string(),
        "one_time" => "One-time".to_string(),
        "" => "-".to_string(),
        other => other.to_string(),
    }
}

fn status_variant(raw: &str) -> BadgeVariant {
    match raw {
        "active" => BadgeVariant::Green,
        "expired" => BadgeVariant::Red,
        "cancelled" | "canceled" => BadgeVariant::Gray,
        "pending" => BadgeVariant::Yellow,
        "draft" => BadgeVariant::Blue,
        _ => BadgeVariant::Gray,
    }
}

// Money formatting is centralized in `crate::utils::money` (MAPPS-197) so
// projects, billing, and contracts render currency identically.
use crate::utils::money::{format_money, format_money_opt};

/// Company options shared by the create/edit form's company picker. The
/// list endpoint returns `CompanyResponse`; we only need id + name.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct CompanyOption {
    id: uuid::Uuid,
    name: String,
}

// ============================================================================
// LIST
// ============================================================================

/// Contract list page. Fetches `GET /contracts` (paginated) with optional
/// `company_id` / `status` / `contract_type` filters.
#[component]
pub fn ContractListPage() -> Element {
    let mut company_filter = use_signal(String::new);
    let mut status_filter = use_signal(String::new);
    let mut type_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("draft", "Draft"),
        SelectOption::new("active", "Active"),
        SelectOption::new("expired", "Expired"),
        SelectOption::new("cancelled", "Cancelled"),
    ];
    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("managed_services", "Managed Services"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("time_and_materials", "Time and Materials"),
        SelectOption::new("fixed_price", "Fixed Price"),
        SelectOption::new("warranty", "Warranty"),
    ];

    // Company filter is a dropdown populated from the companies endpoint.
    let companies_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>(
            "/contacts/companies?page=1&per_page=100&sort=name&sort_dir=asc",
        )
        .await
        .ok()
    });
    let company_options = {
        let mut opts = vec![SelectOption::new("", "All Companies")];
        if let Some(Some(resp)) = &*companies_resource.read_unchecked() {
            for c in &resp.data {
                opts.push(SelectOption::new(c.id.to_string(), c.name.clone()));
            }
        }
        opts
    };

    let company_text = company_filter.read().clone();
    let status_text = status_filter.read().clone();
    let type_text = type_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let company_for_resource = company_text.clone();
    let status_for_resource = status_text.clone();
    let type_for_resource = type_text.clone();
    let contracts_resource = use_resource(move || {
        let company = company_for_resource.clone();
        let status = status_for_resource.clone();
        let contract_type = type_for_resource.clone();
        async move {
            // F1: subscribe to the active-tenant generation so an org
            // switch re-runs this resource.
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/contracts?page={current_page}&per_page={PER_PAGE}");
            if !company.is_empty() {
                path.push_str(&format!("&company_id={}", urlencoding_minimal(&company)));
            }
            if !status.is_empty() {
                path.push_str(&format!("&status={}", urlencoding_minimal(&status)));
            }
            if !contract_type.is_empty() {
                path.push_str(&format!(
                    "&contract_type={}",
                    urlencoding_minimal(&contract_type)
                ));
            }
            crate::hooks::fetch::api::get_with_auth::<Paginated<ContractResponse>>(&path, &token)
                .await
                .ok()
        }
    });

    let resource_snapshot = contracts_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (page_rows, total): (Vec<ContractResponse>, u64) = match &*resource_snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !company_text.is_empty() || !status_text.is_empty() || !type_text.is_empty();

    rsx! {
        AppLayout { title: "Contracts",
            PageHeader {
                title: "Contracts",
                subtitle: "Manage customer contracts and agreements",
                actions: rsx! {
                    Link {
                        to: Route::RateCardList {},
                        Button { variant: ButtonVariant::Secondary, "Rate Cards" }
                    }
                    Link {
                        to: Route::ContractNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Contract"
                        }
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        Select {
                            name: "company",
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
                    Select {
                        name: "type",
                        options: type_options,
                        value: type_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            type_filter.set(e.value());
                            page.set(1);
                        },
                    }
                }
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load contracts. Refresh the page to retry."
                }
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
                            TableHeader { "Contract" }
                            TableHeader { "Type" }
                            TableHeader { "Value" }
                            TableHeader { "Start" }
                            TableHeader { "Expires" }
                            TableHeader { "Status" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 6, rows: 5 }
                    } else if page_rows.is_empty() {
                        if has_filters {
                            TableEmpty {
                                columns: 6,
                                message: "No contracts match your filters.".to_string(),
                            }
                        } else {
                            TableEmpty {
                                columns: 6,
                                title: "No contracts yet".to_string(),
                                description: "Create your first contract to track agreements and billing."
                                    .to_string(),
                                actions: rsx! {
                                    Link {
                                        to: Route::ContractNew {},
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                            "New Contract"
                                        }
                                    }
                                },
                            }
                        }
                    } else {
                        TableBody {
                            for contract in page_rows.iter().cloned() {
                                ContractRow {
                                    key: "{contract.id}",
                                    id: contract.id.to_string(),
                                    name: contract.name.clone(),
                                    contract_type: contract.contract_type.clone(),
                                    value: format_money_opt(contract.billing_amount),
                                    start: contract.start_date.format("%b %-d, %Y").to_string(),
                                    expires: contract
                                        .end_date
                                        .map(|d| d.format("%b %-d, %Y").to_string())
                                        .unwrap_or_else(|| "Ongoing".to_string()),
                                    status: contract.status.clone(),
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
struct ContractRowProps {
    id: String,
    name: String,
    contract_type: String,
    value: String,
    start: String,
    expires: String,
    status: String,
}

#[component]
fn ContractRow(props: ContractRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::ContractDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::ContractDetail { id: props.id.clone() },
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.name}"
                }
            }
            TableCell {
                Badge {
                    variant: contract_type_variant(&props.contract_type),
                    "{humanize_contract_type(&props.contract_type)}"
                }
            }
            TableCell { class: "font-medium", "{props.value}" }
            TableCell { "{props.start}" }
            TableCell { "{props.expires}" }
            TableCell {
                Badge { variant: status_variant(&props.status), "{humanize_contract_status(&props.status)}" }
            }
        }
    }
}

// ============================================================================
// NEW / EDIT
// ============================================================================

/// New contract page.
#[component]
pub fn ContractNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Contract",
            PageHeader { title: "New Contract", subtitle: "Create a new contract" }
            ContractForm { mode: ContractFormMode::Create, initial: ContractFormValues::default() }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ContractEditPageProps {
    pub id: String,
}

#[component]
pub fn ContractEditPage(props: ContractEditPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let id_for_form = props.id.clone();
    let detail_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<ContractResponse>(&format!("/contracts/{id}"))
                .await
                .ok()
        }
    });
    let snap = detail_resource.read_unchecked();
    rsx! {
        AppLayout { title: "Edit Contract",
            PageHeader { title: "Edit Contract" }
            match &*snap {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contract." }
                            Link {
                                to: Route::ContractList {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to contracts"
                            }
                        }
                    }
                },
                Some(Some(c)) => {
                    let initial = ContractFormValues {
                        name: c.name.clone(),
                        company_id: c.company_id.to_string(),
                        contract_type: c.contract_type.clone(),
                        status: if c.status.is_empty() { "draft".to_string() } else { c.status.clone() },
                        billing_cycle: if c.billing_cycle.is_empty() {
                            "monthly".to_string()
                        } else {
                            c.billing_cycle.clone()
                        },
                        billing_amount: c.billing_amount.map(|d| d.to_string()).unwrap_or_default(),
                        start_date: c.start_date.format("%Y-%m-%d").to_string(),
                        end_date: c
                            .end_date
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                        auto_renew: c.auto_renew,
                        contract_number: c.contract_number.clone().unwrap_or_default(),
                        notes: c.notes.clone().unwrap_or_default(),
                        items: Vec::new(),
                    };
                    let id = id_for_form.clone();
                    rsx! {
                        ContractForm { mode: ContractFormMode::Edit { id }, initial }
                    }
                },
            }
        }
    }
}

// Field caps for the contract form's free-text inputs (MAPPS-211). These
// mirror the mokosh-server column limits so over-long input is rejected
// inline (and via `maxlength`) instead of failing later as an opaque 422.
const CONTRACT_NAME_MAX: usize = 200;
const CONTRACT_NUMBER_MAX: usize = 100;
const CONTRACT_NOTES_MAX: usize = 2000;
const ITEM_NAME_MAX: usize = 200;

/// Upper bound for money/quantity fields (MAPPS-211). Comfortably inside
/// `Decimal`'s range while ruling out absurd magnitudes (e.g. a ~50-digit
/// paste), so such input is caught with a clear "out of range" message
/// rather than the misleading "not a number" the bare parse produced.
const VALUE_MAX: i64 = 10_000_000_000;

/// Validate a required, length-capped text field (MAPPS-211). Returns the
/// trimmed value or an inline message for that field.
fn validate_text_required(raw: &str, label: &str, max: usize) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(format!("{label} is required."));
    }
    if t.chars().count() > max {
        return Err(format!("{label} must be {max} characters or fewer."));
    }
    Ok(t.to_string())
}

/// Validate an optional, length-capped text field (MAPPS-211). Blank ->
/// `Ok(None)`; otherwise the trimmed value or an inline message.
fn validate_text_optional(raw: &str, label: &str, max: usize) -> Result<Option<String>, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.chars().count() > max {
        return Err(format!("{label} must be {max} characters or fewer."));
    }
    Ok(Some(t.to_string()))
}

/// Classify a string that failed to parse as a `Decimal`: numeric-looking but
/// out of range (e.g. a ~50-digit value beyond `Decimal`'s capacity) vs.
/// genuinely non-numeric. Lets the money/quantity validators give a precise
/// message instead of the old misleading "must be a number" (MAPPS-211).
fn decimal_parse_error(raw: &str, label: &str) -> String {
    if raw.parse::<f64>().map(|f| f.is_finite()).unwrap_or(false) {
        format!("{label} is out of range.")
    } else {
        format!("{label} must be a number.")
    }
}

/// Validate an optional money amount (MAPPS-211). Blank -> `Ok(None)`.
/// Rejects negatives, more than two decimal places, and out-of-range values,
/// distinguishing a non-numeric entry from one that is numeric but too large.
fn validate_money(raw: &str, label: &str) -> Result<Option<Decimal>, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    match s.parse::<Decimal>() {
        Ok(d) => {
            if d < Decimal::ZERO {
                return Err(format!("{label} must not be negative."));
            }
            if d.scale() > 2 {
                return Err(format!("{label} must have at most 2 decimal places."));
            }
            if d > Decimal::from(VALUE_MAX) {
                return Err(format!("{label} is out of range."));
            }
            Ok(Some(d))
        }
        Err(_) => Err(decimal_parse_error(s, label)),
    }
}

/// Validate a required quantity (MAPPS-211): present, non-negative, in range.
/// Allows fractional quantities (the server column is `Decimal`).
fn validate_quantity(raw: &str, label: &str) -> Result<Decimal, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("{label} is required."));
    }
    match s.parse::<Decimal>() {
        Ok(d) => {
            if d < Decimal::ZERO {
                return Err(format!("{label} must not be negative."));
            }
            if d > Decimal::from(VALUE_MAX) {
                return Err(format!("{label} is out of range."));
            }
            Ok(d)
        }
        Err(_) => Err(decimal_parse_error(s, label)),
    }
}

/// One editable line item in the create form. Mirrors the fields the
/// server's `UpsertContractItemRequest` requires; decimals are held as
/// strings while editing and parsed on submit. The `*_err` fields hold the
/// inline validation message for each input (MAPPS-211).
#[derive(Clone, Debug, PartialEq, Default)]
struct ItemFormValues {
    name: String,
    item_type: String,
    quantity: String,
    unit_price: String,
    included_hours: String,
    name_err: String,
    qty_err: String,
    price_err: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
struct ContractFormValues {
    name: String,
    company_id: String,
    contract_type: String,
    status: String,
    billing_cycle: String,
    billing_amount: String,
    start_date: String,
    end_date: String,
    auto_renew: bool,
    contract_number: String,
    notes: String,
    /// Initial line items, entered only on the create flow. After a
    /// contract exists, its line items are added, edited, and removed from
    /// the detail page's Line Items card (MAPPS-196), so the edit flow
    /// leaves this empty.
    items: Vec<ItemFormValues>,
}

#[derive(Clone, Debug, PartialEq)]
enum ContractFormMode {
    Create,
    Edit { id: String },
}

#[derive(Props, Clone, PartialEq)]
struct ContractFormProps {
    mode: ContractFormMode,
    initial: ContractFormValues,
}

#[component]
fn ContractForm(props: ContractFormProps) -> Element {
    let initial = props.initial.clone();
    let mode = props.mode.clone();
    let is_edit = matches!(mode, ContractFormMode::Edit { .. });

    let mut name = use_signal(|| initial.name.clone());
    let mut company_id = use_signal(|| initial.company_id.clone());
    // PMS-352 AC3: the create flow uses CompanyPicker (with inline create) so a
    // tenant with zero companies isn't dead-ended; the picker reports the
    // chosen company's display name back here. Edit keeps the disabled Select
    // (company is immutable post-create), so this stays empty there.
    let mut company_name = use_signal(String::new);
    let mut contract_type = use_signal(|| {
        if initial.contract_type.is_empty() {
            "managed_services".to_string()
        } else {
            initial.contract_type.clone()
        }
    });
    let mut status = use_signal(|| {
        if initial.status.is_empty() {
            "draft".to_string()
        } else {
            initial.status.clone()
        }
    });
    let mut billing_cycle = use_signal(|| {
        if initial.billing_cycle.is_empty() {
            "monthly".to_string()
        } else {
            initial.billing_cycle.clone()
        }
    });
    let mut billing_amount = use_signal(|| initial.billing_amount.clone());
    let mut start_date = use_signal(|| initial.start_date.clone());
    let mut end_date = use_signal(|| initial.end_date.clone());
    let mut auto_renew = use_signal(|| initial.auto_renew);
    let mut contract_number = use_signal(|| initial.contract_number.clone());
    let mut notes = use_signal(|| initial.notes.clone());
    let mut items = use_signal(|| initial.items.clone());
    let mut is_submitting = use_signal(|| false);
    // Server/submit-time errors only (e.g. a failed POST). Field validation
    // no longer routes through this banner (MAPPS-211).
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-211): each failure is shown
    // under its own field and highlights that field, rather than a single
    // generic top banner.
    let mut name_err = use_signal(String::new);
    let mut company_err = use_signal(String::new);
    let mut billing_err = use_signal(String::new);
    let mut start_err = use_signal(String::new);
    let mut end_err = use_signal(String::new);
    let mut number_err = use_signal(String::new);
    let mut notes_err = use_signal(String::new);

    // Company dropdown options (create flow needs to pick a company; the
    // server forbids changing it on update so the edit flow disables it).
    let companies_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>(
            "/contacts/companies?page=1&per_page=100&sort=name&sort_dir=asc",
        )
        .await
        .ok()
    });
    let company_options = {
        let mut opts = vec![SelectOption::new("", "Select a company")];
        if let Some(Some(resp)) = &*companies_resource.read_unchecked() {
            for c in &resp.data {
                opts.push(SelectOption::new(c.id.to_string(), c.name.clone()));
            }
        }
        opts
    };

    let type_options = vec![
        SelectOption::new("managed_services", "Managed Services"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("time_and_materials", "Time and Materials"),
        SelectOption::new("fixed_price", "Fixed Price"),
        SelectOption::new("warranty", "Warranty"),
    ];
    let status_options = vec![
        SelectOption::new("draft", "Draft"),
        SelectOption::new("active", "Active"),
        SelectOption::new("expired", "Expired"),
        SelectOption::new("cancelled", "Cancelled"),
    ];
    let cycle_options = vec![
        SelectOption::new("monthly", "Monthly"),
        SelectOption::new("quarterly", "Quarterly"),
        SelectOption::new("annually", "Annually"),
        SelectOption::new("one_time", "One-time"),
    ];
    let item_type_options = contract_item_type_options();

    let navigator = use_navigator();
    let submit_label = if is_edit {
        "Save Changes"
    } else {
        "Create Contract"
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        if *is_submitting.read() {
            return;
        }
        error.set(String::new());
        name_err.set(String::new());
        company_err.set(String::new());
        billing_err.set(String::new());
        start_err.set(String::new());
        end_err.set(String::new());
        number_err.set(String::new());
        notes_err.set(String::new());

        // Collect every field error before returning (MAPPS-211) so the user
        // sees all problems at once, each attached to its own field, rather
        // than the form aborting on the first failure.
        let mut ok = true;

        // Name (required, length-capped).
        let name_val = match validate_text_required(&name.read(), "Name", CONTRACT_NAME_MAX) {
            Ok(v) => v,
            Err(msg) => {
                name_err.set(msg);
                ok = false;
                String::new()
            }
        };

        // Company is required on create; the edit flow disables the field and
        // preserves the existing company, so it isn't re-validated there.
        let company_uuid = if is_edit {
            None
        } else {
            match uuid::Uuid::parse_str(company_id.read().trim()) {
                Ok(u) => Some(u),
                Err(_) => {
                    company_err.set("Please pick a company.".to_string());
                    ok = false;
                    None
                }
            }
        };

        // Value (USD): optional money, non-negative, in range, <= 2 decimals.
        let billing = match validate_money(&billing_amount.read(), "Value") {
            Ok(v) => v,
            Err(msg) => {
                billing_err.set(msg);
                ok = false;
                None
            }
        };

        // Start date (required).
        let start = {
            let raw = start_date.read().trim().to_string();
            if raw.is_empty() {
                start_err.set("Start date is required.".to_string());
                ok = false;
                None
            } else {
                match chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
                    Ok(d) => Some(d),
                    Err(_) => {
                        start_err.set("Start date is invalid.".to_string());
                        ok = false;
                        None
                    }
                }
            }
        };

        // End date (optional).
        let end = {
            let raw = end_date.read().trim().to_string();
            if raw.is_empty() {
                None
            } else {
                match chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
                    Ok(d) => Some(d),
                    Err(_) => {
                        end_err.set("End date is invalid.".to_string());
                        ok = false;
                        None
                    }
                }
            }
        };

        // Cross-field: end must be on or after start.
        if let (Some(s), Some(e)) = (start, end) {
            if e < s {
                end_err.set("End date must be on or after the start date.".to_string());
                ok = false;
            }
        }

        // Contract number / notes (optional, length-capped).
        let number_val = match validate_text_optional(
            &contract_number.read(),
            "Contract number",
            CONTRACT_NUMBER_MAX,
        ) {
            Ok(v) => v,
            Err(msg) => {
                number_err.set(msg);
                ok = false;
                None
            }
        };
        let notes_val = match validate_text_optional(&notes.read(), "Notes", CONTRACT_NOTES_MAX) {
            Ok(v) => v,
            Err(msg) => {
                notes_err.set(msg);
                ok = false;
                None
            }
        };

        // Line items (create flow only): validate each populated row and
        // attach errors to its own fields. Blank rows are skipped on submit
        // (see `create_items`), so they aren't validated.
        if !is_edit {
            let mut next_items = items.read().clone();
            for item in next_items.iter_mut() {
                item.name_err.clear();
                item.qty_err.clear();
                item.price_err.clear();
                if item.name.trim().is_empty() {
                    continue;
                }
                if item.name.chars().count() > ITEM_NAME_MAX {
                    item.name_err = format!("Name must be {ITEM_NAME_MAX} characters or fewer.");
                    ok = false;
                }
                if let Err(msg) = validate_quantity(&item.quantity, "Qty") {
                    item.qty_err = msg;
                    ok = false;
                }
                if let Err(msg) = validate_money(&item.unit_price, "Unit price") {
                    item.price_err = msg;
                    ok = false;
                }
            }
            items.set(next_items);
        }

        if !ok {
            return;
        }

        // Validated above: when `ok` holds, the required start date parsed.
        let start = start.expect("start date validated above");

        is_submitting.set(true);
        let mode = mode.clone();
        let type_val = contract_type.read().clone();
        let status_val = status.read().clone();
        let cycle_val = billing_cycle.read().clone();
        let items_snapshot = items.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match &mode {
                    ContractFormMode::Create => {
                        // Validated before the spawn (MAPPS-211); `company_uuid`
                        // is `Some` whenever we reach the create branch.
                        let company_uuid =
                            company_uuid.expect("company validated above for the create flow");
                        let body = CreateContractRequest {
                            contract_number: number_val.clone(),
                            name: name_val.clone(),
                            company_id: company_uuid,
                            contract_type: type_val.clone(),
                            status: status_val.clone(),
                            start_date: start,
                            end_date: end,
                            auto_renew: *auto_renew.read(),
                            billing_cycle: cycle_val.clone(),
                            billing_amount: billing,
                            sla_id: None,
                            signed_date: None,
                            signed_by_contact_id: None,
                            notes: notes_val.clone(),
                        };
                        match crate::hooks::fetch::api::post_authed_typed::<ContractResponse, _>(
                            "/contracts",
                            &body,
                        )
                        .await
                        {
                            Ok(created) => {
                                // Persist any line items entered on the
                                // create form via the nested items
                                // endpoint. A failed item does not unwind
                                // the contract; surface it and still land
                                // on the detail page so the user can fix
                                // it there.
                                let new_id = created.id.to_string();
                                if let Err(err) = create_items(&new_id, &items_snapshot).await {
                                    error.set(format!("Contract saved, but an item failed: {err}"));
                                }
                                Ok(new_id)
                            }
                            Err(err) => Err(err),
                        }
                    }
                    ContractFormMode::Edit { id } => {
                        let body = UpdateContractRequest {
                            contract_number: number_val.clone(),
                            name: Some(name_val.clone()),
                            status: Some(status_val.clone()),
                            end_date: end,
                            auto_renew: Some(*auto_renew.read()),
                            billing_cycle: Some(cycle_val.clone()),
                            billing_amount: billing,
                            sla_id: None,
                            signed_date: None,
                            signed_by_contact_id: None,
                            notes: notes_val.clone(),
                        };
                        let path = format!("/contracts/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<ContractResponse, _>(
                            &path, &body,
                        )
                        .await
                        .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        navigator.push(Route::ContractDetail { id });
                    }
                    Err(err) => {
                        // PMS-364: the end < start cross-field rule comes back
                        // keyed on `end_date`; show it inline under that field.
                        // Server errors without a field target fall back to the
                        // top-of-form banner.
                        if let Some(msg) = err.field_message("end_date") {
                            end_err.set(msg);
                        } else {
                            error.set(err.user_message());
                        }
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    // PMS-352 AC3: feed the create-flow CompanyPicker its "already selected"
    // state. Some(id) renders the selected-chip view; a non-UUID (empty)
    // renders the search dropdown.
    let company_picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };

    rsx! {
        Card {
            form {
                class: "space-y-6",
                onsubmit: handle_submit,

                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "name",
                        label: "Name",
                        placeholder: "e.g. Managed Services Agreement",
                        required: true,
                        maxlength: CONTRACT_NAME_MAX as i64,
                        value: name.read().clone(),
                        error: name_err(),
                        oninput: move |e: FormEvent| name.set(e.value()),
                    }
                    // PMS-352 AC3: on create, use CompanyPicker so a tenant with
                    // no companies can create one inline instead of dead-ending.
                    // On edit the company is immutable, so keep the disabled
                    // Select that shows the existing company by name.
                    if is_edit {
                        Select {
                            name: "company",
                            label: "Company",
                            options: company_options,
                            value: company_id.read().clone(),
                            required: true,
                            disabled: true,
                            error: company_err(),
                            onchange: move |e: FormEvent| company_id.set(e.value()),
                        }
                    } else {
                        div { class: "space-y-1",
                            crate::components::CompanyPicker {
                                value: company_name.read().clone(),
                                selected_id: company_picker_selected_id,
                                required: true,
                                allow_inline_create: true,
                                onselect: move |(id, name): (String, String)| {
                                    company_id.set(id);
                                    company_name.set(name);
                                    company_err.set(String::new());
                                },
                                onclear: move |_| {
                                    company_id.set(String::new());
                                    company_name.set(String::new());
                                },
                            }
                            if !company_err().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{company_err}" }
                            }
                        }
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "type",
                        label: "Type",
                        options: type_options,
                        value: contract_type.read().clone(),
                        disabled: is_edit,
                        onchange: move |e: FormEvent| contract_type.set(e.value()),
                    }
                    Select {
                        name: "status",
                        label: "Status",
                        options: status_options,
                        value: status.read().clone(),
                        onchange: move |e: FormEvent| status.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "billing_cycle",
                        label: "Billing Cycle",
                        options: cycle_options,
                        value: billing_cycle.read().clone(),
                        onchange: move |e: FormEvent| billing_cycle.set(e.value()),
                    }
                    crate::components::Input {
                        name: "billing_amount",
                        label: "Value (USD)",
                        r#type: "number",
                        placeholder: "0.00",
                        min: "0".to_string(),
                        max: VALUE_MAX.to_string(),
                        step: "0.01".to_string(),
                        value: billing_amount.read().clone(),
                        error: billing_err(),
                        oninput: move |e: FormEvent| billing_amount.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "start_date",
                        label: "Start Date",
                        required: true,
                        value: start_date.read().clone(),
                        error: start_err(),
                        oninput: move |e: FormEvent| start_date.set(e.value()),
                    }
                    crate::components::DateField {
                        name: "end_date",
                        label: "End Date",
                        value: end_date.read().clone(),
                        error: end_err(),
                        oninput: move |e: FormEvent| end_date.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "contract_number",
                        label: "Contract Number",
                        placeholder: "Optional reference",
                        maxlength: CONTRACT_NUMBER_MAX as i64,
                        value: contract_number.read().clone(),
                        error: number_err(),
                        oninput: move |e: FormEvent| contract_number.set(e.value()),
                    }
                }

                crate::components::Checkbox {
                    name: "auto_renew",
                    label: "Auto-renew",
                    checked: *auto_renew.read(),
                    help: "Automatically renew this contract at the end date.",
                    onchange: move |_| {
                        let next = !*auto_renew.read();
                        auto_renew.set(next);
                    },
                }

                crate::components::Textarea {
                    name: "notes",
                    label: "Notes",
                    rows: 3,
                    maxlength: CONTRACT_NOTES_MAX as i64,
                    value: notes.read().clone(),
                    error: notes_err(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }

                // Line items (create flow only). On edit, items are
                // managed from the contract detail page so we don't show
                // the editor here (the server has no bulk-replace route).
                if !is_edit {
                    div { class: "border-t border-line pt-6",
                        div { class: "flex items-center justify-between mb-3",
                            h3 { class: "text-sm font-medium text-content", "Line Items" }
                            Button {
                                variant: ButtonVariant::Secondary,
                                r#type: "button",
                                onclick: move |_| {
                                    let mut next = items.read().clone();
                                    next.push(ItemFormValues {
                                        item_type: "recurring_service".to_string(),
                                        quantity: "1".to_string(),
                                        ..ItemFormValues::default()
                                    });
                                    items.set(next);
                                },
                                "Add Item"
                            }
                        }
                        if items.read().is_empty() {
                            p { class: "text-sm text-muted", "No line items. Click Add Item to include one." }
                        } else {
                            div { class: "space-y-4",
                                for (idx, item) in items.read().clone().into_iter().enumerate() {
                                    div {
                                        key: "{idx}",
                                        class: "grid grid-cols-1 gap-3 sm:grid-cols-6 items-end border border-line rounded-md p-3",
                                        div { class: "sm:col-span-2",
                                            crate::components::Input {
                                                name: "item_name_{idx}",
                                                label: "Name",
                                                maxlength: ITEM_NAME_MAX as i64,
                                                value: item.name.clone(),
                                                error: item.name_err.clone(),
                                                oninput: move |e: FormEvent| {
                                                    let mut next = items.read().clone();
                                                    next[idx].name = e.value();
                                                    items.set(next);
                                                },
                                            }
                                        }
                                        Select {
                                            name: "item_type_{idx}",
                                            label: "Type",
                                            options: item_type_options.clone(),
                                            value: item.item_type.clone(),
                                            onchange: move |e: FormEvent| {
                                                let mut next = items.read().clone();
                                                next[idx].item_type = e.value();
                                                items.set(next);
                                            },
                                        }
                                        crate::components::Input {
                                            name: "item_qty_{idx}",
                                            label: "Qty",
                                            r#type: "number",
                                            min: "0".to_string(),
                                            max: VALUE_MAX.to_string(),
                                            step: "0.01".to_string(),
                                            value: item.quantity.clone(),
                                            error: item.qty_err.clone(),
                                            oninput: move |e: FormEvent| {
                                                let mut next = items.read().clone();
                                                next[idx].quantity = e.value();
                                                items.set(next);
                                            },
                                        }
                                        crate::components::Input {
                                            name: "item_price_{idx}",
                                            label: "Unit Price",
                                            r#type: "number",
                                            min: "0".to_string(),
                                            max: VALUE_MAX.to_string(),
                                            step: "0.01".to_string(),
                                            value: item.unit_price.clone(),
                                            error: item.price_err.clone(),
                                            oninput: move |e: FormEvent| {
                                                let mut next = items.read().clone();
                                                next[idx].unit_price = e.value();
                                                items.set(next);
                                            },
                                        }
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            r#type: "button",
                                            onclick: move |_| {
                                                let mut next = items.read().clone();
                                                next.remove(idx);
                                                items.set(next);
                                            },
                                            "Remove"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: Route::ContractList {},
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        "{submit_label}"
                    }
                }
            }
        }
    }
}

/// POST each entered line item to `/contracts/{id}/items`. Blank rows
/// (no name) are skipped. Returns the first error encountered.
#[cfg(feature = "web")]
async fn create_items(contract_id: &str, items: &[ItemFormValues]) -> Result<(), String> {
    use crate::modules::contracts::UpsertContractItemRequest;
    for (idx, item) in items.iter().enumerate() {
        let name = item.name.trim();
        if name.is_empty() {
            continue;
        }
        let quantity = item
            .quantity
            .trim()
            .parse::<Decimal>()
            .map_err(|_| format!("Item {} quantity is not a number", idx + 1))?;
        let unit_price = item
            .unit_price
            .trim()
            .parse::<Decimal>()
            .unwrap_or(Decimal::ZERO);
        let included_hours = {
            let raw = item.included_hours.trim();
            if raw.is_empty() {
                None
            } else {
                raw.parse::<Decimal>().ok()
            }
        };
        let body = UpsertContractItemRequest {
            name: name.to_string(),
            description: None,
            item_type: if item.item_type.is_empty() {
                "recurring_service".to_string()
            } else {
                item.item_type.clone()
            },
            quantity,
            unit_price,
            billing_frequency: "monthly".to_string(),
            work_type_id: None,
            included_hours,
            overage_rate: None,
            rollover_enabled: false,
            max_rollover_hours: None,
            sort_order: idx as i32,
        };
        let path = format!("/contracts/{contract_id}/items");
        crate::hooks::fetch::api::post_authed_typed::<ContractItemResponse, _>(&path, &body)
            .await
            .map_err(|e| e.user_message())?;
    }
    Ok(())
}

// ============================================================================
// DETAIL
// ============================================================================

/// Contract detail page. Loads the contract, its line items, and its
/// current hour-balance periods.
#[derive(Props, Clone, PartialEq)]
pub struct ContractDetailPageProps {
    pub id: String,
}

#[component]
pub fn ContractDetailPage(props: ContractDetailPageProps) -> Element {
    let id_str = props.id.clone();
    let id_for_contract = id_str.clone();
    let id_for_items = id_str.clone();
    let id_for_balance = id_str.clone();
    let id_for_edit = id_str.clone();
    let id_for_delete = id_str.clone();
    let id_for_item_modal = id_str.clone();

    let mut contract_resource = use_resource(move || {
        let id = id_for_contract.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<ContractResponse>(&format!("/contracts/{id}"))
                .await
                .ok()
        }
    });
    let mut items_resource = use_resource(move || {
        let id = id_for_items.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<ContractItemResponse>>(&format!(
                "/contracts/{id}/items?page=1&per_page={SUBLIST_PER_PAGE}"
            ))
            .await
            .ok()
        }
    });
    let balance_resource = use_resource(move || {
        let id = id_for_balance.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<ContractHourBalanceResponse>>(
                &format!("/contracts/{id}/hour-balance?page=1&per_page={SUBLIST_PER_PAGE}"),
            )
            .await
            .ok()
        }
    });

    let contract_snapshot = contract_resource.read_unchecked();
    let header_title = match &*contract_snapshot {
        Some(Some(c)) => c.name.clone(),
        _ => "Contract".to_string(),
    };

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let mut editing_item = use_signal(|| None::<ContractItemFormState>);
    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // actual DELETE fires from `on_confirm_delete` when confirmed.
    let mut confirming_delete = use_signal(|| false);
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();

    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contracts/{id}");
                if crate::hooks::fetch::api::delete_authed_typed(&path)
                    .await
                    .is_ok()
                {
                    navigator.push(Route::ContractList {});
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    rsx! {
        AppLayout { title: "{header_title}",
            crate::components::ConfirmDialog {
                open: confirming_delete(),
                title: "Delete contract".to_string(),
                message: "Delete this contract? This cannot be undone.".to_string(),
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
            PageHeader {
                title: "{header_title}",
                breadcrumbs: rsx! {
                    crate::components::Breadcrumbs {
                        items: vec![
                            crate::components::BreadcrumbItem {
                                label: "Contracts".to_string(),
                                route: Some(Route::ContractList {}),
                            },
                            crate::components::BreadcrumbItem {
                                label: header_title.clone(),
                                route: None,
                            },
                        ],
                    }
                },
                actions: rsx! {
                    Link {
                        to: Route::ContractEdit { id: edit_id },
                        Button { variant: ButtonVariant::Secondary, "Edit" }
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *deleting.read(),
                        onclick: move |_| {
                            if !*deleting.read() {
                                confirming_delete.set(true);
                            }
                        },
                        "Delete"
                    }
                },
            }

            match &*contract_snapshot {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contract." }
                            Link {
                                to: Route::ContractList {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to contracts"
                            }
                        }
                    }
                },
                Some(Some(contract)) => {
                    let type_label = humanize_contract_type(&contract.contract_type);
                    let status_label = humanize_contract_status(&contract.status);
                    // PMS-365: title-case the enum instead of leaking raw `monthly`.
                    let billing_cycle = humanize_billing_cycle(&contract.billing_cycle);
                    let billing_amount = format_money_opt(contract.billing_amount);
                    let start = contract.start_date.format("%b %-d, %Y").to_string();
                    let end = contract
                        .end_date
                        .map(|d| d.format("%b %-d, %Y").to_string())
                        .unwrap_or_else(|| "Ongoing".to_string());
                    let auto_renew = if contract.auto_renew { "Yes" } else { "No" };
                    let number = contract.contract_number.clone().unwrap_or_default();
                    let notes = contract.notes.clone().unwrap_or_default();
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            div { class: "lg:col-span-2 space-y-6",
                                Card { title: "Contract Details",
                                    dl { class: "grid grid-cols-2 gap-4",
                                        div {
                                            dt { class: "text-sm text-muted", "Contract Type" }
                                            dd { class: "mt-1 font-medium", "{type_label}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-muted", "Billing Cycle" }
                                            dd { class: "mt-1", "{billing_cycle}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-muted", "Start Date" }
                                            dd { class: "mt-1", "{start}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-muted", "End Date" }
                                            dd { class: "mt-1", "{end}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-muted", "Auto-Renewal" }
                                            dd { class: "mt-1", "{auto_renew}" }
                                        }
                                        if !number.is_empty() {
                                            div {
                                                dt { class: "text-sm text-muted", "Contract Number" }
                                                dd { class: "mt-1", "{number}" }
                                            }
                                        }
                                    }
                                    if !notes.is_empty() {
                                        div { class: "mt-4",
                                            dt { class: "text-sm text-muted mb-1", "Notes" }
                                            dd { class: "text-sm whitespace-pre-line", "{notes}" }
                                        }
                                    }
                                }

                                ContractItemsCard { items_resource, editing_item }
                                ContractHourBalanceCard { balance_resource }
                            }

                            div { class: "space-y-6",
                                Card { title: "Summary",
                                    dl { class: "space-y-4",
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-muted", "Status" }
                                            dd { Badge { variant: status_variant(&contract.status), "{status_label}" } }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-muted", "Value" }
                                            dd { class: "font-medium text-lg", "{billing_amount}" }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-muted", "Company" }
                                            dd {
                                                Link {
                                                    to: Route::CompanyDetail { id: contract.company_id.to_string() },
                                                    class: "text-sm text-accent hover:opacity-90",
                                                    "View company"
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

            if let Some(state) = editing_item.read().clone() {
                ContractItemFormModal {
                    state,
                    contract_id: id_for_item_modal.clone(),
                    onclose: move |_| editing_item.set(None),
                    onsaved: move |_| {
                        editing_item.set(None);
                        // Refresh the items table and the contract itself so
                        // per-row totals and the Summary "Value" reflect the
                        // change (the server derives Value from items).
                        items_resource.restart();
                        contract_resource.restart();
                    },
                }
            }
        }
    }
}

#[component]
fn ContractItemsCard(
    items_resource: Resource<Option<Paginated<ContractItemResponse>>>,
    editing_item: Signal<Option<ContractItemFormState>>,
) -> Element {
    let mut editing_item = editing_item;
    let snap = items_resource.read_unchecked();
    // Newly added items sort after the existing rows.
    let next_sort_order = match &*snap {
        Some(Some(resp)) => resp.data.len() as i32,
        _ => 0,
    };
    rsx! {
        Card {
            title: "Line Items",
            padding: false,
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: move |_| editing_item.set(Some(ContractItemFormState::new(next_sort_order))),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "Add Item"
                }
            },
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Item" }
                        TableHeader { "Type" }
                        TableHeader { "Qty" }
                        TableHeader { "Unit Price" }
                        TableHeader { "Total" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 5, rows: 3 } },
                    Some(None) => rsx! {
                        TableEmpty { columns: 5, message: "Could not load line items.".to_string() }
                    },
                    Some(Some(resp)) if resp.data.is_empty() => rsx! {
                        TableEmpty { columns: 5, message: "No line items on this contract yet.".to_string() }
                    },
                    Some(Some(resp)) => {
                        let rows = resp.data.clone();
                        rsx! {
                            TableBody {
                                for item in rows.into_iter() {
                                    {
                                        let key = item.id.to_string();
                                        let qty = item.quantity.normalize().to_string();
                                        let unit = format_money(item.unit_price);
                                        let total = format_money(item.total_price);
                                        let type_label = humanize_contract_type(&item.item_type);
                                        let edit_state = ContractItemFormState::from_existing(&item);
                                        rsx! {
                                            TableRow {
                                                key: "{key}",
                                                clickable: true,
                                                onclick: move |_| editing_item.set(Some(edit_state.clone())),
                                                TableCell { class: "font-medium", "{item.name}" }
                                                TableCell { "{type_label}" }
                                                TableCell { "{qty}" }
                                                TableCell { "{unit}" }
                                                TableCell { class: "font-medium", "{total}" }
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
    }
}

/// Line-item type options for the contract-item editors. Values must match
/// the server's `contract_items.item_type` CHECK set (MAPPS-190); the create
/// form and the detail-page Add/Edit modal share this single list so they
/// cannot drift apart.
fn contract_item_type_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("recurring_service", "Recurring Service"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("retainer", "Retainer"),
        SelectOption::new("product", "Product"),
        SelectOption::new("one_time", "One-time"),
    ]
}

/// Editable state for the detail-page Add/Edit line-item modal. The form
/// exposes only name, type, quantity, and unit price; on edit, `original`
/// retains every other field of the existing row so a `PUT` (full replace)
/// does not wipe description, included hours, rollover settings, etc.
#[derive(Clone, Debug, PartialEq)]
struct ContractItemFormState {
    /// Full original row when editing (enables Delete; preserves unexposed
    /// fields). `None` when adding a new item.
    original: Option<ContractItemResponse>,
    name: String,
    item_type: String,
    quantity: String,
    unit_price: String,
    /// sort_order applied to a newly added item; ignored on edit, which
    /// keeps the original row's sort_order.
    new_sort_order: i32,
}

impl ContractItemFormState {
    fn new(next_sort_order: i32) -> Self {
        Self {
            original: None,
            name: String::new(),
            item_type: "recurring_service".to_string(),
            quantity: "1".to_string(),
            unit_price: String::new(),
            new_sort_order: next_sort_order,
        }
    }

    fn from_existing(i: &ContractItemResponse) -> Self {
        Self {
            original: Some(i.clone()),
            name: i.name.clone(),
            item_type: if i.item_type.is_empty() {
                "recurring_service".to_string()
            } else {
                i.item_type.clone()
            },
            quantity: i.quantity.normalize().to_string(),
            unit_price: i.unit_price.to_string(),
            new_sort_order: 0,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContractItemFormModalProps {
    state: ContractItemFormState,
    contract_id: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn ContractItemFormModal(props: ContractItemFormModalProps) -> Element {
    let initial = props.state.clone();
    let original = initial.original.clone();
    let is_edit = original.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut item_type = use_signal(|| initial.item_type.clone());
    let mut quantity = use_signal(|| initial.quantity.clone());
    let mut unit_price = use_signal(|| initial.unit_price.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;
    let type_options = contract_item_type_options();

    let original_for_save = original.clone();
    let contract_for_save = props.contract_id.clone();
    let new_sort_order = initial.new_sort_order;
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        let name_val = name.read().trim().to_string();
        if name_val.is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        let qty_val = match quantity.read().trim().parse::<Decimal>() {
            Ok(d) if !d.is_sign_negative() => d,
            Ok(_) => {
                error.set("Quantity cannot be negative.".to_string());
                return;
            }
            Err(_) => {
                error.set("Quantity is required and must be a number.".to_string());
                return;
            }
        };
        // Unit price is optional; a blank value bills at zero.
        let price_val = {
            let raw = unit_price.read().trim().to_string();
            if raw.is_empty() {
                Decimal::ZERO
            } else {
                match raw.parse::<Decimal>() {
                    Ok(d) if !d.is_sign_negative() => d,
                    Ok(_) => {
                        error.set("Unit price cannot be negative.".to_string());
                        return;
                    }
                    Err(_) => {
                        error.set("Unit price must be a number.".to_string());
                        return;
                    }
                }
            }
        };
        let type_val = {
            let t = item_type.read().clone();
            if t.is_empty() {
                "recurring_service".to_string()
            } else {
                t
            }
        };

        saving.set(true);
        error.set(String::new());
        let orig = original_for_save.clone();
        let contract = contract_for_save.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::modules::contracts::UpsertContractItemRequest;
                // Preserve the fields the form does not expose so an edit
                // (full-replace PUT) does not clear them.
                let body = UpsertContractItemRequest {
                    name: name_val.clone(),
                    description: orig.as_ref().and_then(|o| o.description.clone()),
                    item_type: type_val.clone(),
                    quantity: qty_val,
                    unit_price: price_val,
                    billing_frequency: orig
                        .as_ref()
                        .map(|o| o.billing_frequency.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "monthly".to_string()),
                    work_type_id: orig.as_ref().and_then(|o| o.work_type_id),
                    included_hours: orig.as_ref().and_then(|o| o.included_hours),
                    overage_rate: orig.as_ref().and_then(|o| o.overage_rate),
                    rollover_enabled: orig.as_ref().map(|o| o.rollover_enabled).unwrap_or(false),
                    max_rollover_hours: orig.as_ref().and_then(|o| o.max_rollover_hours),
                    sort_order: orig
                        .as_ref()
                        .map(|o| o.sort_order)
                        .unwrap_or(new_sort_order),
                };
                let result = match &orig {
                    None => crate::hooks::fetch::api::post_authed_typed::<ContractItemResponse, _>(
                        &format!("/contracts/{contract}/items"),
                        &body,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|e| e.user_message()),
                    Some(o) => {
                        crate::hooks::fetch::api::put_authed_typed::<ContractItemResponse, _>(
                            &format!("/contract-items/{}", o.id),
                            &body,
                        )
                        .await
                        .map(|_| ())
                        .map_err(|e| e.user_message())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save line item: {err}")),
                }
            }
            saving.set(false);
        });
    };

    // Delete opens an in-app confirm dialog (MAPPS-189), never the native
    // `window.confirm`.
    let handle_delete_click = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        confirm_delete.set(true);
    };

    let del_id = original.as_ref().map(|o| o.id.to_string());
    let perform_delete = move |_| {
        let Some(iid) = del_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::delete_authed_typed(&format!(
                    "/contract-items/{iid}"
                ))
                .await
                {
                    Ok(()) => onsaved.call(()),
                    Err(err) => {
                        error.set(format!(
                            "Could not delete line item: {}",
                            err.user_message()
                        ));
                    }
                }
            }
            confirm_delete.set(false);
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Line Item".to_string() } else { "Add Line Item".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            create_label: "Add Item".to_string(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete_click,
            crate::components::Input {
                name: "contract_item_name",
                label: "Name",
                placeholder: "e.g. Managed Workstation",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Select {
                name: "contract_item_type",
                label: "Type",
                options: type_options,
                value: item_type.read().clone(),
                onchange: move |e: FormEvent| item_type.set(e.value()),
            }
            crate::components::Input {
                name: "contract_item_qty",
                label: "Quantity",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                required: true,
                value: quantity.read().clone(),
                oninput: move |e: FormEvent| quantity.set(e.value()),
            }
            crate::components::Input {
                name: "contract_item_price",
                label: "Unit Price",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "0.00",
                value: unit_price.read().clone(),
                oninput: move |e: FormEvent| unit_price.set(e.value()),
            }
        }
        crate::components::ConfirmDialog {
            open: *confirm_delete.read(),
            title: "Delete line item".to_string(),
            message: "Delete this line item? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: perform_delete,
            oncancel: move |_| confirm_delete.set(false),
        }
    }
}

#[component]
fn ContractHourBalanceCard(
    balance_resource: Resource<Option<Paginated<ContractHourBalanceResponse>>>,
) -> Element {
    let snap = balance_resource.read_unchecked();
    rsx! {
        Card {
            title: "Hour Balance",
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Period" }
                        TableHeader { "Included" }
                        TableHeader { "Used" }
                        TableHeader { "Remaining" }
                        TableHeader { "Rollover" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 5, rows: 2 } },
                    Some(None) => rsx! {
                        TableEmpty { columns: 5, message: "Could not load hour balance.".to_string() }
                    },
                    Some(Some(resp)) if resp.data.is_empty() => rsx! {
                        TableEmpty { columns: 5, message: "No hour-balance periods for this contract.".to_string() }
                    },
                    Some(Some(resp)) => {
                        let rows = resp.data.clone();
                        rsx! {
                            TableBody {
                                for bal in rows.into_iter() {
                                    {
                                        let key = bal.id.to_string();
                                        let period = format!(
                                            "{} - {}",
                                            bal.period_start.format("%b %-d, %Y"),
                                            bal.period_end.format("%b %-d, %Y"),
                                        );
                                        let included = bal.hours_included.normalize().to_string();
                                        let used = bal.hours_used.normalize().to_string();
                                        let remaining = bal.hours_remaining.normalize().to_string();
                                        let rollover = bal.rollover_hours.normalize().to_string();
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell { "{period}" }
                                                TableCell { "{included}" }
                                                TableCell { "{used}" }
                                                TableCell { class: "font-medium", "{remaining}" }
                                                TableCell { "{rollover}" }
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
    }
}

// ============================================================================
// RATE CARDS
// ============================================================================

/// Rate-card list page. Fetches `GET /rate-cards` (paginated).
#[component]
pub fn RateCardListPage(
    /// Open the create modal on mount. The `/rate-cards/new` route renders
    /// this page with `open_create = true` so the URL lands on the create
    /// form instead of mis-routing to a failed detail load (MAPPS-217).
    #[props(default = false)]
    open_create: bool,
) -> Element {
    let can_edit = use_can_manage_billing();
    let navigator = use_navigator();
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(move || {
        if open_create {
            Some(RateCardFormState::new())
        } else {
            None
        }
    });
    let current_page = (*page.read()).max(1);

    let mut rate_cards_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        let path = format!("/rate-cards?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_with_auth::<Paginated<RateCardResponse>>(&path, &token)
            .await
            .ok()
    });

    let snap = rate_cards_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RateCardResponse>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    rsx! {
        AppLayout { title: "Rate Cards",
            PageHeader {
                title: "Rate Cards",
                subtitle: "Hourly rates by work type",
                actions: rsx! {
                    Link {
                        to: Route::ContractList {},
                        Button { variant: ButtonVariant::Secondary, "Contracts" }
                    }
                    if can_edit {
                        Button {
                            variant: ButtonVariant::Primary,
                            onclick: move |_| editing.set(Some(RateCardFormState::new())),
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Rate Card"
                        }
                    }
                },
            }

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load rate cards. Refresh the page to retry."
                }
            }

            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 3,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Description" }
                            TableHeader { "Default" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 3, rows: 5 }
                    } else if rows.is_empty() && !fetch_failed {
                        TableEmpty {
                            columns: 3,
                            title: "No rate cards yet".to_string(),
                            description: "Create your first rate card to set hourly rates by work type."
                                .to_string(),
                            actions: can_edit.then(|| rsx! {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    onclick: move |_| editing.set(Some(RateCardFormState::new())),
                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                    "New Rate Card"
                                }
                            }),
                        }
                    } else {
                        TableBody {
                            for card in rows.into_iter() {
                                RateCardRow {
                                    key: "{card.id}",
                                    id: card.id.to_string(),
                                    name: card.name.clone(),
                                    description: card.description.clone().unwrap_or_default(),
                                    is_default: card.is_default,
                                }
                            }
                        }
                    }
                }
            }

            if let Some(state) = editing.read().clone() {
                RateCardFormModal {
                    state,
                    onclose: move |_| editing.set(None),
                    onsaved: move |_| {
                        editing.set(None);
                        rate_cards_resource.restart();
                    },
                    oncreated: move |id: String| {
                        editing.set(None);
                        navigator.push(Route::RateCardDetail { id });
                    },
                    ondeleted: move |_| {
                        editing.set(None);
                        rate_cards_resource.restart();
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RateCardRowProps {
    id: String,
    name: String,
    description: String,
    is_default: bool,
}

#[component]
fn RateCardRow(props: RateCardRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();
    let is_default = props.is_default;
    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::RateCardDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::RateCardDetail { id: props.id.clone() },
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.name}"
                }
            }
            TableCell { class: "text-muted", "{props.description}" }
            TableCell {
                if is_default {
                    Badge { variant: BadgeVariant::Green, "Default" }
                }
            }
        }
    }
}

/// Rate-card detail page. Loads the rate card and its line items.
#[derive(Props, Clone, PartialEq)]
pub struct RateCardDetailPageProps {
    pub id: String,
}

#[component]
pub fn RateCardDetailPage(props: RateCardDetailPageProps) -> Element {
    let can_edit = use_can_manage_billing();
    let navigator = use_navigator();
    let id_str = props.id.clone();
    let id_for_card = id_str.clone();
    let id_for_items = id_str.clone();
    let card_id = id_str.clone();

    let mut editing_card = use_signal(|| None::<RateCardFormState>);
    let mut editing_item = use_signal(|| None::<RateCardItemFormState>);

    // The list endpoint is the only read for a single card's metadata
    // (there is no GET /rate-cards/{id}); pull a page and find the row.
    let mut card_resource = use_resource(move || {
        let id = id_for_card.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let resp = crate::hooks::fetch::api::get_authed::<Paginated<RateCardResponse>>(
                "/rate-cards?page=1&per_page=100",
            )
            .await
            .ok()?;
            resp.data.into_iter().find(|c| c.id.to_string() == id)
        }
    });
    let mut items_resource = use_resource(move || {
        let id = id_for_items.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RateCardItemResponse>>(&format!(
                "/rate-cards/{id}/items?page=1&per_page={SUBLIST_PER_PAGE}"
            ))
            .await
            .ok()
        }
    });
    // Work types resolve the item rows' `work_type_id` to names and feed
    // the rate editor's work-type picker.
    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOpt>>("/work-types?per_page=100")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let card_snapshot = card_resource.read_unchecked();
    let header_title = match &*card_snapshot {
        Some(Some(c)) => c.name.clone(),
        _ => "Rate Card".to_string(),
    };
    let card_for_edit = match &*card_snapshot {
        Some(Some(c)) => Some(RateCardFormState::from_existing(c)),
        _ => None,
    };

    let wt_snap = work_types_resource.read_unchecked();
    let work_types: Vec<WorkTypeOpt> = match &*wt_snap {
        Some(v) => v.clone(),
        None => Vec::new(),
    };

    // Work-type ids already on this card, so "Add rate" can exclude them
    // (the server upsert would otherwise silently overwrite an existing row).
    let items_snap = items_resource.read_unchecked();
    let used_work_type_ids: Vec<String> = match &*items_snap {
        Some(Some(resp)) => resp
            .data
            .iter()
            .map(|i| i.work_type_id.to_string())
            .collect(),
        _ => Vec::new(),
    };

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                breadcrumbs: rsx! {
                    crate::components::Breadcrumbs {
                        items: vec![
                            crate::components::BreadcrumbItem {
                                label: "Rate Cards".to_string(),
                                route: Some(Route::RateCardList {}),
                            },
                            crate::components::BreadcrumbItem {
                                label: header_title.clone(),
                                route: None,
                            },
                        ],
                    }
                },
                actions: rsx! {
                    Link {
                        to: Route::RateCardList {},
                        Button { variant: ButtonVariant::Secondary, "Rate Cards" }
                    }
                    if can_edit {
                        if let Some(state) = card_for_edit.clone() {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| editing_card.set(Some(state.clone())),
                                "Edit Card"
                            }
                        }
                    }
                },
            }

            match &*card_snapshot {
                None => rsx! {
                    // PMS-353
                    crate::components::DetailSkeleton {}
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load rate card." }
                            Link {
                                to: Route::RateCardList {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Back to rate cards"
                            }
                        }
                    }
                },
                Some(Some(card)) => {
                    let description = card.description.clone().unwrap_or_default();
                    let is_default = card.is_default;
                    rsx! {
                        div { class: "space-y-6",
                            Card { title: "Details",
                                dl { class: "space-y-4",
                                    if !description.is_empty() {
                                        div {
                                            dt { class: "text-sm text-muted", "Description" }
                                            dd { class: "mt-1 text-sm", "{description}" }
                                        }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Default" }
                                        dd {
                                            if is_default {
                                                Badge { variant: BadgeVariant::Green, "Yes" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "No" }
                                            }
                                        }
                                    }
                                }
                            }
                            RateCardItemsCard {
                                items_resource,
                                can_edit,
                                work_types: work_types.clone(),
                                editing_item,
                            }
                        }
                    }
                },
            }

            if let Some(state) = editing_card.read().clone() {
                RateCardFormModal {
                    state,
                    onclose: move |_| editing_card.set(None),
                    onsaved: move |_| {
                        editing_card.set(None);
                        card_resource.restart();
                    },
                    // The detail page only ever edits an existing card, so a
                    // create callback is never invoked here.
                    oncreated: move |_: String| {},
                    ondeleted: move |_| {
                        editing_card.set(None);
                        navigator.push(Route::RateCardList {});
                    },
                }
            }

            if let Some(state) = editing_item.read().clone() {
                RateCardItemFormModal {
                    state,
                    card_id: card_id.clone(),
                    work_types: work_types.clone(),
                    used_work_type_ids: used_work_type_ids.clone(),
                    onclose: move |_| editing_item.set(None),
                    onsaved: move |_| {
                        editing_item.set(None);
                        items_resource.restart();
                    },
                }
            }
        }
    }
}

#[component]
fn RateCardItemsCard(
    items_resource: Resource<Option<Paginated<RateCardItemResponse>>>,
    can_edit: bool,
    work_types: Vec<WorkTypeOpt>,
    editing_item: Signal<Option<RateCardItemFormState>>,
) -> Element {
    let mut editing_item = editing_item;
    let name_by_id: std::collections::HashMap<Uuid, String> =
        work_types.iter().map(|w| (w.id, w.name.clone())).collect();
    let snap = items_resource.read_unchecked();
    // Every work type already has a rate (or none are defined), so there is
    // nothing left to add: disable the button rather than open a modal whose
    // picker is empty.
    let used_count = match &*snap {
        Some(Some(resp)) => resp.data.len(),
        _ => 0,
    };
    let can_add = !work_types.is_empty() && used_count < work_types.len();
    // When disabled, say why so the greyed-out button is not read as broken
    // (MAPPS-217): either no work types exist, or every one already has a rate.
    let disabled_reason = if work_types.is_empty() {
        "Define a work type in Settings before adding a rate."
    } else {
        "Every work type already has a rate on this card."
    };
    rsx! {
        Card {
            title: "Rates",
            padding: false,
            actions: if can_edit {
                Some(rsx! {
                    // Stack the reason under the button as visible helper text,
                    // not just a `title` tooltip: a native tooltip on a disabled
                    // <button> is suppressed in some browsers (e.g. Chrome), so
                    // the explanation would never show. The tooltip is kept as a
                    // bonus for browsers that do render it (MAPPS-217).
                    div { class: "flex flex-col items-end gap-1",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: !can_add,
                            title: if can_add { None } else { Some(disabled_reason.to_string()) },
                            onclick: move |_| editing_item.set(Some(RateCardItemFormState::new())),
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Add Rate"
                        }
                        if !can_add {
                            p { class: "text-xs text-muted", "{disabled_reason}" }
                        }
                    }
                })
            } else {
                None
            },
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Work Type" }
                        TableHeader { "Hourly" }
                        TableHeader { "After Hours" }
                        TableHeader { "Emergency" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! {
                        TableEmpty { columns: 4, message: "Could not load rates.".to_string() }
                    },
                    Some(Some(resp)) if resp.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No rates on this card yet.".to_string() }
                    },
                    Some(Some(resp)) => {
                        let rows = resp.data.clone();
                        rsx! {
                            TableBody {
                                for item in rows.into_iter() {
                                    {
                                        let key = item.id.to_string();
                                        let wt_name = name_by_id
                                            .get(&item.work_type_id)
                                            .cloned()
                                            .unwrap_or_else(|| item.work_type_id.to_string());
                                        let hourly = format_money(item.hourly_rate);
                                        let after = format_money_opt(item.after_hours_rate);
                                        let emergency = format_money_opt(item.emergency_rate);
                                        let edit_state = RateCardItemFormState::from_existing(&item);
                                        rsx! {
                                            TableRow {
                                                key: "{key}",
                                                clickable: can_edit,
                                                onclick: move |_| {
                                                    if can_edit {
                                                        editing_item.set(Some(edit_state.clone()));
                                                    }
                                                },
                                                TableCell { class: "font-medium", "{wt_name}" }
                                                TableCell { "{hourly}" }
                                                TableCell { "{after}" }
                                                TableCell { "{emergency}" }
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
    }
}

// ============================================================================
// Rate-card editing (MAPPS-160)
//
// Cards: POST/PUT /rate-cards, DELETE /rate-cards/{id}. Items: upsert via
// POST /rate-cards/{id}/items (keyed on work_type_id), DELETE
// /rate-card-items/{id}. All writes require a finance role server-side
// (`RequireFinance`); the UI gates the affordances on the same capability.
// ============================================================================

/// Work-type option, used to resolve a rate row's `work_type_id` to a name
/// and to populate the rate editor's picker. `GET /work-types`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct WorkTypeOpt {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// True when the signed-in user holds a finance role (matches the server's
/// `RequireFinance` write guard). Reads stay open to any authenticated user;
/// only the create/edit/delete affordances are gated on this.
fn use_can_manage_billing() -> bool {
    let auth = crate::hooks::use_auth();
    let state = auth.read();
    state
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false)
}

/// Trim a free-text field to `Some(String)`, or `None` when empty.
fn opt_string(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Parse a money field into an optional `Decimal`. Empty -> `Ok(None)`; a
/// non-empty non-numeric value -> `Err(message)`.
fn parse_money_opt(s: &str) -> Result<Option<Decimal>, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<Decimal>()
        .map(Some)
        .map_err(|_| "Enter a valid amount, e.g. 150.00.".to_string())
}

#[derive(Clone, Debug, PartialEq)]
struct RateCardFormState {
    id: Option<String>,
    name: String,
    description: String,
    is_default: bool,
}

impl RateCardFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            is_default: false,
        }
    }

    fn from_existing(c: &RateCardResponse) -> Self {
        Self {
            id: Some(c.id.to_string()),
            name: c.name.clone(),
            description: c.description.clone().unwrap_or_default(),
            is_default: c.is_default,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RateCardFormModalProps {
    state: RateCardFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
    /// Called with the new card's id after a successful CREATE, so the list
    /// can jump straight to the new card to add rates (avoids a dead-end).
    oncreated: EventHandler<String>,
    ondeleted: EventHandler<()>,
}

#[component]
fn RateCardFormModal(props: RateCardFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;
    let oncreated = props.oncreated;
    let ondeleted = props.ondeleted;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let body = UpsertRateCardRequest {
            name: name.read().trim().to_string(),
            description: opt_string(&desc),
            is_default: *is_default.read(),
        };
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match id {
                    None => match crate::hooks::fetch::api::post_authed::<RateCardResponse, _>(
                        "/rate-cards",
                        &body,
                    )
                    .await
                    {
                        Ok(card) => oncreated.call(card.id.to_string()),
                        Err(err) => error.set(format!("Could not save rate card: {err}")),
                    },
                    Some(id) => match crate::hooks::fetch::api::put_authed::<RateCardResponse, _>(
                        &format!("/rate-cards/{id}"),
                        &body,
                    )
                    .await
                    {
                        Ok(_) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not save rate card: {err}")),
                    },
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
            // MAPPS-189: confirmation is handled by the SettingFormModal
            // ConfirmDialog before this fires, so just perform the delete.
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::delete_authed(&format!("/rate-cards/{id}")).await {
                    Ok(()) => ondeleted.call(()),
                    Err(err) => error.set(format!("Could not delete rate card: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Rate Card".to_string() } else { "New Rate Card".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            create_label: "Create Rate Card".to_string(),
            delete_title: "Delete rate card".to_string(),
            delete_message: "Delete this rate card? This cannot be undone.".to_string(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            crate::components::Input {
                name: "rate_card_name",
                label: "Name",
                placeholder: "e.g. Standard 2026",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "rate_card_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Checkbox {
                name: "rate_card_default",
                label: "Default rate card",
                help: "Used when a contract does not specify one.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RateCardItemFormState {
    /// `Some` => editing an existing line (enables delete; locks work type).
    item_id: Option<String>,
    work_type_id: String,
    hourly: String,
    after: String,
    emergency: String,
}

impl RateCardItemFormState {
    fn new() -> Self {
        Self {
            item_id: None,
            work_type_id: String::new(),
            hourly: String::new(),
            after: String::new(),
            emergency: String::new(),
        }
    }

    fn from_existing(i: &RateCardItemResponse) -> Self {
        Self {
            item_id: Some(i.id.to_string()),
            work_type_id: i.work_type_id.to_string(),
            hourly: i.hourly_rate.to_string(),
            after: i
                .after_hours_rate
                .map(|d| d.to_string())
                .unwrap_or_default(),
            emergency: i.emergency_rate.map(|d| d.to_string()).unwrap_or_default(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RateCardItemFormModalProps {
    state: RateCardItemFormState,
    card_id: String,
    work_types: Vec<WorkTypeOpt>,
    used_work_type_ids: Vec<String>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RateCardItemFormModal(props: RateCardItemFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.item_id.is_some();

    let mut work_type_id = use_signal(|| initial.work_type_id.clone());
    let mut hourly = use_signal(|| initial.hourly.clone());
    let mut after = use_signal(|| initial.after.clone());
    let mut emergency = use_signal(|| initial.emergency.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    // Picker options. On add, exclude work types already on the card (the
    // server upsert would otherwise overwrite an existing rate silently). On
    // edit the work type is the row identity, so it is fixed and disabled.
    let mut options = vec![SelectOption {
        value: String::new(),
        label: "Select a work type".to_string(),
        disabled: false,
    }];
    for wt in props.work_types.iter() {
        let id = wt.id.to_string();
        if !is_edit && props.used_work_type_ids.contains(&id) {
            continue;
        }
        options.push(SelectOption {
            value: id,
            label: wt.name.clone(),
            disabled: false,
        });
    }

    let card_for_save = props.card_id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        let wt = work_type_id.read().trim().to_string();
        if wt.is_empty() {
            error.set("Pick a work type.".to_string());
            return;
        }
        let work_type_uuid = match Uuid::parse_str(&wt) {
            Ok(u) => u,
            Err(_) => {
                error.set("Invalid work type.".to_string());
                return;
            }
        };
        let hourly_str = hourly.read().trim().to_string();
        let hourly_val = match hourly_str.parse::<Decimal>() {
            Ok(d) => d,
            Err(_) => {
                error.set("Hourly rate is required and must be a number.".to_string());
                return;
            }
        };
        let after_str = after.read().trim().to_string();
        let after_val = match parse_money_opt(&after_str) {
            Ok(v) => v,
            Err(e) => {
                error.set(e);
                return;
            }
        };
        let emergency_str = emergency.read().trim().to_string();
        let emergency_val = match parse_money_opt(&emergency_str) {
            Ok(v) => v,
            Err(e) => {
                error.set(e);
                return;
            }
        };
        // `min="0"` on the inputs is only advisory (paste/programmatic input
        // bypasses it), so reject negative rates client-side too.
        if hourly_val.is_sign_negative()
            || after_val.map(|v| v.is_sign_negative()).unwrap_or(false)
            || emergency_val.map(|v| v.is_sign_negative()).unwrap_or(false)
        {
            error.set("Rates cannot be negative.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = UpsertRateCardItemRequest {
            work_type_id: work_type_uuid,
            hourly_rate: hourly_val,
            after_hours_rate: after_val,
            emergency_rate: emergency_val,
        };
        let card = card_for_save.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_authed::<RateCardItemResponse, _>(
                    &format!("/rate-cards/{card}/items"),
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save rate: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let del_item_id = initial.item_id.clone();
    let handle_delete = move |_| {
        let Some(iid) = del_item_id.clone() else {
            return;
        };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            // MAPPS-189: confirmation is handled by the SettingFormModal
            // ConfirmDialog before this fires, so just perform the delete.
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::delete_authed(&format!("/rate-card-items/{iid}"))
                    .await
                {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not delete rate: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Rate".to_string() } else { "Add Rate".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            create_label: "Add Rate".to_string(),
            delete_title: "Delete rate".to_string(),
            delete_message: "Delete this rate? This cannot be undone.".to_string(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            crate::components::Select {
                name: "rate_item_work_type",
                label: "Work type",
                options,
                value: work_type_id.read().clone(),
                disabled: is_edit,
                onchange: move |e: FormEvent| work_type_id.set(e.value()),
            }
            crate::components::Input {
                name: "rate_item_hourly",
                label: "Hourly rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                required: true,
                value: hourly.read().clone(),
                oninput: move |e: FormEvent| hourly.set(e.value()),
            }
            crate::components::Input {
                name: "rate_item_after",
                label: "After-hours rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "Optional",
                value: after.read().clone(),
                oninput: move |e: FormEvent| after.set(e.value()),
            }
            crate::components::Input {
                name: "rate_item_emergency",
                label: "Emergency rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "Optional",
                value: emergency.read().clone(),
                oninput: move |e: FormEvent| emergency.set(e.value()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_money, validate_quantity, validate_text_optional, validate_text_required,
        CONTRACT_NAME_MAX, CONTRACT_NOTES_MAX, VALUE_MAX,
    };
    use rust_decimal::Decimal;

    #[test]
    fn text_required_flags_empty_and_overlong() {
        assert!(validate_text_required("  ", "Name", CONTRACT_NAME_MAX).is_err());
        assert!(validate_text_required("Acme", "Name", CONTRACT_NAME_MAX).is_ok());
        assert!(
            validate_text_required(&"x".repeat(CONTRACT_NAME_MAX), "Name", CONTRACT_NAME_MAX)
                .is_ok()
        );
        assert!(validate_text_required(
            &"x".repeat(CONTRACT_NAME_MAX + 1),
            "Name",
            CONTRACT_NAME_MAX
        )
        .is_err());
    }

    #[test]
    fn text_optional_blank_is_none_overlong_errs() {
        assert_eq!(
            validate_text_optional("  ", "Notes", CONTRACT_NOTES_MAX),
            Ok(None)
        );
        assert_eq!(
            validate_text_optional(" hi ", "Notes", CONTRACT_NOTES_MAX),
            Ok(Some("hi".to_string()))
        );
        assert!(validate_text_optional(
            &"x".repeat(CONTRACT_NOTES_MAX + 1),
            "Notes",
            CONTRACT_NOTES_MAX
        )
        .is_err());
    }

    #[test]
    fn money_blank_is_none() {
        assert_eq!(validate_money("", "Value"), Ok(None));
        assert_eq!(validate_money("   ", "Value"), Ok(None));
    }

    #[test]
    fn money_rejects_negative_and_overscale() {
        assert!(validate_money("-1", "Value")
            .unwrap_err()
            .contains("negative"));
        assert!(validate_money("1.234", "Value")
            .unwrap_err()
            .contains("decimal"));
        assert_eq!(
            validate_money("1234.56", "Value"),
            Ok(Some(Decimal::new(123456, 2)))
        );
    }

    #[test]
    fn money_distinguishes_out_of_range_from_non_numeric() {
        // A ~50-digit numeric paste is out of range, not "must be a number".
        let big = "2".repeat(50);
        assert!(validate_money(&big, "Value")
            .unwrap_err()
            .contains("out of range"));
        // A value within Decimal but above the documented cap is out of range.
        let over_cap = (VALUE_MAX + 1).to_string();
        assert!(validate_money(&over_cap, "Value")
            .unwrap_err()
            .contains("out of range"));
        // Genuinely non-numeric input keeps the "must be a number" message.
        assert!(validate_money("abc", "Value")
            .unwrap_err()
            .contains("must be a number"));
    }

    #[test]
    fn quantity_required_and_non_negative() {
        assert!(validate_quantity("", "Qty")
            .unwrap_err()
            .contains("required"));
        assert!(validate_quantity("-3", "Qty")
            .unwrap_err()
            .contains("negative"));
        assert_eq!(validate_quantity("2.5", "Qty"), Ok(Decimal::new(25, 1)));
    }
}

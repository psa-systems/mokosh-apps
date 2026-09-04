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
    clear_on_edit, contract_status_badge, use_page_title, Badge, BadgeVariant, Button,
    ButtonVariant, Card, DataTable, ErrorBanner, IconSize, PageHeader, PlusIcon, Select,
    SelectOption, SettingFormModal, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::modules::contracts::{
    ContractHourBalanceResponse, ContractItemResponse, ContractResponse, CreateContractRequest,
    RateCardItemResponse, RateCardResponse, UpdateContractRequest, UpsertRateCardItemRequest,
    UpsertRateCardRequest,
};
use crate::utils::sort_keys::COMPANIES_BY_NAME_SORT;
use crate::utils::url::urlencoding_minimal;
use crate::utils::{FormGuard, Paginated};
use crate::Route;

/// Rows per page for the contract list (mirrors `contacts.rs` PER_PAGE).
const PER_PAGE: usize = 25;

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

/// Title-case a billing-cycle enum for display (PMS-365). Mirrors the
/// `cycle_options` labels in the contract form so the detail panel does not
/// leak the raw lowercase value (e.g. `monthly`).
fn humanize_billing_cycle(raw: &str) -> String {
    match raw {
        "monthly" => "Monthly".to_string(),
        "quarterly" => "Quarterly".to_string(),
        "annually" | "annual" => "Annually".to_string(),
        "one_time" => "One-time".to_string(),
        "weekly" => "Weekly".to_string(),
        "bi_weekly" => "Bi-weekly".to_string(),
        "" => "-".to_string(),
        other => other.to_string(),
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
    // MAPPS-271: gate the page on `can_manage_billing` before mounting
    // any of the fetchers. The server's `list_contracts` route is
    // `RequireFinance`-gated so a non-finance role hits a 403, and the
    // generic fetch-failed banner that follows reads as a transient
    // error ("Could not load contracts. Refresh the page to retry.")
    // when the cause is actually a permanent permission boundary.
    // Render the same `PermissionRequired` empty state the Invoices /
    // Payments pages have always used, so every billing-gated list
    // looks consistent. Most users hit this branch only as a
    // theoretical fallback now that PMS-447 floor-promotes Bunyip
    // subscribers to tenant Admin (Admin already passes
    // `can_manage_billing()`); the standardised UI still matters for a
    // future role downgrade or a tenant-internal grant change.
    use_page_title("Contracts");
    // MAPPS-604: a contact with `contracts:read` reaches the list too.
    // Staff / platform sessions bypass `use_capability` unconditionally,
    // so the pre-pivot behaviour is preserved for every existing role.
    let contact_can_read = crate::hooks::capabilities::use_capability("contracts:read");
    if !use_can_manage_billing() && !contact_can_read {
        return rsx! {
            crate::components::PermissionRequired {
                title: "Contracts".to_string(),
                body: "Contracts are restricted to administrator and finance roles. Ask an administrator to grant you access.".to_string(),
            }
        };
    }

    // MAPPS-377: mount the data body only past the permission gate so its
    // fetch hooks run unconditionally within it (and never fire for a
    // non-finance role, preserving the pre-fix behaviour).
    rsx! { ContractListBody {} }
}

#[component]
fn ContractListBody() -> Element {
    // MAPPS-249: seed the company filter from `?company_id=<uuid>` so a context
    // card's "View All" lands here scoped to that company (the dropdown also
    // reflects the selection once its options load).
    let mut company_filter =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
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
    //
    // MAPPS-575: NOT narrowed to active. An archived company's contracts still
    // exist and looking at them is exactly what archiving-instead-of-deleting
    // is for, so it has to stay selectable here.
    let companies_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // The dropdown shows its placeholder alone for a tenant with no
        // companies AND for a failed read, so each says which it is.
        match crate::hooks::fetch::api::get_all_authed::<CompanyOption>(&format!(
            "/contacts/companies?{COMPANIES_BY_NAME_SORT}"
        ))
        .await
        {
            Ok(rows) => {
                if rows.is_empty() {
                    tracing::info!(
                        "contract company filter load succeeded and this tenant has no companies"
                    );
                }
                Some(rows)
            }
            Err(e) => {
                tracing::error!(
                    "contract company filter load failed, the dropdown will be empty: {e}"
                );
                None
            }
        }
    });
    let company_options = {
        let mut opts = vec![SelectOption::new("", "All Companies")];
        if let Some(Some(rows)) = &*companies_resource.read_unchecked() {
            for c in rows {
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
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // on reconnect and a failed load can render ContentUnavailable.
            let _reachable = crate::hooks::use_server_reachable();
            // MAPPS-604: allow either bearer; server scopes to
            // `contact.company_id` on the contact plane.
            if crate::hooks::fetch::api::current_access_token().is_none()
                && !crate::hooks::fetch::api::has_contact_session()
            {
                return None;
            }
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
            crate::hooks::fetch::api::get_authed_any::<Paginated<ContractResponse>>(&path)
                .await
                .inspect_err(|e| tracing::error!("contract list load failed: {e}"))
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

    // MAPPS-357: the contract list (GET /contracts) is this page's primary
    // resource. A failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state instead of an
    // empty table. A 4xx while still reachable keeps the inline banner below.
    // (No direct create/update/delete controls live on this page: New
    // Contract / Rate Cards are navigation and Clear filters is a filter
    // reset, so there is nothing here to gate on `can_mutate`.)
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Contracts".to_string() }
        };
    }

    rsx! {
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

        // MAPPS-321: scope indicator.
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Contracts,
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
            ErrorBanner { class: "mb-3", "Could not load contracts. Refresh the page to retry." }
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
                        // Filtered to nothing: no CTA (creating won't help);
                        // guide the user back to the filters.
                        // MAPPS-291: one-click "Clear filters" affordance.
                        TableEmpty {
                            columns: 6,
                            title: "No contracts match your filters".to_string(),
                            description: "Adjust the filters above, or clear them to see every contract again.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        company_filter.set(String::new());
                                        status_filter.set(String::new());
                                        type_filter.set(String::new());
                                    },
                                    "Clear filters"
                                }
                            },
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
                {
                    let (status_variant, status_label) = contract_status_badge(&props.status);
                    rsx! { Badge { variant: status_variant, "{status_label}" } }
                }
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
    use_page_title("New Contract");
    // MAPPS-300: pre-fill `company_id` from the URL so the Company detail
    // "New Contract" CTA lands on a form already scoped to that company.
    let initial = ContractFormValues {
        company_id: crate::utils::url::current_query_param("company_id").unwrap_or_default(),
        ..ContractFormValues::default()
    };
    rsx! {
        PageHeader {
            title: "New Contract",
            subtitle: "Create a new contract",
            // MAPPS-294: breadcrumb back to the Contracts list.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: vec![
                        crate::components::BreadcrumbItem {
                            label: "Contracts".to_string(),
                            route: Some(Route::ContractList {}),
                        },
                        crate::components::BreadcrumbItem {
                            label: "New Contract".to_string(),
                            route: None,
                        },
                    ],
                }
            },
        }
        ContractForm { mode: ContractFormMode::Create, initial }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ContractEditPageProps {
    pub id: String,
}

#[component]
pub fn ContractEditPage(props: ContractEditPageProps) -> Element {
    use_page_title("Edit Contract");
    let id_for_resource = props.id.clone();
    let id_for_form = props.id.clone();
    let detail_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: re-run on reconnect so the outage state self-heals.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<ContractResponse>(&format!("/contracts/{id}"))
                .await
                .inspect_err(|e| tracing::error!("contract edit load failed for {id}: {e}"))
                .ok()
        }
    });
    let snap = detail_resource.read_unchecked();
    // MAPPS-357: the fetched contract is this edit page's primary resource. A
    // load failure while the server is unreachable is an outage, not a missing
    // contract - render ContentUnavailable rather than the "Could not load"
    // card below (that stays for a 4xx while still reachable).
    let reachable = crate::hooks::use_server_reachable();
    let fetch_failed = matches!(*snap, Some(None));
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Edit Contract".to_string() }
        };
    }
    rsx! {
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
    // MAPPS-582: `Decimal::from_str` is ASCII-only, so an invisible character
    // makes a correct amount report "must be a number" with nothing on screen
    // to explain it.
    let s = crate::utils::text::clean_strict(raw);
    let s = s.as_str();
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
    // MAPPS-582: see `validate_money`.
    let s = crate::utils::text::clean_strict(raw);
    let s = s.as_str();
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

/// Parse a create-form line item's quantity on submit, naming the row.
///
/// The submit handler has already validated the row through
/// [`validate_quantity`] and shown any problem in the row's own `qty_err`
/// slot, so this is the second read of an already-checked value. It stays a
/// hard error, and logs, because reaching it means the two disagreed
/// (MAPPS-703).
fn parse_item_quantity(raw: &str, row: usize) -> Result<Decimal, String> {
    // MAPPS-582: `clean_strict` is what `validate_quantity` accepted, so an
    // invisible character cannot pass validation and then fail here.
    let s = crate::utils::text::clean_strict(raw);
    s.parse::<Decimal>().map_err(|e| {
        tracing::warn!("item {} quantity {s:?} is not a decimal: {e}", row + 1);
        format!("Item {} quantity is not a number", row + 1)
    })
}

/// Parse a create-form line item's unit price on submit, naming the row.
///
/// Blank bills at zero, which is what an untouched price has always meant
/// here. Anything else that is not a `Decimal` is reported and logged rather
/// than billed as zero (MAPPS-703). See [`parse_item_quantity`] for why this
/// is not the row's only report.
fn parse_item_unit_price(raw: &str, row: usize) -> Result<Decimal, String> {
    let s = crate::utils::text::clean_strict(raw);
    if s.is_empty() {
        return Ok(Decimal::ZERO);
    }
    s.parse::<Decimal>().map_err(|e| {
        tracing::warn!("item {} unit price {s:?} is not a decimal: {e}", row + 1);
        format!("Item {} unit price is not a number", row + 1)
    })
}

/// Parse a create-form line item's included hours on submit, naming the row.
///
/// Blank is "not set", which is what an untouched allotment has always meant.
/// An unreadable value used to become that same "not set", so a contract was
/// created with no allotment and nothing said why (MAPPS-703).
fn parse_item_included_hours(raw: &str, row: usize) -> Result<Option<Decimal>, String> {
    let s = crate::utils::text::clean_strict(raw);
    if s.is_empty() {
        return Ok(None);
    }
    match s.parse::<Decimal>() {
        Ok(d) => Ok(Some(d)),
        Err(e) => {
            tracing::warn!(
                "item {} included hours {s:?} is not a decimal: {e}",
                row + 1
            );
            Err(format!("Item {} included hours is not a number", row + 1))
        }
    }
}

/// Read the product picker's id back out of it (MAPPS-640).
///
/// Blank is "no product". A value that is not a UUID should be unreachable,
/// since the only way to set this is to pick a row the server issued, and there
/// is no field a person could correct if it happened. It is logged rather than
/// left silent, because the substitution drops the item's link to the product
/// catalogue and the log is the only witness (MAPPS-703).
fn picked_product_id(raw: &str) -> Option<Uuid> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match Uuid::parse_str(trimmed) {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!("picked product {trimmed:?} is not a UUID: {e}");
            None
        }
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
    // MAPPS-423: Cancel returns to what the user was editing, not to the list.
    let cancel_route = match &mode {
        ContractFormMode::Create => Route::ContractList {},
        ContractFormMode::Edit { id } => Route::ContractDetail { id: id.clone() },
    };

    let name = use_signal(|| initial.name.clone());
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
    let billing_amount = use_signal(|| initial.billing_amount.clone());
    let start_date = use_signal(|| initial.start_date.clone());
    let end_date = use_signal(|| initial.end_date.clone());
    let mut auto_renew = use_signal(|| initial.auto_renew);
    let contract_number = use_signal(|| initial.contract_number.clone());
    let notes = use_signal(|| initial.notes.clone());
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
    //
    // MAPPS-575: active only. This picks the company a NEW contract is written
    // against, which is day-to-day use, and offering an archived company here
    // would put it straight back into the state it was archived to leave. The
    // contracts LIST filter above deliberately does not narrow: an archived
    // company's contracts still exist, and being able to look at them is the
    // reason archiving keeps history in the first place.
    let companies_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // Same pair of indistinguishable outcomes as the list filter above.
        match crate::hooks::fetch::api::get_all_authed::<CompanyOption>(&format!(
            "/contacts/companies?status=active&{COMPANIES_BY_NAME_SORT}"
        ))
        .await
        {
            Ok(rows) => {
                if rows.is_empty() {
                    tracing::info!("contract company picker load succeeded and this tenant has no active companies");
                }
                Some(rows)
            }
            Err(e) => {
                tracing::error!(
                    "contract company picker load failed, the dropdown will be empty: {e}"
                );
                None
            }
        }
    });
    let company_options = {
        let mut opts = vec![SelectOption::new("", "Select a company")];
        if let Some(Some(rows)) = &*companies_resource.read_unchecked() {
            for c in rows {
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
        SelectOption::new("weekly", "Weekly"),
        SelectOption::new("bi_weekly", "Bi-weekly"),
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
            // PMS-518: the validation above already collects every field error at
            // once (MAPPS-211); add focus-first-invalid via FormGuard. The
            // per-field err signals were set in field order, so focus the first
            // non-empty one (each id matches the field's `name`/DOM id). Line-item
            // row errors have no single focusable id, so they fall back to no
            // focus (still blocked) when no top-level field is also invalid.
            let first_invalid = [
                ("name", !name_err.read().is_empty()),
                ("company", !company_err.read().is_empty()),
                ("billing_amount", !billing_err.read().is_empty()),
                ("start_date", !start_err.read().is_empty()),
                ("end_date", !end_err.read().is_empty()),
                ("contract_number", !number_err.read().is_empty()),
                ("notes", !notes_err.read().is_empty()),
            ]
            .into_iter()
            .find(|(_, bad)| *bad)
            .map(|(id, _)| id);
            let mut guard = FormGuard::new();
            guard.note_invalid(first_invalid);
            guard.blocked();
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
            #[cfg(feature = "app")]
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
                        // MAPPS-265: route every server-side field error from the
                        // 422 `errors[]` envelope back onto its own inline field
                        // so the cue persists after the failed submit, not just a
                        // banner. PMS-364's end < start cross-field rule arrives
                        // keyed on `end_date` and lands under that field via the
                        // same path. Errors with no matching field, or a non-422
                        // failure, fall back to the top-of-form banner.
                        let fields = err.field_errors();
                        if fields.is_empty() {
                            error.set(err.user_message());
                        } else {
                            let mut leftover = Vec::new();
                            for fe in fields {
                                match fe.field.as_str() {
                                    "name" => name_err.set(fe.message.clone()),
                                    "company_id" => company_err.set(fe.message.clone()),
                                    "billing_amount" => billing_err.set(fe.message.clone()),
                                    "start_date" => start_err.set(fe.message.clone()),
                                    "end_date" => end_err.set(fe.message.clone()),
                                    "contract_number" => number_err.set(fe.message.clone()),
                                    "notes" => notes_err.set(fe.message.clone()),
                                    _ => leftover.push(fe.message.clone()),
                                }
                            }
                            if !leftover.is_empty() {
                                error.set(leftover.join("; "));
                            }
                        }
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    // MAPPS-357: block the create/save submit while the server is unreachable
    // so a write cannot silently fail (edits are discarded, not queued). The
    // read subscribes this form so the button re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();

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
                    ErrorBanner { "{error.read()}" }
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
                        oninput: clear_on_edit(name, name_err),
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
                            onchange: clear_on_edit(company_id, company_err),
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
                        oninput: clear_on_edit(billing_amount, billing_err),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::DateField {
                        name: "start_date",
                        label: "Start Date",
                        required: true,
                        value: start_date.read().clone(),
                        error: start_err(),
                        oninput: clear_on_edit(start_date, start_err),
                    }
                    crate::components::DateField {
                        name: "end_date",
                        label: "End Date",
                        value: end_date.read().clone(),
                        error: end_err(),
                        oninput: clear_on_edit(end_date, end_err),
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
                        oninput: clear_on_edit(contract_number, number_err),
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
                    oninput: clear_on_edit(notes, notes_err),
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
                                                    next[idx].name_err.clear();
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
                                                next[idx].qty_err.clear();
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
                                                next[idx].price_err.clear();
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
                        to: cancel_route.clone(),
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        // MAPPS-357: disable while the server is unreachable.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        "{submit_label}"
                    }
                }
            }
        }
    }
}

/// POST each entered line item to `/contracts/{id}/items`. Blank rows
/// (no name) are skipped. Returns the first error encountered.
#[cfg(feature = "app")]
async fn create_items(contract_id: &str, items: &[ItemFormValues]) -> Result<(), String> {
    use crate::modules::contracts::UpsertContractItemRequest;
    for (idx, item) in items.iter().enumerate() {
        let name = item.name.trim();
        if name.is_empty() {
            continue;
        }
        let quantity = parse_item_quantity(&item.quantity, idx)?;
        let unit_price = parse_item_unit_price(&item.unit_price, idx)?;
        let included_hours = parse_item_included_hours(&item.included_hours, idx)?;
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
            product_id: None,
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
            // MAPPS-357: re-run on reconnect so the outage state self-heals.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<ContractResponse>(&format!("/contracts/{id}"))
                .await
                .inspect_err(|e| tracing::error!("contract detail load failed for {id}: {e}"))
                .ok()
        }
    });
    let mut items_resource = use_resource(move || {
        let id = id_for_items.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-528: detail sub-lists are not paginated in the UI, so
            // they page to the end rather than asking for one big page.
            crate::hooks::fetch::api::get_all_authed::<ContractItemResponse>(&format!(
                "/contracts/{id}/items"
            ))
            .await
            .inspect_err(|e| tracing::error!("contract item load failed for {id}: {e}"))
            .ok()
        }
    });
    let balance_resource = use_resource(move || {
        let id = id_for_balance.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_all_authed::<ContractHourBalanceResponse>(&format!(
                "/contracts/{id}/hour-balance"
            ))
            .await
            .inspect_err(|e| tracing::error!("contract hour balance load failed for {id}: {e}"))
            .ok()
        }
    });

    let contract_snapshot = contract_resource.read_unchecked();
    let header_title = match &*contract_snapshot {
        Some(Some(c)) => c.name.clone(),
        _ => "Contract".to_string(),
    };
    use_page_title(header_title.clone());

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let mut editing_item = use_signal(|| None::<ContractItemFormState>);
    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // actual DELETE fires from `on_confirm_delete` when confirmed.
    let mut confirming_delete = use_signal(|| false);
    // MAPPS-574: the server's reason for refusing this delete.
    let mut delete_error = use_signal(String::new);
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();

    // MAPPS-357: the fetched contract is this detail page's primary resource.
    // A load failure while the server is unreachable is an outage, not a
    // missing contract - render ContentUnavailable instead of the "Could not
    // load" card. `can_mutate` gates the write controls (Delete here, plus the
    // Add/Edit affordances in the line-item and hour-balance cards) so they
    // disable during the reachable-flips-false window before the resource
    // re-runs and swaps in the outage body.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    let fetch_failed = matches!(*contract_snapshot, Some(None));
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Contract".to_string() }
        };
    }

    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        delete_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/contracts/{id}");
                // MAPPS-574: report the refusal instead of discarding it. This
                // one already used the typed client, so the message is
                // `user_message()` rather than the raw string.
                match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Contract deleted.",
                        );
                        confirming_delete.set(false);
                        navigator.push(Route::ContractList {});
                    }
                    Err(err) => delete_error.set(err.user_message()),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete contract".to_string(),
            message: "Delete this contract? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            error: delete_error.read().clone(),
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                    delete_error.set(String::new());
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
                    // MAPPS-357: block delete while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
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
                let (status_variant, status_label) = contract_status_badge(&contract.status);
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
                                dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
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

                            ContractItemsCard { items_resource, editing_item, can_mutate }
                            ContractHourBalanceCard { balance_resource, items_resource, can_mutate }
                        }

                        div { class: "space-y-6",
                            Card { title: "Summary",
                                dl { class: "space-y-4",
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Status" }
                                        dd { Badge { variant: status_variant, "{status_label}" } }
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

#[component]
fn ContractItemsCard(
    items_resource: Resource<Option<Vec<ContractItemResponse>>>,
    editing_item: Signal<Option<ContractItemFormState>>,
    // MAPPS-357: false while the server is unreachable, so the Add Item
    // affordance (which opens a POST/PUT/DELETE modal) disables.
    can_mutate: bool,
) -> Element {
    let mut editing_item = editing_item;
    let snap = items_resource.read_unchecked();
    // Newly added items sort after the existing rows.
    let next_sort_order = match &*snap {
        Some(Some(rows)) => rows.len() as i32,
        _ => 0,
    };
    rsx! {
        Card {
            title: "Line Items",
            padding: false,
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't add a line item while the server is unreachable".to_string()),
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
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty { columns: 5, message: "No line items on this contract yet.".to_string() }
                    },
                    Some(Some(rows)) => {
                        let rows = rows.clone();
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
                                                onclick: {
                                                    let edit_state = edit_state.clone();
                                                    move |_| editing_item.set(Some(edit_state.clone()))
                                                },
                                                TableCell {
                                                    class: "font-medium",
                                                    // MAPPS-569: the row's click opens a modal, so there is no
                                                    // route to link to; this cell is the keyboard path instead.
                                                    onactivate: move |_| editing_item.set(Some(edit_state.clone())),
                                                    "{item.name}"
                                                }
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
    /// MAPPS-640: the catalog product this item sells, when one was picked
    /// or the row already names one. The price is the item's own.
    product_id: Option<String>,
    product_name: String,
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
            product_id: None,
            product_name: String::new(),
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
            product_id: i.product_id.map(|p| p.to_string()),
            // The response carries the id only; the chip says "a product" until
            // the operator changes it.
            product_name: if i.product_id.is_some() {
                "Product from the price list".to_string()
            } else {
                String::new()
            },
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
    let quantity = use_signal(|| initial.quantity.clone());
    let mut unit_price = use_signal(|| initial.unit_price.clone());
    // MAPPS-640: the picked product, if any.
    let mut product_id = use_signal(|| initial.product_id.clone());
    let mut product_name = use_signal(|| initial.product_name.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline validation errors so each failure shows under
    // its own field and a FormGuard can focus the first invalid one.
    let mut name_err = use_signal(String::new);
    let mut qty_err = use_signal(String::new);
    let mut price_err = use_signal(String::new);

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
        // PMS-518: accumulate every field error (clearing stale ones first)
        // instead of bailing on the first failure, then focus-first via
        // FormGuard. The existing typed parsers are kept so their returned
        // values still build the request body.
        name_err.set(String::new());
        qty_err.set(String::new());
        price_err.set(String::new());
        let mut ok = true;

        let name_val = name.read().trim().to_string();
        if name_val.is_empty() {
            name_err.set("Name is required.".to_string());
            ok = false;
        }
        let qty_val = match quantity.read().trim().parse::<Decimal>() {
            Ok(d) if !d.is_sign_negative() => Some(d),
            Ok(_) => {
                qty_err.set("Quantity cannot be negative.".to_string());
                ok = false;
                None
            }
            Err(_) => {
                qty_err.set("Quantity is required and must be a number.".to_string());
                ok = false;
                None
            }
        };
        // Unit price is optional; a blank value bills at zero.
        let price_val = {
            let raw = unit_price.read().trim().to_string();
            if raw.is_empty() {
                Some(Decimal::ZERO)
            } else {
                match raw.parse::<Decimal>() {
                    Ok(d) if !d.is_sign_negative() => Some(d),
                    Ok(_) => {
                        price_err.set("Unit price cannot be negative.".to_string());
                        ok = false;
                        None
                    }
                    Err(_) => {
                        price_err.set("Unit price must be a number.".to_string());
                        ok = false;
                        None
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

        if !ok {
            let first_invalid = [
                ("contract_item_name", !name_err.read().is_empty()),
                ("contract_item_qty", !qty_err.read().is_empty()),
                ("contract_item_price", !price_err.read().is_empty()),
            ]
            .into_iter()
            .find(|(_, bad)| *bad)
            .map(|(id, _)| id);
            let mut guard = FormGuard::new();
            guard.note_invalid(first_invalid);
            guard.blocked();
            return;
        }

        // Validated above: both parse to a value when `ok` holds.
        let qty_val = qty_val.expect("quantity validated above");
        let price_val = price_val.expect("unit price validated above");

        saving.set(true);
        error.set(String::new());
        let orig = original_for_save.clone();
        let contract = contract_for_save.clone();
        spawn(async move {
            #[cfg(feature = "app")]
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
                    product_id: product_id.read().as_deref().and_then(picked_product_id),
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
            #[cfg(feature = "app")]
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
            // MAPPS-640: pick from the price list. A pick fills the name and
            // the unit price, sets the type to product, and keeps the
            // reference; the price on the item is what the contract agreed.
            crate::components::ProductPicker {
                value: product_name.read().clone(),
                selected_id: product_id.read().clone(),
                label: "From the price list",
                onselect: move |picked: crate::components::PickedProduct| {
                    product_id.set(Some(picked.id.clone()));
                    product_name.set(picked.name.clone());
                    name_err.set(String::new());
                    name.set(picked.name.clone());
                    price_err.set(String::new());
                    unit_price.set(picked.unit_price.clone());
                    item_type.set("product".to_string());
                },
                onclear: move |_| {
                    product_id.set(None);
                    product_name.set(String::new());
                },
            }
            crate::components::Input {
                name: "contract_item_name",
                label: "Name",
                placeholder: "e.g. Managed Workstation",
                required: true,
                value: name.read().clone(),
                error: name_err(),
                oninput: clear_on_edit(name, name_err),
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
                error: qty_err(),
                oninput: clear_on_edit(quantity, qty_err),
            }
            crate::components::Input {
                name: "contract_item_price",
                label: "Unit Price",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "0.00",
                value: unit_price.read().clone(),
                error: price_err(),
                oninput: clear_on_edit(unit_price, price_err),
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
    balance_resource: Resource<Option<Vec<ContractHourBalanceResponse>>>,
    items_resource: Resource<Option<Vec<ContractItemResponse>>>,
    // MAPPS-357: false while the server is unreachable, so the Edit Allotment
    // affordance (which PUTs the block-hours item) disables.
    can_mutate: bool,
) -> Element {
    // MAPPS-255: rebind so a successful allotment edit can refresh both the
    // balance display and the items table.
    let mut balance_resource = balance_resource;
    let mut items_resource = items_resource;
    let can_edit = use_can_manage_billing();
    // The `block_hours` item being edited for its allotment, or `None`.
    let mut editing_allotment = use_signal(|| None::<ContractItemResponse>);

    let snap = balance_resource.read_unchecked();
    let items_snap = items_resource.read_unchecked();

    // The allotment lives on the `block_hours` line item's `included_hours`
    // (f2-hours-balance-be data model), so editing the allotment means
    // editing that item. Find it; absence hides the edit affordance.
    let block_item: Option<ContractItemResponse> = match &*items_snap {
        Some(Some(rows)) => rows.iter().find(|i| i.item_type == "block_hours").cloned(),
        _ => None,
    };

    // Current period = the period that started most recently. Used for the
    // headline so "X of Y hours remaining" reflects the live period rather
    // than an arbitrary table row.
    let headline: Option<(String, String)> = match &*snap {
        Some(Some(rows)) => rows.iter().max_by_key(|b| b.period_start).map(|p| {
            (
                p.hours_remaining.normalize().to_string(),
                p.hours_included.normalize().to_string(),
            )
        }),
        _ => None,
    };

    let edit_action = if can_edit && block_item.is_some() {
        rsx! {
            Button {
                variant: ButtonVariant::Secondary,
                // MAPPS-357: block while the server is unreachable.
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't edit the allotment while the server is unreachable".to_string()),
                onclick: move |_| editing_allotment.set(block_item.clone()),
                "Edit Allotment"
            }
        }
    } else {
        rsx! {}
    };

    rsx! {
        Card {
            title: "Hour Balance",
            padding: false,
            actions: edit_action,
            if let Some((remaining, included)) = headline.as_ref() {
                div { class: "px-4 py-4 border-b border-line",
                    p { class: "text-2xl font-semibold",
                        "{remaining}"
                        span { class: "text-base font-normal text-muted", " of {included} hours remaining" }
                    }
                    p { class: "mt-1 text-sm text-muted", "Current period allotment" }
                }
            }
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
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty { columns: 5, message: "No hour-balance periods for this contract.".to_string() }
                    },
                    Some(Some(rows)) => {
                        let rows = rows.clone();
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

        if let Some(item) = editing_allotment.read().clone() {
            AllotmentFormModal {
                item,
                onclose: move |_| editing_allotment.set(None),
                onsaved: move |_| {
                    editing_allotment.set(None);
                    // Refresh both the balance headline/table and the items
                    // table so the new allotment and remaining balance show.
                    balance_resource.restart();
                    items_resource.restart();
                },
            }
        }
    }
}

/// MAPPS-255: edit a `block_hours` line item's `included_hours` (the contract
/// allotment) in place. PUTs the full item via `UpsertContractItemRequest`,
/// preserving every other field, so the allotment changes without
/// re-creating the line item.
#[derive(Props, Clone, PartialEq)]
struct AllotmentFormModalProps {
    item: ContractItemResponse,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn AllotmentFormModal(props: AllotmentFormModalProps) -> Element {
    let item = props.item.clone();
    let initial = item
        .included_hours
        .map(|h| h.normalize().to_string())
        .unwrap_or_default();
    let allotted = use_signal(|| initial.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error so a FormGuard can focus the field.
    let mut allotted_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let item_for_save = item.clone();
    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        allotted_err.set(String::new());
        // Allotted hours are optional; blank clears the allotment. The outer
        // `Option` distinguishes a parse failure (`None`) from a valid blank
        // (`Some(None)`), so the parsed value still feeds the request body.
        let included = {
            let raw = allotted.read().trim().to_string();
            if raw.is_empty() {
                Some(None)
            } else {
                match raw.parse::<Decimal>() {
                    Ok(d) if !d.is_sign_negative() => Some(Some(d)),
                    Ok(_) => {
                        allotted_err.set("Allotted hours cannot be negative.".to_string());
                        None
                    }
                    Err(_) => {
                        allotted_err.set("Allotted hours must be a number.".to_string());
                        None
                    }
                }
            }
        };
        // PMS-518: single validated field, so focus it directly on failure.
        let Some(included) = included else {
            let mut guard = FormGuard::new();
            guard.note_invalid(Some("contract_allotted_hours"));
            guard.blocked();
            return;
        };

        saving.set(true);
        error.set(String::new());
        let item = item_for_save.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                use crate::modules::contracts::UpsertContractItemRequest;
                // Full-replace PUT: carry over every field except the
                // allotment so nothing the form does not expose is wiped.
                let body = UpsertContractItemRequest {
                    name: item.name.clone(),
                    description: item.description.clone(),
                    item_type: if item.item_type.is_empty() {
                        "block_hours".to_string()
                    } else {
                        item.item_type.clone()
                    },
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    billing_frequency: if item.billing_frequency.is_empty() {
                        "monthly".to_string()
                    } else {
                        item.billing_frequency.clone()
                    },
                    work_type_id: item.work_type_id,
                    included_hours: included,
                    overage_rate: item.overage_rate,
                    rollover_enabled: item.rollover_enabled,
                    max_rollover_hours: item.max_rollover_hours,
                    sort_order: item.sort_order,
                    // MAPPS-640: an edit elsewhere must not drop the reference.
                    product_id: item.product_id,
                };
                match crate::hooks::fetch::api::put_authed_typed::<ContractItemResponse, _>(
                    &format!("/contract-items/{}", item.id),
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => {
                        error.set(format!("Could not save allotment: {}", err.user_message()))
                    }
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: "Edit Allotted Hours".to_string(),
            is_edit: true,
            deletable: false,
            saving: *saving.read(),
            deleting: false,
            error: error.read().clone(),
            create_label: "Save".to_string(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: move |_| {},
            crate::components::Input {
                name: "contract_allotted_hours",
                label: "Allotted Hours",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "0",
                value: allotted.read().clone(),
                error: allotted_err(),
                oninput: clear_on_edit(allotted, allotted_err),
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
    // MAPPS-271: same posture as ContractListPage above. `list_rate_cards`
    // on the server is `RequireFinance`-gated, so a non-finance role
    // hit the generic "Could not load rate cards. Refresh the page to
    // retry." banner that read as transient when it was actually a
    // permission boundary. Render the shared `PermissionRequired`
    // empty state instead.
    use_page_title("Rate Cards");
    if !can_edit {
        return rsx! {
            crate::components::PermissionRequired {
                title: "Rate Cards".to_string(),
                body: "Rate cards are restricted to administrator and finance roles. Ask an administrator to grant you access.".to_string(),
            }
        };
    }

    // MAPPS-377: mount the data body only past the permission gate so its
    // fetch hooks run unconditionally within it. `open_create` and the
    // already-computed `can_edit` flag are threaded through as props.
    rsx! { RateCardListBody { open_create, can_edit } }
}

#[component]
fn RateCardListBody(open_create: bool, can_edit: bool) -> Element {
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
        // MAPPS-357: subscribe to reachability so the list auto-refetches on
        // reconnect and a failed load can render ContentUnavailable.
        let _reachable = crate::hooks::use_server_reachable();
        let token = crate::hooks::fetch::api::current_access_token()?;
        let path = format!("/rate-cards?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_with_auth::<Paginated<RateCardResponse>>(&path, &token)
            .await
            .inspect_err(|e| tracing::error!("rate card list load failed: {e}"))
            .ok()
    });

    let snap = rate_cards_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RateCardResponse>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: the rate-card list (GET /rate-cards) is this page's primary
    // resource. A failed load while the server is flagged down is an outage,
    // not an empty list - render ContentUnavailable rather than an empty
    // table (the inline banner below stays for a 4xx while still reachable).
    // `can_mutate` gates the New Rate Card affordances.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Rate Cards".to_string() }
        };
    }

    rsx! {
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
                        // MAPPS-357: block while the server is unreachable.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create a rate card while the server is unreachable".to_string()),
                        onclick: move |_| editing.set(Some(RateCardFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Rate Card"
                    }
                }
            },
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load rate cards. Refresh the page to retry." }
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
                                // MAPPS-357: block while the server is unreachable.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't create a rate card while the server is unreachable".to_string()),
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
            // MAPPS-357: re-run on reconnect so the outage state self-heals.
            let _reachable = crate::hooks::use_server_reachable();
            let rows = crate::hooks::fetch::api::get_all_authed::<RateCardResponse>("/rate-cards")
                .await
                .inspect_err(|e| tracing::error!("rate card load failed for {id}: {e}"))
                .ok()?;
            rows.into_iter().find(|c| c.id.to_string() == id)
        }
    });
    let mut items_resource = use_resource(move || {
        let id = id_for_items.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_all_authed::<RateCardItemResponse>(&format!(
                "/rate-cards/{id}/items"
            ))
            .await
            .inspect_err(|e| tracing::error!("rate card item load failed for {id}: {e}"))
            .ok()
        }
    });
    // Work types resolve the item rows' `work_type_id` to names and feed
    // the rate editor's work-type picker.
    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<WorkTypeOpt>("/work-types")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: rate rows fall back to a bare work-type id.
                tracing::warn!("work-type load failed: {e}");
                Vec::new()
            })
    });

    let card_snapshot = card_resource.read_unchecked();
    let header_title = match &*card_snapshot {
        Some(Some(c)) => c.name.clone(),
        _ => "Rate Card".to_string(),
    };
    use_page_title(header_title.clone());
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
        Some(Some(rows)) => rows.iter().map(|i| i.work_type_id.to_string()).collect(),
        _ => Vec::new(),
    };

    // MAPPS-357: the fetched rate card is this detail page's primary resource.
    // While the server is unreachable the card lookup can only resolve to
    // Some(None) by way of a failed GET (a genuine "card not in the list"
    // requires a successful fetch), so that pairing is an outage - render
    // ContentUnavailable rather than the "Could not load" card below. A
    // Some(None) while still reachable is the real not-found and keeps the
    // existing branch. `can_mutate` gates the Edit Card / Add Rate controls.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    let fetch_failed = matches!(*card_snapshot, Some(None));
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Rate Card".to_string() }
        };
    }

    rsx! {
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
                            // MAPPS-357: block while the server is unreachable.
                            disabled: !can_mutate,
                            title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
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
                            can_mutate,
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

#[component]
fn RateCardItemsCard(
    items_resource: Resource<Option<Vec<RateCardItemResponse>>>,
    can_edit: bool,
    // MAPPS-357: false while the server is unreachable, so the Add Rate
    // affordance (which POSTs a rate) disables on top of the can_add gate.
    can_mutate: bool,
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
        Some(Some(rows)) => rows.len(),
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
                            // MAPPS-357: also block while the server is unreachable.
                            disabled: !can_add || !can_mutate,
                            title: if !can_mutate {
                                Some("Can't add a rate while the server is unreachable".to_string())
                            } else if can_add {
                                None
                            } else {
                                Some(disabled_reason.to_string())
                            },
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
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No rates on this card yet.".to_string() }
                    },
                    Some(Some(rows)) => {
                        let rows = rows.clone();
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

    let name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error for Name so a FormGuard can focus it.
    let mut name_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;
    let oncreated = props.oncreated;
    let ondeleted = props.ondeleted;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        name_err.set(String::new());
        if name.read().trim().is_empty() {
            // PMS-518: single validated field, so focus it directly on failure.
            name_err.set("Name is required.".to_string());
            let mut guard = FormGuard::new();
            guard.note_invalid(Some("rate_card_name"));
            guard.blocked();
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
            #[cfg(feature = "app")]
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
            #[cfg(feature = "app")]
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
                error: name_err(),
                oninput: clear_on_edit(name, name_err),
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

    let work_type_id = use_signal(|| initial.work_type_id.clone());
    let hourly = use_signal(|| initial.hourly.clone());
    let after = use_signal(|| initial.after.clone());
    let emergency = use_signal(|| initial.emergency.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline validation errors so each failure shows under
    // its own field and a FormGuard can focus the first invalid one.
    let mut wt_err = use_signal(String::new);
    let mut hourly_err = use_signal(String::new);
    let mut after_err = use_signal(String::new);
    let mut emergency_err = use_signal(String::new);

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
        // PMS-518: accumulate every field error (clearing stale ones first)
        // instead of bailing on the first failure, then focus-first via
        // FormGuard. The existing typed parsers are kept so their returned
        // values still build the request body.
        wt_err.set(String::new());
        hourly_err.set(String::new());
        after_err.set(String::new());
        emergency_err.set(String::new());
        let mut ok = true;

        let wt = work_type_id.read().trim().to_string();
        let work_type_uuid = if wt.is_empty() {
            wt_err.set("Pick a work type.".to_string());
            ok = false;
            None
        } else {
            match Uuid::parse_str(&wt) {
                Ok(u) => Some(u),
                Err(_) => {
                    wt_err.set("Invalid work type.".to_string());
                    ok = false;
                    None
                }
            }
        };
        let hourly_str = hourly.read().trim().to_string();
        let hourly_val = match hourly_str.parse::<Decimal>() {
            Ok(d) => Some(d),
            Err(_) => {
                hourly_err.set("Hourly rate is required and must be a number.".to_string());
                ok = false;
                None
            }
        };
        let after_str = after.read().trim().to_string();
        let after_val = match parse_money_opt(&after_str) {
            Ok(v) => Some(v),
            Err(e) => {
                after_err.set(e);
                ok = false;
                None
            }
        };
        let emergency_str = emergency.read().trim().to_string();
        let emergency_val = match parse_money_opt(&emergency_str) {
            Ok(v) => Some(v),
            Err(e) => {
                emergency_err.set(e);
                ok = false;
                None
            }
        };
        // `min="0"` on the inputs is only advisory (paste/programmatic input
        // bypasses it), so reject negative rates client-side too. Route each
        // negative onto its own field; a failed parse already errored that
        // field (its value is `None`), so the check below skips it.
        if let Some(h) = hourly_val {
            if h.is_sign_negative() {
                hourly_err.set("Rates cannot be negative.".to_string());
                ok = false;
            }
        }
        if let Some(Some(a)) = after_val {
            if a.is_sign_negative() {
                after_err.set("Rates cannot be negative.".to_string());
                ok = false;
            }
        }
        if let Some(Some(em)) = emergency_val {
            if em.is_sign_negative() {
                emergency_err.set("Rates cannot be negative.".to_string());
                ok = false;
            }
        }

        if !ok {
            let first_invalid = [
                ("rate_item_work_type", !wt_err.read().is_empty()),
                ("rate_item_hourly", !hourly_err.read().is_empty()),
                ("rate_item_after", !after_err.read().is_empty()),
                ("rate_item_emergency", !emergency_err.read().is_empty()),
            ]
            .into_iter()
            .find(|(_, bad)| *bad)
            .map(|(id, _)| id);
            let mut guard = FormGuard::new();
            guard.note_invalid(first_invalid);
            guard.blocked();
            return;
        }

        // Validated above: every field parsed to a value when `ok` holds.
        let work_type_uuid = work_type_uuid.expect("work type validated above");
        let hourly_val = hourly_val.expect("hourly rate validated above");
        let after_val = after_val.expect("after-hours rate validated above");
        let emergency_val = emergency_val.expect("emergency rate validated above");

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
            #[cfg(feature = "app")]
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
            #[cfg(feature = "app")]
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
                error: wt_err(),
                onchange: clear_on_edit(work_type_id, wt_err),
            }
            crate::components::Input {
                name: "rate_item_hourly",
                label: "Hourly rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                required: true,
                value: hourly.read().clone(),
                error: hourly_err(),
                oninput: clear_on_edit(hourly, hourly_err),
            }
            crate::components::Input {
                name: "rate_item_after",
                label: "After-hours rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "Optional",
                value: after.read().clone(),
                error: after_err(),
                oninput: clear_on_edit(after, after_err),
            }
            crate::components::Input {
                name: "rate_item_emergency",
                label: "Emergency rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "Optional",
                value: emergency.read().clone(),
                error: emergency_err(),
                oninput: clear_on_edit(emergency, emergency_err),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_item_included_hours, parse_item_quantity, parse_item_unit_price, picked_product_id,
        validate_money, validate_quantity, validate_text_optional, validate_text_required,
        CONTRACT_NAME_MAX, CONTRACT_NOTES_MAX, VALUE_MAX,
    };
    use rust_decimal::Decimal;
    use uuid::Uuid;

    /// MAPPS-703: a quantity that will not parse names its row instead of
    /// stopping the whole submit with nothing to point at.
    #[test]
    fn item_quantity_reports_its_row() {
        assert_eq!(parse_item_quantity(" 2.5 ", 0), Ok(Decimal::new(25, 1)));
        assert!(parse_item_quantity("", 0).is_err());
        assert_eq!(
            parse_item_quantity("two", 1),
            Err("Item 2 quantity is not a number".to_string())
        );
    }

    /// MAPPS-703: a blank price still bills zero, an unreadable one no longer
    /// does.
    #[test]
    fn item_unit_price_only_defaults_when_blank() {
        assert_eq!(parse_item_unit_price("", 0), Ok(Decimal::ZERO));
        assert_eq!(parse_item_unit_price("   ", 0), Ok(Decimal::ZERO));
        assert_eq!(
            parse_item_unit_price(" 150.00 ", 0),
            Ok(Decimal::new(15000, 2))
        );
        assert_eq!(
            parse_item_unit_price("$150", 0),
            Err("Item 1 unit price is not a number".to_string())
        );
    }

    /// MAPPS-703: a blank allotment is still "not set", an unreadable one is
    /// no longer indistinguishable from it.
    #[test]
    fn item_included_hours_only_clears_when_blank() {
        assert_eq!(parse_item_included_hours("", 0), Ok(None));
        assert_eq!(parse_item_included_hours("  ", 0), Ok(None));
        assert_eq!(
            parse_item_included_hours(" 10 ", 0),
            Ok(Some(Decimal::from(10)))
        );
        assert_eq!(
            parse_item_included_hours("ten", 2),
            Err("Item 3 included hours is not a number".to_string())
        );
    }

    /// MAPPS-703: blank stays "no product", a picked id parses, and anything
    /// else still reads as "no product" but has been logged on the way through.
    #[test]
    fn picked_product_keeps_blank_as_none() {
        let id = Uuid::new_v4();
        assert_eq!(picked_product_id(""), None);
        assert_eq!(picked_product_id("   "), None);
        assert_eq!(picked_product_id(&format!(" {id} ")), Some(id));
        assert_eq!(picked_product_id("the blue one"), None);
    }

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

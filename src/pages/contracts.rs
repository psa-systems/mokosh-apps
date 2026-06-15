//! Contract pages, wired to the mokosh-server `/contracts` and
//! `/rate-cards` endpoints. Patterns (resource + `active_tenant_generation`,
//! loading/empty/error states, `serde`-typed request bodies, the minimal
//! query-string encoder) mirror `src/pages/contacts.rs`.
//!
//! Rate cards and their line items are fully editable here (MAPPS-160),
//! finance-gated to match the server's `RequireFinance` write guard.
//! Contract-item edit/delete is still ahead of the frontend (MAPPS-138).

use dioxus::prelude::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
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

/// Contract-type enum tags (`managed`, `block_hours`, `time_materials`,
/// `fixed_price`, ...) to a title-case label. Unknown tags pass through.
fn humanize_contract_type(raw: &str) -> String {
    match raw {
        "managed" => "Managed Services".to_string(),
        "block_hours" => "Block Hours".to_string(),
        "time_materials" => "Time & Materials".to_string(),
        "fixed_price" => "Fixed Price".to_string(),
        "recurring" => "Recurring".to_string(),
        other => other.to_string(),
    }
}

fn contract_type_variant(raw: &str) -> BadgeVariant {
    match raw {
        "managed" | "recurring" => BadgeVariant::Blue,
        "block_hours" => BadgeVariant::Purple,
        "fixed_price" => BadgeVariant::Green,
        "time_materials" => BadgeVariant::Yellow,
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

/// Format an optional `Decimal` money value as `$1,234.56`, or `-` when
/// absent. Thousands separators are inserted manually to avoid an extra
/// formatting dependency.
fn format_money_opt(amount: Option<Decimal>) -> String {
    match amount {
        Some(d) => format_money(d),
        None => "-".to_string(),
    }
}

fn format_money(amount: Decimal) -> String {
    let rounded = amount.round_dp(2);
    let negative = rounded.is_sign_negative();
    let abs = rounded.abs();
    let s = format!("{abs:.2}");
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s.as_str(), "00"));
    let mut grouped = String::new();
    let digits: Vec<char> = int_part.chars().collect();
    for (i, ch) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*ch);
    }
    let sign = if negative { "-" } else { "" };
    format!("{sign}${grouped}.{frac_part}")
}

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
        SelectOption::new("managed", "Managed Services"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("time_materials", "Time & Materials"),
        SelectOption::new("fixed_price", "Fixed Price"),
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
                        TableEmpty {
                            columns: 6,
                            message: if has_filters {
                                "No contracts match your filters.".to_string()
                            } else {
                                "No contracts yet. Click New Contract to create one.".to_string()
                            },
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
                    class: "font-medium text-blue-600 hover:text-blue-500",
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
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading contract..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contract." }
                            Link {
                                to: Route::ContractList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
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

/// One editable line item in the create form. Mirrors the fields the
/// server's `UpsertContractItemRequest` requires; decimals are held as
/// strings while editing and parsed on submit.
#[derive(Clone, Debug, PartialEq, Default)]
struct ItemFormValues {
    name: String,
    item_type: String,
    quantity: String,
    unit_price: String,
    included_hours: String,
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
    /// Initial line items (create flow only; the edit flow manages items
    /// from the detail page so it starts empty).
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
    let mut contract_type = use_signal(|| {
        if initial.contract_type.is_empty() {
            "managed".to_string()
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
    let mut error = use_signal(String::new);

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
        SelectOption::new("managed", "Managed Services"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("time_materials", "Time & Materials"),
        SelectOption::new("fixed_price", "Fixed Price"),
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
        SelectOption::new("annual", "Annual"),
        SelectOption::new("one_time", "One-time"),
    ];
    let item_type_options = vec![
        SelectOption::new("recurring", "Recurring"),
        SelectOption::new("one_time", "One-time"),
        SelectOption::new("block_hours", "Block Hours"),
        SelectOption::new("usage", "Usage"),
    ];

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

        // Validate the required start date up front (the server rejects a
        // missing/blank date with a 422, but catch it locally for a
        // clearer message).
        let start_raw = start_date.read().trim().to_string();
        if start_raw.is_empty() {
            error.set("Start date is required.".to_string());
            return;
        }
        let Ok(start) = chrono::NaiveDate::parse_from_str(&start_raw, "%Y-%m-%d") else {
            error.set("Start date is invalid.".to_string());
            return;
        };
        let end = {
            let raw = end_date.read().trim().to_string();
            if raw.is_empty() {
                None
            } else {
                match chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
                    Ok(d) => Some(d),
                    Err(_) => {
                        error.set("End date is invalid.".to_string());
                        return;
                    }
                }
            }
        };
        let billing = {
            let raw = billing_amount.read().trim().to_string();
            if raw.is_empty() {
                None
            } else {
                match raw.parse::<Decimal>() {
                    Ok(d) => Some(d),
                    Err(_) => {
                        error.set("Value must be a number.".to_string());
                        return;
                    }
                }
            }
        };

        is_submitting.set(true);
        let mode = mode.clone();
        let name_val = name.read().trim().to_string();
        let company_val = company_id.read().clone();
        let type_val = contract_type.read().clone();
        let status_val = status.read().clone();
        let cycle_val = billing_cycle.read().clone();
        let number_val = optional_trimmed(&contract_number.read());
        let notes_val = optional_trimmed(&notes.read());
        let items_snapshot = items.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match &mode {
                    ContractFormMode::Create => {
                        let Ok(company_uuid) = uuid::Uuid::parse_str(&company_val) else {
                            error.set("Please pick a company first.".to_string());
                            is_submitting.set(false);
                            return;
                        };
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
                            Err(err) => Err(err.user_message()),
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
                        .map_err(|err| err.user_message())
                    }
                };
                match result {
                    Ok(id) => {
                        navigator.push(Route::ContractDetail { id });
                    }
                    Err(msg) => {
                        error.set(msg);
                    }
                }
            }
            is_submitting.set(false);
        });
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
                        value: name.read().clone(),
                        oninput: move |e: FormEvent| name.set(e.value()),
                    }
                    Select {
                        name: "company",
                        label: "Company",
                        options: company_options,
                        value: company_id.read().clone(),
                        required: true,
                        disabled: is_edit,
                        onchange: move |e: FormEvent| company_id.set(e.value()),
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
                        value: billing_amount.read().clone(),
                        oninput: move |e: FormEvent| billing_amount.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "start_date",
                        label: "Start Date",
                        r#type: "date",
                        required: true,
                        value: start_date.read().clone(),
                        oninput: move |e: FormEvent| start_date.set(e.value()),
                    }
                    crate::components::Input {
                        name: "end_date",
                        label: "End Date",
                        r#type: "date",
                        value: end_date.read().clone(),
                        oninput: move |e: FormEvent| end_date.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "contract_number",
                        label: "Contract Number",
                        placeholder: "Optional reference",
                        value: contract_number.read().clone(),
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
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }

                // Line items (create flow only). On edit, items are
                // managed from the contract detail page so we don't show
                // the editor here (the server has no bulk-replace route).
                if !is_edit {
                    div { class: "border-t border-gray-200 dark:border-gray-700 pt-6",
                        div { class: "flex items-center justify-between mb-3",
                            h3 { class: "text-sm font-medium text-gray-900 dark:text-gray-100", "Line Items" }
                            Button {
                                variant: ButtonVariant::Secondary,
                                r#type: "button",
                                onclick: move |_| {
                                    let mut next = items.read().clone();
                                    next.push(ItemFormValues {
                                        item_type: "recurring".to_string(),
                                        quantity: "1".to_string(),
                                        ..ItemFormValues::default()
                                    });
                                    items.set(next);
                                },
                                "Add Item"
                            }
                        }
                        if items.read().is_empty() {
                            p { class: "text-sm text-gray-500", "No line items. Click Add Item to include one." }
                        } else {
                            div { class: "space-y-4",
                                for (idx, item) in items.read().clone().into_iter().enumerate() {
                                    div {
                                        key: "{idx}",
                                        class: "grid grid-cols-1 gap-3 sm:grid-cols-6 items-end border border-gray-200 dark:border-gray-700 rounded-md p-3",
                                        div { class: "sm:col-span-2",
                                            crate::components::Input {
                                                name: "item_name_{idx}",
                                                label: "Name",
                                                value: item.name.clone(),
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
                                            value: item.quantity.clone(),
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
                                            value: item.unit_price.clone(),
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
                "recurring".to_string()
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

fn optional_trimmed(value: &str) -> Option<String> {
    let t = value.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
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

    let contract_resource = use_resource(move || {
        let id = id_for_contract.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<ContractResponse>(&format!("/contracts/{id}"))
                .await
                .ok()
        }
    });
    let items_resource = use_resource(move || {
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
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Link {
                        to: Route::ContractEdit { id: edit_id },
                        Button { variant: ButtonVariant::Secondary, "Edit" }
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *deleting.read(),
                        onclick: move |_| {
                            let id = delete_id.clone();
                            deleting.set(true);
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let confirmed = web_sys::window()
                                        .and_then(|w| {
                                            w.confirm_with_message(
                                                "Delete this contract? This cannot be undone.",
                                            )
                                            .ok()
                                        })
                                        .unwrap_or(false);
                                    if confirmed {
                                        let path = format!("/contracts/{id}");
                                        if crate::hooks::fetch::api::delete_authed_typed(&path)
                                            .await
                                            .is_ok()
                                        {
                                            navigator.push(Route::ContractList {});
                                        }
                                    }
                                }
                                deleting.set(false);
                            });
                        },
                        "Delete"
                    }
                },
            }

            match &*contract_snapshot {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading contract..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contract." }
                            Link {
                                to: Route::ContractList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to contracts"
                            }
                        }
                    }
                },
                Some(Some(contract)) => {
                    let type_label = humanize_contract_type(&contract.contract_type);
                    let status_label = humanize_contract_status(&contract.status);
                    let billing_cycle = contract.billing_cycle.clone();
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
                                            dt { class: "text-sm text-gray-500", "Contract Type" }
                                            dd { class: "mt-1 font-medium", "{type_label}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Billing Cycle" }
                                            dd { class: "mt-1", "{billing_cycle}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Start Date" }
                                            dd { class: "mt-1", "{start}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "End Date" }
                                            dd { class: "mt-1", "{end}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Auto-Renewal" }
                                            dd { class: "mt-1", "{auto_renew}" }
                                        }
                                        if !number.is_empty() {
                                            div {
                                                dt { class: "text-sm text-gray-500", "Contract Number" }
                                                dd { class: "mt-1", "{number}" }
                                            }
                                        }
                                    }
                                    if !notes.is_empty() {
                                        div { class: "mt-4",
                                            dt { class: "text-sm text-gray-500 mb-1", "Notes" }
                                            dd { class: "text-sm whitespace-pre-line", "{notes}" }
                                        }
                                    }
                                }

                                ContractItemsCard { items_resource }
                                ContractHourBalanceCard { balance_resource }
                            }

                            div { class: "space-y-6",
                                Card { title: "Summary",
                                    dl { class: "space-y-4",
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Status" }
                                            dd { Badge { variant: status_variant(&contract.status), "{status_label}" } }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Value" }
                                            dd { class: "font-medium text-lg", "{billing_amount}" }
                                        }
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Company" }
                                            dd {
                                                Link {
                                                    to: Route::CompanyDetail { id: contract.company_id.to_string() },
                                                    class: "text-sm text-blue-600 hover:text-blue-500",
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
        }
    }
}

#[component]
fn ContractItemsCard(items_resource: Resource<Option<Paginated<ContractItemResponse>>>) -> Element {
    let snap = items_resource.read_unchecked();
    rsx! {
        Card {
            title: "Line Items",
            padding: false,
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
                                        rsx! {
                                            TableRow { key: "{key}",
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
pub fn RateCardListPage() -> Element {
    let can_edit = use_can_manage_billing();
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<RateCardFormState>);
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
                            message: "No rate cards yet.".to_string(),
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
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.name}"
                }
            }
            TableCell { class: "text-gray-500", "{props.description}" }
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
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading rate card..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load rate card." }
                            Link {
                                to: Route::RateCardList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
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
                                            dt { class: "text-sm text-gray-500", "Description" }
                                            dd { class: "mt-1 text-sm", "{description}" }
                                        }
                                    }
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-gray-500", "Default" }
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
    rsx! {
        Card {
            title: "Rates",
            padding: false,
            actions: if can_edit {
                Some(rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| editing_item.set(Some(RateCardItemFormState::new())),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Rate"
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
                let result: Result<(), String> = match id {
                    None => crate::hooks::fetch::api::post_authed::<RateCardResponse, _>(
                        "/rate-cards",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => crate::hooks::fetch::api::put_authed::<RateCardResponse, _>(
                        &format!("/rate-cards/{id}"),
                        &body,
                    )
                    .await
                    .map(|_| ()),
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save rate card: {err}")),
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
                        w.confirm_with_message("Delete this rate card? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    match crate::hooks::fetch::api::delete_authed(&format!("/rate-cards/{id}"))
                        .await
                    {
                        Ok(()) => ondeleted.call(()),
                        Err(err) => error.set(format!("Could not delete rate card: {err}")),
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
            if is_edit { "Save Changes" } else { "Create Rate Card" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: if is_edit { "Edit Rate Card".to_string() } else { "New Rate Card".to_string() },
            size: crate::components::ModalSize::Medium,
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
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this rate? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    match crate::hooks::fetch::api::delete_authed(&format!(
                        "/rate-card-items/{iid}"
                    ))
                    .await
                    {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete rate: {err}")),
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
            if is_edit { "Save Changes" } else { "Add Rate" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: if is_edit { "Edit Rate".to_string() } else { "Add Rate".to_string() },
            size: crate::components::ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
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
}

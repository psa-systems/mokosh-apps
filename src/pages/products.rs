//! The product catalog (MAPPS-640): the price list PMS-955 added to the
//! server, given a settings page.
//!
//! `products` is a tenant-scoped price list, not an inventory and not a
//! second home for labour pricing (that is `rate_cards`, by work type and by
//! the hour). It lives under Settings beside Tax Rates and Payment Terms
//! because it is the same kind of thing: reference data an administrator
//! curates, not a working surface. It carries the finance gate the server has
//! rather than the admin gate the other lookup editors use, for the same
//! reason `/settings/tax-rates` does: the server answers `super_admin |
//! admin | finance`, and a page that hid itself from a finance user would be
//! lying about what they may do.
//!
//! Two rules from the server shape the form, and both are said on it:
//!
//! - **A price is never read through the reference.** An invoice line or a
//!   contract item that names a product stores the price it was written with,
//!   so editing the catalog cannot re-price anything already sold. The form
//!   says so beside the price field.
//! - **A sold product cannot be deleted**, only retired (`is_active = false`).
//!   `ProductResponse.in_use` (PMS-1002) says whether anything names the
//!   product, so the list shows "In use" and the modal withholds Delete where
//!   the server would refuse it (MAPPS-684). The flag is advisory and the FK
//!   is the guard: a product sold between the list read and the click is
//!   still refused with a 409, and the form shows that message in the
//!   confirm dialog rather than closing on nothing (MAPPS-574).

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, ErrorBanner,
    IconSize, PageHeader, PlusIcon, Select, SelectOption, SettingFormModal, Table, TableBody,
    TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::utils::money::format_money_str;
use crate::utils::url::urlencoding_minimal;
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

const PER_PAGE: usize = 25;

/// Said beside the price field, because a catalog edit looks like it should
/// ripple and it must not.
const PRICE_NOTE: &str = "Changing this price does not re-price anything already sold. Every invoice line and contract item keeps the price it was written with; the new price applies to what is picked from the list from now on.";

/// `ProductResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteProduct {
    id: uuid::Uuid,
    #[serde(default)]
    sku: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    unit_price: String,
    #[serde(default)]
    unit: String,
    #[serde(default)]
    is_taxable: bool,
    #[serde(default)]
    is_active: bool,
    /// Whether any invoice line or contract item names it (PMS-1002).
    /// Defaulted so an older server reads false and the page behaves as it
    /// did before the flag: Delete offered, and the 409 shown on refusal.
    #[serde(default)]
    in_use: bool,
}

// ============================================================================
// Query and body helpers, kept pure so they can be tested off-web
// ============================================================================

/// The `is_active` filter the status select maps to. Blank is the whole
/// catalog, active and retired, so an administrator can see history; a
/// picker passes `true` on its own.
pub(crate) fn status_filter(raw: &str) -> Option<bool> {
    match raw.trim() {
        "active" => Some(true),
        "retired" => Some(false),
        _ => None,
    }
}

/// The list path for a page, a search and a status filter.
pub(crate) fn list_path(page: usize, q: &str, status: &str) -> String {
    let mut path = format!("/products?page={page}&per_page={PER_PAGE}");
    let q = q.trim();
    if !q.is_empty() {
        path.push_str(&format!("&q={}", urlencoding_minimal(q)));
    }
    if let Some(active) = status_filter(status) {
        path.push_str(&format!("&is_active={active}"));
    }
    path
}

/// A blank optional field is `null`, not `""`: the server keeps a SKU unique
/// when present, and an empty string is present.
fn optional_json(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

// ============================================================================
// Page
// ============================================================================

/// `/settings/products`. Finance-gated like Tax Rates.
#[component]
pub fn ProductsSettingsPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Products");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Products" } };
    }

    rsx! { ProductsSettingsBody {} }
}

#[component]
fn ProductsSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut search = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut editing = use_signal(|| None::<ProductFormState>);

    let current_page = (*page.read()).max(1);
    let search_text = search.read().clone();
    let status_text = status.read().clone();
    let path_for_resource = list_path(current_page, &search_text, &status_text);
    let mut resource = use_resource(move || {
        let path = path_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteProduct>>(&path)
                .await
                .inspect_err(|e| tracing::error!("product list load failed: {e}"))
                .ok()
        }
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteProduct>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Products".to_string() }
        };
    }

    let filtered = !search_text.trim().is_empty() || !status_text.is_empty();
    let status_options = vec![
        SelectOption::new("", "All products"),
        SelectOption::new("active", "Active"),
        SelectOption::new("retired", "Retired"),
    ];

    rsx! {
        PageHeader {
            title: "Products",
            subtitle: "The price list: what you sell, and what one of it costs",
            breadcrumbs: rsx! {
                crate::pages::settings::SettingsBreadcrumb { current: Route::SettingsProducts {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(ProductFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Product"
                }
            },
        }

        p { class: "mb-4 text-sm text-muted",
            "A product is picked onto an invoice line or a contract item, which then keeps the price it was picked at. This list prices things; labour is priced by the hour on rate cards."
        }

        Card { class: "mb-4",
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                crate::components::Input {
                    name: "product_search",
                    label: "Search",
                    r#type: "search",
                    placeholder: "Name or SKU",
                    value: search_text.clone(),
                    oninput: move |e: FormEvent| {
                        search.set(e.value());
                        page.set(1);
                    },
                }
                Select {
                    name: "product_status",
                    label: "Status",
                    options: status_options,
                    value: status_text.clone(),
                    onchange: move |e: FormEvent| {
                        status.set(e.value());
                        page.set(1);
                    },
                }
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load products. Refresh the page to retry." }
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
                        TableHeader { "Name" }
                        TableHeader { "SKU" }
                        TableHeader { class: "text-right", "Unit Price" }
                        TableHeader { "Unit" }
                        TableHeader { "Taxable" }
                        TableHeader { "Status" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    if filtered {
                        TableEmpty {
                            columns: 6,
                            message: "No products match this filter.".to_string(),
                        }
                    } else {
                        TableEmpty {
                            columns: 6,
                            title: "No products yet".to_string(),
                            description: "Add what you sell, with a unit price, so it can be picked onto invoices and contracts.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    // MAPPS-357: same unreachable guard as the header button.
                                    disabled: !can_mutate,
                                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                                    onclick: move |_| editing.set(Some(ProductFormState::new())),
                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                    "New Product"
                                }
                            },
                        }
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = ProductFormState::from_existing(&row);
                                let name = row.name.clone();
                                let sku = row.sku.clone().unwrap_or_default();
                                let price = format_money_str(&row.unit_price);
                                let unit = row.unit.clone();
                                let taxable = row.is_taxable;
                                let active = row.is_active;
                                let in_use = row.in_use;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: {
                                            let edit_state = edit_state.clone();
                                            move |_| editing.set(Some(edit_state.clone()))
                                        },
                                        TableCell {
                                            // MAPPS-569: the row opens a modal, so this cell is
                                            // the keyboard path.
                                            onactivate: move |_| editing.set(Some(edit_state.clone())),
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if sku.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { class: "font-mono text-xs", "{sku}" }
                                            }
                                        }
                                        TableCell { class: "text-right font-medium", "{price}" }
                                        TableCell { "{unit}" }
                                        TableCell {
                                            if taxable {
                                                Badge { variant: BadgeVariant::Blue, "Taxable" }
                                            } else {
                                                span { class: "text-subtle", "-" }
                                            }
                                        }
                                        TableCell {
                                            if active {
                                                Badge { variant: BadgeVariant::Green, "Active" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "Retired" }
                                            }
                                            if in_use {
                                                Badge { variant: BadgeVariant::Purple, class: "ml-2", "In use" }
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
            ProductFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

// ============================================================================
// Form
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
struct ProductFormState {
    id: Option<String>,
    sku: String,
    name: String,
    description: String,
    unit_price: String,
    unit: String,
    is_taxable: bool,
    is_active: bool,
    /// Carried from the row, never edited: the form has no say in it.
    in_use: bool,
}

impl ProductFormState {
    fn new() -> Self {
        Self {
            id: None,
            sku: String::new(),
            name: String::new(),
            description: String::new(),
            unit_price: String::new(),
            unit: "each".to_string(),
            is_taxable: true,
            is_active: true,
            in_use: false,
        }
    }

    /// Whether the modal offers Delete. Only an existing row can be deleted,
    /// and only one nothing has sold; the server refuses the rest with a 409,
    /// so the button is withheld rather than offered to fail.
    fn deletable(&self) -> bool {
        self.id.is_some() && !self.in_use
    }

    fn from_existing(r: &RemoteProduct) -> Self {
        Self {
            id: Some(r.id.to_string()),
            sku: r.sku.clone().unwrap_or_default(),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            unit_price: r.unit_price.clone(),
            unit: r.unit.clone(),
            is_taxable: r.is_taxable,
            is_active: r.is_active,
            in_use: r.in_use,
        }
    }

    /// The `UpsertProductRequest` body. Pure, so the null-for-blank rule on
    /// the optional fields is pinned by a test.
    fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "sku": optional_json(&self.sku),
            "name": self.name.trim(),
            "description": optional_json(&self.description),
            // A decimal string; the server parses it.
            "unit_price": self.unit_price.trim(),
            "unit": if self.unit.trim().is_empty() { "each" } else { self.unit.trim() },
            "is_taxable": self.is_taxable,
            "is_active": self.is_active,
        })
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProductFormModalProps {
    state: ProductFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn ProductFormModal(props: ProductFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let deletable = initial.deletable();
    // The body ignores it; carried only so the state literal stays complete.
    let deletable_in_use = initial.in_use;

    let mut sku = use_signal(|| initial.sku.clone());
    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut unit_price = use_signal(|| initial.unit_price.clone());
    let mut unit = use_signal(|| initial.unit.clone());
    let mut is_taxable = use_signal(|| initial.is_taxable);
    let mut is_active = use_signal(|| initial.is_active);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut sku_err = use_signal(String::new);
    let mut name_err = use_signal(String::new);
    let mut price_err = use_signal(String::new);
    let mut unit_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());

        // PMS-518: every field reported at once, first invalid focused. The
        // caps mirror `UpsertProductRequest`.
        let mut guard = FormGuard::new();
        let sku_v = sku.read().trim().to_string();
        let name_v = name.read().trim().to_string();
        let price_v = unit_price.read().trim().to_string();
        let unit_v = unit.read().trim().to_string();
        sku_err.set(guard.field("product_sku", &sku_v, "SKU", &[Rule::MaxLen(64)]));
        name_err.set(guard.field(
            "product_name",
            &name_v,
            "Name",
            &[Rule::Required, Rule::MaxLen(255)],
        ));
        price_err.set(guard.field(
            "product_unit_price",
            &price_v,
            "Unit price",
            &[
                Rule::Required,
                Rule::Number {
                    min: Some(0.0),
                    max: None,
                    max_decimals: None,
                },
            ],
        ));
        unit_err.set(guard.field(
            "product_unit",
            &unit_v,
            "Unit",
            &[Rule::Required, Rule::MaxLen(30)],
        ));
        if guard.blocked() {
            return;
        }

        saving.set(true);
        let body = ProductFormState {
            id: save_id.clone(),
            sku: sku_v,
            name: name_v,
            description: description.read().clone(),
            unit_price: price_v,
            unit: unit_v,
            is_taxable: *is_taxable.read(),
            is_active: *is_active.read(),
            in_use: deletable_in_use,
        }
        .body();
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/products",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                        &format!("/products/{id}"),
                        &body,
                    )
                    .await
                    .map(|_| ()),
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save product: {err}")),
                }
            }
            saving.set(false);
        });
    };

    // Delete runs from the shared modal's own confirm dialog. The server
    // refuses a sold product with a 409 that names retiring as the
    // alternative; that message lands in `error`, so the dialog does not
    // close on nothing (MAPPS-574).
    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::delete_authed(&format!("/products/{id}")).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not delete product: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Product".to_string() } else { "New Product".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            deletable,
            create_label: "Create Product".to_string(),
            delete_title: "Delete product".to_string(),
            delete_message: "Delete this product? Only a product nothing has sold can be deleted. One that is on an invoice or a contract is refused, because those documents still name it; retire it instead by unticking Active, which keeps it on what sold it and off the price list.".to_string(),
            crate::components::Input {
                name: "product_name",
                label: "Name",
                placeholder: "e.g. Managed Workstation",
                required: true,
                maxlength: 255,
                rules: vec![Rule::Required, Rule::MaxLen(255)],
                help: "Unique in the price list. Two rows with the same name and different prices is the confusion the list exists to remove.",
                error: name_err(),
                value: name.read().clone(),
                oninput: move |e: FormEvent| {
                    name_err.set(String::new());
                    name.set(e.value());
                },
            }
            crate::components::Input {
                name: "product_sku",
                label: "SKU",
                placeholder: "Optional",
                maxlength: 64,
                rules: vec![Rule::MaxLen(64)],
                help: "Optional, and unique when present.",
                error: sku_err(),
                value: sku.read().clone(),
                oninput: move |e: FormEvent| {
                    sku_err.set(String::new());
                    sku.set(e.value());
                },
            }
            crate::components::Textarea {
                name: "product_description",
                label: "Description",
                placeholder: "Optional",
                rows: 2,
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                crate::components::Input {
                    name: "product_unit_price",
                    label: "Unit Price",
                    r#type: "number",
                    required: true,
                    step: "0.01".to_string(),
                    min: "0".to_string(),
                    placeholder: "0.00",
                    rules: vec![
                        Rule::Required,
                        Rule::Number { min: Some(0.0), max: None, max_decimals: None },
                    ],
                    error: price_err(),
                    value: unit_price.read().clone(),
                    oninput: move |e: FormEvent| {
                        price_err.set(String::new());
                        unit_price.set(e.value());
                    },
                }
                crate::components::Input {
                    name: "product_unit",
                    label: "Unit",
                    placeholder: "each",
                    required: true,
                    maxlength: 30,
                    rules: vec![Rule::Required, Rule::MaxLen(30)],
                    help: "What one of it is: each, hour, month, user.",
                    error: unit_err(),
                    value: unit.read().clone(),
                    oninput: move |e: FormEvent| {
                        unit_err.set(String::new());
                        unit.set(e.value());
                    },
                }
            }
            if is_edit {
                p { class: "text-xs text-muted", "{PRICE_NOTE}" }
            }
            crate::components::Checkbox {
                name: "product_taxable",
                label: "Taxable",
                help: "Tax applies when this is put on an invoice.",
                checked: *is_taxable.read(),
                onchange: move |_| {
                    let next = !*is_taxable.read();
                    is_taxable.set(next);
                },
            }
            crate::components::Checkbox {
                name: "product_active",
                label: "Active",
                help: "Untick to retire: a retired product stays on everything that sold it and cannot be put on a new invoice or contract. Tick again to bring it back.",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_select_maps_onto_the_server_filter() {
        assert_eq!(status_filter(""), None);
        assert_eq!(status_filter("active"), Some(true));
        assert_eq!(status_filter("retired"), Some(false));
        assert_eq!(status_filter("anything"), None);
    }

    #[test]
    fn the_list_path_carries_only_what_was_asked_for() {
        assert_eq!(list_path(1, "", ""), "/products?page=1&per_page=25");
        assert_eq!(
            list_path(2, " ws ", "retired"),
            "/products?page=2&per_page=25&q=ws&is_active=false"
        );
    }

    /// Blank SKU and description go as `null`, because an empty SKU is a
    /// present SKU to the uniqueness rule; a blank unit falls back to `each`.
    #[test]
    fn the_body_sends_null_for_blank_optionals() {
        let state = ProductFormState {
            id: None,
            sku: "  ".to_string(),
            name: " Managed Workstation ".to_string(),
            description: String::new(),
            unit_price: "49.00".to_string(),
            unit: " ".to_string(),
            is_taxable: true,
            is_active: false,
            in_use: false,
        };
        let body = state.body();
        assert!(body["sku"].is_null());
        assert!(body["description"].is_null());
        assert_eq!(body["name"], "Managed Workstation");
        assert_eq!(body["unit_price"], "49.00");
        assert_eq!(body["unit"], "each");
        assert_eq!(body["is_active"], false);
        let with_sku = ProductFormState {
            sku: "WS-01".to_string(),
            unit: "month".to_string(),
            ..state
        }
        .body();
        assert_eq!(with_sku["sku"], "WS-01");
        assert_eq!(with_sku["unit"], "month");
    }

    /// Delete is offered only where the server would not refuse it: an
    /// existing row nothing has sold. A new row has nothing to delete, and a
    /// sold one is retired instead.
    #[test]
    fn delete_is_withheld_for_a_new_row_and_for_a_sold_one() {
        let fresh = ProductFormState {
            id: Some("p1".to_string()),
            ..ProductFormState::new()
        };
        assert!(fresh.deletable());
        let sold = ProductFormState {
            in_use: true,
            ..fresh.clone()
        };
        assert!(!sold.deletable());
        assert!(
            !ProductFormState::new().deletable(),
            "nothing to delete yet"
        );
    }
}

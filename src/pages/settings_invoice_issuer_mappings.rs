//! Settings > Integrations > Xero / QuickBooks > Mappings (MAPPS-1146,
//! server MAPPS-1099 Phase 2c).
//!
//! Without this page a connected tenant cannot issue one invoice. The
//! dispatcher resolves the Mokosh company to the provider's own customer
//! (Xero's `ContactID`, QuickBooks' `CustomerRef`) and refuses the create
//! when there is none, with a message naming a `PUT` route an admin had no
//! way to call.
//!
//! # Listed by company, not by mapping
//!
//! The server answers with the mappings that exist. This page lists the
//! tenant's COMPANIES and shows each one's mapping beside it, because the
//! question an admin has is "which of my clients can I invoice through
//! this provider", and a list of mappings cannot answer it: the companies
//! missing from that list are exactly the ones that matter.
//!
//! # Why products are QuickBooks-only
//!
//! A QuickBooks invoice line carries an `ItemRef` into the company's own
//! item list, so a line with no product has nothing to point at and the
//! client refuses it. Xero takes a free-text line and needs no product
//! mapping at all. Rendering an empty products table for Xero would be an
//! invitation to fill in something that is never read.
//!
//! # The people half (MAPPS-1168, server PMS-1541)
//!
//! Delegating a reimbursement names the person who was paid back. Xero
//! REFUSES a bank transaction with no contact, so a Xero tenant has to map
//! everybody who claims; QuickBooks takes it or leaves it, which the copy
//! says rather than implying both are required.
//!
//! # The bill half (MAPPS-1156, server MAPPS-1155)
//!
//! Delegating BILLS needs the same thing in the other direction: a bill
//! names a vendor over there the way an invoice names a customer, so
//! Vendors is a table on both providers' pages. Expense accounts is
//! QuickBooks-only for exactly the reason products are: every QuickBooks
//! bill line carries an `AccountRef` into the company's chart of
//! accounts, and Xero takes the account from the contact.

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorBanner, Input,
    PageHeader, StatusBanner,
};
use crate::pages::settings::AdminOnlyNotice;
use crate::pages::settings_invoice_issuer::{Issuer, QUICKBOOKS, XERO};
use crate::utils::sort_keys::COMPANIES_BY_NAME_SORT;

/// One company, as the picker list serves it.
#[derive(Clone, PartialEq, Deserialize)]
struct CompanyRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// One product, same shape.
#[derive(Clone, PartialEq, Deserialize)]
struct ProductRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// One vendor, as the picker list serves it (MAPPS-1156).
#[derive(Clone, PartialEq, Deserialize)]
struct VendorRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// One expense category, same shape.
#[derive(Clone, PartialEq, Deserialize)]
struct ExpenseCategoryRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

#[derive(Clone, PartialEq, Deserialize)]
struct VendorMapping {
    vendor_id: Uuid,
    #[serde(default)]
    external_vendor_id: String,
}

#[derive(Clone, PartialEq, Deserialize)]
struct ExpenseAccountMapping {
    category_id: Uuid,
    #[serde(default)]
    external_account_id: String,
}

#[derive(Deserialize)]
struct VendorMappings {
    #[serde(default)]
    mappings: Vec<VendorMapping>,
}

#[derive(Deserialize)]
struct ExpenseAccountMappings {
    #[serde(default)]
    mappings: Vec<ExpenseAccountMapping>,
}

/// One row of `GET /invoice-issuer-mappings/people` (PMS-1541).
#[derive(Clone, PartialEq, Deserialize)]
struct UserMapping {
    user_id: Uuid,
    #[serde(default)]
    external_contact_id: String,
}

#[derive(Deserialize)]
struct UserMappings {
    #[serde(default)]
    mappings: Vec<UserMapping>,
}

#[derive(Clone, PartialEq, Deserialize)]
struct CompanyMapping {
    company_id: Uuid,
    #[serde(default)]
    external_customer_id: String,
}

#[derive(Clone, PartialEq, Deserialize)]
struct ProductMapping {
    product_id: Uuid,
    #[serde(default)]
    external_item_id: String,
}

#[derive(Deserialize)]
struct CompanyMappings {
    #[serde(default)]
    mappings: Vec<CompanyMapping>,
}

#[derive(Deserialize)]
struct ProductMappings {
    #[serde(default)]
    mappings: Vec<ProductMapping>,
}

/// The body of an upsert, which is one field whose NAME is the only thing
/// that differs between the four surfaces (`external_customer_id`,
/// `external_item_id`, `external_vendor_id`, `external_account_id`).
///
/// Built from the name rather than one struct per surface: the editor used
/// to branch on `is_company`, and a third and fourth surface would have
/// made that a chain of booleans deciding a field name it could simply be
/// told (MAPPS-1156).
fn upsert_body(field: &str, value: &str) -> serde_json::Value {
    serde_json::json!({ field: value })
}

/// What one row of either table needs to render.
///
/// The same shape for both, because a company mapped to a customer and a
/// product mapped to an item are the same job twice and the only
/// difference is which path the save goes to.
#[derive(Clone, PartialEq)]
pub struct MappingRow {
    pub id: Uuid,
    pub name: String,
    pub external_id: String,
}

/// Join what this tenant has against what is mapped.
///
/// Records with no mapping come back with an empty `external_id`, which is
/// the state the page exists to make visible: an unmapped company is one
/// the provider will refuse an invoice for.
pub fn rows_from(names: &[(Uuid, String)], mapped: &HashMap<Uuid, String>) -> Vec<MappingRow> {
    names
        .iter()
        .map(|(id, name)| MappingRow {
            id: *id,
            name: name.clone(),
            external_id: mapped.get(id).cloned().unwrap_or_default(),
        })
        .collect()
}

/// How many of these are ready to invoice.
pub fn mapped_count(rows: &[MappingRow]) -> usize {
    rows.iter()
        .filter(|r| !r.external_id.trim().is_empty())
        .count()
}

#[component]
pub fn XeroMappingsPage() -> Element {
    rsx! { IssuerMappingsPage { issuer: XERO } }
}

#[component]
pub fn QuickBooksMappingsPage() -> Element {
    rsx! { IssuerMappingsPage { issuer: QUICKBOOKS } }
}

#[component]
fn IssuerMappingsPage(issuer: Issuer) -> Element {
    use_page_title("Mappings");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Mappings" } };
    }
    rsx! { IssuerMappingsBody { issuer } }
}

#[component]
fn IssuerMappingsBody(issuer: Issuer) -> Element {
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let can_mutate = crate::hooks::use_can_mutate();

    let companies = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let names = crate::hooks::fetch::api::get_all_authed::<CompanyRow>(&format!(
            "/contacts/companies?{COMPANIES_BY_NAME_SORT}"
        ))
        .await
        .inspect_err(|e| tracing::error!("company list load failed: {e}"))
        .ok()?;
        let mapped = crate::hooks::fetch::api::get_authed::<CompanyMappings>(
            "/invoice-issuer-mappings/companies",
        )
        .await
        .inspect_err(|e| tracing::error!("company mapping load failed: {e}"))
        .ok()?;
        let mapped: HashMap<Uuid, String> = mapped
            .mappings
            .into_iter()
            .map(|m| (m.company_id, m.external_customer_id))
            .collect();
        Some(rows_from(
            &names
                .into_iter()
                .map(|c| (c.id, c.name))
                .collect::<Vec<_>>(),
            &mapped,
        ))
    });

    let products = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // Xero lines are free text, so it never needs one of these.
        if issuer.provider == XERO.provider {
            return Some(Vec::new());
        }
        let names = crate::hooks::fetch::api::get_all_authed::<ProductRow>("/products")
            .await
            .inspect_err(|e| tracing::error!("product list load failed: {e}"))
            .ok()?;
        let mapped = crate::hooks::fetch::api::get_authed::<ProductMappings>(
            "/invoice-issuer-mappings/products",
        )
        .await
        .inspect_err(|e| tracing::error!("product mapping load failed: {e}"))
        .ok()?;
        let mapped: HashMap<Uuid, String> = mapped
            .mappings
            .into_iter()
            .map(|m| (m.product_id, m.external_item_id))
            .collect();
        Some(rows_from(
            &names
                .into_iter()
                .map(|p| (p.id, p.name))
                .collect::<Vec<_>>(),
            &mapped,
        ))
    });

    // MAPPS-1156: a bill names a vendor over there, on both providers.
    let vendors = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let names = crate::hooks::fetch::api::get_all_authed::<VendorRow>("/vendors")
            .await
            .inspect_err(|e| tracing::error!("vendor list load failed: {e}"))
            .ok()?;
        let mapped = crate::hooks::fetch::api::get_authed::<VendorMappings>(
            "/invoice-issuer-mappings/vendors",
        )
        .await
        .inspect_err(|e| tracing::error!("vendor mapping load failed: {e}"))
        .ok()?;
        let mapped: HashMap<Uuid, String> = mapped
            .mappings
            .into_iter()
            .map(|m| (m.vendor_id, m.external_vendor_id))
            .collect();
        Some(rows_from(
            &names
                .into_iter()
                .map(|v| (v.id, v.name))
                .collect::<Vec<_>>(),
            &mapped,
        ))
    });

    let expense_accounts = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // A Xero bill line takes its account from the contact, so Xero
        // never needs one of these.
        if issuer.provider == XERO.provider {
            return Some(Vec::new());
        }
        let names =
            crate::hooks::fetch::api::get_all_authed::<ExpenseCategoryRow>("/expense-categories")
                .await
                .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
                .ok()?;
        let mapped = crate::hooks::fetch::api::get_authed::<ExpenseAccountMappings>(
            "/invoice-issuer-mappings/expense-accounts",
        )
        .await
        .inspect_err(|e| tracing::error!("expense account mapping load failed: {e}"))
        .ok()?;
        let mapped: HashMap<Uuid, String> = mapped
            .mappings
            .into_iter()
            .map(|m| (m.category_id, m.external_account_id))
            .collect();
        Some(rows_from(
            &names
                .into_iter()
                .map(|c| (c.id, c.name))
                .collect::<Vec<_>>(),
            &mapped,
        ))
    });

    // MAPPS-1168: the people a reimbursement can name. Listed by MEMBER
    // and not by mapping, like every other table here: the rows that
    // matter are the ones with nothing beside them, because on Xero those
    // are the people whose claims cannot be paid back through it.
    let people = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let names = crate::pages::expenses::member_names().await;
        let mapped =
            crate::hooks::fetch::api::get_authed::<UserMappings>("/invoice-issuer-mappings/people")
                .await
                .inspect_err(|e| tracing::error!("people mapping load failed: {e}"))
                .ok()?;
        let mapped: HashMap<Uuid, String> = mapped
            .mappings
            .into_iter()
            .map(|m| (m.user_id, m.external_contact_id))
            .collect();
        let mut named: Vec<(Uuid, String)> = names
            .into_iter()
            .map(|(id, name)| {
                let name = if name.trim().is_empty() {
                    "A member with no name set".to_string()
                } else {
                    name
                };
                (id, name)
            })
            .collect();
        named.sort_by_key(|a| a.1.to_lowercase());
        Some(rows_from(&named, &mapped))
    });

    let company_rows = companies.read_unchecked().clone().flatten();
    let product_rows = products.read_unchecked().clone().flatten();
    let vendor_rows = vendors.read_unchecked().clone().flatten();
    let people_rows = people.read_unchecked().clone().flatten();
    let expense_account_rows = expense_accounts.read_unchecked().clone().flatten();

    rsx! {
        PageHeader {
            title: "{issuer.display} mappings",
            subtitle: "Every client, vendor and person this flows through has to name what they are called in {issuer.display}. A document for an unmapped record is refused.",
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        if !notice().is_empty() {
            StatusBanner { tone: crate::components::BannerTone::Success, class: "mb-4", "{notice}" }
        }
        div { class: "space-y-6",
            match company_rows {
                None => rsx! { crate::components::DetailSkeleton {} },
                Some(rows) => rsx! {
                    MappingTable {
                        issuer,
                        heading: "Clients".to_string(),
                        empty: "This organization has no clients yet.".to_string(),
                        label: format!("{} customer", issuer.display),
                        help: format!("The customer's id in {}.", issuer.display),
                        path_prefix: "companies".to_string(),
                        body_field: "external_customer_id".to_string(),
                        rows,
                        can_mutate,
                        on_error: move |e: String| error.set(e),
                        on_saved: move |name: String| {
                            notice.set(format!("{name} is mapped."));
                        },
                    }
                },
            }
            match vendor_rows {
                None => rsx! { crate::components::DetailSkeleton {} },
                Some(rows) => rsx! {
                    MappingTable {
                        issuer,
                        heading: "Vendors".to_string(),
                        empty: "This organization has no vendors yet.".to_string(),
                        label: format!("{} vendor", issuer.display),
                        help: format!("The vendor's id in {}. A bill for an unmapped vendor is refused.", issuer.display),
                        path_prefix: "vendors".to_string(),
                        body_field: "external_vendor_id".to_string(),
                        rows,
                        can_mutate,
                        on_error: move |e: String| error.set(e),
                        on_saved: move |name: String| {
                            notice.set(format!("{name} is mapped."));
                        },
                    }
                },
            }
            match people_rows {
                None => rsx! { crate::components::DetailSkeleton {} },
                Some(rows) => rsx! {
                    MappingTable {
                        issuer,
                        heading: "People".to_string(),
                        empty: "This organization has no members yet.".to_string(),
                        label: format!("{} contact", issuer.display),
                        help: if issuer.provider == XERO.provider {
                            format!("{} needs a contact on every reimbursement, so a claim from an unmapped person is refused.", issuer.display)
                        } else {
                            format!("Optional: {} records the reimbursement either way, and a mapped person is named on it.", issuer.display)
                        },
                        path_prefix: "people".to_string(),
                        body_field: "external_contact_id".to_string(),
                        rows,
                        can_mutate,
                        on_error: move |e: String| error.set(e),
                        on_saved: move |name: String| {
                            notice.set(format!("{name} is mapped."));
                        },
                    }
                },
            }
            if issuer.provider == QUICKBOOKS.provider {
                match expense_account_rows {
                    None => rsx! { crate::components::DetailSkeleton {} },
                    Some(rows) => rsx! {
                        MappingTable {
                            issuer,
                            heading: "Expense accounts".to_string(),
                            empty: "This organization has no expense categories yet.".to_string(),
                            label: format!("{} account", issuer.display),
                            help: format!("Every line of a {} bill posts to an account in your chart of accounts.", issuer.display),
                            path_prefix: "expense-accounts".to_string(),
                            body_field: "external_account_id".to_string(),
                            rows,
                            can_mutate,
                            on_error: move |e: String| error.set(e),
                            on_saved: move |name: String| {
                                notice.set(format!("{name} is mapped."));
                            },
                        }
                    },
                }
            }
            if issuer.provider == QUICKBOOKS.provider {
                match product_rows {
                    None => rsx! { crate::components::DetailSkeleton {} },
                    Some(rows) => rsx! {
                        MappingTable {
                            issuer,
                            heading: "Products".to_string(),
                            empty: "This organization has no products yet.".to_string(),
                            label: format!("{} item", issuer.display),
                            help: format!("Every line of a {} invoice names an item.", issuer.display),
                            path_prefix: "products".to_string(),
                            body_field: "external_item_id".to_string(),
                            rows,
                            can_mutate,
                            on_error: move |e: String| error.set(e),
                            on_saved: move |name: String| {
                                notice.set(format!("{name} is mapped."));
                            },
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn MappingTable(
    issuer: Issuer,
    heading: String,
    empty: String,
    label: String,
    help: String,
    path_prefix: String,
    body_field: String,
    rows: Vec<MappingRow>,
    can_mutate: bool,
    on_error: EventHandler<String>,
    on_saved: EventHandler<String>,
) -> Element {
    let ready = mapped_count(&rows);
    let total = rows.len();
    rsx! {
        Card {
            title: heading.clone(),
            actions: rsx! {
                Badge {
                    variant: if ready == total && total > 0 { BadgeVariant::Green } else { BadgeVariant::Gray },
                    "{ready} of {total} mapped"
                }
            },
            if rows.is_empty() {
                p { class: "text-sm text-muted", "{empty}" }
            } else {
                div { class: "space-y-4",
                    for row in rows.iter().cloned() {
                        MappingRowEditor {
                            key: "{row.id}",
                            issuer,
                            path_prefix: path_prefix.clone(),
                            body_field: body_field.clone(),
                            label: label.clone(),
                            help: help.clone(),
                            row,
                            can_mutate,
                            on_error,
                            on_saved,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MappingRowEditor(
    issuer: Issuer,
    path_prefix: String,
    body_field: String,
    label: String,
    help: String,
    row: MappingRow,
    can_mutate: bool,
    on_error: EventHandler<String>,
    on_saved: EventHandler<String>,
) -> Element {
    let mut value = use_signal(|| row.external_id.clone());
    let mut busy = use_signal(|| false);
    let unmapped = row.external_id.trim().is_empty();

    let save = {
        let path_prefix = path_prefix.clone();
        let field = body_field.clone();
        let name = row.name.clone();
        let id = row.id;
        move |_| {
            let path = format!(
                "/invoice-issuer-mappings/{path_prefix}/{}/{id}",
                issuer.provider
            );
            let entered = value.read().trim().to_string();
            let name = name.clone();
            let body = upsert_body(&field, &entered);
            busy.set(true);
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    // The response carries the stored mapping; this page
                    // re-reads nothing from it, because the value it would
                    // show is the one just typed.
                    match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                        .await
                    {
                        Ok(_) => on_saved.call(name),
                        Err(e) => on_error.call(format!("Could not save the mapping: {e}")),
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = (path, body, name);
                }
                busy.set(false);
            });
        }
    };

    rsx! {
        div { class: "flex flex-wrap items-end gap-3",
            div { class: "min-w-48 flex-1",
                p { class: "text-sm font-medium text-content", "{row.name}" }
                if unmapped {
                    p { class: "text-sm text-muted",
                        "Not mapped, so an invoice for this one is refused."
                    }
                }
            }
            div { class: "min-w-56 flex-1",
                Input {
                    name: "mapping-{row.id}",
                    label: label.clone(),
                    value: value(),
                    help: help.clone(),
                    disabled: !can_mutate || busy(),
                    oninput: move |e: FormEvent| value.set(e.value()),
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                loading: busy(),
                disabled: !can_mutate || value.read().trim().is_empty(),
                onclick: save,
                data_testid: "save-mapping-{row.id}",
                "Save"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    /// The list is every record this tenant has, mapped or not.
    ///
    /// A list of MAPPINGS would answer the wrong question: the rows that
    /// matter are the ones missing from it, because those are the clients
    /// an invoice is refused for.
    #[test]
    fn an_unmapped_record_is_listed_with_an_empty_mapping() {
        let names = vec![(id(1), "Acme".to_string()), (id(2), "Globex".to_string())];
        let mut mapped = HashMap::new();
        mapped.insert(id(1), "xero-contact-1".to_string());

        let rows = rows_from(&names, &mapped);
        assert_eq!(rows.len(), 2, "both, not just the mapped one");
        assert_eq!(rows[0].external_id, "xero-contact-1");
        assert!(
            rows[1].external_id.is_empty(),
            "Globex is unmapped, which is the state this page exists to show"
        );
        assert_eq!(mapped_count(&rows), 1);
    }

    /// A mapping that is only whitespace is not a mapping.
    ///
    /// The server refuses a blank one, so counting it as ready would tell
    /// an admin they can invoice a client the provider will refuse.
    #[test]
    fn whitespace_does_not_count_as_mapped() {
        let rows = vec![
            MappingRow {
                id: id(1),
                name: "Acme".into(),
                external_id: "   ".into(),
            },
            MappingRow {
                id: id(2),
                name: "Globex".into(),
                external_id: "58".into(),
            },
        ];
        assert_eq!(mapped_count(&rows), 1);
    }

    /// MAPPS-1168: people are mapped on both providers, and the help says
    /// which one refuses a reimbursement without it.
    ///
    /// Xero requires a contact on every bank transaction and QuickBooks
    /// does not, so one copy for both would either overstate the
    /// requirement or hide it.
    #[test]
    fn both_providers_map_people_and_say_who_needs_it() {
        let src = include_str!("settings_invoice_issuer_mappings.rs");
        let body = &src[src.find("fn IssuerMappingsBody").expect("the body")
            ..src.find("fn MappingTable").expect("the table")];
        let people_at = body.find(r#""People""#).expect("the people section");
        let first_guard = body
            .find("if issuer.provider == QUICKBOOKS.provider")
            .expect("the guard");
        assert!(
            people_at < first_guard,
            "people render before any provider guard, so both pages show them"
        );
        let help = &body[people_at..];
        assert!(
            help.contains("needs a contact on every reimbursement"),
            "the Xero copy states the requirement"
        );
        assert!(
            help.contains("Optional:"),
            "and the QuickBooks copy does not"
        );
    }

    /// MAPPS-1156: the vendors table is not behind the QuickBooks guard.
    ///
    /// A bill names a vendor in both providers, so a Xero tenant that
    /// could not map one would have every delegated bill refused with no
    /// page to fix it on.
    #[test]
    fn both_providers_map_vendors() {
        let src = include_str!("settings_invoice_issuer_mappings.rs");
        let body = &src[src.find("fn IssuerMappingsBody").expect("the body")
            ..src.find("fn MappingTable").expect("the table")];
        let vendors_at = body.find(r#""Vendors""#).expect("the vendors section");
        let first_guard = body
            .find("if issuer.provider == QUICKBOOKS.provider")
            .expect("the guard");
        assert!(
            vendors_at < first_guard,
            "vendors render before any provider guard, so both pages show them"
        );
    }

    /// Expense accounts are a QuickBooks question only, for the reason
    /// products are: a Xero bill line takes its account from the contact.
    #[test]
    fn only_quickbooks_asks_about_expense_accounts() {
        let src = include_str!("settings_invoice_issuer_mappings.rs");
        let body = &src[src.find("fn IssuerMappingsBody").expect("the body")
            ..src.find("fn MappingTable").expect("the table")];
        let accounts_at = body
            .find(r#""Expense accounts""#)
            .expect("the accounts section");
        let guard_at = body
            .find("if issuer.provider == QUICKBOOKS.provider")
            .expect("the guard");
        assert!(
            guard_at < accounts_at,
            "the accounts table renders behind the QuickBooks guard"
        );
    }

    /// The upsert body carries the field name the surface it saves to
    /// expects, and nothing else.
    ///
    /// The four surfaces differ in one field name, and a save that sent
    /// `external_customer_id` to the vendors route would be refused as a
    /// missing field with no hint of which page was wrong.
    #[test]
    fn an_upsert_sends_one_field_named_for_its_surface() {
        let body = upsert_body("external_vendor_id", "xero-contact-1");
        assert_eq!(body["external_vendor_id"], "xero-contact-1");
        assert_eq!(
            body.as_object().map(|o| o.len()),
            Some(1),
            "one field, so a route that validates its own cannot see another's"
        );
        assert_eq!(
            upsert_body("external_account_id", "7")["external_account_id"],
            "7"
        );
    }

    /// Products are a QuickBooks question only.
    ///
    /// A Xero line is free text and names no item, so an empty products
    /// table on the Xero page would invite an admin to fill in something
    /// nothing reads.
    #[test]
    fn only_quickbooks_asks_about_products() {
        let src = include_str!("settings_invoice_issuer_mappings.rs");
        let body = &src[src.find("fn IssuerMappingsBody").expect("the body")
            ..src.find("fn MappingTable").expect("the table")];
        let products_at = body.find("Products").expect("the products section");
        let guard_at = body
            .find("if issuer.provider == QUICKBOOKS.provider")
            .expect("the guard");
        assert!(
            guard_at < products_at,
            "the products table renders behind the QuickBooks guard"
        );
    }
}

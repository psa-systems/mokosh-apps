//! Asset management pages (CMDB), wired to the assets API (PMS-71).

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, Input,
    PageHeader, PlusIcon, SearchInput, Select, SelectOption, Table, TableBody, TableCell,
    TableHead, TableHeader, TableRow,
};
use crate::Route;

/// `PaginatedResponse<T>` envelope; serde drops `meta`.
#[derive(Clone, Debug, Deserialize)]
struct Paginated<T> {
    data: Vec<T>,
}

/// An asset (`GET /api/v1/assets`).
#[derive(Clone, Debug, Deserialize)]
struct RemoteAsset {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    asset_tag: Option<String>,
    #[serde(default)]
    asset_type_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    warranty_expiry: Option<String>,
    #[serde(default)]
    purchase_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct AssetTypeOpt {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CompanyOpt {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RemoteRelationship {
    #[serde(default)]
    child_asset_id: Option<uuid::Uuid>,
    #[serde(default)]
    relationship_type: String,
}

/// Secret-free credential summary (`GET /assets/:id/credentials`).
#[derive(Clone, Debug, Deserialize)]
struct CredSummary {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    credential_type: String,
    #[serde(default)]
    url: Option<String>,
}

/// Decrypted credential from the audited reveal (`GET /credentials/:id`).
#[derive(Clone, Debug, Deserialize)]
struct RevealedCred {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    notes: Option<String>,
}

/// Secret-free configuration-item summary.
#[derive(Clone, Debug, Deserialize)]
struct ConfigSummary {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    category: Option<String>,
}

/// Decrypted configuration item from the audited reveal.
#[derive(Clone, Debug, Deserialize)]
struct RevealedConfig {
    #[serde(default)]
    value: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AuditEntry {
    #[serde(default)]
    action: String,
    #[serde(default)]
    performed_at: Option<String>,
    #[serde(default)]
    changes: Option<serde_json::Value>,
}

/// (badge colour, label) for an asset status.
fn status_badge(status: &str) -> (BadgeVariant, &'static str) {
    match status {
        "active" => (BadgeVariant::Green, "Active"),
        "in_repair" => (BadgeVariant::Yellow, "In Repair"),
        "in_stock" => (BadgeVariant::Blue, "In Stock"),
        "retired" => (BadgeVariant::Red, "Retired"),
        "inactive" => (BadgeVariant::Gray, "Inactive"),
        _ => (BadgeVariant::Gray, "Unknown"),
    }
}

/// "Feb 28, 2025" from an ISO date string; "-" when absent.
fn fmt_date(s: &Option<String>) -> String {
    match s {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map(|nd| nd.format("%b %-d, %Y").to_string())
            .unwrap_or_else(|_| d.clone()),
        None => "-".to_string(),
    }
}

/// Asset list page
#[component]
pub fn AssetListPage() -> Element {
    let mut search = use_signal(String::new);

    let assets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteAsset>>("/assets")
            .await
            .ok()
            .map(|p| p.data)
    });
    let types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeOpt>>("/asset-types")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOpt>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    let snapshot = assets_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let assets: Vec<RemoteAsset> = snapshot.flatten().unwrap_or_default();
    let types = types_resource.read_unchecked().clone().unwrap_or_default();
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let type_name = |id: &Option<uuid::Uuid>| -> String {
        id.and_then(|tid| types.iter().find(|t| t.id == tid))
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "-".to_string())
    };
    let company_name = |id: &Option<uuid::Uuid>| -> String {
        id.and_then(|cid| companies.iter().find(|c| c.id == cid))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "-".to_string())
    };

    let needle = search.read().to_lowercase();
    let filtered: Vec<&RemoteAsset> = assets
        .iter()
        .filter(|a| {
            needle.is_empty()
                || a.name.to_lowercase().contains(&needle)
                || a.serial_number
                    .as_deref()
                    .map(|s| s.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        })
        .collect();
    let total = filtered.len();

    rsx! {
        AppLayout { title: "Assets",
            PageHeader {
                title: "Assets",
                subtitle: "Configuration items and customer assets",
                actions: rsx! {
                    Link {
                        to: Route::AssetNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Asset"
                        }
                    }
                },
            }

            Card { class: "mb-6",
                SearchInput {
                    value: search.read().clone(),
                    placeholder: "Search by name or serial...",
                    oninput: move |e: FormEvent| search.set(e.value()),
                }
            }

            if load_failed {
                Card { class: "mb-6",
                    p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                        "Could not load assets from the server."
                    }
                }
            }

            DataTable {
                total_items: total,
                current_page: 1,
                per_page: if total == 0 { 25 } else { total },
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Asset" }
                            TableHeader { "Type" }
                            TableHeader { "Company" }
                            TableHeader { "Serial / Tag" }
                            TableHeader { "Status" }
                        }
                    }
                    TableBody {
                        if is_loading {
                            TableRow { TableCell { class: "text-gray-400", "Loading…" } }
                        } else if filtered.is_empty() {
                            TableRow {
                                TableCell { class: "text-gray-400 italic",
                                    if assets.is_empty() {
                                        "No assets yet. Create one to get started."
                                    } else {
                                        "No assets match the search."
                                    }
                                }
                            }
                        } else {
                            for a in filtered.iter() {
                                {
                                    let (variant, label) = status_badge(&a.status);
                                    let tname = type_name(&a.asset_type_id);
                                    let cname = company_name(&a.company_id);
                                    let serial = a
                                        .serial_number
                                        .clone()
                                        .or_else(|| a.asset_tag.clone())
                                        .unwrap_or_else(|| "-".to_string());
                                    let aid = a.id.to_string();
                                    rsx! {
                                        TableRow { key: "{aid}",
                                            TableCell {
                                                Link {
                                                    to: Route::AssetDetail { id: aid.clone() },
                                                    class: "font-medium text-blue-600 hover:text-blue-500",
                                                    "{a.name}"
                                                }
                                            }
                                            TableCell { "{tname}" }
                                            TableCell { "{cname}" }
                                            TableCell { class: "font-mono text-sm", "{serial}" }
                                            TableCell { Badge { variant, "{label}" } }
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
}

/// New asset page
#[component]
pub fn AssetNewPage() -> Element {
    let mut name = use_signal(String::new);
    let mut asset_type = use_signal(String::new);
    let mut company = use_signal(String::new);
    let mut serial = use_signal(String::new);
    let mut manufacturer = use_signal(String::new);
    let mut model = use_signal(String::new);
    let mut warranty = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeOpt>>("/asset-types")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOpt>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let types = types_resource.read_unchecked().clone().unwrap_or_default();
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let mut type_options = vec![SelectOption::new("", "Select a type")];
    type_options.extend(
        types
            .iter()
            .map(|t| SelectOption::new(t.id.to_string(), t.name.clone())),
    );
    let mut company_options = vec![SelectOption::new("", "Select a company")];
    company_options.extend(
        companies
            .iter()
            .map(|c| SelectOption::new(c.id.to_string(), c.name.clone())),
    );

    let err = error.read().clone();

    rsx! {
        AppLayout { title: "New Asset",
            PageHeader { title: "New Asset", subtitle: "Add a new configuration item" }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        error.set(String::new());
                        let asset_name = name.read().trim().to_string();
                        let type_id = asset_type.read().clone();
                        let company_id = company.read().clone();
                        let serial_v = serial.read().clone();
                        let manufacturer_v = manufacturer.read().clone();
                        let model_v = model.read().clone();
                        let warranty_v = warranty.read().clone();
                        if asset_name.is_empty() {
                            error.set("Please enter an asset name.".to_string());
                            return;
                        }
                        if type_id.is_empty() {
                            error.set("Please pick an asset type.".to_string());
                            return;
                        }
                        if company_id.is_empty() {
                            error.set("Please pick a company.".to_string());
                            return;
                        }
                        is_submitting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let mut body = serde_json::json!({
                                    "name": asset_name,
                                    "asset_type_id": type_id,
                                    "company_id": company_id,
                                });
                                if !serial_v.is_empty() {
                                    body["serial_number"] = serde_json::json!(serial_v);
                                }
                                if !manufacturer_v.is_empty() {
                                    body["manufacturer"] = serde_json::json!(manufacturer_v);
                                }
                                if !model_v.is_empty() {
                                    body["model"] = serde_json::json!(model_v);
                                }
                                if !warranty_v.is_empty() {
                                    body["warranty_expiry"] = serde_json::json!(warranty_v);
                                }
                                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                        "/assets",
                                        &body,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::AssetList {});
                                    }
                                    Err(e) => {
                                        error.set(format!("Could not create asset: {e}"));
                                    }
                                }
                            }
                            is_submitting.set(false);
                        });
                    },

                    if !err.is_empty() {
                        div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                            p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Input {
                            name: "name",
                            label: "Name",
                            placeholder: "e.g. Exchange Server 01",
                            required: true,
                            value: name.read().clone(),
                            oninput: move |e: FormEvent| name.set(e.value()),
                        }
                        Select {
                            name: "type",
                            label: "Type",
                            options: type_options,
                            value: asset_type.read().clone(),
                            placeholder: "Select a type",
                            required: true,
                            onchange: move |e: FormEvent| asset_type.set(e.value()),
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "company",
                            label: "Company",
                            options: company_options,
                            value: company.read().clone(),
                            placeholder: "Select a company",
                            required: true,
                            onchange: move |e: FormEvent| company.set(e.value()),
                        }
                        Input {
                            name: "serial",
                            label: "Serial Number",
                            value: serial.read().clone(),
                            oninput: move |e: FormEvent| serial.set(e.value()),
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-3",
                        Input {
                            name: "manufacturer",
                            label: "Manufacturer",
                            value: manufacturer.read().clone(),
                            oninput: move |e: FormEvent| manufacturer.set(e.value()),
                        }
                        Input {
                            name: "model",
                            label: "Model",
                            value: model.read().clone(),
                            oninput: move |e: FormEvent| model.set(e.value()),
                        }
                        Input {
                            name: "warranty",
                            label: "Warranty Expires",
                            r#type: "date",
                            value: warranty.read().clone(),
                            oninput: move |e: FormEvent| warranty.set(e.value()),
                        }
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::AssetList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Create Asset"
                        }
                    }
                }
            }
        }
    }
}

/// Asset detail page
#[derive(Props, Clone, PartialEq)]
pub struct AssetDetailPageProps {
    pub id: String,
}

#[component]
pub fn AssetDetailPage(props: AssetDetailPageProps) -> Element {
    let id_for_asset = props.id.clone();
    let asset_resource = use_resource(move || {
        let id = id_for_asset.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<RemoteAsset>(&format!("/assets/{id}"))
                .await
                .ok()
        }
    });
    let id_for_rel = props.id.clone();
    let rel_resource = use_resource(move || {
        let id = id_for_rel.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteRelationship>>(&format!(
                "/assets/{id}/relationships"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let id_for_cfg = props.id.clone();
    let cfg_resource = use_resource(move || {
        let id = id_for_cfg.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<ConfigSummary>>(&format!(
                "/assets/{id}/configuration-items"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let id_for_cred = props.id.clone();
    let cred_resource = use_resource(move || {
        let id = id_for_cred.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<CredSummary>>(&format!(
                "/assets/{id}/credentials"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let id_for_audit = props.id.clone();
    let audit_resource = use_resource(move || {
        let id = id_for_audit.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<AuditEntry>>(&format!(
                "/assets/{id}/audit-log"
            ))
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
        }
    });
    let types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeOpt>>("/asset-types")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOpt>>("/contacts/companies")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    // On-demand audited reveals, keyed by item id.
    let revealed_creds = use_signal(HashMap::<uuid::Uuid, RevealedCred>::new);
    let revealed_cfgs = use_signal(HashMap::<uuid::Uuid, String>::new);

    let snapshot = asset_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let asset = snapshot.flatten();
    let relationships = rel_resource.read_unchecked().clone().unwrap_or_default();
    let config_items = cfg_resource.read_unchecked().clone().unwrap_or_default();
    let credentials = cred_resource.read_unchecked().clone().unwrap_or_default();
    let audit = audit_resource.read_unchecked().clone().unwrap_or_default();
    let types = types_resource.read_unchecked().clone().unwrap_or_default();
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let header_title = match asset.as_ref() {
        Some(a) if !a.name.trim().is_empty() => a.name.clone(),
        Some(_) => format!("Asset {}", props.id),
        None if is_loading => "Loading…".to_string(),
        None => "Asset".to_string(),
    };

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader { title: "{header_title}", subtitle: "Configuration item" }

            if is_loading {
                Card { p { class: "text-sm text-gray-400", "Loading asset…" } }
            } else if asset.is_none() {
                Card {
                    p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                        "Could not load this asset."
                    }
                }
            } else {
                {
                    let a = asset.clone().unwrap();
                    let (status_variant, status_label) = status_badge(&a.status);
                    let tname = a
                        .asset_type_id
                        .and_then(|tid| types.iter().find(|t| t.id == tid))
                        .map(|t| t.name.clone())
                        .unwrap_or_else(|| "-".to_string());
                    let cname = a
                        .company_id
                        .and_then(|cid| companies.iter().find(|c| c.id == cid))
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| "-".to_string());
                    let dash = || "-".to_string();
                    let manufacturer = a.manufacturer.clone().unwrap_or_else(dash);
                    let model = a.model.clone().unwrap_or_else(dash);
                    let serial = a.serial_number.clone().unwrap_or_else(dash);
                    let tag = a.asset_tag.clone().unwrap_or_else(dash);
                    let warranty = fmt_date(&a.warranty_expiry);
                    let purchased = fmt_date(&a.purchase_date);
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            // Main content
                            div { class: "lg:col-span-2 space-y-6",
                                Card { title: "Asset Information",
                                    dl { class: "grid grid-cols-2 gap-4",
                                        div {
                                            dt { class: "text-sm text-gray-500", "Type" }
                                            dd { class: "mt-1", "{tname}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Company" }
                                            dd { class: "mt-1", "{cname}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Manufacturer" }
                                            dd { class: "mt-1", "{manufacturer}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Model" }
                                            dd { class: "mt-1", "{model}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Serial Number" }
                                            dd { class: "mt-1 font-mono text-sm", "{serial}" }
                                        }
                                        div {
                                            dt { class: "text-sm text-gray-500", "Asset Tag" }
                                            dd { class: "mt-1 font-mono text-sm", "{tag}" }
                                        }
                                    }
                                }

                                // Relationships
                                Card { title: "Relationships",
                                    if relationships.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No relationships." }
                                    } else {
                                        div { class: "space-y-2",
                                            for r in relationships.iter() {
                                                {
                                                    let child = r
                                                        .child_asset_id
                                                        .map(|c| c.to_string())
                                                        .unwrap_or_default();
                                                    rsx! {
                                                        div { class: "flex items-center justify-between p-2 bg-gray-50 dark:bg-gray-800 rounded",
                                                            Badge { variant: BadgeVariant::Blue, "{r.relationship_type}" }
                                                            if !child.is_empty() {
                                                                Link {
                                                                    to: Route::AssetDetail { id: child.clone() },
                                                                    class: "text-sm text-blue-600 hover:text-blue-500 font-mono",
                                                                    "{child}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Configuration items (reveal on demand)
                                Card { title: "Configuration Items",
                                    if config_items.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No configuration items." }
                                    } else {
                                        div { class: "space-y-2",
                                            for ci in config_items.iter() {
                                                {
                                                    let cid = ci.id;
                                                    let category = ci.category.clone().unwrap_or_default();
                                                    let revealed = revealed_cfgs.read().get(&cid).cloned();
                                                    let path = format!("/configuration-items/{cid}");
                                                    let mut store = revealed_cfgs;
                                                    rsx! {
                                                        div { class: "flex items-center justify-between p-2 bg-gray-50 dark:bg-gray-800 rounded gap-3",
                                                            div { class: "min-w-0",
                                                                p { class: "font-medium text-sm text-gray-900 dark:text-white", "{ci.name}" }
                                                                if !category.is_empty() {
                                                                    p { class: "text-xs text-gray-500", "{category}" }
                                                                }
                                                                if let Some(val) = revealed {
                                                                    p { class: "font-mono text-sm text-gray-900 dark:text-white break-all mt-1", "{val}" }
                                                                }
                                                            }
                                                            Button {
                                                                variant: ButtonVariant::Secondary,
                                                                onclick: move |_| {
                                                                    let path = path.clone();
                                                                    spawn(async move {
                                                                        #[cfg(feature = "web")]
                                                                        if let Ok(c) = crate::hooks::fetch::api::get_authed::<RevealedConfig>(&path).await {
                                                                            store.write().insert(cid, c.value);
                                                                        }
                                                                    });
                                                                },
                                                                "Reveal"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Credentials (reveal on demand)
                                Card { title: "Credentials",
                                    if credentials.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No credentials." }
                                    } else {
                                        div { class: "space-y-2",
                                            for cr in credentials.iter() {
                                                {
                                                    let crid = cr.id;
                                                    let url = cr.url.clone().unwrap_or_default();
                                                    let revealed = revealed_creds.read().get(&crid).cloned();
                                                    let path = format!("/credentials/{crid}");
                                                    let mut store = revealed_creds;
                                                    rsx! {
                                                        div { class: "p-2 bg-gray-50 dark:bg-gray-800 rounded",
                                                            div { class: "flex items-center justify-between gap-3",
                                                                div { class: "min-w-0",
                                                                    p { class: "font-medium text-sm text-gray-900 dark:text-white", "{cr.name}" }
                                                                    p { class: "text-xs text-gray-500", "{cr.credential_type}" }
                                                                    if !url.is_empty() {
                                                                        p { class: "text-xs text-gray-500 break-all", "{url}" }
                                                                    }
                                                                }
                                                                Button {
                                                                    variant: ButtonVariant::Secondary,
                                                                    onclick: move |_| {
                                                                        let path = path.clone();
                                                                        spawn(async move {
                                                                            #[cfg(feature = "web")]
                                                                            if let Ok(c) = crate::hooks::fetch::api::get_authed::<RevealedCred>(&path).await {
                                                                                store.write().insert(crid, c);
                                                                            }
                                                                        });
                                                                    },
                                                                    "Reveal"
                                                                }
                                                            }
                                                            if let Some(c) = revealed {
                                                                div { class: "mt-2 space-y-1 font-mono text-sm text-gray-900 dark:text-white break-all",
                                                                    p { "user: {c.username}" }
                                                                    p { "pass: {c.password}" }
                                                                    if let Some(n) = c.notes.as_ref().filter(|s| !s.is_empty()) {
                                                                        p { class: "text-gray-500", "notes: {n}" }
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
                                            span { class: "text-gray-500", "Warranty" }
                                            span { class: "font-medium", "{warranty}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-gray-500", "Purchased" }
                                            span { class: "font-medium", "{purchased}" }
                                        }
                                    }
                                }

                                Card { title: "Audit Log",
                                    if audit.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No audit entries." }
                                    } else {
                                        div { class: "space-y-2 text-sm",
                                            for e in audit.iter().take(15) {
                                                {
                                                    let event = e
                                                        .changes
                                                        .as_ref()
                                                        .and_then(|c| c.get("event"))
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("")
                                                        .to_string();
                                                    let when = fmt_date(&e.performed_at);
                                                    rsx! {
                                                        div { class: "flex justify-between gap-2",
                                                            span { class: "text-gray-700 dark:text-gray-300",
                                                                if event.is_empty() {
                                                                    "{e.action}"
                                                                } else {
                                                                    "{e.action}: {event}"
                                                                }
                                                            }
                                                            span { class: "text-gray-400 whitespace-nowrap", "{when}" }
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
                }
            }
        }
    }
}

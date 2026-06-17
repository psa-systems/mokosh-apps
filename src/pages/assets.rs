//! Asset management pages (CMDB), wired to the assets API (PMS-71).

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, Input, Modal,
    PageHeader, PencilIcon, PlusIcon, SearchInput, Select, SelectOption, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::utils::Paginated;
use crate::Route;

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
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
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
    performed_by_id: Option<uuid::Uuid>,
    #[serde(default)]
    changes: Option<serde_json::Value>,
}

/// User option for resolving audit actor ids to display names (`/auth/users`).
#[derive(Clone, Debug, Deserialize)]
struct UserOpt {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
}

/// PMS-344: shallow ticket row for the "Related Tickets" section on the
/// asset detail page. Hits `GET /tickets?asset_id=<id>` and renders the
/// few fields a technician needs to recognise a ticket without leaving
/// the asset view: number, title, status, priority.
#[derive(Clone, Debug, Deserialize)]
struct RelatedTicket {
    id: uuid::Uuid,
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    status: RelatedTicketStatus,
    priority: RelatedTicketPriority,
}

#[derive(Clone, Debug, Deserialize)]
struct RelatedTicketStatus {
    #[serde(default)]
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RelatedTicketPriority {
    #[serde(default)]
    name: String,
}

/// The before/after value of one changed column (PMS-204). `update_asset`
/// stores these as an array in the audit row's `changes` column.
#[derive(Clone, Debug, Deserialize)]
struct FieldChange {
    #[serde(default)]
    field: String,
    #[serde(default)]
    old: Option<serde_json::Value>,
    #[serde(default)]
    new: Option<serde_json::Value>,
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

/// "Feb 28, 2025 3:45 PM" from an ISO datetime; falls back to the date-only
/// formatter, then the raw string. Used for audit timestamps.
/// PMS-253: honours the per-user format pref when set.
fn fmt_datetime(s: &Option<String>) -> String {
    match s {
        Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
            .map(|dt| {
                let utc = dt.with_timezone(&chrono::Utc);
                let pref = crate::utils::datetime::user_format_pref();
                match pref.as_deref().filter(|p| !p.trim().is_empty()) {
                    Some(fmt) => crate::utils::datetime::format_user_datetime(utc, Some(fmt)),
                    None => dt.format("%b %-d, %Y %-I:%M %p").to_string(),
                }
            })
            .unwrap_or_else(|_| fmt_date(s)),
        None => "-".to_string(),
    }
}

/// Resolve an actor id to a display name via the loaded user list; "-" when
/// unknown so the audit/edited markers never show a bare UUID.
fn actor_name(users: &[UserOpt], id: &Option<uuid::Uuid>) -> String {
    match id {
        Some(uid) => users
            .iter()
            .find(|u| &u.id == uid)
            .map(|u| u.full_name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "-".to_string()),
        None => "-".to_string(),
    }
}

/// Humanize an asset audit `action` code for display.
fn action_label(action: &str) -> String {
    match action {
        "created" => "Created".to_string(),
        "updated" => "Updated".to_string(),
        "status_changed" => "Status changed".to_string(),
        "credential_created" => "Credential added".to_string(),
        "credential_deleted" => "Credential removed".to_string(),
        "credential_revealed" => "Credential revealed".to_string(),
        "configuration_revealed" => "Configuration revealed".to_string(),
        other => {
            // snake_case to Sentence case fallback.
            let mut s = other.replace('_', " ");
            if let Some(first) = s.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            s
        }
    }
}

/// "warranty_expiry" to "Warranty expiry" for a single field name.
///
/// PMS-370: column names for foreign-key fields end in `_id`
/// (`asset_type_id`, `company_id`, `account_manager_id`). The audit log
/// records the raw column name, so without trimming the suffix the
/// change-history feed reads "Asset type id" / "Company id". Strip the
/// trailing `_id` first so future FK fields render cleanly without a
/// per-column allow-list.
fn title_field(f: &str) -> String {
    let trimmed = f.strip_suffix("_id").unwrap_or(f);
    let mut s = trimmed.replace('_', " ");
    if let Some(first) = s.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    s
}

/// A 36-char hyphenated UUID, not worth showing as before/after text.
fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes().iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Render an audit value for display: "(empty)" for null/blank, the trimmed
/// text (truncated) for strings, a coarse marker for references/objects.
fn fmt_change_value(v: &Option<serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => "(empty)".to_string(),
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                "(empty)".to_string()
            } else if looks_like_uuid(t) {
                "(reference)".to_string()
            } else if t.chars().count() > 160 {
                format!("{}…", t.chars().take(160).collect::<String>())
            } else {
                t.to_string()
            }
        }
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(_) => "(updated)".to_string(),
    }
}

/// `""` to `None`, otherwise `Some(trimmed)`. Lets an edit form send `null`
/// for a cleared optional field instead of an empty string.
fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
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

// Field caps for the New Asset form's free-text inputs (MAPPS-216). These
// mirror the mokosh-server column limits so over-long input is rejected
// inline (and via `maxlength`) instead of failing later as an opaque 422.
// Assumed to match the server's asset columns (VARCHAR(255)); revise if they
// differ.
const ASSET_NAME_MAX: usize = 255;
const ASSET_SERIAL_MAX: usize = 255;
const ASSET_MANUFACTURER_MAX: usize = 255;
const ASSET_MODEL_MAX: usize = 255;

/// Validate the required Name field (MAPPS-216): present, trimmed, within the
/// length cap. Returns the trimmed value or an inline message for the field.
fn validate_asset_name(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("Please enter an asset name.".to_string());
    }
    if t.chars().count() > ASSET_NAME_MAX {
        return Err(format!(
            "Name must be {ASSET_NAME_MAX} characters or fewer."
        ));
    }
    Ok(t.to_string())
}

/// Validate an optional, length-capped asset text field (MAPPS-216). Blank ->
/// `Ok(None)`; otherwise the trimmed value or an inline message for that field.
fn validate_asset_optional(raw: &str, label: &str, max: usize) -> Result<Option<String>, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.chars().count() > max {
        return Err(format!("{label} must be {max} characters or fewer."));
    }
    Ok(Some(t.to_string()))
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
    // Server/submit-time errors only (e.g. a failed POST). Required-field and
    // length validation no longer routes through this banner (MAPPS-216).
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-216): each failure is shown
    // under its own field and highlights that field, rather than a single
    // generic top banner. `name_error` also carries a server-flagged Name
    // validation message (MAPPS-210).
    let mut name_error = use_signal(String::new);
    let mut type_err = use_signal(String::new);
    let mut company_err = use_signal(String::new);
    let mut serial_err = use_signal(String::new);
    let mut manufacturer_err = use_signal(String::new);
    let mut model_err = use_signal(String::new);

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
                        name_error.set(String::new());
                        type_err.set(String::new());
                        company_err.set(String::new());
                        serial_err.set(String::new());
                        manufacturer_err.set(String::new());
                        model_err.set(String::new());

                        // Collect every field error before returning (MAPPS-216)
                        // so the user sees all problems at once, each attached
                        // to its own field, rather than the form aborting on
                        // the first failure.
                        let mut ok = true;

                        let asset_name = match validate_asset_name(&name.read()) {
                            Ok(v) => v,
                            Err(msg) => {
                                name_error.set(msg);
                                ok = false;
                                String::new()
                            }
                        };
                        let type_id = asset_type.read().clone();
                        if type_id.is_empty() {
                            type_err.set("Please pick an asset type.".to_string());
                            ok = false;
                        }
                        let company_id = company.read().clone();
                        if company_id.is_empty() {
                            company_err.set("Please pick a company.".to_string());
                            ok = false;
                        }
                        let serial_v = match validate_asset_optional(
                            &serial.read(),
                            "Serial number",
                            ASSET_SERIAL_MAX,
                        ) {
                            Ok(v) => v,
                            Err(msg) => {
                                serial_err.set(msg);
                                ok = false;
                                None
                            }
                        };
                        let manufacturer_v = match validate_asset_optional(
                            &manufacturer.read(),
                            "Manufacturer",
                            ASSET_MANUFACTURER_MAX,
                        ) {
                            Ok(v) => v,
                            Err(msg) => {
                                manufacturer_err.set(msg);
                                ok = false;
                                None
                            }
                        };
                        let model_v = match validate_asset_optional(
                            &model.read(),
                            "Model",
                            ASSET_MODEL_MAX,
                        ) {
                            Ok(v) => v,
                            Err(msg) => {
                                model_err.set(msg);
                                ok = false;
                                None
                            }
                        };
                        let warranty_v = warranty.read().clone();

                        if !ok {
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
                                if let Some(s) = serial_v {
                                    body["serial_number"] = serde_json::json!(s);
                                }
                                if let Some(m) = manufacturer_v {
                                    body["manufacturer"] = serde_json::json!(m);
                                }
                                if let Some(m) = model_v {
                                    body["model"] = serde_json::json!(m);
                                }
                                if !warranty_v.is_empty() {
                                    body["warranty_expiry"] = serde_json::json!(warranty_v);
                                }
                                match crate::hooks::fetch::api::post_authed_typed::<
                                        serde_json::Value,
                                        _,
                                    >("/assets", &body)
                                    .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::AssetList {});
                                    }
                                    Err(e) => {
                                        // Route a server-flagged Name validation
                                        // message next to that input; otherwise
                                        // show the general message (MAPPS-210).
                                        if let Some(msg) = e.field_message("name") {
                                            name_error.set(msg);
                                        } else {
                                            error
                                                .set(
                                                    format!("Could not create asset: {}", e.user_message()),
                                                );
                                        }
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
                            maxlength: ASSET_NAME_MAX as i64,
                            error: name_error.read().clone(),
                            value: name.read().clone(),
                            oninput: move |e: FormEvent| {
                                name_error.set(String::new());
                                name.set(e.value());
                            },
                        }
                        Select {
                            name: "type",
                            label: "Type",
                            options: type_options,
                            value: asset_type.read().clone(),
                            placeholder: "Select a type",
                            required: true,
                            error: type_err(),
                            onchange: move |e: FormEvent| {
                                type_err.set(String::new());
                                asset_type.set(e.value());
                            },
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
                            error: company_err(),
                            onchange: move |e: FormEvent| {
                                company_err.set(String::new());
                                company.set(e.value());
                            },
                        }
                        Input {
                            name: "serial",
                            label: "Serial Number",
                            maxlength: ASSET_SERIAL_MAX as i64,
                            error: serial_err(),
                            value: serial.read().clone(),
                            oninput: move |e: FormEvent| {
                                serial_err.set(String::new());
                                serial.set(e.value());
                            },
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-3",
                        Input {
                            name: "manufacturer",
                            label: "Manufacturer",
                            maxlength: ASSET_MANUFACTURER_MAX as i64,
                            error: manufacturer_err(),
                            value: manufacturer.read().clone(),
                            oninput: move |e: FormEvent| {
                                manufacturer_err.set(String::new());
                                manufacturer.set(e.value());
                            },
                        }
                        Input {
                            name: "model",
                            label: "Model",
                            maxlength: ASSET_MODEL_MAX as i64,
                            error: model_err(),
                            value: model.read().clone(),
                            oninput: move |e: FormEvent| {
                                model_err.set(String::new());
                                model.set(e.value());
                            },
                        }
                        crate::components::DateField {
                            name: "warranty",
                            label: "Warranty Expires",
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
    // PMS-344: tickets that reference this asset. Server-side filter on
    // `asset_id` was added in the same change; per_page=50 keeps the
    // payload one round trip without pagination UI since the typical
    // asset has only a handful of related tickets.
    let id_for_tickets = props.id.clone();
    let tickets_resource = use_resource(move || {
        let id = id_for_tickets.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<RelatedTicket>>(&format!(
                "/tickets?asset_id={id}&per_page=50"
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
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<UserOpt>>("/auth/users")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });

    // On-demand audited reveals, keyed by item id.
    let revealed_creds = use_signal(HashMap::<uuid::Uuid, RevealedCred>::new);
    let revealed_cfgs = use_signal(HashMap::<uuid::Uuid, String>::new);

    // PMS-185 edit state. `editing` drives the modal; the field signals are
    // populated from the loaded asset when the user opens it.
    let mut editing = use_signal(|| false);
    let mut e_name = use_signal(String::new);
    let mut e_tag = use_signal(String::new);
    let mut e_type = use_signal(String::new);
    let mut e_status = use_signal(String::new);
    let mut e_manufacturer = use_signal(String::new);
    let mut e_model = use_signal(String::new);
    let mut e_serial = use_signal(String::new);
    let mut e_warranty = use_signal(String::new);
    let mut e_purchase = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    let id_for_save = props.id.clone();

    // MAPPS-158: detail-page Delete, wired to the existing
    // `DELETE /assets/{id}` endpoint (parity with Company/Contract).
    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let id_for_delete = props.id.clone();

    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // actual DELETE fires from `on_confirm_delete` when confirmed.
    let mut confirming_delete = use_signal(|| false);

    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = id_for_delete.clone();
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/assets/{id}");
                if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                    navigator.push(Route::AssetList {});
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let snapshot = asset_resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let asset = snapshot.flatten();
    let relationships = rel_resource.read_unchecked().clone().unwrap_or_default();
    let config_items = cfg_resource.read_unchecked().clone().unwrap_or_default();
    let credentials = cred_resource.read_unchecked().clone().unwrap_or_default();
    let audit = audit_resource.read_unchecked().clone().unwrap_or_default();
    let related_tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let types = types_resource.read_unchecked().clone().unwrap_or_default();
    let companies = companies_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    // "Edited" marker: only when the asset has actually changed since
    // creation (updated_at strictly after created_at). The who/when comes
    // from the most recent audit entry, which `update_asset` writes.
    let was_edited = match (
        asset.as_ref().and_then(|a| a.created_at.as_ref()),
        asset.as_ref().and_then(|a| a.updated_at.as_ref()),
    ) {
        (Some(c), Some(u)) => u > c,
        _ => false,
    };
    let edited_label = if was_edited {
        audit.first().map(|e| {
            let who = actor_name(&users, &e.performed_by_id);
            let when = fmt_datetime(&e.performed_at);
            if who == "-" {
                format!("Edited {when}")
            } else {
                format!("Edited {when} by {who}")
            }
        })
    } else {
        None
    };

    let header_title = match asset.as_ref() {
        Some(a) if !a.name.trim().is_empty() => a.name.clone(),
        Some(_) => format!("Asset {}", props.id),
        None if is_loading => "Loading…".to_string(),
        None => "Asset".to_string(),
    };

    rsx! {
        AppLayout { title: "{header_title}",
            crate::components::ConfirmDialog {
                open: confirming_delete(),
                title: "Delete asset".to_string(),
                message: "Delete this asset? This cannot be undone.".to_string(),
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
                subtitle: "Configuration item",
                actions: rsx! {
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
                    // Snapshot used to seed the edit form when opened.
                    let a_edit = a.clone();
                    let open_edit = move |_| {
                        e_name.set(a_edit.name.clone());
                        e_tag.set(a_edit.asset_tag.clone().unwrap_or_default());
                        e_type
                            .set(a_edit.asset_type_id.map(|t| t.to_string()).unwrap_or_default());
                        e_status.set(a_edit.status.clone());
                        e_manufacturer.set(a_edit.manufacturer.clone().unwrap_or_default());
                        e_model.set(a_edit.model.clone().unwrap_or_default());
                        e_serial.set(a_edit.serial_number.clone().unwrap_or_default());
                        e_warranty.set(a_edit.warranty_expiry.clone().unwrap_or_default());
                        e_purchase.set(a_edit.purchase_date.clone().unwrap_or_default());
                        e_error.set(String::new());
                        editing.set(true);
                    };
                    let edited_marker = edited_label.clone();
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            // Main content
                            div { class: "lg:col-span-2 space-y-6",
                                Card {
                                    title: "Asset Information",
                                    actions: rsx! {
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            onclick: open_edit,
                                            PencilIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                            "Edit"
                                        }
                                    },
                                    if let Some(marker) = edited_marker {
                                        p { class: "text-xs text-gray-400 italic mb-3", "{marker}" }
                                    }
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

                                // PMS-344: tickets that reference this
                                // asset. Click-through to the ticket
                                // detail page; the asset detail and the
                                // ticket detail both surface the link
                                // (bidirectional).
                                Card { title: "Related Tickets",
                                    if related_tickets.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No related tickets." }
                                    } else {
                                        div { class: "space-y-2",
                                            for t in related_tickets.iter() {
                                                {
                                                    let tid = t.id.to_string();
                                                    let number = t.ticket_number.clone();
                                                    let title = t.title.clone();
                                                    let status_name = t.status.name.clone();
                                                    let priority_name = t.priority.name.clone();
                                                    rsx! {
                                                        div { class: "flex items-center justify-between p-2 bg-gray-50 dark:bg-gray-800 rounded gap-3",
                                                            div { class: "min-w-0 flex-1",
                                                                Link {
                                                                    to: Route::TicketDetail { id: tid.clone() },
                                                                    class: "text-sm font-medium text-blue-600 hover:text-blue-500",
                                                                    if !number.is_empty() {
                                                                        span { class: "font-mono mr-2", "{number}" }
                                                                    }
                                                                    span { "{title}" }
                                                                }
                                                            }
                                                            div { class: "flex items-center gap-2 shrink-0",
                                                                if !priority_name.is_empty() {
                                                                    Badge { variant: BadgeVariant::Gray, "{priority_name}" }
                                                                }
                                                                if !status_name.is_empty() {
                                                                    Badge { variant: BadgeVariant::Blue, "{status_name}" }
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

                                Card { title: "Change History",
                                    if audit.is_empty() {
                                        p { class: "text-sm text-gray-400 italic", "No history yet." }
                                    } else {
                                        div { class: "space-y-3 text-sm",
                                            for e in audit.iter().take(15) {
                                                {
                                                    // `changes` is an object {event:...} for reveal ops,
                                                    // or an array of {field, old, new} for edits (PMS-204).
                                                    let event = e
                                                        .changes
                                                        .as_ref()
                                                        .and_then(|c| c.get("event"))
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("")
                                                        .to_string();
                                                    let field_changes: Vec<FieldChange> = e
                                                        .changes
                                                        .as_ref()
                                                        .and_then(|c| {
                                                            serde_json::from_value::<Vec<FieldChange>>(c.clone()).ok()
                                                        })
                                                        .unwrap_or_default();
                                                    let label = action_label(&e.action);
                                                    let when = fmt_datetime(&e.performed_at);
                                                    let who = actor_name(&users, &e.performed_by_id);
                                                    rsx! {
                                                        div { class: "flex justify-between gap-2",
                                                            div { class: "min-w-0",
                                                                p { class: "text-gray-700 dark:text-gray-300",
                                                                    if event.is_empty() {
                                                                        "{label}"
                                                                    } else {
                                                                        "{label}: {event}"
                                                                    }
                                                                }
                                                                if who != "-" {
                                                                    p { class: "text-xs text-gray-400", "by {who}" }
                                                                }
                                                                for c in field_changes.iter() {
                                                                    {
                                                                        let old = fmt_change_value(&c.old);
                                                                        let new = fmt_change_value(&c.new);
                                                                        let fname = title_field(&c.field);
                                                                        if old == "(reference)" && new == "(reference)" {
                                                                            rsx! {}
                                                                        } else {
                                                                            rsx! {
                                                                                p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                                                                    span { class: "font-medium", "{fname}: " }
                                                                                    span { class: "line-through text-gray-400", "{old}" }
                                                                                    " → "
                                                                                    span { "{new}" }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
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

            // PMS-185 edit modal.
            {
                let mut asset_res = asset_resource;
                let mut audit_res = audit_resource;
                let status_opts = vec![
                    SelectOption::new("active", "Active"),
                    SelectOption::new("in_stock", "In Stock"),
                    SelectOption::new("in_repair", "In Repair"),
                    SelectOption::new("retired", "Retired"),
                    SelectOption::new("inactive", "Inactive"),
                ];
                let type_opts: Vec<SelectOption> = types
                    .iter()
                    .map(|t| SelectOption::new(t.id.to_string(), t.name.clone()))
                    .collect();
                let save_id = id_for_save.clone();
                let on_save = move |_| {
                    if e_submitting() {
                        return;
                    }
                    let save_id = save_id.clone();
                    spawn(async move {
                        // Mirror the create-form validation (MAPPS-238/216):
                        // reject a blank or over-long name and over-long
                        // optional fields before the PUT, with an inline
                        // message, instead of posting them to the server.
                        let asset_name = match validate_asset_name(&e_name()) {
                            Ok(v) => v,
                            Err(msg) => {
                                e_error.set(msg);
                                return;
                            }
                        };
                        if let Err(msg) =
                            validate_asset_optional(&e_serial(), "Serial number", ASSET_SERIAL_MAX)
                        {
                            e_error.set(msg);
                            return;
                        }
                        if let Err(msg) = validate_asset_optional(
                            &e_manufacturer(),
                            "Manufacturer",
                            ASSET_MANUFACTURER_MAX,
                        ) {
                            e_error.set(msg);
                            return;
                        }
                        if let Err(msg) =
                            validate_asset_optional(&e_model(), "Model", ASSET_MODEL_MAX)
                        {
                            e_error.set(msg);
                            return;
                        }
                        e_submitting.set(true);
                        e_error.set(String::new());
                        let mut body = serde_json::Map::new();
                        body.insert("name".into(), serde_json::json!(asset_name));
                        body.insert("asset_tag".into(), serde_json::json!(opt_str(&e_tag())));
                        body.insert("asset_type_id".into(), serde_json::json!(opt_str(&e_type())));
                        body.insert("status".into(), serde_json::json!(e_status()));
                        body.insert(
                            "manufacturer".into(),
                            serde_json::json!(opt_str(&e_manufacturer())),
                        );
                        body.insert("model".into(), serde_json::json!(opt_str(&e_model())));
                        body.insert("serial_number".into(), serde_json::json!(opt_str(&e_serial())));
                        body.insert(
                            "warranty_expiry".into(),
                            serde_json::json!(opt_str(&e_warranty())),
                        );
                        body.insert(
                            "purchase_date".into(),
                            serde_json::json!(opt_str(&e_purchase())),
                        );
                        let body = serde_json::Value::Object(body);
                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                            &format!("/assets/{save_id}"),
                            &body,
                        )
                        .await
                        {
                            Ok(_) => {
                                e_submitting.set(false);
                                editing.set(false);
                                asset_res.restart();
                                audit_res.restart();
                            }
                            Err(err) => {
                                e_submitting.set(false);
                                e_error.set(err);
                            }
                        }
                    });
                };
                rsx! {
                    Modal {
                        open: editing(),
                        title: "Edit Asset",
                        onclose: move |_| editing.set(false),
                        footer: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| editing.set(false),
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: e_submitting(),
                                onclick: on_save,
                                if e_submitting() { "Saving…" } else { "Save Changes" }
                            }
                        },
                        div { class: "space-y-4",
                            if !e_error().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{e_error}" }
                            }
                            Input {
                                name: "edit-name",
                                label: "Name",
                                required: true,
                                maxlength: ASSET_NAME_MAX as i64,
                                value: "{e_name}",
                                oninput: move |e: FormEvent| e_name.set(e.value()),
                            }
                            div { class: "grid grid-cols-2 gap-4",
                                Select {
                                    name: "edit-type",
                                    label: "Type",
                                    options: type_opts.clone(),
                                    value: "{e_type}",
                                    placeholder: "Select type",
                                    onchange: move |e: FormEvent| e_type.set(e.value()),
                                }
                                Select {
                                    name: "edit-status",
                                    label: "Status",
                                    options: status_opts.clone(),
                                    value: "{e_status}",
                                    onchange: move |e: FormEvent| e_status.set(e.value()),
                                }
                            }
                            Input {
                                name: "edit-tag",
                                label: "Asset Tag",
                                value: "{e_tag}",
                                oninput: move |e: FormEvent| e_tag.set(e.value()),
                            }
                            div { class: "grid grid-cols-2 gap-4",
                                Input {
                                    name: "edit-manufacturer",
                                    label: "Manufacturer",
                                    maxlength: ASSET_MANUFACTURER_MAX as i64,
                                    value: "{e_manufacturer}",
                                    oninput: move |e: FormEvent| e_manufacturer.set(e.value()),
                                }
                                Input {
                                    name: "edit-model",
                                    label: "Model",
                                    maxlength: ASSET_MODEL_MAX as i64,
                                    value: "{e_model}",
                                    oninput: move |e: FormEvent| e_model.set(e.value()),
                                }
                            }
                            Input {
                                name: "edit-serial",
                                label: "Serial Number",
                                maxlength: ASSET_SERIAL_MAX as i64,
                                value: "{e_serial}",
                                oninput: move |e: FormEvent| e_serial.set(e.value()),
                            }
                            div { class: "grid grid-cols-2 gap-4",
                                crate::components::DateField {
                                    name: "edit-purchase",
                                    label: "Purchase Date",
                                    value: "{e_purchase}",
                                    oninput: move |e: FormEvent| e_purchase.set(e.value()),
                                }
                                crate::components::DateField {
                                    name: "edit-warranty",
                                    label: "Warranty Expiry",
                                    value: "{e_warranty}",
                                    oninput: move |e: FormEvent| e_warranty.set(e.value()),
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
mod validation_tests {
    use super::{validate_asset_name, validate_asset_optional, ASSET_NAME_MAX, ASSET_SERIAL_MAX};

    #[test]
    fn name_required_and_trimmed() {
        assert!(validate_asset_name("   ").is_err());
        assert_eq!(validate_asset_name("  Server 01  ").unwrap(), "Server 01");
    }

    #[test]
    fn name_capped() {
        assert!(validate_asset_name(&"x".repeat(ASSET_NAME_MAX)).is_ok());
        assert!(validate_asset_name(&"x".repeat(ASSET_NAME_MAX + 1)).is_err());
    }

    #[test]
    fn optional_blank_is_none() {
        assert_eq!(
            validate_asset_optional("   ", "Serial number", ASSET_SERIAL_MAX).unwrap(),
            None
        );
    }

    #[test]
    fn optional_trimmed_and_capped() {
        assert_eq!(
            validate_asset_optional("  SN-1  ", "Serial number", ASSET_SERIAL_MAX).unwrap(),
            Some("SN-1".to_string())
        );
        assert!(validate_asset_optional(
            &"x".repeat(ASSET_SERIAL_MAX + 1),
            "Serial number",
            ASSET_SERIAL_MAX
        )
        .is_err());
    }
}

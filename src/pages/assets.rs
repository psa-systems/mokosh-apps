//! Asset management pages (CMDB), wired to the assets API (PMS-71).

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    asset_status_badge, use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card,
    DataTable, ErrorBanner, IconSize, Input, Modal, PageHeader, PencilIcon, PlusIcon, SearchInput,
    Select, SelectOption, Table, TableBody, TableCell, TableEmptyRow, TableHead, TableHeader,
    TableRow, Textarea, TrashIcon,
};
use crate::utils::{FormGuard, Paginated, Rule};
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
    // PMS-454: CMDB expansion fields. Each `#[serde(default)]` so an
    // older server that doesn't ship the column still deserialises
    // (the server-side migration adds them as nullable). The bare
    // `assigned_user_id` UUID is kept on the wire shape but the UI
    // only reads `assigned_user_name`; `#[allow(dead_code)]` documents
    // it as preserved-for-roundtripping rather than dropped.
    #[serde(default)]
    #[allow(dead_code)]
    assigned_user_id: Option<uuid::Uuid>,
    #[serde(default)]
    assigned_user_name: Option<String>,
    #[serde(default)]
    ip_address: Option<String>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    mac_address: Option<String>,
    #[serde(default)]
    installed_date: Option<String>,
    #[serde(default)]
    department: Option<String>,
    #[serde(default)]
    in_transit_ticket_id: Option<uuid::Uuid>,
    // PMS-476 / PMS-456: per-CI lifecycle position (planned /
    // in_service / retired, or a tenant-coined value). Free-text so
    // a tenant can coin a stage; `None` renders as "Unknown".
    #[serde(default)]
    itil_lifecycle_stage: Option<String>,
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
    // MAPPS-233: the relationship row's own id, needed to address it for
    // `DELETE /asset-relationships/{id}` from the Remove control. Optional so a
    // response that omits it simply renders without a Remove button rather than
    // failing to deserialize the whole list.
    #[serde(default)]
    id: Option<uuid::Uuid>,
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

/// MAPPS-304: render-side state for the asset Change History panel. Lets
/// the panel distinguish "still loading", "loaded, no entries", and
/// "fetch failed" - previously every fetch outcome collapsed into a
/// silent empty list that rendered "No history yet" even on permission
/// or network failures.
#[derive(Clone, Debug)]
enum AuditPanelState {
    Loading,
    Ready(Vec<AuditEntry>),
    Failed(String),
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

/// MAPPS-305: the four bucket states the asset-detail Warranty row keys
/// its "Needs refresh" badge on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WarrantyRefreshStatus {
    /// The warranty date is unset or unparseable.
    Unknown,
    /// More than `WARRANTY_REFRESH_THRESHOLD_DAYS` until expiry.
    Healthy,
    /// Within `WARRANTY_REFRESH_THRESHOLD_DAYS` of expiry.
    ExpiringSoon,
    /// Warranty date is in the past.
    Expired,
}

const WARRANTY_REFRESH_THRESHOLD_DAYS: i64 = 30;

/// Compute the refresh-status bucket from a server-formatted (`YYYY-MM-DD`)
/// warranty date string compared to today (in the user's timezone, via
/// `user_today`). Pure read-only - the field's not mutated, the cue is
/// derived at render time, no scheduled job needed.
fn warranty_refresh_status(s: &Option<String>) -> WarrantyRefreshStatus {
    let Some(raw) = s else {
        return WarrantyRefreshStatus::Unknown;
    };
    let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") else {
        return WarrantyRefreshStatus::Unknown;
    };
    let today = crate::utils::datetime::user_today();
    let days_until = (date - today).num_days();
    if days_until < 0 {
        WarrantyRefreshStatus::Expired
    } else if days_until <= WARRANTY_REFRESH_THRESHOLD_DAYS {
        WarrantyRefreshStatus::ExpiringSoon
    } else {
        WarrantyRefreshStatus::Healthy
    }
}

/// Asset list page
#[component]
pub fn AssetListPage() -> Element {
    // PMS-745: row-level navigation for the list below.
    let navigator = use_navigator();
    let mut search = use_signal(String::new);
    // MAPPS-303: page-scoped bulk selection (built on MAPPS-290's
    // `use_bulk_selection`). Drives the per-row checkbox, the
    // `SelectAllHeader`, and the `BulkActionsBar` "Bulk edit" verb
    // below; the bar opens a modal whose submit fires N parallel
    // `PUT /assets/{id}` calls.
    let mut selection = crate::components::use_bulk_selection();
    let mut bulk_modal_open = use_signal(|| false);
    let mut bulk_change_status = use_signal(|| false);
    let mut bulk_status = use_signal(|| "active".to_string());
    let mut bulk_change_company = use_signal(|| false);
    let mut bulk_company_id = use_signal(String::new);
    let mut bulk_company_name = use_signal(String::new);
    let mut bulk_submitting = use_signal(|| false);
    let mut bulk_error = use_signal(String::new);
    // MAPPS-313: bulk-delete confirmation state for the assets list.
    // Mirrors the Tickets bulk-delete pattern from MAPPS-310: snapshot
    // the selection at click time so a mid-dialog uncheck cannot
    // smuggle a row past the prompt.
    let mut bulk_delete_confirm = use_signal::<Option<Vec<String>>>(|| None);
    let mut bulk_delete_running = use_signal(|| false);

    // MAPPS-249: scope to one company when a context card's "View All" passes
    // `?company_id=<uuid>`.
    // MAPPS-357: primary resource. Kept as a hand-rolled `use_resource` (not
    // `use_remote_resource`) because the bulk edit / delete flows call
    // `assets_resource.restart()`. The fetcher keeps `.ok()` (NOT
    // `.unwrap_or_default()`) so a failed load stays distinguishable from an
    // empty list, letting the outage render ContentUnavailable below, and it
    // subscribes to reachability so the list auto-refetches on reconnect.
    let mut assets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        let mut path = String::from("/assets");
        if let Some(company_id) = crate::utils::url::current_query_param("company_id") {
            path.push_str(&format!("?company_id={company_id}"));
        }
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteAsset>>(&path)
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

    use_page_title("Assets");

    // MAPPS-357: a failed primary load while the server is flagged down is an
    // outage, not an empty CMDB - render the honest unavailable state (which
    // keeps the nav + banner) instead of an empty assets table. A fetch that
    // fails while still reachable (a 4xx) keeps the inline "Could not load"
    // notice below. `can_mutate` disables the bulk write controls while down.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if load_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Assets".to_string() }
        };
    }

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

        // MAPPS-321: scope indicator (see ticket for the rationale).
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Assets,
        }

        Card { class: "mb-6",
            SearchInput {
                value: search.read().clone(),
                placeholder: "Search by name or serial…",
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

        // MAPPS-303 + MAPPS-313: bulk-edit + bulk-delete affordances.
        // Both render only when at least one asset is selected.
        crate::components::BulkActionsBar {
            selection,
            label: "asset".to_string(),
            Button {
                variant: ButtonVariant::Primary,
                // MAPPS-357: block bulk edit while the server is down.
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't bulk edit while the server is unreachable".to_string()),
                onclick: move |_| {
                    // Reset per-open form state.
                    bulk_change_status.set(false);
                    bulk_change_company.set(false);
                    bulk_company_id.set(String::new());
                    bulk_company_name.set(String::new());
                    bulk_error.set(String::new());
                    bulk_modal_open.set(true);
                },
                "Bulk edit"
            }
            Button {
                variant: ButtonVariant::Danger,
                // MAPPS-357: block bulk delete while the server is down.
                disabled: bulk_delete_running() || !can_mutate,
                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                onclick: move |_| {
                    let ids: Vec<String> = selection.read().iter().cloned().collect();
                    if !ids.is_empty() {
                        bulk_delete_confirm.set(Some(ids));
                    }
                },
                "Delete selected"
            }
        }
        // MAPPS-313: confirmation dialog for the bulk delete.
        {
            let pending = bulk_delete_confirm.read().clone();
            let pending_count = pending.as_ref().map(|v| v.len()).unwrap_or(0);
            let dialog_message = format!(
                "Delete {pending_count} selected asset(s)? Credentials, configuration items, and relationships on these assets are also removed. This cannot be undone."
            );
            let confirm_text = format!("Delete {pending_count} asset(s)");
            rsx! {
                crate::components::ConfirmDialog {
                    open: pending.is_some(),
                    title: "Delete selected assets".to_string(),
                    message: dialog_message,
                    confirm_text,
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: bulk_delete_running(),
                    onconfirm: move |_| {
                        let Some(ids) = bulk_delete_confirm.read().clone() else { return };
                        if ids.is_empty() || bulk_delete_running() { return; }
                        bulk_delete_running.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use futures_util::future::join_all;
                                let futs = ids.iter().map(|id| {
                                    let path = format!("/assets/{id}");
                                    async move {
                                        crate::hooks::fetch::api::delete_authed(&path).await
                                    }
                                });
                                let results = join_all(futs).await;
                                let failures = results.iter().filter(|r| r.is_err()).count();
                                if failures == 0 {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Success,
                                        format!("Deleted {} asset(s).", ids.len()),
                                    );
                                } else {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Error,
                                        format!("Deleted {} of {}; {} failed.", ids.len() - failures, ids.len(), failures),
                                    );
                                }
                            }
                            crate::components::clear_selection(&mut selection);
                            assets_resource.restart();
                            bulk_delete_confirm.set(None);
                            bulk_delete_running.set(false);
                        });
                    },
                    oncancel: move |_| {
                        if !bulk_delete_running() {
                            bulk_delete_confirm.set(None);
                        }
                    },
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
                        // MAPPS-303: select-all checkbox for the page.
                        crate::components::SelectAllHeader {
                            selection,
                            ids: filtered.iter().map(|a| a.id.to_string()).collect::<Vec<_>>(),
                        }
                        TableHeader { "Asset" }
                        TableHeader { "Type" }
                        TableHeader { "Company" }
                        TableHeader { "Serial / Tag" }
                        TableHeader { "Status" }
                    }
                }
                TableBody {
                    if is_loading {
                        TableRow { TableCell { class: "text-subtle", "Loading…" } }
                    } else if filtered.is_empty() {
                        // MAPPS-388: centered across the table, not left-aligned.
                        TableEmptyRow { columns: 5, class: "text-subtle italic",
                            if assets.is_empty() {
                                "No assets yet. Create one to get started."
                            } else {
                                "No assets match the search."
                            }
                        }
                    } else {
                        for a in filtered.iter() {
                            {
                                let (variant, label) = asset_status_badge(&a.status);
                                let tname = type_name(&a.asset_type_id);
                                let cname = company_name(&a.company_id);
                                let serial = a
                                    .serial_number
                                    .clone()
                                    .or_else(|| a.asset_tag.clone())
                                    .unwrap_or_else(|| "-".to_string());
                                let aid = a.id.to_string();
                                let row_id = aid.clone();
                                rsx! {
                                    TableRow { key: "{aid}",
                                        // PMS-745: the whole row navigates, matching
                                        // ContractRow / TicketRow. `clickable` also
                                        // restores the hover background and pointer
                                        // cursor, which TableRow scopes to interactive
                                        // rows (MAPPS-389).
                                        clickable: true,
                                        onclick: move |_| {
                                            navigator.push(Route::AssetDetail { id: row_id.clone() });
                                        },
                                        // MAPPS-303: per-row checkbox. Its cell stops
                                        // propagation, so selecting a row does not also
                                        // open it.
                                        crate::components::SelectRowCell {
                                            selection,
                                            id: aid.clone(),
                                        }
                                        TableCell {
                                            Link {
                                                to: Route::AssetDetail { id: aid.clone() },
                                                class: "font-medium text-accent hover:opacity-90",
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

        // MAPPS-303: bulk-edit modal. Hidden until the BulkActionsBar
        // verb opens it. Each field is gated on its own "Change this
        // field" checkbox so the user can change a subset
        // (Location-only is the QA's office-move use case). Submit
        // builds a minimal partial body and fires N parallel PUTs.
        if bulk_modal_open() {
            {
                let status_opts = vec![
                    SelectOption::new("active", "Active"),
                    SelectOption::new("in_stock", "In Stock"),
                    SelectOption::new("in_repair", "In Repair"),
                    SelectOption::new("retired", "Retired"),
                    SelectOption::new("inactive", "Inactive"),
                ];
                let count = selection.read().len();
                let count_label = if count == 1 {
                    "1 asset".to_string()
                } else {
                    format!("{} assets", count)
                };
                let company_picker_selected_id = if bulk_company_id.read().is_empty() {
                    None
                } else {
                    Some(bulk_company_id.read().clone())
                };
                rsx! {
                    Modal {
                        open: true,
                        title: format!("Bulk edit ({})", count_label),
                        onclose: move |_| {
                            if !bulk_submitting() {
                                bulk_modal_open.set(false);
                            }
                        },
                        footer: rsx! {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| {
                                    if !bulk_submitting() {
                                        bulk_modal_open.set(false);
                                    }
                                },
                                "Cancel"
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                // MAPPS-357: block the bulk PUT while the server is down.
                                disabled: bulk_submitting() || !can_mutate,
                                loading: bulk_submitting(),
                                title: (!can_mutate).then(|| "Can't apply changes while the server is unreachable".to_string()),
                                onclick: move |_| {
                                    // Validate that at least one field is being changed.
                                    if !bulk_change_status() && !bulk_change_company() {
                                        bulk_error.set("Pick at least one field to change.".to_string());
                                        return;
                                    }
                                    // Build the partial body.
                                    let mut body = serde_json::Map::new();
                                    if bulk_change_status() {
                                        body.insert("status".into(), serde_json::json!(bulk_status.read().as_str()));
                                    }
                                    if bulk_change_company() {
                                        match uuid::Uuid::parse_str(bulk_company_id.read().as_str()) {
                                            Ok(cid) => {
                                                body.insert("company_id".into(), serde_json::json!(cid));
                                            }
                                            Err(_) => {
                                                bulk_error.set("Pick a company first.".to_string());
                                                return;
                                            }
                                        }
                                    }
                                    let body = serde_json::Value::Object(body);
                                    let ids: Vec<String> = selection.read().iter().cloned().collect();
                                    bulk_submitting.set(true);
                                    bulk_error.set(String::new());
                                    spawn(async move {
                                        #[cfg(feature = "web")]
                                        {
                                            use futures_util::future::join_all;
                                            let futs = ids.iter().map(|id| {
                                                let path = format!("/assets/{id}");
                                                let body = body.clone();
                                                async move {
                                                    crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await
                                                }
                                            });
                                            let results = join_all(futs).await;
                                            let failures = results.iter().filter(|r| r.is_err()).count();
                                            if failures == 0 {
                                                crate::hooks::toast::push_toast(
                                                    crate::components::AlertType::Success,
                                                    format!("Updated {} asset(s).", ids.len()),
                                                );
                                            } else {
                                                crate::hooks::toast::push_toast(
                                                    crate::components::AlertType::Error,
                                                    format!("Updated {} of {}; {} failed.", ids.len() - failures, ids.len(), failures),
                                                );
                                            }
                                        }
                                        bulk_submitting.set(false);
                                        bulk_modal_open.set(false);
                                        crate::components::clear_selection(&mut selection);
                                        assets_resource.restart();
                                    });
                                },
                                "Apply to selected"
                            }
                        },
                        div { class: "space-y-4",
                            if !bulk_error.read().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{bulk_error}" }
                            }
                            p { class: "text-sm text-muted",
                                "Each field below is changed only when its checkbox is on. Anything left unchecked stays as-is on every selected asset."
                            }
                            div { class: "space-y-3",
                                crate::components::Checkbox {
                                    name: "bulk_change_status",
                                    label: "Change Status",
                                    checked: bulk_change_status(),
                                    onchange: move |e: FormEvent| bulk_change_status.set(e.checked()),
                                }
                                if bulk_change_status() {
                                    Select {
                                        name: "bulk_status",
                                        label: "Status".to_string(),
                                        options: status_opts.clone(),
                                        value: bulk_status.read().clone(),
                                        onchange: move |e: FormEvent| bulk_status.set(e.value()),
                                    }
                                }
                            }
                            div { class: "space-y-3",
                                crate::components::Checkbox {
                                    name: "bulk_change_company",
                                    label: "Change Company",
                                    checked: bulk_change_company(),
                                    onchange: move |e: FormEvent| bulk_change_company.set(e.checked()),
                                }
                                if bulk_change_company() {
                                    crate::components::CompanyPicker {
                                        value: bulk_company_name.read().clone(),
                                        selected_id: company_picker_selected_id,
                                        onselect: move |(id, name): (String, String)| {
                                            bulk_company_id.set(id);
                                            bulk_company_name.set(name);
                                        },
                                        onclear: move |_| {
                                            bulk_company_id.set(String::new());
                                            bulk_company_name.set(String::new());
                                        },
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

// MAPPS-231: cap the credential Name field to the server's vault limit
// (`CreateCredentialRequest.name` is `length(min = 1, max = 100)` in
// mokosh-server) so an over-long name is rejected inline instead of failing
// as a 422.
const CRED_NAME_MAX: usize = 100;

/// Validate the credential Name field (MAPPS-231): present, trimmed, within
/// the server's 100-char cap. Returns the trimmed value or an inline message.
fn validate_cred_name(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("Please enter a name.".to_string());
    }
    if t.chars().count() > CRED_NAME_MAX {
        return Err(format!("Name must be {CRED_NAME_MAX} characters or fewer."));
    }
    Ok(t.to_string())
}

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
    // MAPPS-300: pre-fill `company` from the URL so the Company detail
    // "New Asset" CTA lands on a form already scoped to that company.
    let mut company =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    // PMS-352 AC3: `company` holds the selected company UUID; CompanyPicker
    // reports the display name back here so the picker can render the chosen
    // company and a tenant with no companies can create one inline.
    let mut company_name = use_signal(String::new);
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
    // PMS-352 AC3: company is now chosen via CompanyPicker (which fetches and
    // filters its own company list with an inline-create affordance), so the
    // page no longer builds a company Select option list.
    let types = types_resource.read_unchecked().clone().unwrap_or_default();

    let mut type_options = vec![SelectOption::new("", "Select a type")];
    type_options.extend(
        types
            .iter()
            .map(|t| SelectOption::new(t.id.to_string(), t.name.clone())),
    );

    // Feed CompanyPicker its "already selected" state: Some(id) when a company
    // is picked (renders the selected chip), None otherwise (search dropdown).
    let company_picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company.read().as_str()).is_ok() {
            Some(company.read().clone())
        } else {
            None
        };

    let err = error.read().clone();

    // MAPPS-357: this is a create form, so it has no primary data resource to
    // gate a ContentUnavailable state on - `types_resource` is a secondary
    // dropdown lookup that degrades to its default (an empty type list) on
    // failure. During an outage the form still renders; only the Create submit
    // is blocked via `can_mutate` so a POST is not attempted against a server
    // that is known to be unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    use_page_title("New Asset");

    rsx! {
        PageHeader {
            title: "New Asset",
            subtitle: "Add a new configuration item",
            // MAPPS-294: breadcrumb back to the Assets list.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: vec![
                        crate::components::BreadcrumbItem {
                            label: "Assets".to_string(),
                            route: Some(Route::AssetList {}),
                        },
                        crate::components::BreadcrumbItem {
                            label: "New Asset".to_string(),
                            route: None,
                        },
                    ],
                }
            },
        }

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

                    // PMS-518: validate every required field through the
                    // shared FormGuard so all failures surface at once (each
                    // in its own inline slot) and the first invalid field is
                    // focused. Keeps the bespoke validators that also return
                    // the trimmed/typed value used to build the body.
                    let mut guard = FormGuard::new();

                    let asset_name = match validate_asset_name(&name.read()) {
                        Ok(v) => v,
                        Err(msg) => {
                            name_error.set(msg);
                            guard.note_invalid(Some("name"));
                            String::new()
                        }
                    };
                    let type_id = asset_type.read().clone();
                    if type_id.is_empty() {
                        type_err.set("Please pick an asset type.".to_string());
                        guard.note_invalid(Some("type"));
                    }
                    let company_id = company.read().clone();
                    if company_id.is_empty() {
                        company_err.set("Please pick a company.".to_string());
                        // CompanyPicker has no focusable field id, so block
                        // without a focus target.
                        guard.note_invalid(None);
                    }
                    let serial_v = match validate_asset_optional(
                        &serial.read(),
                        "Serial number",
                        ASSET_SERIAL_MAX,
                    ) {
                        Ok(v) => v,
                        Err(msg) => {
                            serial_err.set(msg);
                            guard.note_invalid(Some("serial"));
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
                            guard.note_invalid(Some("manufacturer"));
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
                            guard.note_invalid(Some("model"));
                            None
                        }
                    };
                    let warranty_v = warranty.read().clone();

                    if guard.blocked() {
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
                    ErrorBanner { "{err}" }
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
                    // PMS-352 AC3: CompanyPicker (with inline create) so a
                    // tenant with no companies can create one without
                    // leaving the New Asset form. CompanyPicker has no
                    // error prop, so surface company_err just below it.
                    div { class: "space-y-1",
                        crate::components::CompanyPicker {
                            value: company_name.read().clone(),
                            selected_id: company_picker_selected_id,
                            required: true,
                            allow_inline_create: true,
                            onselect: move |(id, name): (String, String)| {
                                company.set(id);
                                company_name.set(name);
                                company_err.set(String::new());
                            },
                            onclear: move |_| {
                                company.set(String::new());
                                company_name.set(String::new());
                            },
                        }
                        if !company_err().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{company_err}" }
                        }
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
                        // MAPPS-357: block create while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create an asset while the server is unreachable".to_string()),
                        "Create Asset"
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
    // MAPPS-357: primary resource - the fetched asset entity. Kept as a
    // hand-rolled `use_resource` (not `use_remote_resource`) because the edit /
    // delete flows call `asset_resource.restart()`. The fetcher keeps `.ok()`
    // (NOT a default) so a failed load stays distinguishable from a real asset,
    // letting the outage render ContentUnavailable below, and it subscribes to
    // reachability so the entity auto-refetches on reconnect.
    let id_for_asset = props.id.clone();
    let asset_resource = use_resource(move || {
        let id = id_for_asset.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
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
    // MAPPS-304: surface the audit-log fetch outcome as `Result` so the
    // panel can render "Loading…", a real entry list, an empty-list state,
    // and a permission / network failure distinctly. Before, every failure
    // (including the 403 `RequireAdmin` gate on `/assets/{id}/audit-log`
    // that a non-admin role hits) was silently collapsed into the
    // `unwrap_or_default()` empty branch, which rendered as "No history
    // yet" - the QA report's "Change history does not work at all". The
    // sibling PMS-447 single-tenancy admin floor stops most users from
    // hitting that gate, but this distinguishes the remaining failure
    // cases (network drop, future role downgrade) from a genuinely-empty
    // audit log.
    let audit_resource: Resource<Result<Vec<AuditEntry>, String>> = use_resource(move || {
        let id = id_for_audit.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<AuditEntry>>(&format!(
                "/assets/{id}/audit-log"
            ))
            .await
            .map(|p| p.data)
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
    // PMS-476: ITIL CI lifecycle stage. Free-text so a tenant can
    // coin a stage; placeholder + help text surface the standard set.
    let mut e_itil_stage = use_signal(String::new);
    // PMS-473: CMDB expansion fields. Every one is optional on the
    // wire so an empty signal means "do not change"; the body
    // serialiser converts the empty string to a JSON null which the
    // server `COALESCE`s on the UPDATE.
    let mut e_assigned_user = use_signal(String::new);
    let mut e_ip = use_signal(String::new);
    let mut e_hostname = use_signal(String::new);
    let mut e_mac = use_signal(String::new);
    let mut e_installed = use_signal(String::new);
    let mut e_department = use_signal(String::new);
    let mut e_in_transit = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    // PMS-518: per-field inline error slots so the edit modal reports every
    // validation failure at once (matching the New Asset form).
    let mut e_name_err = use_signal(String::new);
    let mut e_serial_err = use_signal(String::new);
    let mut e_manufacturer_err = use_signal(String::new);
    let mut e_model_err = use_signal(String::new);
    let id_for_save = props.id.clone();

    // MAPPS-158: detail-page Delete, wired to the existing
    // `DELETE /assets/{id}` endpoint (parity with Company/Contract).
    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let id_for_delete = props.id.clone();

    // MAPPS-189: the Delete button opens the styled ConfirmDialog; the
    // actual DELETE fires from `on_confirm_delete` when confirmed.
    let mut confirming_delete = use_signal(|| false);

    // MAPPS-231: add/remove asset credentials. The vault endpoints support
    // create (`POST /assets/{id}/credentials`) and delete
    // (`DELETE /credentials/{id}`); there is no update endpoint, so a
    // credential is add/remove only (to change one, remove it and add a new
    // one). `cred_adding` drives the add modal; `confirming_cred_delete` holds
    // the id of the credential pending a delete confirmation.
    let mut cred_adding = use_signal(|| false);
    let mut nc_name = use_signal(String::new);
    let mut nc_type = use_signal(String::new);
    let mut nc_username = use_signal(String::new);
    let mut nc_password = use_signal(String::new);
    let mut nc_url = use_signal(String::new);
    let mut nc_notes = use_signal(String::new);
    let mut nc_submitting = use_signal(|| false);
    let mut nc_name_err = use_signal(String::new);
    let mut nc_type_err = use_signal(String::new);
    let mut nc_username_err = use_signal(String::new);
    let mut nc_password_err = use_signal(String::new);
    let mut nc_error = use_signal(String::new);
    let id_for_cred_add = props.id.clone();

    let mut confirming_cred_delete = use_signal(|| Option::<uuid::Uuid>::None);
    let mut cred_deleting = use_signal(|| false);

    // MAPPS-233: add/remove asset relationships. The server exposes create
    // (`POST /assets/{id}/relationships`) and delete
    // (`DELETE /asset-relationships/{id}`); there is no update endpoint, so a
    // relationship is add/remove only (to change one, remove it and add a new
    // one), mirroring the MAPPS-231 credentials flow. `rel_adding` drives the
    // add modal; `confirming_rel_delete` holds the id of the relationship
    // pending a delete confirmation.
    let mut rel_adding = use_signal(|| false);
    let mut nr_child_id = use_signal(String::new);
    let mut nr_child_name = use_signal(String::new);
    let mut nr_type = use_signal(String::new);
    let mut nr_submitting = use_signal(|| false);
    let mut nr_child_err = use_signal(String::new);
    let mut nr_type_err = use_signal(String::new);
    let mut nr_error = use_signal(String::new);
    let id_for_rel_add = props.id.clone();

    let mut confirming_rel_delete = use_signal(|| Option::<uuid::Uuid>::None);
    let mut rel_deleting = use_signal(|| false);

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
    // MAPPS-357: the primary asset fetch resolved but failed (Some(None)),
    // distinct from still-loading (None) and from a real asset (Some(Some)).
    let fetch_failed = matches!(&snapshot, Some(None));
    let asset = snapshot.flatten();
    let relationships = rel_resource.read_unchecked().clone().unwrap_or_default();
    let config_items = cfg_resource.read_unchecked().clone().unwrap_or_default();
    let credentials = cred_resource.read_unchecked().clone().unwrap_or_default();
    // MAPPS-304: project the audit Resource into a three-way render state:
    // `Loading` (resource still in flight), `Ready(Vec)` (fetched - may be
    // empty), `Err(String)` (fetch failed - render a recoverable message,
    // not a misleading "No history yet").
    let audit_state: AuditPanelState = match audit_resource.read_unchecked().as_ref() {
        None => AuditPanelState::Loading,
        Some(Ok(items)) => AuditPanelState::Ready(items.clone()),
        Some(Err(e)) => AuditPanelState::Failed(e.clone()),
    };
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
        // MAPPS-304: read the most recent audit entry through the new
        // `AuditPanelState`. `Failed` and `Loading` fall through to
        // `None` so the "Edited by X" label is hidden rather than
        // wrong - the panel itself shows the real failure.
        let latest = match &audit_state {
            AuditPanelState::Ready(items) => items.first(),
            _ => None,
        };
        latest.map(|e| {
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
    use_page_title(&header_title);

    // MAPPS-357: a failed primary load while the server is flagged down is an
    // outage, not a missing asset - render the honest unavailable state (which
    // keeps the nav + banner) instead of the "Could not load this asset"
    // notice. A fetch that fails while still reachable (a 404 / 4xx) keeps that
    // inline notice below. `can_mutate` disables the write controls while down.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Asset".to_string() }
        };
    }

    rsx! {
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
            // PMS-745: a route back to the list, matching ContractDetailPage.
            // AssetNewPage already carried one (MAPPS-294); the detail page was
            // missed in that pass.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Assets", Route::AssetList {}, &header_title),
                }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Danger,
                    loading: *deleting.read(),
                    // MAPPS-357: block delete while the server is down.
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

        if is_loading {
            // PMS-353
            crate::components::DetailSkeleton {}
        } else if asset.is_none() {
            Card {
                p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                    "Could not load this asset."
                }
            }
        } else {
            {
                let a = asset.clone().unwrap();
                let (status_variant, status_label) = asset_status_badge(&a.status);
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
                // MAPPS-305: derive a "Needs refresh" cue from the warranty
                // date. Expired (date < today) is "Expired"; within 30 days
                // is "Expires soon". Both signal a refresh / replacement
                // candidate without an admin having to spreadsheet-sweep
                // the asset list manually. Threshold is hardcoded at 30
                // days for now; future config-driven threshold is the
                // documented next step on the ticket.
                let warranty_status = warranty_refresh_status(&a.warranty_expiry);
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
                    e_itil_stage.set(a_edit.itil_lifecycle_stage.clone().unwrap_or_default());
                    // PMS-473: seed the CMDB expansion fields.
                    e_assigned_user.set(
                        a_edit
                            .assigned_user_id
                            .map(|u| u.to_string())
                            .unwrap_or_default(),
                    );
                    e_ip.set(a_edit.ip_address.clone().unwrap_or_default());
                    e_hostname.set(a_edit.hostname.clone().unwrap_or_default());
                    e_mac.set(a_edit.mac_address.clone().unwrap_or_default());
                    e_installed.set(a_edit.installed_date.clone().unwrap_or_default());
                    e_department.set(a_edit.department.clone().unwrap_or_default());
                    e_in_transit.set(
                        a_edit
                            .in_transit_ticket_id
                            .map(|t| t.to_string())
                            .unwrap_or_default(),
                    );
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
                                        // MAPPS-357: block edit while the server is down.
                                        disabled: !can_mutate,
                                        title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                                        onclick: open_edit,
                                        PencilIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                        "Edit"
                                    }
                                },
                                if let Some(marker) = edited_marker {
                                    p { class: "text-xs text-subtle italic mb-3", "{marker}" }
                                }
                                dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                                    div {
                                        dt { class: "text-sm text-muted", "Type" }
                                        dd { class: "mt-1", "{tname}" }
                                    }
                                    div {
                                        dt { class: "text-sm text-muted", "Company" }
                                        dd { class: "mt-1", "{cname}" }
                                    }
                                    div {
                                        dt { class: "text-sm text-muted", "Manufacturer" }
                                        dd { class: "mt-1", "{manufacturer}" }
                                    }
                                    div {
                                        dt { class: "text-sm text-muted", "Model" }
                                        dd { class: "mt-1", "{model}" }
                                    }
                                    div {
                                        dt { class: "text-sm text-muted", "Serial Number" }
                                        dd { class: "mt-1 font-mono text-sm", "{serial}" }
                                    }
                                    div {
                                        dt { class: "text-sm text-muted", "Asset Tag" }
                                        dd { class: "mt-1 font-mono text-sm", "{tag}" }
                                    }
                                    // PMS-454: CMDB expansion fields. Each
                                    // renders only when populated so an
                                    // older asset that pre-dates the
                                    // migration does not show a row of
                                    // empty "-" placeholders.
                                    if let Some(name) = a.assigned_user_name.clone().filter(|s| !s.trim().is_empty()) {
                                        div {
                                            dt { class: "text-sm text-muted", "Assigned to" }
                                            dd { class: "mt-1", "{name}" }
                                        }
                                    }
                                    if let Some(host) = a.hostname.clone().filter(|s| !s.trim().is_empty()) {
                                        div {
                                            dt { class: "text-sm text-muted", "Hostname" }
                                            dd { class: "mt-1 font-mono text-sm", "{host}" }
                                        }
                                    }
                                    if let Some(ip) = a.ip_address.clone().filter(|s| !s.trim().is_empty()) {
                                        div {
                                            dt { class: "text-sm text-muted", "IP address" }
                                            dd { class: "mt-1 font-mono text-sm", "{ip}" }
                                        }
                                    }
                                    if let Some(mac) = a.mac_address.clone().filter(|s| !s.trim().is_empty()) {
                                        div {
                                            dt { class: "text-sm text-muted", "MAC address" }
                                            dd { class: "mt-1 font-mono text-sm", "{mac}" }
                                        }
                                    }
                                    if let Some(dept) = a.department.clone().filter(|s| !s.trim().is_empty()) {
                                        div {
                                            dt { class: "text-sm text-muted", "Department" }
                                            dd { class: "mt-1", "{dept}" }
                                        }
                                    }
                                    if let Some(installed) = a.installed_date.as_ref().filter(|s| !s.trim().is_empty()) {
                                        {
                                            let installed_label = fmt_date(&Some(installed.clone()));
                                            rsx! {
                                                div {
                                                    dt { class: "text-sm text-muted", "Installed" }
                                                    dd { class: "mt-1", "{installed_label}" }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(ticket_id) = a.in_transit_ticket_id {
                                        {
                                            let tid = ticket_id.to_string();
                                            rsx! {
                                                div {
                                                    dt { class: "text-sm text-muted", "In-transit ticket" }
                                                    dd { class: "mt-1",
                                                        Link {
                                                            to: Route::TicketDetail { id: tid.clone() },
                                                            class: "text-accent hover:opacity-90 font-mono text-sm",
                                                            "{tid}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Relationships (MAPPS-233: add/remove UI,
                            // mirroring the Credentials card below).
                            Card {
                                title: "Relationships",
                                actions: rsx! {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        // MAPPS-357 parity: block adding while the server is down.
                                        disabled: !can_mutate,
                                        title: (!can_mutate).then(|| "Can't add a relationship while the server is unreachable".to_string()),
                                        onclick: move |_| {
                                            nr_child_id.set(String::new());
                                            nr_child_name.set(String::new());
                                            nr_type.set(String::new());
                                            nr_child_err.set(String::new());
                                            nr_type_err.set(String::new());
                                            nr_error.set(String::new());
                                            rel_adding.set(true);
                                        },
                                        PlusIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                        "Add"
                                    }
                                },
                                if relationships.is_empty() {
                                    p { class: "text-sm text-subtle italic", "No relationships." }
                                } else {
                                    div { class: "space-y-2",
                                        for r in relationships.iter() {
                                            {
                                                let rid = r.id;
                                                let child = r
                                                    .child_asset_id
                                                    .map(|c| c.to_string())
                                                    .unwrap_or_default();
                                                rsx! {
                                                    div { class: "flex items-center justify-between p-2 bg-surface-2 rounded gap-3",
                                                        div { class: "flex items-center gap-2 min-w-0",
                                                            Badge { variant: BadgeVariant::Blue, "{r.relationship_type}" }
                                                            if !child.is_empty() {
                                                                Link {
                                                                    to: Route::AssetDetail { id: child.clone() },
                                                                    class: "text-sm text-accent hover:opacity-90 font-mono truncate",
                                                                    "{child}"
                                                                }
                                                            }
                                                        }
                                                        if let Some(rid) = rid {
                                                            Button {
                                                                variant: ButtonVariant::Danger,
                                                                // MAPPS-357 parity: block removal while the server is down.
                                                                disabled: !can_mutate,
                                                                title: (!can_mutate).then(|| "Can't remove a relationship while the server is unreachable".to_string()),
                                                                onclick: move |_| confirming_rel_delete.set(Some(rid)),
                                                                TrashIcon { size: IconSize::Small }
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
                                    p { class: "text-sm text-subtle italic", "No related tickets." }
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
                                                    div { class: "flex items-center justify-between p-2 bg-surface-2 rounded gap-3",
                                                        div { class: "min-w-0 flex-1",
                                                            Link {
                                                                to: Route::TicketDetail { id: tid.clone() },
                                                                class: "text-sm font-medium text-accent hover:opacity-90",
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

                            // MAPPS-232: configuration items are crypto-vault
                            // secrets surfaced on demand through the audited
                            // `GET /configuration-items/{id}` reveal, exactly
                            // like the Credentials card below. The server
                            // exposes no create / edit / delete route for them,
                            // so this card is intentionally reveal-only: no
                            // add/edit/delete affordance is offered here.
                            Card { title: "Configuration Items",
                                if config_items.is_empty() {
                                    p { class: "text-sm text-subtle italic", "No configuration items." }
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
                                                    div { class: "flex items-center justify-between p-2 bg-surface-2 rounded gap-3",
                                                        div { class: "min-w-0",
                                                            p { class: "font-medium text-sm text-content", "{ci.name}" }
                                                            if !category.is_empty() {
                                                                p { class: "text-xs text-muted", "{category}" }
                                                            }
                                                            if let Some(val) = revealed {
                                                                p { class: "font-mono text-sm text-content break-all mt-1", "{val}" }
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
                            Card {
                                title: "Credentials",
                                actions: rsx! {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        // MAPPS-357: block adding a credential while the server is down.
                                        disabled: !can_mutate,
                                        title: (!can_mutate).then(|| "Can't add a credential while the server is unreachable".to_string()),
                                        onclick: move |_| {
                                            nc_name.set(String::new());
                                            nc_type.set(String::new());
                                            nc_username.set(String::new());
                                            nc_password.set(String::new());
                                            nc_url.set(String::new());
                                            nc_notes.set(String::new());
                                            nc_name_err.set(String::new());
                                            nc_error.set(String::new());
                                            cred_adding.set(true);
                                        },
                                        PlusIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                        "Add"
                                    }
                                },
                                if credentials.is_empty() {
                                    p { class: "text-sm text-subtle italic", "No credentials." }
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
                                                    div { class: "p-2 bg-surface-2 rounded",
                                                        div { class: "flex items-center justify-between gap-3",
                                                            div { class: "min-w-0",
                                                                p { class: "font-medium text-sm text-content", "{cr.name}" }
                                                                p { class: "text-xs text-muted", "{cr.credential_type}" }
                                                                if !url.is_empty() {
                                                                    p { class: "text-xs text-muted break-all", "{url}" }
                                                                }
                                                            }
                                                            div { class: "flex items-center gap-2 shrink-0",
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
                                                                Button {
                                                                    variant: ButtonVariant::Danger,
                                                                    // MAPPS-357: block credential removal while the server is down.
                                                                    disabled: !can_mutate,
                                                                    title: (!can_mutate).then(|| "Can't remove a credential while the server is unreachable".to_string()),
                                                                    onclick: move |_| confirming_cred_delete.set(Some(crid)),
                                                                    TrashIcon { size: IconSize::Small }
                                                                }
                                                            }
                                                        }
                                                        if let Some(c) = revealed {
                                                            div { class: "mt-2 space-y-1 font-mono text-sm text-content break-all",
                                                                p { "user: {c.username}" }
                                                                p { "pass: {c.password}" }
                                                                if let Some(n) = c.notes.as_ref().filter(|s| !s.is_empty()) {
                                                                    p { class: "text-muted", "notes: {n}" }
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
                                        span { class: "text-muted", "Status" }
                                        Badge { variant: status_variant, "{status_label}" }
                                    }
                                    // PMS-476: ITIL CI lifecycle
                                    // stage. Shown only when
                                    // populated so an asset that
                                    // pre-dates the column stays
                                    // visually clean.
                                    if let Some(stage) = a.itil_lifecycle_stage.clone().filter(|s| !s.trim().is_empty()) {
                                        div { class: "flex justify-between items-center",
                                            span { class: "text-muted", "Lifecycle" }
                                            Badge { variant: BadgeVariant::Gray, "{stage}" }
                                        }
                                    }
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-muted", "Warranty" }
                                        div { class: "flex items-center gap-2",
                                            span { class: "font-medium", "{warranty}" }
                                            // MAPPS-305: surface the refresh cue.
                                            match warranty_status {
                                                WarrantyRefreshStatus::Expired => rsx! {
                                                    Badge {
                                                        variant: BadgeVariant::Red,
                                                        "Needs refresh"
                                                    }
                                                },
                                                WarrantyRefreshStatus::ExpiringSoon => rsx! {
                                                    Badge {
                                                        variant: BadgeVariant::Yellow,
                                                        "Expires soon"
                                                    }
                                                },
                                                WarrantyRefreshStatus::Healthy
                                                | WarrantyRefreshStatus::Unknown => rsx! {},
                                            }
                                        }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "Purchased" }
                                        span { class: "font-medium", "{purchased}" }
                                    }
                                }
                            }

                            Card { title: "Change History",
                                match audit_state {
                                    AuditPanelState::Loading => rsx! {
                                        p { class: "text-sm text-subtle italic", "Loading…" }
                                    },
                                    AuditPanelState::Failed(ref e) => rsx! {
                                        // MAPPS-304: a real failure (403, network, etc.)
                                        // no longer rendered as "No history yet". The
                                        // user knows to retry; admins know the gate
                                        // applies.
                                        p {
                                            class: "text-sm text-red-600 dark:text-red-400",
                                            "Could not load change history."
                                        }
                                        p {
                                            class: "text-xs text-subtle mt-1",
                                            "{e}"
                                        }
                                    },
                                    AuditPanelState::Ready(ref audit) if audit.is_empty() => rsx! {
                                        p { class: "text-sm text-subtle italic", "No history yet." }
                                    },
                                    AuditPanelState::Ready(ref audit) => rsx! {
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
                                                            p { class: "text-content",
                                                                if event.is_empty() {
                                                                    "{label}"
                                                                } else {
                                                                    "{label}: {event}"
                                                                }
                                                            }
                                                            if who != "-" {
                                                                p { class: "text-xs text-subtle", "by {who}" }
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
                                                                            p { class: "text-xs text-muted mt-1",
                                                                                span { class: "font-medium", "{fname}: " }
                                                                                span { class: "line-through text-subtle", "{old}" }
                                                                                " → "
                                                                                span { "{new}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        span { class: "text-subtle whitespace-nowrap", "{when}" }
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
                    // reject a blank or over-long name and over-long optional
                    // fields before the PUT. PMS-518: validate all of them and
                    // report every failure at once in its own inline slot, then
                    // focus the first invalid field.
                    e_name_err.set(String::new());
                    e_serial_err.set(String::new());
                    e_manufacturer_err.set(String::new());
                    e_model_err.set(String::new());
                    let mut guard = FormGuard::new();
                    let name_res = validate_asset_name(&e_name());
                    if let Err(msg) = &name_res {
                        e_name_err.set(msg.clone());
                        guard.note_invalid(Some("edit-name"));
                    }
                    if let Err(msg) =
                        validate_asset_optional(&e_serial(), "Serial number", ASSET_SERIAL_MAX)
                    {
                        e_serial_err.set(msg);
                        guard.note_invalid(Some("edit-serial"));
                    }
                    if let Err(msg) = validate_asset_optional(
                        &e_manufacturer(),
                        "Manufacturer",
                        ASSET_MANUFACTURER_MAX,
                    ) {
                        e_manufacturer_err.set(msg);
                        guard.note_invalid(Some("edit-manufacturer"));
                    }
                    if let Err(msg) =
                        validate_asset_optional(&e_model(), "Model", ASSET_MODEL_MAX)
                    {
                        e_model_err.set(msg);
                        guard.note_invalid(Some("edit-model"));
                    }
                    if guard.blocked() {
                        return;
                    }
                    let asset_name = name_res.expect("name validated above");
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
                    // PMS-476: ITIL CI lifecycle stage. Free-text
                    // so a tenant can coin a stage; the server
                    // takes it verbatim and renders it on the
                    // detail page beside Status.
                    body.insert(
                        "itil_lifecycle_stage".into(),
                        serde_json::json!(opt_str(&e_itil_stage())),
                    );
                    // PMS-473: CMDB expansion fields. UUID-shaped
                    // fields parse to JSON null when the signal
                    // is empty so the server clears the column;
                    // a malformed UUID is sent verbatim so the
                    // server validator surfaces the 422 instead
                    // of the client silently swallowing it.
                    body.insert(
                        "assigned_user_id".into(),
                        serde_json::json!(opt_str(&e_assigned_user())),
                    );
                    body.insert("ip_address".into(), serde_json::json!(opt_str(&e_ip())));
                    body.insert("hostname".into(), serde_json::json!(opt_str(&e_hostname())));
                    body.insert("mac_address".into(), serde_json::json!(opt_str(&e_mac())));
                    body.insert(
                        "installed_date".into(),
                        serde_json::json!(opt_str(&e_installed())),
                    );
                    body.insert(
                        "department".into(),
                        serde_json::json!(opt_str(&e_department())),
                    );
                    body.insert(
                        "in_transit_ticket_id".into(),
                        serde_json::json!(opt_str(&e_in_transit())),
                    );
                    let body = serde_json::Value::Object(body);
                    // MAPPS-304: the modal previously used the
                    // string-returning `put_authed` which collapses
                    // "request succeeded but the response body
                    // could not be decoded" into the same `Err`
                    // branch as a real Status / Network failure.
                    // QA reported the symptom as "every save shows
                    // an error toast even though the value
                    // persists" - the mutation lands but a
                    // post-mutation decode quirk drops us into the
                    // error path. Switch to the typed variant and
                    // treat `ApiError::Decode` as success (the
                    // mutation succeeded; we re-fetch the row, so
                    // the decoded body is unused anyway). Status /
                    // Network errors still surface inline so a
                    // genuine 4xx/5xx remains visible.
                    let result = crate::hooks::fetch::api::put_authed_typed::<
                        serde_json::Value,
                        _,
                    >(&format!("/assets/{save_id}"), &body)
                    .await;
                    e_submitting.set(false);
                    match result {
                        Ok(_) | Err(crate::hooks::fetch::api::ApiError::Decode(_)) => {
                            editing.set(false);
                            e_error.set(String::new());
                            asset_res.restart();
                            audit_res.restart();
                            crate::hooks::toast::push_toast(
                                crate::components::AlertType::Success,
                                "Asset updated.",
                            );
                        }
                        Err(crate::hooks::fetch::api::ApiError::Status {
                            message, ..
                        }) => {
                            e_error.set(if message.is_empty() {
                                "Could not update the asset.".into()
                            } else {
                                message
                            });
                        }
                        Err(crate::hooks::fetch::api::ApiError::Network(msg)) => {
                            e_error.set(format!("Network error: {msg}"));
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
                            // MAPPS-357: block the save PUT while the server is down.
                            disabled: e_submitting() || !can_mutate,
                            loading: e_submitting(),
                            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                            onclick: on_save,
                            "Save Changes"
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
                            rules: vec![Rule::Required],
                            error: e_name_err(),
                            value: "{e_name}",
                            oninput: move |e: FormEvent| {
                                e_name_err.set(String::new());
                                e_name.set(e.value());
                            },
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
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
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Input {
                                name: "edit-manufacturer",
                                label: "Manufacturer",
                                maxlength: ASSET_MANUFACTURER_MAX as i64,
                                error: e_manufacturer_err(),
                                value: "{e_manufacturer}",
                                oninput: move |e: FormEvent| {
                                    e_manufacturer_err.set(String::new());
                                    e_manufacturer.set(e.value());
                                },
                            }
                            Input {
                                name: "edit-model",
                                label: "Model",
                                maxlength: ASSET_MODEL_MAX as i64,
                                error: e_model_err(),
                                value: "{e_model}",
                                oninput: move |e: FormEvent| {
                                    e_model_err.set(String::new());
                                    e_model.set(e.value());
                                },
                            }
                        }
                        Input {
                            name: "edit-serial",
                            label: "Serial Number",
                            maxlength: ASSET_SERIAL_MAX as i64,
                            error: e_serial_err(),
                            value: "{e_serial}",
                            oninput: move |e: FormEvent| {
                                e_serial_err.set(String::new());
                                e_serial.set(e.value());
                            },
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
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
                        // PMS-476: ITIL CI lifecycle stage. Free
                        // text so a tenant can coin a stage; the
                        // help line suggests the standard set.
                        Input {
                            name: "edit-itil-stage",
                            label: "ITIL lifecycle stage",
                            placeholder: "e.g. in_service",
                            help: "Standard: planned, in_service, retired. Leave blank for unknown.".to_string(),
                            value: "{e_itil_stage}",
                            oninput: move |e: FormEvent| e_itil_stage.set(e.value()),
                        }
                        // PMS-473: CMDB expansion fields. The
                        // assigned-user picker is a Select built
                        // from the already-cached `users_resource`
                        // so an inline edit doesn't fire a fresh
                        // /auth/users fetch. Other fields are
                        // text / date inputs because the values
                        // are free-form network identifiers.
                        {
                            let mut user_opts = vec![SelectOption::new("", "(unassigned)")];
                            for u in users.iter() {
                                let label = if u.full_name.trim().is_empty() {
                                    u.id.to_string()
                                } else {
                                    u.full_name.clone()
                                };
                                user_opts.push(SelectOption::new(u.id.to_string(), label));
                            }
                            rsx! {
                                Select {
                                    name: "edit-assigned-user",
                                    label: "Assigned user",
                                    options: user_opts,
                                    value: "{e_assigned_user}",
                                    onchange: move |e: FormEvent| e_assigned_user.set(e.value()),
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Input {
                                name: "edit-hostname",
                                label: "Hostname",
                                placeholder: "host.example.com",
                                value: "{e_hostname}",
                                oninput: move |e: FormEvent| e_hostname.set(e.value()),
                            }
                            Input {
                                name: "edit-ip",
                                label: "IP address",
                                placeholder: "10.0.0.1 or fe80::1",
                                help: "IPv4 or IPv6. Server validates the format.".to_string(),
                                value: "{e_ip}",
                                oninput: move |e: FormEvent| e_ip.set(e.value()),
                            }
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Input {
                                name: "edit-mac",
                                label: "MAC address",
                                placeholder: "aa:bb:cc:dd:ee:ff",
                                value: "{e_mac}",
                                oninput: move |e: FormEvent| e_mac.set(e.value()),
                            }
                            crate::components::DateField {
                                name: "edit-installed",
                                label: "Installed date",
                                value: "{e_installed}",
                                oninput: move |e: FormEvent| e_installed.set(e.value()),
                            }
                        }
                        Input {
                            name: "edit-department",
                            label: "Department",
                            placeholder: "e.g. Sales",
                            value: "{e_department}",
                            oninput: move |e: FormEvent| e_department.set(e.value()),
                        }
                        // In-transit ticket reference: free-text
                        // UUID input. A picker over /tickets is
                        // the natural follow-up but the dispatcher
                        // already has the ticket id when they set
                        // status=in_transit, so a paste works in
                        // v1. The server validates the FK.
                        Input {
                            name: "edit-in-transit-ticket",
                            label: "In-transit ticket id",
                            placeholder: "UUID of the dispatch ticket",
                            help: "Optional. Sets the ticket the asset is currently being moved against.".to_string(),
                            value: "{e_in_transit}",
                            oninput: move |e: FormEvent| e_in_transit.set(e.value()),
                        }
                    }
                }
            }
        }

        // MAPPS-231: confirm before removing a credential, mirroring the
        // asset Delete confirmation. The DELETE fires from `on_confirm`.
        {
            let mut cred_res = cred_resource;
            let mut audit_res = audit_resource;
            rsx! {
                crate::components::ConfirmDialog {
                    open: confirming_cred_delete().is_some(),
                    title: "Remove credential".to_string(),
                    message: "Remove this credential? This cannot be undone."
                        .to_string(),
                    confirm_text: "Remove".to_string(),
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: *cred_deleting.read(),
                    onconfirm: move |_| {
                        if *cred_deleting.read() {
                            return;
                        }
                        let Some(crid) = confirming_cred_delete() else {
                            return;
                        };
                        cred_deleting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let path = format!("/credentials/{crid}");
                                if crate::hooks::fetch::api::delete_authed(&path)
                                    .await
                                    .is_ok()
                                {
                                    cred_res.restart();
                                    audit_res.restart();
                                }
                            }
                            cred_deleting.set(false);
                            confirming_cred_delete.set(None);
                        });
                    },
                    oncancel: move |_| {
                        if !*cred_deleting.read() {
                            confirming_cred_delete.set(None);
                        }
                    },
                }
            }
        }

        // MAPPS-231: add-credential modal. POSTs to the vault create
        // endpoint, then refreshes the credentials list and audit log.
        {
            let mut cred_res = cred_resource;
            let mut audit_res = audit_resource;
            let add_id = id_for_cred_add.clone();
            let on_add_cred = move |_| {
                if nc_submitting() {
                    return;
                }
                nc_name_err.set(String::new());
                nc_type_err.set(String::new());
                nc_username_err.set(String::new());
                nc_password_err.set(String::new());
                nc_error.set(String::new());
                // PMS-518: validate every required field through the shared
                // FormGuard so all failures surface at once (each in its own
                // inline slot) and the first invalid field is focused. Name
                // keeps its bespoke validator (length cap + the trimmed value
                // used in the body); the guard adds its first-invalid focus.
                let mut guard = FormGuard::new();
                let name = match validate_cred_name(&nc_name()) {
                    Ok(v) => v,
                    Err(msg) => {
                        nc_name_err.set(msg);
                        guard.note_invalid(Some("cred-name"));
                        String::new()
                    }
                };
                let credential_type = nc_type().trim().to_string();
                nc_type_err.set(guard.field(
                    "cred-type",
                    &credential_type,
                    "Type",
                    &[Rule::Required],
                ));
                let username = nc_username();
                nc_username_err.set(guard.field(
                    "cred-username",
                    &username,
                    "Username",
                    &[Rule::Required],
                ));
                let password = nc_password();
                nc_password_err.set(guard.field(
                    "cred-password",
                    &password,
                    "Password",
                    &[Rule::Required],
                ));
                if guard.blocked() {
                    return;
                }
                let url = opt_str(&nc_url());
                let notes = opt_str(&nc_notes());
                let add_id = add_id.clone();
                nc_submitting.set(true);
                spawn(async move {
                    #[cfg(feature = "web")]
                    {
                        let mut body = serde_json::json!({
                            "name": name,
                            "credential_type": credential_type,
                            "username": username,
                            "password": password,
                        });
                        if let Some(u) = url {
                            body["url"] = serde_json::json!(u);
                        }
                        if let Some(n) = notes {
                            body["notes"] = serde_json::json!(n);
                        }
                        match crate::hooks::fetch::api::post_authed_typed::<
                                serde_json::Value,
                                _,
                            >(&format!("/assets/{add_id}/credentials"), &body)
                            .await
                        {
                            Ok(_) => {
                                cred_adding.set(false);
                                cred_res.restart();
                                audit_res.restart();
                            }
                            Err(e) => {
                                // Route a server-flagged Name/URL validation
                                // message next to the right field; otherwise
                                // show the general banner.
                                if let Some(msg) = e.field_message("name") {
                                    nc_name_err.set(msg);
                                } else if let Some(msg) = e.field_message("url") {
                                    nc_error.set(msg);
                                } else {
                                    nc_error
                                        .set(
                                            format!("Could not add credential: {}", e.user_message()),
                                        );
                                }
                            }
                        }
                    }
                    nc_submitting.set(false);
                });
            };
            rsx! {
                Modal {
                    open: cred_adding(),
                    title: "Add Credential",
                    onclose: move |_| cred_adding.set(false),
                    footer: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| cred_adding.set(false),
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            // MAPPS-357: block the credential POST while the server is down.
                            disabled: nc_submitting() || !can_mutate,
                            loading: nc_submitting(),
                            title: (!can_mutate).then(|| "Can't add a credential while the server is unreachable".to_string()),
                            onclick: on_add_cred,
                            "Add Credential"
                        }
                    },
                    div { class: "space-y-4",
                        if !nc_error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{nc_error}" }
                        }
                        Input {
                            name: "cred-name",
                            label: "Name",
                            required: true,
                            maxlength: CRED_NAME_MAX as i64,
                            error: nc_name_err(),
                            value: "{nc_name}",
                            oninput: move |e: FormEvent| {
                                nc_name_err.set(String::new());
                                nc_name.set(e.value());
                            },
                        }
                        Input {
                            name: "cred-type",
                            label: "Type",
                            required: true,
                            placeholder: "e.g. domain, ssh, rdp",
                            rules: vec![Rule::Required],
                            error: nc_type_err(),
                            value: "{nc_type}",
                            oninput: move |e: FormEvent| {
                                nc_type_err.set(String::new());
                                nc_type.set(e.value());
                            },
                        }
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                            Input {
                                name: "cred-username",
                                label: "Username",
                                required: true,
                                rules: vec![Rule::Required],
                                error: nc_username_err(),
                                value: "{nc_username}",
                                oninput: move |e: FormEvent| {
                                    nc_username_err.set(String::new());
                                    nc_username.set(e.value());
                                },
                            }
                            Input {
                                name: "cred-password",
                                label: "Password",
                                r#type: "password",
                                required: true,
                                rules: vec![Rule::Required],
                                error: nc_password_err(),
                                value: "{nc_password}",
                                oninput: move |e: FormEvent| {
                                    nc_password_err.set(String::new());
                                    nc_password.set(e.value());
                                },
                            }
                        }
                        Input {
                            name: "cred-url",
                            label: "URL",
                            placeholder: "https://… (optional)",
                            value: "{nc_url}",
                            oninput: move |e: FormEvent| nc_url.set(e.value()),
                        }
                        Textarea {
                            name: "cred-notes",
                            label: "Notes",
                            value: "{nc_notes}",
                            oninput: move |e: FormEvent| nc_notes.set(e.value()),
                        }
                    }
                }
            }
        }

        // MAPPS-233: confirm before removing a relationship, mirroring the
        // credential Remove confirmation. The DELETE fires from `onconfirm`.
        {
            let mut rel_res = rel_resource;
            let mut audit_res = audit_resource;
            rsx! {
                crate::components::ConfirmDialog {
                    open: confirming_rel_delete().is_some(),
                    title: "Remove relationship".to_string(),
                    message: "Remove this relationship? This cannot be undone."
                        .to_string(),
                    confirm_text: "Remove".to_string(),
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: *rel_deleting.read(),
                    onconfirm: move |_| {
                        if *rel_deleting.read() {
                            return;
                        }
                        let Some(rid) = confirming_rel_delete() else {
                            return;
                        };
                        rel_deleting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let path = format!("/asset-relationships/{rid}");
                                if crate::hooks::fetch::api::delete_authed(&path)
                                    .await
                                    .is_ok()
                                {
                                    rel_res.restart();
                                    audit_res.restart();
                                }
                            }
                            rel_deleting.set(false);
                            confirming_rel_delete.set(None);
                        });
                    },
                    oncancel: move |_| {
                        if !*rel_deleting.read() {
                            confirming_rel_delete.set(None);
                        }
                    },
                }
            }
        }

        // MAPPS-233: add-relationship modal. POSTs to the relationships
        // create endpoint, then refreshes the relationships list and audit
        // log. The related (child) asset is picked via the shared
        // AssetPicker; relationship_type is one of the server's known kinds
        // ("contains" | "connected_to" | "depends_on" | "hosts").
        {
            let mut rel_res = rel_resource;
            let mut audit_res = audit_resource;
            let add_id = id_for_rel_add.clone();
            let rel_type_opts = vec![
                SelectOption::new("", "Select a type"),
                SelectOption::new("depends_on", "Depends On"),
                SelectOption::new("hosts", "Hosts"),
                SelectOption::new("connected_to", "Connected To"),
                SelectOption::new("contains", "Contains"),
            ];
            let on_add_rel = move |_| {
                if nr_submitting() {
                    return;
                }
                nr_child_err.set(String::new());
                nr_type_err.set(String::new());
                nr_error.set(String::new());
                let child = nr_child_id().trim().to_string();
                let rel_type = nr_type().trim().to_string();
                let mut blocked = false;
                if child.is_empty() {
                    nr_child_err.set("Select an asset.".to_string());
                    blocked = true;
                }
                if rel_type.is_empty() {
                    nr_type_err.set("Select a type.".to_string());
                    blocked = true;
                }
                if blocked {
                    return;
                }
                let add_id = add_id.clone();
                nr_submitting.set(true);
                spawn(async move {
                    #[cfg(feature = "web")]
                    {
                        let body = serde_json::json!({
                            "child_asset_id": child,
                            "relationship_type": rel_type,
                        });
                        match crate::hooks::fetch::api::post_authed_typed::<
                                serde_json::Value,
                                _,
                            >(&format!("/assets/{add_id}/relationships"), &body)
                            .await
                        {
                            Ok(_) => {
                                rel_adding.set(false);
                                rel_res.restart();
                                audit_res.restart();
                            }
                            Err(e) => {
                                // Route a server-flagged field validation
                                // message to its input; otherwise the banner.
                                if let Some(msg) = e.field_message("child_asset_id") {
                                    nr_child_err.set(msg);
                                } else if let Some(msg) = e.field_message("relationship_type") {
                                    nr_type_err.set(msg);
                                } else {
                                    nr_error
                                        .set(
                                            format!("Could not add relationship: {}", e.user_message()),
                                        );
                                }
                            }
                        }
                    }
                    nr_submitting.set(false);
                });
            };
            let child_selected = nr_child_id();
            rsx! {
                Modal {
                    open: rel_adding(),
                    title: "Add Relationship",
                    onclose: move |_| rel_adding.set(false),
                    footer: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| rel_adding.set(false),
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            // MAPPS-357 parity: block the POST while the server is down.
                            disabled: nr_submitting() || !can_mutate,
                            loading: nr_submitting(),
                            title: (!can_mutate).then(|| "Can't add a relationship while the server is unreachable".to_string()),
                            onclick: on_add_rel,
                            "Add Relationship"
                        }
                    },
                    div { class: "space-y-4",
                        if !nr_error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{nr_error}" }
                        }
                        // AssetPicker has no error prop, so surface the
                        // child-asset error just below it (same convention as
                        // the New Asset form's CompanyPicker).
                        div { class: "space-y-1",
                            crate::components::AssetPicker {
                                value: nr_child_name(),
                                selected_id: (!child_selected.is_empty()).then(|| child_selected.clone()),
                                label: "Related asset".to_string(),
                                required: true,
                                onselect: move |(id, name): (String, String)| {
                                    nr_child_id.set(id);
                                    nr_child_name.set(name);
                                    nr_child_err.set(String::new());
                                },
                                onclear: move |_| {
                                    nr_child_id.set(String::new());
                                    nr_child_name.set(String::new());
                                },
                            }
                            if !nr_child_err().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", "{nr_child_err}" }
                            }
                        }
                        Select {
                            name: "rel-type",
                            label: "Relationship type".to_string(),
                            options: rel_type_opts,
                            required: true,
                            error: nr_type_err(),
                            value: nr_type(),
                            onchange: move |e: FormEvent| {
                                nr_type_err.set(String::new());
                                nr_type.set(e.value());
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{
        validate_asset_name, validate_asset_optional, validate_cred_name, ASSET_NAME_MAX,
        ASSET_SERIAL_MAX, CRED_NAME_MAX,
    };

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
    fn cred_name_required_trimmed_and_capped() {
        assert!(validate_cred_name("   ").is_err());
        assert_eq!(
            validate_cred_name("  vault-admin  ").unwrap(),
            "vault-admin"
        );
        assert!(validate_cred_name(&"x".repeat(CRED_NAME_MAX)).is_ok());
        assert!(validate_cred_name(&"x".repeat(CRED_NAME_MAX + 1)).is_err());
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

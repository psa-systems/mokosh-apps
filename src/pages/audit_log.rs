//! Admin audit-log page.
//!
//! Reads `GET /api/v1/audit-log` (admin-only, paginated, filterable) and
//! renders it as a filterable table with an expandable per-row JSON diff
//! (old_values -> new_values). Structure mirrors the contacts list page
//! (`src/pages/contacts.rs`): filter signals reset the page on change, a
//! `use_resource` re-fetches on filter/page change AND on org switch via
//! `active_tenant_generation()`, and loading/empty/error states reuse the
//! shared `DataTable` family.

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Card, DataTable, PageHeader, Select, SelectOption, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::modules::audit::AuditLogEntry;
use crate::utils::url::urlencoding_minimal;
use crate::utils::Paginated;

/// Rows per page for the audit log. Matches the server's default
/// `per_page` and the other list pages' `PER_PAGE`.
const PER_PAGE: usize = 25;

/// Badge color per audit action verb. Unknown verbs fall through to
/// gray so future server-side actions still render legibly.
fn action_variant(action: &str) -> BadgeVariant {
    match action {
        "create" => BadgeVariant::Green,
        "update" => BadgeVariant::Blue,
        "delete" => BadgeVariant::Red,
        "login" => BadgeVariant::Purple,
        "logout" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    }
}

/// Format a UTC timestamp for the table. Honours the per-user format
/// preference set in the Profile page (PMS-253). When the user has
/// `None`, falls back to the legacy "%b %-d, %Y %H:%M:%S UTC" shape.
fn format_timestamp(when: chrono::DateTime<chrono::Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    if let Some(fmt) = pref.as_deref().filter(|s| !s.trim().is_empty()) {
        crate::utils::datetime::format_user_datetime(when, Some(fmt))
    } else {
        when.format("%b %-d, %Y %H:%M:%S UTC").to_string()
    }
}

/// Render an optional UUID as a short hex prefix (first 8 chars) so the
/// table stays scannable; the full value is available in the expanded
/// detail row. Empty -> em-dash placeholder.
fn short_uuid(id: Option<uuid::Uuid>) -> String {
    match id {
        Some(u) => {
            let s = u.to_string();
            s.chars().take(8).collect::<String>()
        }
        None => "-".to_string(),
    }
}

/// Pretty-print an optional JSON value for the diff panel. `None`
/// renders as a literal `null` placeholder so an empty side of the diff
/// is still visible.
fn pretty_json(value: &Option<serde_json::Value>) -> String {
    match value {
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
        None => "null".to_string(),
    }
}

/// A user row used to resolve FK ids in the diff to a display name (PMS-365).
/// Reuses the shape and `/auth/users` endpoint the project pickers already use.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct RemoteUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
}

/// Field keys whose values are user ids the SPA can resolve to a name. Other
/// FK ids (companies, contacts, etc.) stay as their raw value because the
/// audit page does not cache those lists (PMS-365).
fn is_user_fk(key: &str) -> bool {
    matches!(
        key,
        "project_manager_id"
            | "manager_id"
            | "assigned_to_id"
            | "assigned_to"
            | "assigned_technician_id"
            | "technician_id"
            | "user_id"
            | "owner_id"
            | "created_by"
            | "updated_by"
    )
}

/// Turn a snake_case field key into a display label, dropping a trailing `_id`
/// so a FK reads as the entity ("project_manager_id" -> "Project manager")
/// instead of the raw column name (PMS-365).
fn humanize_field_key(key: &str) -> String {
    let base = key.strip_suffix("_id").unwrap_or(key);
    let spaced = base.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

/// Render one JSON field value for the diff. Resolves user-FK fields to a name
/// when the SPA has that user cached; otherwise shows the scalar. Absent, null,
/// or empty renders as `(empty)` (PMS-365).
fn display_field_value(
    key: &str,
    value: Option<&serde_json::Value>,
    users: &[RemoteUser],
) -> String {
    match value {
        None | Some(serde_json::Value::Null) => "(empty)".to_string(),
        Some(serde_json::Value::String(s)) if s.trim().is_empty() => "(empty)".to_string(),
        Some(serde_json::Value::String(s)) => {
            if is_user_fk(key) {
                if let Ok(id) = uuid::Uuid::parse_str(s) {
                    if let Some(u) = users.iter().find(|u| u.id == id) {
                        if !u.full_name.trim().is_empty() {
                            return u.full_name.clone();
                        }
                    }
                    // Known FK field but the user is not in our cache (deleted,
                    // or beyond this page of /auth/users): short prefix, not the
                    // full noisy UUID.
                    return s.chars().take(8).collect();
                }
            }
            s.clone()
        }
        Some(other) => other.to_string(),
    }
}

/// Build the per-field diff rows `(label, old, new)` from the entry's
/// `old_values` / `new_values` JSON objects. Returns an empty vec when neither
/// payload is a JSON object, so the caller can fall back to raw JSON (PMS-365).
fn field_diff_rows(
    old: &Option<serde_json::Value>,
    new: &Option<serde_json::Value>,
    users: &[RemoteUser],
) -> Vec<(String, String, String)> {
    let old_obj = old.as_ref().and_then(|v| v.as_object());
    let new_obj = new.as_ref().and_then(|v| v.as_object());
    if old_obj.is_none() && new_obj.is_none() {
        return Vec::new();
    }
    // New keys first (the update case lists what changed), then any key present
    // only in the old payload.
    let mut keys: Vec<String> = Vec::new();
    if let Some(o) = new_obj {
        keys.extend(o.keys().cloned());
    }
    if let Some(o) = old_obj {
        for k in o.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    keys.into_iter()
        .map(|k| {
            let old_v = old_obj.and_then(|o| o.get(&k));
            let new_v = new_obj.and_then(|o| o.get(&k));
            (
                humanize_field_key(&k),
                display_field_value(&k, old_v, users),
                display_field_value(&k, new_v, users),
            )
        })
        .collect()
}

/// Admin-gated audit-log page.
///
/// This outer component holds only the role gate so its hook order stays
/// stable across renders (rules of hooks). The data-fetching hooks live
/// in [`AuditLogContent`], which is only mounted for admins; non-admins
/// never kick off the (guaranteed-403) fetch.
#[component]
pub fn AuditLogPage() -> Element {
    let auth = crate::hooks::use_auth();
    // Role gate: only org admins / super-admins may view the audit log.
    // Uses the `UserRole::is_admin()` helper on the cached CurrentUser so
    // the gate matches the server's `RequireAdmin` extractor. Loading
    // auth resolves to `false` here, which simply shows the notice until
    // the user is known; the server still enforces the real check.
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false);

    if !is_admin {
        return rsx! {
            AppLayout { title: "Audit Log",
                PageHeader { title: "Audit Log", subtitle: "System activity" }
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm font-medium text-content mb-1",
                            "Admins only"
                        }
                        p { class: "text-sm text-muted",
                            "You do not have permission to view the audit log."
                        }
                    }
                }
            }
        };
    }

    rsx! { AuditLogContent {} }
}

/// Filterable audit-log table. Mounted only for admins by
/// [`AuditLogPage`]; all data-fetching hooks live here so the outer
/// component's hook order never changes with auth state.
#[component]
fn AuditLogContent() -> Element {
    let mut entity_type_filter = use_signal(String::new);
    let mut action_filter = use_signal(String::new);
    let mut user_id_filter = use_signal(String::new);
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    let action_options = vec![
        SelectOption::new("", "All Actions"),
        SelectOption::new("create", "Create"),
        SelectOption::new("update", "Update"),
        SelectOption::new("delete", "Delete"),
        SelectOption::new("login", "Login"),
        SelectOption::new("logout", "Logout"),
    ];

    let entity_text = entity_type_filter.read().trim().to_string();
    let action_text = action_filter.read().clone();
    let user_text = user_id_filter.read().trim().to_string();
    let from_text = from_filter.read().clone();
    let to_text = to_filter.read().clone();
    let current_page = (*page.read()).max(1);

    // F1: read the active-tenant generation inside the resource closure so
    // Dioxus re-runs the fetch on an org switch / token swap. Filters and
    // page are captured by value so the resource also re-fetches on every
    // change.
    let entity_for_resource = entity_text.clone();
    let action_for_resource = action_text.clone();
    let user_for_resource = user_text.clone();
    let from_for_resource = from_text.clone();
    let to_for_resource = to_text.clone();
    let audit_resource = use_resource(move || {
        let entity_type = entity_for_resource.clone();
        let action = action_for_resource.clone();
        let user_id = user_for_resource.clone();
        let from = from_for_resource.clone();
        let to = to_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let mut path = format!("/audit-log?page={current_page}&per_page={PER_PAGE}");
            if !entity_type.is_empty() {
                path.push_str(&format!(
                    "&entity_type={}",
                    urlencoding_minimal(&entity_type)
                ));
            }
            if !action.is_empty() {
                path.push_str(&format!("&action={}", urlencoding_minimal(&action)));
            }
            if !user_id.is_empty() {
                path.push_str(&format!("&user_id={}", urlencoding_minimal(&user_id)));
            }
            // The server parses `from`/`to` as RFC3339 `DateTime<Utc>`.
            // The native date input yields `YYYY-MM-DD`; widen each bound
            // to a full-day UTC instant so the filter matches what the
            // user intends (inclusive day range).
            if !from.is_empty() {
                path.push_str(&format!(
                    "&from={}",
                    urlencoding_minimal(&format!("{from}T00:00:00Z"))
                ));
            }
            if !to.is_empty() {
                path.push_str(&format!(
                    "&to={}",
                    urlencoding_minimal(&format!("{to}T23:59:59Z"))
                ));
            }
            crate::hooks::fetch::api::get_authed_typed::<Paginated<AuditLogEntry>>(&path).await
        }
    });

    // PMS-365: fetch the tenant's users once so the diff can resolve user-FK
    // ids (project_manager_id, assigned_to_id, ...) to names. Re-fetches on org
    // switch via the generation read; an error just leaves ids unresolved.
    let users_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteUser>>("/auth/users")
            .await
            .ok()
    });
    let users: Vec<RemoteUser> = match &*users_resource.read_unchecked() {
        Some(Some(resp)) => resp.data.clone(),
        _ => Vec::new(),
    };

    let resource_snapshot = audit_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let error_message: Option<String> = match &*resource_snapshot {
        Some(Err(e)) => Some(e.user_message()),
        _ => None,
    };
    let (page_rows, total): (Vec<AuditLogEntry>, u64) = match &*resource_snapshot {
        Some(Ok(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !entity_text.is_empty()
        || !action_text.is_empty()
        || !user_text.is_empty()
        || !from_text.is_empty()
        || !to_text.is_empty();

    // PMS-455: build the export URL with the same filter envelope the
    // list resource uses, so what the user sees in the table is what
    // gets downloaded. Browser handles the actual download via a real
    // anchor target so we get the `Content-Disposition` filename
    // verbatim and the Authorization header isn't an issue (the
    // session cookie carries through on same-origin navigations).
    let export_href = {
        let mut q = Vec::<String>::new();
        if !entity_text.is_empty() {
            q.push(format!(
                "entity_type={}",
                crate::utils::url::urlencoding_minimal(&entity_text)
            ));
        }
        if !action_text.is_empty() {
            q.push(format!(
                "action={}",
                crate::utils::url::urlencoding_minimal(&action_text)
            ));
        }
        if !user_text.is_empty() {
            q.push(format!(
                "user_id={}",
                crate::utils::url::urlencoding_minimal(&user_text)
            ));
        }
        if !from_text.is_empty() {
            q.push(format!(
                "from={}",
                crate::utils::url::urlencoding_minimal(&from_text)
            ));
        }
        if !to_text.is_empty() {
            q.push(format!(
                "to={}",
                crate::utils::url::urlencoding_minimal(&to_text)
            ));
        }
        let qstr = if q.is_empty() {
            String::new()
        } else {
            format!("?{}", q.join("&"))
        };
        // `api_base()` returns the API origin + `/api/v1`; the route
        // prefix matches the list endpoint above.
        let base = crate::hooks::fetch::api::api_base();
        format!("{base}/audit-log/export.csv{qstr}")
    };

    rsx! {
        AppLayout { title: "Audit Log",
            PageHeader {
                title: "Audit Log",
                subtitle: "Review who changed what, and when",
                // PMS-455: download CSV of the currently-filtered set.
                actions: rsx! {
                    a {
                        href: "{export_href}",
                        // `download` attr nudges the browser to use
                        // the Content-Disposition filename ("audit-log.csv")
                        // rather than navigating into the response.
                        "download": "audit-log.csv",
                        class: "inline-flex items-center px-4 py-2 text-sm font-medium rounded-md border border-line hover:bg-surface-2 text-content",
                        "Download CSV"
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5",
                    crate::components::Input {
                        name: "entity_type",
                        label: "Entity type",
                        placeholder: "e.g. ticket",
                        value: entity_type_filter.read().clone(),
                        oninput: move |e: FormEvent| {
                            entity_type_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    Select {
                        name: "action",
                        label: "Action",
                        options: action_options,
                        value: action_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            action_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    crate::components::Input {
                        name: "user_id",
                        label: "User ID",
                        placeholder: "UUID",
                        value: user_id_filter.read().clone(),
                        oninput: move |e: FormEvent| {
                            user_id_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    crate::components::DateField {
                        name: "from",
                        label: "From",
                        value: from_filter.read().clone(),
                        oninput: move |e: FormEvent| {
                            from_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    crate::components::DateField {
                        name: "to",
                        label: "To",
                        value: to_filter.read().clone(),
                        oninput: move |e: FormEvent| {
                            to_filter.set(e.value());
                            page.set(1);
                        },
                    }
                }
            }

            if let Some(message) = error_message {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "{message}"
                }
            }

            // Audit table
            DataTable {
                loading: is_loading,
                total_items: total as usize,
                current_page,
                per_page: PER_PAGE,
                columns: 7,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Timestamp" }
                            TableHeader { "Action" }
                            TableHeader { "Entity" }
                            TableHeader { "Record" }
                            TableHeader { "User" }
                            TableHeader { "IP" }
                            TableHeader { "Details" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 7, rows: 5 }
                    } else if page_rows.is_empty() {
                        if has_filters {
                            // Filtered to nothing: no CTA (entries are recorded by
                            // the system); guide the user back to the filters.
                            TableEmpty {
                                columns: 7,
                                title: "No audit entries match your filters".to_string(),
                                description: "Try clearing or adjusting the filters above.".to_string(),
                            }
                        } else {
                            // PMS-354: no CTA - audit entries are recorded by
                            // the system, not created by the user.
                            TableEmpty {
                                columns: 7,
                                title: "No activity yet".to_string(),
                                description: "Changes across the app will appear here as they happen.".to_string(),
                            }
                        }
                    } else {
                        TableBody {
                            for entry in page_rows.iter().cloned() {
                                AuditRow { key: "{entry.id}", entry, users: users.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AuditRowProps {
    entry: AuditLogEntry,
    users: Vec<RemoteUser>,
}

/// One audit row plus a collapsible detail row holding the pretty-printed
/// old_values -> new_values JSON diff. The expand state is per-row so
/// opening one entry does not affect the others.
#[component]
fn AuditRow(props: AuditRowProps) -> Element {
    let AuditRowProps { entry, users } = props;
    let mut expanded = use_signal(|| false);

    let has_detail =
        entry.old_values.is_some() || entry.new_values.is_some() || entry.user_agent.is_some();

    let timestamp = format_timestamp(entry.timestamp);
    let action = entry.action.clone();
    let variant = action_variant(&action);
    let entity_type = entry.entity_type.clone();
    let ip = entry.ip_address.clone().unwrap_or_else(|| "-".to_string());

    let old_json = pretty_json(&entry.old_values);
    let new_json = pretty_json(&entry.new_values);
    // PMS-365: field-by-field diff with humanized labels + user-FK resolution.
    // Empty when the payloads are not JSON objects; then we fall back to the
    // raw pretty-printed JSON below.
    let diff_rows = field_diff_rows(&entry.old_values, &entry.new_values, &users);
    let user_agent = entry.user_agent.clone().unwrap_or_default();
    let entity_id_full = entry
        .entity_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "-".to_string());
    let user_id_full = entry
        .user_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| "-".to_string());

    // Resolved labels with sensible fallbacks. Server-side LEFT JOIN +
    // CASE-per-entity-type populates the name fields whenever it can;
    // when it can't (entity deleted, new entity_type with no resolver
    // yet) we render the short UUID prefix so the row still says
    // SOMETHING the reader can correlate with logs.
    let entity_label = entry
        .entity_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| short_uuid(entry.entity_id));
    let user_label = entry
        .user_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "System".to_string());

    let is_open = *expanded.read();

    rsx! {
        TableRow {
            TableCell { class: "text-muted font-mono text-xs", "{timestamp}" }
            TableCell {
                Badge { variant, "{action}" }
            }
            TableCell { "{entity_type}" }
            TableCell { "{entity_label}" }
            TableCell { "{user_label}" }
            TableCell { class: "text-muted", "{ip}" }
            TableCell {
                if has_detail {
                    button {
                        r#type: "button",
                        class: "text-sm text-accent hover:opacity-90",
                        onclick: move |_| {
                            let next = !*expanded.read();
                            expanded.set(next);
                        },
                        if is_open { "Hide" } else { "View" }
                    }
                } else {
                    span { class: "text-subtle", "-" }
                }
            }
        }
        if is_open {
            tr {
                td {
                    colspan: "7",
                    class: "px-6 py-4 bg-surface-2",
                    dl { class: "grid grid-cols-1 gap-3 sm:grid-cols-2 text-xs",
                        // Detail view shows the resolved name as the
                        // primary value; the raw UUID drops to a muted
                        // sub-line for traceability when chasing rows
                        // across logs / DB queries. When no UUID exists
                        // (system-issued or null fields) the sub-line is
                        // suppressed entirely.
                        div {
                            dt { class: "font-medium text-muted mb-1","Record" }
                            dd { class: "text-content break-all","{entity_label}" }
                            if entry.entity_id.is_some() {
                                dd { class: "font-mono text-subtle text-[10px] mt-0.5 break-all","{entity_id_full}" }
                            }
                        }
                        div {
                            dt { class: "font-medium text-muted mb-1","User" }
                            dd { class: "text-content break-all","{user_label}" }
                            if entry.user_id.is_some() {
                                dd { class: "font-mono text-subtle text-[10px] mt-0.5 break-all","{user_id_full}" }
                            }
                        }
                        if !user_agent.is_empty() {
                            div { class: "sm:col-span-2",
                                dt { class: "font-medium text-muted mb-1","User agent" }
                                dd { class: "text-content break-all","{user_agent}" }
                            }
                        }
                        if diff_rows.is_empty() {
                            // Non-object payloads: keep the raw JSON view.
                            div {
                                dt { class: "font-medium text-muted mb-1","Old values" }
                                dd {
                                    pre { class: "overflow-x-auto rounded-md bg-surface border border-line p-3 font-mono text-content",
                                        "{old_json}"
                                    }
                                }
                            }
                            div {
                                dt { class: "font-medium text-muted mb-1","New values" }
                                dd {
                                    pre { class: "overflow-x-auto rounded-md bg-surface border border-line p-3 font-mono text-content",
                                        "{new_json}"
                                    }
                                }
                            }
                        } else {
                            div { class: "sm:col-span-2",
                                dt { class: "font-medium text-muted mb-1","Changes" }
                                dd {
                                    div { class: "space-y-1",
                                        for (label , oldv , newv) in diff_rows.iter().cloned() {
                                            div { class: "flex flex-wrap items-baseline gap-x-2 gap-y-0.5",
                                                span { class: "font-medium text-content", "{label}" }
                                                span { class: "text-muted line-through break-all", "{oldv}" }
                                                span { class: "text-subtle", "\u{2192}" }
                                                span { class: "text-content break-all", "{newv}" }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jane() -> RemoteUser {
        RemoteUser {
            id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            full_name: "Jane Doe".to_string(),
        }
    }

    #[test]
    fn humanize_field_key_strips_id_suffix_and_sentence_cases() {
        assert_eq!(humanize_field_key("project_manager_id"), "Project manager");
        assert_eq!(humanize_field_key("assigned_to_id"), "Assigned to");
        assert_eq!(humanize_field_key("company_id"), "Company");
        assert_eq!(humanize_field_key("status"), "Status");
    }

    #[test]
    fn display_field_value_resolves_user_fk_to_name() {
        let users = vec![jane()];
        let v = json!("11111111-1111-1111-1111-111111111111");
        assert_eq!(
            display_field_value("project_manager_id", Some(&v), &users),
            "Jane Doe"
        );
    }

    #[test]
    fn display_field_value_unknown_user_fk_falls_back_to_short_prefix() {
        let v = json!("22222222-2222-2222-2222-222222222222");
        assert_eq!(
            display_field_value("assigned_to_id", Some(&v), &[]),
            "22222222"
        );
    }

    #[test]
    fn display_field_value_null_and_empty_render_empty() {
        assert_eq!(display_field_value("status", None, &[]), "(empty)");
        assert_eq!(
            display_field_value("status", Some(&serde_json::Value::Null), &[]),
            "(empty)"
        );
        assert_eq!(
            display_field_value("status", Some(&json!("")), &[]),
            "(empty)"
        );
    }

    #[test]
    fn display_field_value_non_user_fk_passes_through() {
        // A non-user FK id is shown raw, not resolved.
        let v = json!("11111111-1111-1111-1111-111111111111");
        assert_eq!(
            display_field_value("company_id", Some(&v), &[jane()]),
            "11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn field_diff_rows_unions_keys_new_first_then_old_only() {
        let old = Some(json!({"status": "open", "title": "A"}));
        let new = Some(json!({"title": "B", "priority": "high"}));
        let rows = field_diff_rows(&old, &new, &[]);
        let labels: Vec<&str> = rows.iter().map(|(l, _, _)| l.as_str()).collect();
        // serde_json's Map is a BTreeMap, so keys are alphabetical within each
        // group: new keys first (priority, title), then old-only (status).
        assert_eq!(labels, vec!["Priority", "Title", "Status"]);
        // title changed A -> B.
        let title = rows.iter().find(|(l, _, _)| l == "Title").unwrap();
        assert_eq!(title.1, "A");
        assert_eq!(title.2, "B");
        // priority only in new -> old is (empty).
        let prio = rows.iter().find(|(l, _, _)| l == "Priority").unwrap();
        assert_eq!(prio.1, "(empty)");
        assert_eq!(prio.2, "high");
    }

    #[test]
    fn field_diff_rows_empty_for_non_object_payloads() {
        assert!(field_diff_rows(&None, &None, &[]).is_empty());
        assert!(field_diff_rows(&Some(json!("scalar")), &None, &[]).is_empty());
    }
}

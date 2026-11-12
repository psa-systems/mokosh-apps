//! mokosh-contact-login prompt 007: Settings > Contact Roles.
//!
//! Sibling page module (not a submodule of `settings`) so the existing
//! `src/pages/settings.rs` stays intact. Two pages:
//!
//! - `ContactRolesListPage` (`/settings/contact-roles`): table of the
//!   tenant's portal roles with edit + delete affordances. Delete is
//!   visibly disabled with a tooltip for built-in roles or for roles
//!   that still have contacts assigned.
//! - `ContactRoleEditPage` (`/settings/contact-roles/{id}`): the same
//!   editor shape for create (`id == "new"`) and edit (`id == "<uuid>"`).
//!   Name field + a grouped checkbox list of capabilities. Built-in
//!   roles cannot have capabilities unchecked (their capability set is
//!   read-only server-side), but can be renamed.
//!
//! Server contract (prompt 007 spec + `mokosh-server` `portal_roles`
//! module):
//!   - `GET  /api/v1/portal-roles`               -> `Vec<PortalRoleSummary>`
//!   - `GET  /api/v1/portal-roles/{id}`          -> `PortalRole`
//!   - `POST /api/v1/portal-roles`               -> `PortalRole`
//!   - `PUT  /api/v1/portal-roles/{id}`          -> `PortalRole`
//!   - `DELETE /api/v1/portal-roles/{id}`
//!   - `GET  /api/v1/portal-roles/capabilities`  -> `ListCapabilitiesResponse`
//!
//! Both pages are admin only (MAPPS-1211). The server's own admin check on
//! `/portal-roles` is the authorization boundary; the in-page gate is what
//! keeps a non-admin who reaches either URL directly from being shown an
//! editor they cannot save from, and it is what every sibling settings page
//! already did.
//!
//! Every mutation runs `use_can_mutate` so the write buttons flip off
//! when the server flag is down (same posture as the rest of Settings).
//! Non-web builds compile: every `web_sys` and `hooks::fetch::api::*`
//! call sits under `#[cfg(feature = "web")]`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, BreadcrumbItem, Breadcrumbs, Button, ButtonVariant, Card, DataTable,
    ErrorBanner, IconSize, Input, PageHeader, PlusIcon, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
};
use crate::pages::settings::AdminOnlyNotice;
use crate::Route;

/// Comma-list of a role's capability keys, ellipsized past this many
/// characters so the row stays one line at typical grid widths.
const CAPABILITY_LIST_TRUNCATE: usize = 60;

/// One row of `GET /api/v1/portal-roles`. Mirrors the server's
/// `PortalRoleSummary` (see `mokosh_server::modules::contacts::models`).
/// `contacts_count` is optional: the prompt-007 spec calls it out but
/// the server currently returns only `id / name / capabilities /
/// is_builtin`; a missing field falls through to `None` and the list
/// renders "-" instead of a count.
///
/// MAPPS-590 (prompt 012): `company_id` distinguishes tenant-wide rows
/// (`None`) from Company-scoped rows (`Some(<uuid>)`). Absent from
/// pre-PMS-929 server responses; `#[serde(default)]` keeps decoding
/// safe. Settings > Contact Roles renders only tenant-wide rows in
/// practice, but the field is carried so the same struct can back the
/// Company detail page's scoped-roles table.
// SPEC-8.2.2: `capabilities`/`is_builtin` default on absence; `contacts_count`
// stays `None` (not `0`) when the server omits it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct PortalRoleWire {
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) capabilities: Vec<String>,
    #[serde(default)]
    pub(crate) is_builtin: bool,
    #[serde(default)]
    pub(crate) contacts_count: Option<u32>,
    #[serde(default)]
    pub(crate) company_id: Option<uuid::Uuid>,
}

/// Full portal-role row returned by `GET /api/v1/portal-roles/{id}`
/// (and by the POST / PUT responses). The editor only reads `name`,
/// `capabilities`, and `is_builtin`; the rest of the server shape
/// (tenant_id / timestamps) is ignored via `#[serde(default)]` +
/// `deny_unknown_fields = false` (the default).
#[derive(Deserialize, Clone, Debug, PartialEq)]
struct PortalRoleDetailWire {
    #[allow(dead_code)]
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    is_builtin: bool,
}

/// One capability descriptor from `GET /api/v1/portal-roles/capabilities`.
/// Mirrors the server's `CapabilityDescriptor`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
struct CapabilityDescriptorWire {
    key: String,
    label: String,
    group: String,
    description: String,
}

#[derive(Deserialize, Clone, Debug)]
struct ListCapabilitiesResponseWire {
    #[serde(default)]
    capabilities: Vec<CapabilityDescriptorWire>,
}

#[derive(Serialize)]
struct CreateRoleBody {
    name: String,
    capabilities: Vec<String>,
}

// SPEC-8.2.2: unlike `CreateRoleBody`, a `None` here still serializes as
// `null` (no `skip_serializing_if`); the submit path below only sets a field
// to `Some` when it actually changed.
#[derive(Serialize)]
struct UpdateRoleBody {
    name: Option<String>,
    capabilities: Option<Vec<String>>,
}

/// Trim a capability comma-list to at most `CAPABILITY_LIST_TRUNCATE`
/// characters, appending an ellipsis marker when it had to be cut. Kept
/// as a free function (not a method) so it round-trips through
/// `#[cfg(test)]` on native without pulling in Dioxus state.
// SPEC-8.1.9: 60-character truncation with a trailing "...".
fn truncate_capability_list(caps: &[String]) -> String {
    let joined = caps.join(", ");
    if joined.chars().count() <= CAPABILITY_LIST_TRUNCATE {
        return joined;
    }
    let mut out = String::new();
    for c in joined.chars() {
        if out.chars().count() >= CAPABILITY_LIST_TRUNCATE {
            break;
        }
        out.push(c);
    }
    out.push_str("...");
    out
}

// ============================================================================
// List page
// ============================================================================

/// `/settings/contact-roles` - list every portal role in the tenant.
///
/// MAPPS-1211: admin only in the page, not only in the Settings hub. The
/// surface registration's `StaffAdmin` visibility hides the tile; it cannot
/// stop a bookmark, a back button or a pasted link, and this page's own
/// module doc has always described an MSP-admin editor.
///
/// Split into a gate and a body for the reason every sibling page is
/// (MAPPS-602): the early return has to sit where the hook count cannot
/// change across renders. `use_is_admin` reads the auth context, so it
/// answers false until `/me` resolves and true after. With the body's hooks
/// in the same component, the render before `/me` lands would run four hooks
/// fewer than the render after it, and dioxus panics on the hook index. WASM
/// does not unwind that panic, so the page stops responding entirely.
#[component]
pub fn ContactRolesListPage() -> Element {
    use_page_title("Contact Roles");
    // SPEC-8.4.1: gate before the body so a non-admin makes no request.
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Contact Roles" } };
    }
    rsx! { ContactRolesListBody {} }
}

#[component]
fn ContactRolesListBody() -> Element {
    let mut resource = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // Match the rest of Settings: auto-refetch the instant the
        // server flag flips back to reachable.
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            // SPEC-8.5.2 / SPEC-8.7.2: GET /portal-roles; failure is logged and
            // surfaced as the list-load error banner.
            crate::hooks::fetch::api::get_authed_typed::<Vec<PortalRoleWire>>("/portal-roles")
                .await
                .inspect_err(|e| tracing::error!("contact role list load failed: {e}"))
                .ok()
        }
        // SPEC-8.3.6: no-op on a build without the `web` feature.
        #[cfg(not(feature = "web"))]
        {
            None::<Vec<PortalRoleWire>>
        }
    });

    // SPEC-8.1.7: loading table / error banner / empty-table / rows states
    // for the list, derived from one resource snapshot.
    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<PortalRoleWire> = match &*snap {
        Some(Some(list)) => list.clone(),
        _ => Vec::new(),
    };

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Contact Roles".to_string() }
        };
    }

    let total = rows.len();

    rsx! {
        PageHeader {
            title: "Contact Roles",
            subtitle: "Portal roles you assign to contacts. Each role bundles a set of capabilities.",
            breadcrumbs: rsx! {
                Breadcrumbs {
                    items: vec![
                        BreadcrumbItem { label: "Settings".to_string(), route: Some(Route::SettingsHome {}) },
                        BreadcrumbItem { label: "Contact Roles".to_string(), route: None },
                    ],
                }
            },
            actions: rsx! {
                Link { to: Route::ContactRoleEdit { id: "new".to_string() },
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create while the server is unreachable".to_string()),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New role"
                    }
                }
            },
        }

        if fetch_failed {
            div { class: "mb-4",
                ErrorBanner {
                    "Could not load portal roles. Refresh the page to retry."
                }
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total,
            current_page: 1,
            per_page: total.max(1),
            columns: 4,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Capabilities" }
                        TableHeader { class: "text-right", "Contacts" }
                        TableHeader { class: "text-right", "Actions" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 3 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 4,
                        message: "No portal roles yet. Click New role to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            ContactRoleRow {
                                key: "{row.id}",
                                row: row.clone(),
                                can_mutate,
                                on_deleted: move |_| resource.restart(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContactRoleRowProps {
    row: PortalRoleWire,
    can_mutate: bool,
    on_deleted: EventHandler<()>,
}

/// One table row in the list. Pulled out into its own component so the
/// per-row delete state (loading spinner + inline error) has its own
/// signal lifecycle instead of one shared signal for the whole table.
#[component]
fn ContactRoleRow(props: ContactRoleRowProps) -> Element {
    let row = props.row.clone();
    let can_mutate = props.can_mutate;
    let on_deleted = props.on_deleted;
    let id_str = row.id.to_string();

    let mut deleting = use_signal(|| false);
    // MAPPS-436: destructive-confirm guard requires the DELETE to fire
    // from a ConfirmDialog's `onconfirm`, never from a bare `onclick`.
    let mut confirming = use_signal(|| false);
    let mut delete_error = use_signal(String::new);

    // Delete gate: built-in OR still-in-use. The tooltip explains the
    // exact reason so the operator knows what to do next (remove
    // assignments, or leave the built-in row alone).
    // SPEC-8.1.8: priority order is built-in, then in-use, then
    // server-unreachable, each with its own disabled-reason tooltip text.
    let is_builtin = row.is_builtin;
    let contacts_count = row.contacts_count.unwrap_or(0);
    let in_use = contacts_count > 0;
    let disabled_reason: Option<String> = if is_builtin {
        Some("Built-in roles cannot be deleted.".to_string())
    } else if in_use {
        Some(format!(
            "{contacts_count} contact{plural} hold this role; remove those assignments first.",
            plural = if contacts_count == 1 { "" } else { "s" }
        ))
    } else if !can_mutate {
        Some("Can't delete while the server is unreachable.".to_string())
    } else {
        None
    };
    let can_delete = disabled_reason.is_none();

    let caps_display = truncate_capability_list(&row.capabilities);
    // SPEC-8.1.9: "-" for a contact count the server did not return (not "0").
    let contacts_display = match row.contacts_count {
        Some(n) => n.to_string(),
        None => "-".to_string(),
    };

    let delete_id = id_str.clone();
    let role_display_name = row.name.clone();
    // SPEC-8.6.2: checks its own in-flight flag, not just the disabled button.
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        delete_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                // SPEC-8.7.2 / SPEC-8.9.2: DELETE /portal-roles/{id}; a hard,
                // non-reversible delete, reachable only once SPEC-8.1.8's
                // three disable conditions all clear.
                let path = format!("/portal-roles/{}", id);
                match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Role deleted.".to_string(),
                        );
                        confirming.set(false);
                        on_deleted.call(());
                    }
                    Err(err) => {
                        // 400/409 (built-in or in-use, server-side) carry a
                        // useful message; surface it inside the still-open
                        // dialog next to the button that produced it.
                        delete_error.set(err.user_message());
                    }
                }
            }
            // SPEC-8.3.6: no-op on a build without the `web` feature.
            #[cfg(not(feature = "web"))]
            {
                let _ = id;
            }
            deleting.set(false);
        });
    };

    rsx! {
        TableRow {
            TableCell {
                span { class: "font-medium text-content", "{row.name}" }
                if is_builtin {
                    span { class: "ml-2 text-xs text-muted", "(built-in)" }
                }
            }
            TableCell {
                // SPEC-8.1.9: tooltip carries the untruncated list; "-" when
                // there are no capabilities at all.
                span {
                    class: "text-sm text-muted",
                    title: row.capabilities.join(", "),
                    if caps_display.is_empty() {
                        "-"
                    } else {
                        "{caps_display}"
                    }
                }
            }
            TableCell { class: "text-right", "{contacts_display}" }
            TableCell { class: "text-right",
                div { class: "flex justify-end gap-2",
                    Link { to: Route::ContactRoleEdit { id: id_str.clone() },
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: crate::components::ButtonSize::Small,
                            "Edit"
                        }
                    }
                    // SPEC-8.6.1: disabled (and showing its own loading state)
                    // for the duration of an in-flight delete.
                    Button {
                        variant: ButtonVariant::Danger,
                        size: crate::components::ButtonSize::Small,
                        disabled: !can_delete,
                        loading: *deleting.read(),
                        title: disabled_reason.clone(),
                        // SPEC-8.4.3: this click only opens the confirm dialog;
                        // the delete itself fires from the dialog's own confirm.
                        onclick: move |_| {
                            if can_delete {
                                delete_error.set(String::new());
                                confirming.set(true);
                            }
                        },
                        "Delete"
                    }
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming(),
            title: format!("Delete portal role \"{role_display_name}\""),
            message: "This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            error: delete_error.read().clone(),
            loading: deleting(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !deleting() {
                    confirming.set(false);
                    delete_error.set(String::new());
                }
            },
        }
    }
}

// ============================================================================
// Edit / create page
// ============================================================================

/// Terminal state of the role-fetch resource. Distinct from the outer
/// `Option` (`Some` = resource resolved, `None` = still loading), so
/// the render below can pattern-match on ready + kind in one arm.
#[derive(Clone, Debug, PartialEq)]
enum RoleLoad {
    /// Create mode. No fetch was performed; the form starts blank.
    New,
    /// Edit mode. Server returned the role successfully.
    Loaded(PortalRoleDetailWire),
    /// Edit mode. Server fetch failed. Render an inline error rather
    /// than seed a blank form (would silently overwrite on save).
    Failed,
}

/// `/settings/contact-roles/{id}` - create (id == "new") or edit a
/// portal role. Loads the capability descriptor list on mount so the
/// checkboxes carry human-friendly labels + tooltips grouped by domain.
///
/// MAPPS-1211: admin only, the same gate and the same split as the list page.
/// The deep link this one answers is the more reachable of the two: an edit
/// URL carries a role id, so it is the one that lands in a browser history
/// and in a pasted link.
#[component]
pub fn ContactRoleEditPage(id: String) -> Element {
    let title = if id == "new" {
        "New Portal Role"
    } else {
        "Edit Portal Role"
    };
    use_page_title(title.to_string());
    // SPEC-8.4.1: gate before the body so a non-admin makes no request.
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: title.to_string() } };
    }
    rsx! { ContactRoleEditBody { id } }
}

#[component]
fn ContactRoleEditBody(id: String) -> Element {
    let title = if id == "new" {
        "New Portal Role"
    } else {
        "Edit Portal Role"
    };

    // Existing-role fetch: skipped when `id == "new"` (short-circuits
    // to `RoleLoad::New`). A network/HTTP failure lands as
    // `RoleLoad::Failed` so the render can distinguish loading from
    // failure without a triple-nested `Option<Option<Option<_>>>`.
    let id_for_role = id.clone();
    let role_resource = use_resource(move || {
        let id = id_for_role.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            // SPEC-8.1.10: skip the fetch and start from a blank form for "new".
            if id == "new" {
                return RoleLoad::New;
            }
            #[cfg(feature = "web")]
            {
                // SPEC-8.7.2: GET /portal-roles/{id}.
                let path = format!("/portal-roles/{}", id);
                match crate::hooks::fetch::api::get_authed_typed::<PortalRoleDetailWire>(&path)
                    .await
                {
                    Ok(r) => RoleLoad::Loaded(r),
                    Err(_) => RoleLoad::Failed,
                }
            }
            // SPEC-8.3.6: no-op on a build without the `web` feature.
            #[cfg(not(feature = "web"))]
            {
                let _ = id;
                RoleLoad::Failed
            }
        }
    });

    let caps_resource = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            // SPEC-8.5.2 / SPEC-8.7.2: GET /portal-roles/capabilities; a
            // separate error log from the list-load failure above.
            crate::hooks::fetch::api::get_authed_typed::<ListCapabilitiesResponseWire>(
                "/portal-roles/capabilities",
            )
            .await
            .inspect_err(|e| tracing::error!("portal capability list load failed: {e}"))
            .ok()
        }
        // SPEC-8.3.6: no-op on a build without the `web` feature.
        #[cfg(not(feature = "web"))]
        {
            None::<ListCapabilitiesResponseWire>
        }
    });

    let role_snap = role_resource.read_unchecked();
    let caps_snap = caps_resource.read_unchecked();

    let reachable = crate::hooks::use_server_reachable();

    // Loading state: either fetch has not resolved yet.
    // SPEC-8.1.10: rendering stays in the loading state until both the role
    // fetch and the capability-descriptor fetch resolve.
    if role_snap.is_none() || caps_snap.is_none() {
        return rsx! {
            EditPageChrome { title: title.to_string(), name: String::new(),
                crate::components::DetailSkeleton {}
            }
        };
    }

    // Failure state on the role fetch (existing edit only). A missing
    // capability descriptor list is a soft failure (form still renders
    // with an empty capability list); a failed role fetch has no
    // recoverable form to render.
    // SPEC-8.1.10: unavailable notice when the server itself is unreachable,
    // otherwise the generic role-fetch-failed message.
    if matches!(&*role_snap, Some(RoleLoad::Failed)) {
        if !reachable {
            return rsx! {
                crate::components::ContentUnavailable { title: title.to_string() }
            };
        }
        return rsx! {
            EditPageChrome { title: title.to_string(), name: String::new(),
                Card {
                    div { class: "py-12 text-center",
                        ErrorBanner { "Could not load this role. Refresh the page to retry." }
                    }
                }
            }
        };
    }

    let existing: Option<PortalRoleDetailWire> = match &*role_snap {
        Some(RoleLoad::Loaded(r)) => Some(r.clone()),
        _ => None,
    };
    let capabilities: Vec<CapabilityDescriptorWire> = caps_snap
        .as_ref()
        .and_then(|s| s.as_ref())
        .map(|s| s.capabilities.clone())
        .unwrap_or_default();

    rsx! {
        ContactRoleEditForm {
            id: id.clone(),
            existing,
            capabilities,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EditPageChromeProps {
    title: String,
    name: String,
    children: Element,
}

/// Shared header + breadcrumb slot for the edit page's loading /
/// failure states, so all three renders (loading, failure, ready) show
/// the same chrome.
#[component]
fn EditPageChrome(props: EditPageChromeProps) -> Element {
    let leaf = if props.name.trim().is_empty() {
        props.title.clone()
    } else {
        props.name.clone()
    };
    rsx! {
        PageHeader {
            title: props.title.clone(),
            breadcrumbs: rsx! {
                Breadcrumbs {
                    items: vec![
                        BreadcrumbItem { label: "Settings".to_string(), route: Some(Route::SettingsHome {}) },
                        BreadcrumbItem { label: "Contact Roles".to_string(), route: Some(Route::ContactRolesList {}) },
                        BreadcrumbItem { label: leaf, route: None },
                    ],
                }
            },
        }
        {props.children}
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContactRoleEditFormProps {
    id: String,
    existing: Option<PortalRoleDetailWire>,
    capabilities: Vec<CapabilityDescriptorWire>,
}

#[component]
fn ContactRoleEditForm(props: ContactRoleEditFormProps) -> Element {
    let is_new = props.id == "new";
    let is_builtin = props
        .existing
        .as_ref()
        .map(|r| r.is_builtin)
        .unwrap_or(false);

    let initial_name = props
        .existing
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let initial_caps: Vec<String> = props
        .existing
        .as_ref()
        .map(|r| r.capabilities.clone())
        .unwrap_or_default();

    let mut name = use_signal(|| initial_name.clone());
    let mut selected: Signal<Vec<String>> = use_signal(|| initial_caps.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let can_mutate = crate::hooks::use_can_mutate();
    let nav = use_navigator();

    let title = if is_new {
        "New Portal Role"
    } else {
        "Edit Portal Role"
    };
    let leaf = if is_new {
        "New".to_string()
    } else if initial_name.trim().is_empty() {
        title.to_string()
    } else {
        initial_name.clone()
    };

    // Group the capability descriptors by their `group` field, preserving
    // the server's declared order (first appearance wins). Keeping this
    // as a pass over the vector rather than a HashMap so the display
    // order is deterministic across renders.
    // SPEC-8.1.11: groups render in first-appearance order.
    let grouped: Vec<(String, Vec<CapabilityDescriptorWire>)> = {
        let mut order: Vec<String> = Vec::new();
        let mut by_group: std::collections::HashMap<String, Vec<CapabilityDescriptorWire>> =
            std::collections::HashMap::new();
        for cap in props.capabilities.iter().cloned() {
            if !order.iter().any(|g| g == &cap.group) {
                order.push(cap.group.clone());
            }
            by_group.entry(cap.group.clone()).or_default().push(cap);
        }
        order
            .into_iter()
            .map(|g| {
                let list = by_group.remove(&g).unwrap_or_default();
                (g, list)
            })
            .collect()
    };

    let id_for_submit = props.id.clone();
    let initial_caps_for_submit = initial_caps.clone();
    let initial_name_for_submit = initial_name.clone();
    let submit = move |_| {
        if *saving.read() || !can_mutate {
            return;
        }
        // SPEC-8.5.3: validation order is name-required, then name-length,
        // then (create only) at-least-one-capability.
        let new_name = name.read().trim().to_string();
        if new_name.is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        if new_name.chars().count() > 64 {
            error.set("Name must be 64 characters or fewer.".to_string());
            return;
        }
        // Guard against an accidental "no capabilities" submit on
        // create; the server rejects an empty set on update (empty
        // capabilities on a role would grant nothing).
        let picked = selected.read().clone();
        if picked.is_empty() && is_new {
            error.set("Pick at least one capability.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        let id = id_for_submit.clone();
        let initial_caps_snapshot = initial_caps_for_submit.clone();
        let initial_name_snapshot = initial_name_for_submit.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api;

                // SPEC-8.7.2: POST /portal-roles (create) or PUT
                // /portal-roles/{id} (update).
                let outcome: Result<PortalRoleDetailWire, api::ApiError> = if id == "new" {
                    // SPEC-8.2.2: CreateRoleBody always sends both fields.
                    let body = CreateRoleBody {
                        name: new_name.clone(),
                        capabilities: picked.clone(),
                    };
                    api::post_authed_typed::<PortalRoleDetailWire, _>("/portal-roles", &body).await
                } else {
                    // Only send fields that actually changed; a built-in
                    // role can only rename, so its `capabilities` stays
                    // `None` regardless of what the (disabled) checkboxes
                    // look like locally.
                    // SPEC-8.2.2: name sent only when it changed.
                    let name_change = if new_name != initial_name_snapshot {
                        Some(new_name.clone())
                    } else {
                        None
                    };
                    // SPEC-8.2.2 / SPEC-8.4.2: never report a changed
                    // `capabilities` for a built-in role.
                    let caps_change = if is_builtin {
                        None
                    } else if picked != initial_caps_snapshot {
                        Some(picked.clone())
                    } else {
                        None
                    };
                    let body = UpdateRoleBody {
                        name: name_change,
                        capabilities: caps_change,
                    };
                    let path = format!("/portal-roles/{}", id);
                    api::put_authed_typed::<PortalRoleDetailWire, _>(&path, &body).await
                };

                match outcome {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            if id == "new" {
                                "Role created.".to_string()
                            } else {
                                "Role saved.".to_string()
                            },
                        );
                        nav.push(Route::ContactRolesList {});
                    }
                    // SPEC-8.5.3: the server's own message when it returned
                    // a non-empty one, otherwise a generic fallback.
                    Err(api::ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(err) => error.set(err.user_message()),
                }
            }
            // SPEC-8.3.6: no-op on a build without the `web` feature.
            #[cfg(not(feature = "web"))]
            {
                let _ = (
                    id,
                    initial_caps_snapshot,
                    initial_name_snapshot,
                    new_name,
                    picked,
                );
            }
            saving.set(false);
        });
    };

    rsx! {
        PageHeader {
            title: title.to_string(),
            // SPEC-8.1.11: built-in copy swap for the subtitle.
            subtitle: if is_builtin {
                "Built-in role. You can rename it, but its capability set is fixed.".to_string()
            } else {
                "Name the role and pick the capabilities it grants.".to_string()
            },
            breadcrumbs: rsx! {
                Breadcrumbs {
                    items: vec![
                        BreadcrumbItem { label: "Settings".to_string(), route: Some(Route::SettingsHome {}) },
                        BreadcrumbItem { label: "Contact Roles".to_string(), route: Some(Route::ContactRolesList {}) },
                        BreadcrumbItem { label: leaf.clone(), route: None },
                    ],
                }
            },
            actions: rsx! {
                Link { to: Route::ContactRolesList {},
                    Button { variant: ButtonVariant::Secondary, "Back" }
                }
            },
        }

        Card {
            div { class: "space-y-6",
                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }

                // SPEC-8.1.11: required name field, capped at 64 characters.
                Input {
                    name: "role_name",
                    label: "Name",
                    value: name.read().clone(),
                    maxlength: 64i64,
                    required: true,
                    help: "Short label shown in the role picker on a contact.".to_string(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }

                div {
                    h3 { class: "text-sm font-semibold text-content mb-2", "Capabilities" }
                    if props.capabilities.is_empty() {
                        p { class: "text-sm text-muted",
                            "No capabilities available. The server did not return any capability descriptors."
                        }
                    } else {
                        div { class: "space-y-6",
                            for (group, list) in grouped.iter().cloned() {
                                div {
                                    key: "{group}",
                                    h4 { class: "text-xs font-semibold uppercase tracking-wide text-muted mb-2",
                                        "{group}"
                                    }
                                    div { class: "space-y-2",
                                        for cap in list.iter().cloned() {
                                            {
                                                let key = cap.key.clone();
                                                let key_click = cap.key.clone();
                                                let checked = selected.read().iter().any(|k| k == &key);
                                                // SPEC-8.1.11 / SPEC-8.4.2: every checkbox disabled for a
                                                // built-in role (the name field above stays editable).
                                                let disabled_here = is_builtin;
                                                rsx! {
                                                    label {
                                                        key: "{cap.key}",
                                                        class: "flex items-start gap-2",
                                                        title: cap.description.clone(),
                                                        input {
                                                            r#type: "checkbox",
                                                            class: "mt-1",
                                                            checked,
                                                            disabled: disabled_here,
                                                            onchange: move |evt: Event<FormData>| {
                                                                let want = evt.value() == "true" || evt.value() == "on";
                                                                let mut current = selected.read().clone();
                                                                if want {
                                                                    if !current.iter().any(|k| k == &key_click) {
                                                                        current.push(key_click.clone());
                                                                    }
                                                                } else {
                                                                    current.retain(|k| k != &key_click);
                                                                }
                                                                selected.set(current);
                                                            },
                                                        }
                                                        span { class: "text-sm text-content",
                                                            span { class: "font-medium", "{cap.label}" }
                                                            span { class: "ml-2 text-xs text-muted", "{cap.key}" }
                                                            if !cap.description.is_empty() {
                                                                div { class: "text-xs text-muted mt-0.5",
                                                                    "{cap.description}"
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
                        if is_builtin {
                            p { class: "text-xs text-muted mt-3",
                                "This role is built-in; its capability set is fixed. You can still rename it."
                            }
                        }
                    }
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: Route::ContactRolesList {},
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    // SPEC-8.6.1: disabled for the duration of an in-flight save.
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: !can_mutate || *saving.read(),
                        loading: *saving.read(),
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: submit,
                        if is_new { "Create role" } else { "Save Changes" }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

/// MAPPS-1211: both pages refuse a non-admin before they render anything.
///
/// `scripts/check-admin-gate.sh` covers the list page, because that one has a
/// `SETTINGS_SURFACES` entry for the script to start from. The edit page has
/// none: it is a child route reached from the list, so nothing in the registry
/// names it and no repository-wide scan can find it. It is also the more
/// reachable of the two by deep link, since an edit URL carries a role id.
/// That asymmetry is why this guard is here as well as there.
///
/// Scanned as source rather than asserted through a render because what it
/// defends is an ORDER: the refusal has to come before the body, and a render
/// test that mounted the gated page would prove only that the gate answers,
/// not that nothing ran ahead of it.
#[cfg(test)]
mod mapps1211_admin_gate_tests {
    /// The body of `fn <name>(` in this file, by brace depth.
    fn body_of<'a>(src: &'a str, name: &str) -> &'a str {
        let at = src
            .find(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("{name} is not defined in this file"));
        let open = src[at..].find('{').expect("a body") + at;
        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open..=open + i];
                    }
                }
                _ => {}
            }
        }
        panic!("{name}'s body does not close");
    }

    fn head() -> &'static str {
        let src = include_str!("settings_contact_roles.rs");
        &src[..src.find("/// MAPPS-1211: both pages").expect("this module")]
    }

    /// The refusal sits above the body, in both pages.
    #[test]
    fn both_pages_refuse_a_non_admin_before_rendering_a_body() {
        for (page, body) in [
            ("ContactRolesListPage", "ContactRolesListBody"),
            ("ContactRoleEditPage", "ContactRoleEditBody"),
        ] {
            let src = body_of(head(), page);
            let gate = src
                .find("if !crate::pages::settings::use_is_admin() {")
                .unwrap_or_else(|| panic!("{page} renders its body to a non-admin: {src}"));
            assert!(
                src.contains("AdminOnlyNotice {"),
                "{page} refuses without saying why: {src}"
            );
            let renders = src
                .find(&format!("{body} {{"))
                .unwrap_or_else(|| panic!("{page} does not delegate to {body}: {src}"));
            assert!(gate < renders, "{page} renders {body} first: {src}");
        }
    }

    /// And the split is real: the work moved into the body rather than the
    /// gate being parked in front of a shell that still does the work itself.
    /// A non-admin must not cause a `/portal-roles` request, which is both a
    /// guaranteed 403 in the log and a hook the gate would then sit below.
    #[test]
    fn neither_page_fetches_before_the_gate() {
        for (page, body) in [
            ("ContactRolesListPage", "ContactRolesListBody"),
            ("ContactRoleEditPage", "ContactRoleEditBody"),
        ] {
            let gate_src = body_of(head(), page);
            for forbidden in ["use_resource", "portal-roles", "use_server_reachable"] {
                assert!(
                    !gate_src.contains(forbidden),
                    "{page} does {forbidden} ahead of its own gate: {gate_src}"
                );
            }
            let body_src = body_of(head(), body);
            assert!(
                body_src.contains("use_resource"),
                "{body} holds no loads, so the split moved nothing"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Route;
    use std::str::FromStr;

    // Route resolution: prompt 007 spec pins two URLs. Regression guard
    // so a future rename of the route path doesn't silently 404 either.

    #[test]
    fn list_route_resolves() {
        let r = Route::from_str("/settings/contact-roles").expect("list route parses");
        assert!(matches!(r, Route::ContactRolesList {}));
    }

    #[test]
    fn edit_route_resolves_new() {
        let r = Route::from_str("/settings/contact-roles/new").expect("new route parses");
        match r {
            Route::ContactRoleEdit { id } => assert_eq!(id, "new"),
            other => panic!("expected ContactRoleEdit, got {other:?}"),
        }
    }

    #[test]
    fn edit_route_resolves_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let path = format!("/settings/contact-roles/{}", uuid);
        let r = Route::from_str(&path).expect("edit route parses");
        match r {
            Route::ContactRoleEdit { id } => assert_eq!(id, uuid),
            other => panic!("expected ContactRoleEdit, got {other:?}"),
        }
    }

    // Capability list truncation: short lists pass through untouched;
    // long lists cut at `CAPABILITY_LIST_TRUNCATE` and append the
    // ellipsis marker. Kept as raw string assertions (no chars() dance
    // at the call site) so a future tweak to the cutoff length shows
    // up as a legible diff.

    #[test]
    fn truncate_returns_dash_style_empty_for_no_caps() {
        assert_eq!(truncate_capability_list(&[]), "");
    }

    #[test]
    fn truncate_passes_short_list_through_unchanged() {
        let caps: Vec<String> = vec!["tickets:read".into(), "kb:read".into()];
        let out = truncate_capability_list(&caps);
        assert_eq!(out, "tickets:read, kb:read");
    }

    #[test]
    fn truncate_ellipsizes_long_list() {
        let caps: Vec<String> = vec![
            "tickets:read".into(),
            "tickets:comment".into(),
            "tickets:create".into(),
            "invoices:read".into(),
            "invoices:pay".into(),
            "quotes:read".into(),
        ];
        let out = truncate_capability_list(&caps);
        assert!(
            out.ends_with("..."),
            "expected trailing ellipsis, got: {out}"
        );
        assert!(
            out.chars().count() <= CAPABILITY_LIST_TRUNCATE + 3,
            "truncated string should be at most cutoff + '...', got {} chars: {out}",
            out.chars().count(),
        );
    }
}

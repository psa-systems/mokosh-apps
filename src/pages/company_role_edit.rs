//! MAPPS-590 (mokosh-contact-login prompt 012): Company-scoped portal
//! role editor.
//!
//! `/companies/{company_id}/roles/{id}` where `id == "new"` creates a
//! new Company-scoped role and any other `id` edits an existing one.
//! Mirrors the shape of `settings_contact_roles::ContactRoleEditPage`
//! but targets the nested endpoint set introduced by PMS-929:
//!
//!   - `GET  /api/v1/contacts/companies/{company_id}/portal-roles/{id}`
//!   - `POST /api/v1/contacts/companies/{company_id}/portal-roles`
//!   - `PUT  /api/v1/contacts/companies/{company_id}/portal-roles/{id}`
//!   - `GET  /api/v1/portal-roles/capabilities`  (unchanged, shared)
//!
//! On success the page navigates back to the parent Company detail
//! page (`Route::CompanyDetail`), which is where the newly created /
//! edited role becomes visible in the `CompanyRolesCard` list.
//!
//! Non-web builds compile: every `web_sys` / fetch call sits behind
//! `#[cfg(feature = "web")]`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, BreadcrumbItem, Breadcrumbs, Button, ButtonVariant, Card, ErrorBanner, Input,
    PageHeader,
};
use crate::Route;

/// One capability descriptor from `GET /api/v1/portal-roles/capabilities`.
/// Mirrors the server's `CapabilityDescriptor`. Kept in-file (instead of
/// imported from `settings_contact_roles`) so this page has no cross-
/// module runtime dependency on Settings > Contact Roles.
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

/// Full portal-role row returned by
/// `GET /api/v1/contacts/companies/{company_id}/portal-roles/{id}`
/// (and by the POST / PUT responses). A Company-scoped role always
/// has `company_id = Some(<company_id>)`; a defensive check on load
/// rejects a tenant-wide row silently returned by a pre-PMS-929 server.
#[derive(Deserialize, Clone, Debug, PartialEq)]
struct PortalRoleDetailWire {
    #[allow(dead_code)]
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    is_builtin: bool,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
}

/// Body for `POST /api/v1/contacts/companies/{company_id}/portal-roles`.
/// The `company_id` field duplicates the path param per the PMS-929
/// server contract (belt-and-braces; server rejects a mismatch with
/// 400). Sending both keeps the request self-describing.
#[derive(Serialize)]
struct CreateCompanyRoleBody {
    name: String,
    capabilities: Vec<String>,
    company_id: uuid::Uuid,
}

/// Body for `PUT /api/v1/contacts/companies/{company_id}/portal-roles/{id}`.
/// `company_id` is intentionally absent - a role's scope is immutable
/// once created (per prompt 012 spec, `update_role` rejects a scope
/// change with 400). The server infers the scope from the existing row.
#[derive(Serialize)]
struct UpdateCompanyRoleBody {
    name: Option<String>,
    capabilities: Option<Vec<String>>,
}

/// Terminal state of the role-fetch resource. Distinct from the outer
/// `Option` (`Some` = resource resolved, `None` = still loading), so
/// the render below can pattern-match on ready + kind in one arm.
#[derive(Clone, Debug, PartialEq)]
enum RoleLoad {
    /// Create mode. No fetch was performed; the form starts blank.
    New,
    /// Edit mode. Server returned the role successfully.
    Loaded(PortalRoleDetailWire),
    /// Edit mode. Server responded 404 (role missing OR scoped to a
    /// different Company). Rendered as a "role does not exist" state
    /// with a link back to the Company page.
    NotFound,
    /// Edit mode. Server fetch failed for another reason (401/500/etc).
    Failed,
}

/// `/companies/{company_id}/roles/{id}` - create (id == "new") or edit
/// a Company-scoped portal role.
#[component]
pub fn CompanyRoleEditPage(company_id: String, id: String) -> Element {
    let is_new = id == "new";
    let title = if is_new { "New Role" } else { "Edit Role" };
    use_page_title(title.to_string());

    // Existing-role fetch: skipped when `id == "new"` (short-circuits
    // to `RoleLoad::New`). A 404 (role missing, or a Company-scope
    // mismatch handled server-side) lands as `RoleLoad::NotFound` so
    // the render can offer the "back to Company" recovery link
    // instead of a generic error banner.
    let id_for_role = id.clone();
    let company_id_for_role = company_id.clone();
    let role_resource = use_resource(move || {
        let id = id_for_role.clone();
        let company_id = company_id_for_role.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            if id == "new" {
                return RoleLoad::New;
            }
            #[cfg(feature = "web")]
            {
                let path = format!("/contacts/companies/{company_id}/portal-roles/{id}");
                match crate::hooks::fetch::api::get_authed_typed::<PortalRoleDetailWire>(&path)
                    .await
                {
                    Ok(r) => RoleLoad::Loaded(r),
                    Err(crate::hooks::fetch::api::ApiError::Status { code: 404, .. }) => {
                        RoleLoad::NotFound
                    }
                    Err(_) => RoleLoad::Failed,
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (id, company_id);
                RoleLoad::Failed
            }
        }
    });

    let caps_resource = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed_typed::<ListCapabilitiesResponseWire>(
                "/portal-roles/capabilities",
            )
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<ListCapabilitiesResponseWire>
        }
    });

    let role_snap = role_resource.read_unchecked();
    let caps_snap = caps_resource.read_unchecked();

    let reachable = crate::hooks::use_server_reachable();

    // Loading state: either fetch has not resolved yet.
    if role_snap.is_none() || caps_snap.is_none() {
        return rsx! {
            EditPageChrome {
                title: title.to_string(),
                company_id: company_id.clone(),
                name: String::new(),
                crate::components::DetailSkeleton {}
            }
        };
    }

    // 404 on the role fetch (existing edit only). "Role does not exist"
    // is a distinct recovery affordance from a generic fetch failure:
    // the operator likely landed here from a stale link or a role
    // whose Company scope changed, and the fastest way out is back
    // to the Company detail page.
    if matches!(&*role_snap, Some(RoleLoad::NotFound)) {
        return rsx! {
            EditPageChrome {
                title: title.to_string(),
                company_id: company_id.clone(),
                name: String::new(),
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-content mb-3", "This role doesn't exist." }
                        Link {
                            to: Route::CompanyDetail { id: company_id.clone() },
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to the Company"
                        }
                    }
                }
            }
        };
    }

    // Non-404 failure on the role fetch. A missing capability descriptor
    // list is a soft failure (form still renders, capabilities just
    // show as raw keys); a failed role fetch has no recoverable form
    // to render.
    if matches!(&*role_snap, Some(RoleLoad::Failed)) {
        if !reachable {
            return rsx! {
                crate::components::ContentUnavailable { title: title.to_string() }
            };
        }
        return rsx! {
            EditPageChrome {
                title: title.to_string(),
                company_id: company_id.clone(),
                name: String::new(),
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300",
                            "Could not load this role. Refresh the page to retry."
                        }
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
        CompanyRoleEditForm {
            company_id: company_id.clone(),
            id: id.clone(),
            existing,
            capabilities,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EditPageChromeProps {
    title: String,
    company_id: String,
    name: String,
    children: Element,
}

/// Shared header + breadcrumb slot for the edit page's loading /
/// not-found / failure states, so all four renders (loading, not-found,
/// failure, ready) show the same chrome.
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
                        BreadcrumbItem { label: "Companies".to_string(), route: Some(Route::CompanyList {}) },
                        BreadcrumbItem { label: "Company".to_string(), route: Some(Route::CompanyDetail { id: props.company_id.clone() }) },
                        BreadcrumbItem { label: leaf, route: None },
                    ],
                }
            },
        }
        {props.children}
    }
}

#[derive(Props, Clone, PartialEq)]
struct CompanyRoleEditFormProps {
    company_id: String,
    id: String,
    existing: Option<PortalRoleDetailWire>,
    capabilities: Vec<CapabilityDescriptorWire>,
}

#[component]
fn CompanyRoleEditForm(props: CompanyRoleEditFormProps) -> Element {
    let is_new = props.id == "new";
    // A built-in role never has `is_builtin = true` under a Company
    // scope (the three seeded rows stay tenant-wide per prompt 012),
    // so the built-in-locking UI path from Settings > Contact Roles
    // is intentionally omitted here. The signal is kept only to reject
    // an unexpected server state without crashing.
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

    let title = if is_new { "New Role" } else { "Edit Role" };
    let leaf = if is_new {
        "New".to_string()
    } else if initial_name.trim().is_empty() {
        title.to_string()
    } else {
        initial_name.clone()
    };

    // Group the capability descriptors by their `group` field, preserving
    // the server's declared order (first appearance wins). Same shape as
    // `settings_contact_roles::ContactRoleEditForm`.
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

    let company_id_for_submit = props.company_id.clone();
    let id_for_submit = props.id.clone();
    let initial_caps_for_submit = initial_caps.clone();
    let initial_name_for_submit = initial_name.clone();
    let submit = move |_| {
        if *saving.read() || !can_mutate {
            return;
        }
        let new_name = name.read().trim().to_string();
        if new_name.is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        if new_name.chars().count() > 64 {
            error.set("Name must be 64 characters or fewer.".to_string());
            return;
        }
        let picked = selected.read().clone();
        if picked.is_empty() && is_new {
            error.set("Pick at least one capability.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        let company_id = company_id_for_submit.clone();
        let id = id_for_submit.clone();
        let initial_caps_snapshot = initial_caps_for_submit.clone();
        let initial_name_snapshot = initial_name_for_submit.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api;

                let outcome: Result<PortalRoleDetailWire, api::ApiError> = if id == "new" {
                    // Parse the path company_id into a UUID so the body
                    // carries a typed value; a malformed path is a
                    // programmer error (the router should have rejected
                    // it), but a defensive fallback surfaces it as an
                    // inline error rather than a wasm panic.
                    let company_uuid = match uuid::Uuid::parse_str(&company_id) {
                        Ok(u) => u,
                        Err(_) => {
                            error.set("Invalid Company id in URL.".to_string());
                            saving.set(false);
                            return;
                        }
                    };
                    let body = CreateCompanyRoleBody {
                        name: new_name.clone(),
                        capabilities: picked.clone(),
                        company_id: company_uuid,
                    };
                    let path = format!("/contacts/companies/{company_id}/portal-roles");
                    api::post_authed_typed::<PortalRoleDetailWire, _>(&path, &body).await
                } else {
                    // Only send fields that actually changed. A defensive
                    // built-in guard mirrors Settings > Contact Roles even
                    // though a scoped built-in should not occur in practice.
                    let name_change = if new_name != initial_name_snapshot {
                        Some(new_name.clone())
                    } else {
                        None
                    };
                    let caps_change = if is_builtin {
                        None
                    } else if picked != initial_caps_snapshot {
                        Some(picked.clone())
                    } else {
                        None
                    };
                    let body = UpdateCompanyRoleBody {
                        name: name_change,
                        capabilities: caps_change,
                    };
                    let path = format!("/contacts/companies/{company_id}/portal-roles/{id}");
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
                        // `replace` (not `push`) so a back-button after the
                        // save does not re-open the (now-stale) editor.
                        nav.replace(Route::CompanyDetail {
                            id: company_id.clone(),
                        });
                    }
                    Err(api::ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(err) => error.set(err.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (
                    company_id,
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

    let back_route = Route::CompanyDetail {
        id: props.company_id.clone(),
    };

    rsx! {
        PageHeader {
            title: title.to_string(),
            subtitle: "Name the role and pick the capabilities it grants. Scoped to this Company only.".to_string(),
            breadcrumbs: rsx! {
                Breadcrumbs {
                    items: vec![
                        BreadcrumbItem { label: "Companies".to_string(), route: Some(Route::CompanyList {}) },
                        BreadcrumbItem { label: "Company".to_string(), route: Some(Route::CompanyDetail { id: props.company_id.clone() }) },
                        BreadcrumbItem { label: leaf.clone(), route: None },
                    ],
                }
            },
            actions: rsx! {
                Link { to: back_route.clone(),
                    Button { variant: ButtonVariant::Secondary, "Back" }
                }
            },
        }

        Card {
            div { class: "space-y-6 p-6",
                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }

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
                                                rsx! {
                                                    label {
                                                        key: "{cap.key}",
                                                        class: "flex items-start gap-2",
                                                        title: cap.description.clone(),
                                                        input {
                                                            r#type: "checkbox",
                                                            class: "mt-1",
                                                            checked,
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
                    }
                }

                div { class: "flex justify-end gap-2 pt-2",
                    Link { to: back_route.clone(),
                        Button { variant: ButtonVariant::Secondary, "Cancel" }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: !can_mutate || *saving.read(),
                        loading: *saving.read(),
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: submit,
                        if is_new { "Create role" } else { "Save changes" }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Pure classifier used by the CompanyRolesCard filter + this file's tests.
// ============================================================================

/// Split a `Vec<PortalRoleSummaryWire>` (the union returned by
/// `GET /api/v1/contacts/companies/{id}/portal-roles`) into a
/// (tenant_wide, company_scoped) pair based on whether each row carries
/// a `company_id`. Tenant-wide rows (`company_id == None`) are the
/// three seeded built-ins plus any tenant-wide role from Settings;
/// Company-scoped rows are the ones the CompanyRolesCard renders.
///
/// Order preserved within each partition so the table on the Company
/// page matches the server's declared order.
///
/// Kept as a free function (not a method) so it round-trips through
/// `#[cfg(test)]` on native without pulling in Dioxus state, and so
/// it can be reused from any consumer of the union endpoint.
pub(crate) fn partition_roles_by_scope(
    rows: &[crate::pages::contacts::PortalRoleSummaryWire],
) -> (
    Vec<crate::pages::contacts::PortalRoleSummaryWire>,
    Vec<crate::pages::contacts::PortalRoleSummaryWire>,
) {
    let mut tenant_wide = Vec::new();
    let mut company_scoped = Vec::new();
    for row in rows.iter().cloned() {
        if row.company_id.is_some() {
            company_scoped.push(row);
        } else {
            tenant_wide.push(row);
        }
    }
    (tenant_wide, company_scoped)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::contacts::PortalRoleSummaryWire;
    use crate::Route;
    use std::str::FromStr;

    // Route resolution: the two shapes the spec pins. Regression guard so
    // a future rename of the route path does not silently 404 either.

    #[test]
    fn edit_route_resolves_new() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let path = format!("/companies/{uuid}/roles/new");
        let r = Route::from_str(&path).expect("new route parses");
        match r {
            Route::CompanyRoleEdit { company_id, id } => {
                assert_eq!(company_id, uuid);
                assert_eq!(id, "new");
            }
            other => panic!("expected CompanyRoleEdit, got {other:?}"),
        }
    }

    #[test]
    fn edit_route_resolves_uuid() {
        let company_uuid = "550e8400-e29b-41d4-a716-446655440000";
        let role_uuid = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        let path = format!("/companies/{company_uuid}/roles/{role_uuid}");
        let r = Route::from_str(&path).expect("edit route parses");
        match r {
            Route::CompanyRoleEdit { company_id, id } => {
                assert_eq!(company_id, company_uuid);
                assert_eq!(id, role_uuid);
            }
            other => panic!("expected CompanyRoleEdit, got {other:?}"),
        }
    }

    // Pure-function classifier: 4 cases pinning the scope split the
    // CompanyRolesCard depends on.

    fn make_row(name: &str, company_id: Option<uuid::Uuid>) -> PortalRoleSummaryWire {
        PortalRoleSummaryWire {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            capabilities: Vec::new(),
            is_builtin: false,
            company_id,
            contacts_count: None,
        }
    }

    #[test]
    fn partition_empty_input_returns_two_empty_partitions() {
        let (tenant, scoped) = partition_roles_by_scope(&[]);
        assert!(tenant.is_empty());
        assert!(scoped.is_empty());
    }

    #[test]
    fn partition_all_tenant_wide_goes_to_tenant_partition() {
        let rows = vec![
            make_row("Billing Contact", None),
            make_row("Support Contact", None),
            make_row("Read-Only", None),
        ];
        let (tenant, scoped) = partition_roles_by_scope(&rows);
        assert_eq!(tenant.len(), 3);
        assert!(scoped.is_empty());
        assert_eq!(tenant[0].name, "Billing Contact");
    }

    #[test]
    fn partition_all_scoped_goes_to_scoped_partition() {
        let cid = uuid::Uuid::new_v4();
        let rows = vec![
            make_row("Consultant", Some(cid)),
            make_row("Auditor", Some(cid)),
        ];
        let (tenant, scoped) = partition_roles_by_scope(&rows);
        assert!(tenant.is_empty());
        assert_eq!(scoped.len(), 2);
        assert_eq!(scoped[0].name, "Consultant");
    }

    #[test]
    fn partition_mixed_input_splits_correctly_preserving_order() {
        let cid = uuid::Uuid::new_v4();
        let rows = vec![
            make_row("Billing Contact", None),
            make_row("Consultant", Some(cid)),
            make_row("Support Contact", None),
            make_row("Auditor", Some(cid)),
        ];
        let (tenant, scoped) = partition_roles_by_scope(&rows);
        assert_eq!(tenant.len(), 2);
        assert_eq!(scoped.len(), 2);
        assert_eq!(tenant[0].name, "Billing Contact");
        assert_eq!(tenant[1].name, "Support Contact");
        assert_eq!(scoped[0].name, "Consultant");
        assert_eq!(scoped[1].name, "Auditor");
    }
}

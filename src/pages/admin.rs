//! Admin pages (multi-tenant only)

use dioxus::prelude::*;
#[cfg(feature = "multi-tenant")]
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, DataTable, ErrorBanner, PageHeader, StatCard, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};

/// Subset of mokosh-server's `TenantResponse` we render in the admin
/// tenant table. Serde drops unknown fields so adding columns later
/// just means extending this struct.
#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Deserialize)]
struct RemoteTenant {
    id: uuid::Uuid,
    name: String,
    slug: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    subscription_plan: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Deserialize)]
struct PaginatedTenants {
    data: Vec<RemoteTenant>,
}

#[cfg(feature = "multi-tenant")]
fn humanize_plan(raw: &Option<String>) -> String {
    match raw.as_deref() {
        None | Some("") => "Trial".into(),
        Some("trial") => "Trial".into(),
        Some("professional") => "Professional".into(),
        Some("enterprise") => "Enterprise".into(),
        Some(other) => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
fn humanize_tenant_status(raw: &str) -> String {
    match raw {
        "active" => "Active".into(),
        "suspended" => "Suspended".into(),
        "cancelled" => "Cancelled".into(),
        "" => "Active".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
fn format_created(when: chrono::DateTime<chrono::Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(when, Some(fmt)),
        None => when.format("%b %-d, %Y").to_string(),
    }
}

/// Tenant management page (multi-tenant mode only)
#[cfg(feature = "multi-tenant")]
#[component]
pub fn TenantManagementPage() -> Element {
    use_page_title("Tenant Management");
    // MAPPS-438: `None` is a failed load, exactly like the other list pages.
    // The roster renders only what the backend returned.
    let tenants_resource = use_resource(|| async {
        // F1: re-fetch on org switch / token swap so the roster reflects
        // the active scope instead of the prior tenant's cached rows.
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-351: also refetch on reconnect so the roster fills in once the
        // real backend answers again.
        let _reachable = crate::hooks::use_server_reachable();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<PaginatedTenants>("/tenants", &token)
            .await
            .ok()
            .map(|page| page.data)
    });

    let resource_snapshot = tenants_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let remote_tenants: Vec<RemoteTenant> = match &*resource_snapshot {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // MAPPS-351: a failed load while the server is flagged DOWN is an outage,
    // not an empty roster - show the honest unavailable state instead of an
    // empty table. A reachable no-token / 4xx keeps the inline error banner
    // below. Clears on reconnect (the resource subscribes to reachability
    // above).
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Tenant Management".to_string() }
        };
    }

    // Stat-card counts are derived from the same data the table renders,
    // so they stay in sync with the roster instead of the old hardcoded
    // 42/38/4 literals. MRR has no source field on the tenants API (per-row
    // MRR is "-" too), so it is shown as "-" rather than a fabricated dollar
    // figure.
    let (total_tenants, active_count, trial_count) = (
        remote_tenants.len(),
        remote_tenants
            .iter()
            .filter(|t| humanize_tenant_status(&t.status) == "Active")
            .count(),
        remote_tenants
            .iter()
            .filter(|t| humanize_plan(&t.subscription_plan) == "Trial")
            .count(),
    );
    let stat = |n: usize| -> String {
        if is_loading {
            "-".to_string()
        } else {
            n.to_string()
        }
    };
    let total_tenants_label = stat(total_tenants);
    let active_label = stat(active_count);
    let trial_label = stat(trial_count);

    rsx! {
        PageHeader {
            title: "Tenant Management",
            subtitle: "Manage tenants and subscriptions",
            // F5: Add Tenant was decorative (no onclick, no
            // canonical add-tenant flow on this client). Tenant
            // provisioning lives in the Bunyip hub now; this
            // admin page is a read-only roster for super_admins.
            // Hidden until/unless a client-side add flow lands.
        }

        // Stats
        div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-6",
            StatCard { label: "Total Tenants", value: "{total_tenants_label}" }
            StatCard { label: "Active", value: "{active_label}" }
            StatCard { label: "Trial", value: "{trial_label}" }
            StatCard { label: "MRR", value: "-" }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load tenants. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: remote_tenants.len(),
            current_page: 1,
            per_page: 25,
            columns: 6,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { sortable: true, "Tenant" }
                        TableHeader { "Plan" }
                        TableHeader { sortable: true, "Users" }
                        TableHeader { "MRR" }
                        TableHeader { "Status" }
                        TableHeader { sortable: true, "Created" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 4 }
                } else if remote_tenants.is_empty() {
                    TableEmpty {
                        columns: 6,
                        title: "No tenants yet".to_string(),
                        description: "Tenants will appear here once they sign up or are provisioned.".to_string(),
                    }
                } else {
                    TableBody {
                        for tenant in remote_tenants.iter().cloned() {
                            TenantRow {
                                key: "{tenant.id}",
                                name: tenant.name.clone(),
                                domain: tenant.slug.clone(),
                                plan: humanize_plan(&tenant.subscription_plan),
                                users: 0,
                                mrr: "-".to_string(),
                                status: humanize_tenant_status(&tenant.status),
                                created: format_created(tenant.created_at),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
#[derive(Props, Clone, PartialEq)]
struct TenantRowProps {
    name: String,
    domain: String,
    plan: String,
    users: u32,
    mrr: String,
    status: String,
    created: String,
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TenantRow(props: TenantRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Active" => BadgeVariant::Green,
        "Trial" => BadgeVariant::Blue,
        "Suspended" => BadgeVariant::Red,
        _ => BadgeVariant::Gray,
    };

    let plan_variant = match props.plan.as_str() {
        "Enterprise" => BadgeVariant::Purple,
        "Professional" => BadgeVariant::Blue,
        "Trial" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    };

    rsx! {
        TableRow {
            TableCell {
                div {
                    span { class: "font-medium text-content", "{props.name}" }
                    p { class: "text-sm text-muted", "{props.domain}.mokosh.app" }
                }
            }
            TableCell { Badge { variant: plan_variant, "{props.plan}" } }
            TableCell { "{props.users}" }
            TableCell { class: "font-medium", "{props.mrr}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-muted", "{props.created}" }
        }
    }
}

// Fallback for single-tenant mode
#[cfg(not(feature = "multi-tenant"))]
#[component]
pub fn TenantManagementPage() -> Element {
    rsx! {
        div { "Tenant management is not available in single-tenant mode." }
    }
}

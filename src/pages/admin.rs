//! Admin pages (multi-tenant only)

use dioxus::prelude::*;
#[cfg(feature = "multi-tenant")]
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow,
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
#[derive(Clone, Copy, Debug, PartialEq)]
enum TenantSource {
    Backend,
    Demo,
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
fn humanize_status(raw: &str) -> String {
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
    when.format("%b %-d, %Y").to_string()
}

/// Tenant management page (multi-tenant mode only)
#[cfg(feature = "multi-tenant")]
#[component]
pub fn TenantManagementPage() -> Element {
    // Try the live tenants endpoint first; fall back to seeded demo
    // rows so the page stays demoable for envs without a live backend.
    let tenants_resource = use_resource(|| async {
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteTenant>::new(), TenantSource::Demo),
        };
        match crate::hooks::fetch::api::get_with_auth::<PaginatedTenants>(
            "/tenants",
            &token,
        )
        .await
        {
            Ok(page) => (page.data, TenantSource::Backend),
            Err(_) => (Vec::new(), TenantSource::Demo),
        }
    });

    let resource_snapshot = tenants_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_tenants, source) = match &*resource_snapshot {
        Some((rows, source)) => (rows.clone(), *source),
        None => (Vec::new(), TenantSource::Demo),
    };

    rsx! {
        AppLayout { title: "Tenant Management",
            PageHeader {
                title: "Tenant Management",
                subtitle: "Manage tenants and subscriptions",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add Tenant"
                    }
                },
            }

            // Stats
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Total Tenants" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "42" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Active" }
                    p { class: "text-2xl font-bold text-green-600", "38" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Trial" }
                    p { class: "text-2xl font-bold text-blue-600", "4" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "MRR" }
                    p { class: "text-2xl font-bold text-green-600", "$12,450" }
                }
            }

            if source == TenantSource::Demo && !is_loading {
                div {
                    class: "mb-3 text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                    "Backend tenants API not reachable - showing demo rows."
                }
            }

            DataTable {
                loading: is_loading,
                total_items: if source == TenantSource::Backend { remote_tenants.len() } else { 4 },
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
                    } else if source == TenantSource::Backend && remote_tenants.is_empty() {
                        TableEmpty { columns: 6, message: "No tenants yet.".to_string() }
                    } else {
                        TableBody {
                            if source == TenantSource::Backend {
                                for tenant in remote_tenants.iter().cloned() {
                                    TenantRow {
                                        key: "{tenant.id}",
                                        name: tenant.name.clone(),
                                        domain: tenant.slug.clone(),
                                        plan: humanize_plan(&tenant.subscription_plan),
                                        users: 0,
                                        mrr: "-".to_string(),
                                        status: humanize_status(&tenant.status),
                                        created: format_created(tenant.created_at),
                                    }
                                }
                            } else {
                                TenantRow {
                                    name: "Acme MSP",
                                    domain: "acme-msp",
                                    plan: "Professional",
                                    users: 8,
                                    mrr: "$299",
                                    status: "Active",
                                    created: "Jan 15, 2024",
                                }
                                TenantRow {
                                    name: "TechPro Services",
                                    domain: "techpro",
                                    plan: "Enterprise",
                                    users: 25,
                                    mrr: "$599",
                                    status: "Active",
                                    created: "Mar 1, 2024",
                                }
                                TenantRow {
                                    name: "IT Solutions Co",
                                    domain: "itsolutions",
                                    plan: "Professional",
                                    users: 5,
                                    mrr: "$299",
                                    status: "Active",
                                    created: "Jun 15, 2024",
                                }
                                TenantRow {
                                    name: "New MSP Trial",
                                    domain: "newmsp-trial",
                                    plan: "Trial",
                                    users: 2,
                                    mrr: "$0",
                                    status: "Trial",
                                    created: "Jan 10, 2025",
                                }
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
                    span { class: "font-medium text-gray-900 dark:text-white", "{props.name}" }
                    p { class: "text-sm text-gray-500", "{props.domain}.mokosh.app" }
                }
            }
            TableCell { Badge { variant: plan_variant, "{props.plan}" } }
            TableCell { "{props.users}" }
            TableCell { class: "font-medium", "{props.mrr}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-gray-500", "{props.created}" }
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

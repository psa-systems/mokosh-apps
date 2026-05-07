//! Admin pages (multi-tenant only)

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};

/// Tenant management page (multi-tenant mode only)
#[cfg(feature = "multi-tenant")]
#[component]
pub fn TenantManagementPage() -> Element {
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

            DataTable {
                total_items: 42,
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
                    TableBody {
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

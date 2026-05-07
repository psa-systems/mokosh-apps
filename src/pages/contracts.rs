//! Contract pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, DocumentIcon, IconSize,
    PageHeader, PlusIcon, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// Contract list page
#[component]
pub fn ContractListPage() -> Element {
    rsx! {
        AppLayout { title: "Contracts",
            PageHeader {
                title: "Contracts",
                subtitle: "Manage customer contracts and agreements",
                actions: rsx! {
                    Link {
                        to: Route::ContractNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Contract"
                        }
                    }
                },
            }

            DataTable {
                total_items: 15,
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Contract" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { "Type" }
                            TableHeader { sortable: true, "Value" }
                            TableHeader { "Expires" }
                            TableHeader { "Status" }
                        }
                    }
                    TableBody {
                        ContractRow {
                            id: "1",
                            name: "Managed Services Agreement",
                            company: "Acme Corp",
                            contract_type: "Recurring",
                            value: "$2,500/mo",
                            expires: "Dec 31, 2025",
                            status: "Active",
                        }
                        ContractRow {
                            id: "2",
                            name: "Block Hours - 40 Hours",
                            company: "TechStart Inc",
                            contract_type: "Prepaid",
                            value: "$6,000",
                            expires: "Mar 31, 2025",
                            status: "Active",
                        }
                        ContractRow {
                            id: "3",
                            name: "Time & Materials",
                            company: "Global Widgets",
                            contract_type: "T&M",
                            value: "As-needed",
                            expires: "Ongoing",
                            status: "Active",
                        }
                        ContractRow {
                            id: "4",
                            name: "Network Upgrade Project",
                            company: "Acme Corp",
                            contract_type: "Fixed Price",
                            value: "$45,000",
                            expires: "Feb 28, 2025",
                            status: "Active",
                        }
                        ContractRow {
                            id: "5",
                            name: "Previous Agreement",
                            company: "TechStart Inc",
                            contract_type: "Recurring",
                            value: "$1,500/mo",
                            expires: "Dec 31, 2024",
                            status: "Expired",
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContractRowProps {
    id: String,
    name: String,
    company: String,
    contract_type: String,
    value: String,
    expires: String,
    status: String,
}

#[component]
fn ContractRow(props: ContractRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Active" => BadgeVariant::Green,
        "Expired" => BadgeVariant::Red,
        "Pending" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    };

    let type_variant = match props.contract_type.as_str() {
        "Recurring" => BadgeVariant::Blue,
        "Prepaid" => BadgeVariant::Purple,
        "Fixed Price" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();

    let id = props.id.clone();


    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::ContractDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::ContractDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.name}"
                }
            }
            TableCell { "{props.company}" }
            TableCell { Badge { variant: type_variant, "{props.contract_type}" } }
            TableCell { class: "font-medium", "{props.value}" }
            TableCell { "{props.expires}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
        }
    }
}

/// New contract page
#[component]
pub fn ContractNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Contract",
            PageHeader {
                title: "New Contract",
                subtitle: "Create a new contract",
            }

            Card {
                form { class: "space-y-6",
                    p { class: "text-gray-500", "Contract creation form would go here." }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::ContractList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            "Create Contract"
                        }
                    }
                }
            }
        }
    }
}

/// Contract detail page
#[derive(Props, Clone, PartialEq)]
pub struct ContractDetailPageProps {
    pub id: String,
}

#[component]
pub fn ContractDetailPage(props: ContractDetailPageProps) -> Element {
    rsx! {
        AppLayout { title: "Contract Detail",
            PageHeader {
                title: "Managed Services Agreement",
                subtitle: "Acme Corp",
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Edit" }
                    Button { variant: ButtonVariant::Primary, "Renew" }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Contract details
                    Card { title: "Contract Details",
                        dl { class: "grid grid-cols-2 gap-4",
                            div {
                                dt { class: "text-sm text-gray-500", "Contract Type" }
                                dd { class: "mt-1 font-medium", "Monthly Recurring" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Billing Cycle" }
                                dd { class: "mt-1", "Monthly" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Start Date" }
                                dd { class: "mt-1", "Jan 1, 2024" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "End Date" }
                                dd { class: "mt-1", "Dec 31, 2025" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Auto-Renewal" }
                                dd { class: "mt-1", "Yes (1 year)" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Notice Period" }
                                dd { class: "mt-1", "60 days" }
                            }
                        }
                    }

                    // Covered services
                    Card { title: "Covered Services",
                        ul { class: "list-disc list-inside space-y-2 text-gray-700 dark:text-gray-300",
                            li { "24/7 Remote monitoring and alerting" }
                            li { "Unlimited remote support tickets" }
                            li { "Monthly onsite visit (4 hours)" }
                            li { "Patch management and updates" }
                            li { "Antivirus and security monitoring" }
                            li { "Backup monitoring" }
                        }
                    }

                    // Billing history
                    Card { title: "Recent Invoices", padding: false,
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Invoice" }
                                    TableHeader { "Date" }
                                    TableHeader { "Amount" }
                                    TableHeader { "Status" }
                                }
                            }
                            TableBody {
                                TableRow {
                                    TableCell { class: "text-blue-600", "INV-2025-001" }
                                    TableCell { "Jan 1, 2025" }
                                    TableCell { class: "font-medium", "$2,500.00" }
                                    TableCell { Badge { variant: BadgeVariant::Yellow, "Pending" } }
                                }
                                TableRow {
                                    TableCell { class: "text-blue-600", "INV-2024-012" }
                                    TableCell { "Dec 1, 2024" }
                                    TableCell { class: "font-medium", "$2,500.00" }
                                    TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                                }
                                TableRow {
                                    TableCell { class: "text-blue-600", "INV-2024-011" }
                                    TableCell { "Nov 1, 2024" }
                                    TableCell { class: "font-medium", "$2,500.00" }
                                    TableCell { Badge { variant: BadgeVariant::Green, "Paid" } }
                                }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    Card { title: "Summary",
                        dl { class: "space-y-4",
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Status" }
                                dd { Badge { variant: BadgeVariant::Green, "Active" } }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Monthly Value" }
                                dd { class: "font-medium text-lg", "$2,500" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Annual Value" }
                                dd { class: "font-medium", "$30,000" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Days Remaining" }
                                dd { class: "font-medium", "351" }
                            }
                        }
                    }

                    Card { title: "Usage This Month",
                        div { class: "space-y-3",
                            div {
                                div { class: "flex justify-between text-sm mb-1",
                                    span { class: "text-gray-500", "Tickets" }
                                    span { class: "font-medium", "12 / Unlimited" }
                                }
                            }
                            div {
                                div { class: "flex justify-between text-sm mb-1",
                                    span { class: "text-gray-500", "Onsite Hours" }
                                    span { class: "font-medium", "2 / 4" }
                                }
                                div { class: "w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2",
                                    div { class: "bg-blue-600 h-2 rounded-full", style: "width: 50%" }
                                }
                            }
                        }
                    }

                    Card { title: "Documents",
                        div { class: "space-y-2",
                            a { class: "flex items-center text-sm text-blue-600 hover:text-blue-500",
                                DocumentIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "Contract PDF"
                            }
                            a { class: "flex items-center text-sm text-blue-600 hover:text-blue-500",
                                DocumentIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "SLA Agreement"
                            }
                        }
                    }
                }
            }
        }
    }
}

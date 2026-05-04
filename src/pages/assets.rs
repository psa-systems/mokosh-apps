//! Asset management pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, ExclamationIcon,
    IconSize, PageHeader, PlusIcon, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// Asset list page
#[component]
pub fn AssetListPage() -> Element {
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

            DataTable {
                total_items: 50,
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Asset" }
                            TableHeader { sortable: true, "Type" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { "Serial/ID" }
                            TableHeader { "Status" }
                            TableHeader { "Last Seen" }
                        }
                    }
                    TableBody {
                        AssetRow {
                            id: "1",
                            name: "Exchange Server 01",
                            asset_type: "Server",
                            company: "Acme Corp",
                            serial: "SRV-2024-001",
                            status: "Online",
                            last_seen: "Just now",
                        }
                        AssetRow {
                            id: "2",
                            name: "Core Switch",
                            asset_type: "Network",
                            company: "Acme Corp",
                            serial: "NET-SW-001",
                            status: "Online",
                            last_seen: "Just now",
                        }
                        AssetRow {
                            id: "3",
                            name: "Bob's Laptop",
                            asset_type: "Workstation",
                            company: "Acme Corp",
                            serial: "WKS-2024-015",
                            status: "Online",
                            last_seen: "5 min ago",
                        }
                        AssetRow {
                            id: "4",
                            name: "Office Firewall",
                            asset_type: "Network",
                            company: "TechStart Inc",
                            serial: "FW-TS-001",
                            status: "Warning",
                            last_seen: "2 hours ago",
                        }
                        AssetRow {
                            id: "5",
                            name: "Backup NAS",
                            asset_type: "Storage",
                            company: "Global Widgets",
                            serial: "NAS-GW-001",
                            status: "Offline",
                            last_seen: "1 day ago",
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AssetRowProps {
    id: String,
    name: String,
    asset_type: String,
    company: String,
    serial: String,
    status: String,
    last_seen: String,
}

#[component]
fn AssetRow(props: AssetRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Online" => BadgeVariant::Green,
        "Warning" => BadgeVariant::Yellow,
        "Offline" => BadgeVariant::Red,
        _ => BadgeVariant::Gray,
    };

    let type_variant = match props.asset_type.as_str() {
        "Server" => BadgeVariant::Purple,
        "Network" => BadgeVariant::Blue,
        "Workstation" => BadgeVariant::Gray,
        "Storage" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    rsx! {
        TableRow { clickable: true,
            TableCell {
                Link {
                    to: Route::AssetDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.name}"
                }
            }
            TableCell { Badge { variant: type_variant, "{props.asset_type}" } }
            TableCell { "{props.company}" }
            TableCell { class: "font-mono text-sm", "{props.serial}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-gray-500", "{props.last_seen}" }
        }
    }
}

/// New asset page
#[component]
pub fn AssetNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Asset",
            PageHeader {
                title: "New Asset",
                subtitle: "Add a new configuration item",
            }

            Card {
                form { class: "space-y-6",
                    p { class: "text-gray-500", "Asset creation form would go here." }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::AssetList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
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
    rsx! {
        AppLayout { title: "Asset Detail",
            PageHeader {
                title: "Exchange Server 01",
                subtitle: "Acme Corp",
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Edit" }
                    Button { variant: ButtonVariant::Primary, "Remote Connect" }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // System info
                    Card { title: "System Information",
                        dl { class: "grid grid-cols-2 gap-4",
                            div {
                                dt { class: "text-sm text-gray-500", "Hostname" }
                                dd { class: "mt-1 font-mono", "ACME-EXCH01" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "IP Address" }
                                dd { class: "mt-1 font-mono", "192.168.1.10" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Operating System" }
                                dd { class: "mt-1", "Windows Server 2022" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "CPU" }
                                dd { class: "mt-1", "Intel Xeon E-2288G (8 cores)" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Memory" }
                                dd { class: "mt-1", "64 GB" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Storage" }
                                dd { class: "mt-1", "2 TB SSD RAID 1" }
                            }
                        }
                    }

                    // Recent alerts
                    Card { title: "Recent Alerts",
                        div { class: "space-y-3",
                            AlertItem {
                                severity: "warning",
                                message: "High memory usage (85%)",
                                time: "2 hours ago",
                            }
                            AlertItem {
                                severity: "info",
                                message: "Windows Update pending restart",
                                time: "1 day ago",
                            }
                            AlertItem {
                                severity: "critical",
                                message: "Service restart required",
                                time: "2 days ago (resolved)",
                            }
                        }
                    }

                    // Related tickets
                    Card { title: "Related Tickets", padding: false,
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Ticket" }
                                    TableHeader { "Status" }
                                    TableHeader { "Date" }
                                }
                            }
                            TableBody {
                                TableRow { clickable: true,
                                    TableCell {
                                        div {
                                            span { class: "font-medium text-blue-600", "TKT-1234" }
                                            p { class: "text-sm text-gray-500", "Email server not responding" }
                                        }
                                    }
                                    TableCell { Badge { variant: BadgeVariant::Blue, "Open" } }
                                    TableCell { class: "text-gray-500", "Today" }
                                }
                                TableRow { clickable: true,
                                    TableCell {
                                        div {
                                            span { class: "font-medium text-blue-600", "TKT-1150" }
                                            p { class: "text-sm text-gray-500", "Exchange update installation" }
                                        }
                                    }
                                    TableCell { Badge { variant: BadgeVariant::Green, "Resolved" } }
                                    TableCell { class: "text-gray-500", "Dec 15, 2024" }
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
                                Badge { variant: BadgeVariant::Green, "Online" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-gray-500", "Last Check-in" }
                                span { class: "font-medium", "Just now" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-gray-500", "Uptime" }
                                span { class: "font-medium", "45 days" }
                            }
                        }
                    }

                    Card { title: "Details",
                        dl { class: "space-y-4",
                            div {
                                dt { class: "text-sm text-gray-500", "Type" }
                                dd { class: "mt-1", Badge { variant: BadgeVariant::Purple, "Server" } }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Serial Number" }
                                dd { class: "mt-1 font-mono text-sm", "SRV-2024-001" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Location" }
                                dd { class: "mt-1", "Server Room - Rack 3" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Warranty" }
                                dd { class: "mt-1", "Dec 31, 2026" }
                            }
                        }
                    }

                    Card { title: "RMM Integration",
                        div { class: "space-y-3",
                            div { class: "flex justify-between items-center",
                                span { class: "text-sm text-gray-500", "Tactical RMM" }
                                Badge { variant: BadgeVariant::Green, "Connected" }
                            }
                            a { href: "#", class: "block text-sm text-blue-600 hover:text-blue-500",
                                "Open in Tactical RMM"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AlertItemProps {
    severity: String,
    message: String,
    time: String,
}

#[component]
fn AlertItem(props: AlertItemProps) -> Element {
    let (bg_class, icon_class) = match props.severity.as_str() {
        "critical" => (
            "bg-red-50 dark:bg-red-900/20 border-l-red-500",
            "text-red-500",
        ),
        "warning" => (
            "bg-yellow-50 dark:bg-yellow-900/20 border-l-yellow-500",
            "text-yellow-500",
        ),
        _ => (
            "bg-blue-50 dark:bg-blue-900/20 border-l-blue-500",
            "text-blue-500",
        ),
    };

    rsx! {
        div { class: "border-l-4 {bg_class} p-3 rounded-r",
            div { class: "flex items-start",
                ExclamationIcon { size: IconSize::Small, class: "{icon_class} mr-2 mt-0.5".to_string() }
                div { class: "flex-1",
                    p { class: "text-sm font-medium text-gray-900 dark:text-white", "{props.message}" }
                    p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1", "{props.time}" }
                }
            }
        }
    }
}

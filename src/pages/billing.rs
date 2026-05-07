//! Billing pages (invoices and payments)

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

/// Invoice list page
#[component]
pub fn InvoiceListPage() -> Element {
    rsx! {
        AppLayout { title: "Invoices",
            PageHeader {
                title: "Invoices",
                subtitle: "Manage customer invoices and billing",
                actions: rsx! {
                    Link {
                        to: Route::InvoiceNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Invoice"
                        }
                    }
                },
            }

            // Stats
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Outstanding" }
                    p { class: "text-2xl font-bold text-yellow-600", "$12,450" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Overdue" }
                    p { class: "text-2xl font-bold text-red-600", "$3,200" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Paid (This Month)" }
                    p { class: "text-2xl font-bold text-green-600", "$28,500" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Draft" }
                    p { class: "text-2xl font-bold text-gray-600", "$5,600" }
                }
            }

            DataTable {
                total_items: 25,
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Invoice" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { sortable: true, "Date" }
                            TableHeader { sortable: true, "Due Date" }
                            TableHeader { sortable: true, "Amount" }
                            TableHeader { "Status" }
                        }
                    }
                    TableBody {
                        InvoiceRow {
                            id: "1",
                            number: "INV-2025-001",
                            company: "Acme Corp",
                            date: "Jan 1, 2025",
                            due_date: "Jan 31, 2025",
                            amount: "$2,500.00",
                            status: "Pending",
                        }
                        InvoiceRow {
                            id: "2",
                            number: "INV-2025-002",
                            company: "TechStart Inc",
                            date: "Jan 5, 2025",
                            due_date: "Feb 4, 2025",
                            amount: "$1,850.00",
                            status: "Sent",
                        }
                        InvoiceRow {
                            id: "3",
                            number: "INV-2024-098",
                            company: "Global Widgets",
                            date: "Dec 15, 2024",
                            due_date: "Jan 14, 2025",
                            amount: "$3,200.00",
                            status: "Overdue",
                        }
                        InvoiceRow {
                            id: "4",
                            number: "INV-2024-097",
                            company: "Acme Corp",
                            date: "Dec 1, 2024",
                            due_date: "Dec 31, 2024",
                            amount: "$2,500.00",
                            status: "Paid",
                        }
                        InvoiceRow {
                            id: "5",
                            number: "INV-2025-003",
                            company: "New Client",
                            date: "",
                            due_date: "",
                            amount: "$5,600.00",
                            status: "Draft",
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct InvoiceRowProps {
    id: String,
    number: String,
    company: String,
    date: String,
    due_date: String,
    amount: String,
    status: String,
}

#[component]
fn InvoiceRow(props: InvoiceRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Paid" => BadgeVariant::Green,
        "Sent" | "Pending" => BadgeVariant::Blue,
        "Overdue" => BadgeVariant::Red,
        "Draft" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();

    let id = props.id.clone();


    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::InvoiceDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::InvoiceDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.number}"
                }
            }
            TableCell { "{props.company}" }
            TableCell {
                if props.date.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.date}"
                }
            }
            TableCell {
                if props.due_date.is_empty() {
                    span { class: "text-gray-400", "-" }
                } else {
                    "{props.due_date}"
                }
            }
            TableCell { class: "font-medium", "{props.amount}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
        }
    }
}

/// New invoice page
#[component]
pub fn InvoiceNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Invoice",
            PageHeader {
                title: "New Invoice",
                subtitle: "Create a new invoice",
            }

            Card {
                form { class: "space-y-6",
                    p { class: "text-gray-500", "Invoice creation form would go here." }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::InvoiceList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button { variant: ButtonVariant::Secondary, "Save as Draft" }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            "Create & Send"
                        }
                    }
                }
            }
        }
    }
}

/// Invoice detail page
#[derive(Props, Clone, PartialEq)]
pub struct InvoiceDetailPageProps {
    pub id: String,
}

#[component]
pub fn InvoiceDetailPage(props: InvoiceDetailPageProps) -> Element {
    rsx! {
        AppLayout { title: "Invoice Detail",
            PageHeader {
                title: "Invoice INV-2025-001",
                // Audit P1-07: Download PDF / Send / Record Payment buttons
                // were decorative (no onclick, no backing endpoint). Hidden
                // until the billing module ships the corresponding actions.
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Invoice preview
                div { class: "lg:col-span-2",
                    Card {
                        // Invoice header
                        div { class: "flex justify-between mb-8",
                            div {
                                h2 { class: "text-2xl font-bold text-gray-900 dark:text-white", "INVOICE" }
                                p { class: "text-gray-500", "INV-2025-001" }
                            }
                            div { class: "text-right",
                                p { class: "font-bold text-lg", "Mokosh Platform" }
                                p { class: "text-sm text-gray-500", "123 Business Ave" }
                                p { class: "text-sm text-gray-500", "New York, NY 10001" }
                            }
                        }

                        // Bill to
                        div { class: "grid grid-cols-2 gap-8 mb-8",
                            div {
                                h3 { class: "text-sm font-medium text-gray-500 mb-2", "BILL TO" }
                                p { class: "font-medium", "Acme Corp" }
                                p { class: "text-sm text-gray-600", "Bob Johnson" }
                                p { class: "text-sm text-gray-600", "456 Customer St" }
                                p { class: "text-sm text-gray-600", "New York, NY 10002" }
                            }
                            div { class: "text-right",
                                div { class: "mb-2",
                                    span { class: "text-sm text-gray-500", "Invoice Date: " }
                                    span { class: "font-medium", "January 1, 2025" }
                                }
                                div { class: "mb-2",
                                    span { class: "text-sm text-gray-500", "Due Date: " }
                                    span { class: "font-medium", "January 31, 2025" }
                                }
                                div {
                                    span { class: "text-sm text-gray-500", "Terms: " }
                                    span { class: "font-medium", "Net 30" }
                                }
                            }
                        }

                        // Line items
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Description" }
                                    TableHeader { class: "text-right", "Qty" }
                                    TableHeader { class: "text-right", "Rate" }
                                    TableHeader { class: "text-right", "Amount" }
                                }
                            }
                            TableBody {
                                TableRow {
                                    TableCell { "Managed Services - January 2025" }
                                    TableCell { class: "text-right", "1" }
                                    TableCell { class: "text-right", "$2,500.00" }
                                    TableCell { class: "text-right font-medium", "$2,500.00" }
                                }
                            }
                        }

                        // Totals
                        div { class: "mt-8 border-t border-gray-200 dark:border-gray-700 pt-4",
                            div { class: "flex justify-end",
                                div { class: "w-64 space-y-2",
                                    div { class: "flex justify-between",
                                        span { class: "text-gray-500", "Subtotal" }
                                        span { "$2,500.00" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-gray-500", "Tax (0%)" }
                                        span { "$0.00" }
                                    }
                                    div { class: "flex justify-between text-lg font-bold pt-2 border-t border-gray-200 dark:border-gray-700",
                                        span { "Total" }
                                        span { "$2,500.00" }
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
                                Badge { variant: BadgeVariant::Blue, "Pending" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-gray-500", "Amount Due" }
                                span { class: "text-lg font-bold", "$2,500.00" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-gray-500", "Days Until Due" }
                                span { class: "font-medium", "16 days" }
                            }
                        }
                    }

                    Card { title: "Activity",
                        div { class: "space-y-3 text-sm",
                            div { class: "flex justify-between",
                                span { class: "text-gray-600", "Invoice created" }
                                span { class: "text-gray-400", "Jan 1" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-gray-600", "Sent to customer" }
                                span { class: "text-gray-400", "Jan 2" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Payment list page
#[component]
pub fn PaymentListPage() -> Element {
    rsx! {
        AppLayout { title: "Payments",
            PageHeader {
                title: "Payments",
                subtitle: "Track customer payments",
            }

            DataTable {
                total_items: 20,
                current_page: 1,
                per_page: 25,
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Date" }
                            TableHeader { "Company" }
                            TableHeader { "Invoice" }
                            TableHeader { "Method" }
                            TableHeader { sortable: true, "Amount" }
                        }
                    }
                    TableBody {
                        TableRow {
                            TableCell { "Jan 10, 2025" }
                            TableCell { "Acme Corp" }
                            TableCell { class: "text-blue-600", "INV-2024-097" }
                            TableCell { "Credit Card" }
                            TableCell { class: "font-medium text-green-600", "$2,500.00" }
                        }
                        TableRow {
                            TableCell { "Jan 8, 2025" }
                            TableCell { "TechStart Inc" }
                            TableCell { class: "text-blue-600", "INV-2024-095" }
                            TableCell { "ACH Transfer" }
                            TableCell { class: "font-medium text-green-600", "$6,000.00" }
                        }
                        TableRow {
                            TableCell { "Jan 5, 2025" }
                            TableCell { "Global Widgets" }
                            TableCell { class: "text-blue-600", "INV-2024-090" }
                            TableCell { "Check" }
                            TableCell { class: "font-medium text-green-600", "$4,250.00" }
                        }
                    }
                }
            }
        }
    }
}

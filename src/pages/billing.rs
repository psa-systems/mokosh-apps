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
    let mut company = use_signal(String::new);
    let mut contract = use_signal(String::new);
    let mut issue_date = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut line_description = use_signal(String::new);
    let mut line_quantity = use_signal(|| "1".to_string());
    let mut line_unit_price = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);

    let company_options = vec![
        crate::components::SelectOption::new("1", "Acme Corp"),
        crate::components::SelectOption::new("2", "TechStart Inc"),
        crate::components::SelectOption::new("3", "Global Widgets"),
    ];
    let contract_options = vec![
        crate::components::SelectOption::new("", "None"),
        crate::components::SelectOption::new("1", "Managed Services Agreement"),
        crate::components::SelectOption::new("2", "Block Hours - 40 Hours"),
    ];

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        spawn(async move {
            // Server billing module is still 501; stub the submit and
            // navigate to the list. POST goes live with the billing
            // module landing.
            #[cfg(feature = "web")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(1000).await;
            }
            is_submitting.set(false);
            navigator.push(Route::InvoiceList {});
        });
    };

    rsx! {
        AppLayout { title: "New Invoice",
            PageHeader {
                title: "New Invoice",
                subtitle: "Create a new invoice",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: handle_submit,

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Select {
                            name: "company",
                            label: "Bill To",
                            options: company_options,
                            value: company.read().clone(),
                            placeholder: "Select a company",
                            required: true,
                            onchange: move |e: FormEvent| company.set(e.value()),
                        }
                        crate::components::Select {
                            name: "contract",
                            label: "Contract",
                            options: contract_options,
                            value: contract.read().clone(),
                            onchange: move |e: FormEvent| contract.set(e.value()),
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Input {
                            name: "issue_date",
                            label: "Issue Date",
                            r#type: "date",
                            required: true,
                            value: issue_date.read().clone(),
                            oninput: move |e: FormEvent| issue_date.set(e.value()),
                        }
                        crate::components::Input {
                            name: "due_date",
                            label: "Due Date",
                            r#type: "date",
                            required: true,
                            value: due_date.read().clone(),
                            oninput: move |e: FormEvent| due_date.set(e.value()),
                        }
                    }

                    div {
                        h3 { class: "text-sm font-medium text-gray-700 dark:text-gray-300 mb-3", "Line Items" }
                        div { class: "grid grid-cols-1 gap-3 sm:grid-cols-[1fr_100px_140px]",
                            crate::components::Input {
                                name: "line_description",
                                placeholder: "Description",
                                value: line_description.read().clone(),
                                oninput: move |e: FormEvent| line_description.set(e.value()),
                            }
                            crate::components::Input {
                                name: "line_quantity",
                                r#type: "number",
                                placeholder: "Qty",
                                value: line_quantity.read().clone(),
                                oninput: move |e: FormEvent| line_quantity.set(e.value()),
                            }
                            crate::components::Input {
                                name: "line_unit_price",
                                r#type: "number",
                                placeholder: "Unit price",
                                value: line_unit_price.read().clone(),
                                oninput: move |e: FormEvent| line_unit_price.set(e.value()),
                            }
                        }
                        p { class: "mt-2 text-xs text-gray-500",
                            "Multi-line invoices land with the billing module."
                        }
                    }

                    crate::components::Textarea {
                        name: "notes",
                        label: "Notes",
                        placeholder: "Internal notes (not shown to the customer)",
                        rows: 3,
                        value: notes.read().clone(),
                        oninput: move |e: FormEvent| notes.set(e.value()),
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::InvoiceList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
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
#[allow(unused_variables)]
pub fn InvoiceDetailPage(props: InvoiceDetailPageProps) -> Element {
    let header_title = format!("Invoice {}", props.id);
    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
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

#[derive(Props, Clone, PartialEq)]
struct PaymentRowProps {
    date: String,
    company: String,
    invoice_id: String,
    invoice_label: String,
    method: String,
    amount: String,
}

#[component]
fn PaymentRow(props: PaymentRowProps) -> Element {
    rsx! {
        TableRow {
            TableCell { "{props.date}" }
            TableCell { "{props.company}" }
            TableCell {
                Link {
                    to: Route::InvoiceDetail { id: props.invoice_id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.invoice_label}"
                }
            }
            TableCell { "{props.method}" }
            TableCell { class: "font-medium text-green-600", "{props.amount}" }
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
                        PaymentRow {
                            date: "Jan 10, 2025",
                            company: "Acme Corp",
                            invoice_id: "97",
                            invoice_label: "INV-2024-097",
                            method: "Credit Card",
                            amount: "$2,500.00",
                        }
                        PaymentRow {
                            date: "Jan 8, 2025",
                            company: "TechStart Inc",
                            invoice_id: "95",
                            invoice_label: "INV-2024-095",
                            method: "ACH Transfer",
                            amount: "$6,000.00",
                        }
                        PaymentRow {
                            date: "Jan 5, 2025",
                            company: "Global Widgets",
                            invoice_id: "90",
                            invoice_label: "INV-2024-090",
                            method: "Check",
                            amount: "$4,250.00",
                        }
                    }
                }
            }
        }
    }
}

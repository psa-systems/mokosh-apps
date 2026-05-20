//! Contact and company pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, SearchInput, Select, SelectOption, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
};
use crate::Route;

/// Map the server's lowercased `CompanyType` enum tag (`"customer"`,
/// `"prospect"`, `"vendor"`, `"partner"`) to the title-case label that
/// `CompanyRow` keys its badge variant on. Unknown values fall through
/// unchanged so future variants don't disappear.
fn humanize_company_type(raw: &str) -> String {
    match raw {
        "customer" => "Customer".to_string(),
        "prospect" => "Prospect".to_string(),
        "vendor" => "Vendor".to_string(),
        "partner" => "Partner".to_string(),
        other => other.to_string(),
    }
}

/// Subset of mokosh-server's `CompanyResponse` we render in the list. The
/// server returns more fields; serde silently drops the ones we don't ask
/// for, so adding columns later just means extending this struct.
#[derive(Clone, Debug, Deserialize)]
struct RemoteCompany {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    company_type: String,
    #[serde(default)]
    account_manager_name: Option<String>,
    #[serde(default)]
    open_ticket_count: Option<i64>,
}

/// Server-side paginated envelope (`PaginatedResponse<CompanyResponse>`).
#[derive(Clone, Debug, Deserialize)]
struct PaginatedCompanies {
    data: Vec<RemoteCompany>,
}

/// Subset of mokosh-server's `ContactResponse` we render in the contacts
/// list. As with companies, serde drops unknown fields so this can grow
/// without breaking decoding.
#[derive(Clone, Debug, Deserialize)]
struct RemoteContact {
    id: uuid::Uuid,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone_primary: Option<String>,
    #[serde(default)]
    job_title: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedContacts {
    data: Vec<RemoteContact>,
}

/// Source of the rows currently on screen. Same progressive-enablement
/// pattern as the calendar page.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CompanySource {
    Backend,
    Demo,
}

/// Mirror of CompanySource for the contacts list; kept separate to
/// avoid coupling the two pages' fallbacks.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ContactSource {
    Backend,
    Demo,
}

/// Company list page
#[component]
pub fn CompanyListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut type_filter = use_signal(String::new);

    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("customer", "Customer"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
    ];

    // Try to pull live companies from mokosh-server. If we don't have
    // a token yet, or the API errors, or the route isn't deployed yet,
    // fall back to the seeded demo rows below so the page stays demoable.
    let companies_resource = use_resource(|| async {
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteCompany>::new(), CompanySource::Demo),
        };
        match crate::hooks::fetch::api::get_with_auth::<PaginatedCompanies>(
            "/companies",
            &token,
        )
        .await
        {
            Ok(page) => (page.data, CompanySource::Backend),
            Err(_) => (Vec::new(), CompanySource::Demo),
        }
    });

    // `None` here means the use_resource future is still in-flight on
    // the first paint. We use that signal to render TableLoading
    // skeletons rather than briefly flashing the demo rows.
    let resource_snapshot = companies_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_companies, source) = match &*resource_snapshot {
        Some((rows, source)) => (rows.clone(), *source),
        None => (Vec::new(), CompanySource::Demo),
    };

    rsx! {
        AppLayout { title: "Companies",
            PageHeader {
                title: "Companies",
                subtitle: "Manage customer and vendor accounts",
                actions: rsx! {
                    Link {
                        to: Route::CompanyNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Company"
                        }
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        SearchInput {
                            value: search.read().clone(),
                            placeholder: "Search companies...",
                            oninput: move |e: FormEvent| search.set(e.value()),
                        }
                    }
                    Select {
                        name: "type",
                        options: type_options,
                        value: type_filter.read().clone(),
                        onchange: move |e: FormEvent| type_filter.set(e.value()),
                    }
                }
            }

            if source == CompanySource::Demo {
                div {
                    class: "mb-3 text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                    "Backend companies API not reachable - showing demo rows. Create/edit operations are disabled until the API is live."
                }
            }

            // Companies table
            DataTable {
                loading: is_loading,
                total_items: if source == CompanySource::Backend { remote_companies.len() } else { 5 },
                current_page: 1,
                per_page: 25,
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Company" }
                            TableHeader { sortable: true, "Type" }
                            TableHeader { "Primary Contact" }
                            TableHeader { "Open Tickets" }
                            TableHeader { "Contract" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 5, rows: 5 }
                    } else if source == CompanySource::Backend && remote_companies.is_empty() {
                        TableEmpty { columns: 5, message: "No companies yet.".to_string() }
                    } else {
                        TableBody {
                            if source == CompanySource::Backend {
                                for company in remote_companies.iter().cloned() {
                                    CompanyRow {
                                        key: "{company.id}",
                                        id: company.id.to_string(),
                                        name: company.name,
                                        company_type: humanize_company_type(&company.company_type),
                                        primary_contact: company.account_manager_name.unwrap_or_default(),
                                        open_tickets: company.open_ticket_count.unwrap_or(0).max(0) as u32,
                                        contract: String::new(),
                                    }
                                }
                            } else {
                                CompanyRow {
                                id: "1",
                                name: "Acme Corp",
                                company_type: "Customer",
                                primary_contact: "Bob Johnson",
                                open_tickets: 5,
                                contract: "Managed Services",
                            }
                            CompanyRow {
                                id: "2",
                                name: "TechStart Inc",
                                company_type: "Customer",
                                primary_contact: "Alice Williams",
                                open_tickets: 2,
                                contract: "Block Hours",
                            }
                            CompanyRow {
                                id: "3",
                                name: "Global Widgets",
                                company_type: "Customer",
                                primary_contact: "Charlie Brown",
                                open_tickets: 8,
                                contract: "Time & Materials",
                            }
                            CompanyRow {
                                id: "4",
                                name: "New Venture LLC",
                                company_type: "Prospect",
                                primary_contact: "Diana Ross",
                                open_tickets: 0,
                                contract: "None",
                            }
                            CompanyRow {
                                id: "5",
                                name: "Dell Technologies",
                                company_type: "Vendor",
                                primary_contact: "Sales Rep",
                                open_tickets: 0,
                                contract: "Partner",
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CompanyRowProps {
    id: String,
    name: String,
    company_type: String,
    primary_contact: String,
    open_tickets: u32,
    contract: String,
}

#[component]
fn CompanyRow(props: CompanyRowProps) -> Element {
    let type_variant = match props.company_type.as_str() {
        "Customer" => BadgeVariant::Green,
        "Prospect" => BadgeVariant::Blue,
        "Vendor" => BadgeVariant::Purple,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::CompanyDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::CompanyDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.name}"
                }
            }
            TableCell {
                Badge { variant: type_variant, "{props.company_type}" }
            }
            TableCell { "{props.primary_contact}" }
            TableCell {
                if props.open_tickets > 0 {
                    span { class: "font-medium text-blue-600", "{props.open_tickets}" }
                } else {
                    span { class: "text-gray-400", "0" }
                }
            }
            TableCell { "{props.contract}" }
        }
    }
}

/// New company page
#[component]
pub fn CompanyNewPage() -> Element {
    let mut name = use_signal(String::new);
    let mut company_type = use_signal(|| "customer".to_string());
    let mut is_submitting = use_signal(|| false);

    let type_options = vec![
        SelectOption::new("customer", "Customer"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
    ];

    rsx! {
        AppLayout { title: "New Company",
            PageHeader {
                title: "New Company",
                subtitle: "Add a new company account",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        is_submitting.set(true);
                        // P1-04: mock submit until create_company endpoint
                        // is wired through the client.
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use gloo_timers::future::TimeoutFuture;
                                TimeoutFuture::new(1000).await;
                            }
                            is_submitting.set(false);
                            dioxus::prelude::navigator().push(Route::CompanyList {});
                        });
                    },

                    crate::components::Input {
                        name: "name",
                        label: "Company Name",
                        placeholder: "Enter company name",
                        required: true,
                        value: name.read().clone(),
                        oninput: move |e: FormEvent| name.set(e.value()),
                    }

                    Select {
                        name: "type",
                        label: "Company Type",
                        options: type_options,
                        value: company_type.read().clone(),
                        onchange: move |e: FormEvent| company_type.set(e.value()),
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::CompanyList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Create Company"
                        }
                    }
                }
            }
        }
    }
}

/// Company detail page
#[derive(Props, Clone, PartialEq)]
pub struct CompanyDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn CompanyDetailPage(props: CompanyDetailPageProps) -> Element {
    rsx! {
        AppLayout { title: "Company Detail",
            PageHeader {
                title: "Acme Corp",
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Edit" }
                    Link {
                        to: Route::TicketNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Ticket"
                        }
                    }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Contacts
                    Card {
                        title: "Contacts",
                        actions: rsx! {
                            Link {
                                to: Route::ContactNew {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Add Contact"
                            }
                        },
                        padding: false,
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Name" }
                                    TableHeader { "Email" }
                                    TableHeader { "Phone" }
                                    TableHeader { "Role" }
                                }
                            }
                            TableBody {
                                TableRow {
                                    TableCell {
                                        Link {
                                            to: Route::ContactDetail { id: "1".to_string() },
                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                            "Bob Johnson"
                                        }
                                    }
                                    TableCell { "bob@acme.com" }
                                    TableCell { "(555) 123-4567" }
                                    TableCell { Badge { variant: BadgeVariant::Blue, "Primary" } }
                                }
                                TableRow {
                                    TableCell {
                                        Link {
                                            to: Route::ContactDetail { id: "2".to_string() },
                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                            "Sarah Miller"
                                        }
                                    }
                                    TableCell { "sarah@acme.com" }
                                    TableCell { "(555) 123-4568" }
                                    TableCell { "IT Manager" }
                                }
                                TableRow {
                                    TableCell {
                                        Link {
                                            to: Route::ContactDetail { id: "3".to_string() },
                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                            "Mike Davis"
                                        }
                                    }
                                    TableCell { "mike@acme.com" }
                                    TableCell { "(555) 123-4569" }
                                    TableCell { "Finance" }
                                }
                            }
                        }
                    }

                    // Recent tickets
                    Card {
                        title: "Recent Tickets",
                        actions: rsx! {
                            Link {
                                to: Route::TicketList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "View All"
                            }
                        },
                        padding: false,
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Ticket" }
                                    TableHeader { "Status" }
                                    TableHeader { "Updated" }
                                }
                            }
                            TableBody {
                                TableRow {
                                    TableCell {
                                        div {
                                            span { class: "font-medium text-blue-600", "TKT-1234" }
                                            p { class: "text-sm text-gray-500", "Email server not responding" }
                                        }
                                    }
                                    TableCell { Badge { variant: BadgeVariant::Blue, "Open" } }
                                    TableCell { class: "text-gray-500", "5 min ago" }
                                }
                                TableRow {
                                    TableCell {
                                        div {
                                            span { class: "font-medium text-blue-600", "TKT-1231" }
                                            p { class: "text-sm text-gray-500", "VPN connection issues" }
                                        }
                                    }
                                    TableCell { Badge { variant: BadgeVariant::Blue, "Open" } }
                                    TableCell { class: "text-gray-500", "3 hours ago" }
                                }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    // Company info
                    Card { title: "Details",
                        dl { class: "space-y-4",
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Type" }
                                dd { Badge { variant: BadgeVariant::Green, "Customer" } }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Phone" }
                                dd { class: "text-sm", "(555) 123-4500" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-sm text-gray-500", "Website" }
                                dd {
                                    a { href: "https://acme.com", class: "text-sm text-blue-600", "acme.com" }
                                }
                            }
                            div {
                                dt { class: "text-sm text-gray-500 mb-1", "Address" }
                                dd { class: "text-sm",
                                    p { "123 Business Ave" }
                                    p { "Suite 500" }
                                    p { "New York, NY 10001" }
                                }
                            }
                        }
                    }

                    // Contract info
                    Card { title: "Contract",
                        div { class: "space-y-3",
                            Link {
                                to: Route::ContractDetail { id: "1".to_string() },
                                class: "block text-blue-600 hover:text-blue-500 font-medium",
                                "Managed Services Agreement"
                            }
                            div { class: "text-sm text-gray-500",
                                p { "Monthly: $2,500" }
                                p { "Expires: Dec 31, 2025" }
                            }
                        }
                    }

                    // Stats
                    Card { title: "Statistics",
                        div { class: "space-y-3",
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Open Tickets" }
                                span { class: "font-medium text-blue-600", "5" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "This Month" }
                                span { class: "font-medium", "12 tickets" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Billable Hours" }
                                span { class: "font-medium", "45.5h" }
                            }
                            div { class: "flex justify-between",
                                span { class: "text-sm text-gray-500", "Revenue (YTD)" }
                                span { class: "font-medium text-green-600", "$42,500" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Contact list page
#[component]
pub fn ContactListPage() -> Element {
    let mut search = use_signal(String::new);

    // Try the live backend; fall back to seeded demo rows if the
    // route isn't deployed or we have no token.
    let contacts_resource = use_resource(|| async {
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteContact>::new(), ContactSource::Demo),
        };
        match crate::hooks::fetch::api::get_with_auth::<PaginatedContacts>(
            "/contacts",
            &token,
        )
        .await
        {
            Ok(page) => (page.data, ContactSource::Backend),
            Err(_) => (Vec::new(), ContactSource::Demo),
        }
    });

    let resource_snapshot = contacts_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_contacts, source) = match &*resource_snapshot {
        Some((rows, source)) => (rows.clone(), *source),
        None => (Vec::new(), ContactSource::Demo),
    };

    rsx! {
        AppLayout { title: "Contacts",
            PageHeader {
                title: "Contacts",
                subtitle: "Manage customer contacts",
                actions: rsx! {
                    Link {
                        to: Route::ContactNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Contact"
                        }
                    }
                },
            }

            // Filters
            Card { class: "mb-6",
                SearchInput {
                    value: search.read().clone(),
                    placeholder: "Search contacts...",
                    oninput: move |e: FormEvent| search.set(e.value()),
                }
            }

            if source == ContactSource::Demo && !is_loading {
                div {
                    class: "mb-3 text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                    "Backend contacts API not reachable - showing demo rows."
                }
            }

            // Contacts table
            DataTable {
                loading: is_loading,
                total_items: if source == ContactSource::Backend { remote_contacts.len() } else { 3 },
                current_page: 1,
                per_page: 25,
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Name" }
                            TableHeader { sortable: true, "Company" }
                            TableHeader { "Email" }
                            TableHeader { "Phone" }
                            TableHeader { "Role" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 5, rows: 5 }
                    } else if source == ContactSource::Backend && remote_contacts.is_empty() {
                        TableEmpty { columns: 5, message: "No contacts yet.".to_string() }
                    } else {
                        TableBody {
                            if source == ContactSource::Backend {
                                for contact in remote_contacts.iter().cloned() {
                                    ContactRow {
                                        key: "{contact.id}",
                                        id: contact.id.to_string(),
                                        name: format!("{} {}", contact.first_name, contact.last_name).trim().to_string(),
                                        company: contact.company_name.clone().unwrap_or_default(),
                                        company_id: contact.company_id.map(|id| id.to_string()).unwrap_or_default(),
                                        email: contact.email.clone().unwrap_or_default(),
                                        phone: contact.phone_primary.clone().unwrap_or_default(),
                                        role: contact.job_title.clone().unwrap_or_default(),
                                    }
                                }
                            } else {
                                ContactRow {
                                    id: "1",
                                    name: "Bob Johnson",
                                    company: "Acme Corp",
                                    company_id: "1",
                                    email: "bob@acme.com",
                                    phone: "(555) 123-4567",
                                    role: "Primary Contact",
                                }
                                ContactRow {
                                    id: "2",
                                    name: "Alice Williams",
                                    company: "TechStart Inc",
                                    company_id: "2",
                                    email: "alice@techstart.com",
                                    phone: "(555) 234-5678",
                                    role: "CTO",
                                }
                                ContactRow {
                                    id: "3",
                                    name: "Charlie Brown",
                                    company: "Global Widgets",
                                    company_id: "3",
                                    email: "charlie@widgets.com",
                                    phone: "(555) 345-6789",
                                    role: "IT Director",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContactRowProps {
    id: String,
    name: String,
    company: String,
    company_id: String,
    email: String,
    phone: String,
    role: String,
}

#[component]
fn ContactRow(props: ContactRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::ContactDetail { id: id.clone() }); },
            TableCell {
                Link {
                    to: Route::ContactDetail { id: props.id.clone() },
                    class: "font-medium text-blue-600 hover:text-blue-500",
                    "{props.name}"
                }
            }
            TableCell {
                Link {
                    to: Route::CompanyDetail { id: props.company_id.clone() },
                    class: "text-gray-600 hover:text-blue-600",
                    "{props.company}"
                }
            }
            TableCell { "{props.email}" }
            TableCell { "{props.phone}" }
            TableCell { "{props.role}" }
        }
    }
}

/// New contact page
#[component]
pub fn ContactNewPage() -> Element {
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut company = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);

    let company_options = vec![
        SelectOption::new("1", "Acme Corp"),
        SelectOption::new("2", "TechStart Inc"),
        SelectOption::new("3", "Global Widgets"),
    ];

    rsx! {
        AppLayout { title: "New Contact",
            PageHeader {
                title: "New Contact",
                subtitle: "Add a new contact",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        is_submitting.set(true);
                        // P1-04: mock submit until create_contact endpoint
                        // is wired through the client.
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use gloo_timers::future::TimeoutFuture;
                                TimeoutFuture::new(1000).await;
                            }
                            is_submitting.set(false);
                            dioxus::prelude::navigator().push(Route::ContactList {});
                        });
                    },

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        crate::components::Input {
                            name: "first_name",
                            label: "First Name",
                            required: true,
                            value: first_name.read().clone(),
                            oninput: move |e: FormEvent| first_name.set(e.value()),
                        }
                        crate::components::Input {
                            name: "last_name",
                            label: "Last Name",
                            required: true,
                            value: last_name.read().clone(),
                            oninput: move |e: FormEvent| last_name.set(e.value()),
                        }
                    }

                    crate::components::Input {
                        name: "email",
                        label: "Email",
                        r#type: "email",
                        required: true,
                        value: email.read().clone(),
                        oninput: move |e: FormEvent| email.set(e.value()),
                    }

                    Select {
                        name: "company",
                        label: "Company",
                        options: company_options,
                        value: company.read().clone(),
                        placeholder: "Select a company",
                        required: true,
                        onchange: move |e: FormEvent| company.set(e.value()),
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::ContactList {},
                            Button { variant: ButtonVariant::Secondary, "Cancel" }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Create Contact"
                        }
                    }
                }
            }
        }
    }
}

/// Contact detail page
#[derive(Props, Clone, PartialEq)]
pub struct ContactDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn ContactDetailPage(props: ContactDetailPageProps) -> Element {
    rsx! {
        AppLayout { title: "Contact Detail",
            PageHeader {
                title: "Bob Johnson",
                subtitle: "Acme Corp",
                actions: rsx! {
                    Button { variant: ButtonVariant::Secondary, "Edit" }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Main content
                div { class: "lg:col-span-2 space-y-6",
                    // Recent activity
                    Card { title: "Recent Activity",
                        div { class: "space-y-4",
                            p { class: "text-sm text-gray-500", "Created ticket TKT-1234 - 2 hours ago" }
                            p { class: "text-sm text-gray-500", "Received email notification - 1 day ago" }
                            p { class: "text-sm text-gray-500", "Account created - Jan 1, 2024" }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    Card { title: "Contact Information",
                        dl { class: "space-y-4",
                            div {
                                dt { class: "text-sm text-gray-500", "Email" }
                                dd { class: "mt-1",
                                    a { href: "mailto:bob@acme.com", class: "text-blue-600", "bob@acme.com" }
                                }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Phone" }
                                dd { class: "mt-1", "(555) 123-4567" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Mobile" }
                                dd { class: "mt-1", "(555) 987-6543" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Role" }
                                dd { class: "mt-1", "Primary Contact" }
                            }
                            div {
                                dt { class: "text-sm text-gray-500", "Company" }
                                dd { class: "mt-1",
                                    Link {
                                        to: Route::CompanyDetail { id: "1".to_string() },
                                        class: "text-blue-600 hover:text-blue-500",
                                        "Acme Corp"
                                    }
                                }
                            }
                        }
                    }

                    Card { title: "Portal Access",
                        div { class: "space-y-3",
                            div { class: "flex items-center justify-between",
                                span { class: "text-sm text-gray-500", "Status" }
                                Badge { variant: BadgeVariant::Green, "Active" }
                            }
                            div { class: "flex items-center justify-between",
                                span { class: "text-sm text-gray-500", "Last Login" }
                                span { class: "text-sm", "Today, 9:15 AM" }
                            }
                        }
                    }
                }
            }
        }
    }
}

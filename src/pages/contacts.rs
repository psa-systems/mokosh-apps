//! Contact and company pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, PageHeader,
    PlusIcon, SearchInput, Select, SelectOption, SortDirection, Table, TableBody, TableCell,
    TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::Route;

/// Rows per page for the client-side paginated list views (F3).
const PER_PAGE: usize = 25;

/// Sortable columns on the company list (F3).
#[derive(Clone, Copy, PartialEq)]
enum CompanySortKey {
    Company,
    Type,
}

/// Sortable columns on the contact list (F3).
#[derive(Clone, Copy, PartialEq)]
enum ContactSortKey {
    Name,
    Company,
}

/// First click sorts ascending; clicking the active column toggles
/// direction. Resets to page 1 since re-sorting changes the first page.
fn toggle_sort<K: Copy + PartialEq + 'static>(
    current: &mut Signal<Option<(K, SortDirection)>>,
    key: K,
    page: &mut Signal<usize>,
) {
    let next = match *current.read() {
        Some((k, SortDirection::Ascending)) if k == key => Some((key, SortDirection::Descending)),
        Some((k, SortDirection::Descending)) if k == key => Some((key, SortDirection::Ascending)),
        _ => Some((key, SortDirection::Ascending)),
    };
    current.set(next);
    page.set(1);
}

fn sort_dir_for<K: Copy + PartialEq>(
    current: &Option<(K, SortDirection)>,
    key: K,
) -> Option<SortDirection> {
    current.and_then(|(k, dir)| if k == key { Some(dir) } else { None })
}

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

/// Company list page
#[component]
pub fn CompanyListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut type_filter = use_signal(String::new);
    // F3: sort + pagination state.
    let mut sort = use_signal(|| None::<(CompanySortKey, SortDirection)>);
    let mut page = use_signal(|| 1usize);

    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("customer", "Customer"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
    ];

    // F1: read the active-tenant generation so Dioxus re-runs this
    // resource on an org switch / token swap and re-fetches the new
    // tenant's rows instead of leaving the prior tenant's cached.
    let companies_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<PaginatedCompanies>("/companies", &token)
            .await
            .ok()
            .map(|resp| resp.data)
    });

    let resource_snapshot = companies_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let remote_companies: Vec<RemoteCompany> = match &*resource_snapshot {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // F3: client-side filter (search + type), sort, and pagination over
    // the live backend rows.
    let search_q = search.read().trim().to_lowercase();
    let type_q = type_filter.read().clone();
    let mut filtered: Vec<RemoteCompany> = remote_companies
        .iter()
        .filter(|c| {
            if !search_q.is_empty()
                && !c.name.to_lowercase().contains(&search_q)
                && !c
                    .account_manager_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&search_q)
            {
                return false;
            }
            if !type_q.is_empty() && c.company_type != type_q {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    if let Some((key, dir)) = *sort.read() {
        filtered.sort_by(|a, b| {
            let ord = match key {
                CompanySortKey::Company => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                CompanySortKey::Type => a.company_type.cmp(&b.company_type),
            };
            match dir {
                SortDirection::Ascending => ord,
                SortDirection::Descending => ord.reverse(),
            }
        });
    }

    let filtered_total = filtered.len();
    let current_page = (*page.read()).max(1);
    let page_start = (current_page - 1) * PER_PAGE;
    let page_rows: Vec<RemoteCompany> = filtered
        .into_iter()
        .skip(page_start)
        .take(PER_PAGE)
        .collect();
    let sort_snapshot = *sort.read();

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

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load companies. Refresh the page to retry."
                }
            }

            // Companies table
            DataTable {
                loading: is_loading,
                total_items: filtered_total,
                current_page,
                per_page: PER_PAGE,
                columns: 4,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader {
                                sortable: true,
                                sort_direction: sort_dir_for(&sort_snapshot, CompanySortKey::Company),
                                onsort: move |_| toggle_sort(&mut sort, CompanySortKey::Company, &mut page),
                                "Company"
                            }
                            TableHeader {
                                sortable: true,
                                sort_direction: sort_dir_for(&sort_snapshot, CompanySortKey::Type),
                                onsort: move |_| toggle_sort(&mut sort, CompanySortKey::Type, &mut page),
                                "Type"
                            }
                            TableHeader { "Account Manager" }
                            TableHeader { "Open Tickets" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 4, rows: 5 }
                    } else if page_rows.is_empty() {
                        TableEmpty {
                            columns: 4,
                            message: if remote_companies.is_empty() {
                                "No companies yet. Click New Company to create one.".to_string()
                            } else {
                                "No companies match your filters.".to_string()
                            },
                        }
                    } else {
                        TableBody {
                            for company in page_rows.iter().cloned() {
                                CompanyRow {
                                    key: "{company.id}",
                                    id: company.id.to_string(),
                                    name: company.name,
                                    company_type: humanize_company_type(&company.company_type),
                                    primary_contact: company.account_manager_name.unwrap_or_default(),
                                    open_tickets: company.open_ticket_count.unwrap_or(0).max(0) as u32,
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
        }
    }
}

/// New company page
#[component]
pub fn CompanyNewPage() -> Element {
    rsx! {
        AppLayout { title: "New Company",
            PageHeader { title: "New Company", subtitle: "Add a new company account" }
            CompanyForm {
                initial: CompanyFormValues::default(),
                mode: CompanyFormMode::Create,
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct CompanyEditPageProps {
    pub id: String,
}

#[component]
pub fn CompanyEditPage(props: CompanyEditPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let id_for_form = props.id.clone();
    let detail_resource = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<CompanyEditPayload>(&format!("/companies/{id}"))
                .await
                .ok()
        }
    });
    let snap = detail_resource.read_unchecked();
    rsx! {
        AppLayout { title: "Edit Company",
            PageHeader { title: "Edit Company" }
            match &*snap {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading company..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load company." }
                            Link {
                                to: Route::CompanyList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to companies"
                            }
                        }
                    }
                },
                Some(Some(payload)) => {
                    let initial = CompanyFormValues {
                        name: payload.name.clone(),
                        company_type: payload.company_type.clone(),
                        industry: payload.industry.clone().unwrap_or_default(),
                        website: payload.website.clone().unwrap_or_default(),
                        phone: payload.phone.clone().unwrap_or_default(),
                        address_line1: payload.address.line1.clone().unwrap_or_default(),
                        address_line2: payload.address.line2.clone().unwrap_or_default(),
                        address_city: payload.address.city.clone().unwrap_or_default(),
                        address_state: payload.address.state.clone().unwrap_or_default(),
                        address_postal_code: payload.address.postal_code.clone().unwrap_or_default(),
                        address_country: payload.address.country.clone().unwrap_or_default(),
                    };
                    let id = id_for_form.clone();
                    rsx! {
                        CompanyForm {
                            initial,
                            mode: CompanyFormMode::Edit { id },
                        }
                    }
                },
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct CompanyEditPayload {
    name: String,
    #[serde(default)]
    company_type: String,
    #[serde(default)]
    industry: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    address: CompanyAddress,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyFormValues {
    name: String,
    company_type: String,
    industry: String,
    website: String,
    phone: String,
    address_line1: String,
    address_line2: String,
    address_city: String,
    address_state: String,
    address_postal_code: String,
    address_country: String,
}

#[derive(Clone, Debug, PartialEq)]
enum CompanyFormMode {
    Create,
    Edit { id: String },
}

#[derive(Props, Clone, PartialEq)]
struct CompanyFormProps {
    initial: CompanyFormValues,
    mode: CompanyFormMode,
}

#[component]
fn CompanyForm(props: CompanyFormProps) -> Element {
    let initial = props.initial.clone();
    let mode = props.mode.clone();
    let initial_type = if initial.company_type.is_empty() {
        "customer".to_string()
    } else {
        initial.company_type.clone()
    };

    let mut name = use_signal(|| initial.name.clone());
    let mut company_type = use_signal(|| initial_type.clone());
    let mut industry = use_signal(|| initial.industry.clone());
    let mut website = use_signal(|| initial.website.clone());
    let mut phone = use_signal(|| initial.phone.clone());
    let mut line1 = use_signal(|| initial.address_line1.clone());
    let mut line2 = use_signal(|| initial.address_line2.clone());
    let mut city = use_signal(|| initial.address_city.clone());
    let mut state = use_signal(|| initial.address_state.clone());
    let mut postal = use_signal(|| initial.address_postal_code.clone());
    let mut country = use_signal(|| initial.address_country.clone());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let type_options = vec![
        SelectOption::new("customer", "Customer"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
        SelectOption::new("partner", "Partner"),
    ];

    let navigator = use_navigator();
    let submit_label = match &mode {
        CompanyFormMode::Create => "Create Company",
        CompanyFormMode::Edit { .. } => "Save Changes",
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "company_type": company_type.read().clone(),
            "industry": optional_string(&industry.read()),
            "website": optional_string(&website.read()),
            "phone": optional_string(&phone.read()),
            "address": {
                "line1": optional_string(&line1.read()),
                "line2": optional_string(&line2.read()),
                "city": optional_string(&city.read()),
                "state": optional_string(&state.read()),
                "postal_code": optional_string(&postal.read()),
                "country": optional_string(&country.read()),
            },
        });
        let mode = mode.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct CompanyId {
                    id: uuid::Uuid,
                }
                let result = match &mode {
                    CompanyFormMode::Create => {
                        crate::hooks::fetch::api::post_authed::<CompanyId, _>("/companies", &body)
                            .await
                            .map(|c| c.id.to_string())
                    }
                    CompanyFormMode::Edit { id } => {
                        let path = format!("/companies/{id}");
                        crate::hooks::fetch::api::put_authed::<CompanyId, _>(&path, &body)
                            .await
                            .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        navigator.push(Route::CompanyDetail { id });
                    }
                    Err(err) => {
                        error.set(format!("Could not save company: {err}"));
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    rsx! {
        Card {
            form {
                class: "space-y-6",
                onsubmit: handle_submit,

                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
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
                    crate::components::Input {
                        name: "industry",
                        label: "Industry",
                        placeholder: "e.g. Healthcare",
                        value: industry.read().clone(),
                        oninput: move |e: FormEvent| industry.set(e.value()),
                    }
                    crate::components::Input {
                        name: "website",
                        label: "Website",
                        placeholder: "https://example.com",
                        value: website.read().clone(),
                        oninput: move |e: FormEvent| website.set(e.value()),
                    }
                    crate::components::Input {
                        name: "phone",
                        label: "Phone",
                        placeholder: "(555) 555-5555",
                        value: phone.read().clone(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                }

                h3 { class: "text-sm font-medium text-gray-900 dark:text-gray-100 pt-2",
                    "Address"
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "address_line1",
                        label: "Street",
                        value: line1.read().clone(),
                        oninput: move |e: FormEvent| line1.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_line2",
                        label: "Street (line 2)",
                        value: line2.read().clone(),
                        oninput: move |e: FormEvent| line2.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_city",
                        label: "City",
                        value: city.read().clone(),
                        oninput: move |e: FormEvent| city.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_state",
                        label: "State / Region",
                        value: state.read().clone(),
                        oninput: move |e: FormEvent| state.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_postal_code",
                        label: "Postal Code",
                        value: postal.read().clone(),
                        oninput: move |e: FormEvent| postal.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_country",
                        label: "Country",
                        value: country.read().clone(),
                        oninput: move |e: FormEvent| country.set(e.value()),
                    }
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
                        "{submit_label}"
                    }
                }
            }
        }
    }
}

fn optional_string(value: &str) -> serde_json::Value {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(trimmed.to_string())
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
    let company_id_str = props.id.clone();
    let company_id_for_resource = company_id_str.clone();
    let company_id_for_contacts = company_id_str.clone();
    let company_id_for_sites = company_id_str.clone();
    let company_id_for_tickets = company_id_str.clone();
    let company_id_for_edit = company_id_str.clone();
    let company_id_for_delete = company_id_str.clone();

    let company_resource = use_resource(move || {
        let id = company_id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<CompanyDetail>(&format!("/companies/{id}"))
                .await
                .ok()
        }
    });
    let contacts_resource = use_resource(move || {
        let id = company_id_for_contacts.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Vec<RemoteContact>>(&format!(
                "/companies/{id}/contacts"
            ))
            .await
            .ok()
        }
    });
    let sites_resource = use_resource(move || {
        let id = company_id_for_sites.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Vec<SiteSummary>>(&format!(
                "/companies/{id}/sites"
            ))
            .await
            .ok()
        }
    });
    let tickets_resource = use_resource(move || {
        let id = company_id_for_tickets.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<PaginatedTicketSummaries>(&format!(
                "/tickets?company_id={id}&page_size=5&sort=-updated_at"
            ))
            .await
            .ok()
        }
    });

    let company_snapshot = company_resource.read_unchecked();
    let header_title = match &*company_snapshot {
        Some(Some(c)) => c.name.clone(),
        _ => "Company".to_string(),
    };

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let edit_id = company_id_for_edit.clone();
    let delete_id = company_id_for_delete.clone();

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Link {
                        to: Route::CompanyEdit { id: edit_id },
                        Button {
                            variant: ButtonVariant::Secondary,
                            "Edit"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        loading: *deleting.read(),
                        onclick: move |_| {
                            let id = delete_id.clone();
                            deleting.set(true);
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let confirmed = web_sys::window()
                                        .and_then(|w| {
                                            w.confirm_with_message(
                                                "Delete this company? This will also remove its sites and unlink its contacts/tickets.",
                                            )
                                            .ok()
                                        })
                                        .unwrap_or(false);
                                    if confirmed {
                                        let path = format!("/companies/{id}");
                                        if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                                            navigator.push(Route::CompanyList {});
                                        }
                                    }
                                }
                                deleting.set(false);
                            });
                        },
                        "Delete"
                    }
                },
            }

            match &*company_snapshot {
                None => rsx! {
                    Card {
                        div { class: "py-12 text-center text-sm text-gray-500", "Loading company..." }
                    }
                },
                Some(None) => rsx! {
                    Card {
                        div {
                            class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load company." }
                            Link {
                                to: Route::CompanyList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to companies"
                            }
                        }
                    }
                },
                Some(Some(company)) => {
                    let address_parts: Vec<String> = [
                        company.address.line1.clone(),
                        company.address.line2.clone(),
                        company.address.city.clone(),
                        company.address.state.clone(),
                        company.address.postal_code.clone(),
                        company.address.country.clone(),
                    ]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect();
                    let type_label = humanize_company_type(&company.company_type);
                    let website = company.website.clone();
                    let phone = company.phone.clone();
                    let industry = company.industry.clone();
                    let am_name = company.account_manager_name.clone();
                    let open_tickets = company.open_ticket_count.unwrap_or(0).max(0);
                    let contact_count = company.contact_count.unwrap_or(0).max(0);
                    let site_count = company.site_count.unwrap_or(0).max(0);
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            div { class: "lg:col-span-2 space-y-6",
                                // Contacts
                                CompanyContactsCard { contacts_resource }
                                // Sites
                                CompanySitesCard { sites_resource }
                                // Recent tickets
                                CompanyTicketsCard { tickets_resource }
                            }
                            // Sidebar
                            div { class: "space-y-6",
                                Card { title: "Details",
                                    dl { class: "space-y-4",
                                        div { class: "flex justify-between",
                                            dt { class: "text-sm text-gray-500", "Type" }
                                            dd { Badge { variant: BadgeVariant::Green, "{type_label}" } }
                                        }
                                        if let Some(industry) = industry {
                                            if !industry.is_empty() {
                                                div { class: "flex justify-between",
                                                    dt { class: "text-sm text-gray-500", "Industry" }
                                                    dd { class: "text-sm", "{industry}" }
                                                }
                                            }
                                        }
                                        if let Some(phone) = phone {
                                            if !phone.is_empty() {
                                                div { class: "flex justify-between",
                                                    dt { class: "text-sm text-gray-500", "Phone" }
                                                    dd { class: "text-sm", "{phone}" }
                                                }
                                            }
                                        }
                                        if let Some(website) = website {
                                            if !website.is_empty() {
                                                div { class: "flex justify-between",
                                                    dt { class: "text-sm text-gray-500", "Website" }
                                                    dd {
                                                        a {
                                                            href: "{website}",
                                                            target: "_blank",
                                                            class: "text-sm text-blue-600 hover:text-blue-500",
                                                            "{website}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(am) = am_name {
                                            if !am.is_empty() {
                                                div { class: "flex justify-between",
                                                    dt { class: "text-sm text-gray-500", "Account Manager" }
                                                    dd { class: "text-sm", "{am}" }
                                                }
                                            }
                                        }
                                        if !address_parts.is_empty() {
                                            div {
                                                dt { class: "text-sm text-gray-500 mb-1", "Address" }
                                                dd { class: "text-sm space-y-0.5",
                                                    for line in address_parts.iter() {
                                                        p { "{line}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Card { title: "Statistics",
                                    div { class: "space-y-3",
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Open Tickets" }
                                            span { class: "font-medium text-gray-900 dark:text-white", "{open_tickets}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Contacts" }
                                            span { class: "font-medium", "{contact_count}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-sm text-gray-500", "Sites" }
                                            span { class: "font-medium", "{site_count}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// CompanyResponse subset for the detail page. Mirrors mokosh-server's
/// shape; serde drops fields we don't read.
#[derive(Clone, Debug, Deserialize)]
struct CompanyDetail {
    name: String,
    #[serde(default)]
    company_type: String,
    #[serde(default)]
    industry: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    address: CompanyAddress,
    #[serde(default)]
    account_manager_name: Option<String>,
    #[serde(default)]
    contact_count: Option<i64>,
    #[serde(default)]
    site_count: Option<i64>,
    #[serde(default)]
    open_ticket_count: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CompanyAddress {
    #[serde(default)]
    line1: Option<String>,
    #[serde(default)]
    line2: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    postal_code: Option<String>,
    #[serde(default)]
    country: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct SiteSummary {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    address: CompanyAddress,
    #[serde(default)]
    is_primary: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedTicketSummaries {
    data: Vec<TicketSummary>,
}

#[derive(Clone, Debug, Deserialize)]
struct TicketSummary {
    id: uuid::Uuid,
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: TicketStatusBadge,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TicketStatusBadge {
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_closed: bool,
}

#[component]
fn CompanyContactsCard(contacts_resource: Resource<Option<Vec<RemoteContact>>>) -> Element {
    let snap = contacts_resource.read_unchecked();
    rsx! {
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
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load contacts.".to_string() } },
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No contacts at this company yet.".to_string() }
                    },
                    Some(Some(rows)) => {
                        let rows = rows.clone();
                        rsx! {
                            TableBody {
                                for contact in rows.into_iter() {
                                    {
                                        let id = contact.id.to_string();
                                        let name = format!("{} {}", contact.first_name, contact.last_name).trim().to_string();
                                        let email = contact.email.clone().unwrap_or_default();
                                        let phone = contact.phone_primary.clone().unwrap_or_default();
                                        let role = contact.job_title.clone().unwrap_or_default();
                                        rsx! {
                                            TableRow { key: "{id}",
                                                TableCell {
                                                    Link {
                                                        to: Route::ContactDetail { id: id.clone() },
                                                        class: "font-medium text-blue-600 hover:text-blue-500",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { "{email}" }
                                                TableCell { "{phone}" }
                                                TableCell { "{role}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CompanySitesCard(sites_resource: Resource<Option<Vec<SiteSummary>>>) -> Element {
    let snap = sites_resource.read_unchecked();
    rsx! {
        Card {
            title: "Sites",
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Address" }
                        TableHeader { "Primary" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 3, rows: 2 } },
                    Some(None) => rsx! { TableEmpty { columns: 3, message: "Could not load sites.".to_string() } },
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty { columns: 3, message: "No sites for this company yet.".to_string() }
                    },
                    Some(Some(rows)) => {
                        let rows = rows.clone();
                        rsx! {
                            TableBody {
                                for site in rows.into_iter() {
                                    {
                                        let key = site.id.to_string();
                                        let parts: Vec<String> = [
                                            site.address.line1.clone(),
                                            site.address.city.clone(),
                                            site.address.state.clone(),
                                        ]
                                        .into_iter()
                                        .flatten()
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                        let addr = parts.join(", ");
                                        let is_primary = site.is_primary;
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell { "{site.name}" }
                                                TableCell { class: "text-gray-500", "{addr}" }
                                                TableCell {
                                                    if is_primary {
                                                        Badge { variant: BadgeVariant::Blue, "Primary" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn CompanyTicketsCard(tickets_resource: Resource<Option<PaginatedTicketSummaries>>) -> Element {
    let snap = tickets_resource.read_unchecked();
    rsx! {
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
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 2, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 2, message: "Could not load tickets.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 2, message: "No tickets for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows = page.data.clone();
                        rsx! {
                            TableBody {
                                for ticket in rows.into_iter() {
                                    {
                                        let id = ticket.id.to_string();
                                        let key = id.clone();
                                        let number = ticket.ticket_number.clone();
                                        let title = ticket.title.clone();
                                        let status_name = ticket.status.name.clone();
                                        let variant = if ticket.status.is_closed {
                                            BadgeVariant::Gray
                                        } else {
                                            BadgeVariant::Blue
                                        };
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell {
                                                    div {
                                                        Link {
                                                            to: Route::TicketDetail { id: id.clone() },
                                                            class: "font-medium text-blue-600 hover:text-blue-500",
                                                            "{number}"
                                                        }
                                                        p { class: "text-sm text-gray-500", "{title}" }
                                                    }
                                                }
                                                TableCell {
                                                    Badge { variant, "{status_name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Contact list page
#[component]
pub fn ContactListPage() -> Element {
    let mut search = use_signal(String::new);
    // F3: sort + pagination state.
    let mut sort = use_signal(|| None::<(ContactSortKey, SortDirection)>);
    let mut page = use_signal(|| 1usize);

    let contacts_resource = use_resource(|| async {
        // F1: re-fetch on org switch (see CompanyListPage for rationale).
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_with_auth::<PaginatedContacts>("/contacts", &token)
            .await
            .ok()
            .map(|resp| resp.data)
    });

    let resource_snapshot = contacts_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let remote_contacts: Vec<RemoteContact> = match &*resource_snapshot {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // F3: client-side filter (search), sort, and pagination over the
    // live backend rows. Demo rows below are an unfiltered fallback.
    let search_q = search.read().trim().to_lowercase();
    let mut filtered: Vec<RemoteContact> = remote_contacts
        .iter()
        .filter(|c| {
            if search_q.is_empty() {
                return true;
            }
            let hay = format!(
                "{} {} {} {}",
                c.first_name,
                c.last_name,
                c.company_name.as_deref().unwrap_or_default(),
                c.email.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            hay.contains(&search_q)
        })
        .cloned()
        .collect();

    if let Some((key, dir)) = *sort.read() {
        filtered.sort_by(|a, b| {
            let ord = match key {
                ContactSortKey::Name => {
                    let an = format!("{} {}", a.first_name, a.last_name).to_lowercase();
                    let bn = format!("{} {}", b.first_name, b.last_name).to_lowercase();
                    an.cmp(&bn)
                }
                ContactSortKey::Company => a
                    .company_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&b.company_name.as_deref().unwrap_or_default().to_lowercase()),
            };
            match dir {
                SortDirection::Ascending => ord,
                SortDirection::Descending => ord.reverse(),
            }
        });
    }

    let filtered_total = filtered.len();
    let current_page = (*page.read()).max(1);
    let page_start = (current_page - 1) * PER_PAGE;
    let page_rows: Vec<RemoteContact> = filtered
        .into_iter()
        .skip(page_start)
        .take(PER_PAGE)
        .collect();
    let sort_snapshot = *sort.read();

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

            if fetch_failed {
                div {
                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    "Could not load contacts. Refresh the page to retry."
                }
            }

            // Contacts table
            DataTable {
                loading: is_loading,
                total_items: filtered_total,
                current_page,
                per_page: PER_PAGE,
                columns: 5,
                onpagechange: move |p| page.set(p),
                Table {
                    TableHead {
                        TableRow {
                            TableHeader {
                                sortable: true,
                                sort_direction: sort_dir_for(&sort_snapshot, ContactSortKey::Name),
                                onsort: move |_| toggle_sort(&mut sort, ContactSortKey::Name, &mut page),
                                "Name"
                            }
                            TableHeader {
                                sortable: true,
                                sort_direction: sort_dir_for(&sort_snapshot, ContactSortKey::Company),
                                onsort: move |_| toggle_sort(&mut sort, ContactSortKey::Company, &mut page),
                                "Company"
                            }
                            TableHeader { "Email" }
                            TableHeader { "Phone" }
                            TableHeader { "Role" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 5, rows: 5 }
                    } else if page_rows.is_empty() {
                        TableEmpty {
                            columns: 5,
                            message: if remote_contacts.is_empty() {
                                "No contacts yet. Click New Contact to add one.".to_string()
                            } else {
                                "No contacts match your search.".to_string()
                            },
                        }
                    } else {
                        TableBody {
                            for contact in page_rows.iter().cloned() {
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

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);

        // Snapshot signals so the spawn doesn't borrow them across await.
        let first_name_v = first_name.read().clone();
        let last_name_v = last_name.read().clone();
        let email_v = email.read().clone();
        let company_v = company.read().clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // F5: real POST to /contacts (the list page already GETs
                // from this route). The company Select still ships the
                // hardcoded "1"/"2"/"3" placeholders until it is wired to
                // the companies list (tracked under the contacts story);
                // parse as Uuid with a nil() fallback so the POST
                // exercises the wire and the server returns a typed
                // validation error rather than us faking success.
                let company_id =
                    uuid::Uuid::parse_str(&company_v).unwrap_or_else(|_| uuid::Uuid::nil());
                let body = serde_json::json!({
                    "first_name": first_name_v,
                    "last_name": last_name_v,
                    "email": if email_v.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(email_v)
                    },
                    "company_id": company_id,
                });

                #[derive(serde::Deserialize)]
                struct CreatedContact {
                    id: uuid::Uuid,
                }

                match crate::hooks::fetch::api::post_authed::<CreatedContact, _>("/contacts", &body)
                    .await
                {
                    Ok(created) => {
                        navigator.push(Route::ContactDetail {
                            id: created.id.to_string(),
                        });
                    }
                    Err(err) => {
                        web_sys::console::error_1(
                            &format!("Could not create contact: {err}").into(),
                        );
                    }
                }
            }

            is_submitting.set(false);
        });
    };

    rsx! {
        AppLayout { title: "New Contact",
            PageHeader {
                title: "New Contact",
                subtitle: "Add a new contact",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: handle_submit,

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
    let header_title = format!("Contact {}", props.id);
    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                // F5: Edit was decorative (no onclick). Hidden until
                // the contacts mutation surface ships.
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

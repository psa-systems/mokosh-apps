//! Contact and company pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, IconSize, Modal,
    PageHeader, PlusIcon, SearchInput, Select, SelectOption, SortDirection, Table, TableBody,
    TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::modules::contacts::Address;
use crate::utils::url::urlencoding_minimal;
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

fn company_sort_query(
    current: Option<(CompanySortKey, SortDirection)>,
) -> Option<(&'static str, &'static str)> {
    let (key, dir) = current?;
    let field = match key {
        CompanySortKey::Company => "name",
        CompanySortKey::Type => "company_type",
    };
    let dir = match dir {
        SortDirection::Ascending => "asc",
        SortDirection::Descending => "desc",
    };
    Some((field, dir))
}

fn contact_sort_query(
    current: Option<(ContactSortKey, SortDirection)>,
) -> Option<(&'static str, &'static str)> {
    let (key, dir) = current?;
    let field = match key {
        ContactSortKey::Name => "last_name",
        ContactSortKey::Company => "company_name",
    };
    let dir = match dir {
        SortDirection::Ascending => "asc",
        SortDirection::Descending => "desc",
    };
    Some((field, dir))
}

/// Map the server's lowercased `CompanyType` enum tag (`"client"`,
/// `"prospect"`, `"vendor"`, `"partner"`) to the title-case label that
/// `CompanyRow` keys its badge variant on. Unknown values fall through
/// unchanged so future variants don't disappear.
fn humanize_company_type(raw: &str) -> String {
    match raw {
        "client" => "Client".to_string(),
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
    #[serde(default)]
    meta: PaginationMeta,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PaginationMeta {
    #[serde(default)]
    total: u64,
}

/// Subset of mokosh-server's `ContactResponse` we render in the contacts
/// list. As with companies, serde drops unknown fields so this can grow
/// without breaking decoding. Field names match the server's
/// `ContactResponse` shape (`phone`, `title`); earlier names
/// (`phone_primary`, `job_title`) silently parsed as `None` because
/// `#[serde(default)]` swallowed the absent fields, which is why the
/// company-detail Contacts card showed blank Phone and Role columns.
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
    phone: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedContacts {
    data: Vec<RemoteContact>,
    #[serde(default)]
    meta: PaginationMeta,
}

/// Company list page
#[component]
pub fn CompanyListPage() -> Element {
    let mut search = use_signal(String::new);
    let mut type_filter = use_signal(String::new);
    let mut sort = use_signal(|| None::<(CompanySortKey, SortDirection)>);
    let mut page = use_signal(|| 1usize);

    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("client", "Client"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
    ];

    let search_text = search.read().trim().to_string();
    let type_text = type_filter.read().clone();
    let current_page = (*page.read()).max(1);
    let sort_snapshot = *sort.read();

    // MAPPS-148: read every reactive input (page, search, type filter,
    // sort) INSIDE the resource closure. Dioxus `use_resource` only
    // re-runs when a signal read within the closure changes; values
    // captured by value (the old `*_for_resource` snapshots) never
    // subscribe the resource, so paging/filtering merely re-rendered the
    // footer label while the resource kept serving page 1. Reading the
    // signals here subscribes the resource so a page change fetches and
    // binds the requested page. `active_tenant_generation` stays read so
    // an org switch / token swap still re-fetches.
    let companies_resource = use_resource(move || {
        let q = search.read().trim().to_string();
        let type_filter = type_filter.read().clone();
        let sort = company_sort_query(*sort.read());
        let current_page = (*page.read()).max(1);
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/contacts/companies?page={current_page}&per_page={PER_PAGE}");
            if !q.is_empty() {
                path.push_str(&format!("&q={}", urlencoding_minimal(&q)));
            }
            if !type_filter.is_empty() {
                path.push_str(&format!(
                    "&company_type={}",
                    urlencoding_minimal(&type_filter)
                ));
            }
            if let Some((field, dir)) = sort {
                path.push_str(&format!("&sort={field}&sort_dir={dir}"));
            }
            crate::hooks::fetch::api::get_with_auth::<PaginatedCompanies>(&path, &token)
                .await
                .ok()
        }
    });

    let resource_snapshot = companies_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (page_rows, total): (Vec<RemoteCompany>, u64) = match &*resource_snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !search_text.is_empty() || !type_text.is_empty();

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
                            oninput: move |e: FormEvent| {
                                search.set(e.value());
                                page.set(1);
                            },
                        }
                    }
                    Select {
                        name: "type",
                        options: type_options,
                        value: type_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            type_filter.set(e.value());
                            page.set(1);
                        },
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
                total_items: total as usize,
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
                            message: if has_filters {
                                "No companies match your filters.".to_string()
                            } else {
                                "No companies yet. Click New Company to create one.".to_string()
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
        "Client" => BadgeVariant::Green,
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
            crate::hooks::fetch::api::get_authed::<CompanyEditPayload>(&format!(
                "/contacts/companies/{id}"
            ))
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
    address: Address,
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
        "client".to_string()
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
        SelectOption::new("client", "Client"),
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
                        crate::hooks::fetch::api::post_authed::<CompanyId, _>(
                            "/contacts/companies",
                            &body,
                        )
                        .await
                        .map(|c| c.id.to_string())
                    }
                    CompanyFormMode::Edit { id } => {
                        let path = format!("/contacts/companies/{id}");
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
            crate::hooks::fetch::api::get_authed::<CompanyDetail>(&format!(
                "/contacts/companies/{id}"
            ))
            .await
            .ok()
        }
    });
    let contacts_resource = use_resource(move || {
        let id = company_id_for_contacts.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // Server returns a paginated envelope `{data, meta}`, not a
            // bare `Vec`. Decoding into `Vec<RemoteContact>` always fails
            // here and `.ok()` swallowed it as `None`, rendering the
            // "Could not load contacts" empty state.
            crate::hooks::fetch::api::get_authed::<PaginatedContacts>(&format!(
                "/contacts/companies/{id}/contacts"
            ))
            .await
            .ok()
        }
    });
    let sites_resource = use_resource(move || {
        let id = company_id_for_sites.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<PaginatedSites>(&format!(
                "/contacts/companies/{id}/sites"
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
                "/tickets?company_id={id}&per_page=5&sort=-updated_at"
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
                                        let path = format!("/contacts/companies/{id}");
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
                                CompanyContactsCard {
                                    company_id: company_id_str.clone(),
                                    company_name: company.name.clone(),
                                    contacts_resource,
                                }
                                // Sites
                                CompanySitesCard {
                                    company_id: company_id_str.clone(),
                                    sites_resource,
                                }
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
    address: Address,
    #[serde(default)]
    account_manager_name: Option<String>,
    #[serde(default)]
    contact_count: Option<i64>,
    #[serde(default)]
    site_count: Option<i64>,
    #[serde(default)]
    open_ticket_count: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
struct SiteSummary {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    address: Address,
    #[serde(default)]
    is_primary: bool,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedSites {
    data: Vec<SiteSummary>,
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
fn CompanyContactsCard(
    company_id: String,
    company_name: String,
    contacts_resource: Resource<Option<PaginatedContacts>>,
) -> Element {
    let snap = contacts_resource.read_unchecked();
    let new_href = format!(
        "/contacts/new?company_id={}&company_name={}",
        urlencoding_minimal(&company_id),
        urlencoding_minimal(&company_name)
    );
    rsx! {
        Card {
            title: "Contacts",
            actions: rsx! {
                a {
                    href: "{new_href}",
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
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No contacts at this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows = page.data.clone();
                        rsx! {
                            TableBody {
                                for contact in rows.into_iter() {
                                    {
                                        let id = contact.id.to_string();
                                        let name = format!("{} {}", contact.first_name, contact.last_name).trim().to_string();
                                        let email = contact.email.clone().unwrap_or_default();
                                        let phone = contact.phone.clone().unwrap_or_default();
                                        let role = contact.title.clone().unwrap_or_default();
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
fn CompanySitesCard(
    company_id: String,
    mut sites_resource: Resource<Option<PaginatedSites>>,
) -> Element {
    let snap = sites_resource.read_unchecked();
    let mut editing = use_signal(|| None::<SiteFormState>);

    rsx! {
        Card {
            title: "Sites",
            actions: rsx! {
                button {
                    r#type: "button",
                    class: "text-sm text-blue-600 hover:text-blue-500",
                    onclick: {
                        let company_id = company_id.clone();
                        move |_| {
                            editing.set(Some(SiteFormState::new_for_company(&company_id)));
                        }
                    },
                    "New Site"
                }
            },
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
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 3, message: "No sites for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows = page.data.clone();
                        let company_id = company_id.clone();
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
                                        let site_for_edit = site.clone();
                                        let company_id_for_edit = company_id.clone();
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell {
                                                    button {
                                                        r#type: "button",
                                                        class: "text-left font-medium text-blue-600 hover:text-blue-500",
                                                        onclick: move |_| {
                                                            editing.set(Some(SiteFormState::from_existing(
                                                                &company_id_for_edit,
                                                                &site_for_edit,
                                                            )));
                                                        },
                                                        "{site.name}"
                                                    }
                                                }
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

        if let Some(state) = editing.read().clone() {
            SiteFormModal {
                state,
                onclose: move |_| { editing.set(None); },
                onsaved: move |_| {
                    editing.set(None);
                    sites_resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SiteFormState {
    company_id: String,
    /// Some => edit existing site
    site_id: Option<String>,
    name: String,
    line1: String,
    line2: String,
    city: String,
    state: String,
    postal_code: String,
    country: String,
    phone: String,
    is_primary: bool,
    timezone: String,
}

impl SiteFormState {
    fn new_for_company(company_id: &str) -> Self {
        Self {
            company_id: company_id.to_string(),
            site_id: None,
            name: String::new(),
            line1: String::new(),
            line2: String::new(),
            city: String::new(),
            state: String::new(),
            postal_code: String::new(),
            country: String::new(),
            phone: String::new(),
            is_primary: false,
            timezone: String::new(),
        }
    }

    fn from_existing(company_id: &str, site: &SiteSummary) -> Self {
        Self {
            company_id: company_id.to_string(),
            site_id: Some(site.id.to_string()),
            name: site.name.clone(),
            line1: site.address.line1.clone().unwrap_or_default(),
            line2: site.address.line2.clone().unwrap_or_default(),
            city: site.address.city.clone().unwrap_or_default(),
            state: site.address.state.clone().unwrap_or_default(),
            postal_code: site.address.postal_code.clone().unwrap_or_default(),
            country: site.address.country.clone().unwrap_or_default(),
            phone: site.phone.clone().unwrap_or_default(),
            is_primary: site.is_primary,
            timezone: site.timezone.clone().unwrap_or_default(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SiteFormModalProps {
    state: SiteFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn SiteFormModal(props: SiteFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.site_id.is_some();
    let modal_title = if is_edit { "Edit Site" } else { "New Site" };

    let mut name = use_signal(|| initial.name.clone());
    let mut line1 = use_signal(|| initial.line1.clone());
    let mut line2 = use_signal(|| initial.line2.clone());
    let mut city = use_signal(|| initial.city.clone());
    let mut state = use_signal(|| initial.state.clone());
    let mut postal = use_signal(|| initial.postal_code.clone());
    let mut country = use_signal(|| initial.country.clone());
    let mut phone = use_signal(|| initial.phone.clone());
    let mut timezone = use_signal(|| initial.timezone.clone());
    let mut is_primary = use_signal(|| initial.is_primary);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_state = initial.clone();
    let handle_save = move |_| {
        if saving() || deleting() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Site name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "company_id": save_state.company_id,
            "name": name.read().trim(),
            "address": {
                "line1": optional_string(&line1.read()),
                "line2": optional_string(&line2.read()),
                "city": optional_string(&city.read()),
                "state": optional_string(&state.read()),
                "postal_code": optional_string(&postal.read()),
                "country": optional_string(&country.read()),
            },
            "phone": optional_string(&phone.read()),
            "is_primary": *is_primary.read(),
            "timezone": optional_string(&timezone.read()),
        });
        let site_id = save_state.site_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), String> = match site_id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/contacts/sites",
                        &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => {
                        let path = format!("/contacts/sites/{id}");
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save site: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.site_id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if saving() || deleting() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let confirmed = web_sys::window()
                    .and_then(|w| {
                        w.confirm_with_message("Delete this site? This cannot be undone.")
                            .ok()
                    })
                    .unwrap_or(false);
                if confirmed {
                    let path = format!("/contacts/sites/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(()) => onsaved.call(()),
                        Err(err) => error.set(format!("Could not delete site: {err}")),
                    }
                }
            }
            deleting.set(false);
        });
    };

    let footer = rsx! {
        if is_edit {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                onclick: handle_delete,
                "Delete"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create Site" }
        }
    };

    rsx! {
        Modal {
            open: true,
            title: modal_title,
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                crate::components::Input {
                    name: "site_name",
                    label: "Name",
                    placeholder: "e.g. Main Office",
                    required: true,
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "site_line1",
                        label: "Street",
                        value: line1.read().clone(),
                        oninput: move |e: FormEvent| line1.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_line2",
                        label: "Street (line 2)",
                        value: line2.read().clone(),
                        oninput: move |e: FormEvent| line2.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_city",
                        label: "City",
                        value: city.read().clone(),
                        oninput: move |e: FormEvent| city.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_state",
                        label: "State / Region",
                        value: state.read().clone(),
                        oninput: move |e: FormEvent| state.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_postal",
                        label: "Postal Code",
                        value: postal.read().clone(),
                        oninput: move |e: FormEvent| postal.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_country",
                        label: "Country",
                        value: country.read().clone(),
                        oninput: move |e: FormEvent| country.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_phone",
                        label: "Phone",
                        value: phone.read().clone(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_timezone",
                        label: "Timezone",
                        placeholder: "e.g. America/New_York",
                        value: timezone.read().clone(),
                        oninput: move |e: FormEvent| timezone.set(e.value()),
                    }
                }
                crate::components::Checkbox {
                    name: "site_is_primary",
                    label: "Primary site",
                    checked: *is_primary.read(),
                    help: "Marks this as the main location for the company.",
                    onchange: move |_| {
                        let next = !*is_primary.read();
                        is_primary.set(next);
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
    let mut contact_type_filter = use_signal(String::new);
    let mut portal_filter = use_signal(String::new);
    let mut sort = use_signal(|| None::<(ContactSortKey, SortDirection)>);
    let mut page = use_signal(|| 1usize);

    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("primary", "Primary"),
        SelectOption::new("technical", "Technical"),
        SelectOption::new("billing", "Billing"),
        SelectOption::new("other", "Other"),
    ];
    let portal_options = vec![
        SelectOption::new("", "All Contacts"),
        SelectOption::new("true", "Portal users only"),
        SelectOption::new("false", "Non-portal only"),
    ];

    let search_text = search.read().trim().to_string();
    let type_text = contact_type_filter.read().clone();
    let portal_text = portal_filter.read().clone();
    let current_page = (*page.read()).max(1);
    let sort_snapshot = *sort.read();

    // MAPPS-148: read page/filters/sort INSIDE the resource closure so the
    // resource subscribes to them and re-fetches when they change. Values
    // captured by value never re-trigger a Dioxus resource, which is why
    // paging only moved the footer label and never loaded the next page.
    let contacts_resource = use_resource(move || {
        let q = search.read().trim().to_string();
        let contact_type = contact_type_filter.read().clone();
        let portal = portal_filter.read().clone();
        let sort = contact_sort_query(*sort.read());
        let current_page = (*page.read()).max(1);
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/contacts/contacts?page={current_page}&per_page={PER_PAGE}");
            if !q.is_empty() {
                path.push_str(&format!("&q={}", urlencoding_minimal(&q)));
            }
            if !contact_type.is_empty() {
                path.push_str(&format!(
                    "&contact_type={}",
                    urlencoding_minimal(&contact_type)
                ));
            }
            if !portal.is_empty() {
                path.push_str(&format!("&is_portal_user={portal}"));
            }
            if let Some((field, dir)) = sort {
                path.push_str(&format!("&sort={field}&sort_dir={dir}"));
            }
            crate::hooks::fetch::api::get_with_auth::<PaginatedContacts>(&path, &token)
                .await
                .ok()
        }
    });

    let resource_snapshot = contacts_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (page_rows, total): (Vec<RemoteContact>, u64) = match &*resource_snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !search_text.is_empty() || !type_text.is_empty() || !portal_text.is_empty();

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
                div { class: "flex flex-col sm:flex-row gap-4",
                    div { class: "flex-1",
                        SearchInput {
                            value: search.read().clone(),
                            placeholder: "Search contacts...",
                            oninput: move |e: FormEvent| {
                                search.set(e.value());
                                page.set(1);
                            },
                        }
                    }
                    Select {
                        name: "contact_type",
                        options: type_options,
                        value: contact_type_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            contact_type_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    Select {
                        name: "portal",
                        options: portal_options,
                        value: portal_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            portal_filter.set(e.value());
                            page.set(1);
                        },
                    }
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
                total_items: total as usize,
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
                            message: if has_filters {
                                "No contacts match your filters.".to_string()
                            } else {
                                "No contacts yet. Click New Contact to add one.".to_string()
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
                                    phone: contact.phone.clone().unwrap_or_default(),
                                    role: contact.title.clone().unwrap_or_default(),
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

/// New contact page. When linked with `?company_id=<uuid>` (the
/// "Add Contact" button on the company detail page does this) the
/// CompanyPicker pre-fills with that company and the user only has
/// to fill in the contact's own fields.
#[component]
pub fn ContactNewPage() -> Element {
    // Resolve the prefill from window.location.search. We could
    // route through Dioxus' Route enum but that would require turning
    // the query into a typed param and refactoring every
    // `Route::ContactNew {}` link site; for a single optional prefill
    // a one-shot web-sys read keeps the change local.
    let prefill = use_signal(read_company_prefill_from_url);
    let prefill = prefill.read().clone();

    let initial = ContactFormValues {
        company_id: prefill.id.clone(),
        company_name: prefill.name.clone(),
        contact_type: "other".to_string(),
        ..ContactFormValues::default()
    };

    rsx! {
        AppLayout { title: "New Contact",
            PageHeader { title: "New Contact", subtitle: "Add a new contact" }
            ContactForm {
                initial,
                mode: ContactFormMode::Create,
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyPrefill {
    id: String,
    name: String,
}

fn read_company_prefill_from_url() -> CompanyPrefill {
    #[cfg(feature = "web")]
    {
        if let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) {
            if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                let id = params.get("company_id").unwrap_or_default();
                let name = params.get("company_name").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return CompanyPrefill { id, name };
                }
            }
        }
    }
    CompanyPrefill::default()
}

#[derive(Props, Clone, PartialEq)]
pub struct ContactEditPageProps {
    pub id: String,
}

#[component]
pub fn ContactEditPage(props: ContactEditPageProps) -> Element {
    let id_for_resource = props.id.clone();
    let id_for_form = props.id.clone();
    let detail = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<ContactEditPayload>(&format!(
                "/contacts/contacts/{id}"
            ))
            .await
            .ok()
        }
    });
    let snap = detail.read_unchecked();
    rsx! {
        AppLayout { title: "Edit Contact",
            PageHeader { title: "Edit Contact" }
            match &*snap {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading contact..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contact." }
                            Link {
                                to: Route::ContactList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to contacts"
                            }
                        }
                    }
                },
                Some(Some(payload)) => {
                    let initial = ContactFormValues {
                        first_name: payload.first_name.clone(),
                        last_name: payload.last_name.clone(),
                        email: payload.email.clone().unwrap_or_default(),
                        phone: payload.phone.clone().unwrap_or_default(),
                        mobile: payload.mobile.clone().unwrap_or_default(),
                        title: payload.title.clone().unwrap_or_default(),
                        department: payload.department.clone().unwrap_or_default(),
                        contact_type: if payload.contact_type.is_empty() {
                            "other".to_string()
                        } else {
                            payload.contact_type.clone()
                        },
                        company_id: payload.company_id.to_string(),
                        company_name: payload.company_name.clone().unwrap_or_default(),
                    };
                    let id = id_for_form.clone();
                    rsx! {
                        ContactForm {
                            initial,
                            mode: ContactFormMode::Edit { id },
                        }
                    }
                },
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ContactEditPayload {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    department: Option<String>,
    #[serde(default)]
    contact_type: String,
    company_id: uuid::Uuid,
    #[serde(default)]
    company_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ContactFormValues {
    first_name: String,
    last_name: String,
    email: String,
    phone: String,
    mobile: String,
    title: String,
    department: String,
    contact_type: String,
    company_id: String,
    company_name: String,
}

#[derive(Clone, Debug, PartialEq)]
enum ContactFormMode {
    Create,
    Edit { id: String },
}

#[derive(Props, Clone, PartialEq)]
struct ContactFormProps {
    initial: ContactFormValues,
    mode: ContactFormMode,
}

#[component]
fn ContactForm(props: ContactFormProps) -> Element {
    let initial = props.initial.clone();
    let mode = props.mode.clone();
    let mut first_name = use_signal(|| initial.first_name.clone());
    let mut last_name = use_signal(|| initial.last_name.clone());
    let mut email = use_signal(|| initial.email.clone());
    let mut phone = use_signal(|| initial.phone.clone());
    let mut mobile = use_signal(|| initial.mobile.clone());
    let mut title = use_signal(|| initial.title.clone());
    let mut department = use_signal(|| initial.department.clone());
    let mut contact_type = use_signal(|| {
        if initial.contact_type.is_empty() {
            "other".to_string()
        } else {
            initial.contact_type.clone()
        }
    });
    let mut company_id = use_signal(|| initial.company_id.clone());
    let mut company_name = use_signal(|| initial.company_name.clone());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let type_options = vec![
        SelectOption::new("primary", "Primary"),
        SelectOption::new("technical", "Technical"),
        SelectOption::new("billing", "Billing"),
        SelectOption::new("other", "Other"),
    ];

    let navigator = use_navigator();
    let submit_label = match &mode {
        ContactFormMode::Create => "Create Contact",
        ContactFormMode::Edit { .. } => "Save Changes",
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        error.set(String::new());

        let parsed_company = uuid::Uuid::parse_str(company_id.read().as_str()).ok();
        let Some(company_uuid) = parsed_company else {
            error.set("Please pick a company first.".to_string());
            is_submitting.set(false);
            return;
        };

        let body = serde_json::json!({
            "company_id": company_uuid,
            "first_name": first_name.read().trim(),
            "last_name": last_name.read().trim(),
            "email": optional_string(&email.read()),
            "phone": optional_string(&phone.read()),
            "mobile": optional_string(&mobile.read()),
            "title": optional_string(&title.read()),
            "department": optional_string(&department.read()),
            "contact_type": contact_type.read().clone(),
        });
        let mode = mode.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct ContactId {
                    id: uuid::Uuid,
                }
                let result = match &mode {
                    ContactFormMode::Create => {
                        crate::hooks::fetch::api::post_authed::<ContactId, _>(
                            "/contacts/contacts",
                            &body,
                        )
                        .await
                        .map(|c| c.id.to_string())
                    }
                    ContactFormMode::Edit { id } => {
                        let path = format!("/contacts/contacts/{id}");
                        crate::hooks::fetch::api::put_authed::<ContactId, _>(&path, &body)
                            .await
                            .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        navigator.push(Route::ContactDetail { id });
                    }
                    Err(err) => {
                        error.set(format!("Could not save contact: {err}"));
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    let picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
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
                    crate::components::Input {
                        name: "email",
                        label: "Email",
                        r#type: "email",
                        value: email.read().clone(),
                        oninput: move |e: FormEvent| email.set(e.value()),
                    }
                    crate::components::Input {
                        name: "title",
                        label: "Title",
                        value: title.read().clone(),
                        oninput: move |e: FormEvent| title.set(e.value()),
                    }
                    crate::components::Input {
                        name: "phone",
                        label: "Phone",
                        value: phone.read().clone(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    crate::components::Input {
                        name: "mobile",
                        label: "Mobile",
                        value: mobile.read().clone(),
                        oninput: move |e: FormEvent| mobile.set(e.value()),
                    }
                    crate::components::Input {
                        name: "department",
                        label: "Department",
                        value: department.read().clone(),
                        oninput: move |e: FormEvent| department.set(e.value()),
                    }
                    Select {
                        name: "contact_type",
                        label: "Type",
                        options: type_options,
                        value: contact_type.read().clone(),
                        onchange: move |e: FormEvent| contact_type.set(e.value()),
                    }
                }

                crate::components::CompanyPicker {
                    value: company_name.read().clone(),
                    selected_id: picker_selected_id,
                    required: true,
                    onselect: move |(id, name): (String, String)| {
                        company_id.set(id);
                        company_name.set(name);
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                    },
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
                        "{submit_label}"
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
    let contact_id_str = props.id.clone();
    let id_for_resource = contact_id_str.clone();
    let id_for_tickets = contact_id_str.clone();
    let id_for_edit = contact_id_str.clone();
    let id_for_delete = contact_id_str.clone();
    let id_for_portal = contact_id_str.clone();

    let mut contact = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<ContactDetail>(&format!(
                "/contacts/contacts/{id}"
            ))
            .await
            .ok()
        }
    });
    let tickets = use_resource(move || {
        let id = id_for_tickets.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<PaginatedTicketSummaries>(&format!(
                "/tickets?contact_id={id}&per_page=5&sort=-updated_at"
            ))
            .await
            .ok()
        }
    });

    let snap = contact.read_unchecked();
    let header_title = match &*snap {
        Some(Some(c)) => format!("{} {}", c.first_name, c.last_name)
            .trim()
            .to_string(),
        _ => "Contact".to_string(),
    };

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let portal_toggling = use_signal(|| false);
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();

    rsx! {
        AppLayout { title: "{header_title}",
            PageHeader {
                title: "{header_title}",
                actions: rsx! {
                    Link {
                        to: Route::ContactEdit { id: edit_id },
                        Button { variant: ButtonVariant::Secondary, "Edit" }
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
                                                "Delete this contact? This cannot be undone.",
                                            )
                                            .ok()
                                        })
                                        .unwrap_or(false);
                                    if confirmed {
                                        let path = format!("/contacts/contacts/{id}");
                                        if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                                            navigator.push(Route::ContactList {});
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

            match &*snap {
                None => rsx! {
                    Card { div { class: "py-12 text-center text-sm text-gray-500", "Loading contact..." } }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-8 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contact." }
                            Link {
                                to: Route::ContactList {},
                                class: "text-sm text-blue-600 hover:text-blue-500",
                                "Back to contacts"
                            }
                        }
                    }
                },
                Some(Some(c)) => {
                    let company_id = c.company_id.to_string();
                    let company_name = c.company_name.clone().unwrap_or_default();
                    let email = c.email.clone();
                    let phone = c.phone.clone();
                    let mobile = c.mobile.clone();
                    let title = c.title.clone();
                    let department = c.department.clone();
                    let contact_type = c.contact_type.clone();
                    let is_portal_user = c.is_portal_user;
                    let portal_id = id_for_portal.clone();
                    rsx! {
                        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                            div { class: "lg:col-span-2 space-y-6",
                                ContactTicketsCard { tickets_resource: tickets }
                            }
                            div { class: "space-y-6",
                                Card { title: "Contact Information",
                                    dl { class: "space-y-4",
                                        if let Some(email) = email {
                                            if !email.is_empty() {
                                                div {
                                                    dt { class: "text-sm text-gray-500", "Email" }
                                                    dd { class: "mt-1",
                                                        a {
                                                            href: "mailto:{email}",
                                                            class: "text-blue-600 hover:text-blue-500",
                                                            "{email}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(phone) = phone {
                                            if !phone.is_empty() {
                                                div {
                                                    dt { class: "text-sm text-gray-500", "Phone" }
                                                    dd { class: "mt-1", "{phone}" }
                                                }
                                            }
                                        }
                                        if let Some(mobile) = mobile {
                                            if !mobile.is_empty() {
                                                div {
                                                    dt { class: "text-sm text-gray-500", "Mobile" }
                                                    dd { class: "mt-1", "{mobile}" }
                                                }
                                            }
                                        }
                                        if let Some(title) = title {
                                            if !title.is_empty() {
                                                div {
                                                    dt { class: "text-sm text-gray-500", "Title" }
                                                    dd { class: "mt-1", "{title}" }
                                                }
                                            }
                                        }
                                        if let Some(dept) = department {
                                            if !dept.is_empty() {
                                                div {
                                                    dt { class: "text-sm text-gray-500", "Department" }
                                                    dd { class: "mt-1", "{dept}" }
                                                }
                                            }
                                        }
                                        if !contact_type.is_empty() {
                                            div {
                                                dt { class: "text-sm text-gray-500", "Type" }
                                                dd { class: "mt-1",
                                                    Badge { variant: BadgeVariant::Blue, "{humanize_contact_type(&contact_type)}" }
                                                }
                                            }
                                        }
                                        if !company_name.is_empty() {
                                            div {
                                                dt { class: "text-sm text-gray-500", "Company" }
                                                dd { class: "mt-1",
                                                    Link {
                                                        to: Route::CompanyDetail { id: company_id.clone() },
                                                        class: "text-blue-600 hover:text-blue-500",
                                                        "{company_name}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                ContactPortalCard {
                                    contact_id: portal_id,
                                    is_portal_user,
                                    toggling: portal_toggling,
                                    on_change: move |_| { contact.restart(); },
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

fn humanize_contact_type(raw: &str) -> String {
    match raw {
        "primary" => "Primary".to_string(),
        "technical" => "Technical".to_string(),
        "billing" => "Billing".to_string(),
        "other" => "Other".to_string(),
        s => s.to_string(),
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ContactDetail {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    department: Option<String>,
    #[serde(default)]
    contact_type: String,
    #[serde(default)]
    is_portal_user: bool,
    company_id: uuid::Uuid,
    #[serde(default)]
    company_name: Option<String>,
}

#[component]
fn ContactTicketsCard(tickets_resource: Resource<Option<PaginatedTicketSummaries>>) -> Element {
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
                    Some(None) => rsx! {
                        TableEmpty { columns: 2, message: "Could not load tickets.".to_string() }
                    },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 2, message: "No tickets from this contact yet.".to_string() }
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

#[derive(Props, Clone, PartialEq)]
struct ContactPortalCardProps {
    contact_id: String,
    is_portal_user: bool,
    toggling: Signal<bool>,
    on_change: EventHandler<()>,
}

#[component]
fn ContactPortalCard(props: ContactPortalCardProps) -> Element {
    let contact_id = props.contact_id.clone();
    let is_portal_user = props.is_portal_user;
    let mut toggling = props.toggling;
    let on_change = props.on_change;
    rsx! {
        Card { title: "Portal Access",
            if is_portal_user {
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-gray-500", "Status" }
                        Badge { variant: BadgeVariant::Green, "Granted" }
                    }
                    p { class: "text-xs text-gray-500",
                        "This contact can sign in to the customer portal once a password has been issued from Settings > Portal Users."
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *toggling.read(),
                        onclick: move |_| {
                            let id = contact_id.clone();
                            toggling.set(true);
                            spawn(async move {
                                let path = format!("/contacts/contacts/{id}");
                                let body = serde_json::json!({ "is_portal_user": false });
                                let _ = crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await;
                                on_change.call(());
                                toggling.set(false);
                            });
                        },
                        "Revoke portal access"
                    }
                }
            } else {
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-gray-500", "Status" }
                        Badge { variant: BadgeVariant::Gray, "Not granted" }
                    }
                    p { class: "text-xs text-gray-500",
                        "Granting access flips the portal flag. A password still has to be issued separately from Settings > Portal Users before the contact can sign in."
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *toggling.read(),
                        onclick: move |_| {
                            let id = contact_id.clone();
                            toggling.set(true);
                            spawn(async move {
                                let path = format!("/contacts/contacts/{id}");
                                let body = serde_json::json!({ "is_portal_user": true });
                                let _ = crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await;
                                on_change.call(());
                                toggling.set(false);
                            });
                        },
                        "Grant portal access"
                    }
                }
            }
        }
    }
}

//! Contact and company pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    asset_status_badge, contract_status_badge, invoice_status_badge, project_status_badge,
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, CollapsibleCard, DataTable,
    ErrorBanner, IconSize, Modal, PageHeader, PlusIcon, SearchInput, Select, SelectOption,
    SortDirection, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow,
};
use crate::modules::contacts::Address;
use crate::utils::money::format_money_str;
use crate::utils::url::{safe_href, urlencoding_minimal};
use crate::utils::{FormGuard, Paginated};
use crate::Route;

/// Rows per page for the client-side paginated list views (F3).
const PER_PAGE: usize = 25;

/// PMS-581: US states / territories for the company Address State dropdown.
/// `(USPS code, name)`; the stored value is the code. The product is US-only,
/// so this is the canonical region list.
const US_STATES: &[(&str, &str)] = &[
    ("AL", "Alabama"),
    ("AK", "Alaska"),
    ("AZ", "Arizona"),
    ("AR", "Arkansas"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("DC", "District of Columbia"),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("IA", "Iowa"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("ME", "Maine"),
    ("MD", "Maryland"),
    ("MA", "Massachusetts"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MS", "Mississippi"),
    ("MO", "Missouri"),
    ("MT", "Montana"),
    ("NE", "Nebraska"),
    ("NV", "Nevada"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NY", "New York"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VT", "Vermont"),
    ("VA", "Virginia"),
    ("WA", "Washington"),
    ("WV", "West Virginia"),
    ("WI", "Wisconsin"),
    ("WY", "Wyoming"),
    ("AS", "American Samoa"),
    ("GU", "Guam"),
    ("MP", "Northern Mariana Islands"),
    ("PR", "Puerto Rico"),
    ("VI", "U.S. Virgin Islands"),
];

/// PMS-601: one row of the company-industry lookup, used to populate the
/// Industry combobox's suggestions from the tenant's editable list. The
/// canonical defaults now live server-side (seeded per tenant), not in a
/// frontend constant; the field stays free text for the long tail.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct IndustryOption {
    name: String,
    #[serde(default)]
    is_active: bool,
}

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

/// Map the server's lowercased `CompanyType` enum tag to the title-case label
/// that `CompanyRow` keys its badge variant on. Covers every variant of
/// `mokosh_types::contacts::CompanyType`; unknown values fall through
/// unchanged so future variants don't disappear.
fn humanize_company_type(raw: &str) -> String {
    match raw {
        "client" => "Client".to_string(),
        "prospect" => "Prospect".to_string(),
        "vendor" => "Vendor".to_string(),
        "partner" => "Partner".to_string(),
        // MAPPS-383: the tenant's own company (PMS-413). Without this arm it
        // rendered as the raw lowercase tag.
        "internal" => "Internal".to_string(),
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
/// `ContactResponse` shape (`phone`, `contact_type`); earlier names
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
    // PMS-368: the list "Role" column used to render `title` (a free-text job
    // title like "IT Manager"), which mismatched its header. `contact_type` is
    // the field that actually classifies the contact's role (Primary /
    // Technical / Billing / Other) and is what the column now binds. Server
    // serializes it snake_case on `ContactResponse`/`Contact`. `title` is
    // still shown on the contact detail page, which decodes its own struct.
    #[serde(default)]
    contact_type: Option<String>,
    /// MAPPS-456: whether the contact can sign in to the client portal.
    /// Server always sends this on `ContactResponse` (mokosh-types
    /// contacts::ContactResponse). Default false keeps decoding safe if
    /// an older server variant ever omits it.
    #[serde(default)]
    is_portal_user: bool,
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
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
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

    use_page_title("Companies");

    // MAPPS-357: `companies_resource` is this page's primary resource. It
    // stays a hand-rolled `use_resource` (rather than `use_remote_resource`)
    // because the page needs the loading / failed / `meta.total` distinction
    // from the `Option<PaginatedCompanies>` envelope (which is not `Default`).
    // A failed load while the server is flagged down is an outage, not an
    // empty account list: render the honest unavailable state instead of an
    // empty companies table. A 4xx while still reachable keeps the inline
    // banner below. There are no write controls on this page (New = nav,
    // Clear filters = filter reset), so nothing to gate with `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Companies".to_string() }
        };
    }

    rsx! {
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

        // MAPPS-388: de-boxed. Search + type controls sit directly on the
        // page; the surrounding Card was much larger than the controls it held.
        div { class: "mb-6",
            div { class: "flex flex-col sm:flex-row gap-4",
                div { class: "flex-1",
                    SearchInput {
                        value: search.read().clone(),
                        placeholder: "Search companies…",
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
            ErrorBanner { class: "mb-3", "Could not load companies. Refresh the page to retry." }
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
                striped: true,
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
                    if has_filters {
                        // Filtered to nothing: MAPPS-291 "Clear filters"
                        // affordance so the user does not have to find
                        // every control and reset each one to recover.
                        TableEmpty {
                            columns: 4,
                            title: "No companies match your filters".to_string(),
                            description: "Adjust the filters above, or clear them to see every company again.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        search.set(String::new());
                                        type_filter.set(String::new());
                                    },
                                    "Clear filters"
                                }
                            },
                        }
                    } else {
                        TableEmpty {
                            columns: 4,
                            title: "No companies yet".to_string(),
                            description: "Add your first company to start managing clients.".to_string(),
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
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.name}"
                }
            }
            TableCell {
                Badge { variant: type_variant, "{props.company_type}" }
            }
            TableCell { "{props.primary_contact}" }
            TableCell {
                if props.open_tickets > 0 {
                    span { class: "font-medium text-accent", "{props.open_tickets}" }
                } else {
                    span { class: "text-subtle", "0" }
                }
            }
        }
    }
}

/// New company page
#[component]
pub fn CompanyNewPage() -> Element {
    // MAPPS-357: N/A for a ContentUnavailable state - this page fetches no
    // primary entity (it is a blank create form). The one write control (the
    // Create submit) is disabled while the server is down inside `CompanyForm`,
    // which owns the button and is shared with the edit page.
    use_page_title("New Company");

    rsx! {
        PageHeader { title: "New Company", subtitle: "Add a new company account" }
        CompanyForm {
            initial: CompanyFormValues::default(),
            mode: CompanyFormMode::Create,
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
            // MAPPS-357: subscribe to reachability so the edited entity
            // auto-refetches once the server comes back.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<CompanyEditPayload>(&format!(
                "/contacts/companies/{id}"
            ))
            .await
            .ok()
        }
    });
    let snap = detail_resource.read_unchecked();
    use_page_title("Edit Company");
    // MAPPS-357: the fetched company is this edit page's primary resource. A
    // failed load while the server is flagged down is an outage, not a missing
    // record - render the honest unavailable state instead of "Could not load
    // company" (which is kept below for a 4xx while still reachable). The
    // Save submit is gated by `can_mutate` inside `CompanyForm`.
    let fetch_failed = matches!(*snap, Some(None));
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Edit Company".to_string() }
        };
    }
    rsx! {
        PageHeader { title: "Edit Company" }
        match &*snap {
            None => rsx! {
                crate::components::DetailSkeleton {} // PMS-353
            },
            Some(None) => rsx! {
                Card {
                    div { class: "py-8 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load company." }
                        Link {
                            to: Route::CompanyList {},
                            class: "text-sm text-accent hover:opacity-90",
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
    // PMS-601: industry suggestions come from the tenant's editable lookup
    // (Settings > Company Industries), not a hardcoded list. Active names only.
    let industry_options = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<IndustryOption>>(
            "/contacts/company-industries?per_page=200",
        )
        .await
        .ok()
        .map(|p| {
            p.data
                .into_iter()
                .filter(|o| o.is_active)
                .map(|o| o.name)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
    });
    let industry_suggestions = industry_options
        .read_unchecked()
        .clone()
        .unwrap_or_default();
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
    // Per-field inline validation errors (MAPPS-177, MAPPS-213, MAPPS-265).
    let mut name_err = use_signal(String::new);
    let mut website_err = use_signal(String::new);
    let mut phone_err = use_signal(String::new);
    let mut postal_err = use_signal(String::new);
    // PMS-581: inline errors for the previously-unvalidated address text fields.
    let mut line1_err = use_signal(String::new);
    let mut line2_err = use_signal(String::new);
    let mut city_err = use_signal(String::new);

    // MAPPS-292: install a `beforeunload` guard while the user has typed
    // anything into the form. The Company form has roughly ten fields so
    // a tab close after half-filling it lost everything silently; the
    // browser now prompts before discarding. The dirty signal compares
    // every field to its initial value so saving (which navigates away)
    // does not trigger the prompt because the in-progress submit hits
    // `is_submitting=true` AFTER the dirty check fires once; we suppress
    // it then.
    let initial_for_dirty = initial.clone();
    let dirty = use_memo(move || {
        if is_submitting() {
            return false;
        }
        let same_type_default =
            initial_for_dirty.company_type.is_empty() && *company_type.read() == "client";
        *name.read() != initial_for_dirty.name
            || (*company_type.read() != initial_for_dirty.company_type && !same_type_default)
            || *industry.read() != initial_for_dirty.industry
            || *website.read() != initial_for_dirty.website
            || *phone.read() != initial_for_dirty.phone
            || *line1.read() != initial_for_dirty.address_line1
            || *line2.read() != initial_for_dirty.address_line2
            || *city.read() != initial_for_dirty.address_city
            || *state.read() != initial_for_dirty.address_state
            || *postal.read() != initial_for_dirty.address_postal_code
            || *country.read() != initial_for_dirty.address_country
    });
    crate::hooks::use_unsaved_guard(dirty.into());

    // MAPPS-383: `CompanyType::Internal` is provisioned server-side for the
    // tenant's own company and is not user-selectable, but editing such a
    // company must not silently retype it. Preserve the current value as its
    // own option when it is outside the customer-facing list (same idiom as
    // `state_options` below).
    let type_options = {
        let current = company_type.read().clone();
        let mut opts = vec![
            SelectOption::new("client", "Client"),
            SelectOption::new("prospect", "Prospect"),
            SelectOption::new("vendor", "Vendor"),
            SelectOption::new("partner", "Partner"),
        ];
        if !current.is_empty() && !opts.iter().any(|o| o.value == current) {
            opts.push(SelectOption::new(
                current.clone(),
                humanize_company_type(&current),
            ));
        }
        opts
    };

    // PMS-581: US-state dropdown options. Leading blank = "no state". Preserve
    // any existing non-code value (legacy free text) as its own option so an
    // edit never silently drops it.
    let state_options = {
        let current = state.read().clone();
        let mut opts = vec![SelectOption::new("", "Select a state")];
        let known = US_STATES.iter().any(|(code, _)| *code == current);
        if !current.is_empty() && !known {
            opts.push(SelectOption::new(current.clone(), current.clone()));
        }
        for (code, name) in US_STATES {
            opts.push(SelectOption::new(*code, *name));
        }
        opts
    };

    // PMS-581: US-only country. Offer United States; preserve any existing
    // non-US value so editing a legacy company keeps it. A blank country is
    // sent as "US" at submit time (see handle_submit).
    let country_options = {
        let current = country.read().trim().to_string();
        let mut opts = vec![SelectOption::new("US", "United States")];
        if !current.is_empty() && current != "US" {
            opts.push(SelectOption::new(current.clone(), current.clone()));
        }
        opts
    };

    let navigator = use_navigator();
    // MAPPS-357: block the Create / Save submit while the server is
    // unreachable so a write cannot silently fail (edits are discarded, not
    // queued). Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();
    let submit_label = match &mode {
        CompanyFormMode::Create => "Create Company",
        CompanyFormMode::Edit { .. } => "Save Changes",
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(String::new());
        name_err.set(String::new());
        website_err.set(String::new());
        phone_err.set(String::new());
        postal_err.set(String::new());
        line1_err.set(String::new());
        line2_err.set(String::new());
        city_err.set(String::new());
        // PMS-518: validate the formatted/structured fields inline before submit
        // (MAPPS-177, MAPPS-213) and report ALL failures at once, each in its own
        // inline slot, then focus the first invalid field. The bespoke validators
        // parse-and-return the typed values the body uses, so they are kept; the
        // results are carried past the single bail and unwrapped once validation
        // passes.
        let mut guard = FormGuard::new();
        let name_res = validate_name_field(&name.read());
        if let Err(msg) = &name_res {
            name_err.set(msg.clone());
            guard.note_invalid(Some("name"));
        }
        let website_res = validate_website_field(&website.read());
        if let Err(msg) = &website_res {
            website_err.set(msg.clone());
            guard.note_invalid(Some("website"));
        }
        let phone_res = validate_phone_field(&phone.read(), "Phone");
        if let Err(msg) = &phone_res {
            phone_err.set(msg.clone());
            guard.note_invalid(Some("phone"));
        }
        let postal_res = validate_postal_field(&postal.read());
        if let Err(msg) = &postal_res {
            postal_err.set(msg.clone());
            guard.note_invalid(Some("address_postal_code"));
        }
        // PMS-581: bound the free-text address fields and reject control chars,
        // matching the inline-validation standard of the rest of the form.
        if let Err(msg) = validate_address_text(&line1.read(), "Street", 255) {
            line1_err.set(msg);
            guard.note_invalid(Some("address_line1"));
        }
        if let Err(msg) = validate_address_text(&line2.read(), "Street (line 2)", 255) {
            line2_err.set(msg);
            guard.note_invalid(Some("address_line2"));
        }
        if let Err(msg) = validate_address_text(&city.read(), "City", 100) {
            city_err.set(msg);
            guard.note_invalid(Some("address_city"));
        }
        if guard.blocked() {
            return;
        }
        let name_value = name_res.expect("name validated above");
        let website_value = website_res.expect("website validated above");
        let phone_value = phone_res.expect("phone validated above");
        let postal_value = postal_res.expect("postal validated above");
        // PMS-581: US-only. A blank country defaults to "US"; a preserved
        // legacy value passes through unchanged.
        let country_value = {
            let c = country.read().trim().to_string();
            if c.is_empty() {
                "US".to_string()
            } else {
                c
            }
        };
        is_submitting.set(true);
        let body = serde_json::json!({
            "name": name_value,
            "company_type": company_type.read().clone(),
            "industry": optional_string(&industry.read()),
            "website": website_value,
            "phone": phone_value,
            "address": {
                "line1": optional_string(&line1.read()),
                "line2": optional_string(&line2.read()),
                "city": optional_string(&city.read()),
                "state": optional_string(&state.read()),
                "postal_code": postal_value,
                "country": country_value,
            },
        });
        let mode = mode.clone();
        // MAPPS-293: clone the mode again for the post-success toast so the
        // outer `mode` is still available in case of an Err branch.
        let mode_for_toast = mode.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct CompanyId {
                    id: uuid::Uuid,
                }
                let result = match &mode {
                    CompanyFormMode::Create => crate::hooks::fetch::api::post_authed_typed::<
                        CompanyId,
                        _,
                    >("/contacts/companies", &body)
                    .await
                    .map(|c| c.id.to_string()),
                    CompanyFormMode::Edit { id } => {
                        let path = format!("/contacts/companies/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<CompanyId, _>(&path, &body)
                            .await
                            .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        // MAPPS-293: confirming success toast. The mode tells
                        // us whether the user created vs. saved an edit.
                        let msg = match mode_for_toast {
                            CompanyFormMode::Create => "Company created.",
                            CompanyFormMode::Edit { .. } => "Company saved.",
                        };
                        crate::hooks::toast::push_toast(crate::components::AlertType::Success, msg);
                        navigator.push(Route::CompanyDetail { id });
                    }
                    Err(err) => {
                        // MAPPS-246: a duplicate company name comes back as a
                        // 409 from the create/edit endpoint (b3 work). Route the
                        // server's conflict message onto the Company Name field
                        // inline instead of the generic banner. Detect it by
                        // status code when available, otherwise fall back to the
                        // conflict message text so the cue still lands if the
                        // helper ever drops the code. Either way the normal
                        // success / other-error paths stay intact.
                        let is_name_conflict = err.status_code() == Some(409)
                            || (err.status_code().is_none() && {
                                let m = err.user_message().to_ascii_lowercase();
                                m.contains("already")
                                    || m.contains("duplicate")
                                    || m.contains("taken")
                                    || m.contains("exists")
                            });
                        if is_name_conflict {
                            name_err.set(err.user_message());
                            is_submitting.set(false);
                            return;
                        }
                        // MAPPS-265: map every server-side field error from the
                        // 422 `errors[]` envelope back onto its own inline field
                        // (e.g. a Website scheme rule the client did not mirror,
                        // MAPPS-210 / MAPPS-213) so each cue persists after the
                        // failed submit. Unmatched fields, or a non-422 failure,
                        // fall back to the top-of-form banner.
                        let fields = err.field_errors();
                        if fields.is_empty() {
                            error.set(format!("Could not save company: {}", err.user_message()));
                        } else {
                            let mut leftover = Vec::new();
                            for fe in fields {
                                match fe.field.as_str() {
                                    "name" => name_err.set(fe.message.clone()),
                                    "website" => website_err.set(fe.message.clone()),
                                    "phone" => phone_err.set(fe.message.clone()),
                                    "postal_code" | "address.postal_code" => {
                                        postal_err.set(fe.message.clone())
                                    }
                                    _ => leftover.push(fe.message.clone()),
                                }
                            }
                            if !leftover.is_empty() {
                                error.set(format!(
                                    "Could not save company: {}",
                                    leftover.join("; ")
                                ));
                            }
                        }
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
                    ErrorBanner { "{error.read()}" }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "name",
                        label: "Company Name",
                        placeholder: "Enter company name",
                        required: true,
                        // Mirror the server cap (CreateCompanyRequest.name: max 255).
                        maxlength: 255,
                        value: name.read().clone(),
                        error: name_err(),
                        oninput: move |e: FormEvent| name.set(e.value()),
                    }
                    Select {
                        name: "type",
                        label: "Company Type",
                        options: type_options,
                        value: company_type.read().clone(),
                        onchange: move |e: FormEvent| company_type.set(e.value()),
                    }
                    crate::components::SuggestInput {
                        name: "industry",
                        label: "Industry",
                        suggestions: industry_suggestions.clone(),
                        help: "Pick a standard industry or type your own.",
                        value: industry.read().clone(),
                        oninput: move |v: String| industry.set(v),
                    }
                    crate::components::Input {
                        name: "website",
                        label: "Website",
                        placeholder: "https://example.com",
                        maxlength: 255,
                        value: website.read().clone(),
                        error: website_err(),
                        oninput: move |e: FormEvent| website.set(e.value()),
                    }
                    crate::components::Input {
                        name: "phone",
                        label: "Phone",
                        placeholder: "(555) 555-5555",
                        value: phone.read().clone(),
                        error: phone_err(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                }

                h3 { class: "text-sm font-medium text-content pt-2",
                    "Address"
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "address_line1",
                        label: "Street",
                        maxlength: 255,
                        value: line1.read().clone(),
                        error: line1_err(),
                        oninput: move |e: FormEvent| line1.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_line2",
                        label: "Street (line 2)",
                        maxlength: 255,
                        value: line2.read().clone(),
                        error: line2_err(),
                        oninput: move |e: FormEvent| line2.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_city",
                        label: "City",
                        maxlength: 100,
                        value: city.read().clone(),
                        error: city_err(),
                        oninput: move |e: FormEvent| city.set(e.value()),
                    }
                    Select {
                        name: "address_state",
                        label: "State / Region",
                        options: state_options,
                        value: state.read().clone(),
                        onchange: move |e: FormEvent| state.set(e.value()),
                    }
                    crate::components::Input {
                        name: "address_postal_code",
                        label: "Postal Code",
                        // Matches the client postal rule (max 12 chars).
                        maxlength: 12,
                        value: postal.read().clone(),
                        error: postal_err(),
                        oninput: move |e: FormEvent| postal.set(e.value()),
                    }
                    Select {
                        name: "address_country",
                        label: "Country",
                        // PMS-581: US-only. A blank value displays/saves as US.
                        options: country_options,
                        value: country.read().clone(),
                        onchange: move |e: FormEvent| country.set(e.value()),
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
                        // MAPPS-357: block the submit while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
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

// ---- MAPPS-177: client-side field validation, mirroring the PMS-325 server
// rules so the user gets immediate per-field feedback instead of a server error.

/// Validate an optional phone field. Blank -> `Ok(None)`. Otherwise strips
/// common formatting (spaces, dashes, parens, dots) and requires E.164: an
/// optional leading `+` then 2-15 digits, the first 1-9. Returns the normalized
/// value or an inline error message. `label` names the field in the message.
/// Validate the required company name, mirroring the server's
/// `validate_company_name` rule (PMS / b3 work): trim, reject empty or
/// whitespace-only input, and reject control characters. On success returns the
/// trimmed name so the client surfaces the "Company name is required" cue inline
/// (MAPPS-246) before any request instead of bouncing off an opaque server 422.
fn validate_name_field(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Company name is required.".to_string());
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err("Company name must not contain control characters.".to_string());
    }
    Ok(trimmed.to_string())
}

/// MAPPS-283: render a stored canonical phone number with readable
/// separators so contacts and companies don't display as a raw run of
/// digits. The server stores phone numbers as the trimmed-formatting
/// output of [`validate_phone_field`] (digits with an optional leading
/// `+`); this helper reverses that for the read surface.
///
/// Rules: a 10-digit US number renders as `(555) 123-4567`; an 11-digit
/// number starting with `1` renders as `+1 (555) 123-4567`; anything
/// else (E.164 with a non-US country code, partial entry, legacy
/// payloads with extensions) passes through unchanged so we never
/// mangle a non-NANP number into a US shape. Empty input renders as the
/// empty string so callers can chain `.unwrap_or_default()`.
pub fn format_phone(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        10 => format!("({}) {}-{}", &digits[0..3], &digits[3..6], &digits[6..10]),
        11 if digits.starts_with('1') => format!(
            "+1 ({}) {}-{}",
            &digits[1..4],
            &digits[4..7],
            &digits[7..11]
        ),
        _ => trimmed.to_string(),
    }
}

fn validate_phone_field(raw: &str, label: &str) -> Result<serde_json::Value, String> {
    let normalized: String = raw
        .chars()
        .filter(|c| !matches!(c, ' ' | '\t' | '\u{00A0}' | '-' | '(' | ')' | '.'))
        .collect();
    if normalized.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    let digits = normalized.strip_prefix('+').unwrap_or(&normalized);
    let valid = (2..=15).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits
            .as_bytes()
            .first()
            .is_some_and(|&b| (b'1'..=b'9').contains(&b));
    if valid {
        Ok(serde_json::Value::String(normalized))
    } else {
        Err(format!(
            "{label} must be a valid phone number (e.g. +14155551234)."
        ))
    }
}

/// Validate an optional ISO 3166-1 alpha-2 country code. Blank -> `Ok(None)`.
/// Requires exactly two ASCII letters (normalized to uppercase). The server
/// (PMS-325) checks membership against the official set.
fn validate_country_field(raw: &str) -> Result<serde_json::Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    if trimmed.len() == 2 && trimmed.bytes().all(|b| b.is_ascii_alphabetic()) {
        Ok(serde_json::Value::String(trimmed.to_ascii_uppercase()))
    } else {
        Err("Country must be a 2-letter ISO code (e.g. US).".to_string())
    }
}

/// PMS-581: validate an optional free-text address field. Blank is accepted.
/// Otherwise it must be within `max` characters and free of control characters
/// (which includes NUL, rejected by Postgres anyway). Gating only - the body
/// keeps sending the `optional_string` form.
fn validate_address_text(raw: &str, label: &str, max: usize) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.chars().count() > max {
        return Err(format!("{label} must be {max} characters or fewer."));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(format!("{label} contains invalid characters."));
    }
    Ok(())
}

/// Validate an optional postal code. Blank -> `Ok(None)`. Otherwise 2-12
/// characters of letters, digits, spaces, or hyphens.
fn validate_postal_field(raw: &str) -> Result<serde_json::Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    let len_ok = (2..=12).contains(&trimmed.chars().count());
    let charset_ok = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-');
    if len_ok && charset_ok {
        Ok(serde_json::Value::String(trimmed.to_string()))
    } else {
        Err("Postal code must be 2-12 letters, digits, spaces, or hyphens.".to_string())
    }
}

/// Validate an optional Website URL. Blank -> `Ok(None)`. Otherwise the value
/// must carry an explicit `http`/`https` scheme and a non-empty host. Dangerous
/// schemes (`javascript:`, `data:`, `vbscript:`, anything else) and malformed
/// URLs are rejected with an inline message *before* any request, so the user
/// learns Website is the problem instead of hitting an opaque server 422
/// (MAPPS-213). The scheme check reuses `utils::url::scheme_of`, the same
/// whitespace-collapsing detection `safe_href` applies at render time, so
/// `java\tscript:` cannot slip through.
fn validate_website_field(raw: &str) -> Result<serde_json::Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    const MSG: &str = "Website must be a valid http(s) URL (e.g. https://example.com).";
    // Reject anything but an explicit http/https scheme (covers javascript:,
    // data:, vbscript:, mailto:, scheme-less input, ...).
    match crate::utils::url::scheme_of(trimmed).as_deref() {
        Some("http") | Some("https") => {}
        _ => return Err(MSG.to_string()),
    }
    // Require `scheme://host` with a non-empty host and no embedded whitespace
    // so `http://`, `http:/x`, and `https://exa mple.com` are rejected.
    let host = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    if host.is_empty() || trimmed.chars().any(|c| c.is_whitespace()) {
        return Err(MSG.to_string());
    }
    Ok(serde_json::Value::String(trimmed.to_string()))
}

/// Validate an optional IANA time zone. Blank -> `Ok(None)`. A light client
/// check (must look like `Area/Location` with no spaces) that catches the
/// common `America/New York` mistake; the server (PMS-325) is authoritative.
fn validate_timezone_field(raw: &str) -> Result<serde_json::Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    if !trimmed.contains(' ') && trimmed.contains('/') {
        Ok(serde_json::Value::String(trimmed.to_string()))
    } else {
        Err("Time zone must be an IANA name (e.g. America/New_York).".to_string())
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
    let company_id_for_contracts = company_id_str.clone();
    let company_id_for_projects = company_id_str.clone();
    let company_id_for_invoices = company_id_str.clone();
    let company_id_for_assets = company_id_str.clone();
    let company_id_for_edit = company_id_str.clone();
    let company_id_for_delete = company_id_str.clone();

    // MAPPS-357: the company record is this detail page's primary resource
    // (the child-list resources below are secondary and keep degrading to
    // their own empty/error cards). Subscribe to reachability so it
    // auto-refetches on reconnect.
    let company_resource = use_resource(move || {
        let id = company_id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
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
            // MAPPS-247: cap the preview fetch (was uncapped) so a company
            // with many contacts no longer pulls every row inline. The card
            // shows the first 5 in a collapsible and `meta.total` reports the
            // full count.
            crate::hooks::fetch::api::get_authed::<PaginatedContacts>(&format!(
                "/contacts/companies/{id}/contacts?per_page=5"
            ))
            .await
            .ok()
        }
    });
    let sites_resource = use_resource(move || {
        let id = company_id_for_sites.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-316: fetch every site for the company. The 5-row
            // preview cap from MAPPS-247 combined with the 3-row
            // render cap below hid sites past the third one with no
            // "View all" link to escape to (unlike Contacts /
            // Tickets / Contracts on the same page). Sites per
            // company are small in practice (typical 1-20), so
            // dropping the cap to a high ceiling is the right
            // shape: every site renders inline, no separate list
            // page needed.
            crate::hooks::fetch::api::get_authed::<PaginatedSites>(&format!(
                "/contacts/companies/{id}/sites?per_page=200"
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
    // MAPPS-195: surface the company's other first-class relationships
    // (contracts, projects, invoices, assets) alongside contacts/sites/tickets.
    // Each reuses the module's existing `company_id` list filter; `meta.total`
    // from the same envelope feeds the Statistics counts so no extra count
    // endpoints are needed.
    let contracts_resource = use_resource(move || {
        let id = company_id_for_contracts.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<ContractSummary>>(&format!(
                "/contracts?company_id={id}&per_page=5"
            ))
            .await
            .ok()
        }
    });
    let projects_resource = use_resource(move || {
        let id = company_id_for_projects.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<ProjectSummary>>(&format!(
                "/projects?company_id={id}&per_page=5"
            ))
            .await
            .ok()
        }
    });
    let invoices_resource = use_resource(move || {
        let id = company_id_for_invoices.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<InvoiceSummary>>(&format!(
                "/invoices?company_id={id}&per_page=5"
            ))
            .await
            .ok()
        }
    });
    let assets_resource = use_resource(move || {
        let id = company_id_for_assets.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<Paginated<AssetSummary>>(&format!(
                "/assets?company_id={id}&per_page=5"
            ))
            .await
            .ok()
        }
    });
    // Asset rows carry only `asset_type_id`; load the type list once to render
    // a human-readable type name in the Assets card.
    let asset_types_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeOption>>(
            "/asset-types?per_page=100",
        )
        .await
        .ok()
    });

    // Statistics counts pulled from each list envelope's `meta.total`.
    let contract_count = paginated_total(&contracts_resource);
    let project_count = paginated_total(&projects_resource);
    let invoice_count = paginated_total(&invoices_resource);
    let asset_count = paginated_total(&assets_resource);

    let company_snapshot = company_resource.read_unchecked();
    // MAPPS-278: while the record is loading, show "Loading..." instead
    // of the generic entity type ("Company"). The previous fallback
    // briefly flashed a generic header until the resource resolved,
    // which read as the wrong company before settling. A loading label
    // is honest about the state.
    let header_title = match &*company_snapshot {
        Some(Some(c)) => c.name.clone(),
        None => "Loading…".to_string(),
        Some(None) => "Company not found".to_string(),
    };
    use_page_title(&header_title);

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let edit_id = company_id_for_edit.clone();
    let delete_id = company_id_for_delete.clone();
    let mut confirming_delete = use_signal(|| false);
    // MAPPS-357: gate the destructive Delete while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contacts/companies/{id}");
                if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                    navigator.push(Route::CompanyList {});
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    // MAPPS-357: a failed load of the primary company while the server is
    // flagged down is an outage, not a missing record - render the honest
    // unavailable state instead of "Could not load company" (kept below for a
    // 4xx while still reachable).
    let fetch_failed = matches!(*company_snapshot, Some(None));
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Company".to_string() }
        };
    }

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete company".to_string(),
            message: "Delete this company? This will also remove its sites and unlink its contacts/tickets.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            // PMS-369: this delete cascades (removes sites, unlinks
            // contacts/tickets), so gate it behind typing the company name.
            confirm_phrase: header_title.clone(),
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
        PageHeader {
            title: "{header_title}",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: vec![
                        crate::components::BreadcrumbItem {
                            label: "Companies".to_string(),
                            route: Some(Route::CompanyList {}),
                        },
                        crate::components::BreadcrumbItem {
                            label: header_title.clone(),
                            route: None,
                        },
                    ],
                }
            },
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
                    // MAPPS-357: block delete while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        if !*deleting.read() {
                            confirming_delete.set(true);
                        }
                    },
                    "Delete"
                }
            },
        }

        match &*company_snapshot {
            None => rsx! {
                crate::components::DetailSkeleton {} // PMS-353
            },
            Some(None) => rsx! {
                Card {
                    div {
                        class: "py-8 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load company." }
                        Link {
                            to: Route::CompanyList {},
                            class: "text-sm text-accent hover:opacity-90",
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
                            // MAPPS-456: company-level entry point for
                            // enabling portal access on multiple contacts
                            // from one screen (previously required drilling
                            // into each contact detail page). Fetches its
                            // own uncapped list so the roster reflects
                            // every contact, not just the first 5 the
                            // Contacts card shows.
                            CompanyPortalAccessCard {
                                company_id: company_id_str.clone(),
                            }
                            // Sites
                            CompanySitesCard {
                                company_id: company_id_str.clone(),
                                sites_resource,
                                company_resource,
                            }
                            // Recent tickets
                            CompanyTicketsCard {
                                company_id: company_id_str.clone(),
                                company_name: company.name.clone(),
                                tickets_resource,
                            }
                            // Contracts (MAPPS-195)
                            CompanyContractsCard {
                                company_id: company_id_str.clone(),
                                contracts_resource,
                            }
                            // Projects (MAPPS-195)
                            CompanyProjectsCard {
                                company_id: company_id_str.clone(),
                                projects_resource,
                            }
                            // Invoices (MAPPS-195)
                            CompanyInvoicesCard {
                                company_id: company_id_str.clone(),
                                invoices_resource,
                            }
                            // PMS-730: request forms sent to this client,
                            // and the control to send another. Self-fetching,
                            // so it needs no resource threaded from here.
                            crate::pages::request_links::CompanyRequestFormsCard {
                                company_id: company_id_str.clone(),
                                company_name: company.name.clone(),
                            }
                            // Assets (MAPPS-195)
                            CompanyAssetsCard {
                                company_id: company_id_str.clone(),
                                assets_resource,
                                asset_types_resource,
                            }
                        }
                        // Sidebar
                        div { class: "space-y-6",
                            Card { title: "Details",
                                dl { class: "space-y-4",
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Type" }
                                        dd { Badge { variant: BadgeVariant::Green, "{type_label}" } }
                                    }
                                    if let Some(industry) = industry {
                                        if !industry.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-sm text-muted", "Industry" }
                                                dd { class: "text-sm", "{industry}" }
                                            }
                                        }
                                    }
                                    if let Some(phone) = phone {
                                        if !phone.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-sm text-muted", "Phone" }
                                                // MAPPS-283: render with separators.
                                                dd { class: "text-sm", {format_phone(&phone)} }
                                            }
                                        }
                                    }
                                    if let Some(website) = website {
                                        if !website.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-sm text-muted", "Website" }
                                                dd {
                                                    // Only render a live link when the value carries a
                                                    // safe URL scheme; `javascript:`/`data:`/`vbscript:`
                                                    // values fall back to plain text (MAPPS-149).
                                                    if let Some(href) = safe_href(&website) {
                                                        a {
                                                            href: "{href}",
                                                            target: "_blank",
                                                            rel: "noopener noreferrer",
                                                            class: "text-sm text-accent hover:opacity-90",
                                                            "{website}"
                                                        }
                                                    } else {
                                                        span { class: "text-sm", "{website}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(am) = am_name {
                                        if !am.is_empty() {
                                            div { class: "flex justify-between",
                                                dt { class: "text-sm text-muted", "Account Manager" }
                                                dd { class: "text-sm", "{am}" }
                                            }
                                        }
                                    }
                                    if !address_parts.is_empty() {
                                        div {
                                            dt { class: "text-sm text-muted mb-1", "Address" }
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
                                        span { class: "text-sm text-muted", "Open Tickets" }
                                        span { class: "font-medium text-content", "{open_tickets}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Contacts" }
                                        span { class: "font-medium", "{contact_count}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Sites" }
                                        span { class: "font-medium", "{site_count}" }
                                    }
                                    // MAPPS-195: counts for the newly surfaced relationships.
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Contracts" }
                                        span { class: "font-medium", "{contract_count}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Projects" }
                                        span { class: "font-medium", "{project_count}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Invoices" }
                                        span { class: "font-medium", "{invoice_count}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-sm text-muted", "Assets" }
                                        span { class: "font-medium", "{asset_count}" }
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
    // MAPPS-247: capped preview fetch carries the full count in `meta.total`
    // so the collapsible Sites card can show how many sites exist.
    #[serde(default)]
    meta: PaginationMeta,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedTicketSummaries {
    data: Vec<TicketSummary>,
    // MAPPS-249: the capped preview fetch carries the full count in
    // `meta.total` so the tickets card can show the same count badge as the
    // other collapsible relationship cards.
    #[serde(default)]
    meta: PaginationMeta,
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

/// MAPPS-249: per-row hover actions for the company context cards.
///
/// A three-dot (`⋯`) trigger that stays hidden until the row is hovered (the
/// row carries the Tailwind `group` class) and opens a small dropdown offering
/// Edit and Delete. Edit defers to the caller (`on_edit`) so each card can
/// navigate to its module's edit surface or open its own modal; Delete routes
/// through the shared `ConfirmDialog`, calls the module's DELETE endpoint, and
/// on success fires `on_deleted` so the card can restart its resource.
///
/// This is a focused row-level companion to `OverflowActions`: that component
/// collapses *header* action clusters at the `sm` breakpoint and would render
/// always-visible inline buttons on desktop, which is the opposite of the
/// hover-revealed row menu this card design calls for.
#[component]
fn RowActions(
    on_edit: EventHandler<()>,
    /// API path to DELETE, e.g. `"/tickets/{id}"`.
    delete_path: String,
    /// Lower-case singular noun for the confirm copy, e.g. `"ticket"`.
    delete_label: String,
    /// Fired after a successful delete so the caller can refresh its resource.
    on_deleted: EventHandler<()>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut confirming = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    // MAPPS-357: gate the row Delete while the server is unreachable. Edit is
    // pure navigation and stays enabled.
    let can_mutate = crate::hooks::use_can_mutate();

    // Keep the trigger visible while its menu is open; otherwise reveal it only
    // on row hover (or keyboard focus within the cell, for accessibility).
    let trigger_class = if open() {
        "opacity-100"
    } else {
        "opacity-0 group-hover:opacity-100 focus-within:opacity-100"
    };

    let path = delete_path.clone();
    let on_confirm_delete = move |_: ()| {
        if deleting() {
            return;
        }
        deleting.set(true);
        let path = path.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                    confirming.set(false);
                    open.set(false);
                    on_deleted.call(());
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        div { class: "relative flex justify-end transition-opacity {trigger_class}",
            button {
                r#type: "button",
                class: "px-2 py-1 text-muted hover:text-content rounded",
                title: "Actions",
                aria_label: "Row actions",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    open.toggle();
                },
                "\u{22EF}"
            }
            if open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        open.set(false);
                    },
                }
                div { class: "absolute right-0 top-full z-50 mt-1 w-32 rounded-md bg-raised shadow-lg ring-1 ring-black/5 py-1 flex flex-col",
                    button {
                        r#type: "button",
                        class: "px-3 py-1.5 text-left text-sm text-content hover:bg-surface-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            open.set(false);
                            on_edit.call(());
                        },
                        "Edit"
                    }
                    button {
                        r#type: "button",
                        class: "px-3 py-1.5 text-left text-sm text-red-600 dark:text-red-400 hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed",
                        // MAPPS-357: block delete while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            open.set(false);
                            confirming.set(true);
                        },
                        "Delete"
                    }
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming(),
            title: format!("Delete {delete_label}"),
            message: format!("Delete this {delete_label}? This cannot be undone."),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: deleting(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !deleting() {
                    confirming.set(false);
                }
            },
        }
    }
}

#[component]
fn CompanyContactsCard(
    company_id: String,
    company_name: String,
    mut contacts_resource: Resource<Option<PaginatedContacts>>,
) -> Element {
    let snap = contacts_resource.read_unchecked();
    // MAPPS-247: full count from the capped preview envelope feeds the
    // collapsible header badge.
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    // MAPPS-207: "Add Contact" now opens a picker that can attach an
    // *existing* contact to this company (search/select), with create-new
    // still offered inside the same modal.
    let mut show_add = use_signal(|| false);
    let navigator = use_navigator();
    // MAPPS-249: "View All" stays inside this company by carrying its id to the
    // scoped contact list (a plain anchor, matching the file's existing
    // query-param navigation pattern).
    let view_all_href = format!("/contacts?company_id={}", urlencoding_minimal(&company_id));
    rsx! {
        CollapsibleCard {
            title: "Contacts",
            count,
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Link,
                    onclick: move |_| show_add.set(true),
                    "Add Contact"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
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
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 5, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 5, message: "Could not load contacts.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 5, message: "No contacts at this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        // MAPPS-249: cap the preview at three rows; "View All" is
                        // the path to the full set.
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for contact in rows.into_iter() {
                                    {
                                        let id = contact.id.to_string();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/contacts/contacts/{id}");
                                        let name = format!("{} {}", contact.first_name, contact.last_name).trim().to_string();
                                        let email = contact.email.clone().unwrap_or_default();
                                        // MAPPS-283: render with separators.
                                        let phone = format_phone(&contact.phone.clone().unwrap_or_default());
                                        let role = humanize_contact_type(
                                            contact.contact_type.as_deref().unwrap_or_default(),
                                        );
                                        rsx! {
                                            TableRow { key: "{id}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::ContactDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { "{email}" }
                                                TableCell { "{phone}" }
                                                TableCell { "{role}" }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::ContactEdit { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "contact".to_string(),
                                                        on_deleted: move |_| { contacts_resource.restart(); },
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

        if show_add() {
            AddContactModal {
                company_id: company_id.clone(),
                company_name: company_name.clone(),
                onclose: move |_| { show_add.set(false); },
                onsaved: move |_| {
                    show_add.set(false);
                    contacts_resource.restart();
                },
            }
        }
    }
}

/// MAPPS-456: company-level entry point for granting / revoking client-portal
/// access. Lists every contact under the company with a per-row toggle so an
/// admin onboarding a customer with several employees does not need to drill
/// into each contact detail page in turn. Both grant and revoke fire the same
/// `PUT /contacts/contacts/{id}` the per-contact card at
/// [`ContactPortalCard`] uses (`{"is_portal_user": bool}`); no new endpoints.
///
/// Fetches its own uncapped roster (per_page=200) rather than sharing the
/// [`CompanyContactsCard`] resource, which is capped at 5 for preview purposes.
/// Companies with more than ~20 contacts are rare; the ceiling is a soft cap
/// to keep the roster inline.
#[component]
fn CompanyPortalAccessCard(company_id: String) -> Element {
    let id_for_resource = company_id.clone();
    let mut roster = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<PaginatedContacts>(&format!(
                "/contacts/companies/{id}/contacts?per_page=200"
            ))
            .await
            .ok()
        }
    });
    let can_mutate = crate::hooks::use_can_mutate();
    // Per-row spinner tracking: whichever contact is mid-toggle disables its
    // own button rather than freezing the whole card. Uuid keys the map.
    let mut toggling: Signal<std::collections::HashSet<uuid::Uuid>> =
        use_signal(std::collections::HashSet::new);
    let snap = roster.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    rsx! {
        CollapsibleCard {
            title: "Portal Access",
            count,
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Contact" }
                        TableHeader { "Email" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Action" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load contacts.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No contacts at this company yet. Add one to grant portal access.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows: Vec<_> = page.data.iter().cloned().collect();
                        rsx! {
                            TableBody {
                                for contact in rows.into_iter() {
                                    {
                                        let contact_id = contact.id;
                                        let contact_id_str = contact_id.to_string();
                                        let name = format!("{} {}", contact.first_name, contact.last_name).trim().to_string();
                                        let email = contact.email.clone().unwrap_or_default();
                                        let has_email = !email.trim().is_empty();
                                        let is_portal_user = contact.is_portal_user;
                                        let is_toggling = toggling.read().contains(&contact_id);
                                        rsx! {
                                            TableRow { key: "{contact_id_str}",
                                                TableCell {
                                                    Link {
                                                        to: Route::ContactDetail { id: contact_id_str.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { "{email}" }
                                                TableCell {
                                                    if is_portal_user {
                                                        Badge { variant: BadgeVariant::Green, "Granted" }
                                                    } else {
                                                        Badge { variant: BadgeVariant::Gray, "Not granted" }
                                                    }
                                                }
                                                TableCell { class: "text-right w-32",
                                                    if is_portal_user {
                                                        Button {
                                                            variant: ButtonVariant::Secondary,
                                                            loading: is_toggling,
                                                            disabled: is_toggling || !can_mutate,
                                                            onclick: move |_| {
                                                                let path = format!("/contacts/contacts/{contact_id_str}");
                                                                toggling.write().insert(contact_id);
                                                                spawn(async move {
                                                                    let body = serde_json::json!({ "is_portal_user": false });
                                                                    match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await {
                                                                        Ok(_) => {
                                                                            crate::hooks::toast::push_toast(
                                                                                crate::components::AlertType::Success,
                                                                                "Portal access revoked.",
                                                                            );
                                                                            roster.restart();
                                                                        }
                                                                        Err(err) => crate::hooks::toast::push_toast(
                                                                            crate::components::AlertType::Error,
                                                                            format!("Could not revoke portal access: {err}"),
                                                                        ),
                                                                    }
                                                                    toggling.write().remove(&contact_id);
                                                                });
                                                            },
                                                            "Revoke"
                                                        }
                                                    } else if !has_email {
                                                        // Mirror the per-contact card's guard: no email = no
                                                        // portal grant, because the setup link has nowhere to go.
                                                        Button {
                                                            variant: ButtonVariant::Secondary,
                                                            disabled: true,
                                                            "Grant"
                                                        }
                                                    } else {
                                                        Button {
                                                            variant: ButtonVariant::Primary,
                                                            loading: is_toggling,
                                                            disabled: is_toggling || !can_mutate,
                                                            onclick: move |_| {
                                                                let path = format!("/contacts/contacts/{contact_id_str}");
                                                                toggling.write().insert(contact_id);
                                                                spawn(async move {
                                                                    let body = serde_json::json!({ "is_portal_user": true });
                                                                    match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await {
                                                                        Ok(_) => {
                                                                            crate::hooks::toast::push_toast(
                                                                                crate::components::AlertType::Success,
                                                                                "Portal access granted. A setup email is on its way.",
                                                                            );
                                                                            roster.restart();
                                                                        }
                                                                        Err(err) => crate::hooks::toast::push_toast(
                                                                            crate::components::AlertType::Error,
                                                                            format!("Could not grant portal access: {err}"),
                                                                        ),
                                                                    }
                                                                    toggling.write().remove(&contact_id);
                                                                });
                                                            },
                                                            "Grant"
                                                        }
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

/// MAPPS-207: "Add Contact" modal for a company. Lets the user search and
/// select an existing contact (attaching it to this company via a PUT that
/// sets `company_id`), or fall through to the full new-contact form with
/// the company pre-filled.
#[component]
fn AddContactModal(
    company_id: String,
    company_name: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let mut selected_id = use_signal(String::new);
    let mut selected_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-357: block the attach write while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    let new_href = format!(
        "/contacts/new?company_id={}&company_name={}",
        urlencoding_minimal(&company_id),
        urlencoding_minimal(&company_name)
    );

    let picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(selected_id.read().as_str()).is_ok() {
            Some(selected_id.read().clone())
        } else {
            None
        };

    let company_id_for_attach = company_id.clone();
    let on_attach = move |_| {
        let Ok(contact_uuid) = uuid::Uuid::parse_str(selected_id.read().as_str()) else {
            error.set("Pick a contact to attach.".to_string());
            return;
        };
        let Ok(company_uuid) = uuid::Uuid::parse_str(company_id_for_attach.as_str()) else {
            error.set("Invalid company.".to_string());
            return;
        };
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                // The server's `UpdateContactRequest` accepts a bare
                // `company_id`; every other field is optional, so a
                // single-field PUT re-points the contact at this company.
                let body = serde_json::json!({ "company_id": company_uuid });
                let path = format!("/contacts/contacts/{contact_uuid}");
                #[derive(serde::Deserialize)]
                struct ContactId {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::put_authed::<ContactId, _>(&path, &body).await {
                    Ok(_) => {
                        onsaved.call(());
                    }
                    Err(err) => {
                        error.set(format!("Could not attach contact: {err}"));
                    }
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Add Contact".to_string(),
            onclose: move |_| {
                if !saving() {
                    onclose.call(());
                }
            },
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| {
                        if !saving() {
                            onclose.call(());
                        }
                    },
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: saving(),
                    // MAPPS-357: block attach while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't attach while the server is unreachable".to_string()),
                    onclick: on_attach,
                    "Attach Contact"
                }
            },
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                crate::components::ContactPicker {
                    value: selected_name.read().clone(),
                    selected_id: picker_selected_id,
                    label: "Existing contact".to_string(),
                    onselect: move |(id, name): (String, String)| {
                        selected_id.set(id);
                        selected_name.set(name);
                    },
                    onclear: move |_| {
                        selected_id.set(String::new());
                        selected_name.set(String::new());
                    },
                }
                p { class: "text-xs text-muted",
                    "Search for a contact to attach to this company. Attaching moves the contact to this company."
                }
                div { class: "border-t border-line pt-3",
                    a {
                        href: "{new_href}",
                        class: "text-sm text-accent hover:opacity-90",
                        "+ Create a new contact instead"
                    }
                }
            }
        }
    }
}

#[component]
fn CompanySitesCard(
    company_id: String,
    mut sites_resource: Resource<Option<PaginatedSites>>,
    // The Statistics counters (Sites, Contacts, Open Tickets) read denormalized
    // counts off `company_resource`, not the child table resources. Restart it
    // after an add so the Sites counter refreshes in the same render cycle
    // instead of staying stale until a manual reload (PMS-363).
    mut company_resource: Resource<Option<CompanyDetail>>,
) -> Element {
    let snap = sites_resource.read_unchecked();
    // MAPPS-247: full count from the capped preview envelope feeds the
    // collapsible header badge.
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let mut editing = use_signal(|| None::<SiteFormState>);

    rsx! {
        CollapsibleCard {
            title: "Sites",
            count,
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Link,
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
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 2 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load sites.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No sites for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        // MAPPS-316: render every site the fetch
                        // returned. Sites per company are small;
                        // the previous `.take(3)` capped the user
                        // out of seeing the rest because Sites has
                        // no "View all" escape link.
                        let rows: Vec<_> = page.data.clone();
                        let company_id = company_id.clone();
                        rsx! {
                            TableBody {
                                for site in rows.into_iter() {
                                    {
                                        let key = site.id.to_string();
                                        let delete_path = format!("/contacts/sites/{key}");
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
                                        let site_for_actions = site.clone();
                                        let company_id_for_actions = company_id.clone();
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    Button {
                                                        variant: ButtonVariant::Link,
                                                        class: "text-left".to_string(),
                                                        onclick: move |_| {
                                                            editing.set(Some(SiteFormState::from_existing(
                                                                &company_id_for_edit,
                                                                &site_for_edit,
                                                            )));
                                                        },
                                                        "{site.name}"
                                                    }
                                                }
                                                TableCell { class: "text-muted", "{addr}" }
                                                TableCell {
                                                    if is_primary {
                                                        Badge { variant: BadgeVariant::Blue, "Primary" }
                                                    }
                                                }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| {
                                                            editing.set(Some(SiteFormState::from_existing(
                                                                &company_id_for_actions,
                                                                &site_for_actions,
                                                            )));
                                                        },
                                                        delete_path,
                                                        delete_label: "site".to_string(),
                                                        on_deleted: move |_| {
                                                            sites_resource.restart();
                                                            company_resource.restart();
                                                        },
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
                    // Refresh the denormalized counts so the Sites counter in
                    // the Statistics card updates without a manual reload.
                    company_resource.restart();
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
    // Per-field inline validation errors (MAPPS-177).
    let mut phone_err = use_signal(String::new);
    let mut tz_err = use_signal(String::new);
    let mut postal_err = use_signal(String::new);
    let mut country_err = use_signal(String::new);
    // MAPPS-357: block the site create / save / delete writes while the server
    // is unreachable. Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_state = initial.clone();
    let handle_save = move |_| {
        if saving() || deleting() {
            return;
        }
        error.set(String::new());
        phone_err.set(String::new());
        tz_err.set(String::new());
        postal_err.set(String::new());
        country_err.set(String::new());
        // PMS-518: validate all fields and report every failure at once, then
        // focus the first invalid (in on-screen order). Name has no inline slot,
        // so it stays on the banner; the rest use their per-field slots. The
        // bespoke validators that parse-and-return are kept and unwrapped past
        // the single bail.
        let mut guard = FormGuard::new();
        if name.read().trim().is_empty() {
            error.set("Site name is required.".to_string());
            guard.note_invalid(Some("site_name"));
        }
        let phone_res = validate_phone_field(&phone.read(), "Phone");
        if let Err(msg) = &phone_res {
            phone_err.set(msg.clone());
            guard.note_invalid(Some("site_phone"));
        }
        let tz_res = validate_timezone_field(&timezone.read());
        if let Err(msg) = &tz_res {
            tz_err.set(msg.clone());
            guard.note_invalid(Some("site_timezone"));
        }
        let postal_res = validate_postal_field(&postal.read());
        if let Err(msg) = &postal_res {
            postal_err.set(msg.clone());
            guard.note_invalid(Some("site_postal"));
        }
        let country_res = validate_country_field(&country.read());
        if let Err(msg) = &country_res {
            country_err.set(msg.clone());
            guard.note_invalid(Some("site_country"));
        }
        if guard.blocked() {
            return;
        }
        let phone_value = phone_res.expect("phone validated above");
        let tz_value = tz_res.expect("timezone validated above");
        let postal_value = postal_res.expect("postal validated above");
        let country_value = country_res.expect("country validated above");
        saving.set(true);
        let body = serde_json::json!({
            "company_id": save_state.company_id,
            "name": name.read().trim(),
            "address": {
                "line1": optional_string(&line1.read()),
                "line2": optional_string(&line2.read()),
                "city": optional_string(&city.read()),
                "state": optional_string(&state.read()),
                "postal_code": postal_value,
                "country": country_value,
            },
            "phone": phone_value,
            "is_primary": *is_primary.read(),
            "timezone": tz_value,
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
    let can_delete = delete_id.is_some();
    // MAPPS-189: Delete opens the styled ConfirmDialog; the DELETE runs
    // from `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let handle_delete = move |_| {
        if !can_delete || saving() || deleting() {
            return;
        }
        confirming_delete.set(true);
    };
    let on_confirm_delete = move |_: ()| {
        let Some(id) = delete_id.clone() else { return };
        if deleting() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contacts/sites/{id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not delete site: {err}")),
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let footer = rsx! {
        if is_edit {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                // MAPPS-357: block delete while the server is down.
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
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
            // MAPPS-357: block save while the server is down.
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
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
                    ErrorBanner { "{error.read()}" }
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
                        error: postal_err(),
                        oninput: move |e: FormEvent| postal.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_country",
                        label: "Country",
                        placeholder: "US",
                        value: country.read().clone(),
                        error: country_err(),
                        oninput: move |e: FormEvent| country.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_phone",
                        label: "Phone",
                        value: phone.read().clone(),
                        error: phone_err(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    crate::components::Input {
                        name: "site_timezone",
                        label: "Timezone",
                        placeholder: "e.g. America/New_York",
                        value: timezone.read().clone(),
                        error: tz_err(),
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
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete site".to_string(),
            message: "Delete this site? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
    }
}

#[component]
fn CompanyTicketsCard(
    company_id: String,
    company_name: String,
    mut tickets_resource: Resource<Option<PaginatedTicketSummaries>>,
) -> Element {
    let snap = tickets_resource.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let navigator = use_navigator();
    // MAPPS-207: offer a "New Ticket" path straight from the company so a
    // user no longer has to leave for the global ticket list to start one.
    // The query params pre-fill the company on the New Ticket form.
    let new_ticket_href = format!(
        "/tickets/new?company_id={}&company_name={}",
        urlencoding_minimal(&company_id),
        urlencoding_minimal(&company_name)
    );
    // MAPPS-249: "View All" stays scoped to this company.
    let view_all_href = format!("/tickets?company_id={}", urlencoding_minimal(&company_id));
    rsx! {
        CollapsibleCard {
            title: "Recent Tickets",
            count,
            actions: rsx! {
                a {
                    href: "{new_ticket_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "New Ticket"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Ticket" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 3, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 3, message: "Could not load tickets.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 3, message: "No tickets for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        // MAPPS-249: cap the preview at three rows.
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for ticket in rows.into_iter() {
                                    {
                                        let id = ticket.id.to_string();
                                        let key = id.clone();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/tickets/{id}");
                                        let number = ticket.ticket_number.clone();
                                        let title = ticket.title.clone();
                                        let status_name = ticket.status.name.clone();
                                        let variant = if ticket.status.is_closed {
                                            BadgeVariant::Gray
                                        } else {
                                            BadgeVariant::Blue
                                        };
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    div {
                                                        Link {
                                                            to: Route::TicketDetail { id: id.clone() },
                                                            class: "font-medium text-accent hover:opacity-90",
                                                            "{number}"
                                                        }
                                                        p { class: "text-sm text-muted", "{title}" }
                                                    }
                                                }
                                                TableCell {
                                                    Badge { variant, "{status_name}" }
                                                }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::TicketDetail { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "ticket".to_string(),
                                                        on_deleted: move |_| { tickets_resource.restart(); },
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

// ============================================================================
// MAPPS-195: company-scoped relationship cards (Contracts, Projects, Invoices,
// Assets). Each decodes a lightweight subset of its module's list response.
// Status/money helpers are kept local (the per-module versions are private)
// so this stays a single-file change.
// ============================================================================

/// Read the `meta.total` of a list resource, defaulting to 0 while loading or
/// on fetch failure. Feeds the Statistics counts without a separate count call.
fn paginated_total<T: 'static>(res: &Resource<Option<Paginated<T>>>) -> u64 {
    match &*res.read_unchecked() {
        Some(Some(p)) => p.meta.total,
        _ => 0,
    }
}

/// Money fields arrive either as a JSON string (rust_decimal's serde form) or
/// a bare number depending on the endpoint. Decode either into the raw string
/// so `format_money_str` can render it; absent -> `None`.
fn de_money_opt<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

/// Render an optional money string; `None` -> `-`.
fn money_label(v: &Option<String>) -> String {
    match v {
        Some(s) => format_money_str(s),
        None => "-".to_string(),
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ContractSummary {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default, deserialize_with = "de_money_opt")]
    billing_amount: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ProjectSummary {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default, deserialize_with = "de_money_opt")]
    budget_amount: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct InvoiceSummary {
    id: uuid::Uuid,
    #[serde(default)]
    invoice_number: String,
    #[serde(default)]
    status: String,
    #[serde(default, deserialize_with = "de_money_opt")]
    balance_due: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct AssetSummary {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    asset_type_id: Option<uuid::Uuid>,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct AssetTypeOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

#[component]
fn CompanyContractsCard(
    company_id: String,
    mut contracts_resource: Resource<Option<Paginated<ContractSummary>>>,
) -> Element {
    let snap = contracts_resource.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let navigator = use_navigator();
    let view_all_href = format!("/contracts?company_id={}", urlencoding_minimal(&company_id));
    // MAPPS-300: "New Contract" CTA pre-fills this company on the create form
    // (Company appears in the create URL as `?company_id=<uuid>` and the
    // destination form reads it - see `ContractNewPage`).
    let new_contract_href = format!(
        "/contracts/new?company_id={}",
        urlencoding_minimal(&company_id)
    );
    rsx! {
        CollapsibleCard {
            title: "Contracts",
            count,
            actions: rsx! {
                a {
                    href: "{new_contract_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "New Contract"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Contract" }
                        TableHeader { "Value" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load contracts.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No contracts for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for contract in rows.into_iter() {
                                    {
                                        let id = contract.id.to_string();
                                        let key = id.clone();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/contracts/{id}");
                                        let name = contract.name.clone();
                                        let value = money_label(&contract.billing_amount);
                                        let (variant, label) = contract_status_badge(&contract.status);
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::ContractDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { class: "font-medium", "{value}" }
                                                TableCell { Badge { variant, "{label}" } }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::ContractEdit { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "contract".to_string(),
                                                        on_deleted: move |_| { contracts_resource.restart(); },
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
fn CompanyProjectsCard(
    company_id: String,
    mut projects_resource: Resource<Option<Paginated<ProjectSummary>>>,
) -> Element {
    let snap = projects_resource.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let navigator = use_navigator();
    let view_all_href = format!("/projects?company_id={}", urlencoding_minimal(&company_id));
    // MAPPS-300: "New Project" CTA pre-fills this company on the create form.
    let new_project_href = format!(
        "/projects/new?company_id={}",
        urlencoding_minimal(&company_id)
    );
    rsx! {
        CollapsibleCard {
            title: "Projects",
            count,
            actions: rsx! {
                a {
                    href: "{new_project_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "New Project"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Project" }
                        TableHeader { "Budget" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load projects.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No projects for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for project in rows.into_iter() {
                                    {
                                        let id = project.id.to_string();
                                        let key = id.clone();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/projects/{id}");
                                        let name = project.name.clone();
                                        let budget = money_label(&project.budget_amount);
                                        let (variant, label) = project_status_badge(&project.status);
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::ProjectDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { class: "font-medium", "{budget}" }
                                                TableCell { Badge { variant, "{label}" } }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::ProjectDetail { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "project".to_string(),
                                                        on_deleted: move |_| { projects_resource.restart(); },
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
fn CompanyInvoicesCard(
    company_id: String,
    mut invoices_resource: Resource<Option<Paginated<InvoiceSummary>>>,
) -> Element {
    let snap = invoices_resource.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let navigator = use_navigator();
    let view_all_href = format!("/invoices?company_id={}", urlencoding_minimal(&company_id));
    // MAPPS-300: "New Invoice" CTA pre-fills this company on the create form.
    let new_invoice_href = format!(
        "/invoices/new?company_id={}",
        urlencoding_minimal(&company_id)
    );
    rsx! {
        CollapsibleCard {
            title: "Invoices",
            count,
            actions: rsx! {
                a {
                    href: "{new_invoice_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "New Invoice"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Invoice" }
                        TableHeader { "Balance" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load invoices.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No invoices for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for invoice in rows.into_iter() {
                                    {
                                        let id = invoice.id.to_string();
                                        let key = id.clone();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/invoices/{id}");
                                        let number = invoice.invoice_number.clone();
                                        let balance = money_label(&invoice.balance_due);
                                        let (variant, label) = invoice_status_badge(&invoice.status);
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::InvoiceDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{number}"
                                                    }
                                                }
                                                TableCell { class: "font-medium", "{balance}" }
                                                TableCell { Badge { variant, "{label}" } }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::InvoiceDetail { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "invoice".to_string(),
                                                        on_deleted: move |_| { invoices_resource.restart(); },
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
fn CompanyAssetsCard(
    company_id: String,
    mut assets_resource: Resource<Option<Paginated<AssetSummary>>>,
    asset_types_resource: Resource<Option<Paginated<AssetTypeOption>>>,
) -> Element {
    let snap = assets_resource.read_unchecked();
    let count = match &*snap {
        Some(Some(page)) => Some(page.meta.total),
        _ => None,
    };
    let navigator = use_navigator();
    let view_all_href = format!("/assets?company_id={}", urlencoding_minimal(&company_id));
    // MAPPS-300: "New Asset" CTA pre-fills this company on the create form.
    let new_asset_href = format!(
        "/assets/new?company_id={}",
        urlencoding_minimal(&company_id)
    );
    let types_snap = asset_types_resource.read_unchecked();
    // Build an id -> type-name lookup from the (best-effort) type list.
    let type_name = |id: &Option<uuid::Uuid>| -> String {
        match id {
            Some(tid) => match &*types_snap {
                Some(Some(page)) => page
                    .data
                    .iter()
                    .find(|t| &t.id == tid)
                    .map(|t| t.name.clone())
                    .unwrap_or_default(),
                _ => String::new(),
            },
            None => String::new(),
        }
    };
    rsx! {
        CollapsibleCard {
            title: "Assets",
            count,
            actions: rsx! {
                a {
                    href: "{new_asset_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "New Asset"
                }
                a {
                    href: "{view_all_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "View All"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Asset" }
                        TableHeader { "Type" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! { TableEmpty { columns: 4, message: "Could not load assets.".to_string() } },
                    Some(Some(page)) if page.data.is_empty() => rsx! {
                        TableEmpty { columns: 4, message: "No assets for this company yet.".to_string() }
                    },
                    Some(Some(page)) => {
                        let rows: Vec<_> = page.data.iter().take(3).cloned().collect();
                        rsx! {
                            TableBody {
                                for asset in rows.into_iter() {
                                    {
                                        let id = asset.id.to_string();
                                        let key = id.clone();
                                        let edit_id = id.clone();
                                        let delete_path = format!("/assets/{id}");
                                        let name = asset.name.clone();
                                        let tname = type_name(&asset.asset_type_id);
                                        let (variant, label) = asset_status_badge(&asset.status);
                                        rsx! {
                                            TableRow { key: "{key}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::AssetDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                }
                                                TableCell { class: "text-muted", "{tname}" }
                                                TableCell { Badge { variant, "{label}" } }
                                                TableCell { class: "w-10",
                                                    RowActions {
                                                        on_edit: move |_| { navigator.push(Route::AssetDetail { id: edit_id.clone() }); },
                                                        delete_path,
                                                        delete_label: "asset".to_string(),
                                                        on_deleted: move |_| { assets_resource.restart(); },
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
            // MAPPS-357: subscribe to reachability so the list auto-refetches
            // the instant the server comes back (paired with the recovery poll).
            let _reachable = crate::hooks::use_server_reachable();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/contacts/contacts?page={current_page}&per_page={PER_PAGE}");
            // MAPPS-249: scope to one company when a context card's "View All"
            // passes `?company_id=<uuid>`.
            if let Some(company_id) = crate::utils::url::current_query_param("company_id") {
                path.push_str(&format!("&company_id={}", urlencoding_minimal(&company_id)));
            }
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

    use_page_title("Contacts");

    // MAPPS-357: `contacts_resource` is this page's primary resource. It stays
    // a hand-rolled `use_resource` (rather than `use_remote_resource`) because
    // the page needs the loading / failed / `meta.total` distinction from the
    // `Option<PaginatedContacts>` envelope (which is not `Default`). A failed
    // load while the server is flagged down is an outage, not an empty list:
    // render the honest unavailable state instead of an empty contacts table.
    // A 4xx while still reachable keeps the inline banner below. There are no
    // write controls on this page (New = nav, Clear filters = filter reset).
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Contacts".to_string() }
        };
    }

    rsx! {
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

        // MAPPS-321: scope indicator.
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Contacts,
        }

        // MAPPS-388: de-boxed. Search + type controls sit directly on the
        // page; the surrounding Card was much larger than the controls it held.
        div { class: "mb-6",
            div { class: "flex flex-col sm:flex-row gap-4",
                div { class: "flex-1",
                    SearchInput {
                        value: search.read().clone(),
                        placeholder: "Search contacts…",
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
            ErrorBanner { class: "mb-3", "Could not load contacts. Refresh the page to retry." }
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
                striped: true,
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
                    if has_filters {
                        // MAPPS-291 "Clear filters" affordance on the
                        // contacts list mirrors the companies list.
                        TableEmpty {
                            columns: 5,
                            title: "No contacts match your filters".to_string(),
                            description: "Adjust the filters above, or clear them to see every contact again.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        search.set(String::new());
                                        contact_type_filter.set(String::new());
                                        portal_filter.set(String::new());
                                    },
                                    "Clear filters"
                                }
                            },
                        }
                    } else {
                        TableEmpty {
                            columns: 5,
                            title: "No contacts yet".to_string(),
                            description: "Add your first contact to a company.".to_string(),
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
                                role: humanize_contact_type(
                                    contact.contact_type.as_deref().unwrap_or_default(),
                                ),
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
                    class: "font-medium text-accent hover:opacity-90",
                    "{props.name}"
                }
            }
            TableCell {
                // MAPPS-251: a freeform-only contact carries an empty company_id;
                // render its company name as plain text (no CompanyDetail link).
                if !props.company_id.is_empty() {
                    Link {
                        to: Route::CompanyDetail { id: props.company_id.clone() },
                        class: "text-muted hover:text-accent",
                        "{props.company}"
                    }
                } else {
                    span { class: "text-muted", "{props.company}" }
                }
            }
            TableCell { "{props.email}" }
            // MAPPS-283: route the phone column through `format_phone`
            // so the cell shows `(555) 123-4567` not `5551234567`.
            TableCell { {format_phone(&props.phone)} }
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
    // MAPPS-357: N/A for a ContentUnavailable state - this page fetches no
    // primary entity (it is a blank create form; the company prefill below is
    // read from the URL, not the server). The one write control (the Create
    // submit) is disabled while the server is down inside `ContactForm`, which
    // owns the button and is shared with the edit page.
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

    // MAPPS-207: when prefilled from a company, the breadcrumb points back
    // to that company so "Create a new contact" leaves a way back.
    let crumbs = if prefill.id.is_empty() {
        vec![
            crate::components::BreadcrumbItem {
                label: "Contacts".to_string(),
                route: Some(Route::ContactList {}),
            },
            crate::components::BreadcrumbItem {
                label: "New Contact".to_string(),
                route: None,
            },
        ]
    } else {
        vec![
            crate::components::BreadcrumbItem {
                label: "Companies".to_string(),
                route: Some(Route::CompanyList {}),
            },
            crate::components::BreadcrumbItem {
                label: prefill.name.clone(),
                route: Some(Route::CompanyDetail {
                    id: prefill.id.clone(),
                }),
            },
            crate::components::BreadcrumbItem {
                label: "New Contact".to_string(),
                route: None,
            },
        ]
    };

    use_page_title("New Contact");

    rsx! {
        PageHeader {
            title: "New Contact",
            subtitle: "Add a new contact",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs { items: crumbs }
            },
        }
        ContactForm {
            initial,
            mode: ContactFormMode::Create,
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
            // MAPPS-357: subscribe to reachability so the edited entity
            // auto-refetches once the server comes back.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<ContactEditPayload>(&format!(
                "/contacts/contacts/{id}"
            ))
            .await
            .ok()
        }
    });
    let snap = detail.read_unchecked();
    use_page_title("Edit Contact");
    // MAPPS-357: the fetched contact is this edit page's primary resource. A
    // failed load while the server is flagged down is an outage, not a missing
    // record - render the honest unavailable state instead of "Could not load
    // contact" (kept below for a 4xx while still reachable). The Save submit is
    // gated by `can_mutate` inside `ContactForm`.
    let fetch_failed = matches!(*snap, Some(None));
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Edit Contact".to_string() }
        };
    }
    rsx! {
        PageHeader { title: "Edit Contact" }
        match &*snap {
            None => rsx! {
                crate::components::DetailSkeleton {} // PMS-353
            },
            Some(None) => rsx! {
                Card {
                    div { class: "py-8 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contact." }
                        Link {
                            to: Route::ContactList {},
                            class: "text-sm text-accent hover:opacity-90",
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
                    company_id: payload
                        .company_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
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
    // MAPPS-251: optional so a freeform-company contact (company_name only,
    // no FK) deserializes without a null/absent company_id panicking.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
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
    // MAPPS-396 / PMS-729: single-shot "create contact + grant portal
    // access" checkbox. Only wired in Create mode (Edit uses the
    // dedicated ContactPortalCard toggle on the detail page, which the
    // server already mints a setup token from on the false->true
    // transition). When ticked, the POST body carries
    // `create_portal_access: true` and the server mints the setup token
    // and dispatches the auth.welcome email in the same transaction.
    let mut create_portal_access = use_signal(|| false);
    // MAPPS-251: a contact's company can be a freeform typed name instead of an
    // FK-linked CRM company. Open in freeform mode when the loaded contact has a
    // company_name but no resolvable company_id (a freeform-only contact); else
    // open in the existing "link a CRM company" picker mode.
    let initial_freeform = uuid::Uuid::parse_str(initial.company_id.as_str()).is_err()
        && !initial.company_name.trim().is_empty();
    let mut freeform_mode = use_signal(|| initial_freeform);
    let mut freeform_company = use_signal(|| {
        if initial_freeform {
            initial.company_name.clone()
        } else {
            String::new()
        }
    });
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-177, MAPPS-265).
    let mut first_name_err = use_signal(String::new);
    let mut last_name_err = use_signal(String::new);
    let mut email_err = use_signal(String::new);
    let mut phone_err = use_signal(String::new);
    let mut mobile_err = use_signal(String::new);

    let type_options = vec![
        SelectOption::new("primary", "Primary"),
        SelectOption::new("technical", "Technical"),
        SelectOption::new("billing", "Billing"),
        SelectOption::new("other", "Other"),
    ];

    let navigator = use_navigator();
    // MAPPS-357: block the Create / Save submit while the server is
    // unreachable. Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();
    let submit_label = match &mode {
        ContactFormMode::Create => "Create Contact",
        ContactFormMode::Edit { .. } => "Save Changes",
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(String::new());
        first_name_err.set(String::new());
        last_name_err.set(String::new());
        email_err.set(String::new());
        phone_err.set(String::new());
        mobile_err.set(String::new());

        // MAPPS-281: trim required name fields client-side so a
        // whitespace-only value cannot satisfy the browser's native
        // `required` check (which passes any non-empty string, including
        // "   "). Without this the request reached the server and surfaced
        // a raw 422 banner with no field-level error. Reject inline first
        // so the user sees which field failed.
        // PMS-518: validate every field and report all failures at once, then
        // focus the first invalid field (in on-screen order). note_invalid keeps
        // the first id it is given, so the calls below run in field order.
        let mut guard = FormGuard::new();

        // MAPPS-281: trim required names so a whitespace-only value cannot satisfy
        // the browser's native `required` (which passes any non-empty string).
        if first_name.read().trim().is_empty() {
            first_name_err.set("First name is required.".to_string());
            guard.note_invalid(Some("first_name"));
        }
        if last_name.read().trim().is_empty() {
            last_name_err.set("Last name is required.".to_string());
            guard.note_invalid(Some("last_name"));
        }

        // MAPPS-251: company is optional - an FK-linked CRM company (company_id)
        // OR a freeform typed name (company_name), not both. The XOR error has no
        // inline slot, so it goes to the banner; note_invalid blocks and ties
        // focus to the freeform input.
        let picked_company = uuid::Uuid::parse_str(company_id.read().as_str()).ok();
        let freeform_name = freeform_company.read().trim().to_string();
        if picked_company.is_some() && !freeform_name.is_empty() {
            error.set("Pick an existing company or type a new one, not both.".to_string());
            guard.note_invalid(Some("company_name_freeform"));
        }

        // Validate phone/mobile inline before submit (MAPPS-177). The bespoke
        // validator parses-and-returns the value the body uses.
        let phone_res = validate_phone_field(&phone.read(), "Phone");
        if let Err(msg) = &phone_res {
            phone_err.set(msg.clone());
            guard.note_invalid(Some("phone"));
        }
        let mobile_res = validate_phone_field(&mobile.read(), "Mobile");
        if let Err(msg) = &mobile_res {
            mobile_err.set(msg.clone());
            guard.note_invalid(Some("mobile"));
        }

        if guard.blocked() {
            return;
        }
        let phone_value = phone_res.expect("phone validated above");
        let mobile_value = mobile_res.expect("mobile validated above");
        is_submitting.set(true);

        let mut body = serde_json::json!({
            "first_name": first_name.read().trim(),
            "last_name": last_name.read().trim(),
            "email": optional_string(&email.read()),
            "phone": phone_value,
            "mobile": mobile_value,
            "title": optional_string(&title.read()),
            "department": optional_string(&department.read()),
            "contact_type": contact_type.read().clone(),
        });
        // MAPPS-251: send company_id when a CRM company is picked, company_name
        // when a freeform name is typed, and neither when left blank.
        if let Some(company_uuid) = picked_company {
            body["company_id"] = serde_json::json!(company_uuid);
        } else if !freeform_name.is_empty() {
            body["company_name"] = serde_json::json!(freeform_name);
        }
        // MAPPS-396 / PMS-729: opt-in single-shot "create + grant portal
        // access". Only sent on Create (Edit ignores it; the toggle on
        // the detail page owns the grant/revoke there).
        if matches!(mode, ContactFormMode::Create) && *create_portal_access.read() {
            body["create_portal_access"] = serde_json::json!(true);
        }
        let mode = mode.clone();
        let mode_for_toast = mode.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct ContactId {
                    id: uuid::Uuid,
                }
                let result = match &mode {
                    ContactFormMode::Create => crate::hooks::fetch::api::post_authed_typed::<
                        ContactId,
                        _,
                    >("/contacts/contacts", &body)
                    .await
                    .map(|c| c.id.to_string()),
                    ContactFormMode::Edit { id } => {
                        let path = format!("/contacts/contacts/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<ContactId, _>(&path, &body)
                            .await
                            .map(|_| id.clone())
                    }
                };
                match result {
                    Ok(id) => {
                        // MAPPS-293: confirming success toast.
                        let msg = match mode_for_toast {
                            ContactFormMode::Create => "Contact created.",
                            ContactFormMode::Edit { .. } => "Contact saved.",
                        };
                        crate::hooks::toast::push_toast(crate::components::AlertType::Success, msg);
                        navigator.push(Route::ContactDetail { id });
                    }
                    Err(err) => {
                        // MAPPS-265: map server-side field errors from the 422
                        // `errors[]` envelope onto their inline fields so the cue
                        // persists after a failed submit; unmatched fields or a
                        // non-422 failure fall back to the top-of-form banner.
                        let fields = err.field_errors();
                        if fields.is_empty() {
                            error.set(format!("Could not save contact: {}", err.user_message()));
                        } else {
                            let mut leftover = Vec::new();
                            for fe in fields {
                                match fe.field.as_str() {
                                    "first_name" => first_name_err.set(fe.message.clone()),
                                    "last_name" => last_name_err.set(fe.message.clone()),
                                    "email" => email_err.set(fe.message.clone()),
                                    "phone" => phone_err.set(fe.message.clone()),
                                    "mobile" => mobile_err.set(fe.message.clone()),
                                    _ => leftover.push(fe.message.clone()),
                                }
                            }
                            if !leftover.is_empty() {
                                error.set(format!(
                                    "Could not save contact: {}",
                                    leftover.join("; ")
                                ));
                            }
                        }
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
                    ErrorBanner { "{error.read()}" }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "first_name",
                        label: "First Name",
                        required: true,
                        value: first_name.read().clone(),
                        error: first_name_err(),
                        oninput: move |e: FormEvent| first_name.set(e.value()),
                    }
                    crate::components::Input {
                        name: "last_name",
                        label: "Last Name",
                        required: true,
                        value: last_name.read().clone(),
                        error: last_name_err(),
                        oninput: move |e: FormEvent| last_name.set(e.value()),
                    }
                    crate::components::Input {
                        name: "email",
                        label: "Email",
                        r#type: "email",
                        value: email.read().clone(),
                        error: email_err(),
                        oninput: move |e: FormEvent| email.set(e.value()),
                    }
                    crate::components::SuggestInput {
                        name: "title",
                        label: "Title",
                        field: "title",
                        help: "Free text. Suggestions are titles already used in your workspace.",
                        value: title.read().clone(),
                        oninput: move |v: String| title.set(v),
                    }
                    crate::components::Input {
                        name: "phone",
                        label: "Phone",
                        value: phone.read().clone(),
                        error: phone_err(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    crate::components::Input {
                        name: "mobile",
                        label: "Mobile",
                        value: mobile.read().clone(),
                        error: mobile_err(),
                        oninput: move |e: FormEvent| mobile.set(e.value()),
                    }
                    crate::components::SuggestInput {
                        name: "department",
                        label: "Department",
                        field: "department",
                        help: "Free text. Suggestions are departments already used in your workspace.",
                        value: department.read().clone(),
                        oninput: move |v: String| department.set(v),
                    }
                    Select {
                        name: "contact_type",
                        label: "Type",
                        options: type_options,
                        value: contact_type.read().clone(),
                        onchange: move |e: FormEvent| contact_type.set(e.value()),
                    }
                }

                // MAPPS-251: company is optional and can be entered two ways. The
                // toggle flips between "link an existing CRM company" (the picker)
                // and "+ Add Company" (a freeform typed name that creates no
                // `companies` row). Switching modes clears the other mode's value
                // so only one company source is ever submitted.
                div { class: "space-y-2",
                    div { class: "flex items-center justify-between",
                        span { class: "block text-sm font-medium text-content", "Company" }
                        button {
                            r#type: "button",
                            class: "inline-flex items-center text-sm text-blue-600 hover:text-blue-500",
                            onclick: move |_| {
                                let next = !*freeform_mode.read();
                                if next {
                                    company_id.set(String::new());
                                    company_name.set(String::new());
                                } else {
                                    freeform_company.set(String::new());
                                }
                                freeform_mode.set(next);
                            },
                            if *freeform_mode.read() {
                                "Link existing company"
                            } else {
                                PlusIcon { size: IconSize::Small, class: "mr-1".to_string() }
                                "Add Company"
                            }
                        }
                    }
                    if *freeform_mode.read() {
                        crate::components::Input {
                            name: "company_name_freeform",
                            value: freeform_company.read().clone(),
                            oninput: move |e: FormEvent| freeform_company.set(e.value()),
                        }
                        p { class: "text-xs text-muted",
                            "Typed company name only. Not linked to a CRM company record."
                        }
                    } else {
                        crate::components::CompanyPicker {
                            value: company_name.read().clone(),
                            selected_id: picker_selected_id,
                            // MAPPS-251: company is no longer mandatory; a contact
                            // can be saved with no company at all.
                            required: false,
                            label: String::new(),
                            // PMS-352: keep the inline "+ Create new company"
                            // affordance for first-time tenants with zero companies;
                            // distinct from the freeform path, it materializes a real
                            // `companies` row.
                            allow_inline_create: true,
                            onselect: move |(id, name): (String, String)| {
                                company_id.set(id);
                                company_name.set(name);
                            },
                            onclear: move |_| {
                                company_id.set(String::new());
                                company_name.set(String::new());
                            },
                        }
                    }
                }

                // MAPPS-396 / PMS-729: single-shot portal-grant option. Only
                // rendered on Create; Edit uses the ContactPortalCard on the
                // detail page (which already fires the same setup-email
                // dispatch on the false->true transition).
                if matches!(&props.mode, ContactFormMode::Create) {
                    div { class: "space-y-2 rounded-md border border-line bg-surface p-4",
                        label { class: "flex items-start gap-3 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "mt-1 h-4 w-4 rounded border-line text-accent focus:ring-accent",
                                checked: *create_portal_access.read(),
                                oninput: move |e: FormEvent| {
                                    create_portal_access.set(e.value() == "true");
                                },
                            }
                            div {
                                span { class: "block text-sm font-medium text-content",
                                    "Grant portal access"
                                }
                                p { class: "mt-1 text-xs text-muted",
                                    "Emails this contact a link to set a password and sign in to the Client Portal. Requires an email address above."
                                }
                            }
                        }
                    }
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
                        // MAPPS-357: block the submit while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
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

    // MAPPS-357: the contact record is this detail page's primary resource
    // (the tickets list below is secondary and keeps degrading to its own
    // card). Subscribe to reachability so it auto-refetches on reconnect.
    let mut contact = use_resource(move || {
        let id = id_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
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
    // MAPPS-278: prefer an honest "Loading..." over the generic entity
    // type while the fetch is in flight; reserve a distinct "Contact not
    // found" for a confirmed-empty resource so the user does not see a
    // blank "Contact" header that briefly looked correct.
    let header_title = match &*snap {
        Some(Some(c)) => format!("{} {}", c.first_name, c.last_name)
            .trim()
            .to_string(),
        None => "Loading…".to_string(),
        Some(None) => "Contact not found".to_string(),
    };
    use_page_title(&header_title);

    let navigator = use_navigator();
    let mut deleting = use_signal(|| false);
    let portal_toggling = use_signal(|| false);
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();
    let mut confirming_delete = use_signal(|| false);
    // MAPPS-357: gate the destructive Delete while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contacts/contacts/{id}");
                if crate::hooks::fetch::api::delete_authed(&path).await.is_ok() {
                    navigator.push(Route::ContactList {});
                }
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    // MAPPS-357: a failed load of the primary contact while the server is
    // flagged down is an outage, not a missing record - render the honest
    // unavailable state instead of "Could not load contact" (kept below for a
    // 4xx while still reachable).
    let fetch_failed = matches!(*snap, Some(None));
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Contact".to_string() }
        };
    }

    rsx! {
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete contact".to_string(),
            message: "Delete this contact? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
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
                    // MAPPS-357: block delete while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        if !*deleting.read() {
                            confirming_delete.set(true);
                        }
                    },
                    "Delete"
                }
            },
        }

        match &*snap {
            None => rsx! {
                crate::components::DetailSkeleton {} // PMS-353
            },
            Some(None) => rsx! {
                Card {
                    div { class: "py-8 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300 mb-2", "Could not load contact." }
                        Link {
                            to: Route::ContactList {},
                            class: "text-sm text-accent hover:opacity-90",
                            "Back to contacts"
                        }
                    }
                }
            },
            Some(Some(c)) => {
                let company_id = c.company_id.map(|id| id.to_string());
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
                                                dt { class: "text-sm text-muted", "Email" }
                                                dd { class: "mt-1",
                                                    a {
                                                        href: "mailto:{email}",
                                                        class: "text-accent hover:opacity-90",
                                                        "{email}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(phone) = phone {
                                        if !phone.is_empty() {
                                            div {
                                                dt { class: "text-sm text-muted", "Phone" }
                                                // MAPPS-283: render with separators.
                                                dd { class: "mt-1", {format_phone(&phone)} }
                                            }
                                        }
                                    }
                                    if let Some(mobile) = mobile {
                                        if !mobile.is_empty() {
                                            div {
                                                dt { class: "text-sm text-muted", "Mobile" }
                                                // MAPPS-283: render with separators.
                                                dd { class: "mt-1", {format_phone(&mobile)} }
                                            }
                                        }
                                    }
                                    if let Some(title) = title {
                                        if !title.is_empty() {
                                            div {
                                                dt { class: "text-sm text-muted", "Title" }
                                                dd { class: "mt-1", "{title}" }
                                            }
                                        }
                                    }
                                    if let Some(dept) = department {
                                        if !dept.is_empty() {
                                            div {
                                                dt { class: "text-sm text-muted", "Department" }
                                                dd { class: "mt-1", "{dept}" }
                                            }
                                        }
                                    }
                                    if !contact_type.is_empty() {
                                        div {
                                            dt { class: "text-sm text-muted", "Type" }
                                            dd { class: "mt-1",
                                                Badge { variant: BadgeVariant::Blue, "{humanize_contact_type(&contact_type)}" }
                                            }
                                        }
                                    }
                                    if !company_name.is_empty() {
                                        div {
                                            dt { class: "text-sm text-muted", "Company" }
                                            dd { class: "mt-1",
                                                // MAPPS-251: link only when an FK-linked CRM
                                                // company exists; a freeform company name has
                                                // no `companies` row to navigate to.
                                                if let Some(cid) = company_id.clone() {
                                                    Link {
                                                        to: Route::CompanyDetail { id: cid },
                                                        class: "text-accent hover:opacity-90",
                                                        "{company_name}"
                                                    }
                                                } else {
                                                    span { class: "text-content", "{company_name}" }
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
    // MAPPS-251: optional FK; `None` for a freeform-company contact.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
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
                    class: "text-sm text-accent hover:opacity-90",
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
                                                            class: "font-medium text-accent hover:opacity-90",
                                                            "{number}"
                                                        }
                                                        p { class: "text-sm text-muted", "{title}" }
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
    // MAPPS-357: block the portal grant / revoke writes while the server is
    // unreachable. Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();
    rsx! {
        Card { title: "Portal Access",
            if is_portal_user {
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-muted", "Status" }
                        Badge { variant: BadgeVariant::Green, "Granted" }
                    }
                    p { class: "text-xs text-muted",
                        "This contact can sign in to the Client Portal. A setup-password email was sent when access was granted; if the link expired (72h TTL), revoke and grant again to reissue it."
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *toggling.read(),
                        // MAPPS-357: block revoke while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't change portal access while the server is unreachable".to_string()),
                        onclick: move |_| {
                            let id = contact_id.clone();
                            toggling.set(true);
                            spawn(async move {
                                let path = format!("/contacts/contacts/{id}");
                                let body = serde_json::json!({ "is_portal_user": false });
                                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await {
                                    Ok(_) => on_change.call(()),
                                    Err(err) => crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Error,
                                        format!("Could not revoke portal access: {err}"),
                                    ),
                                }
                                toggling.set(false);
                            });
                        },
                        "Revoke portal access"
                    }
                }
            } else {
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        span { class: "text-sm text-muted", "Status" }
                        Badge { variant: BadgeVariant::Gray, "Not granted" }
                    }
                    p { class: "text-xs text-muted",
                        "Granting access emails this contact a link to set a password and sign in to the Client Portal. Requires a valid email address on the contact."
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *toggling.read(),
                        // MAPPS-357: block grant while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't change portal access while the server is unreachable".to_string()),
                        onclick: move |_| {
                            let id = contact_id.clone();
                            toggling.set(true);
                            spawn(async move {
                                let path = format!("/contacts/contacts/{id}");
                                let body = serde_json::json!({ "is_portal_user": true });
                                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body).await {
                                    Ok(_) => on_change.call(()),
                                    Err(err) => crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Error,
                                        format!("Could not grant portal access: {err}"),
                                    ),
                                }
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

#[cfg(test)]
mod company_type_tests {
    use super::humanize_company_type;
    use crate::modules::contacts::CompanyType;

    /// Exhaustive over the shared enum: adding a `CompanyType` variant
    /// upstream stops this compiling, so a new variant cannot silently reach
    /// the UI as a raw snake_case tag (which is how `Internal` surfaced).
    fn expected_label(variant: CompanyType) -> &'static str {
        match variant {
            CompanyType::Client => "Client",
            CompanyType::Prospect => "Prospect",
            CompanyType::Vendor => "Vendor",
            CompanyType::Partner => "Partner",
            CompanyType::Internal => "Internal",
        }
    }

    /// MAPPS-383: every `CompanyType` the shared crate can send gets a
    /// title-case label, including the added `Internal`.
    #[test]
    fn every_shared_variant_has_a_title_case_label() {
        for variant in [
            CompanyType::Client,
            CompanyType::Prospect,
            CompanyType::Vendor,
            CompanyType::Partner,
            CompanyType::Internal,
        ] {
            assert_eq!(
                humanize_company_type(variant.as_str()),
                expected_label(variant)
            );
        }
    }

    #[test]
    fn unknown_tag_falls_through_unchanged() {
        assert_eq!(humanize_company_type("franchisee"), "franchisee");
    }
}

#[cfg(test)]
mod validation_tests {
    use super::{
        validate_country_field, validate_name_field, validate_phone_field, validate_postal_field,
        validate_timezone_field, validate_website_field,
    };
    use serde_json::Value;

    #[test]
    fn name_required_and_rejects_control_chars() {
        // Trims and returns the cleaned name.
        assert_eq!(validate_name_field("  Acme Co  ").unwrap(), "Acme Co");
        // Empty / whitespace-only rejected.
        assert!(validate_name_field("").is_err());
        assert!(validate_name_field("   ").is_err());
        // Control characters rejected.
        assert!(validate_name_field("Acme\tCo").is_err());
        assert!(validate_name_field("Acme\u{0007}Co").is_err());
    }

    #[test]
    fn website_requires_http_scheme_and_rejects_dangerous() {
        // Blank -> null (Website is optional).
        assert_eq!(validate_website_field("  ").unwrap(), Value::Null);
        // Valid http/https pass through unchanged.
        assert_eq!(
            validate_website_field("https://example.com").unwrap(),
            Value::String("https://example.com".into())
        );
        assert_eq!(
            validate_website_field("http://example.com/path?q=1").unwrap(),
            Value::String("http://example.com/path?q=1".into())
        );
        // Dangerous schemes are rejected before any request.
        assert!(validate_website_field("javascript:alert(1)").is_err());
        assert!(validate_website_field("java\tscript:alert(1)").is_err());
        assert!(validate_website_field("data:text/html,<script>").is_err());
        assert!(validate_website_field("vbscript:msgbox(1)").is_err());
        // Non-http schemes and scheme-less input are rejected.
        assert!(validate_website_field("mailto:a@example.com").is_err());
        assert!(validate_website_field("example.com").is_err());
        // Malformed http(s) URLs are rejected.
        assert!(validate_website_field("http://").is_err());
        assert!(validate_website_field("https://exa mple.com").is_err());
    }

    #[test]
    fn phone_normalizes_and_validates() {
        // Blank -> null.
        assert_eq!(validate_phone_field("  ", "Phone").unwrap(), Value::Null);
        // Formatted -> normalized E.164.
        assert_eq!(
            validate_phone_field("+1 (415) 555-1234", "Phone").unwrap(),
            Value::String("+14155551234".into())
        );
        // Garbage / leading zero rejected.
        assert!(validate_phone_field("not-a-phone", "Phone").is_err());
        assert!(validate_phone_field("0412 345 678", "Phone").is_err());
    }

    #[test]
    fn country_requires_two_letters() {
        assert_eq!(validate_country_field("").unwrap(), Value::Null);
        assert_eq!(
            validate_country_field("us").unwrap(),
            Value::String("US".into())
        );
        assert!(validate_country_field("USA").is_err());
        assert!(validate_country_field("United States").is_err());
    }

    #[test]
    fn postal_permissive_rule() {
        assert_eq!(validate_postal_field("").unwrap(), Value::Null);
        assert!(validate_postal_field("K1A 0B1").is_ok());
        assert!(validate_postal_field("90210-1234").is_ok());
        assert!(validate_postal_field("X".repeat(13).as_str()).is_err());
        assert!(validate_postal_field("12_34").is_err());
    }

    #[test]
    fn timezone_light_check() {
        assert_eq!(validate_timezone_field("").unwrap(), Value::Null);
        assert!(validate_timezone_field("America/New_York").is_ok());
        assert!(validate_timezone_field("America/New York").is_err());
        assert!(validate_timezone_field("notazone").is_err());
    }

    // PMS-368: the list "Role" column binds humanized `contact_type`
    // (the role classification) instead of the free-text `title`.
    #[test]
    fn contact_type_humanizes_canonical_set() {
        use super::humanize_contact_type;
        assert_eq!(humanize_contact_type("primary"), "Primary");
        assert_eq!(humanize_contact_type("technical"), "Technical");
        assert_eq!(humanize_contact_type("billing"), "Billing");
        assert_eq!(humanize_contact_type("other"), "Other");
        // Unknown / absent values pass through verbatim rather than vanishing.
        assert_eq!(humanize_contact_type("escalation"), "escalation");
        assert_eq!(humanize_contact_type(""), "");
    }
}

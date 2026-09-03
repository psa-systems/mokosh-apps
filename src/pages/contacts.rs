//! Contact and company pages

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    asset_status_badge, clear_on_edit, contract_status_badge, invoice_status_badge,
    project_status_badge, use_page_title, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant,
    Card, CollapsibleCard, DataTable, ErrorBanner, IconSize, Modal, PageHeader, PlusIcon,
    SearchInput, Select, SelectOption, SortDirection, Table, TableBody, TableCell, TableEmpty,
    TableHead, TableHeader, TableLoading, TableRow,
};
use crate::modules::contacts::Address;
use crate::utils::money::format_money_str;
use crate::utils::sort_keys::TICKETS_RECENT_SORT;
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
///
/// MAPPS-527: only columns whose sort key is in `COMPANY_SORT_KEYS` belong
/// here. `Type` mapped to `company_type`, which the server discards, so the
/// header rendered a sort the rows never had; its affordance is gone until
/// the server allow-lists the key.
#[derive(Clone, Copy, PartialEq)]
enum CompanySortKey {
    Company,
}

impl CompanySortKey {
    /// Every variant, for the allow-list test. `company_sort_query` matches
    /// exhaustively, so a new variant fails to compile until it is mapped.
    #[cfg(test)]
    const ALL: &'static [Self] = &[Self::Company];
}

/// Sortable columns on the contact list (F3).
///
/// MAPPS-527: `Company` mapped to `company_name`, which is absent from
/// `CONTACT_SORT_KEYS`, so it is no longer offered.
#[derive(Clone, Copy, PartialEq)]
enum ContactSortKey {
    Name,
}

impl ContactSortKey {
    /// Every variant, for the allow-list test.
    #[cfg(test)]
    const ALL: &'static [Self] = &[Self::Name];
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
    // MAPPS-575: `active` | `inactive` | `prospect`. Absent on an older server,
    // which `#[serde(default)]` renders as empty and the UI treats as active.
    #[serde(default)]
    status: String,
    #[serde(default)]
    account_manager_name: Option<String>,
    #[serde(default)]
    open_ticket_count: Option<i64>,
}

/// PMS-926 `GET /contacts/companies/{id}/deletion-preview`.
///
/// The client deliberately holds no list of which tables block a delete. It
/// held one until MAPPS-577, and that copy went stale the moment PMS-919
/// changed the rules: the dialog kept warning about projects, appointments and
/// sub-companies long after those started unlinking instead of blocking.
#[derive(Clone, Debug, Default, Deserialize)]
struct DeletionPreview {
    #[serde(default)]
    can_delete: bool,
    /// Refused for what the company IS (the tenant's own company, PMS-919)
    /// rather than for what references it, so `blocking` is empty and the
    /// delete still fails.
    #[serde(default)]
    is_own_company: bool,
    #[serde(default)]
    blocking: Vec<DeletionRecords>,
    #[serde(default)]
    unlinked: Vec<DeletionRecords>,
    #[serde(default)]
    removed: Vec<DeletionRecords>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct DeletionRecords {
    label: String,
    count: i64,
    /// PMS-920: these exist to be KEPT. Telling somebody to clear their
    /// invoices to tidy a client list destroys the record the refusal is
    /// protecting, so the two read differently.
    #[serde(default)]
    retained: bool,
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

/// MAPPS-481: one entry of a contact's `phones` array (PMS-806's
/// `contact_phones`). The array already arrives in `sort_order`, so the SPA
/// keeps the server's order rather than re-deriving it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct RemotePhone {
    #[serde(default)]
    phone_type: String,
    #[serde(default)]
    number: String,
    #[serde(default)]
    extension: Option<String>,
    #[serde(default)]
    is_primary: bool,
}

/// MAPPS-481: one entry of a contact's `companies` array (PMS-806's
/// `contact_companies`). `title` is the role at THIS company; the contact's
/// own `title` stays their default.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct RemoteCompanyLink {
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    is_primary: bool,
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
    // MAPPS-481: `#[serde(default)]` so a server that predates PMS-806 still
    // deserializes; the `phone` / `company_id` mirrors above cover that case.
    #[serde(default)]
    phones: Vec<RemotePhone>,
    #[serde(default)]
    companies: Vec<RemoteCompanyLink>,
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
    // MAPPS-575: default to active, which is what makes archiving worth doing.
    // The Select renders this value, so the default is STATED rather than
    // silently applied: a user who cannot find a company they archived can see
    // that the list is filtered and change it, instead of concluding it was
    // deleted.
    let mut status_filter = use_signal(|| "active".to_string());
    let mut sort = use_signal(|| None::<(CompanySortKey, SortDirection)>);
    let mut page = use_signal(|| 1usize);

    let type_options = vec![
        SelectOption::new("", "All Types"),
        SelectOption::new("client", "Client"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("vendor", "Vendor"),
    ];
    let status_options = vec![
        SelectOption::new("active", "Active"),
        SelectOption::new("inactive", "Inactive (archived)"),
        SelectOption::new("prospect", "Prospect"),
        SelectOption::new("", "Any status"),
    ];

    let search_text = search.read().trim().to_string();
    let type_text = type_filter.read().clone();
    let status_text = status_filter.read().clone();
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
        let status_filter = status_filter.read().clone();
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
            // Server-side (`CompanyFilter::status`), not a client-side pass over
            // the current page: filtering after paging would show a short page
            // and a wrong total.
            if !status_filter.is_empty() {
                path.push_str(&format!("&status={}", urlencoding_minimal(&status_filter)));
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
    // The status filter counts as a filter only when it is NOT the default:
    // a brand-new tenant with no companies must read as "No companies yet",
    // not as "No companies match your filters".
    let has_filters = !search_text.is_empty() || !type_text.is_empty() || status_text != "active";

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
                Select {
                    name: "status",
                    options: status_options,
                    value: status_filter.read().clone(),
                    onchange: move |e: FormEvent| {
                        status_filter.set(e.value());
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
                        TableHeader { "Type" }
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
                                        status_filter.set("active".to_string());
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
                                status: company.status,
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
    /// MAPPS-575: raw `companies.status`. Only `inactive` renders anything, so
    /// an older server that omits the field reads as active.
    status: String,
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
                // MAPPS-575: an archived company is reachable from "Any status",
                // and once it is on screen it has to be distinguishable from an
                // active one. Beside the name rather than in its own column: the
                // default view is active-only, so a whole column would be empty
                // almost always.
                if props.status == "inactive" {
                    Badge { variant: BadgeVariant::Gray, class: "ml-2", "Archived" }
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
                    status: payload.status.clone(),
                    industry: payload.industry.clone().unwrap_or_default(),
                    website: payload.website.clone().unwrap_or_default(),
                    phone: payload.phone.clone().unwrap_or_default(),
                    address_line1: payload.address.line1.clone().unwrap_or_default(),
                    address_line2: payload.address.line2.clone().unwrap_or_default(),
                    address_city: payload.address.city.clone().unwrap_or_default(),
                    address_state: payload.address.state.clone().unwrap_or_default(),
                    address_postal_code: payload.address.postal_code.clone().unwrap_or_default(),
                    address_country: payload.address.country.clone().unwrap_or_default(),
                    notes: payload.notes.clone().unwrap_or_default(),
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
    status: String,
    #[serde(default)]
    industry: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    address: Address,
    // MAPPS-614 / PMS-952: the free-text note, held and rendered as Markdown.
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyFormValues {
    name: String,
    company_type: String,
    status: String,
    industry: String,
    website: String,
    phone: String,
    address_line1: String,
    address_line2: String,
    address_city: String,
    address_state: String,
    address_postal_code: String,
    address_country: String,
    notes: String,
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

    // MAPPS-575: archiving is what an operator almost always wants when they
    // reach for Delete on a company that has history, and it is the alternative
    // the server's own delete refusal names (PMS-920). The column and the API
    // have carried it since migration 004; only the form was missing.
    let initial_status = if initial.status.is_empty() {
        "active".to_string()
    } else {
        initial.status.clone()
    };
    let name = use_signal(|| initial.name.clone());
    let mut company_type = use_signal(|| initial_type.clone());
    let mut status = use_signal(|| initial_status.clone());
    let mut industry = use_signal(|| initial.industry.clone());
    // PMS-601: industry suggestions come from the tenant's editable lookup
    // (Settings > Company Industries), not a hardcoded list. Active names only.
    let industry_options = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<IndustryOption>("/contacts/company-industries")
            .await
            .map(|rows| {
                rows.into_iter()
                    .filter(|o| o.is_active)
                    .map(|o| o.name)
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|e| {
                // Best-effort: the field stays free-text without suggestions.
                tracing::warn!("industry suggestion load failed: {e}");
                Vec::new()
            })
    });
    let industry_suggestions = industry_options
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let mut website = use_signal(|| initial.website.clone());
    let phone = use_signal(|| initial.phone.clone());
    let line1 = use_signal(|| initial.address_line1.clone());
    let line2 = use_signal(|| initial.address_line2.clone());
    let city = use_signal(|| initial.address_city.clone());
    let mut state = use_signal(|| initial.address_state.clone());
    let postal = use_signal(|| initial.address_postal_code.clone());
    let mut country = use_signal(|| initial.address_country.clone());
    let mut notes = use_signal(|| initial.notes.clone());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field inline validation errors (MAPPS-177, MAPPS-213, MAPPS-265).
    let mut name_err = use_signal(String::new);
    let mut website_err = use_signal(String::new);
    let mut phone_err = use_signal(String::new);
    let mut postal_err = use_signal(String::new);
    // MAPPS-480: advisory note under the Website field carrying the background
    // probe's state, and the value that probe was last fired for so tabbing
    // through an unchanged field does not re-fire it.
    let mut website_note = use_signal(String::new);
    let mut website_probed = use_signal(String::new);
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
        // Same shape as the type default above: a blank incoming status renders
        // as "active", which is not an edit.
        let same_status_default = initial_for_dirty.status.is_empty() && *status.read() == "active";
        *name.read() != initial_for_dirty.name
            || (*company_type.read() != initial_for_dirty.company_type && !same_type_default)
            || (*status.read() != initial_for_dirty.status && !same_status_default)
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

    // MAPPS-575: the three values `companies_status_check` allows. Preserve any
    // value outside the list as its own option, the same way `type_options`
    // does, so editing a company never silently retypes it.
    let status_options = {
        let current = status.read().clone();
        let mut opts = vec![
            SelectOption::new("active", "Active"),
            SelectOption::new("inactive", "Inactive (archived)"),
            SelectOption::new("prospect", "Prospect"),
        ];
        if !current.is_empty() && !opts.iter().any(|o| o.value == current) {
            opts.push(SelectOption::new(current.clone(), current.clone()));
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
    // MAPPS-423: Cancel returns to what the user was editing, not to the list.
    let cancel_route = match &mode {
        CompanyFormMode::Create => Route::CompanyList {},
        CompanyFormMode::Edit { id } => Route::CompanyDetail { id: id.clone() },
    };

    // MAPPS-480: resolve what a typed domain actually serves, in the background,
    // while the user finishes the rest of the form. Fired on blur only, so one
    // completed value costs one request instead of one per keystroke. Advisory
    // throughout: it never gates validation and never blocks or delays submit,
    // which saves whatever is in the field at the time.
    let mut probe_website = move || {
        let typed = website.read().trim().to_string();
        if typed.is_empty() || typed == website_probed() {
            return;
        }
        let normalized = match validate_website_field(&typed) {
            Ok(serde_json::Value::String(url)) => url,
            // Blank returns above, so this arm is unreachable; it exists so
            // the match is total rather than discarding the value.
            Ok(_) => return,
            // Nothing to probe, and the message is shown now rather than
            // dropped and re-derived at submit. `oninput` clears it again as
            // soon as the user resumes typing.
            Err(msg) => {
                website_err.set(msg);
                website_note.set(String::new());
                return;
            }
        };
        website_probed.set(typed);
        website_note.set(format!("Checking {}…", website_host(&normalized)));
        spawn(async move {
            #[cfg(feature = "app")]
            {
                // `/contacts` is where the server nests the contacts router
                // (`api/router.rs`), so the probe sits beside the company
                // create/edit calls above, not at a bare `/companies/...`.
                let path = format!(
                    "/contacts/companies/website-probe?url={}",
                    urlencoding_minimal(&normalized)
                );
                match crate::hooks::fetch::api::get_authed::<WebsiteProbe>(&path).await {
                    Ok(probe) => {
                        // A site that answered replaces the value with the
                        // address that actually answered; anything else keeps
                        // the normalized value the user typed.
                        if probe.reachable {
                            if let Some(canonical) = probe.canonical_url.clone() {
                                website.set(canonical.clone());
                                website_probed.set(canonical);
                            }
                        }
                        website_note.set(website_probe_note(&normalized, &probe));
                    }
                    Err(e) => {
                        // Logged with the underlying cause before the note is
                        // rendered, so a probe that failed is never a silent
                        // no-op (error-visibility rule).
                        tracing::warn!("website probe for {normalized} failed: {e}");
                        website_note.set(website_unreachable_note(&normalized, "the check failed"));
                    }
                }
            }
        });
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
            "status": status.read().clone(),
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
            // MAPPS-614: always a string, never null. See `clearable_string`.
            "notes": clearable_string(&notes.read()),
        });
        let mode = mode.clone();
        // MAPPS-293: clone the mode again for the post-success toast so the
        // outer `mode` is still available in case of an Err branch.
        let mode_for_toast = mode.clone();
        spawn(async move {
            #[cfg(feature = "app")]
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
                        oninput: clear_on_edit(name, name_err),
                    }
                    Select {
                        name: "type",
                        label: "Company Type",
                        options: type_options,
                        value: company_type.read().clone(),
                        onchange: move |e: FormEvent| company_type.set(e.value()),
                    }
                    Select {
                        name: "status",
                        label: "Status",
                        options: status_options,
                        // MAPPS-575: say what archiving DOES, because the
                        // alternative the user was reaching for is Delete, and
                        // the difference that matters to them is whether the
                        // company's history survives.
                        help: "Inactive archives the company: its history is kept and it drops out of the default lists and pickers.",
                        value: status.read().clone(),
                        onchange: move |e: FormEvent| status.set(e.value()),
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
                        placeholder: "example.com",
                        maxlength: 255,
                        value: website.read().clone(),
                        error: website_err(),
                        // MAPPS-480: the probe's note. `Input` renders `help`
                        // only when no error is showing, so an inline
                        // validation error takes precedence with no extra
                        // logic here.
                        help: website_note(),
                        oninput: move |e: FormEvent| {
                            // Clear the blur-time message as soon as the value
                            // is being corrected.
                            website_err.set(String::new());
                            website.set(e.value());
                        },
                        onblur: move |_| probe_website(),
                    }
                    crate::components::Input {
                        name: "phone",
                        label: "Phone",
                        placeholder: "(555) 555-5555",
                        value: phone.read().clone(),
                        error: phone_err(),
                        oninput: clear_on_edit(phone, phone_err),
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
                        oninput: clear_on_edit(line1, line1_err),
                    }
                    crate::components::Input {
                        name: "address_line2",
                        label: "Street (line 2)",
                        maxlength: 255,
                        value: line2.read().clone(),
                        error: line2_err(),
                        oninput: clear_on_edit(line2, line2_err),
                    }
                    crate::components::Input {
                        name: "address_city",
                        label: "City",
                        maxlength: 100,
                        value: city.read().clone(),
                        error: city_err(),
                        oninput: clear_on_edit(city, city_err),
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
                        oninput: clear_on_edit(postal, postal_err),
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

                // MAPPS-614: David's ask, and the reason it is the shared
                // editor rather than a textarea: every other Markdown surface
                // in the app went through MAPPS-610 to get here, and a second
                // implementation would be the one thing that ticket exists to
                // prevent.
                crate::components::MarkdownEditor {
                    name: "company_notes".to_string(),
                    label: "Notes".to_string(),
                    placeholder: "Anything worth knowing about this company.".to_string(),
                    rows: 8,
                    views: true,
                    view_pref_key: "company_notes_view_mode".to_string(),
                    disabled: !can_mutate,
                    value: notes.read().clone(),
                    oninput: move |next: String| notes.set(next),
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: cancel_route.clone(),
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

/// MAPPS-614: a field the user must be able to empty again.
///
/// `optional_string` below sends JSON null for a blank field, and the server's
/// company and contact updates add `notes = $n` to the UPDATE only when the
/// key carries a value (PMS-952 pins this). A null therefore means "leave it
/// alone", not "erase it": a user who deletes their notes and saves would get
/// a 200 and find the old text still there. An empty string is what clears it.
///
/// The same reasoning already governs `company_name`, which this form sends as
/// `""` when a company is linked.
fn clearable_string(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.trim().to_string())
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
///
/// MAPPS-582: `is_control` is true only for `Cc`, so it let U+200B and U+FEFF
/// through and the app stored them, making `Acme\u{200B}` a second record
/// indistinguishable from `Acme` on screen. The invisibles are stripped rather
/// than rejected, because the user cannot see the character and so cannot act
/// on a message about it; a real control character stays an error.
fn validate_name_field(raw: &str) -> Result<String, String> {
    let cleaned = crate::utils::text::strip_invisible(raw);
    let trimmed = cleaned.trim();
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
    // MAPPS-582: `clean_strict` removes the characters that render as nothing
    // (U+200B, U+FEFF, the soft hyphen, the bidi marks) and folds every exotic
    // space onto a plain one. The strip set below then drops whitespace via
    // `char::is_whitespace` rather than the three hardcoded space characters it
    // used to name, which missed U+202F, U+2007, U+3000 and the rest and let
    // them reach the E.164 check as "not a digit".
    let raw = crate::utils::text::clean_strict(raw);
    let normalized: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '(' | ')' | '.'))
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

// ---- MAPPS-481 / PMS-806: a contact's typed phone list and company links.

/// The five `contact_phones.phone_type` values PMS-806's CHECK constraint
/// allows, paired with their labels and in the order the form offers them.
const PHONE_TYPES: &[(&str, &str)] = &[
    ("mobile", "Mobile"),
    ("work", "Work"),
    ("home", "Home"),
    ("fax", "Fax"),
    ("other", "Other"),
];

/// Coerce a form value onto one of the wire values. Anything unrecognized
/// becomes `other`, which is the server's own default for the column.
fn normalize_phone_type(raw: &str) -> &'static str {
    PHONE_TYPES
        .iter()
        .find(|(value, _)| *value == raw)
        .map(|(value, _)| *value)
        .unwrap_or("other")
}

/// Label a stored `phone_type` for the read surfaces. An unknown value (a
/// server that grew a sixth type) passes through verbatim rather than
/// vanishing, the same rule [`humanize_contact_type`] follows.
fn humanize_phone_type(raw: &str) -> String {
    PHONE_TYPES
        .iter()
        .find(|(value, _)| *value == raw)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| raw.to_string())
}

/// Render one phone entry as the read surfaces show it: the number with
/// separators (MAPPS-283) and its extension when it has one.
fn format_phone_entry(number: &str, extension: Option<&str>) -> String {
    let base = format_phone(number);
    match extension.map(str::trim).filter(|e| !e.is_empty()) {
        Some(ext) => format!("{base} ext. {ext}"),
        None => base,
    }
}

/// The entry a list view shows for a contact: the one flagged primary, else
/// the first. PMS-806 promotes the first entry when none is flagged, so this
/// mirrors what the server stored.
fn primary_entry<T>(entries: &[T], is_primary: impl Fn(&T) -> bool) -> Option<&T> {
    entries
        .iter()
        .find(|e| is_primary(e))
        .or_else(|| entries.first())
}

/// The contacts-list Phone cell: the primary number with its type, e.g.
/// "Mobile (904) 210-8340". Falls back to the `phone` mirror when the server
/// sent no list, so a pre-PMS-806 response still fills the column.
fn primary_phone_label(phones: &[RemotePhone], fallback: &str) -> String {
    match primary_entry(phones, |p| p.is_primary) {
        Some(entry) => {
            let number = format_phone_entry(&entry.number, entry.extension.as_deref());
            let label = humanize_phone_type(&entry.phone_type);
            if label.is_empty() {
                number
            } else {
                format!("{label} {number}")
            }
        }
        None => format_phone(fallback),
    }
}

/// The row index in a server field name of the form `phones[2].number`, which
/// is how PMS-806 identifies the entry that failed. `None` for any other field
/// name, which keeps the existing mapping untouched.
fn phone_row_index(field: &str) -> Option<usize> {
    let (index, _) = field.strip_prefix("phones[")?.split_once(']')?;
    index.parse().ok()
}

/// The contacts-list Company cell suffix: "+N" for the links beyond the one
/// shown, empty when the contact links at most one company.
fn extra_company_suffix(total: usize) -> String {
    if total > 1 {
        format!("+{}", total - 1)
    } else {
        String::new()
    }
}

/// Validate an optional ISO 3166-1 alpha-2 country code. Blank -> `Ok(None)`.
/// Requires exactly two ASCII letters (normalized to uppercase). The server
/// (PMS-325) checks membership against the official set.
fn validate_country_field(raw: &str) -> Result<serde_json::Value, String> {
    // MAPPS-582: a country code admits no invisible character anywhere.
    let cleaned = crate::utils::text::clean_strict(raw);
    let trimmed = cleaned.as_str();
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
    // MAPPS-582: `is_control` covers only `Cc`, so an invisible `Cf` character
    // used to pass this gate and get stored. Strip those first, then keep
    // rejecting the real control characters.
    let cleaned = crate::utils::text::strip_invisible(raw);
    let trimmed = cleaned.trim();
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
    // MAPPS-582: the charset below is ASCII-only, so an invisible character
    // rejected the code with a message naming characters the user cannot see.
    let cleaned = crate::utils::text::clean_strict(raw);
    let trimmed = cleaned.as_str();
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

/// Validate an optional Website URL. Blank -> `Ok(None)`. An explicit
/// `http`/`https` scheme passes through unchanged; a scheme-less value is
/// treated as a bare host and normalized to `https://<value>` (MAPPS-480),
/// which is what the server's own deserializer does (PMS-805). Any other
/// scheme (`javascript:`, `data:`, `vbscript:`, `mailto:`, ...) and any value
/// carrying whitespace or control characters is still rejected with an inline
/// message *before* any request, so the user learns Website is the problem
/// instead of hitting an opaque server 422 (MAPPS-213). The scheme check
/// reuses `utils::url::scheme_of`, the same whitespace-collapsing detection
/// `safe_href` applies at render time, so `java\tscript:` cannot slip through.
fn validate_website_field(raw: &str) -> Result<serde_json::Value, String> {
    // MAPPS-582: an invisible character never belongs in a URL either, and it
    // is one more way a scheme can disguise itself from the check below.
    let cleaned = crate::utils::text::clean_strict(raw);
    let trimmed = cleaned.as_str();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    const MSG: &str =
        "Website must be a domain or http(s) URL (e.g. example.com or https://example.com).";
    // Whitespace and control characters never belong in a URL whatever the
    // scheme, and they are how `java\tscript:` disguises itself.
    if trimmed
        .chars()
        .any(|c| c.is_whitespace() || (c as u32) < 0x20)
    {
        return Err(MSG.to_string());
    }
    match crate::utils::url::scheme_of(trimmed).as_deref() {
        // Require `scheme://host` with a non-empty host, so `http://` and
        // `http:/x` are rejected.
        Some("http") | Some("https") => {
            let host = trimmed
                .split_once("://")
                .map(|(_, rest)| rest)
                .unwrap_or("")
                .split(['/', '?', '#'])
                .next()
                .unwrap_or("");
            if host.is_empty() {
                return Err(MSG.to_string());
            }
            Ok(serde_json::Value::String(trimmed.to_string()))
        }
        // Any other explicit scheme stays rejected.
        Some(_) => Err(MSG.to_string()),
        // Scheme-less: a bare host, so add the scheme the product wants and
        // keep whatever path, query or fragment was typed after it.
        None => {
            let authority = trimmed.split(['/', '?', '#']).next().unwrap_or("");
            if !is_host_like(authority) {
                return Err(MSG.to_string());
            }
            Ok(serde_json::Value::String(format!("https://{trimmed}")))
        }
    }
}

/// Whether `host` can be the host of a public web address: host-legal
/// characters only, and at least one dot with a non-empty label either side,
/// so `localhost` and `no-dot` are not silently turned into `https://` URLs
/// the server cannot resolve.
///
/// No port is accepted here, and none can reach this function: a `:` before
/// the first `/` is what `scheme_of` reads as a scheme, so `example.com:8443`
/// is rejected above as an unknown scheme. The server allows ports 80 and 443
/// only, so nothing useful is lost.
fn is_host_like(host: &str) -> bool {
    if !host
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
    {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|l| !l.is_empty())
}

/// The host of a normalized `https://...` value, for the probe note. The value
/// always carries a scheme (it came out of [`validate_website_field`]), so the
/// fallbacks below are unreachable rather than lossy.
fn website_host(normalized: &str) -> String {
    normalized
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(normalized)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(normalized)
        .to_string()
}

/// PMS-805 `GET /companies/website-probe` response. Only the fields the note
/// below renders are deserialized; the rest of the body is ignored.
///
/// `redirect_truncated` is `#[serde(default)]` because the shipped server does
/// not send it: PMS-805 specified the field but merged without it, and a chain
/// still redirecting at the hop limit comes back as `reachable: false` with
/// `unreachable_reason: "refused"` instead. The branch here is inert until the
/// server adds the field, and is tracked in MAPPS-486.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct WebsiteProbe {
    reachable: bool,
    canonical_url: Option<String>,
    #[serde(default)]
    http_redirects_to_https: bool,
    #[serde(default)]
    www_change: String,
    unreachable_reason: Option<String>,
    #[serde(default)]
    redirect_truncated: bool,
}

/// Human-readable note for a probe that answered, rendered under the field.
/// `normalized` is the value the probe was given, so an unreachable site still
/// tells the user exactly what will be saved.
fn website_probe_note(normalized: &str, probe: &WebsiteProbe) -> String {
    if !probe.reachable {
        let reason = match probe.unreachable_reason.as_deref() {
            Some("dns") => "no DNS record",
            Some("timeout") => "timeout",
            Some("tls") => "TLS error",
            Some("refused") => "connection refused",
            Some("blocked_host") => "address not reachable from the internet",
            // A reason the client does not know is still shown, never dropped.
            Some(other) => other,
            None => "no reason given",
        };
        return website_unreachable_note(normalized, reason);
    }
    let Some(canonical) = probe.canonical_url.as_deref() else {
        // Reachable with no canonical URL is not a shape the server produces;
        // say so rather than presenting the probe as having settled anything.
        return format!(
            "{} answered but reported no address. Saving as {normalized}.",
            website_host(normalized)
        );
    };
    let mut changes: Vec<&str> = Vec::new();
    if probe.http_redirects_to_https {
        changes.push("http redirects to https");
    }
    match probe.www_change.as_str() {
        "added" => changes.push("www added"),
        "removed" => changes.push("www removed"),
        _ => {}
    }
    if probe.redirect_truncated {
        changes.push("site redirects again; not followed");
    }
    if changes.is_empty() {
        format!("Resolved to {canonical}")
    } else {
        format!("Resolved to {canonical} ({})", changes.join(", "))
    }
}

/// Note for a site the probe could not resolve, and for a probe request that
/// failed outright. Names the value that will be saved either way, because the
/// probe is advisory and never blocks the form.
fn website_unreachable_note(normalized: &str, reason: &str) -> String {
    format!(
        "Could not reach {} ({reason}). Saving as {normalized}.",
        website_host(normalized)
    )
}

/// Validate an optional IANA time zone. Blank -> `Ok(None)`. A light client
/// check (must look like `Area/Location` with no spaces) that catches the
/// common `America/New York` mistake; the server (PMS-325) is authoritative.
fn validate_timezone_field(raw: &str) -> Result<serde_json::Value, String> {
    // MAPPS-582: an IANA name is ASCII, so strip the invisibles before the
    // "no spaces, has a slash" shape check.
    let cleaned = crate::utils::text::clean_strict(raw);
    let trimmed = cleaned.as_str();
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
    let mut company_resource = use_resource(move || {
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
    let mut contacts_resource = use_resource(move || {
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
            // every site renders inline, no separate list page
            // needed. MAPPS-528: paged, because the old
            // `per_page=200` was clamped to 100 by the server.
            crate::hooks::fetch::api::get_all_authed::<SiteSummary>(&format!(
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
                "/tickets?company_id={id}&per_page=5&{TICKETS_RECENT_SORT}"
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
        crate::hooks::fetch::api::get_all_authed::<AssetTypeOption>("/asset-types")
            .await
            .ok()
    });

    // Statistics counts pulled from each list envelope's `meta.total`.
    let contract_count = paginated_total(&contracts_resource);
    let project_count = paginated_total(&projects_resource);
    let invoice_count = paginated_total(&invoices_resource);
    let asset_count = paginated_total(&assets_resource);

    let company_snapshot = company_resource.read_unchecked();
    // MAPPS-278: while the record is loading, show "Loading…" instead
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
    // MAPPS-577: what a delete would actually do, from the server (PMS-926).
    // NOT derived from the Statistics card: `open_ticket_count` counts only
    // open tickets while the delete guard counts every one, so a company with
    // closed tickets and none open reads as deletable there and is refused.
    // Fetched when the dialog opens rather than on page load, because a page
    // view is not an intent to delete.
    let preview_id = company_id_str.clone();
    let deletion_preview = use_resource(move || {
        let id = preview_id.clone();
        let open = confirming_delete();
        async move {
            if !open {
                return None;
            }
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<DeletionPreview>(&format!(
                "/contacts/companies/{id}/deletion-preview"
            ))
            .await
            .ok()
        }
    });
    // MAPPS-574: why the last delete attempt was refused. The server answers a
    // blocked delete with 400 and an actionable message ("Cannot delete company
    // with existing tickets", or the PMS-170 related-records list); this holds
    // it for the dialog.
    let mut delete_error = use_signal(String::new);
    // MAPPS-577: the archive alternative offered when a delete is refused.
    let mut archiving = use_signal(|| false);
    // MAPPS-644: the billing contact, read on its own. The Contacts card
    // preview is capped, so the billing contact need not be in it. The
    // company is read INSIDE the closure so the fetch re-runs once the
    // company has loaded and again after a change.
    let billing_contact_resource = use_resource(move || async move {
        let id = company_resource
            .read_unchecked()
            .clone()
            .flatten()
            .and_then(|c| c.default_billing_contact_id)?;
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<RemoteContact>(&format!("/contacts/contacts/{id}"))
            .await
            .ok()
    });
    let mut show_set_billing = use_signal(|| false);
    // MAPPS-357: gate the destructive Delete while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        delete_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/contacts/companies/{id}");
                // MAPPS-574: a refusal is a normal outcome here, not an edge
                // case - a company with a single ticket or project cannot be
                // deleted - so it has to be reported. The previous
                // `.is_ok()` discarded the message and closed the dialog, which
                // left the user on the unchanged page with nothing to read and
                // no way to tell a refusal from a dead button. The dialog stays
                // open on failure so the reason sits next to the control that
                // produced it.
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Company deleted.",
                        );
                        confirming_delete.set(false);
                        navigator.push(Route::CompanyList {});
                    }
                    Err(err) => delete_error.set(err),
                }
            }
            deleting.set(false);
        });
    };

    // MAPPS-357: a failed load of the primary company while the server is
    // flagged down is an outage, not a missing record - render the honest
    // unavailable state instead of "Could not load company" (kept below for a
    // 4xx while still reachable).
    let fetch_failed = matches!(*company_snapshot, Some(None));
    // MAPPS-575: read here rather than inside the body's match, because the
    // banner renders above the header and outside that arm.
    let archived_banner = matches!(&*company_snapshot, Some(Some(c)) if c.status == "inactive");
    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Company".to_string() }
        };
    }

    rsx! {
        {
            let snapshot = deletion_preview.read_unchecked().clone().flatten();
            // MAPPS-577 AC7: a preview that has not arrived, or failed, must
            // not block the delete. With none, the dialog behaves exactly as it
            // did before this change: a generic warning, the phrase gate, an
            // attempt, and the server's own refusal if it is refused. The
            // delete path never depends on a second request succeeding.
            let preview = snapshot.clone().unwrap_or_default();
            let known = snapshot.is_some();
            let blocked = known && !preview.can_delete;

            // MAPPS-577 AC1: no hardcoded list of tables. Everything specific
            // comes from the server, so the two cannot drift again.
            let message = if !known {
                "Delete this company? Its sites are removed and its contacts are unlinked, and this cannot be undone."
                    .to_string()
            } else if preview.is_own_company {
                "This is your organisation's own company record, which general and overhead time is logged against. It cannot be deleted."
                    .to_string()
            } else if blocked {
                "This company cannot be deleted while these records exist.".to_string()
            } else {
                "Delete this company? This cannot be undone.".to_string()
            };

            let retained: Vec<DeletionRecords> = preview
                .blocking
                .iter()
                .filter(|r| r.retained)
                .cloned()
                .collect();
            let removable: Vec<DeletionRecords> = preview
                .blocking
                .iter()
                .filter(|r| !r.retained)
                .cloned()
                .collect();
            let effects: Vec<(String, Vec<DeletionRecords>)> = vec![
                ("Removed".to_string(), preview.removed.clone()),
                ("Unlinked, not deleted".to_string(), preview.unlinked.clone()),
            ];

            let archive_id = company_id_str.clone();
            rsx! {
                crate::components::ConfirmDialog {
                    open: confirming_delete(),
                    title: "Delete company".to_string(),
                    message,
                    confirm_text: "Delete".to_string(),
                    cancel_text: if blocked { "Close".to_string() } else { "Cancel".to_string() },
                    destructive: true,
                    // AC5: withheld while blocked, so nobody types a company
                    // name to enable a button that cannot work.
                    blocked,
                    // PMS-369: the delete unlinks and removes, so gate it behind
                    // typing the company name.
                    confirm_phrase: header_title.clone(),
                    error: delete_error.read().clone(),
                    loading: *deleting.read(),
                    body: rsx! {
                        if !retained.is_empty() {
                            div { class: "space-y-1",
                                p { class: "text-xs font-medium text-content",
                                    "Kept as a permanent record, so they are not something to clear:"
                                }
                                ul { class: "list-disc pl-5 text-sm text-muted",
                                    for row in retained.iter() {
                                        li { key: "{row.label}", "{row.count} {row.label}" }
                                    }
                                }
                            }
                        }
                        if !removable.is_empty() {
                            div { class: "mt-2 space-y-1",
                                p { class: "text-xs font-medium text-content",
                                    "Remove or reassign these first, or archive the company instead:"
                                }
                                ul { class: "list-disc pl-5 text-sm text-muted",
                                    for row in removable.iter() {
                                        li { key: "{row.label}", "{row.count} {row.label}" }
                                    }
                                }
                            }
                        }
                        // AC2: what the delete WOULD do, shown whether or not
                        // anything blocks it. On a deletable company this is
                        // the whole point of opening the dialog.
                        for (heading , rows) in effects.iter() {
                            if !rows.is_empty() {
                                div { key: "{heading}", class: "mt-2 space-y-1",
                                    p { class: "text-xs font-medium text-content", "{heading}:" }
                                    ul { class: "list-disc pl-5 text-sm text-muted",
                                        for row in rows.iter() {
                                            li { key: "{row.label}", "{row.count} {row.label}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    // AC4: the refusal already names archiving; this makes it a
                    // control rather than an instruction to go and find it.
                    // Absent on the own-company refusal, where archiving is not
                    // the answer either.
                    alternative: (blocked && !preview.is_own_company).then(|| rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            loading: *archiving.read(),
                            disabled: !can_mutate,
                            onclick: move |_| {
                                if *archiving.read() { return; }
                                archiving.set(true);
                                delete_error.set(String::new());
                                let id = archive_id.clone();
                                spawn(async move {
                                    #[cfg(feature = "app")]
                                    {
                                        let path = format!("/contacts/companies/{id}");
                                        let body = serde_json::json!({ "status": "inactive" });
                                        match crate::hooks::fetch::api::put_authed_typed::<
                                            serde_json::Value,
                                            _,
                                        >(&path, &body)
                                            .await
                                        {
                                            Ok(_) => {
                                                crate::hooks::toast::push_toast(
                                                    crate::components::AlertType::Success,
                                                    "Company archived. Its history is kept and it is out of your active lists.",
                                                );
                                                confirming_delete.set(false);
                                                let mut cr = company_resource;
                                                cr.restart();
                                            }
                                            Err(e) => delete_error
                                                .set(format!("Could not archive: {}", e.user_message())),
                                        }
                                    }
                                    #[cfg(not(feature = "app"))]
                                    let _ = &id;
                                    archiving.set(false);
                                });
                            },
                            "Archive instead"
                        }
                    }),
                    onconfirm: on_confirm_delete,
                    oncancel: move |_| {
                        if !*deleting.read() {
                            confirming_delete.set(false);
                            delete_error.set(String::new());
                        }
                    },
                }
            }
        }
        // MAPPS-575: an archived company is out of the default lists and
        // pickers, so anyone who reaches this page has arrived by a link or a
        // deliberate filter change. Say so at the top: a field in the sidebar
        // is easy to miss, and the consequence (it will not appear where they
        // expect to pick it) is not something a Status badge conveys.
        if archived_banner {
            crate::components::StatusBanner {
                tone: crate::components::BannerTone::Info,
                class: "mb-4",
                "This company is archived. Its history is kept, and it stays out of the default company list and pickers until its status is set back to Active."
            }
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
                let is_archived = company.status == "inactive";
                let status_label = match company.status.as_str() {
                    "inactive" => "Inactive (archived)",
                    "prospect" => "Prospect",
                    // Empty from an older server reads as the default the
                    // column itself carries.
                    "" | "active" => "Active",
                    other => other,
                };
                let website = company.website.clone();
                let phone = company.phone.clone();
                let industry = company.industry.clone();
                let am_name = company.account_manager_name.clone();
                // MAPPS-644: the billing contact's row, once its read lands.
                let billing_contact_row = billing_contact_resource.read_unchecked().clone().flatten();
                let notes = company.notes.clone().unwrap_or_default();
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
                                billing_contact_id: company.default_billing_contact_id,
                                contacts_resource,
                            }
                            // MAPPS-644: set or change the billing contact.
                            if show_set_billing() {
                                SetBillingContactModal {
                                    company_id: company_id_str.clone(),
                                    current_id: company.default_billing_contact_id.map(|c| c.to_string()),
                                    current_name: billing_contact_row
                                        .as_ref()
                                        .map(|r| format!("{} {}", r.first_name, r.last_name).trim().to_string())
                                        .unwrap_or_default(),
                                    onclose: move |_| show_set_billing.set(false),
                                    onsaved: move |_| {
                                        show_set_billing.set(false);
                                        company_resource.restart();
                                        contacts_resource.restart();
                                    },
                                }
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
                            // MAPPS-614: near the bottom of the record, which
                            // is where David asked for it. Rendered through
                            // the shared component, so it is sanitised by the
                            // same path as every other Markdown surface.
                            // Hidden when empty, following the ticket
                            // description's own rule, so a record nobody has
                            // written on does not grow a blank card.
                            if !notes.trim().is_empty() {
                                Card { title: "Notes",
                                    crate::components::Markdown { content: notes.clone() }
                                }
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
                                    // MAPPS-575: the field the archive lives in,
                                    // shown for every value rather than only the
                                    // archived one, so "Active" is a fact the
                                    // page states rather than an absence the
                                    // reader has to infer.
                                    div { class: "flex justify-between",
                                        dt { class: "text-sm text-muted", "Status" }
                                        dd {
                                            Badge {
                                                variant: if is_archived { BadgeVariant::Gray } else { BadgeVariant::Green },
                                                "{status_label}"
                                            }
                                        }
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
                                    // MAPPS-644: who invoices go to. Always
                                    // rendered, because "not set" is the state
                                    // that has to be visible: a send is refused
                                    // until there is one (PMS-992).
                                    div { class: "flex justify-between gap-4",
                                        dt { class: "text-sm text-muted shrink-0", "Billing Contact" }
                                        dd { class: "text-sm text-right min-w-0",
                                            match (company.default_billing_contact_id, billing_contact_row.clone()) {
                                                (None, _) => rsx! {
                                                    p { class: "text-amber-700 dark:text-amber-300", "Not set" }
                                                    p { class: "text-xs text-muted", "Invoices cannot be sent until one is set." }
                                                    Button {
                                                        variant: ButtonVariant::Link,
                                                        size: ButtonSize::Small,
                                                        onclick: move |_| show_set_billing.set(true),
                                                        "Set billing contact"
                                                    }
                                                },
                                                (Some(id), Some(row)) => {
                                                    let name = format!("{} {}", row.first_name, row.last_name).trim().to_string();
                                                    let email = row.email.clone().unwrap_or_default();
                                                    rsx! {
                                                        Link {
                                                            to: Route::ContactDetail { id: id.to_string() },
                                                            class: "text-accent hover:opacity-90",
                                                            "{name}"
                                                        }
                                                        if email.is_empty() {
                                                            p { class: "text-xs text-amber-700 dark:text-amber-300", "No email address on file, so invoices cannot be sent." }
                                                        } else {
                                                            p { class: "text-xs text-muted break-all", "{email}" }
                                                        }
                                                        Button {
                                                            variant: ButtonVariant::Link,
                                                            size: ButtonSize::Small,
                                                            onclick: move |_| show_set_billing.set(true),
                                                            "Change"
                                                        }
                                                    }
                                                }
                                                (Some(id), None) => rsx! {
                                                    Link {
                                                        to: Route::ContactDetail { id: id.to_string() },
                                                        class: "text-accent hover:opacity-90",
                                                        "View contact"
                                                    }
                                                    Button {
                                                        variant: ButtonVariant::Link,
                                                        size: ButtonSize::Small,
                                                        onclick: move |_| show_set_billing.set(true),
                                                        "Change"
                                                    }
                                                },
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
    /// MAPPS-644: who invoices are emailed to when the invoice names no
    /// contact of its own (PMS-992).
    #[serde(default)]
    default_billing_contact_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_type: String,
    #[serde(default)]
    status: String,
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
    // MAPPS-614 / PMS-952: rendered as Markdown in the Notes card.
    #[serde(default)]
    notes: Option<String>,
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
    // MAPPS-574: the server's reason for refusing this row's delete.
    let mut delete_error = use_signal(String::new);
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
        delete_error.set(String::new());
        let path = path.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                // MAPPS-574: this row menu deletes whatever `delete_path`
                // points at, so it inherits every refusal the detail pages get.
                // It used to leave the dialog open on failure with the spinner
                // simply stopped, which reads as a hung request rather than a
                // decision the server made.
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => {
                        confirming.set(false);
                        open.set(false);
                        on_deleted.call(());
                    }
                    Err(err) => delete_error.set(err),
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
                div { class: "dropdown-panel absolute right-0 top-full z-50 mt-1 w-32 py-1 flex flex-col",
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
            error: delete_error.read().clone(),
            loading: deleting(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !deleting() {
                    confirming.set(false);
                    delete_error.set(String::new());
                }
            },
        }
    }
}

#[component]
fn CompanyContactsCard(
    company_id: String,
    company_name: String,
    /// MAPPS-644: the company's billing contact, marked in its row.
    billing_contact_id: Option<uuid::Uuid>,
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
                                        // MAPPS-481: the same primary-with-its-type cell as the
                                        // contacts list, so every Phone column reads alike.
                                        let phone = primary_phone_label(
                                            &contact.phones,
                                            contact.phone.as_deref().unwrap_or_default(),
                                        );
                                        let role = humanize_contact_type(
                                            contact.contact_type.as_deref().unwrap_or_default(),
                                        );
                                        let is_billing = billing_contact_id == Some(contact.id);
                                        rsx! {
                                            TableRow { key: "{id}", class: "group",
                                                TableCell {
                                                    Link {
                                                        to: Route::ContactDetail { id: id.clone() },
                                                        class: "font-medium text-accent hover:opacity-90",
                                                        "{name}"
                                                    }
                                                    // MAPPS-644: where this company's invoices go.
                                                    if is_billing {
                                                        Badge { variant: BadgeVariant::Blue, class: "ml-2", "Billing" }
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

/// MAPPS-207: "Add Contact" modal for a company. Lets the user search and
/// select an existing contact (attaching it to this company via a PUT that
/// sets `company_id`), or fall through to the full new-contact form with
/// the company pre-filled.
/// MAPPS-644: set or change the company's billing contact, the fallback
/// recipient for every invoice that names no contact of its own (PMS-992).
///
/// Scoped to the company's contacts, because a billing contact at another
/// company would be a foreign address on this company's invoices. No clear:
/// the server leaves an absent value unchanged, and "no billing contact" is
/// the state this page exists to make visible rather than easy to return to.
#[component]
fn SetBillingContactModal(
    company_id: String,
    current_id: Option<String>,
    current_name: String,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let mut selected_id = use_signal(|| current_id.clone().unwrap_or_default());
    let mut selected_name = use_signal(|| current_name.clone());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let can_mutate = crate::hooks::use_can_mutate();

    let picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(selected_id.read().as_str()).is_ok() {
            Some(selected_id.read().clone())
        } else {
            None
        };
    let unchanged = current_id.as_deref() == Some(selected_id.read().as_str());

    let company_id_for_save = company_id.clone();
    let on_save = move |_| {
        let Ok(contact_uuid) = uuid::Uuid::parse_str(selected_id.read().as_str()) else {
            error.set("Pick a contact.".to_string());
            return;
        };
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        let path = format!("/contacts/companies/{company_id_for_save}");
        spawn(async move {
            #[cfg(feature = "app")]
            {
                // `UpdateCompanyRequest` is all-optional; a one-field PUT
                // writes only this column.
                let body = serde_json::json!({ "default_billing_contact_id": contact_uuid });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not set the billing contact: {err}")),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (path, contact_uuid);
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Billing contact".to_string(),
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
                    disabled: !can_mutate || unchanged || picker_selected_id.is_none(),
                    title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                    onclick: on_save,
                    "Save"
                }
            },
            div { class: "space-y-4",
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                crate::components::ContactPicker {
                    value: selected_name.read().clone(),
                    selected_id: picker_selected_id,
                    label: "Billing contact".to_string(),
                    required: true,
                    company_filter: Some(company_id.clone()),
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
                    "Invoices for this company are emailed to this contact unless the invoice names its own. Only contacts at this company are offered, and the contact needs an email address for a send to go through."
                }
            }
        }
    }
}

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
            #[cfg(feature = "app")]
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
    mut sites_resource: Resource<Option<Vec<SiteSummary>>>,
    // The Statistics counters (Sites, Contacts, Open Tickets) read denormalized
    // counts off `company_resource`, not the child table resources. Restart it
    // after an add so the Sites counter refreshes in the same render cycle
    // instead of staying stale until a manual reload (PMS-363).
    mut company_resource: Resource<Option<CompanyDetail>>,
) -> Element {
    let snap = sites_resource.read_unchecked();
    // MAPPS-528: every site is fetched, so the row count IS the full count
    // that feeds the collapsible header badge.
    let count = match &*snap {
        Some(Some(rows)) => Some(rows.len() as u64),
        _ => None,
    };
    let mut editing = use_signal(|| None::<SiteFormState>);

    rsx! {
        CollapsibleCard {
            title: "Sites",
            // MAPPS-597: say what a site IS, on the card that introduces them.
            // The word was read as "web sites", which this page invites by
            // carrying a Website field a few inches away. "Site" stays because
            // it is what ConnectWise, HaloPSA and Atera call this and so is the
            // word the audience already has; "Location" is taken by the
            // appointment field, and "Office" is wrong for a warehouse or a
            // datacenter. See the ticket for the rename that was rejected.
            subtitle: "Offices, warehouses and other addresses where this company operates.",
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
                    Some(Some(page)) if page.is_empty() => rsx! {
                        // MAPPS-597: an empty state is where a reader who does
                        // not know what a site is meets the word. Saying only
                        // that there are none teaches them nothing; this says
                        // what to add and why they would.
                        TableEmpty {
                            columns: 4,
                            message: "No locations recorded yet. Add the addresses you visit or support."
                                .to_string(),
                        }
                    },
                    Some(Some(page)) => {
                        // MAPPS-316: render every site the fetch
                        // returned. Sites per company are small;
                        // the previous `.take(3)` capped the user
                        // out of seeing the rest because Sites has
                        // no "View all" escape link.
                        let rows: Vec<_> = page.clone();
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
    let postal = use_signal(|| initial.postal_code.clone());
    let country = use_signal(|| initial.country.clone());
    let phone = use_signal(|| initial.phone.clone());
    let timezone = use_signal(|| initial.timezone.clone());
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
            #[cfg(feature = "app")]
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
            #[cfg(feature = "app")]
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
                // MAPPS-597: the same sentence the card carries, once, where
                // somebody is about to type an address into the form. Only on
                // the create path: an edit already has the answer in front of it.
                if !is_edit {
                    p { class: "text-sm text-muted",
                        "A site is an office, warehouse or other address where this company operates."
                    }
                }
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
                        oninput: clear_on_edit(postal, postal_err),
                    }
                    crate::components::Input {
                        name: "site_country",
                        label: "Country",
                        placeholder: "US",
                        value: country.read().clone(),
                        error: country_err(),
                        oninput: clear_on_edit(country, country_err),
                    }
                    crate::components::Input {
                        name: "site_phone",
                        label: "Phone",
                        value: phone.read().clone(),
                        error: phone_err(),
                        oninput: clear_on_edit(phone, phone_err),
                    }
                    crate::components::Input {
                        name: "site_timezone",
                        label: "Timezone",
                        placeholder: "e.g. America/New_York",
                        value: timezone.read().clone(),
                        error: tz_err(),
                        oninput: clear_on_edit(timezone, tz_err),
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
    // MAPPS-639: the company's account over a period, seeded with this
    // company the same way View All is.
    let statement_href = format!(
        "/statements?company_id={}",
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
                    href: "{statement_href}",
                    class: "text-sm text-accent hover:opacity-90",
                    "Statement"
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
    asset_types_resource: Resource<Option<Vec<AssetTypeOption>>>,
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
                Some(Some(types)) => types
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
                        TableHeader { "Company" }
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
                            // MAPPS-481: the Company cell shows the primary
                            // link plus a "+N" for the rest, and the Phone cell
                            // the primary number with its type. Both fall back
                            // to the `company_id` / `phone` mirrors, which is
                            // also the freeform-company case (no link at all).
                            {
                                let primary_company = primary_entry(&contact.companies, |c| c.is_primary);
                                let company = primary_company
                                    .and_then(|c| c.company_name.clone())
                                    .or_else(|| contact.company_name.clone())
                                    .unwrap_or_default();
                                let company_id = primary_company
                                    .and_then(|c| c.company_id)
                                    .or(contact.company_id)
                                    .map(|id| id.to_string())
                                    .unwrap_or_default();
                                rsx! {
                                    ContactRow {
                                        key: "{contact.id}",
                                        id: contact.id.to_string(),
                                        name: format!("{} {}", contact.first_name, contact.last_name).trim().to_string(),
                                        company,
                                        company_id,
                                        company_extra: extra_company_suffix(contact.companies.len()),
                                        email: contact.email.clone().unwrap_or_default(),
                                        phone: primary_phone_label(
                                            &contact.phones,
                                            contact.phone.as_deref().unwrap_or_default(),
                                        ),
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
    }
}

#[derive(Props, Clone, PartialEq)]
struct ContactRowProps {
    id: String,
    name: String,
    company: String,
    company_id: String,
    /// MAPPS-481: "+N" for the company links beyond the one shown; empty when
    /// the contact links at most one.
    company_extra: String,
    email: String,
    /// MAPPS-481: the primary number already rendered with its type and
    /// separators, because the cell shows one of a list rather than a field.
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
                    // MAPPS-484: say the name is a typed string, not a company
                    // record. Link colour alone was the only signal before.
                    if !props.company.is_empty() {
                        span { class: "text-subtle ml-1", "(typed)" }
                    }
                }
                // MAPPS-481: the contact links more companies than the one
                // shown; the detail page lists them all.
                if !props.company_extra.is_empty() {
                    span { class: "text-subtle ml-1", "{props.company_extra}" }
                }
            }
            TableCell { "{props.email}" }
            // MAPPS-481: the primary number with its type, already formatted
            // by `primary_phone_label` (which keeps the MAPPS-283 separators).
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

    // MAPPS-481: the prefill seeds the first row of the company list rather
    // than a single company field.
    let initial = ContactFormValues {
        companies: company_rows_from_remote(
            &[],
            uuid::Uuid::parse_str(&prefill.id).ok(),
            Some(&prefill.name),
        ),
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
    #[cfg(feature = "app")]
    {
        if let Some(search) = crate::platform::location::search() {
            {
                let params = crate::utils::url::QueryString::parse(&search);
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
                // MAPPS-481: the child lists round-trip; the freeform name is
                // seeded only when the contact links no company, which is the
                // one case it can be set in (a link plus a freeform name is a
                // 422 on the server).
                let companies = company_rows_from_remote(
                    &payload.companies,
                    payload.company_id,
                    payload.company_name.as_deref(),
                );
                let initial = ContactFormValues {
                    first_name: payload.first_name.clone(),
                    last_name: payload.last_name.clone(),
                    email: payload.email.clone().unwrap_or_default(),
                    title: payload.title.clone().unwrap_or_default(),
                    department: payload.department.clone().unwrap_or_default(),
                    contact_type: if payload.contact_type.is_empty() {
                        "other".to_string()
                    } else {
                        payload.contact_type.clone()
                    },
                    company_name: if companies.is_empty() {
                        payload.company_name.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                    phones: phone_rows_from_remote(
                        &payload.phones,
                        payload.phone.as_deref(),
                        payload.mobile.as_deref(),
                    ),
                    companies,
                    notes: payload.notes.clone().unwrap_or_default(),
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
    // MAPPS-614 / PMS-952: the free-text note, held and rendered as Markdown.
    #[serde(default)]
    notes: Option<String>,
    // MAPPS-251: optional so a freeform-company contact (company_name only,
    // no FK) deserializes without a null/absent company_id panicking.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: Option<String>,
    // MAPPS-481: `#[serde(default)]` so a pre-PMS-806 response still decodes
    // and the form falls back to the scalar mirrors above.
    #[serde(default)]
    phones: Vec<RemotePhone>,
    #[serde(default)]
    companies: Vec<RemoteCompanyLink>,
}

/// MAPPS-481: one editable row of the contact form's phone list. `error` is
/// the row's own inline slot, so one bad number never masks another's message
/// (see the repeating-child-row rules in `docs/form-conventions.md`).
#[derive(Clone, Debug, Default, PartialEq)]
struct PhoneRow {
    phone_type: String,
    number: String,
    extension: String,
    is_primary: bool,
    error: String,
}

/// MAPPS-481: one editable row of the contact form's company list. Always a
/// linked CRM company; the freeform typed name is the no-linked-company case
/// and lives in its own signal.
#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyRow {
    company_id: String,
    company_name: String,
    title: String,
    is_primary: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ContactFormValues {
    first_name: String,
    last_name: String,
    email: String,
    title: String,
    department: String,
    contact_type: String,
    /// MAPPS-481: the freeform typed company name (MAPPS-251), which is the
    /// no-linked-company case and so is only ever set while `companies` is
    /// empty.
    company_name: String,
    phones: Vec<PhoneRow>,
    companies: Vec<CompanyRow>,
    notes: String,
}

/// MAPPS-481: seed the form's phone rows from a loaded contact. The PMS-806
/// list wins; a response that carries none (a server that predates it) falls
/// back to the `phone` / `mobile` mirrors so an edit does not silently drop
/// the numbers already on the record.
fn phone_rows_from_remote(
    phones: &[RemotePhone],
    phone: Option<&str>,
    mobile: Option<&str>,
) -> Vec<PhoneRow> {
    if !phones.is_empty() {
        return phones
            .iter()
            .map(|p| PhoneRow {
                phone_type: normalize_phone_type(&p.phone_type).to_string(),
                number: p.number.clone(),
                extension: p.extension.clone().unwrap_or_default(),
                is_primary: p.is_primary,
                error: String::new(),
            })
            .collect();
    }
    let mut rows = Vec::new();
    for (value, phone_type) in [(phone, "work"), (mobile, "mobile")] {
        let number = value.unwrap_or_default().trim();
        if !number.is_empty() {
            rows.push(PhoneRow {
                phone_type: phone_type.to_string(),
                number: number.to_string(),
                is_primary: rows.is_empty(),
                ..PhoneRow::default()
            });
        }
    }
    rows
}

/// MAPPS-481: seed the form's company rows from a loaded contact. As with
/// phones, the PMS-806 list wins and the single `company_id` mirror is the
/// fallback. A freeform-only contact has neither and stays on the typed-name
/// path.
fn company_rows_from_remote(
    companies: &[RemoteCompanyLink],
    company_id: Option<uuid::Uuid>,
    company_name: Option<&str>,
) -> Vec<CompanyRow> {
    if !companies.is_empty() {
        return companies
            .iter()
            .filter_map(|c| {
                Some(CompanyRow {
                    company_id: c.company_id?.to_string(),
                    company_name: c.company_name.clone().unwrap_or_default(),
                    title: c.title.clone().unwrap_or_default(),
                    is_primary: c.is_primary,
                })
            })
            .collect();
    }
    match company_id {
        Some(id) => vec![CompanyRow {
            company_id: id.to_string(),
            company_name: company_name.unwrap_or_default().to_string(),
            title: String::new(),
            is_primary: true,
        }],
        None => Vec::new(),
    }
}

/// MAPPS-481: validate every phone row and build the request's `phones`
/// array. Evaluates ALL rows before bailing and returns one message per row
/// (empty where the row passed), so no row's failure masks another's, per the
/// every-required-field rule in `docs/form-conventions.md`.
///
/// A row whose number is blank is dropped rather than rejected: an added row
/// the user left empty is not a number, and a contact with none is valid.
/// The surviving entries keep their form order, which is the `sort_order`
/// PMS-806 derives from the array index, and exactly one carries
/// `is_primary` - the flagged row, or the first when none is flagged.
fn validate_phone_rows(rows: &[PhoneRow]) -> Result<Vec<serde_json::Value>, Vec<String>> {
    let mut errors = vec![String::new(); rows.len()];
    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut primary_at: Option<usize> = None;

    for (index, row) in rows.iter().enumerate() {
        // The message lands in this row's own slot, so "Number" is the whole
        // label the reader needs.
        match validate_phone_field(&row.number, "Number") {
            Err(message) => errors[index] = message,
            // Blank row: nothing to send.
            Ok(serde_json::Value::Null) => {}
            Ok(number) => {
                if row.is_primary && primary_at.is_none() {
                    primary_at = Some(entries.len());
                }
                entries.push(serde_json::json!({
                    "phone_type": normalize_phone_type(&row.phone_type),
                    "number": number,
                    "extension": optional_string(&row.extension),
                    "is_primary": false,
                }));
            }
        }
    }

    if errors.iter().any(|e| !e.is_empty()) {
        return Err(errors);
    }
    // Send the promotion explicitly instead of leaning on the server's
    // promote-the-first rule, so the saved primary is the one the form shows.
    if let Some(index) = primary_at.or(if entries.is_empty() { None } else { Some(0) }) {
        entries[index]["is_primary"] = serde_json::json!(true);
    }
    Ok(entries)
}

/// MAPPS-481: build the request's `companies` array from the form's rows,
/// applying the same single-primary rule as [`validate_phone_rows`]. Rows
/// carry an already-picked company id, so there is nothing left to validate.
fn company_link_entries(rows: &[CompanyRow]) -> Vec<serde_json::Value> {
    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut primary_at: Option<usize> = None;
    for row in rows {
        if row.is_primary && primary_at.is_none() {
            primary_at = Some(entries.len());
        }
        entries.push(serde_json::json!({
            "company_id": row.company_id,
            "title": optional_string(&row.title),
            "is_primary": false,
        }));
    }
    if let Some(index) = primary_at.or(if entries.is_empty() { None } else { Some(0) }) {
        entries[index]["is_primary"] = serde_json::json!(true);
    }
    entries
}

/// MAPPS-484: the contact form's two company paths, each named for what it
/// does. The old freeform toggle read "+ Add Company", which is a create
/// label on a control that creates nothing; the picker's own "+ New company"
/// button is the create affordance now.
const FREEFORM_TOGGLE_LABEL: &str = "Enter a name without creating a company";
const LINK_COMPANY_TOGGLE_LABEL: &str = "Link an existing company";

/// MAPPS-484: marks a company name that is a typed string rather than a
/// `companies` row, so link colour is not the only signal.
const FREEFORM_COMPANY_NOTE: &str = "Typed name - not a company record.";

/// MAPPS-481: the label on the control that appends a company LINK. Now that
/// a contact holds several companies, "add" means "add another company" and
/// nothing else; the picker's own "+ New company" button stays the only
/// control on the form that creates a `companies` row (MAPPS-484).
fn add_company_label(linked: usize) -> &'static str {
    if linked == 0 {
        "Add a company"
    } else {
        "Add another company"
    }
}

/// MAPPS-484: the consequence of the typed-name path, stated in the value the
/// user typed. Empty while nothing has been typed, so the form renders no note.
fn freeform_company_note(value: &str) -> String {
    let name = value.trim();
    if name.is_empty() {
        String::new()
    } else {
        format!("Saved as a typed name. {name} will not appear under Companies.")
    }
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
    let first_name = use_signal(|| initial.first_name.clone());
    let last_name = use_signal(|| initial.last_name.clone());
    let email = use_signal(|| initial.email.clone());
    let mut title = use_signal(|| initial.title.clone());
    let mut department = use_signal(|| initial.department.clone());
    let mut contact_type = use_signal(|| {
        if initial.contact_type.is_empty() {
            "other".to_string()
        } else {
            initial.contact_type.clone()
        }
    });
    // MAPPS-481: the two child collections. Row order is the order the server
    // stores (it derives `sort_order` from the array index).
    let mut phones = use_signal(|| initial.phones.clone());
    let mut companies = use_signal(|| initial.companies.clone());
    let mut notes = use_signal(|| initial.notes.clone());
    // MAPPS-481: the "+ Add another company" picker, shown only while the user
    // is adding one, and the inline note for picking one already in the list.
    let mut adding_company = use_signal(|| false);
    let mut company_add_note = use_signal(String::new);
    // MAPPS-251: a contact's company can be a freeform typed name instead of an
    // FK-linked CRM company. MAPPS-481: that path is the no-linked-company
    // case, so it opens only when the loaded contact links none and carries a
    // typed name.
    let initial_freeform = initial.companies.is_empty() && !initial.company_name.trim().is_empty();
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
    // Per-field inline validation errors (MAPPS-177, MAPPS-265). Phone errors
    // live on their own row (MAPPS-481), not in a shared slot.
    let mut first_name_err = use_signal(String::new);
    let mut last_name_err = use_signal(String::new);
    let mut email_err = use_signal(String::new);

    let type_options = vec![
        SelectOption::new("primary", "Primary"),
        SelectOption::new("technical", "Technical"),
        SelectOption::new("billing", "Billing"),
        SelectOption::new("other", "Other"),
    ];
    let phone_type_options: Vec<SelectOption> = PHONE_TYPES
        .iter()
        .map(|(value, label)| SelectOption::new(*value, *label))
        .collect();

    let navigator = use_navigator();
    // MAPPS-357: block the Create / Save submit while the server is
    // unreachable. Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();
    let submit_label = match &mode {
        ContactFormMode::Create => "Create Contact",
        ContactFormMode::Edit { .. } => "Save Changes",
    };
    // MAPPS-423: Cancel returns to what the user was editing, not to the list.
    let cancel_route = match &mode {
        ContactFormMode::Create => Route::ContactList {},
        ContactFormMode::Edit { id } => Route::ContactDetail { id: id.clone() },
    };

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(String::new());
        first_name_err.set(String::new());
        last_name_err.set(String::new());
        email_err.set(String::new());
        // MAPPS-481: clear every phone row's own slot, same as the fixed
        // fields it replaced.
        for row in phones.write().iter_mut() {
            row.error.clear();
        }

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

        // MAPPS-251 / MAPPS-481: company is optional - any number of linked CRM
        // companies OR a freeform typed name, never both (the server 422s on
        // the pair). The XOR error has no inline slot, so it goes to the
        // banner; note_invalid blocks and ties focus to the freeform input.
        let company_rows = companies.read().clone();
        let freeform_name = freeform_company.read().trim().to_string();
        if !company_rows.is_empty() && !freeform_name.is_empty() {
            error.set("Link companies or type a name, not both.".to_string());
            guard.note_invalid(Some("company_name_freeform"));
        }

        // MAPPS-481: every phone row is validated and every failure lands in
        // that row's own slot before the submit bails once.
        let phone_rows = phones.read().clone();
        let phone_entries = match validate_phone_rows(&phone_rows) {
            Ok(entries) => Some(entries),
            Err(messages) => {
                let mut rows = phones.write();
                for (index, message) in messages.iter().enumerate() {
                    if message.is_empty() {
                        continue;
                    }
                    rows[index].error = message.clone();
                    guard.note_invalid(Some(&format!("phone_number_{index}")));
                }
                None
            }
        };

        if guard.blocked() {
            return;
        }
        let phone_entries = phone_entries.expect("phone rows validated above");
        let company_entries = company_link_entries(&company_rows);
        is_submitting.set(true);

        let has_links = !company_entries.is_empty();
        let body = serde_json::json!({
            "first_name": first_name.read().trim(),
            "last_name": last_name.read().trim(),
            "email": optional_string(&email.read()),
            "title": optional_string(&title.read()),
            "department": optional_string(&department.read()),
            "contact_type": contact_type.read().clone(),
            // PMS-806: both lists are authoritative when present, and both are
            // always sent, so removing the last row really unlinks.
            "phones": phone_entries,
            "companies": company_entries,
            // MAPPS-251: the freeform typed name is the no-linked-company case.
            // Sent as `""` whenever a company is linked, which clears any name
            // stored by an earlier save (a link plus a name is a 422).
            "company_name": if has_links { "" } else { freeform_name.as_str() },
            // MAPPS-614: always a string, never null, for the same reason
            // `company_name` above is. See `clearable_string`.
            "notes": clearable_string(&notes.read()),
        });
        let mode = mode.clone();
        let mode_for_toast = mode.clone();
        spawn(async move {
            #[cfg(feature = "app")]
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
                                // MAPPS-481: PMS-806 names a bad entry
                                // `phones[i].number`, so route it to that row's
                                // own slot rather than the banner.
                                if let Some(index) = phone_row_index(&fe.field) {
                                    let mut rows = phones.write();
                                    if let Some(row) = rows.get_mut(index) {
                                        row.error = fe.message.clone();
                                        continue;
                                    }
                                }
                                match fe.field.as_str() {
                                    "first_name" => first_name_err.set(fe.message.clone()),
                                    "last_name" => last_name_err.set(fe.message.clone()),
                                    "email" => email_err.set(fe.message.clone()),
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

    // MAPPS-481: snapshots, so the rsx below holds no signal borrow while a
    // row handler writes back into the same signal.
    let phone_rows = phones.read().clone();
    let company_rows = companies.read().clone();

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
                        oninput: clear_on_edit(first_name, first_name_err),
                    }
                    crate::components::Input {
                        name: "last_name",
                        label: "Last Name",
                        required: true,
                        value: last_name.read().clone(),
                        error: last_name_err(),
                        oninput: clear_on_edit(last_name, last_name_err),
                    }
                    crate::components::Input {
                        name: "email",
                        label: "Email",
                        r#type: "email",
                        value: email.read().clone(),
                        error: email_err(),
                        oninput: clear_on_edit(email, email_err),
                    }
                    crate::components::SuggestInput {
                        name: "title",
                        label: "Title",
                        field: "title",
                        help: "Free text. Suggestions are titles already used in your workspace.",
                        value: title.read().clone(),
                        oninput: move |v: String| title.set(v),
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

                // MAPPS-481: the phone list. Any number of typed numbers, each
                // with its own type, extension, primary radio and remove
                // control, and each with its own error slot so one bad number
                // never masks another's message. Zero rows is valid.
                fieldset { class: "space-y-2",
                    legend { class: "block text-sm font-medium text-content", "Phone Numbers" }
                    if phone_rows.is_empty() {
                        p { class: "text-xs text-muted", "No phone numbers yet. A contact can be saved without one." }
                    }
                    for (index, row) in phone_rows.iter().cloned().enumerate() {
                        div {
                            key: "{index}",
                            class: "rounded-md border border-line p-3 space-y-2",
                            div { class: "grid grid-cols-1 gap-3 sm:grid-cols-12",
                                div { class: "sm:col-span-3",
                                    Select {
                                        name: "phone_type_{index}",
                                        label: "Type",
                                        options: phone_type_options.clone(),
                                        value: row.phone_type.clone(),
                                        onchange: move |e: FormEvent| {
                                            phones.write()[index].phone_type = e.value();
                                        },
                                    }
                                }
                                div { class: "sm:col-span-6",
                                    crate::components::Input {
                                        name: "phone_number_{index}",
                                        label: "Number",
                                        value: row.number.clone(),
                                        error: row.error.clone(),
                                        oninput: move |e: FormEvent| {
                                            let mut rows = phones.write();
                                            rows[index].number = e.value();
                                            rows[index].error.clear();
                                        },
                                    }
                                }
                                div { class: "sm:col-span-3",
                                    crate::components::Input {
                                        name: "phone_extension_{index}",
                                        label: "Extension",
                                        maxlength: 20,
                                        value: row.extension.clone(),
                                        oninput: move |e: FormEvent| {
                                            phones.write()[index].extension = e.value();
                                        },
                                    }
                                }
                            }
                            div { class: "flex items-center justify-between",
                                label { class: "flex items-center gap-2 text-sm text-content",
                                    input {
                                        r#type: "radio",
                                        name: "phone_primary",
                                        checked: row.is_primary,
                                        // Single-select: marking one row primary
                                        // clears the flag on every other row.
                                        onchange: move |_| {
                                            let mut rows = phones.write();
                                            for (i, r) in rows.iter_mut().enumerate() {
                                                r.is_primary = i == index;
                                            }
                                        },
                                    }
                                    "Primary"
                                }
                                crate::components::IconButton {
                                    label: "Remove phone number".to_string(),
                                    class: "p-1 text-subtle hover:text-red-600 dark:hover:text-red-400".to_string(),
                                    onclick: move |_| { phones.write().remove(index); },
                                    crate::components::TrashIcon { size: IconSize::Small }
                                }
                            }
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Small,
                        onclick: move |_| {
                            // The first number added is the primary one; after
                            // that the user picks.
                            let is_first = phones.read().is_empty();
                            phones.write().push(PhoneRow {
                                phone_type: "mobile".to_string(),
                                is_primary: is_first,
                                ..PhoneRow::default()
                            });
                        },
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Add phone number"
                    }
                }

                // MAPPS-251: company is optional and can be entered two ways -
                // link CRM companies (the picker) or type a name that creates
                // no `companies` row. Switching modes clears the other mode's
                // value so only one company source is ever submitted.
                // MAPPS-484: the picker's own "+ New company" button is the
                // visible create affordance and it really does create; the
                // typed-name path is a secondary text link named for what it
                // does, because the old "+ Add Company" button created nothing.
                // MAPPS-481: a contact can link several companies, one of them
                // primary, each with its role at THAT company. The typed name
                // is the no-linked-company case and is offered only while the
                // list is empty.
                fieldset { class: "space-y-2",
                    legend { class: "block text-sm font-medium text-content", "Companies" }
                    for (index, row) in company_rows.iter().cloned().enumerate() {
                        div {
                            key: "{row.company_id}",
                            class: "rounded-md border border-line p-3 space-y-2",
                            div { class: "flex items-center justify-between gap-3",
                                span { class: "text-sm font-medium text-content", "{row.company_name}" }
                                crate::components::IconButton {
                                    label: "Remove company link".to_string(),
                                    class: "p-1 text-subtle hover:text-red-600 dark:hover:text-red-400".to_string(),
                                    onclick: move |_| {
                                        companies.write().remove(index);
                                        company_add_note.set(String::new());
                                    },
                                    crate::components::TrashIcon { size: IconSize::Small }
                                }
                            }
                            crate::components::Input {
                                name: "company_title_{index}",
                                label: "Title at this company",
                                maxlength: 100,
                                help: "Optional. Leave blank to use the contact's own title.".to_string(),
                                value: row.title.clone(),
                                oninput: move |e: FormEvent| {
                                    companies.write()[index].title = e.value();
                                },
                            }
                            label { class: "flex items-center gap-2 text-sm text-content",
                                input {
                                    r#type: "radio",
                                    name: "company_primary",
                                    checked: row.is_primary,
                                    onchange: move |_| {
                                        let mut rows = companies.write();
                                        for (i, r) in rows.iter_mut().enumerate() {
                                            r.is_primary = i == index;
                                        }
                                    },
                                }
                                "Primary"
                            }
                        }
                    }
                    if *freeform_mode.read() {
                        crate::components::Input {
                            name: "company_name_freeform",
                            value: freeform_company.read().clone(),
                            oninput: move |e: FormEvent| freeform_company.set(e.value()),
                        }
                        // MAPPS-484: state the outcome in the user's own value
                        // instead of the old "not linked to a CRM company record"
                        // jargon. Empty until something is typed.
                        if !freeform_company_note(&freeform_company.read()).is_empty() {
                            p { class: "text-xs text-muted",
                                {freeform_company_note(&freeform_company.read())}
                            }
                        }
                    } else if *adding_company.read() {
                        crate::components::CompanyPicker {
                            value: String::new(),
                            selected_id: None,
                            // MAPPS-251: company is no longer mandatory; a contact
                            // can be saved with no company at all.
                            required: false,
                            label: String::new(),
                            // PMS-352: keep the inline "+ Create new company"
                            // affordance for first-time tenants with zero companies;
                            // distinct from the freeform path, it materializes a real
                            // `companies` row.
                            allow_inline_create: true,
                            // MAPPS-484: and surface it as a button beside the
                            // input, so a user with nothing to search for does
                            // not have to open the dropdown to find it.
                            show_create_button: true,
                            onselect: move |(id, name): (String, String)| {
                                // Picking a company already linked is a no-op
                                // that says so, not a duplicate row (the server
                                // 422s on a repeated company_id anyway).
                                if companies.read().iter().any(|c| c.company_id == id) {
                                    company_add_note.set(format!("{name} is already linked to this contact."));
                                    return;
                                }
                                let is_first = companies.read().is_empty();
                                companies.write().push(CompanyRow {
                                    company_id: id,
                                    company_name: name,
                                    title: String::new(),
                                    is_primary: is_first,
                                });
                                // Linking a company clears the typed name: the
                                // server rejects a link plus a freeform name.
                                freeform_company.set(String::new());
                                company_add_note.set(String::new());
                                adding_company.set(false);
                            },
                            onclear: move |_| { company_add_note.set(String::new()); },
                        }
                    }
                    if !company_add_note.read().is_empty() {
                        p { class: "text-xs text-muted", role: "status", "{company_add_note}" }
                    }
                    div { class: "flex flex-wrap items-center gap-3",
                        if !*freeform_mode.read() {
                            if *adding_company.read() {
                                // The picker is open with nothing picked yet,
                                // so there is a way back out of it.
                                button {
                                    r#type: "button",
                                    class: "inline-flex items-center text-xs text-accent hover:opacity-90",
                                    onclick: move |_| {
                                        company_add_note.set(String::new());
                                        adding_company.set(false);
                                    },
                                    "Don't add a company"
                                }
                            } else {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    size: ButtonSize::Small,
                                    onclick: move |_| {
                                        company_add_note.set(String::new());
                                        adding_company.set(true);
                                    },
                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                    {add_company_label(company_rows.len())}
                                }
                            }
                        }
                        // MAPPS-481: the typed name is what a contact has
                        // INSTEAD of a link, so the path disappears once one
                        // company is linked.
                        if company_rows.is_empty() {
                            button {
                                r#type: "button",
                                class: "inline-flex items-center text-xs text-accent hover:opacity-90",
                                onclick: move |_| {
                                    let next = !*freeform_mode.read();
                                    if next {
                                        adding_company.set(false);
                                    } else {
                                        freeform_company.set(String::new());
                                    }
                                    company_add_note.set(String::new());
                                    freeform_mode.set(next);
                                },
                                if *freeform_mode.read() {
                                    {LINK_COMPANY_TOGGLE_LABEL}
                                } else {
                                    {FREEFORM_TOGGLE_LABEL}
                                }
                            }
                        }
                    }
                }

                // MAPPS-614: the same field and the same editor as the company
                // form, because David asked for description-type fields across
                // the system rather than for one record.
                crate::components::MarkdownEditor {
                    name: "contact_notes".to_string(),
                    label: "Notes".to_string(),
                    placeholder: "Anything worth knowing about this person.".to_string(),
                    rows: 8,
                    views: true,
                    view_pref_key: "contact_notes_view_mode".to_string(),
                    disabled: !can_mutate,
                    value: notes.read().clone(),
                    oninput: move |next: String| notes.set(next),
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: cancel_route.clone(),
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

/// MAPPS-484: the part of the created company's `CompanyResponse` the
/// "Create this company" recovery needs to link the contact. Serde drops
/// the rest.
#[derive(Clone, Debug, Deserialize)]
struct CreatedCompanyRef {
    id: uuid::Uuid,
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
    // MAPPS-484: the id the "Create this company" recovery links the new
    // company to.
    let id_for_create_company = contact_id_str.clone();

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
                "/tickets?contact_id={id}&per_page=5&{TICKETS_RECENT_SORT}"
            ))
            .await
            .ok()
        }
    });

    let snap = contact.read_unchecked();
    // MAPPS-278: prefer an honest "Loading…" over the generic entity
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
    // MAPPS-484: state for "Create this company", the recovery path for a
    // contact whose company is a typed name.
    let mut creating_company = use_signal(|| false);
    let mut create_company_error = use_signal(String::new);
    // Holds the company created by a first attempt whose link then failed, so
    // the retry links that row instead of creating a duplicate.
    let mut created_company_id = use_signal(String::new);
    let edit_id = id_for_edit.clone();
    let delete_id = id_for_delete.clone();
    let mut confirming_delete = use_signal(|| false);
    // MAPPS-574: same swallow the company delete had - hold the server's reason
    // instead of discarding it.
    let mut delete_error = use_signal(String::new);
    // MAPPS-357: gate the destructive Delete while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();
    let on_confirm_delete = move |_: ()| {
        if *deleting.read() {
            return;
        }
        let id = delete_id.clone();
        deleting.set(true);
        delete_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/contacts/contacts/{id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Contact deleted.",
                        );
                        confirming_delete.set(false);
                        navigator.push(Route::ContactList {});
                    }
                    Err(err) => delete_error.set(err),
                }
            }
            deleting.set(false);
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
            error: delete_error.read().clone(),
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                    delete_error.set(String::new());
                }
            },
        }
        PageHeader {
            title: "{header_title}",
            // PMS-746: a route back to the list, matching CompanyDetailPage.
            // The trail stays flat (`Contacts > <name>`) even though a contact
            // also belongs to a company: a company-aware parent would have to
            // depend on how the page was reached, which is a separate change.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Contacts", Route::ContactList {}, &header_title),
                }
            },
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
                let notes = c.notes.clone().unwrap_or_default();
                let is_portal_user = c.is_portal_user;
                let portal_id = id_for_portal.clone();
                // MAPPS-481: every phone and every company link, each as its
                // own row. Empty lists keep the pre-PMS-806 scalar rendering
                // below, which is also the freeform-company path (MAPPS-484).
                let phone_entries = c.phones.clone();
                let company_entries = c.companies.clone();
                // MAPPS-484: turn a typed company name into a real `companies`
                // row and link the contact to it. Two calls, each reported: the
                // create, then the link. A create that succeeds with a failed
                // link says so, naming both facts, so the half-state is never
                // silent (the server clears the stored freeform name itself
                // once `company_id` is set).
                let create_name = company_name.clone();
                let create_contact_id = id_for_create_company.clone();
                let on_create_company = move |_| {
                    if *creating_company.read() {
                        return;
                    }
                    let name = create_name.clone();
                    let contact_id = create_contact_id.clone();
                    creating_company.set(true);
                    create_company_error.set(String::new());
                    spawn(async move {
                        #[cfg(feature = "app")]
                        {
                            // A retry after a failed link must link the company
                            // the first attempt created, not create a second one
                            // under the same name.
                            let already_created = created_company_id.read().clone();
                            let company_id = if already_created.is_empty() {
                                let body = serde_json::json!({ "name": name });
                                match crate::hooks::fetch::api::post_authed::<CreatedCompanyRef, _>(
                                    "/contacts/companies",
                                    &body,
                                )
                                .await
                                {
                                    Ok(created) => {
                                        let id = created.id.to_string();
                                        created_company_id.set(id.clone());
                                        id
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            "creating company \"{name}\" from contact {contact_id} failed: {err}"
                                        );
                                        create_company_error
                                            .set(format!("Could not create {name}: {err}"));
                                        creating_company.set(false);
                                        return;
                                    }
                                }
                            } else {
                                already_created
                            };
                            let path = format!("/contacts/contacts/{contact_id}");
                            let link_body = serde_json::json!({ "company_id": company_id });
                            match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                                &path, &link_body,
                            )
                            .await
                            {
                                Ok(_) => {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Success,
                                        "Company created and linked.",
                                    );
                                    contact.restart();
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        "linking contact {contact_id} to new company {company_id} failed: {err}"
                                    );
                                    create_company_error.set(format!(
                                        "{name} was created under Companies, but this contact still needs linking: {err}"
                                    ));
                                }
                            }
                        }
                        creating_company.set(false);
                    });
                };
                rsx! {
                    div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                        div { class: "lg:col-span-2 space-y-6",
                            ContactTicketsCard { tickets_resource: tickets }
                            // MAPPS-614: near the bottom of the record, the
                            // Google Contacts placement David described.
                            // Hidden when empty, like the company card.
                            if !notes.trim().is_empty() {
                                Card { title: "Notes",
                                    crate::components::Markdown { content: notes.clone() }
                                }
                            }
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
                                    // MAPPS-481: one row per phone, labelled
                                    // with its type and marking the primary.
                                    if !phone_entries.is_empty() {
                                        for (index, entry) in phone_entries.iter().cloned().enumerate() {
                                            div { key: "{index}",
                                                dt { class: "text-sm text-muted",
                                                    {humanize_phone_type(&entry.phone_type)}
                                                    if entry.is_primary {
                                                        span { class: "text-subtle ml-1", "(primary)" }
                                                    }
                                                }
                                                // MAPPS-283: render with separators.
                                                dd { class: "mt-1",
                                                    {format_phone_entry(&entry.number, entry.extension.as_deref())}
                                                }
                                            }
                                        }
                                    } else {
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
                                    // MAPPS-481: one row per linked company,
                                    // each with its role at THAT company and
                                    // the primary marked.
                                    if !company_entries.is_empty() {
                                        for (index, link) in company_entries.iter().cloned().enumerate() {
                                            div { key: "{index}",
                                                dt { class: "text-sm text-muted",
                                                    "Company"
                                                    if link.is_primary {
                                                        span { class: "text-subtle ml-1", "(primary)" }
                                                    }
                                                }
                                                dd { class: "mt-1",
                                                    if let Some(cid) = link.company_id {
                                                        Link {
                                                            to: Route::CompanyDetail { id: cid.to_string() },
                                                            class: "text-accent hover:opacity-90",
                                                            {link.company_name.clone().unwrap_or_default()}
                                                        }
                                                    } else {
                                                        span { class: "text-content",
                                                            {link.company_name.clone().unwrap_or_default()}
                                                        }
                                                    }
                                                    if let Some(role) = link.title.clone().filter(|t| !t.trim().is_empty()) {
                                                        p { class: "text-xs text-subtle", "{role}" }
                                                    }
                                                }
                                            }
                                        }
                                    } else if !company_name.is_empty() {
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
                                                    // MAPPS-484: say what the name is, and
                                                    // offer the one click that turns it into
                                                    // a real `companies` row. Before this the
                                                    // only way out was retyping the name on
                                                    // the Companies form.
                                                    p { class: "text-xs text-subtle", {FREEFORM_COMPANY_NOTE} }
                                                    if !create_company_error.read().is_empty() {
                                                        p {
                                                            class: "text-xs text-red-600 dark:text-red-400 mt-1",
                                                            role: "alert",
                                                            "{create_company_error}"
                                                        }
                                                    }
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        size: ButtonSize::Small,
                                                        class: "mt-2".to_string(),
                                                        loading: *creating_company.read(),
                                                        disabled: !can_mutate,
                                                        title: (!can_mutate).then(|| "Can't create a company while the server is unreachable".to_string()),
                                                        onclick: on_create_company,
                                                        "Create this company"
                                                    }
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
    // MAPPS-614 / PMS-952: rendered as Markdown in the Notes card.
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_portal_user: bool,
    // MAPPS-251: optional FK; `None` for a freeform-company contact.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: Option<String>,
    // MAPPS-481: `#[serde(default)]` so a pre-PMS-806 response still decodes
    // and the page keeps rendering the scalar mirrors above.
    #[serde(default)]
    phones: Vec<RemotePhone>,
    #[serde(default)]
    companies: Vec<RemoteCompanyLink>,
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
                        "This contact can sign in to the Client Portal once a password has been issued from Settings > Portal Users."
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
                        "Granting access flips the portal flag. A password still has to be issued separately from Settings > Portal Users before the contact can sign in."
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
        validate_address_text, validate_country_field, validate_name_field, validate_phone_field,
        validate_postal_field, validate_timezone_field, validate_website_field, website_probe_note,
        WebsiteProbe,
    };
    use serde_json::Value;

    /// A reachable probe of `canonical`, with every optional signal off.
    fn reached(canonical: &str) -> WebsiteProbe {
        WebsiteProbe {
            reachable: true,
            canonical_url: Some(canonical.to_string()),
            http_redirects_to_https: false,
            www_change: "none".to_string(),
            unreachable_reason: None,
            redirect_truncated: false,
        }
    }

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
        // Non-http schemes are rejected; a scheme-less host is accepted and
        // normalized (MAPPS-480).
        assert!(validate_website_field("mailto:a@example.com").is_err());
        assert_eq!(
            validate_website_field("example.com").unwrap(),
            Value::String("https://example.com".into())
        );
        // Malformed http(s) URLs are rejected.
        assert!(validate_website_field("http://").is_err());
        assert!(validate_website_field("https://exa mple.com").is_err());
        // Whitespace and control characters are rejected whatever the scheme.
        assert!(validate_website_field("exa mple.com").is_err());
        assert!(validate_website_field("example\u{0007}.com").is_err());
    }

    #[test]
    fn website_normalizes_a_scheme_less_host() {
        // MAPPS-480: a bare domain gets the scheme the product wants.
        assert_eq!(
            validate_website_field("DentalArtsPractice.com").unwrap(),
            Value::String("https://DentalArtsPractice.com".into())
        );
        // Path, query and fragment survive normalization.
        assert_eq!(
            validate_website_field("example.com/path?q=1").unwrap(),
            Value::String("https://example.com/path?q=1".into())
        );
        assert_eq!(
            validate_website_field("example.com/a#frag").unwrap(),
            Value::String("https://example.com/a#frag".into())
        );
        // Trimmed before normalization, so surrounding space is not baked in.
        assert_eq!(
            validate_website_field("  example.com  ").unwrap(),
            Value::String("https://example.com".into())
        );
        // A single-label host is not a public website address.
        assert!(validate_website_field("localhost").is_err());
        assert!(validate_website_field("no-dot").is_err());
        // Empty labels and non-host characters in the authority are rejected.
        assert!(validate_website_field("example.").is_err());
        assert!(validate_website_field(".com").is_err());
        assert!(validate_website_field("user@example.com").is_err());
        // A `:` before the first `/` reads as a scheme, so a scheme-less
        // host:port is rejected rather than guessed at.
        assert!(validate_website_field("example.com:8443").is_err());
    }

    #[test]
    fn website_probe_note_reports_every_state() {
        // Nothing changed: just the address that answered.
        assert_eq!(
            website_probe_note("https://example.com", &reached("https://example.com/")),
            "Resolved to https://example.com/"
        );
        // What changed is named, so a rewritten value is never silent.
        let mut probe = reached("https://www.example.com/");
        probe.http_redirects_to_https = true;
        probe.www_change = "added".to_string();
        assert_eq!(
            website_probe_note("https://example.com", &probe),
            "Resolved to https://www.example.com/ (http redirects to https, www added)"
        );
        let mut probe = reached("https://example.com/");
        probe.www_change = "removed".to_string();
        assert_eq!(
            website_probe_note("https://www.example.com", &probe),
            "Resolved to https://example.com/ (www removed)"
        );
        // A chain the server stopped following is reported as unsettled
        // rather than presented as canonical (MAPPS-486).
        let mut probe = reached("https://www.example.com/");
        probe.redirect_truncated = true;
        assert_eq!(
            website_probe_note("https://example.com", &probe),
            "Resolved to https://www.example.com/ (site redirects again; not followed)"
        );
        // Unreachable names the cause and the value that will be saved.
        let probe = WebsiteProbe {
            reachable: false,
            canonical_url: None,
            http_redirects_to_https: false,
            www_change: "none".to_string(),
            unreachable_reason: Some("timeout".to_string()),
            redirect_truncated: false,
        };
        assert_eq!(
            website_probe_note("https://example.com", &probe),
            "Could not reach example.com (timeout). Saving as https://example.com."
        );
        // An unknown reason is passed through, never dropped.
        let probe = WebsiteProbe {
            unreachable_reason: Some("teapot".to_string()),
            ..probe
        };
        assert_eq!(
            website_probe_note("https://example.com/x", &probe),
            "Could not reach example.com (teapot). Saving as https://example.com/x."
        );
    }

    #[test]
    fn website_probe_body_deserializes_without_redirect_truncated() {
        // The shipped server (PMS-805) omits `redirect_truncated`; the client
        // must still read the body it actually sends (MAPPS-486).
        let body = serde_json::json!({
            "input": "example.com",
            "reachable": true,
            "canonical_url": "https://www.example.com/",
            "https_ok": true,
            "http_ok": true,
            "http_redirects_to_https": true,
            "www_change": "added",
            "final_status": 200,
            "unreachable_reason": null
        });
        let probe: WebsiteProbe = serde_json::from_value(body).unwrap();
        assert!(!probe.redirect_truncated);
        assert_eq!(
            website_probe_note("https://example.com", &probe),
            "Resolved to https://www.example.com/ (http redirects to https, www added)"
        );
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

    /// MAPPS-581 was reported as "Phone rejects 919-397-4144". The value was
    /// always valid; the message on screen was a stale one from an earlier
    /// submit that the field never cleared. This pins the value so a future
    /// change to the rule cannot make the original report true after the fact.
    #[test]
    fn reported_us_phone_is_valid() {
        assert_eq!(
            validate_phone_field("919-397-4144", "Phone").unwrap(),
            Value::String("9193974144".into())
        );
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

    // ---- MAPPS-582: invisible characters never reach a validator, and never
    // get stored under a name that looks clean.

    /// The reported defect: `919-397-4144` with an invisible character
    /// appended rendered identically to the valid number and was rejected with
    /// a message the user could not act on. U+202F is the other half: `.trim()`
    /// removed it, but the old hardcoded `' ' | '\t' | '\u{00A0}'` strip set
    /// did not, so it reached the E.164 check as "not a digit".
    #[test]
    fn phone_accepts_a_number_carrying_an_invisible_character() {
        for suffix in [
            "\u{200B}", "\u{FEFF}", "\u{00AD}", "\u{200E}", "\u{200C}", "\u{202F}", "\u{00A0}",
            "\u{2007}", "\u{3000}", "\u{2060}", "\u{2069}",
        ] {
            let raw = format!("919-397-4144{suffix}");
            assert_eq!(
                validate_phone_field(&raw, "Phone"),
                Ok(serde_json::Value::String("9193974144".to_string())),
                "U+{:04X} must not reject a valid number",
                suffix.chars().next().unwrap() as u32
            );
        }
        // An interior one is stripped too, not just a trailing one.
        assert_eq!(
            validate_phone_field("919-397\u{200B}-4144", "Phone"),
            Ok(serde_json::Value::String("9193974144".to_string()))
        );
    }

    /// Stripping the invisibles must not widen what the field accepts: an
    /// extension is still not part of an E.164 number.
    #[test]
    fn phone_still_rejects_a_genuinely_invalid_number() {
        for bad in ["919-397-4144 x12", "919-397-4144x12", "abc", "1"] {
            assert!(
                validate_phone_field(bad, "Phone").is_err(),
                "{bad} must still be rejected"
            );
        }
    }

    /// The silent half of the bug: a free-text field with no format rule
    /// accepted the invisible character and stored it, so `Acme\u{200B}` became
    /// a second company indistinguishable from `Acme` in every list, search box
    /// and picker.
    #[test]
    fn a_company_name_cannot_be_saved_with_an_invisible_character() {
        let clean = validate_name_field("Acme").expect("Acme is a valid name");
        for raw in [
            "Acme\u{200B}",
            "\u{FEFF}Acme",
            "Ac\u{00AD}me",
            "Acme\u{202F}",
            "Acme\u{00A0}",
            "Acme\u{2069}",
        ] {
            assert_eq!(
                validate_name_field(raw).as_deref(),
                Ok(clean.as_str()),
                "{raw:?} must not be storable as a name distinct from \"Acme\""
            );
        }
        // A real control character is still an error: it is not invisible in
        // the same sense, and the user can remove it.
        assert!(validate_name_field("Acme\u{0007}").is_err());
    }

    /// The same hole in the address fields, which also gated on `is_control`.
    #[test]
    fn an_address_line_accepts_a_value_carrying_an_invisible_character() {
        assert_eq!(
            validate_address_text("1 Main St\u{200B}", "Address", 255),
            Ok(())
        );
        assert!(validate_address_text("1 Main St\u{0007}", "Address", 255).is_err());
    }

    /// The structured fields all take the same route.
    #[test]
    fn the_structured_fields_strip_invisible_characters() {
        assert_eq!(
            validate_country_field("US\u{200B}"),
            Ok(serde_json::Value::String("US".to_string()))
        );
        assert_eq!(
            validate_postal_field("27519\u{FEFF}"),
            Ok(serde_json::Value::String("27519".to_string()))
        );
        assert_eq!(
            validate_timezone_field("America/New_York\u{200E}"),
            Ok(serde_json::Value::String("America/New_York".to_string()))
        );
        assert_eq!(
            validate_website_field("https://example.com\u{00AD}"),
            Ok(serde_json::Value::String("https://example.com".to_string()))
        );
    }

    /// The hand-rolled strip set that could not see U+202F is gone, and the
    /// whitespace test is `char::is_whitespace`, so a new exotic space cannot
    /// reintroduce the defect.
    #[test]
    fn the_phone_strip_set_is_not_hardcoded() {
        // The function's own body, so these needles cannot match themselves
        // further down in this test.
        let src = include_str!("contacts.rs");
        let start = src
            .find("fn validate_phone_field(")
            .expect("validate_phone_field is defined in this file");
        let rest = &src[start..];
        let body = &rest[..rest.find("\n}\n").expect("the function closes")];
        assert!(
            !body.contains(r"'\u{00A0}'"),
            "the hardcoded phone strip set must not come back"
        );
        assert!(
            body.contains("!c.is_whitespace()"),
            "validate_phone_field must strip whitespace via char::is_whitespace"
        );
    }
}

#[cfg(test)]
mod company_source_tests {
    use super::{
        freeform_company_note, FREEFORM_COMPANY_NOTE, FREEFORM_TOGGLE_LABEL,
        LINK_COMPANY_TOGGLE_LABEL,
    };

    /// MAPPS-484: the visible create affordance is the picker's "+ New
    /// company" button, which creates a `companies` row. Neither company
    /// control that does NOT create may read like one, which is what the old
    /// "+ Add Company" label did.
    #[test]
    fn neither_toggle_label_promises_a_create() {
        for label in [FREEFORM_TOGGLE_LABEL, LINK_COMPANY_TOGGLE_LABEL] {
            let lowered = label.to_lowercase();
            assert!(
                !lowered.contains("add "),
                "{label} reads like a create action but creates nothing"
            );
            assert!(
                !lowered.contains("new compan"),
                "{label} reads like a create action but creates nothing"
            );
        }
        assert_eq!(
            FREEFORM_TOGGLE_LABEL,
            "Enter a name without creating a company"
        );
    }

    /// The note names the value and the consequence, and says nothing until
    /// something has been typed.
    #[test]
    fn freeform_note_names_the_value_and_the_consequence() {
        assert_eq!(
            freeform_company_note("PugTsurani"),
            "Saved as a typed name. PugTsurani will not appear under Companies."
        );
        // Trimmed, so the note reads the same as the value that gets submitted.
        assert_eq!(
            freeform_company_note("  PugTsurani  "),
            "Saved as a typed name. PugTsurani will not appear under Companies."
        );
        assert!(freeform_company_note("").is_empty());
        assert!(freeform_company_note("   ").is_empty());
    }

    /// The read side says what the name is rather than leaving link colour as
    /// the only signal.
    #[test]
    fn read_side_note_says_it_is_not_a_record() {
        assert!(FREEFORM_COMPANY_NOTE.contains("not a company record"));
    }
}

/// MAPPS-481: the contact form's two repeating child collections. The rules
/// under test are the ones `docs/form-conventions.md` states for any repeating
/// child row: validate every row, exactly one primary, order preserved.
#[cfg(test)]
mod contact_child_row_tests {
    use super::{
        add_company_label, company_link_entries, company_rows_from_remote, extra_company_suffix,
        humanize_phone_type, normalize_phone_type, phone_row_index, phone_rows_from_remote,
        primary_phone_label, validate_phone_rows, CompanyRow, ContactDetail, PhoneRow,
        RemoteCompanyLink, RemoteContact, RemotePhone,
    };

    fn row(phone_type: &str, number: &str, is_primary: bool) -> PhoneRow {
        PhoneRow {
            phone_type: phone_type.to_string(),
            number: number.to_string(),
            is_primary,
            ..PhoneRow::default()
        }
    }

    /// Every row is evaluated before the submit bails, so a bad row two rows
    /// down still gets its own message. No row's failure masks another's.
    #[test]
    fn every_row_is_validated_not_just_the_first() {
        let rows = [
            row("work", "not-a-phone", false),
            row("mobile", "+14155551234", false),
            row("home", "0412 345 678", false),
        ];
        let errors = validate_phone_rows(&rows).expect_err("two rows are invalid");
        assert_eq!(errors.len(), 3);
        assert!(errors[0].starts_with("Number must be a valid phone number"));
        assert!(errors[1].is_empty(), "the valid row keeps a clean slot");
        assert!(errors[2].starts_with("Number must be a valid phone number"));
    }

    /// A contact with no numbers is valid, and a row the user added and left
    /// empty is not a number rather than an error.
    #[test]
    fn no_rows_and_blank_rows_both_save() {
        assert!(validate_phone_rows(&[]).expect("valid").is_empty());
        let entries = validate_phone_rows(&[row("work", "   ", false)]).expect("valid");
        assert!(entries.is_empty());
    }

    /// Exactly one entry is sent primary: the flagged row, or the first when
    /// none is flagged (which is also PMS-806's own promotion rule).
    #[test]
    fn exactly_one_entry_is_primary() {
        let entries = validate_phone_rows(&[
            row("work", "+14155551234", false),
            row("mobile", "+14155559999", true),
        ])
        .expect("valid");
        assert_eq!(entries[0]["is_primary"], serde_json::json!(false));
        assert_eq!(entries[1]["is_primary"], serde_json::json!(true));

        let promoted = validate_phone_rows(&[
            row("work", "+14155551234", false),
            row("mobile", "+14155559999", false),
        ])
        .expect("valid");
        assert_eq!(promoted[0]["is_primary"], serde_json::json!(true));
        assert_eq!(promoted[1]["is_primary"], serde_json::json!(false));
    }

    /// Row order is the payload order, which is the `sort_order` PMS-806
    /// derives from the array index, and the type/extension travel with it.
    #[test]
    fn payload_keeps_row_order_and_fields() {
        let rows = [
            PhoneRow {
                phone_type: "work".to_string(),
                number: "(415) 555-1234".to_string(),
                extension: "220".to_string(),
                is_primary: false,
                error: String::new(),
            },
            row("pager", "+14155559999", false),
        ];
        let entries = validate_phone_rows(&rows).expect("valid");
        assert_eq!(entries[0]["phone_type"], serde_json::json!("work"));
        // `validate_phone_field` strips the separators and keeps what was typed.
        assert_eq!(entries[0]["number"], serde_json::json!("4155551234"));
        assert_eq!(entries[0]["extension"], serde_json::json!("220"));
        // An unknown type falls back to the server's own default.
        assert_eq!(entries[1]["phone_type"], serde_json::json!("other"));
        assert_eq!(entries[1]["extension"], serde_json::Value::Null);
    }

    #[test]
    fn phone_types_normalize_and_humanize() {
        assert_eq!(normalize_phone_type("fax"), "fax");
        assert_eq!(normalize_phone_type("pager"), "other");
        assert_eq!(normalize_phone_type(""), "other");
        assert_eq!(humanize_phone_type("mobile"), "Mobile");
        // Unknown values pass through rather than vanishing.
        assert_eq!(humanize_phone_type("pager"), "pager");
    }

    /// Create and edit round-trip: the server's list reloads into the form
    /// with the same rows, types, order and primary flags.
    #[test]
    fn phone_rows_round_trip_from_the_server_list() {
        let remote = [
            RemotePhone {
                phone_type: "work".to_string(),
                number: "+14155551234".to_string(),
                extension: Some("220".to_string()),
                is_primary: false,
            },
            RemotePhone {
                phone_type: "mobile".to_string(),
                number: "+14155559999".to_string(),
                extension: None,
                is_primary: true,
            },
        ];
        let rows = phone_rows_from_remote(&remote, Some("+14155551234"), None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].phone_type, "work");
        assert_eq!(rows[0].extension, "220");
        assert!(!rows[0].is_primary);
        assert_eq!(rows[1].phone_type, "mobile");
        assert!(rows[1].is_primary);
    }

    /// A server that predates PMS-806 sends no list, so the scalar mirrors
    /// seed the rows instead of the edit silently dropping the numbers.
    #[test]
    fn phone_rows_fall_back_to_the_scalar_mirrors() {
        let rows = phone_rows_from_remote(&[], Some("+14155551234"), Some("+14155559999"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].phone_type, "work");
        assert!(rows[0].is_primary);
        assert_eq!(rows[1].phone_type, "mobile");
        assert!(!rows[1].is_primary);
        assert!(phone_rows_from_remote(&[], None, None).is_empty());
    }

    #[test]
    fn company_rows_round_trip_and_fall_back() {
        let remote = [
            RemoteCompanyLink {
                company_id: Some(uuid::Uuid::nil()),
                company_name: Some("Acme".to_string()),
                title: Some("IT Director".to_string()),
                is_primary: true,
            },
            RemoteCompanyLink {
                company_id: Some(uuid::Uuid::max()),
                company_name: Some("Globex".to_string()),
                title: None,
                is_primary: false,
            },
        ];
        let rows = company_rows_from_remote(&remote, None, None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].company_name, "Acme");
        assert_eq!(rows[0].title, "IT Director");
        assert!(rows[0].is_primary);
        assert_eq!(rows[1].company_name, "Globex");

        // No list: the single-company mirror seeds one row.
        let mirrored = company_rows_from_remote(&[], Some(uuid::Uuid::nil()), Some("Acme"));
        assert_eq!(mirrored.len(), 1);
        assert!(mirrored[0].is_primary);
        // A freeform-only contact links nothing, so it stays on the typed path.
        assert!(company_rows_from_remote(&[], None, Some("PugTsurani")).is_empty());
    }

    #[test]
    fn company_entries_promote_exactly_one_primary() {
        let rows = [
            CompanyRow {
                company_id: "a".to_string(),
                company_name: "Acme".to_string(),
                title: "  ".to_string(),
                is_primary: false,
            },
            CompanyRow {
                company_id: "b".to_string(),
                company_name: "Globex".to_string(),
                title: "Consultant".to_string(),
                is_primary: true,
            },
        ];
        let entries = company_link_entries(&rows);
        assert_eq!(entries[0]["is_primary"], serde_json::json!(false));
        assert_eq!(entries[0]["title"], serde_json::Value::Null);
        assert_eq!(entries[1]["is_primary"], serde_json::json!(true));
        assert_eq!(entries[1]["title"], serde_json::json!("Consultant"));
        assert!(company_link_entries(&[]).is_empty());
    }

    /// The list Phone cell names the type, and the Company cell counts the
    /// links the cell does not show.
    #[test]
    fn list_cells_show_the_primary_and_the_remainder() {
        let phones = [
            RemotePhone {
                phone_type: "work".to_string(),
                number: "+19042108340".to_string(),
                extension: Some("12".to_string()),
                is_primary: false,
            },
            RemotePhone {
                phone_type: "mobile".to_string(),
                number: "9042108340".to_string(),
                extension: None,
                is_primary: true,
            },
        ];
        assert_eq!(
            primary_phone_label(&phones, ""),
            "Mobile (904) 210-8340".to_string()
        );
        // No list: the `phone` mirror still fills the column.
        assert_eq!(primary_phone_label(&[], "9042108340"), "(904) 210-8340");
        assert_eq!(primary_phone_label(&[], ""), "");
        // No flag: the first entry is what the server promoted, and the
        // extension rides along.
        assert_eq!(
            primary_phone_label(&phones[..1], ""),
            "Work +1 (904) 210-8340 ext. 12".to_string()
        );

        assert_eq!(extra_company_suffix(0), "");
        assert_eq!(extra_company_suffix(1), "");
        assert_eq!(extra_company_suffix(3), "+2");
    }

    /// PMS-806 names a rejected entry `phones[i].number`, so the message can
    /// land in that row's own slot instead of the form-level banner.
    #[test]
    fn server_phone_field_names_resolve_to_a_row() {
        assert_eq!(phone_row_index("phones[0].number"), Some(0));
        assert_eq!(phone_row_index("phones[12].number"), Some(12));
        assert_eq!(phone_row_index("phones[].number"), None);
        assert_eq!(phone_row_index("phone"), None);
        assert_eq!(phone_row_index("first_name"), None);
    }

    /// "Add" on the company block means add another company, and never reads
    /// like the create affordance MAPPS-484 reserved for the picker.
    #[test]
    fn add_company_label_only_ever_adds() {
        assert_eq!(add_company_label(0), "Add a company");
        assert_eq!(add_company_label(2), "Add another company");
        for linked in [0usize, 2] {
            assert!(!add_company_label(linked)
                .to_lowercase()
                .contains("new compan"));
        }
    }

    /// The DTOs carry both lists with `#[serde(default)]`, so a response from
    /// a server that predates PMS-806 still deserializes.
    #[test]
    fn contact_dtos_decode_without_the_child_lists() {
        let body = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "first_name": "Ada",
            "last_name": "Lovelace",
        });
        let listed: RemoteContact = serde_json::from_value(body.clone()).expect("decodes");
        assert!(listed.phones.is_empty());
        assert!(listed.companies.is_empty());
        let detail: ContactDetail = serde_json::from_value(body).expect("decodes");
        assert!(detail.phones.is_empty());
        assert!(detail.companies.is_empty());
    }
}

/// MAPPS-481: the create/edit round trip. A contact saved with three phones
/// and two companies reloads into the form with the same rows, types, order
/// and primary flags. Exercised over the pure edges of the trip (form rows ->
/// request payload, and server response -> form rows) with the payload echoed
/// back as the server would return it.
#[cfg(test)]
mod contact_round_trip_tests {
    use super::{
        company_link_entries, company_rows_from_remote, phone_rows_from_remote,
        validate_phone_rows, CompanyRow, PhoneRow, RemoteCompanyLink, RemotePhone,
    };

    /// Turn the request entries back into the response shape the server sends
    /// for the same contact, so the reload starts from real wire values.
    fn echo_phones(entries: &[serde_json::Value]) -> Vec<RemotePhone> {
        entries
            .iter()
            .map(|e| RemotePhone {
                phone_type: e["phone_type"].as_str().unwrap_or_default().to_string(),
                number: e["number"].as_str().unwrap_or_default().to_string(),
                extension: e["extension"].as_str().map(str::to_string),
                is_primary: e["is_primary"].as_bool().unwrap_or_default(),
            })
            .collect()
    }

    fn echo_companies(entries: &[serde_json::Value], names: &[&str]) -> Vec<RemoteCompanyLink> {
        entries
            .iter()
            .zip(names)
            .map(|(e, name)| RemoteCompanyLink {
                company_id: e["company_id"]
                    .as_str()
                    .and_then(|id| uuid::Uuid::parse_str(id).ok()),
                company_name: Some((*name).to_string()),
                title: e["title"].as_str().map(str::to_string),
                is_primary: e["is_primary"].as_bool().unwrap_or_default(),
            })
            .collect()
    }

    #[test]
    fn three_phones_and_two_companies_reload_unchanged() {
        let saved_phones = vec![
            PhoneRow {
                phone_type: "work".to_string(),
                number: "+14155551234".to_string(),
                extension: "220".to_string(),
                is_primary: false,
                error: String::new(),
            },
            PhoneRow {
                phone_type: "mobile".to_string(),
                number: "+14155559999".to_string(),
                extension: String::new(),
                is_primary: true,
                error: String::new(),
            },
            PhoneRow {
                phone_type: "fax".to_string(),
                number: "+14155550000".to_string(),
                extension: String::new(),
                is_primary: false,
                error: String::new(),
            },
        ];
        let acme = uuid::Uuid::nil();
        let globex = uuid::Uuid::max();
        let saved_companies = vec![
            CompanyRow {
                company_id: acme.to_string(),
                company_name: "Acme".to_string(),
                title: "IT Director".to_string(),
                is_primary: false,
            },
            CompanyRow {
                company_id: globex.to_string(),
                company_name: "Globex".to_string(),
                title: String::new(),
                is_primary: true,
            },
        ];

        let phone_entries = validate_phone_rows(&saved_phones).expect("valid");
        let company_entries = company_link_entries(&saved_companies);
        assert_eq!(phone_entries.len(), 3);
        assert_eq!(company_entries.len(), 2);

        // The mirrors the server derives are ignored once the lists arrive.
        let reloaded_phones =
            phone_rows_from_remote(&echo_phones(&phone_entries), Some("+14155559999"), None);
        assert_eq!(reloaded_phones, saved_phones);

        let reloaded_companies = company_rows_from_remote(
            &echo_companies(&company_entries, &["Acme", "Globex"]),
            Some(globex),
            Some("Globex"),
        );
        assert_eq!(reloaded_companies, saved_companies);
    }
}

/// MAPPS-527: every sort key these lists can send must be one the server
/// allow-lists, or the header renders a sort the rows never got.
#[cfg(test)]
mod sort_key_tests {
    use super::{
        company_sort_query, contact_sort_query, CompanySortKey, ContactSortKey, SortDirection,
    };
    use crate::utils::sort_keys::{COMPANY_SORT_KEYS, CONTACT_SORT_KEYS};

    #[test]
    fn every_company_sort_key_is_server_accepted() {
        for key in CompanySortKey::ALL {
            let (field, _) =
                company_sort_query(Some((*key, SortDirection::Ascending))).expect("maps to a key");
            assert!(
                COMPANY_SORT_KEYS.contains(&field),
                "`{field}` is not in the server's company sort allow-list"
            );
        }
    }

    #[test]
    fn every_contact_sort_key_is_server_accepted() {
        for key in ContactSortKey::ALL {
            let (field, _) =
                contact_sort_query(Some((*key, SortDirection::Ascending))).expect("maps to a key");
            assert!(
                CONTACT_SORT_KEYS.contains(&field),
                "`{field}` is not in the server's contact sort allow-list"
            );
        }
    }

    /// The two keys MAPPS-527 withdrew. Neither is accepted, so neither may
    /// come back as a column affordance without a server change first.
    #[test]
    fn the_withdrawn_keys_are_still_not_accepted() {
        assert!(!COMPANY_SORT_KEYS.contains(&"company_type"));
        assert!(!CONTACT_SORT_KEYS.contains(&"company_name"));
    }
}

/// MAPPS-575: archiving a company is only useful if it actually takes the
/// company out of day-to-day use, and only safe if its history stays reachable.
/// Those two pull in opposite directions at every call site that lists
/// companies, so which ones narrow to `status=active` is the decision this
/// feature rests on, and it is invisible in a rendered page.
///
/// A source scan rather than SSR: each of these is a URL built inside a
/// `use_resource` closure that only runs under the `app` feature, so no host
/// test can observe the request. What is being pinned is the classification.
#[cfg(test)]
mod archive_scope_tests {
    /// Selectors that CHOOSE a company for new work. An archived company
    /// offered here goes straight back into the state it was archived to leave.
    const MUST_NARROW: &[(&str, &str)] = &[(
        "company picker (new/edit contact, tickets, time entries)",
        include_str!("../components/company_picker.rs"),
    )];

    #[test]
    fn every_company_selector_for_new_work_asks_for_active_only() {
        for (what, src) in MUST_NARROW {
            let queries: Vec<&str> = src
                .lines()
                .filter(|l| l.contains("/contacts/companies?"))
                .collect();
            assert!(
                !queries.is_empty(),
                "{what}: expected at least one companies query; did the endpoint move?"
            );
            for q in queries {
                assert!(
                    q.contains("status=active"),
                    "{what}: a selector for new work must ask for active companies only, \
                     or archiving does not remove the company from day-to-day use. Found: {q}"
                );
            }
        }
    }

    /// The counterweight. A LIST FILTER must not narrow: an archived company's
    /// contracts and quotes still exist, and being able to look at them is the
    /// reason archiving keeps history rather than deleting it. A blanket
    /// "add status=active everywhere" sweep would break exactly this, and it
    /// would look like tightening rather than like the regression it is.
    #[test]
    fn list_filters_still_offer_archived_companies() {
        for (what, src) in [
            ("contracts list filter", include_str!("contracts.rs")),
            ("quotes list filter", include_str!("quotes.rs")),
        ] {
            let unnarrowed = src
                .lines()
                .filter(|l| l.contains("/contacts/companies?"))
                .filter(|l| !l.contains("status=active"))
                .count();
            assert!(
                unnarrowed >= 1,
                "{what}: the list filter must still offer archived companies, so their \
                 kept history is reachable; every companies query in this file narrows"
            );
        }
    }

    /// The company list's own default. Active, and the value is bound to the
    /// Select, so the default is stated on screen rather than silently applied:
    /// a user who cannot find a company they archived can see the list is
    /// filtered instead of concluding it was deleted.
    #[test]
    fn the_company_list_defaults_to_active_and_says_so() {
        const SRC: &str = include_str!("contacts.rs");
        assert!(
            SRC.contains(r#"let mut status_filter = use_signal(|| "active".to_string());"#),
            "the company list must default to active"
        );
        assert!(
            SRC.contains(r#"value: status_filter.read().clone(),"#),
            "and must bind that default to the Select, so it is visible"
        );
        assert!(
            SRC.contains(r#"SelectOption::new("", "Any status")"#),
            "and must offer a way back to the archived ones"
        );
    }

    /// The default must not read as a filter, or a brand-new tenant with no
    /// companies is told "No companies match your filters" on first load and
    /// offered a Clear filters button for filters it never set.
    #[test]
    fn the_default_status_does_not_count_as_a_filter() {
        const SRC: &str = include_str!("contacts.rs");
        assert!(
            SRC.contains(
                r#"!search_text.is_empty() || !type_text.is_empty() || status_text != "active""#
            ),
            "an empty tenant must read as \"No companies yet\", not as a filtered-out list"
        );
    }
}

/// MAPPS-577: the delete dialog's decisions, none of which a rendered snapshot
/// shows. Source scans for the same reason the other page suites use them: the
/// preview fetch and the archive PUT only run under the `app` feature.
#[cfg(test)]
mod delete_dialog_tests {
    const SRC: &str = include_str!("contacts.rs");

    /// Shipping code only, whitespace-collapsed. Excludes this module, because
    /// every assertion quotes the pattern it looks for and would match itself.
    fn code_only() -> String {
        let end = SRC
            .find("mod delete_dialog_tests")
            .expect("this module is part of this file");
        SRC[..end]
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// AC1, and the defect MAPPS-577 reported. The client held an English copy
    /// of the server's blocking rules; PMS-919 changed them and the copy went
    /// stale, so the dialog kept warning that projects, appointments and
    /// sub-companies block a delete long after they started unlinking.
    #[test]
    fn the_dialog_holds_no_copy_of_which_tables_block() {
        let code = code_only();
        for stale in [
            "cannot be deleted; remove those first",
            "appointments or sub-companies",
        ] {
            assert!(
                !code.contains(stale),
                "the dialog must not restate the server's blocking rules: found {stale:?}"
            );
        }
        assert!(
            code.contains("deletion-preview"),
            "what blocks a delete comes from PMS-926, so the two cannot drift"
        );
    }

    /// AC8. The Statistics card cannot answer this: `open_ticket_count` filters
    /// on `closed_at IS NULL` while the delete guard counts every ticket, so a
    /// company with closed tickets and none open reads as deletable there and
    /// is then refused.
    #[test]
    fn the_counts_are_not_re_derived_from_the_statistics_card() {
        let code = code_only();
        let preview_block = {
            let start = code
                .find("let deletion_preview = use_resource")
                .expect("the preview resource exists");
            &code[start..start + 600]
        };
        assert!(
            !preview_block.contains("open_ticket_count"),
            "the preview must not be built from the page's own counts"
        );
    }

    /// AC5. A refused delete offers no gate and no Delete button.
    #[test]
    fn a_blocked_delete_withholds_the_gate() {
        let code = code_only();
        assert!(
            code.contains("let blocked = known && !preview.can_delete;"),
            "blocked follows the server's own verdict"
        );
        assert!(
            code.contains("blocked,"),
            "and is passed to the dialog, which withholds the gate and the \
             confirm button"
        );
    }

    /// AC7. The delete path never depends on the preview arriving. An unknown
    /// preview degrades to the pre-MAPPS-577 behaviour rather than blocking the
    /// dialog or disabling the delete.
    #[test]
    fn an_absent_preview_does_not_block_the_delete() {
        let code = code_only();
        assert!(
            code.contains("let known = snapshot.is_some();"),
            "the dialog distinguishes 'no preview' from 'preview says blocked'"
        );
        assert!(
            code.contains("let blocked = known && !preview.can_delete;"),
            "an absent preview is NOT blocked: without the `known &&`, a failed \
             fetch would withhold the Delete button on every company"
        );
    }

    /// AC4. Archiving is a control in the dialog, not an instruction to go and
    /// find the edit form.
    #[test]
    fn a_blocked_delete_offers_archiving_in_place() {
        let code = code_only();
        assert!(
            code.contains(r#""status": "inactive""#),
            "the alternative performs the archive itself"
        );
        assert!(
            code.contains("Archive instead"),
            "and is labelled as the alternative it is"
        );
        // The own-company refusal is not solved by archiving either, so the
        // action is withheld there rather than offered and then failing.
        assert!(
            code.contains("(blocked && !preview.is_own_company).then("),
            "archiving is not offered where it would not help"
        );
    }
}

#[cfg(test)]
mod mapps597_site_wording_tests {
    const SRC: &str = include_str!("contacts.rs");

    fn code_only() -> String {
        let end = SRC
            .find("mod mapps597_site_wording_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// MAPPS-597: the word stays, and the card says what it means.
    ///
    /// "Site" is what ConnectWise, HaloPSA and Atera call a customer's physical
    /// location, so it is the word staff arriving from those tools already
    /// have. What it lacked was any statement of what it means, on a page that
    /// also carries a Website field.
    #[test]
    fn the_sites_card_says_what_a_site_is() {
        let code = code_only();
        let card = code
            .find("CollapsibleCard { title: \"Sites\",")
            .expect("the Sites card");
        let window = &code[card..code.len().min(card + 900)];
        assert!(
            window.contains("subtitle: \"Offices, warehouses and other addresses"),
            "the card explains the term in its header: {window}"
        );
    }

    /// An empty state is where a reader who does not know the word meets it.
    /// Saying only that there are none teaches nothing.
    #[test]
    fn the_empty_state_says_what_to_add() {
        let code = code_only();
        assert!(
            !code.contains("No sites for this company yet."),
            "the bare count-of-zero message is gone"
        );
        assert!(
            code.contains("No locations recorded yet. Add the addresses you visit or support."),
            "and says what a site is for"
        );
    }

    /// The rename was investigated and rejected: "Location" is taken by the
    /// appointment field, "Office" is wrong for a warehouse or a datacenter,
    /// and "Branch" is not a term any PSA uses. Pinned because a later reader
    /// hitting the same ambiguity will reach for the same rename, and the
    /// reasons live in the ticket rather than in the diff.
    #[test]
    fn nothing_was_renamed() {
        let code = code_only();
        for kept in [
            "CollapsibleCard { title: \"Sites\",",
            "\"New Site\"",
            "\"Edit Site\"",
            "\"Create Site\"",
        ] {
            assert!(code.contains(kept), "{kept} still says Site");
        }
        assert!(
            code.contains("\"/contacts/sites\""),
            "and the API path is untouched, because this is copy and not a contract change"
        );
    }

    /// The hint belongs where somebody is about to type an address, and only
    /// there: an edit form already has the answer filled in above it.
    #[test]
    fn the_create_form_explains_itself_once() {
        let code = code_only();
        assert!(
            code.contains(
                "if !is_edit { p { class: \"text-sm text-muted\", \
                 \"A site is an office, warehouse or other address where this company operates.\" } }"
            ),
            "the create path carries the sentence and the edit path does not"
        );
    }
}

#[cfg(test)]
mod mapps614_notes_as_markdown_tests {
    use super::*;

    const SRC: &str = include_str!("contacts.rs");

    /// The shipping code with runs of whitespace collapsed, excluding this
    /// module: every assertion quotes the pattern it looks for, so a scan
    /// including its own source would match itself and pass regardless.
    fn code_only() -> String {
        let end = SRC
            .find("mod mapps614_notes_as_markdown_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The trap this whole field walks into, and the reason `clearable_string`
    /// exists beside `optional_string` instead of reusing it.
    ///
    /// The server adds `notes = $n` to its UPDATE only when the request
    /// carries a value (PMS-952 pins that from the other side), so a null is
    /// "leave it alone" and not "erase it". Sent through `optional_string`, a
    /// user who selects their notes, deletes them and saves would get a
    /// success toast and find the old text still on the record.
    #[test]
    fn an_emptied_note_clears_the_record_rather_than_being_ignored() {
        assert_eq!(clearable_string(""), serde_json::json!(""));
        assert_eq!(clearable_string("   "), serde_json::json!(""));
        assert!(
            optional_string("").is_null(),
            "the contrast is the point: the general helper sends null, which \
             the server reads as no change"
        );
        // A real value is trimmed and sent as itself, so nothing else changes.
        assert_eq!(
            clearable_string("  Renews in March.  "),
            serde_json::json!("Renews in March.")
        );
    }

    /// Both records edit the note in the shared editor, so the toolbar, the
    /// shortcuts and the Write/Preview/Split switcher come with it. A bare
    /// textarea would be the same syntax with none of the help, which is the
    /// state MAPPS-610 moved every other Markdown surface out of.
    #[test]
    fn both_records_edit_their_note_in_the_shared_editor() {
        let code = code_only();
        assert_eq!(
            code.matches("crate::components::MarkdownEditor {").count(),
            2,
            "one on the company form, one on the contact form"
        );
        for name in ["\"company_notes\"", "\"contact_notes\""] {
            assert!(
                code.contains(&format!(
                    "name: {name}.to_string(), label: \"Notes\".to_string(),"
                )),
                "{name} is the shared editor's field, labelled Notes"
            );
        }
        assert!(
            !code.contains("Textarea { name: \"notes\","),
            "neither note is a bare textarea"
        );
    }

    /// Both detail pages render through the shared component, which is the
    /// only path in this app from Markdown source to HTML and is already
    /// scrubbed with ammonia. Rendering here instead would mean a second
    /// sanitiser to keep in step with the first.
    #[test]
    fn both_records_render_their_note_through_the_shared_renderer() {
        let code = code_only();
        assert_eq!(
            code.matches("crate::components::Markdown { content: notes.clone() }")
                .count(),
            2,
            "one on the company detail page, one on the contact detail page"
        );
        // Hidden when there is nothing in it, so a record nobody has written
        // on does not grow a blank card.
        assert_eq!(
            code.matches("if !notes.trim().is_empty() { Card { title: \"Notes\",")
                .count(),
            2,
            "each card is gated on having something to show"
        );
    }

    /// The write path uses the clearing helper on both forms, and neither one
    /// reaches for the general optional helper for this field.
    #[test]
    fn neither_form_sends_the_note_as_a_null() {
        let code = code_only();
        assert_eq!(
            code.matches("\"notes\": clearable_string(&notes.read()),")
                .count(),
            2,
            "the company body and the contact body"
        );
        assert!(
            !code.contains("\"notes\": optional_string("),
            "optional_string would send null for an emptied field"
        );
    }
}

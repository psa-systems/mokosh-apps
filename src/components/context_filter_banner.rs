//! MAPPS-321: shared "you are scoped to {entity}" banner shown above
//! the filter card on a list page.
//!
//! Background. Resource cards on CompanyDetail (Tickets, Assets,
//! Contracts, Projects, Invoices, Contacts) ship a "View all" link to
//! the matching list page with `?company_id=<uuid>`. Every list page
//! reads that query param inside its fetch resource and the server
//! honours `filter.company_id`, so the rendered rows are already
//! scoped. But the page header was hard-coded ("Tickets") and no
//! visible chip indicated the active scope, so QA reported the
//! "View all" link as broken - they could not tell the list was
//! already narrowed.
//!
//! This component reads `?company_id=` (extensible later to
//! `contact_id` / `asset_id`), resolves the company name, and renders
//! a `Badge`-style banner with a `[x]` button that navigates to the
//! same list without the query string. When the param is absent or
//! the lookup fails, the component renders nothing.

use dioxus::prelude::*;
use serde::Deserialize;

use super::table::{Badge, BadgeVariant};
use crate::Route;

#[derive(Clone, Debug, Deserialize)]
struct CompanyNameRow {
    #[serde(default)]
    name: String,
}

/// Which list page is mounting the banner. Drives the "clear filter"
/// navigation target (the same route, without the query string).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ContextFilterScope {
    Tickets,
    Assets,
    Contracts,
    Projects,
    Invoices,
    Contacts,
}

impl ContextFilterScope {
    fn label(self) -> &'static str {
        match self {
            Self::Tickets => "tickets",
            Self::Assets => "assets",
            Self::Contracts => "contracts",
            Self::Projects => "projects",
            Self::Invoices => "invoices",
            Self::Contacts => "contacts",
        }
    }

    fn clear_route(self) -> Route {
        match self {
            Self::Tickets => Route::TicketList {},
            Self::Assets => Route::AssetList {},
            Self::Contracts => Route::ContractList {},
            Self::Projects => Route::ProjectList {},
            Self::Invoices => Route::InvoiceList {},
            Self::Contacts => Route::ContactList {},
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ContextFilterBannerProps {
    pub scope: ContextFilterScope,
}

#[component]
pub fn ContextFilterBanner(props: ContextFilterBannerProps) -> Element {
    // Read the URL once per mount. The list page's `use_resource`
    // already re-reads it on each remount (route change), so we
    // match its lifecycle by reading at the top of render.
    let company_id = crate::utils::url::current_query_param("company_id");

    let company_id_for_fetch = company_id.clone();
    let name_resource = use_resource(move || {
        let id = company_id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let id = id?;
            crate::hooks::fetch::api::get_authed::<CompanyNameRow>(&format!(
                "/contacts/companies/{id}"
            ))
            .await
            .ok()
            .map(|r| r.name)
        }
    });

    // No filter -> render nothing. The banner is invisible on the
    // ordinary list view; it only appears when the user arrived via a
    // context "View all" link.
    if company_id.is_none() {
        return rsx! {};
    }
    let snapshot = name_resource.read_unchecked();
    let resolved_name = match &*snapshot {
        Some(Some(name)) if !name.trim().is_empty() => Some(name.clone()),
        _ => None,
    };
    // Name lookup failure: collapse to nothing rather than leave the
    // banner orphan-rendering "Filtered by company: " with no value.
    let Some(name) = resolved_name else {
        return rsx! {};
    };

    let label = props.scope.label();
    let clear_route = props.scope.clear_route();
    let nav = use_navigator();
    rsx! {
        div { class: "mb-4 flex items-center gap-3 rounded-md border border-line bg-surface-2/50 px-3 py-2",
            Badge { variant: BadgeVariant::Blue, "Scope" }
            span { class: "text-sm text-content",
                "Showing only " strong { "{name}" } "'s {label}."
            }
            div { class: "flex-1" }
            button {
                r#type: "button",
                class: "text-xs font-medium text-accent hover:opacity-90",
                aria_label: "Clear company filter",
                onclick: move |_| {
                    nav.push(clear_route.clone());
                },
                "Clear filter ×"
            }
        }
    }
}

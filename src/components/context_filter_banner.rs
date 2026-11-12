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
//! same list without the query string. When the param is absent the
//! component renders nothing. When the name cannot be resolved (still
//! loading, the lookup failed, or the name is blank) it still renders,
//! saying "the selected company" with the id in a tooltip (MAPPS-1243):
//! the list is scoped either way, and a scoped list that looks unscoped is
//! the state this component exists to prevent.

use dioxus::prelude::*;
use serde::Deserialize;

use super::button::{Button, ButtonSize, ButtonVariant};
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
    // SPEC-5.18.4
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

    // SPEC-5.18.5
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

/// The company name to show, when the lookup produced a usable one.
fn resolved_company_name(snapshot: Option<&Option<String>>) -> Option<String> {
    match snapshot {
        Some(Some(name)) if !name.trim().is_empty() => Some(name.trim().to_string()),
        _ => None,
    }
}

/// What the banner says when the company name is not available.
fn unresolved_scope_sentence(label: &str) -> String {
    format!("Showing only the selected company's {label}.")
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
    // SPEC-5.18.1
    let company_id = crate::utils::url::current_query_param("company_id");

    let company_id_for_fetch = company_id.clone();
    let name_resource = use_resource(move || {
        let id = company_id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let id = id?;
            // SPEC-5.18.2
            crate::hooks::fetch::api::get_authed::<CompanyNameRow>(&format!(
                "/contacts/companies/{id}"
            ))
            .await
            // Best-effort: the banner falls back to the bare filter chip.
            // SPEC-5.18.3
            .inspect_err(|e| {
                tracing::warn!("context filter company name load failed for {id}: {e}")
            })
            .ok()
            .map(|r| r.name)
        }
    });

    // MAPPS-377: `use_navigator` must run on every render, so read it here
    // (before the two early returns below) instead of just above the final
    // rsx. It takes no post-return state and only reads router context.
    let nav = use_navigator();

    // No filter -> render nothing. The banner is invisible on the
    // ordinary list view; it only appears when the user arrived via a
    // context "View all" link.
    if company_id.is_none() {
        return rsx! {};
    }
    let snapshot = name_resource.read_unchecked();
    // SPEC-5.18.3
    let name = resolved_company_name(snapshot.as_ref());
    let tooltip = company_id.clone().unwrap_or_default();

    let label = props.scope.label();
    let clear_route = props.scope.clear_route();
    // SPEC-5.18.4
    rsx! {
        div { class: "mb-4 flex items-center gap-3 rounded-md border border-line bg-surface-2/50 px-3 py-2",
            Badge { variant: BadgeVariant::Blue, "Scope" }
            span { class: "text-sm text-content", title: "{tooltip}",
                // MAPPS-1243: an unresolved name still says the list is
                // scoped; it used to render nothing while the filter applied.
                if let Some(name) = name {
                    "Showing only " strong { "{name}" } "'s {label}."
                } else {
                    "{unresolved_scope_sentence(label)}"
                }
            }
            div { class: "flex-1" }
            Button {
                variant: ButtonVariant::Link,
                size: ButtonSize::Small,
                // SPEC-5.18.5
                aria_label: Some("Clear company filter".to_string()),
                onclick: move |_| {
                    nav.push(clear_route.clone());
                },
                "Clear filter ×"
            }
        }
    }
}

/// MAPPS-1243: an unresolved company name still renders the banner.
#[cfg(test)]
mod tests {
    use super::{resolved_company_name, unresolved_scope_sentence};

    #[test]
    fn only_a_real_name_is_shown_as_the_name() {
        assert_eq!(
            resolved_company_name(Some(&Some(" Acme ".into()))).as_deref(),
            Some("Acme")
        );
        assert_eq!(resolved_company_name(Some(&Some("  ".into()))), None);
        assert_eq!(resolved_company_name(Some(&None)), None);
        assert_eq!(resolved_company_name(None), None);
    }

    #[test]
    fn an_unresolved_name_still_says_the_list_is_scoped() {
        assert_eq!(
            unresolved_scope_sentence("tickets"),
            "Showing only the selected company's tickets."
        );
        let src = include_str!("context_filter_banner.rs");
        let code = &src[..src.find("mod tests {").unwrap()];
        assert!(
            !code.contains("let Some(name) = resolved_name else {"),
            "the early return is gone"
        );
        assert!(
            code.contains("title: \"{tooltip}\""),
            "the id rides in a tooltip"
        );
    }
}

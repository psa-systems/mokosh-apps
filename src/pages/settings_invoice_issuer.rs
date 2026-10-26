//! Settings > Integrations > Xero / QuickBooks (MAPPS-1144, server
//! MAPPS-1142 and MAPPS-1143).
//!
//! Delegating invoicing means the provider issues the invoice the customer
//! receives: it numbers it, it delivers it, and Mokosh keeps a mirror row
//! for history and reconciliation. The server grew the whole round trip
//! over MAPPS-1099 and nothing in this app called any of it, so an admin
//! had no way to connect and the feature could not be switched on by
//! anyone.
//!
//! # One page, two providers
//!
//! A table rather than two files, which is what MAPPS-1124 did for the two
//! contact-sync import pages. These two differ in their name, in what
//! their organisation is called (Xero has organisations, QuickBooks has
//! company files) and in nothing else this page renders. A copied page
//! would be a second place to fix every future change to the flow.
//!
//! # Two directions, two decisions (MAPPS-1156, server MAPPS-1155)
//!
//! Delegating invoicing hands over what customers owe the MSP.
//! Delegating bills hands over what the MSP owes its vendors, and it is a
//! separate decision: connecting IS the invoice delegation, and the
//! server writes `{invoicing}` and nothing else. The card carries the
//! bills toggle because nothing else can set that capability, so without
//! it no tenant on any deployment could delegate one bill.
//!
//! # The switch a tenant cannot touch
//!
//! `configured` says whether the DEPLOYMENT holds an application for the
//! provider. A tenant cannot do anything about that (PMS-1430: the
//! application is the host's and there is nothing for them to edit), so
//! the card says the integration is unavailable here rather than offering
//! a Connect button that would fail at the provider. That is the defect
//! MAPPS-1121 removed, and this page is a place it could come back.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, ErrorBanner,
    PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// What one provider is called, and what its organisation is called.
///
/// Every string this page renders that names a provider comes from here,
/// which is what keeps one page honest about two of them.
#[derive(Clone, Copy, PartialEq)]
pub struct Issuer {
    /// The path segment the server parses, and the `provider` field it
    /// answers with.
    pub provider: &'static str,
    pub display: &'static str,
    /// What this provider calls the thing a connection is made to. Xero
    /// authorises an organisation; Intuit authorises a company file.
    pub organisation: &'static str,
    /// What delegating invoicing to this provider means, in its own terms.
    pub description: &'static str,
    pub testid: &'static str,
}

pub const XERO: Issuer = Issuer {
    provider: "xero",
    display: "Xero",
    organisation: "organisation",
    description: "Xero issues the invoice your customer receives: it numbers it, it sends it, and it records the payment. Mokosh keeps a copy for history and reconciliation and stops being the system of record for the document.",
    testid: "xero",
};

pub const QUICKBOOKS: Issuer = Issuer {
    provider: "quickbooks",
    display: "QuickBooks",
    organisation: "company",
    description: "QuickBooks issues the invoice your customer receives: it numbers it, it sends it, and it records the payment. Mokosh keeps a copy for history and reconciliation and stops being the system of record for the document.",
    testid: "quickbooks",
};

/// `GET /integrations/invoice-issuer/{provider}` (MAPPS-1143).
#[derive(Clone, PartialEq, Deserialize)]
pub struct Overview {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub organisation_id: Option<String>,
    #[serde(default)]
    pub delegated: bool,
    /// Whether delegated issuing is switched on for the DEPLOYMENT
    /// (MAPPS-1154).
    ///
    /// Defaults to TRUE for a server that predates the field, which keeps
    /// an older deployment rendering as it does today rather than telling
    /// every connected tenant their invoices are going nowhere.
    #[serde(default = "default_delegation_enabled")]
    pub delegation_enabled: bool,
    /// Whether this tenant delegated BILLS, which connecting does not do
    /// and only the bills toggle sets (MAPPS-1156).
    #[serde(default)]
    pub bills_delegated: bool,
    /// Whether the DEPLOYMENT has `delegated_bills` switched on. Its own
    /// switch, so the invoice one answers a different question.
    ///
    /// No `true` default, unlike the invoice one: a server that predates
    /// the field cannot have delegated a bill either, so `bills_delegated`
    /// is false there and the pair this is read with never renders.
    #[serde(default)]
    pub bills_delegation_enabled: bool,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// A server that does not send `delegation_enabled` predates MAPPS-1154,
/// and so predates the card being able to tell. The honest default is the
/// one that says nothing rather than the one that contradicts.
fn default_delegation_enabled() -> bool {
    true
}

/// `POST /integrations/invoice-issuer/{provider}/authorize` answers where
/// to send the browser.
///
/// The SPA navigates rather than the server redirecting, which is the
/// shape contact sync uses (PSA-70 K): this page says what is about to
/// happen first, and this is the moment a customer's invoicing starts
/// moving into a shared system.
#[derive(Deserialize)]
struct AuthorizeResponse {
    #[serde(default)]
    authorize_url: String,
}

#[derive(Serialize)]
struct NoBody {}

/// `PUT /integrations/invoice-issuer/{provider}/bills` (MAPPS-1155).
#[derive(Serialize)]
struct SetBillsRequest {
    enabled: bool,
}

/// The four states this card renders.
#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    /// The deployment holds no application for this provider.
    NotAvailable,
    NeverConnected,
    Connected {
        organisation_id: Option<String>,
        delegated: bool,
        /// The deployment's own switch. A tenant who delegated everything
        /// on a deployment that has not switched it on still has every
        /// invoice issued by Mokosh (MAPPS-1154).
        delegation_enabled: bool,
        /// MAPPS-1156: the tenant's bill decision, and the deployment's.
        /// Both on the state rather than read again in the body, so the
        /// branch that renders the toggle is the only place either is
        /// consulted.
        bills_delegated: bool,
        bills_delegation_enabled: bool,
    },
    /// Connected, and the last thing the provider said was a failure.
    Failing {
        message: Option<String>,
    },
}

/// What the server said, as the card reads it.
///
/// `configured` is checked first and alone, because a deployment with no
/// application cannot be in any of the other states whatever the row says:
/// the tenant's row can only have been written while an application
/// existed, and what matters now is that there is nothing they can press.
pub fn card_state_of(overview: &Overview) -> CardState {
    if !overview.configured {
        return CardState::NotAvailable;
    }
    match overview.status.as_str() {
        "connected" => CardState::Connected {
            organisation_id: overview.organisation_id.clone(),
            delegated: overview.delegated,
            delegation_enabled: overview.delegation_enabled,
            bills_delegated: overview.bills_delegated,
            bills_delegation_enabled: overview.bills_delegation_enabled,
        },
        "error" => CardState::Failing {
            message: overview.last_error.clone(),
        },
        // `disconnected` keeps the row and means nothing is delegated now,
        // which is the same offer as never having connected. The server
        // distinguishes them for its own reasons (the capability set
        // survives a disconnect) and this page does not need to.
        _ => CardState::NeverConnected,
    }
}

/// The badge beside the heading.
pub fn badge_for(state: &CardState) -> (&'static str, BadgeVariant) {
    match state {
        CardState::NotAvailable => ("Not available here", BadgeVariant::Gray),
        CardState::NeverConnected => ("Not connected", BadgeVariant::Gray),
        // Something actually reaches the provider: a direction the tenant
        // delegated AND whose deployment switch is on. Either direction
        // counts (MAPPS-1156), because a tenant whose payables go to the
        // provider has a working integration whether or not their invoices
        // do, and one guard says that once instead of enumerating the four
        // combinations of two pairs.
        CardState::Connected {
            delegated,
            delegation_enabled,
            bills_delegated,
            bills_delegation_enabled,
            ..
        } if (*delegated && *delegation_enabled)
            || (*bills_delegated && *bills_delegation_enabled) =>
        {
            ("Connected", BadgeVariant::Green)
        }
        // Connected to the provider and handing it nothing is a real state
        // and not a good one: the connection is live and every invoice is
        // still issued locally. Three ways to be in it - the tenant
        // delegated nothing, the deployment has not switched the direction
        // they delegated on, or both - and they read the same here, because
        // the consequence is the same.
        CardState::Connected { .. } => ("Connected, not issuing", BadgeVariant::Gray),
        CardState::Failing { .. } => ("Error", BadgeVariant::Red),
    }
}

/// Where this provider's mapping page lives.
pub fn mappings_route(issuer: Issuer) -> Route {
    if issuer.provider == XERO.provider {
        Route::SettingsXeroMappings {}
    } else {
        Route::SettingsQuickBooksMappings {}
    }
}

#[component]
pub fn XeroSettingsPage() -> Element {
    rsx! { IssuerSettingsPage { issuer: XERO } }
}

#[component]
pub fn QuickBooksSettingsPage() -> Element {
    rsx! { IssuerSettingsPage { issuer: QUICKBOOKS } }
}

#[component]
fn IssuerSettingsPage(issuer: Issuer) -> Element {
    use_page_title(issuer.display);
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "{issuer.display}" } };
    }
    rsx! { IssuerSettingsBody { issuer } }
}

#[component]
fn IssuerSettingsBody(issuer: Issuer) -> Element {
    let overview = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Overview>(&format!(
            "/integrations/invoice-issuer/{}",
            issuer.provider
        ))
        .await
        .inspect_err(|e| tracing::error!("invoice issuer status load failed: {e}"))
        .ok()
    });
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();

    let snap = overview.read_unchecked().clone();
    let data = snap.as_ref().and_then(|o| o.as_ref()).cloned();
    let state = data.as_ref().map(card_state_of);

    let connect = move |_| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<AuthorizeResponse, _>(
                    &format!("/integrations/invoice-issuer/{}/authorize", issuer.provider),
                    &NoBody {},
                )
                .await
                {
                    Ok(resp) => {
                        // The consent screen, then back to this app through
                        // the server's callback. Busy stays on: this page is
                        // about to be replaced.
                        #[cfg(target_arch = "wasm32")]
                        if let Err(e) = crate::platform::location::set_href(&resp.authorize_url) {
                            error.set(format!("Could not open the sign-in page: {e}"));
                            busy.set(false);
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            // A desktop host has no document to navigate; say
                            // where the consent screen is rather than doing
                            // nothing.
                            error.set(format!("Open this to continue: {}", resp.authorize_url));
                            busy.set(false);
                        }
                    }
                    Err(e) => {
                        error.set(format!("Could not start the connection: {e}"));
                        busy.set(false);
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            busy.set(false);
        });
    };

    let mut overview_handle = overview;
    let disconnect = move |_| {
        busy.set(true);
        error.set(String::new());
        confirm_disconnect.set(false);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                if let Err(e) = crate::hooks::fetch::api::post_authed_no_content(&format!(
                    "/integrations/invoice-issuer/{}/disconnect",
                    issuer.provider
                ))
                .await
                {
                    error.set(format!("Could not disconnect: {e}"));
                }
            }
            busy.set(false);
            overview_handle.restart();
        });
    };

    // MAPPS-1156: the only caller of the one route that can set the
    // `bills_and_expenses` capability. `enabled` is sent rather than
    // toggled server-side, so a stale card cannot flip it the wrong way.
    let mut set_bills = move |next: bool| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                if let Err(e) = crate::hooks::fetch::api::put_authed_json_no_content(
                    &format!("/integrations/invoice-issuer/{}/bills", issuer.provider),
                    &SetBillsRequest { enabled: next },
                )
                .await
                {
                    error.set(format!("Could not change what is delegated: {e}"));
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = next;
            busy.set(false);
            overview_handle.restart();
        });
    };

    let route = if issuer.provider == XERO.provider {
        Route::SettingsXero {}
    } else {
        Route::SettingsQuickBooks {}
    };

    rsx! {
        PageHeader {
            title: "{issuer.display}",
            subtitle: "Let {issuer.display} issue the invoices your customers receive.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: route }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match state {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(state) => {
                let (badge, tone) = badge_for(&state);
                rsx! {
                    Card {
                        title: issuer.display.to_string(),
                        actions: rsx! { Badge { variant: tone, "{badge}" } },
                        div { class: "space-y-4", "data-testid": "invoice-issuer-{issuer.testid}",
                            p { class: "text-sm text-muted", "{issuer.description}" }
                            match state {
                                CardState::NotAvailable => rsx! {
                                    p { class: "text-sm text-content",
                                        "This deployment has no {issuer.display} application configured, so there is nothing to connect to. An operator sets one up for everyone on this deployment."
                                    }
                                },
                                CardState::NeverConnected => rsx! {
                                    p { class: "text-sm text-content",
                                        "Connecting opens {issuer.display} so you can sign in and choose the {issuer.organisation} to invoice from. Nothing is sent there until an invoice is created."
                                    }
                                    div { class: "pt-2",
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            loading: busy(),
                                            disabled: !can_mutate,
                                            onclick: connect,
                                            data_testid: "connect-{issuer.testid}",
                                            "Connect {issuer.display}"
                                        }
                                    }
                                },
                                CardState::Connected { organisation_id, delegated, delegation_enabled, bills_delegated, bills_delegation_enabled } => rsx! {
                                    if let Some(id) = organisation_id.clone() {
                                        p { class: "text-sm text-content",
                                            "Connected to the {issuer.display} {issuer.organisation} "
                                            span { class: "font-medium", "{id}" }
                                            "."
                                        }
                                    }
                                    if delegated && delegation_enabled {
                                        p { class: "text-sm text-muted",
                                            "New invoices are issued by {issuer.display}. The number your customer sees is theirs; Mokosh keeps its own for reference."
                                        }
                                    } else if !delegation_enabled {
                                        // MAPPS-1154: the deployment's switch, which
                                        // is off until an operator flips it. Said
                                        // separately from the tenant's own, because
                                        // the person reading this cannot fix it and
                                        // needs to know who can.
                                        StatusBanner { tone: crate::components::BannerTone::Warning,
                                            "Invoicing through {issuer.display} is switched off for this deployment, so Mokosh is still issuing every invoice. An operator turns it on; the connection is kept and starts being used when they do."
                                        }
                                    } else {
                                        StatusBanner { tone: crate::components::BannerTone::Warning,
                                            "The connection is live and nothing is delegated to it, so Mokosh is still issuing every invoice."
                                        }
                                    }
                                    // The mappings are the other half of being
                                    // able to invoice: connected and unmapped
                                    // refuses every create, which is a state
                                    // this card would otherwise call working.
                                    Link {
                                        to: mappings_route(issuer),
                                        class: "text-sm text-accent hover:underline",
                                        "Set up which client is which {issuer.organisation} in {issuer.display}"
                                    }
                                    // MAPPS-1156: the other direction. Its own
                                    // section rather than a line in the one
                                    // above, because it is a decision and not a
                                    // consequence of having connected.
                                    div { class: "border-t border-line pt-4 space-y-2",
                                        p { class: "text-sm font-medium text-content", "Bills and expenses" }
                                        p { class: "text-sm text-muted",
                                            "The other direction: what you are billed for. With this on, a new bill is raised in {issuer.display} as well as here, and your accountant reconciles it there."
                                        }
                                        if bills_delegated && !bills_delegation_enabled {
                                            StatusBanner { tone: crate::components::BannerTone::Warning,
                                                "This deployment has not switched delegated bills on, so bills are still recorded only in Mokosh. An operator turns it on for everyone on this deployment."
                                            }
                                        } else if bills_delegated {
                                            p { class: "text-sm text-muted",
                                                "New bills are raised in {issuer.display}. A bill for a vendor with no mapping is refused, so map them first."
                                            }
                                        }
                                        div { class: "pt-1",
                                            Button {
                                                variant: if bills_delegated { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                                                size: crate::components::ButtonSize::Small,
                                                loading: busy(),
                                                disabled: !can_mutate,
                                                onclick: move |_| set_bills(!bills_delegated),
                                                data_testid: "bills-delegation-{issuer.testid}",
                                                if bills_delegated { "Stop sending bills" } else { "Send bills to {issuer.display} too" }
                                            }
                                        }
                                    }
                                    div { class: "pt-2",
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            loading: busy(),
                                            disabled: !can_mutate,
                                            onclick: move |_| confirm_disconnect.set(true),
                                            data_testid: "disconnect-{issuer.testid}",
                                            "Disconnect"
                                        }
                                    }
                                    ConfirmDialog {
                                        open: confirm_disconnect(),
                                        title: "Disconnect {issuer.display}?",
                                        message: format!(
                                            "New invoices go back to being issued by Mokosh, with Mokosh's own numbers. Invoices {} has already issued stay where they are, and what you delegated is remembered if you connect again.",
                                            issuer.display,
                                        ),
                                        confirm_text: "Disconnect",
                                        destructive: true,
                                        loading: busy(),
                                        onconfirm: disconnect,
                                        oncancel: move |_| confirm_disconnect.set(false),
                                    }
                                },
                                CardState::Failing { message } => {
                                    let said = message.clone().unwrap_or_else(|| {
                                        format!("The last thing {} said was a failure.", issuer.display)
                                    });
                                    rsx! {
                                    ErrorBanner { "{said}" }
                                    p { class: "text-sm text-muted",
                                        "Connecting again re-authorises the {issuer.organisation}."
                                    }
                                    div { class: "pt-2",
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            loading: busy(),
                                            disabled: !can_mutate,
                                            onclick: connect,
                                            data_testid: "reconnect-{issuer.testid}",
                                            "Reconnect {issuer.display}"
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overview(json: serde_json::Value) -> Overview {
        serde_json::from_value(json).expect("the fixture deserialises")
    }

    /// `configured` is answered before anything else, because a deployment
    /// with no application leaves a tenant nothing to press whatever their
    /// own row says.
    #[test]
    fn a_deployment_without_an_application_offers_nothing() {
        let state = card_state_of(&overview(serde_json::json!({
            "configured": false,
            "status": "connected",
            "organisation_id": "xero-org-1",
            "delegated": true,
        })));
        assert_eq!(state, CardState::NotAvailable);
        assert_eq!(badge_for(&state).0, "Not available here");
    }

    /// Connected and delegating nothing is a real state, and it does not
    /// read as working.
    ///
    /// The connection is live, so "Not connected" would be wrong; every
    /// invoice is still issued locally, so a green "Connected" would be
    /// worse.
    /// MAPPS-1156: the bills toggle belongs to the connected card.
    ///
    /// Read off the rendered body, the shape MAPPS-1145 used for
    /// Disconnect: the server refuses the call unless the row is
    /// connected, so a button on an earlier card would be an offer that
    /// cannot be taken.
    #[test]
    fn the_bills_toggle_belongs_to_the_connected_card() {
        let src = include_str!("settings_invoice_issuer.rs");
        let card = &src[src
            .find(r#""data-testid": "invoice-issuer-"#)
            .expect("the card body")..];
        let connected_at = card
            .find("CardState::Connected {")
            .expect("the connected arm");
        let failing_at = card
            .find("CardState::Failing { message } =>")
            .expect("the failing arm");
        assert!(
            card[connected_at..failing_at].contains("bills-delegation-{issuer.testid}"),
            "a connected card offers it"
        );
        assert!(
            !card[..connected_at].contains("bills-delegation-"),
            "and the cards before it do not: there is nothing connected to delegate to"
        );
    }

    /// Delegated bills with the deployment switch off says so.
    ///
    /// The MAPPS-1154 case in the other direction: a tenant who pressed
    /// the button would otherwise read "new bills are raised in Xero"
    /// while the deployment raises none.
    #[test]
    fn bills_delegated_with_the_switch_off_is_said_out_loud() {
        let state = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": true,
            "bills_delegated": true,
            "bills_delegation_enabled": false,
        })));
        assert!(matches!(
            state,
            CardState::Connected {
                bills_delegated: true,
                bills_delegation_enabled: false,
                ..
            }
        ));
        let src = include_str!("settings_invoice_issuer.rs");
        let body = &src[src.find("Bills and expenses").expect("the section")..];
        let guard = body
            .find("if bills_delegated && !bills_delegation_enabled")
            .expect("the switched-off branch");
        let banner = body
            .find("has not switched delegated bills on")
            .expect("the copy");
        assert!(guard < banner, "the copy is behind the switched-off branch");
    }

    /// A tenant that delegated only BILLS has a working integration.
    ///
    /// `delegated` alone decided the badge, so handing over payables and
    /// nothing else read as "Connected, not issuing", which is true of
    /// invoices and wrong about the provider doing nothing.
    #[test]
    fn bills_alone_is_not_an_idle_connection() {
        let bills_only = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": false,
            "bills_delegated": true,
            "bills_delegation_enabled": true,
        })));
        assert!(
            matches!(badge_for(&bills_only).1, BadgeVariant::Green),
            "the provider is doing something for this tenant, in the other direction"
        );
        // And the MAPPS-1154 rule holds for this direction too: delegated
        // by the tenant with the deployment switch off sends nothing, so it
        // does not read as working.
        let switched_off = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": false,
            "bills_delegated": true,
            "bills_delegation_enabled": false,
        })));
        assert!(!matches!(badge_for(&switched_off).1, BadgeVariant::Green));
        let nothing = card_state_of(&overview(serde_json::json!({
            "configured": true, "status": "connected", "delegated": false,
        })));
        assert!(
            !matches!(badge_for(&nothing).1, BadgeVariant::Green),
            "and a connection handing over neither direction still is not"
        );
    }

    /// Invoicing switched off at the deployment while BILLS flow is still
    /// a working connection.
    ///
    /// The pair-per-direction is why: a badge that read only the invoice
    /// pair called this idle while every bill the MSP entered was being
    /// raised in the provider.
    #[test]
    fn bills_flowing_outweighs_invoicing_switched_off() {
        let state = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": true,
            "delegation_enabled": false,
            "bills_delegated": true,
            "bills_delegation_enabled": true,
        })));
        assert!(matches!(badge_for(&state).1, BadgeVariant::Green));
    }

    #[test]
    fn connected_without_delegation_does_not_read_as_issuing() {
        let issuing = card_state_of(&overview(serde_json::json!({
            "configured": true, "status": "connected", "delegated": true,
        })));
        let idle = card_state_of(&overview(serde_json::json!({
            "configured": true, "status": "connected", "delegated": false,
        })));
        assert!(matches!(badge_for(&issuing).1, BadgeVariant::Green));
        assert!(
            !matches!(badge_for(&idle).1, BadgeVariant::Green),
            "a connection handing over nothing is not a working integration"
        );
        assert_ne!(badge_for(&issuing).0, badge_for(&idle).0);
    }

    /// MAPPS-1154: a tenant who delegated everything on a deployment that
    /// has not switched delegating on is not issuing through the provider,
    /// and the card does not say they are.
    ///
    /// This is the state every deployment is in to begin with, because the
    /// toggle defaults off. Before this the card read "Connected" in green
    /// with "New invoices are issued by Xero" while Mokosh issued every one
    /// of them locally.
    #[test]
    fn the_deployment_switch_being_off_does_not_read_as_issuing() {
        let tenant_ready_deployment_not = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": true,
            "delegation_enabled": false,
        })));
        assert!(
            !matches!(
                badge_for(&tenant_ready_deployment_not).1,
                BadgeVariant::Green
            ),
            "nothing is being issued by the provider, so this is not a working integration"
        );
        assert_eq!(
            badge_for(&tenant_ready_deployment_not).0,
            "Connected, not issuing",
            "the same badge as a tenant who delegated nothing: the consequence is the same"
        );

        let both = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "connected",
            "delegated": true,
            "delegation_enabled": true,
        })));
        assert!(matches!(badge_for(&both).1, BadgeVariant::Green));
    }

    /// An older server that does not send the field is read as switched on.
    ///
    /// It predates the card being able to tell, so the default that says
    /// nothing is better than the one that tells every connected tenant
    /// their invoices are going nowhere.
    #[test]
    fn an_older_server_is_not_assumed_to_have_it_switched_off() {
        let state = card_state_of(&overview(serde_json::json!({
            "configured": true, "status": "connected", "delegated": true,
        })));
        assert!(matches!(badge_for(&state).1, BadgeVariant::Green));
    }

    /// The deployment's switch and the tenant's are explained separately.
    ///
    /// A tenant admin cannot flip the deployment's, so the copy has to say
    /// that somebody else does and that the connection survives until they
    /// do; "nothing is delegated to it" would send them looking for a
    /// checkbox they do not have.
    #[test]
    fn the_two_switches_are_explained_as_two_different_things() {
        let src = include_str!("settings_invoice_issuer.rs");
        let card = &src[src
            .find(r#""data-testid": "invoice-issuer-"#)
            .expect("the card body")
            ..src.find("mod tests").expect("this module")];
        let deployment_copy = card
            .find("switched off for this deployment")
            .expect("the deployment switch has its own sentence");
        let tenant_copy = card
            .find("nothing is delegated to it")
            .expect("the tenant switch keeps its own");
        assert!(
            deployment_copy < tenant_copy,
            "both are rendered, in their own arms"
        );
        assert!(
            card[deployment_copy..tenant_copy].contains("An operator turns it on"),
            "the deployment sentence says who can fix it"
        );
    }

    /// A disconnected row offers the same thing as never having connected.
    ///
    /// The server keeps the two apart because a disconnect preserves the
    /// capability set; this page does not need to, and an admin looking at
    /// either one wants the same button.
    #[test]
    fn disconnected_and_never_connected_offer_the_same_thing() {
        for status in ["not_connected", "disconnected"] {
            let state = card_state_of(&overview(serde_json::json!({
                "configured": true, "status": status,
            })));
            assert_eq!(state, CardState::NeverConnected, "{status}");
        }
    }

    /// An error carries the provider's own message through.
    #[test]
    fn a_failing_connection_shows_what_the_provider_said() {
        let state = card_state_of(&overview(serde_json::json!({
            "configured": true,
            "status": "error",
            "last_error": "the grant was revoked",
        })));
        assert_eq!(
            state,
            CardState::Failing {
                message: Some("the grant was revoked".to_string())
            }
        );
    }

    /// MAPPS-1145: Disconnect is offered on a connected card and nowhere
    /// else.
    ///
    /// Read off the rendered body rather than asserted on a state machine,
    /// because the mistake worth catching is a button that escaped its
    /// branch: a Disconnect on an unconfigured card would post to a
    /// provider this deployment cannot reach, and one on a never-connected
    /// card would offer to undo something that has not happened.
    #[test]
    fn disconnect_belongs_to_the_connected_card() {
        let src = include_str!("settings_invoice_issuer.rs");
        // The rendered card only; the enum and `card_state_of` above it name
        // the same variants and are not what this is about.
        let card = &src[src
            .find(r#""data-testid": "invoice-issuer-"#)
            .expect("the card body")..];
        // The arm's own prefix, not its full pattern: MAPPS-1156 added two
        // fields to it, and a test that spells the whole pattern out finds
        // its own literal further down this file once the pattern moves on.
        let connected_at = card
            .find("CardState::Connected {")
            .expect("the connected arm");
        let failing_at = card
            .find("CardState::Failing { message } =>")
            .expect("the failing arm");
        assert!(
            card[connected_at..failing_at].contains("disconnect-{issuer.testid}"),
            "a connected card offers Disconnect"
        );
        assert!(
            !card[..connected_at].contains("disconnect-"),
            "and the cards before it do not: there is nothing to disconnect from"
        );
    }

    /// The confirmation says what disconnecting costs and what it keeps.
    ///
    /// Both halves matter to the decision. New invoices go back to Mokosh's
    /// own numbers, which is the visible change; what was already issued
    /// stays where it is, which is the fear.
    #[test]
    fn the_confirmation_says_what_it_does_and_what_it_leaves() {
        let src = include_str!("settings_invoice_issuer.rs");
        let message = &src[src.find("New invoices go back").expect("the copy")..];
        let message = &message[..message.find("\",").unwrap_or(message.len())];
        assert!(message.contains("issued by Mokosh"), "{message}");
        assert!(
            message.contains("stay where they are"),
            "an admin needs to know the provider's invoices are not withdrawn: {message}"
        );
        assert!(
            message.contains("remembered"),
            "and that reconnecting does not start from an empty page: {message}"
        );
    }

    /// Neither provider's copy names the other.
    ///
    /// The whole reason this is one page is that the two differ in a table
    /// and nowhere else; a literal that escaped the table is how that stops
    /// being true.
    #[test]
    fn neither_issuer_names_the_other() {
        let src = include_str!("settings_invoice_issuer.rs");
        let body = &src[src
            .find("fn IssuerSettingsBody")
            .expect("the body is in this file")
            ..src.find("mod tests").expect("this module")];
        for name in ["Xero", "QuickBooks", "Intuit"] {
            assert!(
                !body.contains(&format!("\"{name}")),
                "{name} is written into the shared body; it belongs in the table"
            );
        }
    }
}

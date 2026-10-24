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
    #[serde(default)]
    pub last_error: Option<String>,
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

/// The four states this card renders.
#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    /// The deployment holds no application for this provider.
    NotAvailable,
    NeverConnected,
    Connected {
        organisation_id: Option<String>,
        delegated: bool,
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
        CardState::Connected {
            delegated: true, ..
        } => ("Connected", BadgeVariant::Green),
        // Connected to the provider and handing it nothing is a real state
        // and not a good one: the connection is live and every invoice is
        // still issued locally.
        CardState::Connected {
            delegated: false, ..
        } => ("Connected, not issuing", BadgeVariant::Gray),
        CardState::Failing { .. } => ("Error", BadgeVariant::Red),
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
                                CardState::Connected { organisation_id, delegated } => rsx! {
                                    if let Some(id) = organisation_id.clone() {
                                        p { class: "text-sm text-content",
                                            "Connected to the {issuer.display} {issuer.organisation} "
                                            span { class: "font-medium", "{id}" }
                                            "."
                                        }
                                    }
                                    if delegated {
                                        p { class: "text-sm text-muted",
                                            "New invoices are issued by {issuer.display}. The number your customer sees is theirs; Mokosh keeps its own for reference."
                                        }
                                    } else {
                                        StatusBanner { tone: crate::components::BannerTone::Warning,
                                            "The connection is live and nothing is delegated to it, so Mokosh is still issuing every invoice."
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
        let connected_at = card
            .find("CardState::Connected { organisation_id, delegated } => rsx!")
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

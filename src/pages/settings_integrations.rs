//! Settings > Integrations (MAPPS-971).
//!
//! The page PMS-1310's last acceptance criterion asked for and that shipped
//! without: list every provider, show what each one can be handed, and let an
//! admin connect, disconnect and change what is delegated. Server routes are
//! under `/api/v1/integrations` and `src/modules/integrations/routes.rs` carried
//! a parity record saying nothing called them; this is what retires it.
//!
//! ## The server is the authority, and that is not a slogan here
//!
//! Every list on this page comes from the response: the providers and their
//! order, each one's description, its capability keys with their labels and
//! descriptions, whether it polls and the floor on that interval, whether its
//! connection is managed on another surface, and whether its credential is
//! entered on one. Nothing is hardcoded, because each of those has already
//! moved once. PMS-1312 moved the payment providers' connection INTO
//! integrations while leaving their credential on the payments surface, and
//! PMS-1447 had to add `credential_elsewhere` precisely because a page cannot
//! derive that from a provider name.
//!
//! ## Two kinds of "somewhere else", which are not the same thing
//!
//! `managed_elsewhere` means the CONNECTION is owned by another subsystem, so
//! there is nothing to connect here and the server answers 409. Google carries
//! it until PMS-1315. The card names the surface and offers no Connect.
//!
//! `credential_elsewhere` means the connection is ours but the CREDENTIAL is
//! entered elsewhere, so Connect works and takes no credential. Stripe and
//! PayPal carry it, and the server answers 400 for a credential sent here. The
//! card offers Connect with no field, plus a link to where the keys go.
//!
//! A page that collapsed the two would either hide Connect from Stripe or offer
//! it for Google, and both were states this surface existed to get right.
//!
//! ## Status is four values
//!
//! `not_connected`, `connected`, `disconnected` and `error`, and the middle two
//! are the reason it is not a boolean: `disconnected` keeps the capability set
//! so reconnecting does not start from an empty page, which a two-state toggle
//! cannot express. `error` carries `last_error` and it is shown, because an
//! integration that is failing and an integration that is off are different
//! problems for whoever is reading.

use dioxus::prelude::*;
use mokosh_types::integrations::IntegrationStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, Checkbox, ErrorBanner, Input,
    PageHeader,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

const INTEGRATIONS_PATH: &str = "/integrations";

/// One capability, label and description served with the key (PMS-1310), so the
/// checkbox list is the server's vocabulary rather than a copy of it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct CapabilityDescriptor {
    key: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    description: String,
}

/// Where a provider's CONNECTION lives, when it is not here.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ManagedElsewhere {
    #[serde(default)]
    configured_at: String,
    #[serde(default)]
    issue: String,
}

/// Where a provider's CREDENTIAL is entered, when it is not in the connect
/// request (PMS-1447).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CredentialElsewhere {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    entered_at: String,
}

/// The poll setting a provider offers, absent for one nothing polls.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Polling {
    #[serde(default)]
    default_minutes: i32,
    #[serde(default)]
    min_minutes: i32,
}

/// One row of `GET /integrations`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Integration {
    provider: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    supported_capabilities: Vec<CapabilityDescriptor>,
    #[serde(default)]
    enabled_capabilities: Vec<String>,
    status: IntegrationStatus,
    #[serde(default)]
    managed_elsewhere: Option<ManagedElsewhere>,
    #[serde(default)]
    credential_elsewhere: Option<CredentialElsewhere>,
    #[serde(default)]
    polling: Option<Polling>,
    #[serde(default)]
    poll_interval_minutes: Option<i32>,
    #[serde(default)]
    last_error: Option<String>,
}

impl Integration {
    /// Whether this card offers Connect at all.
    ///
    /// `managed_elsewhere` is the only thing that removes it. A credential
    /// entered elsewhere does NOT: the connection is still ours, which is the
    /// distinction PMS-1312 created and PMS-1447 made visible.
    fn connectable(&self) -> bool {
        self.managed_elsewhere.is_none()
    }

    /// Whether Connect should collect a credential.
    ///
    /// Exactly when the server did not say the credential lives somewhere else.
    /// By presence rather than by a provider list, so a provider that moves home
    /// moves with it.
    fn takes_credential(&self) -> bool {
        self.connectable() && self.credential_elsewhere.is_none()
    }
}

/// `PUT /integrations/{provider}`: the capability set, nothing else here.
#[derive(Debug, Serialize)]
struct UpdateBody {
    capabilities: Vec<String>,
}

/// `PUT /integrations/{provider}` for the poll interval alone.
#[derive(Debug, Serialize)]
struct PollBody {
    poll_interval_minutes: Option<i32>,
}

/// `POST /integrations/{provider}/connect`.
///
/// `credential` is omitted entirely for a provider whose credential is entered
/// elsewhere, because the server refuses one that is sent rather than ignoring
/// it.
#[derive(Debug, Serialize)]
struct ConnectBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    credential: Option<Value>,
}

/// `POST /integrations/{provider}/disconnect` takes no fields.
///
/// A type rather than a `json!({})` literal, because MAPPS-685's rule is that a
/// request body is a type: a literal object is where a field gets added one day
/// with nothing to check it against. The guard in this file's tests enforces it.
#[derive(Debug, Serialize)]
struct NoBody {}

/// The badge and the sentence beside it, per status.
///
/// A function rather than four branches at the call site, so the four stay
/// visibly four. `disconnected` says the capability set was kept, since that is
/// the whole reason the server distinguishes it from `not_connected`.
fn status_copy(status: IntegrationStatus) -> (&'static str, BadgeVariant, &'static str) {
    match status {
        IntegrationStatus::NotConnected => (
            "Not connected",
            BadgeVariant::Gray,
            "Nothing is delegated to this provider yet.",
        ),
        IntegrationStatus::Connected => (
            "Connected",
            BadgeVariant::Green,
            "Delegating the capabilities ticked below.",
        ),
        IntegrationStatus::Disconnected => (
            "Disconnected",
            BadgeVariant::Gray,
            "Switched off. What was ticked is kept, so connecting again does not start from nothing.",
        ),
        IntegrationStatus::Error => (
            "Error",
            BadgeVariant::Red,
            "Connected, and the last attempt to use it failed.",
        ),
    }
}

/// The settings surface a `credential_elsewhere` or `managed_elsewhere` string
/// points at, as a route this app can navigate to.
///
/// Matched on the credential-home KIND rather than on the human string, because
/// the string is prose for a reader and the kind is the contract. An unknown
/// kind renders the server's sentence with no link, which is the honest
/// degradation: a new credential home should not produce a link to the wrong
/// page.
fn surface_route(kind: &str) -> Option<Route> {
    match kind {
        "payment_gateway" => Some(Route::PaymentGatewayConfig {}),
        "contact_sync" => Some(Route::SettingsGoogleContacts {}),
        _ => None,
    }
}

#[component]
pub fn IntegrationsPage() -> Element {
    use_page_title("Integrations");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Integrations" } };
    }
    rsx! { IntegrationsList {} }
}

#[component]
fn IntegrationsList() -> Element {
    let mut error = use_signal(String::new);

    let mut resource = use_resource(move || async move {
        let _reachable = crate::hooks::use_server_reachable();
        crate::pages::settings::load_operator_setting::<Vec<Integration>>(
            INTEGRATIONS_PATH,
            "integrations",
        )
        .await
    });
    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(crate::pages::settings::OperatorLoad::Failed));
    let not_operator = matches!(
        *snap,
        Some(crate::pages::settings::OperatorLoad::NotOperator)
    );
    let integrations: Vec<Integration> = match &*snap {
        Some(crate::pages::settings::OperatorLoad::Loaded(list)) => list.clone(),
        _ => Vec::new(),
    };

    rsx! {
        PageHeader {
            title: "Integrations",
            subtitle: "What this organization delegates to other systems, and what each one is allowed to do.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsIntegrations {} }
            },
        }

        div { class: "space-y-6",
            if !error().is_empty() {
                ErrorBanner { class: "mb-4", "{error()}" }
            }
            if fetch_failed {
                Card { ErrorBanner { "Could not load the integrations." } }
            }
            if not_operator {
                Card {
                    p { class: "text-sm text-muted",
                        "Integrations are set up by an administrator of this organization."
                    }
                }
            }
            if is_loading {
                crate::components::DetailSkeleton {}
            }
            // Server order, not sorted here: the registry decides what comes
            // first and re-sorting would be a second opinion about it.
            for integration in integrations {
                IntegrationCard {
                    integration: integration.clone(),
                    on_changed: move |_| resource.restart(),
                    on_error: move |message: String| error.set(message),
                }
            }
        }
    }
}

#[component]
fn IntegrationCard(
    integration: Integration,
    on_changed: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut credential = use_signal(String::new);
    let mut poll_minutes = use_signal(|| {
        integration
            .poll_interval_minutes
            .or_else(|| integration.polling.as_ref().map(|p| p.default_minutes))
            .map(|m| m.to_string())
            .unwrap_or_default()
    });
    let can_mutate = crate::hooks::use_can_mutate();

    let (badge, tone, line) = status_copy(integration.status);
    let provider = integration.provider.clone();
    let connected = matches!(
        integration.status,
        IntegrationStatus::Connected | IntegrationStatus::Error
    );

    // Each call reports the server's own message. The server refuses six
    // distinct things here (a capability outside the supported set, a
    // credential for a provider that takes none, a missing credential for one
    // that needs it, a provider managed elsewhere, a poll interval under the
    // floor, a config that is not an object) and a generic "something went
    // wrong" would waste all six.
    let call = move |path: String, body: Value| {
        let on_changed = on_changed;
        let on_error = on_error;
        async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed_json_no_content(&path, &body).await {
                    Ok(()) => on_changed.call(()),
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        on_error.call(err.user_message());
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (path, body);
            }
        }
    };

    let handle_connect = {
        let provider = provider.clone();
        let takes_credential = integration.takes_credential();
        move |_| {
            if busy() {
                return;
            }
            let raw = credential.read().trim().to_string();
            if takes_credential && raw.is_empty() {
                on_error.call(
                    "This provider's credential is entered here, so Connect needs one.".to_string(),
                );
                return;
            }
            let body = ConnectBody {
                credential: if takes_credential {
                    Some(Value::String(raw))
                } else {
                    None
                },
            };
            let path = format!("{INTEGRATIONS_PATH}/{provider}/connect");
            let body = serde_json::to_value(&body).unwrap_or(Value::Null);
            busy.set(true);
            spawn(async move {
                call(path, body).await;
                credential.set(String::new());
                busy.set(false);
            });
        }
    };

    let handle_disconnect = {
        let provider = provider.clone();
        move |_| {
            if busy() {
                return;
            }
            let path = format!("{INTEGRATIONS_PATH}/{provider}/disconnect");
            let body = serde_json::to_value(NoBody {}).unwrap_or(Value::Null);
            busy.set(true);
            spawn(async move {
                call(path, body).await;
                busy.set(false);
            });
        }
    };

    // A capability change is a PUT of the whole set, which is what the server
    // takes: it stores the resolved list in registry order, so sending the
    // delta would mean reimplementing that ordering here.
    // `use_callback` rather than a plain closure: this is called once per
    // checkbox inside a loop, so it has to be `Copy`.
    let toggle_capability = use_callback({
        let provider = provider.clone();
        let enabled = integration.enabled_capabilities.clone();
        move |(key, on): (String, bool)| {
            let mut next: Vec<String> = enabled.clone();
            if on {
                if !next.contains(&key) {
                    next.push(key);
                }
            } else {
                next.retain(|k| k != &key);
            }
            let path = format!("{INTEGRATIONS_PATH}/{provider}");
            let body =
                serde_json::to_value(UpdateBody { capabilities: next }).unwrap_or(Value::Null);
            let on_changed = on_changed;
            let on_error = on_error;
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    match crate::hooks::fetch::api::put_authed_typed::<Integration, _>(&path, &body)
                        .await
                    {
                        Ok(_updated) => on_changed.call(()),
                        Err(err) => {
                            crate::hooks::push_api_error(&err);
                            on_error.call(err.user_message());
                        }
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = (path, body);
                }
            });
        }
    });

    let handle_poll_save = {
        let provider = provider.clone();
        move |_| {
            let raw = poll_minutes.read().trim().to_string();
            let parsed = if raw.is_empty() {
                None
            } else {
                match raw.parse::<i32>() {
                    Ok(v) => Some(v),
                    Err(_) => {
                        on_error.call("Enter the poll interval in whole minutes.".to_string());
                        return;
                    }
                }
            };
            let path = format!("{INTEGRATIONS_PATH}/{provider}");
            let body = serde_json::to_value(PollBody {
                poll_interval_minutes: parsed,
            })
            .unwrap_or(Value::Null);
            let on_changed = on_changed;
            let on_error = on_error;
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    match crate::hooks::fetch::api::put_authed_typed::<Integration, _>(&path, &body)
                        .await
                    {
                        Ok(_updated) => on_changed.call(()),
                        Err(err) => {
                            crate::hooks::push_api_error(&err);
                            on_error.call(err.user_message());
                        }
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = (path, body);
                }
            });
        }
    };

    rsx! {
        Card {
            // MAPPS-967: the provider IS this card's label, and its status badge is
            // the heading-row control, so the pair becomes title + actions.
            title: integration.display_name.clone(),
            actions: rsx! {
                Badge { variant: tone, "{badge}" }
            },
            div { class: "space-y-4",
                p { class: "text-sm text-muted", "{integration.description}" }
                p { class: "text-sm text-content", "{line}" }

                if let Some(last_error) = integration.last_error.as_ref() {
                    ErrorBanner { "{last_error}" }
                }

                // The connection is owned by another subsystem: say where, and
                // offer nothing, because connecting here is a 409.
                if let Some(elsewhere) = integration.managed_elsewhere.as_ref() {
                    div { class: "space-y-2",
                        p { class: "text-sm text-muted",
                            "This connection is set up under {elsewhere.configured_at}, not here."
                        }
                        if let Some(route) = surface_route("contact_sync") {
                            Link { to: route, class: "text-sm text-accent hover:underline",
                                "Go to {elsewhere.configured_at}"
                            }
                        }
                    }
                }

                if integration.connectable() {
                    div { class: "space-y-4",
                        // The capability set. Bounded by what the server says
                        // the provider supports; ticking anything else is a 400
                        // naming the legal set, so the list is never invented
                        // here.
                        if !integration.supported_capabilities.is_empty() {
                            div { class: "space-y-2",
                                p { class: "text-sm font-medium text-content", "What it may do" }
                                for capability in integration.supported_capabilities.iter() {
                                    Checkbox {
                                        name: "{integration.provider}_{capability.key}",
                                        label: capability.label.clone(),
                                        help: capability.description.clone(),
                                        checked: integration.enabled_capabilities.contains(&capability.key),
                                        disabled: !can_mutate || busy(),
                                        onchange: {
                                            let key = capability.key.clone();
                                            move |e: FormEvent| {
                                                toggle_capability.call((key.clone(), e.value() == "true"))
                                            }
                                        },
                                    }
                                }
                            }
                        }

                        // A poll interval only where the provider polls, with
                        // the floor the database enforces stated rather than
                        // discovered through a 400.
                        if let Some(polling) = integration.polling.as_ref() {
                            div { class: "max-w-xs",
                                Input {
                                    name: "{integration.provider}_poll",
                                    label: "Check every (minutes)",
                                    r#type: "number".to_string(),
                                    value: poll_minutes(),
                                    disabled: !can_mutate || busy(),
                                    help: format!("At least {} minutes. Blank uses the default of {}.", polling.min_minutes, polling.default_minutes),
                                    oninput: move |e: FormEvent| poll_minutes.set(e.value()),
                                }
                                div { class: "flex justify-end pt-2",
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        disabled: !can_mutate || busy(),
                                        onclick: handle_poll_save,
                                        "Save Interval"
                                    }
                                }
                            }
                        }

                        // The credential, exactly when the server did not say it
                        // is entered elsewhere.
                        if integration.takes_credential() && !connected {
                            Input {
                                name: "{integration.provider}_credential",
                                label: "Credential",
                                r#type: "password".to_string(),
                                value: credential(),
                                disabled: !can_mutate || busy(),
                                help: "Write-only: never shown once saved.".to_string(),
                                oninput: move |e: FormEvent| credential.set(e.value()),
                            }
                        }
                        if let Some(elsewhere) = integration.credential_elsewhere.as_ref() {
                            div { class: "space-y-1",
                                p { class: "text-sm text-muted",
                                    "The keys for this provider are entered under {elsewhere.entered_at}. Connecting here says what it may do; it does not ask for them again."
                                }
                                if let Some(route) = surface_route(&elsewhere.kind) {
                                    Link { to: route, class: "text-sm text-accent hover:underline",
                                        "Go to {elsewhere.entered_at}"
                                    }
                                }
                            }
                        }

                        div { class: "flex flex-wrap gap-3 pt-2",
                            if connected {
                                Button {
                                    variant: ButtonVariant::Danger,
                                    loading: busy(),
                                    disabled: !can_mutate,
                                    onclick: handle_disconnect,
                                    data_testid: "integration-disconnect-{integration.provider}",
                                    "Disconnect"
                                }
                            } else {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    loading: busy(),
                                    disabled: !can_mutate,
                                    onclick: handle_connect,
                                    data_testid: "integration-connect-{integration.provider}",
                                    "Connect"
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

    fn integration(json: serde_json::Value) -> Integration {
        let mut base = serde_json::json!({
            "provider": "quickbooks",
            "display_name": "QuickBooks",
            "description": "d",
            "supported_capabilities": [],
            "enabled_capabilities": [],
            "status": "not_connected",
            "config": {},
        });
        if let (Some(b), Some(o)) = (base.as_object_mut(), json.as_object()) {
            for (k, v) in o {
                b.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value(base).expect("the fixture deserialises")
    }

    /// Connect is offered unless the CONNECTION is managed elsewhere, and a
    /// credential is collected unless the CREDENTIAL is.
    ///
    /// The two are separate questions and this is the table that says so. A
    /// page that collapsed them would either hide Connect from Stripe, whose
    /// connection is ours, or offer it for Google, whose is not, and both were
    /// states this surface exists to get right (PMS-1312, PMS-1447).
    #[test]
    fn connect_and_the_credential_field_answer_two_different_questions() {
        let ours = integration(serde_json::json!({}));
        assert!(ours.connectable());
        assert!(
            ours.takes_credential(),
            "QuickBooks' credential arrives here"
        );

        let payments = integration(serde_json::json!({
            "provider": "stripe",
            "credential_elsewhere": { "kind": "payment_gateway", "entered_at": "Settings > Payment Gateways" },
        }));
        assert!(
            payments.connectable(),
            "the payment providers' connection is ours since PMS-1312"
        );
        assert!(
            !payments.takes_credential(),
            "and their keys are entered on the payments surface, so Connect takes none"
        );

        let google = integration(serde_json::json!({
            "provider": "google",
            "managed_elsewhere": { "configured_at": "Settings > Contact sync", "issue": "PMS-1315" },
            "credential_elsewhere": { "kind": "contact_sync", "entered_at": "Settings > Google Contacts" },
        }));
        assert!(
            !google.connectable(),
            "connecting Google here is a 409 until PMS-1315"
        );
        assert!(!google.takes_credential());
    }

    /// Every credential home the server can report either resolves to a route
    /// or renders without one, and never to the wrong page.
    ///
    /// Matched on the `kind` rather than on `entered_at`, because the string is
    /// prose for a reader and the kind is the contract. An unknown kind has to
    /// degrade to no link: a credential home added server-side should not
    /// silently point an admin at the payments page.
    #[test]
    fn a_credential_home_resolves_to_its_own_page_or_to_none() {
        assert_eq!(
            surface_route("payment_gateway"),
            Some(Route::PaymentGatewayConfig {})
        );
        assert_eq!(
            surface_route("contact_sync"),
            Some(Route::SettingsGoogleContacts {})
        );
        assert_eq!(
            surface_route("a_home_added_after_this_was_written"),
            None,
            "an unknown credential home must not resolve to some other page"
        );
    }

    /// All four statuses are distinct, and `disconnected` says what it keeps.
    ///
    /// The server keeps `not_connected` and `disconnected` apart on purpose:
    /// the second holds the capability set so reconnecting does not start from
    /// an empty page. If this page rendered them identically that distinction
    /// would be invisible, which is the same as not having it.
    #[test]
    fn the_four_statuses_read_differently_and_disconnected_explains_itself() {
        let all = [
            IntegrationStatus::NotConnected,
            IntegrationStatus::Connected,
            IntegrationStatus::Disconnected,
            IntegrationStatus::Error,
        ];
        let badges: Vec<&str> = all.iter().map(|s| status_copy(*s).0).collect();
        let lines: Vec<&str> = all.iter().map(|s| status_copy(*s).2).collect();
        for set in [&badges, &lines] {
            let mut seen = set.clone();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), all.len(), "two statuses read the same: {set:?}");
        }
        assert!(
            status_copy(IntegrationStatus::Disconnected)
                .2
                .contains("kept"),
            "disconnected has to say the capability set survives"
        );
        assert!(
            matches!(status_copy(IntegrationStatus::Error).1, BadgeVariant::Red),
            "an integration that is failing cannot look the same as one that is off"
        );
    }

    /// The page holds no provider list, no capability list and no status
    /// vocabulary of its own.
    ///
    /// Scanned as source because that is where the mistake would be made. Each
    /// of these has already moved once on the server (PMS-1312 moved two
    /// providers' connection home, PMS-1447 added a credential home), so a copy
    /// here would be wrong on the next move rather than merely redundant.
    #[test]
    fn the_page_invents_no_list_the_server_is_authoritative_about() {
        let src = include_str!("settings_integrations.rs");
        let all = &src[..src.find("mod tests").expect("this module")];
        // Comment lines are exempt: this file EXPLAINS why a request body is a
        // type rather than a literal, and a guard that cannot be documented
        // gets reworded instead of understood. What it defends is the code.
        let head: String = all
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let head = head.as_str();
        for banned in [
            "\"quickbooks\"",
            "\"xero\"",
            "\"microsoft\"",
            "\"paypal\"",
            "\"invoicing\"",
            "\"bills_and_expenses\"",
            "\"point_of_sale\"",
        ] {
            assert!(
                !head.contains(banned),
                "{banned} is the server's to decide; this page renders what it is given"
            );
        }
        // The two it DOES name are credential-home kinds, which it matches on
        // to pick a route, and that is the contract rather than a list of
        // providers or capabilities.
        assert!(head.contains("\"payment_gateway\"") && head.contains("\"contact_sync\""));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
    }
}

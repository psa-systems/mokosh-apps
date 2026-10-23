//! Settings > Integrations > Microsoft 365 Contacts (MAPPS-1124, server
//! MAPPS-1101).
//!
//! The third contact-sync card. MAPPS-1101 shipped the whole server side (the
//! Graph client, the Entra consent round trip, the provider in `SUPPORTED`,
//! the sync dispatch) and no surface in this app, so an admin could not start
//! the flow and the URL `contact_sync::runs` builds for a Microsoft run
//! (`/settings/integrations/microsoft-contacts`) resolved to nothing.
//!
//! # What it borrows, and from which sibling
//!
//! Connecting is Google's shape: an Entra consent screen the browser leaves
//! for, not a credential typed here, which is what the iCloud card does. The
//! lifecycle states and the buttons each one offers are the shared
//! [`card_state_of`] and [`actions_for`]; only the copy is local, because the
//! states mean Microsoft things here: a revoked grant is re-consented at
//! Microsoft, and what is selected for import is FOLDERS, which is what Graph
//! calls the thing Google calls a label and Apple calls a group.
//!
//! # Two switches, and the one a tenant cannot touch
//!
//! `microsoft_enabled` is the organization's own switch (server PMS-1512) and
//! is editable here. `microsoft_configured` says whether the deployment's host
//! holds an Entra ID client at all; a tenant cannot do anything about that, so
//! the card says the integration is unavailable rather than offering a Connect
//! whose only outcome is an error nobody on this side can act on.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, ErrorBanner,
    PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::pages::settings_contact_sync::{
    actions_for, card_state_of, disconnect_message_from, CardState, Connection, Overview,
    RunProgress, StateCopy, StateIcon,
};
use crate::Route;

/// The status read: one response carries every provider's connection.
const STATUS_PATH: &str = "/integrations/contact-sync";

/// The tenant switch for this integration (server PMS-1512).
const SETTING_CATEGORY: &str = "integrations";
const SETTING_KEY: &str = "microsoft_contacts_enabled";

/// How often the card re-reads while an import is queued or running.
const POLL_MS: u32 = 3_000;

/// What the admin is agreeing to before they leave for Microsoft's consent
/// screen (the PSA-70 K shape, Microsoft's version).
///
/// Said before the redirect and not after, because the consent screen is where
/// a customer's directory starts moving into a shared system and the moment to
/// understand that is before it happens.
pub const CONSENT_POINTS: &[&str] = &[
    "Mokosh asks Microsoft for read-only access to the signed-in account's contacts.",
    "Nothing is imported until you choose folders: connecting alone moves no data.",
    "Imports are one way. Nothing writes back to Microsoft 365.",
    "An admin can disconnect here at any time, and imported contacts stay in Mokosh.",
];

/// The badge, headline and next step for each state, in Microsoft's words.
pub fn copy_for(state: &CardState) -> StateCopy {
    match state {
        CardState::TurnedOff { connected_account } => StateCopy {
            badge: "Turned off",
            tone: BadgeVariant::Gray,
            headline: "Microsoft 365 Contacts is turned off for this organization.".to_string(),
            next_step: match connected_account {
                Some(account) => format!(
                    "Nothing syncs while it is off. The connection to {account} and every imported contact are kept; turn it back on below to resume."
                ),
                None => "Nobody can connect a Microsoft 365 account while it is off. Turn it on below to allow it.".to_string(),
            },
        },
        CardState::NotConfigured => StateCopy {
            badge: "Unavailable here",
            tone: BadgeVariant::Gray,
            headline: "This deployment has no Microsoft 365 sign-in client.".to_string(),
            next_step:
                "Whoever runs this deployment registers one Entra ID application for everybody on it. There is nothing to set up in this organization."
                    .to_string(),
        },
        CardState::NeverConnected => StateCopy {
            badge: "Not connected",
            tone: BadgeVariant::Gray,
            headline: "No Microsoft 365 account is connected.".to_string(),
            next_step:
                "Connect one below. Microsoft asks for consent to read contacts; nothing is imported until you then choose folders."
                    .to_string(),
        },
        CardState::Syncing { waiting, .. } => StateCopy {
            badge: if *waiting { "Waiting" } else { "Importing" },
            tone: BadgeVariant::Blue,
            headline: if *waiting {
                "Microsoft asked us to wait, so the import is paused.".to_string()
            } else {
                "Importing contacts from Microsoft 365.".to_string()
            },
            next_step: if *waiting {
                "It resumes by itself. Nothing is lost and you can leave this page.".to_string()
            } else {
                "You can leave this page; the import keeps running.".to_string()
            },
        },
        CardState::ReconnectRequired { account } => StateCopy {
            badge: "Consent needed",
            tone: BadgeVariant::Red,
            headline: format!("Microsoft has revoked Mokosh's access to {account}."),
            next_step:
                "Nothing imports until somebody consents again. Reconnect below; the account owner or a Microsoft admin may have to approve it."
                    .to_string(),
        },
        CardState::NoLabelsChosen { account } => StateCopy {
            badge: "No folders chosen",
            tone: BadgeVariant::Yellow,
            headline: format!("{account} is connected and nothing is selected."),
            next_step:
                "Nothing is imported until you choose folders. An empty selection never means every contact."
                    .to_string(),
        },
        CardState::Throttled { account } => StateCopy {
            badge: "Waiting",
            tone: BadgeVariant::Yellow,
            headline: format!("Microsoft asked us to slow down on {account}."),
            next_step:
                "Nothing is wrong with the connection. The next import starts once Microsoft's limit clears."
                    .to_string(),
        },
        CardState::FailingRepeatedly {
            account, failures, ..
        } => StateCopy {
            badge: "Failing",
            tone: BadgeVariant::Red,
            headline: format!("The last {failures} imports from {account} failed."),
            next_step:
                "Import now to see the error. If it names consent, reconnect; otherwise the detail says what Microsoft refused."
                    .to_string(),
        },
        CardState::PartiallyImported { account, .. } => StateCopy {
            badge: "Partly imported",
            tone: BadgeVariant::Yellow,
            headline: format!("The last import from {account} finished with records left out."),
            next_step: "Review what was held back, then import again.".to_string(),
        },
        CardState::Failed { account, .. } => StateCopy {
            badge: "Last import failed",
            tone: BadgeVariant::Red,
            headline: format!("The last import from {account} did not finish."),
            next_step: "Import now to retry. The run detail says what Microsoft returned."
                .to_string(),
        },
        CardState::Healthy { account, last_sync_at } => StateCopy {
            badge: "Connected",
            tone: BadgeVariant::Green,
            headline: format!("Importing contacts from {account}."),
            next_step: match last_sync_at {
                Some(at) => format!("Last import {}.", crate::utils::datetime::fmt_relative(*at)),
                None => "The first import runs on the next schedule, or start one now.".to_string(),
            },
        },
    }
}

#[derive(Serialize)]
struct NoBody {}

#[derive(Serialize)]
struct SettingWrite {
    category: &'static str,
    key: &'static str,
    value: bool,
}

/// `POST /integrations/contact-sync/microsoft/authorize` answers where to send
/// the browser. The SPA navigates rather than the server redirecting, so this
/// page can say what is about to happen first (PSA-70 K).
#[derive(Deserialize)]
struct AuthorizeResponse {
    #[serde(default)]
    authorize_url: String,
}

#[component]
pub fn MicrosoftContactsSettingsPage() -> Element {
    use_page_title("Microsoft 365 Contacts");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Microsoft 365 Contacts" } };
    }
    rsx! { MicrosoftContactsSettingsBody {} }
}

#[component]
fn MicrosoftContactsSettingsBody() -> Element {
    let mut overview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Overview>(STATUS_PATH)
            .await
            .inspect_err(|e| tracing::error!("contact sync status load failed: {e}"))
            .ok()
    });
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);
    let mut confirm_off = use_signal(|| false);
    let navigator = use_navigator();
    let can_mutate = crate::hooks::use_can_mutate();

    let snap = overview.read_unchecked().clone();
    let data = snap.as_ref().and_then(|o| o.as_ref()).cloned();
    let state = data.as_ref().map(|o| {
        card_state_of(
            o.microsoft_enabled,
            o.microsoft_configured,
            o.microsoft_connection.as_ref(),
        )
    });
    let syncing = matches!(state, Some(CardState::Syncing { .. }));

    // Re-read while an import is in flight, and stop when it is not.
    let mut polling = use_signal(|| false);
    use_effect(move || {
        if syncing && !polling() {
            polling.set(true);
            spawn(async move {
                crate::platform::timer::sleep_ms(POLL_MS).await;
                polling.set(false);
                overview.restart();
            });
        }
    });

    let mut connect = move || {
        busy.set(true);
        error.set(String::new());
        notice.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<AuthorizeResponse, _>(
                    "/integrations/contact-sync/microsoft/authorize",
                    &NoBody {},
                )
                .await
                {
                    Ok(resp) => {
                        // Microsoft's consent screen, then back here with
                        // `?contact_sync=` set. Busy stays on: this page is
                        // about to be replaced.
                        #[cfg(target_arch = "wasm32")]
                        if let Err(e) = crate::platform::location::set_href(&resp.authorize_url) {
                            error.set(format!("Could not open Microsoft's sign-in page: {e}"));
                            busy.set(false);
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            // A desktop host has no document to navigate; say
                            // where Microsoft is rather than doing nothing.
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

    let mut post = move |path: &'static str, after: &'static str| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    path,
                    &NoBody {},
                )
                .await
                {
                    Ok(_) => overview.restart(),
                    Err(e) => error.set(format!("{after}: {e}")),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (path, after);
            busy.set(false);
        });
    };

    let mut write_enabled = move |value: bool| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = SettingWrite {
                    category: SETTING_CATEGORY,
                    key: SETTING_KEY,
                    value,
                };
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    "/settings",
                    &body,
                )
                .await
                {
                    Ok(_) => overview.restart(),
                    Err(e) => error.set(format!("Could not change the setting: {e}")),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = value;
            busy.set(false);
        });
    };

    rsx! {
        PageHeader {
            title: "Microsoft 365 Contacts",
            subtitle: "Import contacts from a Microsoft 365 account into Mokosh through Microsoft Graph. One way only: Mokosh never writes back.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsMicrosoftContacts {} }
            },
        }
        if !notice().is_empty() {
            StatusBanner { tone: crate::components::BannerTone::Success, class: "mb-4", "{notice}" }
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match (data, state) {
            (None, _) | (_, None) => rsx! {
                crate::components::DetailSkeleton {}
            },
            (Some(data), Some(state)) => {
                let copy = copy_for(&state);
                let actions = actions_for(&state);
                let connection: Option<Connection> = data.microsoft_connection.clone();
                let connected_account = connection.as_ref().map(|c| c.account_email.clone());
                let disabled = busy() || !can_mutate;
                let enabled = data.microsoft_enabled;
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            div {
                                class: "flex flex-wrap items-center gap-3",
                                "aria-live": "polite",
                                "data-testid": "microsoft-sync-state",
                                span { "aria-hidden": "true", class: "text-muted",
                                    StateIcon { state: state.clone() }
                                }
                                Badge { variant: copy.tone, "{copy.badge}" }
                                p { class: "text-sm font-medium text-content", "{copy.headline}" }
                            }
                            p { class: "text-sm text-muted", "{copy.next_step}" }
                            if let CardState::Syncing { run, .. } = &state {
                                RunProgress { run: run.clone(), source: "Microsoft 365".to_string() }
                            }
                            if let Some(connection) = connection.as_ref() {
                                dl { class: "grid gap-3 text-sm sm:grid-cols-2",
                                    div {
                                        dt { class: "text-muted", "Microsoft account" }
                                        dd { class: "text-content", "{connection.account_email}" }
                                    }
                                    if connection.open_reviews > 0 {
                                        div {
                                            dt { class: "text-muted", "Waiting for review" }
                                            dd { class: "text-content", "{connection.open_reviews}" }
                                        }
                                    }
                                    if connection.deleted_in_source > 0 {
                                        div {
                                            dt { class: "text-muted", "Deleted in Microsoft 365" }
                                            dd { class: "text-content",
                                                "{connection.deleted_in_source}, kept in Mokosh"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                if actions.connect || actions.reconnect {
                                    Button {
                                        disabled,
                                        variant: ButtonVariant::Primary,
                                        onclick: move |_| connect(),
                                        data_testid: "microsoft-sync-connect",
                                        if actions.reconnect { "Reconnect with Microsoft" } else { "Connect a Microsoft 365 account" }
                                    }
                                }
                                if actions.choose_labels {
                                    Button {
                                        disabled,
                                        variant: if matches!(state, CardState::NoLabelsChosen { .. }) { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                                        onclick: move |_| { navigator.push(Route::SettingsMicrosoftContactsImport {}); },
                                        data_testid: "microsoft-sync-choose-folders",
                                        "Choose folders to import"
                                    }
                                }
                                if actions.sync_now {
                                    Button {
                                        disabled,
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| post("/integrations/contact-sync/runs?provider=microsoft", "Could not start the import"),
                                        data_testid: "microsoft-sync-now",
                                        "Import now"
                                    }
                                }
                                if actions.disconnect {
                                    Button {
                                        disabled,
                                        variant: ButtonVariant::Danger,
                                        onclick: move |_| confirm_disconnect.set(true),
                                        data_testid: "microsoft-sync-disconnect",
                                        "Disconnect"
                                    }
                                }
                            }
                        }
                    }

                    // Said before the redirect, not after: the consent screen is
                    // where the data starts moving.
                    if enabled && data.microsoft_configured && (actions.connect || actions.reconnect) {
                        Card { class: "mt-4",
                            title: "What connecting does",
                            ul { class: "list-disc space-y-1 pl-5",
                                for point in CONSENT_POINTS.iter() {
                                    li { class: "text-sm text-muted", "{point}" }
                                }
                            }
                        }
                    }

                    Card { class: "mt-4",
                        title: "Allow Microsoft 365 Contacts for this organization",
                        subtitle: "Off stops every import and hides the connect button. Nothing already imported is removed.".to_string(),
                        div { class: "flex flex-wrap items-center gap-3",
                            Button {
                                disabled,
                                variant: if enabled { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                                onclick: move |_| {
                                    if enabled { confirm_off.set(true) } else { write_enabled(true) }
                                },
                                data_testid: "microsoft-sync-toggle",
                                if enabled { "Turn off" } else { "Turn on" }
                            }
                            span { class: "text-sm text-muted",
                                if enabled { "On: an admin may connect an account." } else { "Off: nothing syncs and nobody can connect." }
                            }
                        }
                    }

                    ConfirmDialog {
                        open: confirm_disconnect(),
                        title: "Disconnect Microsoft 365 Contacts?",
                        message: disconnect_message_from("Microsoft 365", connected_account.as_deref()),
                        confirm_text: "Disconnect",
                        destructive: true,
                        loading: busy(),
                        onconfirm: move |_| {
                            confirm_disconnect.set(false);
                            post("/integrations/contact-sync/microsoft/disconnect", "Could not disconnect");
                        },
                        oncancel: move |_| confirm_disconnect.set(false),
                    }
                    ConfirmDialog {
                        open: confirm_off(),
                        title: "Turn Microsoft 365 Contacts off?",
                        message: "Imports stop for everyone in this organization. The connection and every imported contact are kept, and turning it back on resumes.".to_string(),
                        confirm_text: "Turn off",
                        destructive: true,
                        loading: busy(),
                        onconfirm: move |_| {
                            confirm_off.set(false);
                            write_enabled(false);
                        },
                        oncancel: move |_| confirm_off.set(false),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::settings_contact_sync::card_state_of;

    fn overview(json: serde_json::Value) -> Overview {
        serde_json::from_value(json).expect("the fixture deserialises")
    }

    fn state_of(o: &Overview) -> CardState {
        card_state_of(
            o.microsoft_enabled,
            o.microsoft_configured,
            o.microsoft_connection.as_ref(),
        )
    }

    fn connected(sync_status: &str, selected: serde_json::Value) -> Overview {
        overview(serde_json::json!({
            "enabled": true,
            "configured": true,
            "microsoft_enabled": true,
            "microsoft_configured": true,
            "microsoft_connection": {
                "id": "3f1c2f1e-0000-4000-8000-00000000abcd",
                "account_email": "ops@msp.onmicrosoft.example",
                "sync_status": sync_status,
                "selected_groups": selected,
                "consecutive_failures": 0,
            },
        }))
    }

    /// A deployment with no Entra client says so, instead of offering a Connect
    /// whose only outcome is an error nobody in this organization can act on.
    ///
    /// This is the state the Google card has had since PMS-1430 and the iCloud
    /// card can never reach: Apple needs no deployment-level client. It is
    /// reachable here, which is why the overview had to grow
    /// `microsoft_configured` rather than the page assuming true.
    #[test]
    fn a_deployment_with_no_entra_client_is_unavailable_not_disconnected() {
        let no_client = overview(serde_json::json!({
            "enabled": true, "configured": true, "microsoft_enabled": true,
        }));
        assert_eq!(state_of(&no_client), CardState::NotConfigured);
        let copy = copy_for(&state_of(&no_client));
        assert_eq!(copy.badge, "Unavailable here");
        assert!(
            copy.next_step.contains("whoever runs this deployment")
                || copy.next_step.contains("Whoever runs this deployment"),
            "it says who can fix it, which is not this tenant: {copy:?}"
        );
        assert_eq!(
            actions_for(&state_of(&no_client)),
            crate::pages::settings_contact_sync::Actions::default(),
            "nothing is offered when nothing can work"
        );
    }

    /// An absent switch reads as enabled, the server's own default; an absent
    /// `microsoft_configured` reads as NOT configured.
    ///
    /// The two defaults point in opposite directions on purpose. A server too
    /// old to send the switch has the setting on, and a server too old to send
    /// the client flag has no Microsoft flow at all, so Connect there would
    /// produce an error with nothing an admin could do about it.
    #[test]
    fn the_two_absent_fields_default_in_opposite_directions() {
        let bare = overview(serde_json::json!({ "enabled": true, "configured": true }));
        assert!(bare.microsoft_enabled, "the switch defaults on");
        assert!(!bare.microsoft_configured, "the client flag defaults off");
        assert_eq!(state_of(&bare), CardState::NotConfigured);
    }

    /// Connect is offered when a consent round trip is what is needed, and a
    /// revoked grant is reconnected rather than re-credentialed.
    #[test]
    fn connect_is_offered_when_consent_is_what_is_needed() {
        let never = overview(serde_json::json!({
            "enabled": true, "configured": true,
            "microsoft_enabled": true, "microsoft_configured": true,
        }));
        let actions = actions_for(&state_of(&never));
        assert!(actions.connect && !actions.reconnect);

        let revoked = connected("reconnect_required", serde_json::json!(["folder-1"]));
        let actions = actions_for(&state_of(&revoked));
        assert!(actions.reconnect && actions.disconnect);
        let copy = copy_for(&state_of(&revoked));
        assert_eq!(copy.badge, "Consent needed");

        let healthy = connected("success", serde_json::json!(["folder-1"]));
        let actions = actions_for(&state_of(&healthy));
        assert!(!actions.connect && !actions.reconnect);
        assert!(actions.sync_now && actions.choose_labels);
    }

    /// Nothing is imported until folders are chosen, and the card says so
    /// rather than reading as healthy.
    #[test]
    fn a_connection_with_no_folders_says_nothing_is_imported() {
        let none = connected("success", serde_json::json!([]));
        assert_eq!(
            state_of(&none),
            CardState::NoLabelsChosen {
                account: "ops@msp.onmicrosoft.example".to_string()
            }
        );
        let copy = copy_for(&state_of(&none));
        assert!(copy.next_step.contains("Nothing is imported"), "{copy:?}");
        assert!(
            copy.badge.contains("folders"),
            "the word is Microsoft's, not Google's or Apple's: {copy:?}"
        );
    }

    /// Every state's copy is Microsoft's, naming neither sibling provider.
    #[test]
    fn no_state_names_another_provider() {
        for status in ["success", "failed", "throttled", "reconnect_required"] {
            for selected in [serde_json::json!([]), serde_json::json!(["folder-1"])] {
                let copy = copy_for(&state_of(&connected(status, selected)));
                assert!(
                    !copy.headline.is_empty() && !copy.next_step.is_empty(),
                    "{copy:?}"
                );
                for foreign in ["Google", "iCloud", "Apple"] {
                    assert!(
                        !copy.headline.contains(foreign) && !copy.next_step.contains(foreign),
                        "a Microsoft state must not name {foreign}: {copy:?}"
                    );
                }
            }
        }
    }

    /// The consent block says what is about to happen before the browser
    /// leaves, which is the one moment it is worth saying (PSA-70 K).
    #[test]
    fn the_consent_block_is_said_before_the_redirect() {
        let all = CONSENT_POINTS.join(" ");
        assert!(all.contains("read-only"));
        assert!(all.contains("Nothing is imported until you choose folders"));
        assert!(all.contains("Nothing writes back"));
    }

    /// The page is admin-only, and the one shared route it calls names the
    /// provider, or Import now would start a run on the tenant's GOOGLE
    /// connection while the button says Microsoft 365.
    #[test]
    fn the_page_is_admin_only_and_the_shared_call_names_the_provider() {
        let src = include_str!("settings_contact_sync_microsoft.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("use_is_admin"), "admin gate");
        assert!(head.contains("AdminOnlyNotice"));
        const SHARED: &str = "/integrations/contact-sync/runs";
        let at = head.find(SHARED).expect("the shared route is called");
        assert!(
            head[at..].starts_with(&format!("{SHARED}?provider=microsoft")),
            "{SHARED} must name the provider"
        );
    }
}

//! Settings > Integrations > iCloud Contacts (PMS-1409, server PMS-1341).
//!
//! The Google card's sibling, and it differs in exactly one way that matters:
//! there is no consent screen to send anybody to. Apple publishes no OAuth scope
//! for contacts, so the credential is an Apple ID and an app-specific password
//! typed into this page, which is why connecting here is a FORM where the Google
//! card has a button that leaves the app.
//!
//! # What is shared and what is not
//!
//! The lifecycle states are the same states, so
//! [`card_state_of`](crate::pages::settings_contact_sync::card_state_of) decides
//! them and [`actions_for`](crate::pages::settings_contact_sync::actions_for)
//! says which buttons a state offers. What is local is the copy, because two of
//! the states mean something different here: a credential that stopped working
//! is a REVOKED APP-SPECIFIC PASSWORD, reissued at appleid.apple.com, so
//! "reconnect the account" is advice that cannot work, and a rate limit is
//! Apple's rather than Google's. `configured` is always true: there is no
//! deployment-level client to set up, so the state the Google card shows an
//! operator is unreachable here.
//!
//! # The password is never read back
//!
//! The app-specific password goes to the server's secret provider and no read
//! returns it, so the field is always empty when the page opens, even for a
//! connected account. Reconnecting means typing a new one, which is what
//! reissuing at Apple gives you anyway.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, Checkbox, ConfirmDialog,
    ErrorBanner, Input, PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::pages::settings_contact_sync::{
    actions_for, card_state_of, disconnect_message_from, CardState, Connection, Overview,
    RunProgress, StateCopy, StateIcon,
};
use crate::Route;

/// The status read: one response carries both providers' connections.
const STATUS_PATH: &str = "/integrations/contact-sync";

/// The tenant switch for this integration (server PMS-1341).
const SETTING_CATEGORY: &str = "integrations";
const SETTING_KEY: &str = "icloud_contacts_enabled";

/// Where Apple issues an app-specific password. Named in the field's help and in
/// every refusal about the credential, because it is the one place the admin has
/// to go and it is not where they would look.
pub const APPLE_ID_URL: &str = "https://appleid.apple.com";

/// How often the card re-reads while an import is queued or running.
const POLL_MS: u32 = 3_000;

/// What the admin is agreeing to before they paste a password (the PSA-70 K
/// shape, iCloud's version).
///
/// The second point is the one worth having: an app-specific password is not a
/// scope, so "read-only" here is a property of what Mokosh does with it rather
/// than of what Apple granted, and saying so is more honest than implying Apple
/// limited it.
pub const CONSENT_POINTS: &[&str] = &[
    "Read: names, email addresses, phone numbers, company, job title and department, plus which iCloud groups each contact is in. Photos, addresses and birthdays are not read.",
    "An app-specific password is not a limited permission: it can read and write your whole iCloud account. Mokosh only ever reads contacts, and nothing writes back, which a source-level guard in the server enforces.",
    "Where it goes: contacts you choose to import are stored in this organization's Mokosh CRM. Nothing is imported until you pick groups and start an import.",
    "Who sees it: everyone in your organization who can see contacts in Mokosh. Your clients' portal users do not.",
    "You can revoke the password at appleid.apple.com at any time, or disconnect here. Imported contacts stay as local records.",
];

/// The copy for each state, in iCloud's terms.
pub fn copy_for(state: &CardState) -> StateCopy {
    match state {
        CardState::TurnedOff { connected_account } => StateCopy {
            badge: "Turned off",
            tone: BadgeVariant::Gray,
            headline: "iCloud Contacts is turned off for this organization.".to_string(),
            next_step: match connected_account {
                Some(account) => format!(
                    "Nothing syncs while it is off. The connection to {account} and every imported contact are kept; turn it back on below to resume."
                ),
                None => "Nobody can connect an iCloud account while it is off. Turn it on below to allow it.".to_string(),
            },
        },
        // Unreachable here: there is no deployment-level client for iCloud, so
        // the card passes `configured: true`. Worded rather than `unreachable!`,
        // because a panic in a render is a blank page.
        CardState::NotConfigured => StateCopy {
            badge: "Not available",
            tone: BadgeVariant::Gray,
            headline: "iCloud Contacts is not available on this deployment.".to_string(),
            next_step: "Ask your Mokosh provider about the iCloud contact import.".to_string(),
        },
        CardState::NeverConnected => StateCopy {
            badge: "Not connected",
            tone: BadgeVariant::Gray,
            headline: "No iCloud account is connected.".to_string(),
            next_step: format!(
                "Enter the Apple ID and an app-specific password generated at {APPLE_ID_URL}. Nothing is imported until you choose groups."
            ),
        },
        CardState::Syncing { waiting, .. } => StateCopy {
            badge: if *waiting { "Waiting" } else { "Importing" },
            tone: BadgeVariant::Blue,
            headline: if *waiting {
                "Apple asked us to wait, so the import is paused.".to_string()
            } else {
                "Importing contacts from iCloud.".to_string()
            },
            next_step: if *waiting {
                "It resumes by itself. Nothing is lost and you can leave this page.".to_string()
            } else {
                "You can leave this page; the import keeps running.".to_string()
            },
        },
        CardState::ReconnectRequired { account } => StateCopy {
            badge: "Password rejected",
            tone: BadgeVariant::Red,
            headline: format!("Apple no longer accepts the app-specific password for {account}."),
            next_step: format!(
                "Generate a new app-specific password at {APPLE_ID_URL} and enter it below. Nothing is imported until you do, and nothing already imported is affected."
            ),
        },
        CardState::Throttled { account } => StateCopy {
            badge: "Waiting",
            tone: BadgeVariant::Yellow,
            headline: format!("Apple is rate limiting the connection to {account}."),
            next_step: "Imports resume by themselves. Nothing is lost.".to_string(),
        },
        CardState::FailingRepeatedly {
            account,
            failures,
            error,
        } => StateCopy {
            badge: "Failing",
            tone: BadgeVariant::Red,
            headline: format!("The last {failures} imports from {account} failed."),
            next_step: match error {
                Some(error) => format!("{error} Try an import now; if it keeps failing, reconnect with a new app-specific password."),
                None => "Try an import now; if it keeps failing, reconnect with a new app-specific password.".to_string(),
            },
        },
        CardState::PartiallyImported {
            account,
            landed,
            failed,
        } => StateCopy {
            badge: "Partly imported",
            tone: BadgeVariant::Yellow,
            headline: format!("{landed} contacts landed from {account} and {failed} did not."),
            next_step: "The reasons are below. The next import retries them.".to_string(),
        },
        CardState::Failed { account, error } => StateCopy {
            badge: "Failed",
            tone: BadgeVariant::Red,
            headline: format!("The last import from {account} failed."),
            next_step: match error {
                Some(error) => error.clone(),
                None => "Try an import now.".to_string(),
            },
        },
        CardState::NoLabelsChosen { account } => StateCopy {
            badge: "Nothing chosen",
            tone: BadgeVariant::Yellow,
            headline: format!("{account} is connected, and no groups are chosen."),
            next_step: "Nothing is imported until you choose which iCloud groups to bring in."
                .to_string(),
        },
        CardState::Healthy {
            account,
            last_sync_at,
        } => StateCopy {
            badge: "Connected",
            tone: BadgeVariant::Green,
            headline: format!("Importing contacts from {account}."),
            next_step: match last_sync_at {
                Some(_) => "Imports run on a schedule. Start one now if you cannot wait."
                    .to_string(),
                None => "The first import has not run yet. Start one now.".to_string(),
            },
        },
    }
}

/// Request bodies. Typed rather than `json!` literals, the MAPPS-685 rule.
#[derive(Serialize)]
struct NoBody {}

#[derive(Serialize)]
struct SettingWrite {
    category: &'static str,
    key: &'static str,
    value: bool,
}

/// `POST /integrations/contact-sync/icloud/connect`.
///
/// `app_password` is a `SECRET_FIELD_NAMES` name on the server (PMS-1409), so
/// its bytes reach the handler untouched: Apple prints the password in four
/// hyphen-separated groups and a rewritten one would be a refusal nobody could
/// place.
#[derive(Serialize)]
struct ConnectBody {
    apple_id: String,
    app_password: String,
}

#[derive(Deserialize)]
struct ConnectResponse {
    #[serde(default)]
    reconnected: bool,
}

#[component]
pub fn ICloudContactsSettingsPage() -> Element {
    use_page_title("iCloud Contacts");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "iCloud Contacts" } };
    }
    rsx! { ICloudContactsSettingsBody {} }
}

#[component]
fn ICloudContactsSettingsBody() -> Element {
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
    let mut apple_id = use_signal(String::new);
    let mut app_password = use_signal(String::new);
    let mut confirm_disconnect = use_signal(|| false);
    let mut confirm_off = use_signal(|| false);
    let navigator = use_navigator();
    let can_mutate = crate::hooks::use_can_mutate();

    let snap = overview.read_unchecked().clone();
    let data = snap.as_ref().and_then(|o| o.as_ref()).cloned();
    let state = data
        .as_ref()
        .map(|o| card_state_of(o.icloud_enabled, true, o.icloud_connection.as_ref()));
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
                let body = ConnectBody {
                    apple_id: apple_id().trim().to_string(),
                    app_password: app_password(),
                };
                match crate::hooks::fetch::api::post_authed::<ConnectResponse, _>(
                    "/integrations/contact-sync/icloud/connect",
                    &body,
                )
                .await
                {
                    Ok(answer) => {
                        // The password is never read back, so it is cleared here
                        // rather than left in a field that suggests it was kept.
                        app_password.set(String::new());
                        notice.set(if answer.reconnected {
                            "Reconnected. Imports resume on the next sync.".to_string()
                        } else {
                            "iCloud is connected. Nothing is imported until you choose groups."
                                .to_string()
                        });
                        overview.restart();
                    }
                    Err(e) => error.set(e.to_string()),
                }
            }
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
            title: "iCloud Contacts",
            subtitle: "Import contacts from an iCloud account into Mokosh over CardDAV. One way only: Mokosh never writes back to iCloud.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsICloudContacts {} }
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
                let connection: Option<Connection> = data.icloud_connection.clone();
                let connected_account = connection.as_ref().map(|c| c.account_email.clone());
                let disabled = busy() || !can_mutate;
                let enabled = data.icloud_enabled;
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            div {
                                class: "flex flex-wrap items-center gap-3",
                                "aria-live": "polite",
                                "data-testid": "icloud-sync-state",
                                span { "aria-hidden": "true", class: "text-muted",
                                    StateIcon { state: state.clone() }
                                }
                                Badge { variant: copy.tone, "{copy.badge}" }
                                p { class: "text-sm font-medium text-content", "{copy.headline}" }
                            }
                            p { class: "text-sm text-muted", "{copy.next_step}" }
                            if let CardState::Syncing { run, .. } = &state {
                                RunProgress { run: run.clone(), source: "iCloud".to_string() }
                            }
                            if let Some(connection) = connection.as_ref() {
                                dl { class: "grid gap-3 text-sm sm:grid-cols-2",
                                    div {
                                        dt { class: "text-muted", "Apple ID" }
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
                                            dt { class: "text-muted", "Deleted in iCloud" }
                                            dd { class: "text-content",
                                                "{connection.deleted_in_source}, kept in Mokosh"
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                if actions.choose_labels {
                                    Button {
                                        disabled,
                                        variant: if matches!(state, CardState::NoLabelsChosen { .. }) { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                                        onclick: move |_| { navigator.push(Route::SettingsICloudContactsImport {}); },
                                        data_testid: "icloud-sync-choose-groups",
                                        "Choose groups to import"
                                    }
                                }
                                if actions.sync_now {
                                    Button {
                                        disabled,
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| post("/integrations/contact-sync/runs?provider=icloud", "Could not start the import"),
                                        data_testid: "icloud-sync-now",
                                        "Import now"
                                    }
                                }
                                if actions.disconnect {
                                    Button {
                                        disabled,
                                        variant: ButtonVariant::Danger,
                                        onclick: move |_| confirm_disconnect.set(true),
                                        data_testid: "icloud-sync-disconnect",
                                        "Disconnect"
                                    }
                                }
                            }
                        }
                    }

                    // The connect form: shown while the integration is on and
                    // either nothing is connected or the password was rejected.
                    // Not hidden behind the Reconnect button, because what
                    // reconnecting NEEDS is a new password, and a button that
                    // reveals a field is one click more for no information.
                    if enabled && (actions.connect || actions.reconnect) {
                        Card { class: "mt-4",
                            h2 {
                                class: "text-base font-medium text-content focus:outline-none",
                                tabindex: "-1",
                                onmounted: move |e| async move {
                                    let _ = e.set_focus(true).await;
                                },
                                if actions.reconnect { "Enter a new app-specific password" } else { "Connect an iCloud account" }
                            }
                            p { class: "mt-1 text-sm text-muted",
                                "Apple has no consent screen for contacts, so this uses an app-specific password. Generate one at "
                                a {
                                    class: "underline",
                                    href: APPLE_ID_URL,
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "appleid.apple.com"
                                }
                                " under Sign-In and Security, App-Specific Passwords."
                            }
                            div { class: "mt-4 grid gap-4 sm:grid-cols-2",
                                Input {
                                    name: "apple_id",
                                    label: "Apple ID",
                                    r#type: "email",
                                    value: apple_id(),
                                    placeholder: "name@icloud.com",
                                    disabled,
                                    oninput: move |e: FormEvent| apple_id.set(e.value()),
                                    data_testid: "icloud-apple-id",
                                }
                                Input {
                                    name: "app_password",
                                    label: "App-specific password",
                                    r#type: "password",
                                    value: app_password(),
                                    placeholder: "xxxx-xxxx-xxxx-xxxx",
                                    help: "Not your Apple ID password. Mokosh stores it encrypted and never shows it again.",
                                    disabled,
                                    oninput: move |e: FormEvent| app_password.set(e.value()),
                                    data_testid: "icloud-app-password",
                                }
                            }
                            ul { class: "mt-4 list-disc space-y-1 pl-5 text-sm text-muted",
                                for point in CONSENT_POINTS {
                                    li { "{point}" }
                                }
                            }
                            div { class: "mt-4",
                                Button {
                                    disabled: disabled || apple_id().trim().is_empty() || app_password().is_empty(),
                                    onclick: move |_| connect(),
                                    data_testid: "icloud-sync-connect",
                                    if actions.reconnect { "Save password" } else { "Connect iCloud account" }
                                }
                            }
                        }
                    }

                    Card { class: "mt-4",
                        Checkbox {
                            name: "icloud_contacts_enabled",
                            label: "Allow iCloud Contacts for this organization",
                            checked: enabled,
                            disabled,
                            onchange: move |e: FormEvent| {
                                if e.value() == "true" {
                                    write_enabled(true);
                                } else {
                                    confirm_off.set(true);
                                }
                            },
                        }
                        p { class: "mt-2 text-sm text-muted",
                            "Off stops every iCloud import and hides the connect form. Google Contacts has its own switch and is not affected."
                        }
                    }

                    if confirm_disconnect() {
                        ConfirmDialog {
                            open: confirm_disconnect(),
                            title: "Disconnect iCloud Contacts?",
                            message: disconnect_message_from("iCloud", connected_account.as_deref()),
                            confirm_text: "Disconnect",
                            destructive: true,
                            loading: busy(),
                            onconfirm: move |_| {
                                confirm_disconnect.set(false);
                                post("/integrations/contact-sync/icloud/disconnect", "Could not disconnect");
                            },
                            oncancel: move |_| confirm_disconnect.set(false),
                        }
                    }
                    if confirm_off() {
                        ConfirmDialog {
                            open: confirm_off(),
                            title: "Turn off iCloud Contacts?",
                            message: "Nothing will sync from iCloud while it is off. The connection, every imported contact and the review queue are kept, and turning it back on resumes where it stopped.",
                            confirm_text: "Turn off",
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overview(json: serde_json::Value) -> Overview {
        serde_json::from_value(json).expect("overview")
    }

    fn connected(sync_status: &str, selected: serde_json::Value) -> Overview {
        overview(serde_json::json!({
            "enabled": true,
            "configured": true,
            "connection": null,
            "icloud_enabled": true,
            "icloud_connection": {
                "id": "2f1c2f1e-0000-4000-8000-0000000012cd",
                "account_email": "ops@icloud.example",
                "sync_status": sync_status,
                "selected_groups": selected,
                "consecutive_failures": 0,
            },
        }))
    }

    fn state_of(o: &Overview) -> CardState {
        card_state_of(o.icloud_enabled, true, o.icloud_connection.as_ref())
    }

    /// The two reads are independent: an iCloud card built from a response whose
    /// Google half is connected still reads its own connection, which is the
    /// whole point of PMS-1409.
    #[test]
    fn the_icloud_card_reads_the_icloud_connection() {
        let both = overview(serde_json::json!({
            "enabled": true,
            "configured": true,
            "connection": {
                "id": "2f1c2f1e-0000-4000-8000-00000000abcd",
                "account_email": "ops@msp.example",
                "sync_status": "success",
                "selected_groups": ["contactGroups/clients"],
                "consecutive_failures": 0,
            },
            "icloud_enabled": true,
            "icloud_connection": null,
        }));
        assert_eq!(state_of(&both), CardState::NeverConnected);
        assert_eq!(
            crate::pages::settings_contact_sync::card_state(&both),
            CardState::Healthy {
                account: "ops@msp.example".to_string(),
                last_sync_at: None
            },
            "the Google card is unaffected by the iCloud half"
        );
    }

    /// An absent `icloud_enabled` reads as enabled, the server's own default for
    /// the setting: a card that read it as off would hide the connect form on
    /// every deployment that has not touched the switch.
    #[test]
    fn an_absent_switch_reads_as_enabled() {
        let bare = overview(serde_json::json!({ "enabled": true, "configured": true }));
        assert!(bare.icloud_enabled);
        assert_eq!(state_of(&bare), CardState::NeverConnected);
    }

    /// The form is offered exactly when a password would be accepted: never
    /// connected, or connected with a password Apple has stopped accepting.
    #[test]
    fn the_password_form_is_offered_when_a_password_is_what_is_needed() {
        let never = overview(serde_json::json!({ "enabled": true, "configured": true }));
        let actions = actions_for(&state_of(&never));
        assert!(actions.connect && !actions.reconnect);

        let rejected = connected("reconnect_required", serde_json::json!(["G-1"]));
        let actions = actions_for(&state_of(&rejected));
        assert!(actions.reconnect, "a rejected password is re-entered here");
        assert!(actions.disconnect);

        let healthy = connected("success", serde_json::json!(["G-1"]));
        let actions = actions_for(&state_of(&healthy));
        assert!(
            !actions.connect && !actions.reconnect,
            "a working connection is not asked for a password"
        );

        let off = overview(serde_json::json!({
            "enabled": true, "configured": true, "icloud_enabled": false,
        }));
        let actions = actions_for(&state_of(&off));
        assert!(
            !actions.connect && !actions.reconnect,
            "a turned-off integration offers nothing"
        );
    }

    /// Every state's next step points at what fixes it, and a credential refusal
    /// points at Apple rather than at a reconnect that cannot work.
    #[test]
    fn a_rejected_password_is_sent_to_apple_and_nothing_names_google() {
        let rejected = connected("reconnect_required", serde_json::json!(["G-1"]));
        let copy = copy_for(&state_of(&rejected));
        assert_eq!(copy.badge, "Password rejected");
        assert!(copy.next_step.contains(APPLE_ID_URL), "{copy:?}");

        for status in ["success", "failed", "throttled", "reconnect_required"] {
            for selected in [serde_json::json!([]), serde_json::json!(["G-1"])] {
                let o = connected(status, selected);
                let copy = copy_for(&state_of(&o));
                assert!(
                    !copy.headline.is_empty() && !copy.next_step.is_empty(),
                    "{copy:?}"
                );
                assert!(
                    !copy.headline.contains("Google") && !copy.next_step.contains("Google"),
                    "an iCloud state must not name Google: {copy:?}"
                );
            }
        }
    }

    /// Nothing is imported until groups are chosen, and the card says so rather
    /// than reading as healthy.
    #[test]
    fn a_connection_with_no_groups_says_nothing_is_imported() {
        let none = connected("success", serde_json::json!([]));
        assert_eq!(
            state_of(&none),
            CardState::NoLabelsChosen {
                account: "ops@icloud.example".to_string()
            }
        );
        let copy = copy_for(&state_of(&none));
        assert!(copy.next_step.contains("Nothing is imported"), "{copy:?}");
    }

    /// The consent block says what an app-specific password actually is, which is
    /// the one thing that differs in kind from Google's read-only scope.
    #[test]
    fn the_consent_block_is_honest_about_what_the_password_can_do() {
        let all = CONSENT_POINTS.join(" ");
        assert!(all.contains("read and write your whole iCloud account"));
        assert!(all.contains("nothing writes back"));
        assert!(all.contains("Nothing is imported until"));
        assert!(all.contains("appleid.apple.com"));
    }

    /// The page is admin-only, and every request it makes names the provider, or
    /// it would act on the Google connection.
    #[test]
    fn the_page_is_admin_only_and_every_shared_call_names_the_provider() {
        let src = include_str!("settings_contact_sync_icloud.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("use_is_admin"), "admin gate");
        assert!(head.contains("AdminOnlyNotice"));
        // The one shared route this page calls. Without the provider it would
        // start an import on the tenant's GOOGLE connection while the button
        // says iCloud, which is the mistake the query parameter's default
        // (server PMS-1409) makes easy.
        const SHARED: &str = "/integrations/contact-sync/runs";
        let at = head.find(SHARED).expect("the shared route is called");
        assert!(
            head[at..].starts_with(&format!("{SHARED}?provider=icloud")),
            "{SHARED} must name the provider"
        );
    }
}

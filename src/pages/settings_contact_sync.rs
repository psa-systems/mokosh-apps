//! Settings > Integrations > Google Contacts (MAPPS-808, PSA-70 phase 6).
//!
//! The fourth Integrations surface beside the three RMM editors: whether the
//! integration is allowed for this organization, whether a Google account is
//! connected, how its last import went, and the three things an admin does
//! with it - connect (or reconnect), sync now, and disconnect.
//!
//! # One read, one decision
//!
//! `GET /integrations/contact-sync` (PMS-1241) answers `{ enabled, configured,
//! connection }`, and [`card_state`] turns that into exactly one [`CardState`].
//! Every state PSA-70 J names has a rendering and a next step, and the order
//! they are checked in is the order an admin needs to hear about them: an
//! import in flight first, then the one state only a human can fix (a revoked
//! grant), then waiting, then failing, then partial. Keeping the decision in a
//! pure function is what lets every state be pinned by a test without a
//! browser.
//!
//! # Admin only
//!
//! The page answers [`AdminOnlyNotice`] to anyone else, like every other
//! integration editor, so a non-admin cannot reach connect, sync or disconnect.
//! The server refuses all three for a non-admin regardless (`RequireAdmin`).
//!
//! # Motion and keyboard
//!
//! Every action is a real `button` from the shared [`Button`], so it is in the
//! tab order with the design system's focus ring. The one animated element,
//! the syncing icon, spins only under `motion-safe:`, and the progress bar only
//! transitions its width under `motion-safe:`, so `prefers-reduced-motion`
//! gets a still icon and a bar that jumps. The status region is
//! `aria-live="polite"`, so a screen reader hears an import finish without
//! the page stealing focus.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, BannerTone, Button, ButtonVariant, Card, Checkbox,
    ConfirmDialog, ErrorBanner, PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// The status read.
const STATUS_PATH: &str = "/integrations/contact-sync";

/// The tenant setting the organization switch writes (PMS-1241).
const SETTING_CATEGORY: &str = "integrations";
const SETTING_KEY: &str = "google_contacts_enabled";

/// The not-configured next step for the one person who can fix it.
const NOT_CONFIGURED_EDITABLE: &str = "Enter this deployment's Google sign-in client below. It only has to be done once, for every organisation on the deployment.";

/// Failed runs in a row before the card calls it failing repeatedly. The
/// server mails an admin at the same count (`runs::NOTIFY_AFTER`).
const FAILING_AFTER: i32 = 3;

/// How often the card re-reads while an import is queued or running.
const POLL_MS: u32 = 3_000;

fn enabled_by_default() -> bool {
    true
}

/// `GET /integrations/contact-sync` (PMS-1241).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Overview {
    /// Absent reads as enabled, the server's own default for the setting.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub connection: Option<Connection>,
    /// The caller may set the deployment's Google client (PMS-1264).
    #[serde(default)]
    pub client_editable: bool,
}

/// The connection half of the status read (PMS-1212, PMS-1215).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub account_email: String,
    pub sync_status: String,
    #[serde(default)]
    pub last_sync_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub selected_groups: Vec<String>,
    #[serde(default)]
    pub consecutive_failures: i32,
    #[serde(default)]
    pub open_reviews: i64,
    #[serde(default)]
    pub deleted_in_source: i64,
    #[serde(default)]
    pub latest_run: Option<Run>,
}

/// One import run (`contact_sync_runs`, PMS-1215).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Run {
    pub id: Uuid,
    pub status: String,
    #[serde(default)]
    pub total: Option<i32>,
    #[serde(default)]
    pub processed: i32,
    #[serde(default)]
    pub created: i32,
    #[serde(default)]
    pub linked: i32,
    #[serde(default)]
    pub updated: i32,
    #[serde(default)]
    pub queued_for_review: i32,
    #[serde(default)]
    pub failed_records: i32,
    /// What did not land, per record (capped by the server at 50).
    #[serde(default)]
    pub failures: Vec<RunFailure>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub not_before: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RunFailure {
    #[serde(default)]
    pub reason: String,
}

/// The reasons records did not land, each once with how many it stopped, most
/// frequent first. A reason is the server's own words for one record; the
/// record itself is a Google id nobody here would recognise, so it is not
/// shown.
pub fn failure_reasons(run: &Run) -> Vec<(String, usize)> {
    let mut counted: Vec<(String, usize)> = Vec::new();
    for failure in &run.failures {
        match counted
            .iter_mut()
            .find(|(reason, _)| *reason == failure.reason)
        {
            Some((_, n)) => *n += 1,
            None => counted.push((failure.reason.clone(), 1)),
        }
    }
    counted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counted
}

/// What the admin is agreeing to, shown before Google's consent screen
/// (PSA-70 K): what is read, that nothing is written back, where it goes and
/// who sees it.
pub const CONSENT_POINTS: &[&str] = &[
    "Read: names, email addresses, phone numbers, company, job title and department, and which labels each contact carries. Photos, addresses, birthdays and notes are not read.",
    "Nothing is written back. Mokosh asks Google for read-only access, so it cannot change or delete anything in Google Contacts.",
    "Where it goes: contacts you choose to import are stored in this organization's Mokosh CRM. Nothing is imported until you pick labels and start an import.",
    "Who sees it: everyone in your organization who can see contacts in Mokosh. Your clients' portal users do not.",
    "You can disconnect at any time. Imported contacts stay as local records, and a person's imported data can be removed on request.",
];

impl Run {
    fn active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running")
    }

    /// Records that landed, whatever became of each.
    fn landed(&self) -> i32 {
        self.created + self.linked + self.updated + self.queued_for_review
    }
}

/// Every state PSA-70 J names, plus the two a tenant can be in before any of
/// them apply.
#[derive(Clone, Debug, PartialEq)]
pub enum CardState {
    /// `integrations/google_contacts_enabled` is off.
    TurnedOff {
        connected_account: Option<String>,
    },
    /// This deployment has no Google OAuth client.
    NotConfigured,
    NeverConnected,
    /// A run is queued (possibly waiting out a rate limit) or running.
    Syncing {
        run: Run,
        waiting: bool,
    },
    ReconnectRequired {
        account: String,
    },
    Throttled {
        account: String,
    },
    FailingRepeatedly {
        account: String,
        failures: i32,
        error: Option<String>,
    },
    PartiallyImported {
        account: String,
        landed: i32,
        failed: i32,
    },
    Failed {
        account: String,
        error: Option<String>,
    },
    /// Connected, and nothing is imported because no label is chosen.
    NoLabelsChosen {
        account: String,
    },
    Healthy {
        account: String,
        last_sync_at: Option<DateTime<Utc>>,
    },
}

/// The one state the card renders for `overview`. Checked in the order an
/// admin needs to hear about them; see the module doc.
pub fn card_state(overview: &Overview) -> CardState {
    if !overview.enabled {
        return CardState::TurnedOff {
            connected_account: overview
                .connection
                .as_ref()
                .map(|c| c.account_email.clone()),
        };
    }
    let Some(connection) = overview.connection.as_ref() else {
        return if overview.configured {
            CardState::NeverConnected
        } else {
            CardState::NotConfigured
        };
    };
    let account = connection.account_email.clone();
    if let Some(run) = connection.latest_run.as_ref().filter(|r| r.active()) {
        // A queued run carrying `not_before` is one the provider asked to
        // wait; it is still an import in flight, not a failure.
        let waiting = run.status == "queued" && run.not_before.is_some();
        return CardState::Syncing {
            run: run.clone(),
            waiting,
        };
    }
    match connection.sync_status.as_str() {
        "reconnect_required" => return CardState::ReconnectRequired { account },
        "throttled" => return CardState::Throttled { account },
        _ => {}
    }
    if connection.consecutive_failures >= FAILING_AFTER {
        return CardState::FailingRepeatedly {
            account,
            failures: connection.consecutive_failures,
            error: connection.last_error.clone(),
        };
    }
    if let Some(run) = connection
        .latest_run
        .as_ref()
        .filter(|r| r.status == "failed" && r.failed_records > 0)
    {
        return CardState::PartiallyImported {
            account,
            landed: run.landed(),
            failed: run.failed_records,
        };
    }
    if connection.sync_status == "failed" {
        return CardState::Failed {
            account,
            error: connection.last_error.clone(),
        };
    }
    if connection.selected_groups.is_empty() {
        return CardState::NoLabelsChosen { account };
    }
    CardState::Healthy {
        account,
        last_sync_at: connection.last_sync_at,
    }
}

/// What the card says about a state: a badge, a headline, and what to do
/// next. Every state has a next step, even when it is "nothing".
#[derive(Clone, Debug, PartialEq)]
pub struct StateCopy {
    pub badge: &'static str,
    pub tone: BadgeVariant,
    pub headline: String,
    pub next_step: String,
}

pub fn copy_for(state: &CardState) -> StateCopy {
    match state {
        CardState::TurnedOff { connected_account } => StateCopy {
            badge: "Turned off",
            tone: BadgeVariant::Gray,
            headline: "Google Contacts is turned off for this organization.".to_string(),
            next_step: match connected_account {
                Some(account) => format!(
                    "Nothing syncs while it is off. The connection to {account} and every imported contact are kept; turn it back on below to resume."
                ),
                None => "Nobody can connect a Google account while it is off. Turn it on below to allow it.".to_string(),
            },
        },
        CardState::NotConfigured => StateCopy {
            badge: "Not available",
            tone: BadgeVariant::Gray,
            headline: "This deployment has no Google sign-in client configured.".to_string(),
            next_step: "Ask whoever runs this deployment to set up its Google sign-in client. Nothing here can be connected until then.".to_string(),
        },
        CardState::NeverConnected => StateCopy {
            badge: "Not connected",
            tone: BadgeVariant::Gray,
            headline: "Import contacts from your organization's Google account.".to_string(),
            next_step: "Connect an account to begin. Mokosh asks Google for read-only access, so it can never change or delete anything in Google Contacts.".to_string(),
        },
        CardState::Syncing { run, waiting } => StateCopy {
            badge: if *waiting { "Waiting" } else { "Syncing" },
            tone: BadgeVariant::Blue,
            headline: if *waiting {
                "Google asked this import to wait, and it resumes by itself.".to_string()
            } else if run.status == "queued" {
                "An import is queued and starts within a minute.".to_string()
            } else {
                "Importing contacts now.".to_string()
            },
            next_step: "You can leave this page; the import keeps running on the server.".to_string(),
        },
        CardState::ReconnectRequired { account } => StateCopy {
            badge: "Reconnect needed",
            tone: BadgeVariant::Red,
            headline: format!("Google no longer accepts this organization's access to {account}."),
            next_step: format!(
                "Reconnect and sign in as {account} again. Imported contacts, the label selection and the review queue are kept."
            ),
        },
        CardState::Throttled { account } => StateCopy {
            badge: "Rate limited",
            tone: BadgeVariant::Yellow,
            headline: format!("Google is limiting how fast {account} can be read."),
            next_step: "Nothing is wrong and nothing is needed: the next sync retries by itself.".to_string(),
        },
        CardState::FailingRepeatedly { failures, error, .. } => StateCopy {
            badge: "Failing",
            tone: BadgeVariant::Red,
            headline: format!("The last {failures} imports did not complete."),
            next_step: format!(
                "{} Contacts already imported are unchanged. Try Sync now; if it fails again, disconnect and connect the account again.",
                error.as_deref().map(|e| format!("The last one said: {e}")).unwrap_or_default()
            )
            .trim_start()
            .to_string(),
        },
        CardState::PartiallyImported { landed, failed, .. } => StateCopy {
            badge: "Partly imported",
            tone: BadgeVariant::Orange,
            headline: format!(
                "The last import brought in {landed} {} and could not import {failed}.",
                if *landed == 1 { "contact" } else { "contacts" }
            ),
            next_step: "What landed is kept. The next sync retries the rest; Sync now retries them straight away.".to_string(),
        },
        CardState::Failed { error, .. } => StateCopy {
            badge: "Last sync failed",
            tone: BadgeVariant::Yellow,
            headline: error
                .clone()
                .unwrap_or_else(|| "The last import did not complete.".to_string()),
            next_step: "Contacts already imported are unchanged. The next sync tries again, or use Sync now.".to_string(),
        },
        CardState::NoLabelsChosen { account } => StateCopy {
            badge: "Connected",
            tone: BadgeVariant::Green,
            headline: format!("Connected to {account}."),
            next_step: "No Google labels are chosen yet, so nothing is imported. Choose which labels hold your business contacts; you see what the import would do before anything is written.".to_string(),
        },
        CardState::Healthy { account, .. } => StateCopy {
            badge: "Connected",
            tone: BadgeVariant::Green,
            headline: format!("Connected to {account}."),
            next_step: "Imports run on their own schedule. Use Sync now to import changes straight away.".to_string(),
        },
    }
}

/// What the browser was sent back with after Google's consent screen.
fn return_banner(flag: Option<&str>) -> Option<(BannerTone, &'static str)> {
    match flag? {
        "connected" => Some((
            BannerTone::Success,
            "Google Contacts is connected. Nothing is imported until labels are chosen.",
        )),
        "reconnected" => Some((
            BannerTone::Success,
            "Reconnected. Imports resume on the next sync.",
        )),
        "failed" => Some((
            BannerTone::Error,
            "Google did not complete the connection, so nothing was stored. Try connecting again.",
        )),
        _ => None,
    }
}

/// Which actions a state offers. Kept beside [`card_state`] so a test can pin
/// that, for example, a turned-off organization is never offered Connect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    pub connect: bool,
    pub reconnect: bool,
    pub sync_now: bool,
    pub disconnect: bool,
    /// Open the label picker (MAPPS-809): the primary action when nothing is
    /// chosen, a secondary one once imports run.
    pub choose_labels: bool,
}

pub fn actions_for(state: &CardState) -> Actions {
    match state {
        CardState::TurnedOff { .. } | CardState::NotConfigured => Actions::default(),
        CardState::NeverConnected => Actions {
            connect: true,
            ..Actions::default()
        },
        CardState::Syncing { .. } => Actions {
            disconnect: true,
            ..Actions::default()
        },
        CardState::ReconnectRequired { .. } => Actions {
            reconnect: true,
            disconnect: true,
            ..Actions::default()
        },
        CardState::NoLabelsChosen { .. } => Actions {
            disconnect: true,
            choose_labels: true,
            ..Actions::default()
        },
        CardState::Throttled { .. }
        | CardState::FailingRepeatedly { .. }
        | CardState::PartiallyImported { .. }
        | CardState::Failed { .. }
        | CardState::Healthy { .. } => Actions {
            sync_now: true,
            disconnect: true,
            choose_labels: true,
            ..Actions::default()
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

#[derive(Deserialize)]
struct AuthorizeResponse {
    authorize_url: String,
}

#[component]
pub fn GoogleContactsSettingsPage() -> Element {
    use_page_title("Google Contacts");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Google Contacts" } };
    }
    rsx! { GoogleContactsSettingsBody {} }
}

#[component]
fn GoogleContactsSettingsBody() -> Element {
    let mut overview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Overview>(STATUS_PATH)
            .await
            .inspect_err(|e| tracing::error!("contact sync status load failed: {e}"))
            .ok()
    });
    let returned = use_signal(|| {
        crate::utils::url::current_query_param("contact_sync")
            .and_then(|flag| return_banner(Some(&flag)))
    });
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);
    let mut confirm_off = use_signal(|| false);
    let mut confirm_connect = use_signal(|| false);
    let navigator = use_navigator();
    let can_mutate = crate::hooks::use_can_mutate();

    let snap = overview.read_unchecked().clone();
    let state = snap.as_ref().and_then(|o| o.as_ref()).map(card_state);
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

    let mut post = move |path: String, after: &'static str| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    &path,
                    &NoBody {},
                )
                .await;
                match result {
                    Ok(_) => overview.restart(),
                    Err(e) => error.set(format!("{after}: {e}")),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (path, after);
            busy.set(false);
        });
    };

    let connect = move || {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = crate::hooks::fetch::api::post_authed::<AuthorizeResponse, _>(
                    "/integrations/contact-sync/google/authorize",
                    &NoBody {},
                )
                .await;
                match result {
                    Ok(resp) => {
                        // Google's consent screen, then back to this page with
                        // `?contact_sync=` set. Busy stays on: the page is
                        // about to be replaced.
                        #[cfg(target_arch = "wasm32")]
                        if let Err(e) = crate::platform::location::set_href(&resp.authorize_url) {
                            error.set(format!("Could not open Google's sign-in page: {e}"));
                            busy.set(false);
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            // A desktop host has no document to navigate; say
                            // where Google is rather than doing nothing.
                            error.set(format!(
                                "Open this address in a browser to connect: {}",
                                resp.authorize_url
                            ));
                            busy.set(false);
                        }
                        return;
                    }
                    Err(e) => error.set(format!("Could not start the connection: {e}")),
                }
            }
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
            title: "Google Contacts",
            subtitle: "Import contacts from your organization's Google account into Mokosh. One way only: Mokosh never writes back to Google.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsGoogleContacts {} }
            },
        }
        if let Some((tone, message)) = returned() {
            StatusBanner { tone, class: "mb-4", "{message}" }
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match (snap, state) {
            (None, _) => rsx! {
                crate::components::DetailSkeleton {}
            },
            (Some(None), _) | (Some(Some(_)), None) => rsx! {
                Card { ErrorBanner { "Could not load the Google Contacts connection." } }
            },
            (Some(Some(data)), Some(state)) => {
                let mut copy = copy_for(&state);
                // PMS-1264: the person who CAN set the client is told where.
                if state == CardState::NotConfigured && data.client_editable {
                    copy.next_step = NOT_CONFIGURED_EDITABLE.to_string();
                }
                let client_editable = data.client_editable;
                let actions = actions_for(&state);
                let disabled = busy() || !can_mutate;
                let connection = data.connection.clone();
                let connected_account = connection.as_ref().map(|c| c.account_email.clone());
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            div {
                                class: "flex flex-wrap items-center gap-3",
                                "aria-live": "polite",
                                "data-testid": "contact-sync-state",
                                span { "aria-hidden": "true", class: "text-muted",
                                    StateIcon { state: state.clone() }
                                }
                                Badge { variant: copy.tone, "{copy.badge}" }
                                p { class: "text-sm font-medium text-content", "{copy.headline}" }
                            }
                            p { class: "text-sm text-muted", "{copy.next_step}" }
                            if let CardState::Syncing { run, .. } = &state {
                                RunProgress { run: run.clone() }
                            }
                            if let Some(run) = connection
                                .as_ref()
                                .and_then(|c| c.latest_run.clone())
                                .filter(|r| !r.active() && r.failed_records > 0)
                            {
                                FailureList { run }
                            }
                            if let Some(connection) = connection.as_ref() {
                                ConnectionFacts { connection: connection.clone() }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                if actions.choose_labels && matches!(state, CardState::NoLabelsChosen { .. }) {
                                    Button {
                                        disabled,
                                        onclick: move |_| { navigator.push(Route::SettingsGoogleContactsImport {}); },
                                        data_testid: "contact-sync-choose-labels",
                                        "Choose labels to import"
                                    }
                                }
                                if actions.connect {
                                    Button {
                                        disabled,
                                        onclick: move |_| confirm_connect.set(true),
                                        data_testid: "contact-sync-connect",
                                        "Connect Google account"
                                    }
                                }
                                if actions.reconnect {
                                    Button {
                                        disabled,
                                        onclick: move |_| confirm_connect.set(true),
                                        data_testid: "contact-sync-reconnect",
                                        "Reconnect"
                                    }
                                }
                                if actions.sync_now {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        disabled,
                                        onclick: move |_| post("/integrations/contact-sync/runs".to_string(), "Could not start an import"),
                                        data_testid: "contact-sync-now",
                                        "Sync now"
                                    }
                                }
                                if actions.choose_labels && !matches!(state, CardState::NoLabelsChosen { .. }) {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        disabled,
                                        onclick: move |_| { navigator.push(Route::SettingsGoogleContactsImport {}); },
                                        data_testid: "contact-sync-change-labels",
                                        "Change labels"
                                    }
                                }
                                if connection.as_ref().is_some_and(|c| c.open_reviews > 0) {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| { navigator.push(Route::ContactImportReview {}); },
                                        data_testid: "contact-sync-review",
                                        "Review {connection.as_ref().map(|c| c.open_reviews).unwrap_or(0)} waiting"
                                    }
                                }
                                if actions.disconnect {
                                    Button {
                                        variant: ButtonVariant::Danger,
                                        disabled,
                                        onclick: move |_| confirm_disconnect.set(true),
                                        data_testid: "contact-sync-disconnect",
                                        "Disconnect"
                                    }
                                }
                            }
                        }
                    }
                    if client_editable {
                        crate::pages::settings_contact_sync_client::GoogleClientForm {
                            on_change: move |_| overview.restart(),
                        }
                    }
                    Card { class: "mt-6",
                        div { class: "space-y-2",
                            Checkbox {
                                name: "google_contacts_enabled",
                                label: "Allow Google Contacts for this organization",
                                checked: data.enabled,
                                disabled,
                                help: "Off stops every sync and hides Connect for everyone. The connection and imported contacts are kept.",
                                onchange: move |e: FormEvent| {
                                    if e.checked() {
                                        write_enabled(true);
                                    } else {
                                        confirm_off.set(true);
                                    }
                                },
                            }
                        }
                    }
                    ConfirmDialog {
                        open: confirm_connect(),
                        title: "Connect Google Contacts?",
                        message: "Before Google asks you to sign in, here is what connecting means for this organization.",
                        confirm_text: "Continue to Google",
                        loading: busy(),
                        body: rsx! {
                            ul { class: "list-disc space-y-2 pl-5 text-sm text-muted",
                                for point in CONSENT_POINTS {
                                    li { "{point}" }
                                }
                            }
                        },
                        onconfirm: move |_| {
                            confirm_connect.set(false);
                            let mut connect = connect;
                            connect();
                        },
                        oncancel: move |_| confirm_connect.set(false),
                    }
                    ConfirmDialog {
                        open: confirm_disconnect(),
                        title: "Disconnect Google Contacts?",
                        message: disconnect_message(connected_account.as_deref()),
                        confirm_text: "Disconnect",
                        destructive: true,
                        loading: busy(),
                        onconfirm: move |_| {
                            confirm_disconnect.set(false);
                            post("/integrations/contact-sync/google/disconnect".to_string(), "Could not disconnect");
                        },
                        oncancel: move |_| confirm_disconnect.set(false),
                    }
                    ConfirmDialog {
                        open: confirm_off(),
                        title: "Turn off Google Contacts?",
                        message: "Syncing stops for the whole organization and nobody can connect an account while it is off. The connection, imported contacts and the review queue are kept, and turning it back on resumes where it stopped.",
                        confirm_text: "Turn off",
                        loading: busy(),
                        onconfirm: move |_| {
                            confirm_off.set(false);
                            write_enabled(false);
                        },
                        oncancel: move |_| {
                            confirm_off.set(false);
                            // The checkbox already unticked itself; re-read so
                            // it shows what the server still says.
                            overview.restart();
                        },
                    }
                }
            }
        }
    }
}

/// What a disconnect does, stated before it happens (PSA-70 J).
pub fn disconnect_message(account: Option<&str>) -> String {
    let from = account.map(|a| format!(" from {a}")).unwrap_or_default();
    format!(
        "Syncing stops and the stored Google access is removed. Every contact imported{from} stays in Mokosh as a local record that still shows where it came from. Nothing is deleted from Google or from Mokosh, and an import in progress stops."
    )
}

#[component]
fn StateIcon(state: CardState) -> Element {
    use crate::components::{CheckIcon, ExclamationIcon, InformationIcon};
    match state {
        CardState::Syncing { .. } => rsx! { SyncIcon {} },
        CardState::Healthy { .. } | CardState::NoLabelsChosen { .. } => rsx! { CheckIcon {} },
        CardState::ReconnectRequired { .. }
        | CardState::FailingRepeatedly { .. }
        | CardState::Failed { .. }
        | CardState::PartiallyImported { .. }
        | CardState::Throttled { .. } => rsx! { ExclamationIcon {} },
        CardState::TurnedOff { .. } | CardState::NotConfigured | CardState::NeverConnected => {
            rsx! { InformationIcon {} }
        }
    }
}

/// Heroicons `arrow-path` (MIT), inline. Spins only when motion is allowed.
#[component]
fn SyncIcon() -> Element {
    rsx! {
        svg {
            class: "w-5 h-5 motion-safe:animate-spin",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            stroke_width: "1.5",
            stroke: "currentColor",
            path {
                stroke_linecap: "round",
                stroke_linejoin: "round",
                d: "M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99",
            }
        }
    }
}

/// Percent done, when the run knows its total yet.
pub fn percent(run: &Run) -> Option<i32> {
    let total = run.total.filter(|t| *t > 0)?;
    Some(((run.processed.max(0) * 100) / total).clamp(0, 100))
}

#[component]
fn RunProgress(run: Run) -> Element {
    let label = match (percent(&run), run.total) {
        (Some(p), Some(total)) => format!("{} of {total} contacts read ({p}%)", run.processed),
        _ => "Reading contacts from Google…".to_string(),
    };
    let width = percent(&run).unwrap_or(0);
    rsx! {
        div { class: "space-y-1",
            div {
                class: "h-2 w-full overflow-hidden rounded-full bg-surface-2",
                role: "progressbar",
                "aria-label": "Import progress",
                "aria-valuemin": "0",
                "aria-valuemax": "100",
                "aria-valuenow": percent(&run).map(|p| p.to_string()),
                "aria-valuetext": "{label}",
                div {
                    class: "h-full rounded-full bg-accent motion-safe:transition-[width] motion-safe:duration-500",
                    style: "width: {width}%",
                }
            }
            p { class: "text-xs text-muted", "{label}" }
        }
    }
}

/// What did not land in the last import (MAPPS-809): each reason once, with
/// how many records it stopped. Beside what DID land, which the facts below
/// already count.
#[component]
fn FailureList(run: Run) -> Element {
    let reasons = failure_reasons(&run);
    let unlisted = usize::try_from(run.failed_records)
        .unwrap_or(0)
        .saturating_sub(run.failures.len());
    rsx! {
        div { class: "rounded-md border border-line p-3",
            p { class: "text-sm font-medium text-content",
                "{run.failed_records} could not be imported"
            }
            ul { class: "mt-2 space-y-1 text-sm text-muted",
                for (reason, count) in reasons {
                    li { "{count} × {reason}" }
                }
                if unlisted > 0 {
                    li { "{unlisted} more not listed" }
                }
            }
        }
    }
}

#[component]
fn ConnectionFacts(connection: Connection) -> Element {
    let last_sync = connection
        .last_sync_at
        .map(crate::utils::datetime::fmt_datetime_pref)
        .unwrap_or_else(|| "Never".to_string());
    let last_run = connection
        .latest_run
        .as_ref()
        .filter(|r| !r.active())
        .map(|r| {
            format!(
                "{} created, {} linked, {} updated, {} waiting for review{}",
                r.created,
                r.linked,
                r.updated,
                r.queued_for_review,
                if r.failed_records > 0 {
                    format!(", {} could not be imported", r.failed_records)
                } else {
                    String::new()
                }
            )
        });
    let labels = match connection.selected_groups.len() {
        0 => "None chosen".to_string(),
        1 => "1 label".to_string(),
        n => format!("{n} labels"),
    };
    rsx! {
        dl { class: "grid grid-cols-1 gap-x-6 gap-y-2 text-sm sm:grid-cols-2",
            div {
                dt { class: "text-muted", "Account" }
                dd { class: "text-content", "{connection.account_email}" }
            }
            div {
                dt { class: "text-muted", "Last sync" }
                dd { class: "text-content", "{last_sync}" }
            }
            div {
                dt { class: "text-muted", "Labels imported" }
                dd { class: "text-content", "{labels}" }
            }
            if let Some(last_run) = last_run {
                div {
                    dt { class: "text-muted", "Last import" }
                    dd { class: "text-content", "{last_run}" }
                }
            }
            if connection.open_reviews > 0 {
                div {
                    dt { class: "text-muted", "Waiting for review" }
                    dd { class: "text-content", "{connection.open_reviews}" }
                }
            }
            if connection.deleted_in_source > 0 {
                div {
                    dt { class: "text-muted", "Deleted in Google" }
                    dd { class: "text-content",
                        "{connection.deleted_in_source}, kept in Mokosh"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(json: serde_json::Value) -> Overview {
        let mut base = serde_json::json!({
            "id": "2f1c2f1e-0000-4000-8000-00000000abcd",
            "account_email": "ops@msp.example",
            "sync_status": "success",
            "selected_groups": ["contactGroups/clients"],
            "consecutive_failures": 0,
        });
        for (k, v) in json.as_object().unwrap() {
            base[k] = v.clone();
        }
        serde_json::from_value(serde_json::json!({
            "enabled": true, "configured": true, "connection": base,
        }))
        .expect("overview")
    }

    fn run(json: serde_json::Value) -> serde_json::Value {
        let mut base = serde_json::json!({
            "id": "2f1c2f1e-0000-4000-8000-00000000beef",
            "status": "completed", "processed": 0,
        });
        for (k, v) in json.as_object().unwrap() {
            base[k] = v.clone();
        }
        base
    }

    /// Every state in PSA-70 J is reachable from a real server answer.
    #[test]
    fn every_lifecycle_state_is_reachable() {
        let never: Overview =
            serde_json::from_str(r#"{"enabled":true,"configured":true,"connection":null}"#)
                .unwrap();
        assert_eq!(card_state(&never), CardState::NeverConnected);

        let unconfigured: Overview =
            serde_json::from_str(r#"{"enabled":true,"configured":false,"connection":null}"#)
                .unwrap();
        assert_eq!(card_state(&unconfigured), CardState::NotConfigured);

        assert!(matches!(
            card_state(&connection(serde_json::json!({}))),
            CardState::Healthy { .. }
        ));
        assert!(matches!(
            card_state(&connection(
                serde_json::json!({ "latest_run": run(serde_json::json!({"status": "running", "total": 10, "processed": 4})) })
            )),
            CardState::Syncing { waiting: false, .. }
        ));
        assert!(matches!(
            card_state(&connection(
                serde_json::json!({ "sync_status": "reconnect_required" })
            )),
            CardState::ReconnectRequired { .. }
        ));
        assert!(matches!(
            card_state(&connection(
                serde_json::json!({ "sync_status": "throttled" })
            )),
            CardState::Throttled { .. }
        ));
        assert!(matches!(
            card_state(&connection(
                serde_json::json!({ "sync_status": "failed", "consecutive_failures": 3 })
            )),
            CardState::FailingRepeatedly { failures: 3, .. }
        ));
        assert!(matches!(
            card_state(&connection(serde_json::json!({
                "sync_status": "failed", "consecutive_failures": 1,
                "latest_run": run(serde_json::json!({"status": "failed", "created": 7, "failed_records": 2})),
            }))),
            CardState::PartiallyImported {
                landed: 7,
                failed: 2,
                ..
            }
        ));
        assert!(matches!(
            card_state(&connection(
                serde_json::json!({ "sync_status": "failed", "consecutive_failures": 1 })
            )),
            CardState::Failed { .. }
        ));
        assert!(matches!(
            card_state(&connection(serde_json::json!({ "selected_groups": [] }))),
            CardState::NoLabelsChosen { .. }
        ));
    }

    /// Off wins over everything, including an import in flight, and keeps
    /// naming the account so the admin knows the connection is still there.
    #[test]
    fn turned_off_wins_and_offers_nothing() {
        let mut overview = connection(serde_json::json!({
            "latest_run": run(serde_json::json!({"status": "running"})),
        }));
        overview.enabled = false;
        let state = card_state(&overview);
        assert_eq!(
            state,
            CardState::TurnedOff {
                connected_account: Some("ops@msp.example".into())
            }
        );
        assert_eq!(
            actions_for(&state),
            Actions::default(),
            "no connect, sync or disconnect"
        );
        assert!(copy_for(&state).next_step.contains("ops@msp.example"));
    }

    /// An import in flight is reported before a stale failure, and a queued run
    /// waiting out a rate limit reads as waiting, not failing.
    #[test]
    fn an_import_in_flight_comes_first() {
        let state = card_state(&connection(serde_json::json!({
            "sync_status": "failed", "consecutive_failures": 5,
            "latest_run": run(serde_json::json!({"status": "queued", "not_before": "2026-09-16T12:00:00Z"})),
        })));
        assert!(
            matches!(state, CardState::Syncing { waiting: true, .. }),
            "{state:?}"
        );
        assert_eq!(copy_for(&state).badge, "Waiting");
    }

    /// A revoked grant outranks a failure streak: it is the state only a human
    /// can fix, and Reconnect is the way out.
    #[test]
    fn reconnect_outranks_a_failure_streak_and_offers_reconnect() {
        let state = card_state(&connection(serde_json::json!({
            "sync_status": "reconnect_required", "consecutive_failures": 9,
        })));
        assert!(matches!(state, CardState::ReconnectRequired { .. }));
        let actions = actions_for(&state);
        assert!(actions.reconnect && actions.disconnect && !actions.connect && !actions.sync_now);
    }

    /// The label picker is offered once there is a connection to import from,
    /// and never while an import runs or the integration is off.
    #[test]
    fn choose_labels_is_offered_only_to_a_connected_idle_tenant() {
        for (state, offered) in [
            (
                CardState::NoLabelsChosen {
                    account: "a@b".into(),
                },
                true,
            ),
            (
                CardState::Healthy {
                    account: "a@b".into(),
                    last_sync_at: None,
                },
                true,
            ),
            (CardState::NeverConnected, false),
            (
                CardState::TurnedOff {
                    connected_account: Some("a@b".into()),
                },
                false,
            ),
            (
                CardState::ReconnectRequired {
                    account: "a@b".into(),
                },
                false,
            ),
        ] {
            assert_eq!(actions_for(&state).choose_labels, offered, "{state:?}");
        }
    }

    /// Consent names what is read, that nothing is written back, where it
    /// goes and who sees it (PSA-70 K), and it is shown before Google's
    /// screen, not after.
    #[test]
    fn consent_covers_what_where_who_and_one_way() {
        let all = CONSENT_POINTS.join(" ");
        for needle in [
            "Read:",
            "Nothing is written back",
            "Where it goes",
            "Who sees it",
        ] {
            assert!(all.contains(needle), "{needle}");
        }
        let src = include_str!("settings_contact_sync.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(
            !head.contains("onclick: connect"),
            "Connect must open the consent dialog, not go straight to Google"
        );
        assert!(head.contains("onclick: move |_| confirm_connect.set(true)"));
    }

    #[test]
    fn failure_reasons_are_counted_most_frequent_first() {
        let run: Run = serde_json::from_value(serde_json::json!({
            "id": "2f1c2f1e-0000-4000-8000-00000000beef", "status": "failed",
            "failed_records": 3,
            "failures": [
                {"external_id": "people/c1", "reason": "b"},
                {"external_id": "people/c2", "reason": "a"},
                {"external_id": "people/c3", "reason": "a"}
            ]
        }))
        .unwrap();
        assert_eq!(
            failure_reasons(&run),
            vec![("a".to_string(), 2), ("b".to_string(), 1)]
        );
    }

    /// Connect is offered only where connecting can work.
    #[test]
    fn connect_is_offered_only_to_a_never_connected_configured_tenant() {
        for (state, offered) in [
            (CardState::NeverConnected, true),
            (CardState::NotConfigured, false),
            (
                CardState::TurnedOff {
                    connected_account: None,
                },
                false,
            ),
            (
                CardState::Healthy {
                    account: "a@b".into(),
                    last_sync_at: None,
                },
                false,
            ),
        ] {
            assert_eq!(actions_for(&state).connect, offered, "{state:?}");
        }
    }

    /// Every state says what to do next.
    #[test]
    fn every_state_has_a_next_step() {
        let states = [
            CardState::TurnedOff {
                connected_account: None,
            },
            CardState::NotConfigured,
            CardState::NeverConnected,
            CardState::ReconnectRequired {
                account: "a@b".into(),
            },
            CardState::Throttled {
                account: "a@b".into(),
            },
            CardState::FailingRepeatedly {
                account: "a@b".into(),
                failures: 3,
                error: None,
            },
            CardState::PartiallyImported {
                account: "a@b".into(),
                landed: 1,
                failed: 1,
            },
            CardState::Failed {
                account: "a@b".into(),
                error: None,
            },
            CardState::NoLabelsChosen {
                account: "a@b".into(),
            },
            CardState::Healthy {
                account: "a@b".into(),
                last_sync_at: None,
            },
        ];
        for state in states {
            let copy = copy_for(&state);
            assert!(
                !copy.headline.is_empty() && !copy.next_step.is_empty(),
                "{state:?}"
            );
            assert!(
                !copy.next_step.starts_with(' '),
                "{state:?}: {:?}",
                copy.next_step
            );
        }
    }

    /// The disconnect confirmation says the contacts are kept before anyone
    /// presses it (PSA-70 J).
    #[test]
    fn the_disconnect_confirmation_says_contacts_are_kept() {
        let message = disconnect_message(Some("ops@msp.example"));
        assert!(
            message.contains("stays in Mokosh as a local record"),
            "{message}"
        );
        assert!(message.contains("Nothing is deleted"), "{message}");
        assert!(message.contains("ops@msp.example"), "{message}");
    }

    #[test]
    fn the_return_flag_from_google_becomes_a_banner() {
        assert_eq!(
            return_banner(Some("connected")).map(|b| b.0),
            Some(BannerTone::Success)
        );
        assert_eq!(
            return_banner(Some("failed")).map(|b| b.0),
            Some(BannerTone::Error)
        );
        assert_eq!(return_banner(Some("anything-else")), None);
        assert_eq!(return_banner(None), None);
    }

    #[test]
    fn progress_is_a_bounded_percent() {
        let r: Run =
            serde_json::from_value(run(serde_json::json!({"total": 200, "processed": 50})))
                .unwrap();
        assert_eq!(percent(&r), Some(25));
        let unknown: Run =
            serde_json::from_value(run(serde_json::json!({"processed": 50}))).unwrap();
        assert_eq!(
            percent(&unknown),
            None,
            "no total yet is no percent, not zero"
        );
    }

    /// The page is admin only, and the one animation respects reduced motion.
    #[test]
    fn the_page_is_admin_only_and_motion_is_opt_in() {
        let src = include_str!("settings_contact_sync.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(
            !head.contains("class: \"animate-spin"),
            "an unconditional spin ignores prefers-reduced-motion"
        );
        assert!(head.contains("motion-safe:animate-spin"));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
    }
}

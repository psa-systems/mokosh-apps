//! Settings > Google sign-in client (MAPPS-980).
//!
//! The deployment's Google OAuth client, server `GET`/`PUT
//! /settings/google-contacts-client` (PMS-1444). Deployment-wide like
//! [`super::settings_email`] and app name: one Google application per
//! installation, effective for every tenant, so it gets its own page rather
//! than joining Organization.
//!
//! ## Why this is not the form MAPPS-977 deleted
//!
//! PMS-1264 put a Google client form on the Contact sync CARD and PMS-1340 gave
//! every tenant its own, which put a Google Cloud console walkthrough in front
//! of every customer for a credential that was never theirs. MAPPS-977 removed
//! it. This is the opposite surface: the deployment's operator, on a page a
//! tenant admin cannot reach, for a credential that genuinely is theirs to set.
//! The console steps live here for the same reason, and that is where they
//! always belonged.
//!
//! ## Neither half is ever read back
//!
//! The server returns whether each half is set and which provider serves them,
//! never a value, not even the id (PMS-1444 is deliberate about the id: an
//! endpoint that returns it grows a page that displays it). So the fields start
//! blank even on a configured deployment, and saving always sends a complete
//! pair. That is not a limitation to work around: a client id and its secret
//! have to come from the same Google project, so "change one" is not a thing an
//! operator should be offered.
//!
//! ## The redirect URI
//!
//! Derived from this deployment's own API base rather than described, because it
//! is the single most error-prone field in the whole setup: one character out
//! and Google refuses with `redirect_uri_mismatch` before Mokosh is involved.
//! A page can print the exact string to paste. A document cannot.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorBanner, Input,
    PageHeader,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

const CLIENT_PATH: &str = "/settings/google-contacts-client";

/// The path the OAuth callback is served at, relative to the API base.
const CALLBACK_PATH: &str = "/public/contact-sync/google/callback";

/// The console walkthrough, carried over from the card MAPPS-977 emptied.
///
/// Two of these six are the ones people get wrong: the scope list, because a
/// consent screen that grants less than the sync asks for fails later and
/// vaguely, and the redirect URI, which is why this page prints it rather than
/// describing it.
const CONSOLE_STEPS: &[&str] = &[
    "In the Google Cloud console, create a project or pick one.",
    "APIs and Services, Library: enable the Google People API. Without it every import fails with SERVICE_DISABLED.",
    "Google Auth Platform (older consoles call it the OAuth consent screen): fill in Branding, then set Audience to External unless this account is in a Google Workspace organization. While the app is in Testing, add the accounts that may connect as test users.",
    "Data access: add exactly three scopes - .../auth/contacts.readonly, openid and email. Read-only is the whole bargain: Mokosh can never change anything in Google Contacts.",
    "Clients, Create client, application type Web application. Paste the redirect URI above under Authorized redirect URIs, exactly as it appears. No JavaScript origins are needed, because the exchange happens on the server.",
    "Copy the client ID and client secret it shows you into the fields below.",
];

/// What publishing costs, which belongs to the procedure and not to the code.
const TESTING_MODE_WARNING: &str = "While the app is in Testing, Google expires every refresh token after seven days, so connections stop working a week later with no warning. Publishing without Google's verification caps the app at 100 users and shows each of them an unverified-app screen.";

/// `GET`/`PUT /settings/google-contacts-client`, for the server's
/// `GoogleClientView`. Names and booleans only: there is no field here that
/// could carry a credential, and that is the point.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct GoogleClientView {
    #[serde(default)]
    client_id_set: bool,
    #[serde(default)]
    client_secret_set: bool,
    #[serde(default)]
    configured: bool,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    writable: bool,
    #[serde(default)]
    restart_required: bool,
}

/// The write body. Both halves, always.
#[derive(Debug, Serialize)]
struct GoogleClientBody {
    client_id: String,
    client_secret: String,
}

/// The exact redirect URI to paste into the Google console.
///
/// `api_base()` already ends in `/api/v1`, so the callback is joined to it
/// rather than rebuilt from the origin: deriving it from `window.location`
/// would be wrong on every deployment whose API is on a different host from the
/// SPA, which is every hosted one.
fn callback_uri(api_base: &str) -> String {
    format!("{}{CALLBACK_PATH}", api_base.trim_end_matches('/'))
}

fn redirect_uri() -> String {
    #[cfg(feature = "app")]
    {
        callback_uri(&crate::hooks::fetch::api::api_base())
    }
    #[cfg(not(feature = "app"))]
    {
        callback_uri("")
    }
}

#[component]
pub fn GoogleClientPage() -> Element {
    use_page_title("Google sign-in client");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Google sign-in client" } };
    }
    rsx! { GoogleClientForm {} }
}

#[component]
fn GoogleClientForm() -> Element {
    let mut client_id = use_signal(String::new);
    let mut client_secret = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut state = use_signal(GoogleClientView::default);
    let mut seeded = use_signal(|| false);

    let can_mutate = crate::hooks::use_can_mutate();

    let settings_resource = use_resource(move || async move {
        let _reachable = crate::hooks::use_server_reachable();
        crate::pages::settings::load_operator_setting::<GoogleClientView>(
            CLIENT_PATH,
            "Google client settings",
        )
        .await
    });
    let snap = settings_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(crate::pages::settings::OperatorLoad::Failed));
    let not_operator = matches!(
        *snap,
        Some(crate::pages::settings::OperatorLoad::NotOperator)
    );
    if !seeded() {
        if let Some(crate::pages::settings::OperatorLoad::Loaded(view)) = &*snap {
            state.set(view.clone());
            seeded.set(true);
        }
    }

    let view = state();
    let uri = redirect_uri();
    // Both halves or nothing: the server refuses a partial write, so the button
    // refuses to make one.
    let complete = !client_id.read().trim().is_empty() && !client_secret.read().trim().is_empty();

    let handle_save = move |_| {
        if saving() {
            return;
        }
        error.set(String::new());
        let body = GoogleClientBody {
            client_id: client_id.read().trim().to_string(),
            client_secret: client_secret.read().trim().to_string(),
        };
        if body.client_id.is_empty() || body.client_secret.is_empty() {
            error.set(
                "Enter both the client ID and the client secret. A deployment holding one half of \
                 the pair cannot start."
                    .to_string(),
            );
            return;
        }
        saving.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<GoogleClientView, _>(
                    CLIENT_PATH,
                    &body,
                )
                .await
                {
                    Ok(saved) => {
                        // The server says whether its write is live, and this
                        // line is the only thing an operator has to go on, so it
                        // reads the field rather than restating what PMS-1444
                        // does today. A page that promised immediacy
                        // unconditionally would start lying the moment the
                        // server's answer changed.
                        let message = if saved.restart_required {
                            "Google sign-in client saved. Restart the deployment to start using it."
                        } else {
                            "Google sign-in client saved. It is in use now, with no restart."
                        };
                        state.set(saved);
                        // Cleared on success, not kept: the server will not
                        // return them and a filled field after a save suggests
                        // the page is showing what is stored.
                        client_id.set(String::new());
                        client_secret.set(String::new());
                        crate::hooks::push_toast(crate::components::AlertType::Success, message);
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        PageHeader {
            title: "Google sign-in client",
            subtitle: "The Google application this deployment authenticates as when an organization connects Google Contacts. One client for every organization here; none of them can see or set it.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsGoogleClient {} }
            },
        }

        div { class: "space-y-6",
            if fetch_failed {
                Card {
                    ErrorBanner { "Could not load the Google sign-in client settings." }
                }
            }
            if not_operator {
                Card {
                    p { class: "text-sm text-muted",
                        "The Google sign-in client for this deployment is set by whoever runs it. Every organisation here connects through that one client, so only its operator can see or change this."
                    }
                }
            } else {

            Card {
                div { class: "space-y-3 max-w-2xl",
                    div { class: "flex items-center gap-3",
                        if view.configured {
                            Badge { variant: BadgeVariant::Green, "Configured" }
                        } else {
                            Badge { variant: BadgeVariant::Gray, "Not configured" }
                        }
                        p { class: "text-sm text-content",
                            if view.configured {
                                "Organizations on this deployment can connect a Google account."
                            } else {
                                "Google Contacts is unavailable to every organization here until this is set."
                            }
                        }
                    }
                    // Named per half, because a deployment holding one refuses
                    // to start and "not configured" would not say which.
                    if view.client_id_set != view.client_secret_set {
                        ErrorBanner {
                            "Only one half of the pair is stored, which stops this deployment from starting. Enter both below."
                        }
                    }
                    if view.configured && view.restart_required {
                        p { class: "text-sm text-muted",
                            "A change here needs the deployment restarted before it is used."
                        }
                    }
                    if !view.provider.is_empty() {
                        p { class: "text-sm text-muted",
                            "Stored in the {view.provider} secret provider."
                            if !view.writable {
                                " It cannot be written from here on this deployment, so set it with "
                                code { class: "text-xs", "mokosh-server provider-set" }
                                " instead."
                            }
                        }
                    }
                }
            }

            Card {
                title: if view.configured {
                    "Replace the client".to_string()
                } else {
                    "Set the client".to_string()
                },
                div { class: "space-y-4 max-w-2xl",
                    if !error().is_empty() {
                        ErrorBanner { "{error()}" }
                    }
                    p { class: "text-sm text-muted",
                        "Neither half is ever shown again once saved, so both fields start blank and both are required. A client ID and its secret have to come from the same Google project."
                    }
                    if view.configured {
                        p { class: "text-sm text-muted",
                            "Replacing the client ID disconnects every organization that has connected: Google ties a refresh token to the application that issued it, so each one is asked to connect again. Replacing only the secret disconnects nobody."
                        }
                    }
                    Input {
                        name: "google_client_id",
                        label: "Client ID",
                        value: client_id(),
                        disabled: is_loading || saving() || !view.writable,
                        placeholder: "123456789-abc.apps.googleusercontent.com",
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            client_id.set(e.value());
                        },
                    }
                    Input {
                        name: "google_client_secret",
                        label: "Client secret",
                        r#type: "password".to_string(),
                        value: client_secret(),
                        disabled: is_loading || saving() || !view.writable,
                        placeholder: "GOCSPX-...",
                        help: "Write-only: never shown once saved.".to_string(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            client_secret.set(e.value());
                        },
                    }
                    div { class: "flex justify-end",
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: saving(),
                            disabled: !can_mutate || is_loading || !view.writable || !complete,
                            title: if !complete {
                                Some("Enter both the client ID and the client secret".to_string())
                            } else if !can_mutate {
                                Some("Can't save while the server is unreachable".to_string())
                            } else {
                                None
                            },
                            onclick: handle_save,
                            if view.configured { "Replace Client" } else { "Save Client" }
                        }
                    }
                }
            }

            Card {
                title: "Creating the client in Google".to_string(),
                div { class: "space-y-4 max-w-2xl",
                    div { class: "space-y-1",
                        p { class: "text-sm font-medium text-content", "Authorized redirect URI" }
                        p { class: "text-sm text-muted",
                            "Paste this into the Google console exactly as it appears. One character out and Google refuses the sign-in before Mokosh is involved."
                        }
                        code {
                            class: "block text-xs break-all rounded bg-surface-2 p-2 text-content",
                            "data-testid": "google-client-redirect-uri",
                            "{uri}"
                        }
                    }
                    ol { class: "list-decimal space-y-2 pl-5 text-sm text-muted",
                        for step in CONSOLE_STEPS.iter() {
                            li { "{step}" }
                        }
                    }
                    p { class: "text-sm text-muted", "{TESTING_MODE_WARNING}" }
                }
            }

            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The redirect URI is joined to the API base, which is the only place it
    /// can come from.
    ///
    /// Not from `window.location`: on every hosted deployment the SPA and the
    /// API are different hosts (`msp.a8n.systems` and `api.msp.a8n.systems`),
    /// so an origin-derived URI would be wrong exactly where it matters and
    /// right in dev, which is the worst way for this to be wrong.
    #[test]
    fn the_redirect_uri_is_the_api_base_plus_the_callback() {
        assert_eq!(
            callback_uri("https://api.msp.a8n.systems/api/v1"),
            "https://api.msp.a8n.systems/api/v1/public/contact-sync/google/callback"
        );
        // A trailing slash on the base must not double the separator: Google
        // compares the string, so `//public` is a different URI.
        assert_eq!(
            callback_uri("https://api.msp.psa.systems/api/v1/"),
            "https://api.msp.psa.systems/api/v1/public/contact-sync/google/callback"
        );
        // Dev's same-origin base.
        assert_eq!(
            callback_uri("/api/v1"),
            "/api/v1/public/contact-sync/google/callback"
        );
    }

    /// The console steps name the API and every scope the server asks for.
    ///
    /// Carried over from the page MAPPS-977 deleted, because the pairing it
    /// guards is unchanged: `contact_sync::oauth::SCOPES` on the server is the
    /// other half, and a scope missing from this list is a consent screen that
    /// grants less than the sync needs. The seven-day rule is on the page for
    /// the same reason it was before: it is the one that fails silently a week
    /// later.
    #[test]
    fn the_steps_name_the_api_and_every_scope_the_server_asks_for() {
        let joined = CONSOLE_STEPS.join(" ");
        for required in [
            "People API",
            "contacts.readonly",
            "openid",
            "email",
            "Web application",
        ] {
            assert!(
                joined.contains(required),
                "the steps never mention {required}"
            );
        }
        assert!(
            TESTING_MODE_WARNING.contains("seven days"),
            "the rule that fails silently a week later has to be on the page"
        );
    }

    /// The secret is a password input, neither half is ever seeded from a read,
    /// and the view type has no field that could hold one.
    ///
    /// The second clause is the one worth scanning for. The server does not
    /// return either value, so a `set` from a response is a page displaying
    /// something it invented, and the id is as strict as the secret here on
    /// purpose: PMS-1444 withholds it precisely so no page can show it.
    #[test]
    fn neither_half_is_ever_filled_from_a_response() {
        let src = include_str!("settings_google_client.rs");
        let head = &src[..src.find("mod tests").expect("this module")];

        assert!(head.contains("r#type: \"password\""));
        for banned in [
            "client_id.set(saved",
            "client_secret.set(saved",
            "client_id.set(view",
            "client_secret.set(view",
        ] {
            assert!(
                !head.contains(banned),
                "{banned} fills a field from a read; the server returns neither half"
            );
        }
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );

        // The view carries booleans and a provider name. A field typed
        // `String` other than `provider` would be a value coming back.
        let view = &head[head.find("struct GoogleClientView").expect("the view")..];
        let view = &view[..view.find('}').expect("its closing brace")];
        let strings = view.matches("String").count();
        assert_eq!(
            strings, 1,
            "GoogleClientView grew a second string field; the only one that belongs is the \
             provider NAME: {view}"
        );
    }

    /// The page does not promise immediacy on its own authority.
    ///
    /// `restart_required` exists on the server's view so the client can stop
    /// claiming a save is live if that ever stops being true (PMS-1444 swaps
    /// the running client, so it is `false` today). A hard-coded "no restart"
    /// line would turn that field into decoration and the page into a lie on
    /// the day it flipped.
    #[test]
    fn the_save_message_reads_the_servers_answer_rather_than_asserting_it() {
        let src = include_str!("settings_google_client.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(
            head.contains("if saved.restart_required"),
            "the save message has to branch on what the server said"
        );
        let promise = "in use now, with no restart";
        assert!(
            head.contains(promise),
            "and it still says so in the case where it is true"
        );
        // The claim appears once, inside that branch. A second copy would be
        // the unconditional one coming back.
        assert_eq!(
            head.matches(promise).count(),
            1,
            "the no-restart claim appears more than once, so one of them is unconditional"
        );
    }

    /// Saving is refused until both halves are present, in the handler and not
    /// only in the button.
    ///
    /// A disabled button is a hint, not a gate: the handler re-checks, because
    /// the server refuses a partial write with a 422 and the operator should
    /// read why from this page rather than from a toast.
    #[test]
    fn the_handler_refuses_a_partial_pair_too() {
        let src = include_str!("settings_google_client.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(
            head.contains("if body.client_id.is_empty() || body.client_secret.is_empty()"),
            "the save handler has to re-check the pair, not trust the disabled button"
        );
        assert!(
            head.contains("!complete"),
            "and the button is disabled while the pair is incomplete"
        );
    }
}

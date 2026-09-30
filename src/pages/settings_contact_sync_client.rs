//! This organisation's Google OAuth client, set in the app (PMS-1264, PMS-1340).
//!
//! `GET` / `PUT /integrations/contact-sync/google/client`. The client is the
//! ORGANISATION's: the server resolves this organisation's own registration
//! first, falls back to the deployment-wide value PMS-1264 stored, then to
//! `GOOGLE_CONTACTS_CLIENT_ID` / `GOOGLE_CONTACTS_CLIENT_SECRET`, and
//! [`source_text`] says which of the three answered. That matters to an admin
//! rather than being bookkeeping: the consent screen a customer reads names
//! whichever application it is, the API quota belongs to whoever owns it, and so
//! does the verification status.
//!
//! Any admin sees this for their own organisation (the server answers
//! `client_editable` on the status read). PMS-1264 allowed only an admin of the
//! deployment's own organisation, on the grounds that a customer organisation
//! must not be able to swap the client every other one connects through; that
//! reason dissolved when there stopped being such a shared client.
//!
//! MAPPS-972 is why the card carries the console steps rather than one sentence
//! naming the console. Enabling the right API, asking for the right scopes and
//! deciding whether the app may stay in Testing are each a dead end that looks
//! like a Mokosh failure from here, and the person who hits one is in somebody
//! else's UI with nothing to go on.
//!
//! The secret is write-only. The server never returns it, only whether one is
//! set, so the field stays empty and a blank field keeps the stored secret.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    BannerTone, Button, ButtonVariant, Card, ConfirmDialog, ErrorBanner, Input, StatusBanner,
};

const CLIENT_PATH: &str = "/integrations/contact-sync/google/client";

/// The Google Cloud console, for the links beside the steps.
const CONSOLE_URL: &str = "https://console.cloud.google.com/";

/// What to do in Google Cloud, in the order it has to be done (MAPPS-972).
///
/// Checked against the server rather than written from memory:
/// `contact_sync::oauth::SCOPES` is exactly these three, `authorization_url`
/// sends `access_type=offline` with `prompt=consent`, and the redirect is
/// `{PUBLIC_API_BASE_URL}/api/v1/public/contact-sync/google/callback`, which this
/// card already renders verbatim below. A step here that drifts from that sends
/// an admin to configure something Google will then refuse, so the two belong in
/// one change.
const CONSOLE_STEPS: &[&str] = &[
    "In the Google Cloud console, create a project or pick one.",
    "APIs and Services, Library: enable the Google People API. Without it every import fails with SERVICE_DISABLED.",
    "Google Auth Platform (older consoles call it the OAuth consent screen): fill in Branding, then set Audience to External unless this account is in a Google Workspace organization. While the app is in Testing, add the account you will connect as a test user.",
    "Data access: add exactly three scopes - .../auth/contacts.readonly, openid and email. Read-only is the whole bargain: Mokosh can never change anything in Google Contacts.",
    "Clients, Create client, application type Web application. Paste the redirect URI above under Authorized redirect URIs, exactly as it appears. No JavaScript origins are needed, because the exchange happens on the server.",
    "Copy the client ID and client secret it shows you into the fields below.",
];

/// The one Google rule that fails quietly a week later, so it is on the page
/// rather than in a runbook nobody has open.
const TESTING_MODE_WARNING: &str = "An External app still in Testing gets a refresh token that Google expires after seven days. That is fine while you are trying this out; a connection meant to last needs the app published.";

/// `ClientSettingsView` (PMS-1264).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ClientSettings {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub secret_set: bool,
    #[serde(default)]
    pub redirect_uri: Option<String>,
}

/// `ClientSettingsInput`: `None` keeps a field, `""` clears it.
#[derive(Serialize)]
struct ClientSettingsBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
}

/// Where the client in force comes from, in words.
///
/// PMS-1340 split the old `database` in two, because "set here" stopped being one
/// thing: the credential is now this organisation's own Google registration, and
/// the deployment-wide value PMS-1264 stored is a deprecated fallback for a
/// tenant that has not entered one yet. An admin needs to know which of those is
/// answering, since the consent screen a customer reads names whichever
/// application it is.
pub fn source_text(source: &str) -> &'static str {
    match source {
        "tenant" => "Your organization's own Google registration. This is what your clients consent to.",
        "deployment" => "Falling back to the deployment-wide client set by your provider. Save your own registration here to use your own consent screen and quota.",
        "environment" => "Taken from the deployment's environment (GOOGLE_CONTACTS_CLIENT_ID). Saving here replaces it for this organization.",
        "incomplete" => "A client id is saved without its secret, so nothing can connect. Enter the secret.",
        _ => "Not set. Nobody in this organization can connect Google Contacts until it is.",
    }
}

/// What the save sends: the id always (so the form is what is stored), the
/// secret only when one was typed, so a blank field keeps the stored one.
fn body_for(client_id: &str, client_secret: &str) -> ClientSettingsBody {
    ClientSettingsBody {
        client_id: Some(client_id.trim().to_string()),
        client_secret: Some(client_secret.trim().to_string()).filter(|s| !s.is_empty()),
    }
}

#[component]
pub fn GoogleClientForm(on_change: EventHandler<()>) -> Element {
    let mut settings = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<ClientSettings>(CLIENT_PATH)
            .await
            .inspect_err(|e| tracing::error!("google client settings load failed: {e}"))
            .ok()
    });
    let mut client_id = use_signal(String::new);
    let mut client_secret = use_signal(String::new);
    let mut seeded = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut confirm_clear = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();

    use_effect(move || {
        if let Some(Some(s)) = settings.read().as_ref() {
            if !*seeded.peek() {
                client_id.set(s.client_id.clone().unwrap_or_default());
                seeded.set(true);
            }
        }
    });

    let mut send = move |body: ClientSettingsBody, done: &'static str| {
        busy.set(true);
        error.set(String::new());
        saved.set(false);
        spawn(async move {
            #[cfg(feature = "app")]
            match crate::hooks::fetch::api::put_authed::<ClientSettings, _>(CLIENT_PATH, &body)
                .await
            {
                Ok(fresh) => {
                    client_id.set(fresh.client_id.clone().unwrap_or_default());
                    client_secret.set(String::new());
                    saved.set(true);
                    tracing::info!("{done}");
                    settings.restart();
                    on_change.call(());
                }
                Err(e) => error.set(format!("Could not save the Google client: {e}")),
            }
            #[cfg(not(feature = "app"))]
            let _ = (body, done);
            busy.set(false);
        });
    };

    let snap = settings.read_unchecked().clone();
    rsx! {
        Card {
            title: "Google sign-in client",
            subtitle: "The Google Cloud OAuth client your organization connects Google Contacts with. Your clients see its consent screen, and its API quota is yours.",
            class: "mt-6",
            match snap {
                None => rsx! { crate::components::DetailSkeleton {} },
                Some(None) => rsx! { ErrorBanner { "Could not load the Google client settings." } },
                Some(Some(current)) => {
                    let secret_hint = if current.secret_set {
                        "A secret is stored. Leave blank to keep it."
                    } else {
                        "From the Google Cloud console, beside the client id."
                    };
                    rsx! {
                        div { class: "space-y-4",
                            p { class: "text-sm text-muted", "{source_text(&current.source)}" }
                            if let Some(uri) = current.redirect_uri.clone() {
                                div { class: "text-sm",
                                    p { class: "text-muted",
                                        "In the Google Cloud console, create an OAuth client of type Web application and add this as an authorized redirect URI:"
                                    }
                                    code { class: "mt-1 block break-all rounded bg-surface-2 px-2 py-1 text-content", "{uri}" }
                                }
                            } else {
                                StatusBanner { tone: BannerTone::Warning,
                                    "This deployment has no public API address (PUBLIC_API_BASE_URL), so Google has nowhere to return to and nothing can connect yet."
                                }
                            }
                            // Open for somebody who has nothing stored, which is
                            // exactly the person who needs them, and closed once
                            // a client is in place so the card stays short for
                            // everybody else.
                            details {
                                class: "rounded border border-line bg-surface-2 p-3",
                                open: current.client_id.is_none() && !current.secret_set,
                                summary { class: "cursor-pointer text-sm font-medium text-content",
                                    "How to create this client in Google Cloud"
                                }
                                ol { class: "mt-3 list-decimal space-y-2 pl-5 text-sm text-muted",
                                    for step in CONSOLE_STEPS {
                                        li { "{step}" }
                                    }
                                }
                                p { class: "mt-3 text-sm text-muted",
                                    a {
                                        class: "underline",
                                        href: CONSOLE_URL,
                                        target: "_blank",
                                        rel: "noopener noreferrer",
                                        "Open the Google Cloud console"
                                    }
                                }
                                p { class: "mt-3 text-xs text-subtle", "{TESTING_MODE_WARNING}" }
                            }
                            if !error().is_empty() {
                                ErrorBanner { "{error}" }
                            }
                            if saved() {
                                StatusBanner { tone: BannerTone::Success,
                                    "Saved. It takes effect for the next connection straight away."
                                }
                            }
                            Input {
                                name: "google_client_id",
                                label: "Client ID",
                                value: client_id(),
                                placeholder: "1234-abc.apps.googleusercontent.com",
                                disabled: busy(),
                                oninput: move |e: FormEvent| client_id.set(e.value()),
                            }
                            Input {
                                name: "google_client_secret",
                                label: "Client secret",
                                r#type: "password",
                                value: client_secret(),
                                help: secret_hint,
                                disabled: busy(),
                                oninput: move |e: FormEvent| client_secret.set(e.value()),
                            }
                            p { class: "text-xs text-subtle",
                                "Changing the client after an account is connected means connecting it again: Google ties each connection to the client that made it."
                            }
                            div { class: "flex flex-wrap gap-3",
                                Button {
                                    disabled: busy() || !can_mutate || client_id().trim().is_empty(),
                                    onclick: move |_| send(body_for(&client_id(), &client_secret()), "google client saved"),
                                    data_testid: "google-client-save",
                                    "Save client"
                                }
                                // PMS-1340: only a credential stored at THIS level
                                // can be cleared from here. On `deployment` there
                                // is nothing of this tenant's to clear, and the
                                // fallback belongs to the provider.
                                if current.source == "tenant" || current.source == "incomplete" {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        disabled: busy() || !can_mutate,
                                        onclick: move |_| confirm_clear.set(true),
                                        data_testid: "google-client-clear",
                                        "Clear and use the environment"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        ConfirmDialog {
            open: confirm_clear(),
            title: "Clear the Google client?",
            message: "The client id and secret saved here are removed, and the deployment falls back to its environment settings. If those are not set, nobody can connect Google Contacts, and connected accounts stop syncing until a client is set again.",
            confirm_text: "Clear",
            destructive: true,
            loading: busy(),
            onconfirm: move |_| {
                confirm_clear.set(false);
                send(
                    ClientSettingsBody { client_id: Some(String::new()), client_secret: None },
                    "google client cleared",
                );
            },
            oncancel: move |_| confirm_clear.set(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank secret keeps the stored one; a typed one is sent trimmed.
    #[test]
    fn a_blank_secret_is_not_sent() {
        let kept = serde_json::to_value(body_for(" id.apps.googleusercontent.com ", "  ")).unwrap();
        assert_eq!(
            kept,
            serde_json::json!({ "client_id": "id.apps.googleusercontent.com" })
        );
        let sent = serde_json::to_value(body_for("id", " s3cret ")).unwrap();
        assert_eq!(sent["client_secret"], "s3cret");
    }

    #[test]
    fn every_source_is_explained() {
        for source in ["tenant", "deployment", "environment", "incomplete", "none"] {
            assert!(!source_text(source).is_empty(), "{source}");
        }
        assert!(source_text("incomplete").contains("Enter the secret"));
        // PMS-1340: the two levels have to read differently, or an admin cannot
        // tell their own registration from their provider's fallback, which is
        // the whole point of showing the source at all.
        assert_ne!(source_text("tenant"), source_text("deployment"));
        assert!(
            source_text("deployment").contains("your own"),
            "the fallback should say what to do about it: {}",
            source_text("deployment")
        );
    }

    /// MAPPS-972: no copy on either page describes the client as the
    /// deployment's or as shared across organisations.
    ///
    /// PMS-1340 made it the organisation's and `source_text` was updated; three
    /// other strings were not, so the page told an admin their entry affected
    /// every organisation on the deployment and pointed them at their provider
    /// for something they can do themselves. Wrong words are one thing; the wrong
    /// ACTION is what this guards. Both files are scanned, because the copy the
    /// admin reads is split between the status card and this one, and the needles
    /// are assembled here so this test's own prose is not a match.
    #[test]
    fn no_copy_calls_the_client_the_deployments() {
        let dep = "deployment";
        let stale = [
            format!("this {dep}'s Google"),
            format!("every organisation on the {dep}"),
            format!("{dep} connects Google Contacts"),
            format!("This {dep} has no Google sign-in client"),
        ];
        let client_src = include_str!("settings_contact_sync_client.rs");
        let head = &client_src[..client_src.find("mod tests").expect("this module")];
        let status_src = include_str!("settings_contact_sync.rs");
        for (name, text) in [
            ("settings_contact_sync_client.rs", head),
            ("settings_contact_sync.rs", status_src),
        ] {
            for needle in &stale {
                assert!(
                    !text.contains(needle.as_str()),
                    "{name} still says {needle:?}; the client is the organisation's (PMS-1340)"
                );
            }
        }
    }

    /// The steps name what the server actually asks Google for.
    ///
    /// An admin configures Google from this list, so a scope missing here is a
    /// consent screen that grants less than the sync needs, and one that is
    /// stale is a permission nobody can explain. `contact_sync::oauth::SCOPES` on
    /// the server is the other half; these two move together or the page sends
    /// people to configure something Google will refuse.
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
            "the one rule that fails silently a week later has to be on the page"
        );
    }

    /// The secret field is a password input and never seeded from the server.
    #[test]
    fn the_secret_is_write_only() {
        let src = include_str!("settings_contact_sync_client.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("r#type: \"password\""));
        assert!(
            !head.contains("client_secret.set(s."),
            "the secret is never filled from a read"
        );
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
    }
}

//! The deployment's Google OAuth client, set in the app (PMS-1264).
//!
//! `GET` / `PUT /integrations/contact-sync/google/client`. One client for the
//! whole deployment, like the email settings: what is entered here wins over
//! `GOOGLE_CONTACTS_CLIENT_ID` / `GOOGLE_CONTACTS_CLIENT_SECRET`, and clearing
//! it falls back to them, so connecting Google Contacts no longer needs a
//! change to the deployment's environment.
//!
//! Only an admin of the deployment's own organisation sees this (the server
//! answers `client_editable` on the status read and refuses everyone else), so
//! a customer organisation on a shared deployment cannot swap the client every
//! other organisation connects through.
//!
//! The secret is write-only. The server never returns it, only whether one is
//! set, so the field stays empty and a blank field keeps the stored secret.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    BannerTone, Button, ButtonVariant, Card, ConfirmDialog, ErrorBanner, Input, StatusBanner,
};

const CLIENT_PATH: &str = "/integrations/contact-sync/google/client";

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
pub fn source_text(source: &str) -> &'static str {
    match source {
        "database" => "Set here. It is used instead of the deployment's environment.",
        "environment" => "Taken from the deployment's environment (GOOGLE_CONTACTS_CLIENT_ID). Saving here replaces it.",
        "incomplete" => "A client id is saved here without its secret, so nothing can connect. Enter the secret.",
        _ => "Not set. Nobody on this deployment can connect Google Contacts until it is.",
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
            subtitle: "The Google Cloud OAuth client this deployment connects Google Contacts with. One for every organisation on the deployment.",
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
                                if current.source == "database" || current.source == "incomplete" {
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
        for source in ["database", "environment", "incomplete", "none"] {
            assert!(!source_text(source).is_empty(), "{source}");
        }
        assert!(source_text("incomplete").contains("Enter the secret"));
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

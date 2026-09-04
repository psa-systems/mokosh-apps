//! A button that fetches a document with the bearer and saves it
//! (MAPPS-641).
//!
//! The SPA holds its token in memory, so a plain link to `/invoices/{id}/pdf`
//! navigates without an `Authorization` header and gets a 401. The pieces
//! that solve that already exist: `get_authed_bytes_typed` fetches with the
//! bearer and reads the filename the server sets in `Content-Disposition`,
//! and `save_bytes_as_file` hands the bytes to the browser's download shelf
//! or, on the desktop build, writes them somewhere and reports where
//! (MAPPS-504). Four pages need exactly that sequence with the same three
//! states (fetching, failed, saved-to-a-path), so it lives here once rather
//! than as four subtly different copies.
//!
//! Failure wording is by kind, not by raw string: a 403 is the wrong role, a
//! 404 is a document that is gone, a transport error is the network, and a
//! 5xx is the server. [`DownloadFailure`] is the classification, kept free of
//! the app-runtime feature so a test can pin every arm without a browser.

use dioxus::prelude::*;

use crate::components::{Button, ButtonSize, ButtonVariant};

/// Why a download did not produce a file, in the user's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadFailure {
    SessionExpired,
    Forbidden,
    NotFound,
    Network,
    Server,
    /// A status the arms above do not name, with the server's own message
    /// when it sent one.
    Other(String),
    /// The bytes arrived and the host could not write them.
    CouldNotSave(String),
}

impl DownloadFailure {
    /// From an HTTP status, or `None` for a transport failure.
    pub fn from_status(status: Option<u16>, message: &str) -> Self {
        match status {
            None => Self::Network,
            Some(401) => Self::SessionExpired,
            Some(403) => Self::Forbidden,
            Some(404) => Self::NotFound,
            Some(500..=599) => Self::Server,
            Some(code) => {
                if message.trim().is_empty() {
                    Self::Other(format!("The request failed ({code})."))
                } else {
                    Self::Other(message.trim().to_string())
                }
            }
        }
    }

    /// What the page shows. `what` names the document ("the invoice PDF").
    pub fn describe(&self, what: &str) -> String {
        match self {
            Self::SessionExpired => {
                "Your session has expired. Sign in again to download it.".to_string()
            }
            Self::Forbidden => format!("Your role cannot download {what}."),
            Self::NotFound => format!("{} is no longer available.", capitalize(what)),
            Self::Network => {
                "Could not reach the server. Check your connection and try again.".to_string()
            }
            Self::Server => format!("The server could not produce {what}. Try again in a moment."),
            Self::Other(message) => format!("Could not download {what}: {message}"),
            Self::CouldNotSave(reason) => {
                format!("Fetched {what} but could not save it: {reason}")
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// What the button reports after a click, rendered under it.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    Failed(String),
    /// The desktop build chose the destination itself; a file that lands
    /// somewhere the user cannot find has not been delivered (MAPPS-504).
    SavedTo(String),
}

#[derive(Props, Clone, PartialEq)]
pub struct DownloadButtonProps {
    /// API path, `/api/v1` relative, e.g. `/invoices/{id}/pdf`.
    pub path: String,
    /// Filename when the server sends no `Content-Disposition`.
    pub fallback_name: String,
    /// The document, for the failure sentence: "the invoice PDF".
    pub what: String,
    #[props(default = String::from("Download PDF"))]
    pub label: String,
    #[props(default = ButtonVariant::Secondary)]
    pub variant: ButtonVariant,
    #[props(default)]
    pub size: ButtonSize,
    /// Native tooltip: what the reader gets, which for an invoice differs
    /// between a draft and a sent one.
    #[props(default)]
    pub title: Option<String>,
    #[props(default)]
    pub disabled: bool,
}

#[component]
pub fn DownloadButton(props: DownloadButtonProps) -> Element {
    let mut busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<Outcome>);

    let path = props.path.clone();
    let fallback = props.fallback_name.clone();
    let what = props.what.clone();
    let on_click = move |_| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        outcome.set(None);
        let path = path.clone();
        let fallback = fallback.clone();
        let what = what.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::get_authed_bytes_typed(&path).await {
                    Ok((bytes, name)) => {
                        let filename = name.unwrap_or(fallback);
                        match crate::utils::download::save_bytes_as_file(&bytes, &filename) {
                            Ok(Some(saved)) => outcome.set(Some(Outcome::SavedTo(saved))),
                            // The browser shows its own download shelf.
                            Ok(None) => {}
                            Err(reason) => outcome.set(Some(Outcome::Failed(
                                DownloadFailure::CouldNotSave(reason).describe(&what),
                            ))),
                        }
                    }
                    Err(err) => {
                        let failure = DownloadFailure::from_status(
                            err.status_code(),
                            &match &err {
                                crate::hooks::fetch::api::ApiError::Status { message, .. } => {
                                    message.clone()
                                }
                                _ => String::new(),
                            },
                        );
                        outcome.set(Some(Outcome::Failed(failure.describe(&what))));
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (path, fallback, what);
            }
            busy.set(false);
        });
    };

    let current = outcome.read().clone();
    rsx! {
        div { class: "inline-flex flex-col items-start gap-1",
            Button {
                variant: props.variant,
                size: props.size,
                loading: *busy.read(),
                disabled: props.disabled,
                title: props.title.clone(),
                onclick: on_click,
                "{props.label}"
            }
            match current {
                Some(Outcome::Failed(message)) => rsx! {
                    p { class: "text-xs text-red-600 dark:text-red-300", role: "alert", "{message}" }
                },
                Some(Outcome::SavedTo(saved)) => rsx! {
                    p { class: "text-xs text-muted", role: "status", "Saved to {saved}" }
                },
                None => rsx! {},
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadFailure;

    /// Every failure the four routes can produce has its own sentence, and
    /// none of them is the raw string the transport hands back.
    #[test]
    fn each_failure_kind_has_its_own_sentence() {
        assert_eq!(
            DownloadFailure::from_status(None, ""),
            DownloadFailure::Network
        );
        assert_eq!(
            DownloadFailure::from_status(Some(401), ""),
            DownloadFailure::SessionExpired
        );
        assert_eq!(
            DownloadFailure::from_status(Some(403), "Forbidden"),
            DownloadFailure::Forbidden
        );
        assert_eq!(
            DownloadFailure::from_status(Some(404), ""),
            DownloadFailure::NotFound
        );
        assert_eq!(
            DownloadFailure::from_status(Some(502), ""),
            DownloadFailure::Server
        );
        assert_eq!(
            DownloadFailure::from_status(Some(400), "format must be csv or pdf"),
            DownloadFailure::Other("format must be csv or pdf".to_string())
        );
        assert_eq!(
            DownloadFailure::from_status(Some(418), "  "),
            DownloadFailure::Other("The request failed (418).".to_string())
        );
    }

    #[test]
    fn the_sentence_names_the_document() {
        assert_eq!(
            DownloadFailure::Forbidden.describe("the invoice PDF"),
            "Your role cannot download the invoice PDF."
        );
        assert_eq!(
            DownloadFailure::NotFound.describe("the credit note PDF"),
            "The credit note PDF is no longer available."
        );
        assert_eq!(
            DownloadFailure::CouldNotSave("disk full".to_string()).describe("the statement PDF"),
            "Fetched the statement PDF but could not save it: disk full"
        );
        assert!(DownloadFailure::Server
            .describe("the report")
            .contains("could not produce the report"));
    }
}

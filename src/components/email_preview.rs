//! Read the email before it goes out (MAPPS-482).
//!
//! Every send trigger in the app is a one-way door: the operator clicks and
//! finds out what the recipient read by asking them. This is the "Preview
//! email" affordance that sits beside such a trigger. It opens a modal and
//! asks the server, through `POST /notifications/preview` (PMS-808), to render
//! exactly what `dispatch` would render for an event type plus a context, and
//! to send nothing.
//!
//! Two rules the implementation exists to hold:
//!
//! 1. `body_html` is a tenant-editable template, so it NEVER reaches the DOM as
//!    markup. The modal shows `body_text`, and when that is all there is, the
//!    HTML source escaped inside a `pre`. There is no `dangerous_inner_html`
//!    here and there must never be one: a preview that can execute a template's
//!    markup inside an authenticated app is an XSS surface bought for a
//!    cosmetic gain.
//! 2. The preview never gates the send. A slow, empty or failed preview shows
//!    what happened and leaves the send button exactly as it was.

use dioxus::prelude::*;

use super::button::{Button, ButtonVariant};
use super::error_banner::ErrorBanner;
use super::modal::{Modal, ModalSize};

/// One message the server would send, as returned by
/// `POST /notifications/preview`.
///
/// Field names mirror `NotificationPreviewResponse` in mokosh-server
/// (`src/modules/notifications/models.rs`). `unresolved` names the
/// `{{key}}` placeholders the context did not carry: they are the send-time
/// values (a minted token and its link) the preview deliberately does not
/// create, and they stay literal in the rendered text.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct EmailPreviewEntry {
    pub rule_name: String,
    pub channel: String,
    pub recipients: Vec<String>,
    pub subject: Option<String>,
    pub body_text: String,
    pub body_html: Option<String>,
    pub unresolved: Vec<String>,
}

/// What the modal shows for one entry's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewBody {
    /// The plain-text body, shown as the recipient reads it.
    Text(String),
    /// No text body, so the HTML template's source is shown escaped instead.
    HtmlSource(String),
    /// The rule renders neither.
    Empty,
}

/// Pick the body rendering for one entry. Text wins whenever there is any;
/// HTML is only ever offered as source, never as markup.
pub fn preview_body(body_text: &str, body_html: Option<&str>) -> PreviewBody {
    if !body_text.trim().is_empty() {
        return PreviewBody::Text(body_text.to_string());
    }
    match body_html.map(str::trim).filter(|h| !h.is_empty()) {
        Some(html) => PreviewBody::HtmlSource(html.to_string()),
        None => PreviewBody::Empty,
    }
}

/// MAPPS-642: a message the SERVER composes rather than a notification rule,
/// described by the page that knows its shape.
///
/// `POST /notifications/preview` renders rules, so for a trigger like the
/// invoice pay-now mail (built in `billing::service`, PMS-711) it answers
/// with nothing, and the modal used to say "No email rule matches this
/// action, so nothing will be sent" over that. The first half was true of
/// rules and the second half was false of the send, and an operator read it
/// as "email is not configured". A page that mirrors the server's template
/// passes one of these instead, with `blockers` naming every condition under
/// which the server would send nothing, so the modal can say what Send does
/// and why it might do nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinEmail {
    /// Who receives it, as a sentence or an address.
    pub recipient: String,
    pub subject: String,
    /// Plain text, with `{{key}}` for a value the server fills in when it
    /// sends (a minted link), listed in `unresolved`.
    pub body: String,
    pub unresolved: Vec<String>,
    /// Why Send would email nobody, each a full sentence. Empty when every
    /// condition the server checks is met as far as the page can tell.
    pub blockers: Vec<String>,
    /// What the message will lack without stopping it (MAPPS-663): a pay
    /// link with no gateway connected. Rendered as information, not a
    /// warning, because Send still mails.
    pub notes: Vec<String>,
}

/// "Preview email" trigger plus its modal.
///
/// Render it beside the send button, not in place of it.
#[component]
pub fn EmailPreview(
    /// The notification event type the send dispatches, e.g.
    /// `forms.request_link`.
    event_type: String,
    /// The context this form already holds. Anything it cannot supply comes
    /// back in `unresolved` and is shown as filled in when sent.
    context: serde_json::Value,
    /// Optional extra sentence under the empty-response line, for a trigger
    /// whose message does not come from a notification rule at all.
    #[props(default)]
    empty_note: String,
    /// MAPPS-642: a server-built message to show instead of asking the rules
    /// endpoint, which cannot render it. When `Some`, no request is made.
    #[props(default)]
    builtin: Option<BuiltinEmail>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut entries = use_signal(Vec::<EmailPreviewEntry>::new);

    let request = serde_json::json!({ "event_type": event_type, "context": context });
    let is_builtin = builtin.is_some();

    let on_open = move |_| {
        let request = request.clone();
        open.set(true);
        error.set(String::new());
        entries.set(Vec::new());
        if is_builtin {
            return;
        }
        loading.set(true);
        spawn(async move {
            match crate::hooks::fetch::api::post_authed_typed::<Vec<EmailPreviewEntry>, _>(
                "/notifications/preview",
                &request,
            )
            .await
            {
                Ok(list) => entries.set(list),
                Err(e) => {
                    // Visible in the modal AND in the log: the operator is
                    // about to send, so "the preview is unavailable" is a
                    // thing they have to be told, not swallowed.
                    tracing::warn!("email preview request failed: {e:?}");
                    error.set(e.user_message());
                }
            }
            loading.set(false);
        });
    };

    let list = entries.read().clone();

    rsx! {
        Button {
            variant: ButtonVariant::Link,
            onclick: on_open,
            "Preview email"
        }

        Modal {
            open: open(),
            title: "Preview email".to_string(),
            size: ModalSize::Large,
            onclose: move |_| open.set(false),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| open.set(false),
                    "Close"
                }
            },

            div { class: "space-y-4",
                if let Some(mail) = builtin.as_ref() {
                    if !mail.blockers.is_empty() {
                        crate::components::StatusBanner {
                            tone: crate::components::BannerTone::Warning,
                            p { class: "font-medium", "Send will be refused as things stand." }
                            ul { class: "mt-1 list-disc pl-5 space-y-1",
                                for reason in mail.blockers.iter() {
                                    li { key: "{reason}", "{reason}" }
                                }
                            }
                        }
                    }
                    if !mail.notes.is_empty() {
                        crate::components::StatusBanner {
                            tone: crate::components::BannerTone::Info,
                            ul { class: "list-disc pl-5 space-y-1",
                                for note in mail.notes.iter() {
                                    li { key: "{note}", "{note}" }
                                }
                            }
                        }
                    }
                    p { class: "text-sm text-muted",
                        "This message is built into the server rather than by a notification rule, so it cannot be edited under Settings. What follows is the text the server sends, with the values it fills in at send time marked."
                    }
                    EmailPreviewEntryView {
                        entry: EmailPreviewEntry {
                            rule_name: "Built into the server".to_string(),
                            channel: "email".to_string(),
                            recipients: vec![mail.recipient.clone()],
                            subject: Some(mail.subject.clone()),
                            body_text: mail.body.clone(),
                            body_html: None,
                            unresolved: mail.unresolved.clone(),
                        },
                    }
                } else if loading() {
                    p { class: "text-sm text-subtle", "Rendering what the recipient will see." }
                } else if !error().is_empty() {
                    ErrorBanner { "Could not render the preview: {error()}" }
                } else if list.is_empty() {
                    div {
                        p { class: "text-sm text-content",
                            "No email rule matches this action, so nothing will be sent."
                        }
                        if !empty_note.is_empty() {
                            p { class: "mt-2 text-sm text-subtle", "{empty_note}" }
                        }
                    }
                } else {
                    for (i , entry) in list.iter().enumerate() {
                        EmailPreviewEntryView { key: "{i}", entry: entry.clone() }
                    }
                }
            }
        }
    }
}

/// One rendered message: who it goes to, what it says, and which values are
/// still to be filled in.
#[component]
fn EmailPreviewEntryView(entry: EmailPreviewEntry) -> Element {
    let recipients = if entry.recipients.is_empty() {
        "No recipient resolves for this rule.".to_string()
    } else {
        entry.recipients.join(", ")
    };
    let subject = entry
        .subject
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "No subject".to_string());
    let body = preview_body(&entry.body_text, entry.body_html.as_deref());

    rsx! {
        div { class: "rounded-md border border-line p-3 space-y-3",
            p { class: "text-xs uppercase tracking-wide text-muted",
                "{entry.rule_name} via {entry.channel}"
            }

            div {
                p { class: "text-xs text-muted", "To" }
                p { class: "text-sm text-content break-words", "{recipients}" }
            }

            div {
                p { class: "text-xs text-muted", "Subject" }
                p { class: "text-sm text-content break-words", "{subject}" }
            }

            div {
                p { class: "text-xs text-muted", "Body" }
                match body {
                    // Text nodes are escaped by Dioxus, which is the whole
                    // point for the HTML branch: the template's source is
                    // read, never executed.
                    PreviewBody::Text(text) => rsx! {
                        pre { class: "mt-1 whitespace-pre-wrap break-words text-sm text-content", "{text}" }
                    },
                    PreviewBody::HtmlSource(html) => rsx! {
                        p { class: "mt-1 text-xs text-subtle",
                            "This rule has no plain-text body. Its HTML source is shown as text, never rendered."
                        }
                        pre { class: "mt-1 whitespace-pre-wrap break-words text-xs text-content", "{html}" }
                    },
                    PreviewBody::Empty => rsx! {
                        p { class: "mt-1 text-sm text-subtle", "This rule renders no body." }
                    },
                }
            }

            if !entry.unresolved.is_empty() {
                ul { class: "text-xs text-subtle space-y-1",
                    for key in entry.unresolved.iter() {
                        li { key: "{key}", "filled in when sent: {key}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_body_wins_over_html() {
        assert_eq!(
            preview_body("Open {{link}}.", Some("<p>Open</p>")),
            PreviewBody::Text("Open {{link}}.".to_string())
        );
    }

    #[test]
    fn html_only_is_shown_as_source() {
        assert_eq!(
            preview_body("", Some("<p>Open</p>")),
            PreviewBody::HtmlSource("<p>Open</p>".to_string())
        );
        // Whitespace is not a text body.
        assert_eq!(
            preview_body("   \n", Some("<p>Open</p>")),
            PreviewBody::HtmlSource("<p>Open</p>".to_string())
        );
    }

    #[test]
    fn neither_body_is_reported_as_empty() {
        assert_eq!(preview_body("", None), PreviewBody::Empty);
        assert_eq!(preview_body("  ", Some("   ")), PreviewBody::Empty);
    }

    #[test]
    fn entry_deserializes_the_server_shape() {
        let entry: EmailPreviewEntry = serde_json::from_str(
            r#"{"rule_name":"Request form link","channel":"email",
                "recipients":["a@example.com"],"subject":"A form to fill in",
                "body_text":"Open {{form_link}}.","body_html":null,
                "unresolved":["form_link"]}"#,
        )
        .expect("the PMS-808 response shape deserializes");
        assert_eq!(entry.recipients, vec!["a@example.com".to_string()]);
        assert_eq!(entry.subject.as_deref(), Some("A form to fill in"));
        assert_eq!(entry.unresolved, vec!["form_link".to_string()]);
        assert!(entry.body_html.is_none());
    }
}

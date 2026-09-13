//! MAPPS-775: what a portal contact sees where they lack access, and how they
//! ask for it.
//!
//! A contact who lacks a capability met an empty screen that said nothing, and
//! could not tell a portal with nothing in it from a portal that will not show
//! them what is there. Worse, the state they landed on was written for staff:
//! "restricted to administrator and finance roles. Ask an administrator" is
//! true of a technician and false of a customer, who has no administrator and
//! no role in the MSP's staff sense at all - the MAPPS-763 class of defect,
//! copy that asserts something untrue of the person reading it.
//!
//! So this is the customer's half, and it names the MSP rather than "your
//! administrator", says what is not shared rather than what is restricted, and
//! offers the one action that can change it.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Card, IconSize, InformationIcon, PageHeader};

/// MAPPS-775 / PMS-1187: the portal areas a contact can ask for, keyed the way
/// the server's `ACCESS_AREAS` keys them.
///
/// The client sends the AREA and never a capability: a customer asking to see
/// their invoices should not have to know that `invoices:pay` and
/// `invoices:download_pdf` are separate strings, and the server picks the role
/// that answers the request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccessArea {
    /// As the server's closed set names it.
    pub key: &'static str,
    /// What the customer calls it, mid-sentence.
    pub label: &'static str,
}

pub const INVOICES: AccessArea = AccessArea {
    key: "invoices",
    label: "invoices",
};
pub const TICKETS: AccessArea = AccessArea {
    key: "tickets",
    label: "tickets",
};
pub const QUOTES: AccessArea = AccessArea {
    key: "quotes",
    label: "quotes",
};

/// What the page says when a contact cannot see an area.
///
/// Names the MSP when the brand supplies one, because "ask Niceguy IT" is
/// actionable and "ask your administrator" is not: a customer has no
/// administrator, and the person they need is the MSP they hired.
pub(crate) fn access_required_body(area: &str, msp_name: Option<&str>) -> String {
    match msp_name {
        Some(name) => format!("Your {name} portal does not share {area} with your account. You can ask them for access."),
        None => format!("This portal does not share {area} with your account. You can ask your provider for access."),
    }
}

/// What the page says once the request has gone.
///
/// Says the request landed and who has it, so the customer knows the next move
/// is not theirs. It deliberately does not promise a timeframe.
pub(crate) fn access_requested_body(msp_name: Option<&str>) -> String {
    match msp_name {
        Some(name) => format!(
            "Your request has gone to {name}. They will let you know when access is granted."
        ),
        None => {
            "Your request has gone to your provider. They will let you know when access is granted."
                .to_string()
        }
    }
}

/// The heading, which names the area so a customer who opened two tabs knows
/// which one this is.
pub(crate) fn access_required_title(area: &str) -> String {
    let mut chars = area.chars();
    match chars.next() {
        Some(first) => format!(
            "{}{} are not shared with you",
            first.to_uppercase(),
            chars.as_str()
        ),
        None => "This is not shared with you".to_string(),
    }
}

/// MAPPS-775 / PMS-1187: `POST /api/v1/contact/access-requests`.
///
/// Typed rather than a `json!` literal for the MAPPS-685 reason the contacts
/// page has a guard about: a literal compiles clean against any DTO, so the
/// field names and the server's agree only by luck.
#[derive(Debug, serde::Serialize)]
struct AccessRequestBody {
    /// The portal area, keyed the way the server's closed `ACCESS_AREAS` set
    /// keys it. Never a capability name.
    area: &'static str,
    /// What the customer wrote, omitted entirely when they wrote nothing, so
    /// the server stores an absent note rather than an empty string.
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Props, Clone, PartialEq)]
pub struct PortalAccessRequiredProps {
    /// The page title, which stays the section's name: the customer navigated
    /// to Invoices and should still be looking at a page called Invoices.
    pub title: String,
    pub area: AccessArea,
}

/// MAPPS-775: the contact-plane empty state, with the ask.
#[component]
pub fn PortalAccessRequired(props: PortalAccessRequiredProps) -> Element {
    let area = props.area;
    let brand = crate::hooks::branding::EFFECTIVE_BRANDING.read().clone();
    let msp_name = brand
        .display_name
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| brand.company_name.clone().filter(|s| !s.is_empty()));

    let mut note = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut sent = use_signal(|| false);
    let mut error = use_signal(String::new);
    let can_mutate = crate::hooks::use_can_mutate();

    let body = access_required_body(area.label, msp_name.as_deref());
    let sent_body = access_requested_body(msp_name.as_deref());
    let heading = access_required_title(area.label);

    let handle_ask = move |_| {
        if *sending.read() || *sent.read() {
            return;
        }
        sending.set(true);
        error.set(String::new());
        let body = AccessRequestBody {
            area: area.key,
            note: Some(note.read().trim().to_string()).filter(|n| !n.is_empty()),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed_any_typed::<serde_json::Value, _>(
                    "/access-requests",
                    &body,
                )
                .await
                {
                    Ok(_) => sent.set(true),
                    Err(err) => error.set(format!(
                        "Could not send your request: {}",
                        err.user_message()
                    )),
                }
            }
            sending.set(false);
        });
    };

    rsx! {
        PageHeader { title: "{props.title}" }
        Card {
            div { class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                div { class: "mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-surface-2",
                    InformationIcon { size: IconSize::Large, class: "text-subtle".to_string() }
                }
                h3 { class: "text-base font-medium text-content", "{heading}" }
                if *sent.read() {
                    p { class: "mt-2 text-sm text-muted", "{sent_body}" }
                } else {
                    p { class: "mt-2 text-sm text-muted", "{body}" }
                    div { class: "mt-4 w-full",
                        crate::components::Textarea {
                            name: "access_request_note",
                            label: "Anything to add (optional)",
                            rows: 2,
                            value: note.read().clone(),
                            oninput: move |e: FormEvent| note.set(e.value()),
                        }
                    }
                    if !error.read().is_empty() {
                        p { class: "mt-2 text-sm text-red-600 dark:text-red-300", "{error}" }
                    }
                    div { class: "mt-4",
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: *sending.read(),
                            disabled: !can_mutate || *sending.read(),
                            title: (!can_mutate).then(|| "Can't reach the server to send your request".to_string()),
                            onclick: handle_ask,
                            "Ask for access"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{access_requested_body, access_required_body, access_required_title};

    /// The MSP is named where the brand supplies one, because "ask Niceguy IT"
    /// is actionable and "ask your administrator" is not: a customer has no
    /// administrator.
    #[test]
    fn the_copy_names_the_msp_when_it_can() {
        let named = access_required_body("invoices", Some("Niceguy IT"));
        assert!(named.contains("Niceguy IT"), "{named}");
        assert!(named.contains("invoices"), "{named}");
        assert!(
            !named.contains("administrator"),
            "a customer has no administrator: {named}"
        );
    }

    /// With no brand loaded the sentence still stands on its own and still
    /// points somewhere real.
    #[test]
    fn the_copy_holds_without_a_brand() {
        let anonymous = access_required_body("tickets", None);
        assert!(anonymous.contains("tickets"), "{anonymous}");
        assert!(anonymous.contains("provider"), "{anonymous}");
        assert!(
            !anonymous.contains("{"),
            "no unreplaced placeholder: {anonymous}"
        );
    }

    /// The sent state says the request landed and whose move it is now, and
    /// promises no timeframe the MSP has not agreed to.
    #[test]
    fn the_sent_state_says_where_the_request_went() {
        let sent = access_requested_body(Some("Niceguy IT"));
        assert!(sent.contains("Niceguy IT"), "{sent}");
        for promise in ["24", "48", "shortly", "soon"] {
            assert!(!sent.contains(promise), "no timeframe was agreed: {sent}");
        }
    }

    /// The heading names the area, so a customer with two tabs open knows
    /// which one they are looking at.
    #[test]
    fn the_heading_names_the_area() {
        assert_eq!(
            access_required_title("invoices"),
            "Invoices are not shared with you"
        );
        assert_eq!(
            access_required_title("tickets"),
            "Tickets are not shared with you"
        );
    }
}

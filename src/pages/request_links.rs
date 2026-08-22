//! PMS-730: sending a client a request-form link, and seeing what became of
//! the ones already sent.
//!
//! Lives on the company detail page, because the workflow is "I am looking at
//! this client and I need something from them" rather than "I am looking at a
//! form". mokosh-server issues the link and emails it
//! (`src/modules/forms/request_links.rs`); this surface chooses the form and
//! the recipient.
//!
//! In its own module rather than inside `contacts.rs`, which is already the
//! largest page in the repo at ~4.8k lines. The card owns its own resource so
//! placing it costs the detail page one line and it can refresh itself after
//! a send.
//!
//! MAPPS-424: the same send lives on the builder too, reached the other way
//! round ("I have just defined this form, send it to someone"). That entry
//! point picks the company first and then hands off to the modal below with
//! the form already chosen, so both routes issue the link through one
//! code path.

use chrono::Utc;
use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, CollapsibleCard, ErrorBanner, IconSize,
    Input, MailIcon, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::modules::forms::{
    FormDefinition, IssueRequestLinkRequest, RequestLink, RequestLinkStatus,
};

#[derive(Clone, Debug, Deserialize)]
struct PickerContact {
    id: Uuid,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: Option<String>,
}

#[component]
pub fn CompanyRequestFormsCard(company_id: String, company_name: String) -> Element {
    let links_company_id = company_id.clone();
    let mut links = use_resource(move || {
        let id = links_company_id.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            let _token = crate::hooks::fetch::api::current_access_token()?;
            crate::hooks::fetch::api::get_authed_typed::<Vec<RequestLink>>(&format!(
                "/form-request-links?company_id={id}"
            ))
            .await
            .ok()
        }
    });

    let mut sending = use_signal(|| false);
    let snap = links.read_unchecked();
    let count = match &*snap {
        Some(Some(rows)) => Some(rows.len() as u64),
        _ => None,
    };
    let can_mutate = crate::hooks::use_can_mutate();
    let now = Utc::now();

    rsx! {
        CollapsibleCard {
            title: "Request forms",
            count,
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Link,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't send while the server is unreachable".to_string()),
                    onclick: move |_| sending.set(true),
                    "Send a form"
                }
            },
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Form" }
                        TableHeader { "Sent to" }
                        TableHeader { "Status" }
                        TableHeader { "Expires" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 4, rows: 3 } },
                    Some(None) => rsx! {
                        TableEmpty { columns: 4, message: "Could not load request forms.".to_string() }
                    },
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty {
                            columns: 4,
                            message: "No request forms sent to this client yet.".to_string(),
                        }
                    },
                    Some(Some(rows)) => rsx! {
                        TableBody {
                            for link in rows.clone().into_iter() {
                                {
                                    let key = link.id.to_string();
                                    let status = link.status(now);
                                    let variant = match status {
                                        RequestLinkStatus::Submitted => BadgeVariant::Green,
                                        RequestLinkStatus::Awaiting => BadgeVariant::Blue,
                                        RequestLinkStatus::Expired => BadgeVariant::Gray,
                                    };
                                    let expires = crate::utils::datetime::format_user_datetime(link.expires_at, None);
                                    rsx! {
                                        TableRow { key: "{key}",
                                            TableCell { span { class: "font-medium text-content", "{link.form_name}" } }
                                            TableCell { class: "text-muted", "{link.recipient_email}" }
                                            TableCell {
                                                Badge { variant, "{status.label()}" }
                                            }
                                            TableCell { class: "text-muted", "{expires}" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }

        if sending() {
            SendRequestLinkModal {
                company_id: company_id.clone(),
                company_name: company_name.clone(),
                onclose: move |_| sending.set(false),
                onsent: move |_| {
                    sending.set(false);
                    links.restart();
                },
            }
        }
    }
}

/// PMS-764: how many of the tenant's most recent links the builder shows.
///
/// Enough to answer "did that go out, and has anyone replied" at a glance,
/// short enough that the panel stays a panel rather than becoming a second
/// table competing with the definitions above it. The full history per client
/// is on the company page, which every row links to.
const RECENT_SENT: usize = 8;

/// The links the panel shows, and how many it is leaving out.
///
/// The server returns every link the tenant has issued, newest first, and this
/// takes the head of that. The remainder is returned rather than dropped
/// because a truncated list that does not say so reads as "this is everything".
fn recent_visible(rows: Vec<RequestLink>, cap: usize) -> (Vec<RequestLink>, usize) {
    let hidden = rows.len().saturating_sub(cap);
    let mut rows = rows;
    rows.truncate(cap);
    (rows, hidden)
}

/// PMS-764: what became of the forms this tenant has sent, on the page they
/// were sent from.
///
/// Until this existed, a sent link appeared in exactly one place, the company
/// detail page, and nothing on the builder led there: you sent a form, got a
/// toast, and the page you were standing on looked exactly as it had before.
/// The status data needed no new endpoint - `GET /form-request-links` takes
/// `company_id` as an OPTIONAL filter, and without it returns the tenant's
/// whole list, newest first, with each row already naming its client.
///
/// `reload` is bumped by the caller after a send so the row appears at the
/// moment it is created, which is when the question is actually being asked.
#[component]
pub fn SentRequestLinksPanel(reload: ReadSignal<u32>) -> Element {
    let links = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        // Subscribes this resource to the caller's send, without the caller
        // needing a handle on the resource itself.
        let _after_send = reload();
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Vec<RequestLink>>("/form-request-links")
            .await
            .ok()
    });

    let snap = links.read_unchecked();
    let now = Utc::now();

    rsx! {
        Card {
            title: "Recently sent".to_string(),
            // PMS-765: the card's own subtitle, in the header above the rule.
            // As the first child of the body it had no space above it (a
            // `padding: false` card gives its body none) and a table header row
            // directly below, so it read as a band of small grey text wedged
            // between two lines.
            subtitle: "Links you have emailed clients, newest first. Open a client to see everything sent to them.".to_string(),
            padding: false,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Form" }
                        TableHeader { "Client" }
                        TableHeader { "Sent to" }
                        TableHeader { "Status" }
                        TableHeader { "Expires" }
                    }
                }
                match &*snap {
                    None => rsx! { TableLoading { columns: 5, rows: 3 } },
                    Some(None) => rsx! {
                        TableEmpty { columns: 5, message: "Could not load sent forms.".to_string() }
                    },
                    Some(Some(rows)) if rows.is_empty() => rsx! {
                        TableEmpty {
                            columns: 5,
                            message: "Nothing sent yet. Use Send on a form above to email a client a link.".to_string(),
                        }
                    },
                    Some(Some(rows)) => {
                        let (visible, hidden) = recent_visible(rows.clone(), RECENT_SENT);
                        rsx! {
                            TableBody {
                                for link in visible.into_iter() {
                                    {
                                        let key = link.id.to_string();
                                        let status = link.status(now);
                                        let variant = match status {
                                            RequestLinkStatus::Submitted => BadgeVariant::Green,
                                            RequestLinkStatus::Awaiting => BadgeVariant::Blue,
                                            RequestLinkStatus::Expired => BadgeVariant::Gray,
                                        };
                                        let expires = crate::utils::datetime::format_user_datetime(link.expires_at, None);
                                        let company_id = link.company_id.to_string();
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell { span { class: "font-medium text-content", "{link.form_name}" } }
                                                TableCell {
                                                    // The way in. Everything else
                                                    // about this client, including
                                                    // their full request history,
                                                    // is one click from here.
                                                    Link {
                                                        to: crate::Route::CompanyDetail { id: company_id },
                                                        class: "underline text-accent hover:opacity-90",
                                                        "{link.company_name}"
                                                    }
                                                }
                                                TableCell { class: "text-muted", "{link.recipient_email}" }
                                                TableCell { Badge { variant, "{status.label()}" } }
                                                TableCell { class: "text-muted", "{expires}" }
                                            }
                                        }
                                    }
                                }
                            }
                            if hidden > 0 {
                                TableBody {
                                    TableRow {
                                        TableCell { colspan: 5, class: "text-xs text-muted",
                                            "{hidden} older link(s) not shown. Open a client to see their full history."
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// PMS-747: where "Add one" goes when a client has no emailable contact.
///
/// `/contacts/new` reads its prefill from the query string rather than from a
/// typed route param (`contacts.rs::read_company_prefill_from_url`), so the
/// pair of keys below is the contract: the new contact lands on this client and
/// its breadcrumb leads back to the company. A plain anchor, matching the other
/// company-scoped links in this file.
fn add_contact_href(company_id: &str, company_name: &str) -> String {
    format!(
        "/contacts/new?company_id={}&company_name={}",
        company_id,
        crate::utils::url::urlencoding_minimal(company_name)
    )
}

/// MAPPS-424: the builder reaches this after choosing a company, with the
/// definition already known, so `preselected_form_id` seeds the Form select.
/// It stays editable: preselecting is a convenience, not a lock, and an agent
/// who opened the wrong row should not have to start over.
#[component]
pub(crate) fn SendRequestLinkModal(
    company_id: String,
    company_name: String,
    #[props(default)] preselected_form_id: Option<String>,
    onclose: EventHandler<()>,
    onsent: EventHandler<()>,
) -> Element {
    let mut form_id = use_signal(|| preselected_form_id.clone().unwrap_or_default());
    let mut contact_id = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut form_error = use_signal(String::new);
    let mut email_error = use_signal(String::new);

    // Only forms a client could actually be sent: a retired definition refuses
    // submissions server-side, so offering it would issue a link that dies on
    // arrival.
    let forms = use_resource(|| async {
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Vec<FormDefinition>>("/forms?active_only=true")
            .await
            .ok()
    });
    let form_options: Vec<SelectOption> = match &*forms.read_unchecked() {
        Some(Some(list)) => list
            .iter()
            .map(|f| SelectOption::new(f.id.to_string(), f.name.clone()))
            .collect(),
        _ => Vec::new(),
    };
    let no_forms = matches!(&*forms.read_unchecked(), Some(Some(list)) if list.is_empty());

    let contacts_company = company_id.clone();
    let contacts = use_resource(move || {
        let id = contacts_company.clone();
        async move {
            let _token = crate::hooks::fetch::api::current_access_token()?;
            // MAPPS-528: every contact of the company, paged. A plain `Select`
            // is right per docs/form-conventions.md, but the old single
            // `per_page=200` ask was clamped to 100 by the server, which hid
            // the rest of a large company's contacts from the picker. This
            // deliberately does NOT reuse the detail page's contacts resource,
            // capped at 5 rows for its preview card.
            crate::hooks::fetch::api::get_all_authed::<PickerContact>(&format!(
                "/contacts/companies/{id}/contacts"
            ))
            .await
            .ok()
        }
    });
    let contact_rows: Vec<PickerContact> = match &*contacts.read_unchecked() {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    // A contact with no email cannot receive the link, so it is not offered:
    // choosing one would only produce a server 400 telling the agent to supply
    // an address they already could have typed.
    let contact_options: Vec<SelectOption> = contact_rows
        .iter()
        .filter(|c| c.email.as_deref().map(|e| !e.is_empty()).unwrap_or(false))
        .map(|c| {
            let name = format!("{} {}", c.first_name, c.last_name);
            let name = name.trim().to_string();
            let label = if name.is_empty() {
                c.email.clone().unwrap_or_default()
            } else {
                format!("{name} ({})", c.email.clone().unwrap_or_default())
            };
            SelectOption::new(c.id.to_string(), label)
        })
        .collect();

    // PMS-747: a client with no emailable contact used to get a Select holding
    // nothing but its placeholder, under help text that said only which
    // contacts are listed. True, and useless: it never said there were none,
    // and it read as "this needs a contact I have no way to create". The
    // distinction only exists once the fetch has settled; mid-flight an empty
    // list is just a list that has not arrived.
    let contacts_loaded = matches!(&*contacts.read_unchecked(), Some(Some(_)));
    let no_contacts = contacts_loaded && contact_options.is_empty();
    let add_contact_href = add_contact_href(&company_id, &company_name);

    let rows_for_pick = contact_rows.clone();
    let company_uuid = Uuid::parse_str(&company_id).ok();

    let handle_send = move |_| {
        if saving() {
            return;
        }
        form_error.set(String::new());
        email_error.set(String::new());
        error.set(String::new());
        let mut failed = false;

        let Some(company_uuid) = company_uuid else {
            error.set("This company could not be identified. Reload the page.".to_string());
            return;
        };
        let form_uuid = Uuid::parse_str(form_id.read().trim()).ok();
        if form_uuid.is_none() {
            form_error.set("Choose a form to send.".to_string());
            failed = true;
        }
        let chosen_contact = Uuid::parse_str(contact_id.read().trim()).ok();
        let typed_email = email.read().trim().to_string();
        // The server needs one of the two. A contact carries its own address,
        // so the email is only required when sending to someone who is not a
        // contact yet.
        if chosen_contact.is_none() && typed_email.is_empty() {
            email_error.set("Choose a contact or enter an email address.".to_string());
            failed = true;
        }
        if failed {
            return;
        }

        saving.set(true);
        let req = IssueRequestLinkRequest {
            form_definition_id: form_uuid.expect("checked above"),
            company_id: company_uuid,
            contact_id: chosen_contact,
            recipient_email: (!typed_email.is_empty()).then_some(typed_email),
        };

        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_authed_typed::<RequestLink, _>(
                    "/form-request-links",
                    &req,
                )
                .await
                {
                    Ok(link) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            format!("Request form sent to {}.", link.recipient_email),
                        );
                        onsent.call(());
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        // The server routes a bad address to `recipient_email`;
                        // anything else is a whole-request problem.
                        match err.field_message("recipient_email") {
                            Some(m) => email_error.set(m),
                            None => error.set(err.user_message()),
                        }
                    }
                }
            }
            saving.set(false);
        });
    };

    let can_mutate = crate::hooks::use_can_mutate();

    // MAPPS-482: what this form already knows about the message. The token,
    // its link and the tenant's own identity only exist at send time, so they
    // come back in `unresolved` and the modal shows them as filled in when
    // sent rather than pretending to a value it does not have.
    let preview_recipient = {
        let typed = email.read().trim().to_string();
        if typed.is_empty() {
            let chosen = contact_id.read().trim().to_string();
            contact_rows
                .iter()
                .find(|c| c.id.to_string() == chosen)
                .and_then(|c| c.email.clone())
                .unwrap_or_default()
        } else {
            typed
        }
    };
    let preview_form_name = {
        let chosen = form_id.read().trim().to_string();
        form_options
            .iter()
            .find(|o| o.value == chosen)
            .map(|o| o.label.clone())
            .unwrap_or_default()
    };
    let preview_context = serde_json::json!({
        "recipient_email": preview_recipient,
        "company_name": company_name.clone(),
        "form_name": preview_form_name,
    });

    let footer = rsx! {
        Button { variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), "Cancel" }
        crate::components::EmailPreview {
            event_type: "forms.request_link".to_string(),
            context: preview_context,
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: saving(),
            disabled: !can_mutate || no_forms,
            title: if no_forms {
                Some("Define a request form first".to_string())
            } else if !can_mutate {
                Some("Can't send while the server is unreachable".to_string())
            } else {
                None
            },
            onclick: handle_send,
            MailIcon { size: IconSize::Small, class: "mr-2".to_string() }
            "Send link"
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: format!("Send a request form to {company_name}"),
            size: crate::components::ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer,

            div { class: "space-y-4",
                if !error().is_empty() {
                    ErrorBanner { "{error()}" }
                }

                if no_forms {
                    ErrorBanner {
                        "No active request forms exist yet. "
                        Link {
                            to: crate::Route::FormsBuilder {},
                            class: "underline font-medium",
                            "Define one under Request Forms"
                        }
                        " first."
                    }
                }

                Select {
                    name: "form_definition_id",
                    label: "Form",
                    options: form_options,
                    value: form_id(),
                    placeholder: "Choose a form".to_string(),
                    required: true,
                    disabled: saving() || no_forms,
                    error: form_error(),
                    help: "The client is asked only for what this form defines.".to_string(),
                    onchange: move |e: FormEvent| form_id.set(e.value()),
                }

                Select {
                    name: "contact_id",
                    label: "Contact",
                    options: contact_options,
                    // PMS-747: the placeholder says what choosing it DOES. As
                    // "Someone else" it was the only entry a client with no
                    // contacts ever showed, and it explained nothing about the
                    // address field below it.
                    placeholder: "Someone else (type an address below)".to_string(),
                    value: contact_id(),
                    disabled: saving(),
                    help: "Only contacts with an email address are listed.".to_string(),
                    onchange: move |e: FormEvent| {
                        let v = e.value();
                        // Filling the address from the chosen contact keeps the
                        // agent's eye on where this is actually going, and
                        // leaves it editable for a one-off override.
                        if let Some(c) = rows_for_pick.iter().find(|c| c.id.to_string() == v) {
                            email.set(c.email.clone().unwrap_or_default());
                        } else if v.is_empty() {
                            email.set(String::new());
                        }
                        email_error.set(String::new());
                        contact_id.set(v);
                    },
                }

                // PMS-747: named as a fact about this client, with the route to
                // fix it, rather than left as an empty dropdown to interpret.
                // Not an error: the send works fine on a typed address alone.
                if no_contacts {
                    p { class: "-mt-2 text-xs text-muted",
                        "{company_name} has no contact with an email address yet. "
                        a {
                            href: "{add_contact_href}",
                            class: "underline text-accent hover:opacity-90",
                            "Add one"
                        }
                        ", or type an address below."
                    }
                }

                Input {
                    name: "recipient_email",
                    label: "Email address",
                    r#type: "email".to_string(),
                    value: email(),
                    disabled: saving(),
                    error: email_error(),
                    help: "Where the link is sent. Overrides the contact's address if both are set.".to_string(),
                    oninput: move |e: FormEvent| {
                        email_error.set(String::new());
                        email.set(e.value());
                    },
                }

                p { class: "text-xs text-muted",
                    "The link can be submitted once and expires in 7 days. Their answers arrive as a ticket for this client."
                }
            }
        }
    }
}

/// MAPPS-424: sending started from the builder instead of from a company.
///
/// The builder knows the form but not the client, which is the mirror image of
/// the company card, so this asks for the company and then hands off to
/// [`SendRequestLinkModal`] with the form preselected. Two steps rather than
/// one combined modal: a company is picked by search against every company in
/// the tenant, while the contact list is a plain bounded `Select` that cannot
/// even be populated until the company is known.
#[component]
pub fn SendFormToClientModal(
    form_definition_id: String,
    form_name: String,
    onclose: EventHandler<()>,
    onsent: EventHandler<()>,
) -> Element {
    let mut company_id = use_signal(String::new);
    let mut company_name = use_signal(String::new);
    let mut company_error = use_signal(String::new);
    let mut chosen = use_signal(|| None::<(String, String)>);

    // Company settled: the rest of the send is identical to the company-page
    // route, so it runs through the same modal rather than a second copy of
    // the contact picker and the issue call.
    if let Some((id, name)) = chosen.read().clone() {
        return rsx! {
            SendRequestLinkModal {
                company_id: id,
                company_name: name,
                preselected_form_id: Some(form_definition_id.clone()),
                onclose: move |_| onclose.call(()),
                onsent: move |_| onsent.call(()),
            }
        };
    }

    // `then_some` on the owned value: `then(|| company_id())` reads as a
    // redundant closure to clippy, and its suggested `&company_id` does not
    // compile because a `Signal` is not `FnOnce`.
    let selected_company_id = {
        let current = company_id();
        (!current.is_empty()).then_some(current)
    };

    let footer = rsx! {
        Button { variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), "Cancel" }
        Button {
            variant: ButtonVariant::Primary,
            onclick: move |_| {
                let id = company_id.read().trim().to_string();
                if id.is_empty() {
                    company_error.set("Choose the client this form is going to.".to_string());
                    return;
                }
                chosen.set(Some((id, company_name.read().clone())));
            },
            "Continue"
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: format!("Send {form_name} to a client"),
            size: crate::components::ModalSize::Small,
            onclose: move |_| onclose.call(()),
            footer,

            div { class: "space-y-4",
                crate::components::CompanyPicker {
                    value: company_name(),
                    selected_id: selected_company_id,
                    label: "Client".to_string(),
                    required: true,
                    error: company_error(),
                    onselect: move |(id, name): (String, String)| {
                        company_error.set(String::new());
                        company_id.set(id);
                        company_name.set(name);
                    },
                    onclear: move |_| {
                        company_id.set(String::new());
                        company_name.set(String::new());
                    },
                }

                p { class: "text-xs text-muted",
                    "The link is emailed to someone at this client. You choose who, and see what has already been sent, on their company page."
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn link(used: bool, expires_in: Duration) -> RequestLink {
        RequestLink {
            id: Uuid::nil(),
            form_definition_id: Uuid::nil(),
            form_name: "New starter".into(),
            company_id: Uuid::nil(),
            company_name: "Acme".into(),
            contact_id: None,
            recipient_email: "client@example.com".into(),
            expires_at: Utc::now() + expires_in,
            used_at: used.then(Utc::now),
            submission_id: None,
        }
    }

    /// PMS-747: the prefill keys are what makes "Add one" land on THIS client
    /// rather than on a blank contact form, so they are asserted rather than
    /// left to drift against `contacts.rs`.
    #[test]
    fn add_contact_carries_the_client_it_was_offered_from() {
        let href = add_contact_href("11111111-1111-1111-1111-111111111111", "Acme & Co");
        assert_eq!(
            href,
            "/contacts/new?company_id=11111111-1111-1111-1111-111111111111&company_name=Acme%20%26%20Co",
            "an unencoded `&` in the name would truncate the query and drop the prefill"
        );
    }

    /// PMS-764: the panel shows the newest few and says how many it is not
    /// showing. A truncated list that keeps quiet about it reads as "this is
    /// everything", which is exactly the wrong impression for a page whose
    /// whole job is telling you what has been sent.
    #[test]
    fn the_panel_admits_what_it_is_not_showing() {
        let rows: Vec<RequestLink> = (0..11).map(|_| link(false, Duration::days(3))).collect();
        let (visible, hidden) = recent_visible(rows, RECENT_SENT);
        assert_eq!(visible.len(), RECENT_SENT);
        assert_eq!(hidden, 3, "the rest are counted, not dropped silently");
    }

    #[test]
    fn a_short_list_is_shown_whole_and_claims_nothing_hidden() {
        let rows: Vec<RequestLink> = (0..3).map(|_| link(false, Duration::days(3))).collect();
        let (visible, hidden) = recent_visible(rows, RECENT_SENT);
        assert_eq!(visible.len(), 3);
        assert_eq!(hidden, 0);

        let (visible, hidden) = recent_visible(Vec::new(), RECENT_SENT);
        assert!(visible.is_empty());
        assert_eq!(hidden, 0, "an empty list has nothing hidden behind it");
    }

    /// The server returns the tenant's links newest first; the panel must not
    /// reorder them, or "recently sent" stops meaning recently sent.
    #[test]
    fn the_server_ordering_is_kept() {
        let mut newest = link(false, Duration::days(5));
        newest.form_name = "Newest".into();
        let mut oldest = link(true, Duration::days(1));
        oldest.form_name = "Oldest".into();
        let (visible, _) = recent_visible(vec![newest, oldest], RECENT_SENT);
        assert_eq!(visible[0].form_name, "Newest");
        assert_eq!(visible[1].form_name, "Oldest");
    }

    #[test]
    fn a_live_unused_link_is_awaiting() {
        assert_eq!(
            link(false, Duration::days(3)).status(Utc::now()),
            RequestLinkStatus::Awaiting
        );
    }

    #[test]
    fn an_unused_past_expiry_link_is_expired() {
        assert_eq!(
            link(false, Duration::days(-1)).status(Utc::now()),
            RequestLinkStatus::Expired
        );
    }

    #[test]
    fn submitted_wins_over_expired() {
        // A link that was used and has since passed its expiry is still a
        // request that came in. Reporting it as expired would read as though
        // the client never replied.
        assert_eq!(
            link(true, Duration::days(-1)).status(Utc::now()),
            RequestLinkStatus::Submitted
        );
    }
}

//! Where a contact came from, on the contact list and detail (MAPPS-811,
//! PSA-70 C, H, I, J, K).
//!
//! * [`ProvenanceBadge`] marks an imported contact on the list row and in the
//!   detail header, from `ContactResponse.imported_from` (PMS-1260), so an
//!   imported contact is told apart from a hand-entered one at a glance.
//! * [`ImportCard`] is the detail page's account of the import, from
//!   `GET /contacts/contacts/{id}/sync` (PMS-1214): which account, when it
//!   last synced, whether Google still has it, every locked field with who
//!   locked it and a per-field release, Unlink, and - for an admin - removal
//!   of the person's imported data.
//! * [`LockMarker`] sits beside each field on the detail page that a Mokosh
//!   edit has locked, so the lock is visible where the value is.
//!
//! A deletion in Google is a STATE here ("Deleted in Google"), never a
//! removal: the contact is kept (PSA-70 I).
//!
//! MAPPS-916: a contact can also come from an uploaded vCard file (PMS-1290,
//! provider `vcard`). A file is imported once rather than synced, so its card
//! says which file, who uploaded it and when, instead of an account and a last
//! sync, and every sentence that used to say "Google" names the source it is
//! about.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ConfirmDialog, ErrorBanner, Input,
};
use crate::Route;

/// `ContactResponse.imported_from` (PMS-1260).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ImportedFrom {
    pub provider: String,
    pub account_email: String,
    #[serde(default)]
    pub linked: bool,
    #[serde(default)]
    pub deleted_in_source: bool,
}

/// `GET /contacts/contacts/{id}/sync` (PMS-1214).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Provenance {
    #[serde(default)]
    pub links: Vec<ProvenanceLink>,
    #[serde(default)]
    pub locks: Vec<FieldLock>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ProvenanceLink {
    pub provider: String,
    pub source_account_email: String,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub last_synced_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_in_source_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub unlinked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub unlink_reason: Option<String>,
    #[serde(default)]
    pub suggested_company_id: Option<Uuid>,
    #[serde(default)]
    pub suggested_company_name: Option<String>,
    /// PMS-1290: the upload a `vcard` link came from.
    #[serde(default)]
    pub import_file_name: Option<String>,
    #[serde(default)]
    pub import_file_uploaded_by_name: Option<String>,
    #[serde(default)]
    pub import_file_uploaded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FieldLock {
    pub field: String,
    #[serde(default)]
    pub locked_by_name: Option<String>,
    pub locked_at: DateTime<Utc>,
}

impl Provenance {
    /// The link the card describes: the live one, else the most recent (the
    /// server answers live first).
    pub fn current(&self) -> Option<&ProvenanceLink> {
        self.links.first()
    }

    pub fn is_locked(&self, field: &str) -> bool {
        self.locks.iter().any(|l| l.field == field)
    }
}

/// A provider's name as people know it.
pub fn provider_name(provider: &str) -> &str {
    match provider {
        "google" => "Google",
        "vcard" => "vCard",
        other => other,
    }
}

/// An uploaded file rather than a synced directory (PMS-1290): imported once,
/// never synced, never "deleted there".
pub fn is_file(provider: &str) -> bool {
    provider == "vcard"
}

/// What a source does to an unlocked field later, in words: a directory syncs
/// it, a file only changes it if someone imports a file again.
fn later_update(provider: &str) -> String {
    if is_file(provider) {
        "a later vCard import".to_string()
    } else {
        format!("the next {} sync", provider_name(provider))
    }
}

/// A lockable field's label (`contact_sync::sync::fields`).
pub fn field_label(field: &str) -> &str {
    match field {
        "first_name" => "First name",
        "last_name" => "Last name",
        "email" => "Email",
        "title" => "Title",
        "department" => "Department",
        "company_name" => "Company",
        "phones" => "Phone numbers",
        "tags" => "Tags",
        "notes" => "Notes",
        other => other,
    }
}

/// The state an imported contact is in, as words and a badge colour.
pub fn link_state(link: &ProvenanceLink) -> (&'static str, BadgeVariant) {
    if link.deleted_in_source_at.is_some() {
        ("Deleted in Google, kept here", BadgeVariant::Orange)
    } else if link.unlinked_at.is_some() {
        match link.unlink_reason.as_deref() {
            Some("disconnected") => (
                "Integration disconnected, kept as a local record",
                BadgeVariant::Gray,
            ),
            _ => ("Unlinked, kept as a local record", BadgeVariant::Gray),
        }
    } else if is_file(&link.provider) {
        ("Imported from a file", BadgeVariant::Blue)
    } else {
        ("Synced", BadgeVariant::Green)
    }
}

/// The badge's words for a list row.
pub fn badge_text(origin: &ImportedFrom) -> String {
    let provider = provider_name(&origin.provider);
    if origin.deleted_in_source {
        format!("{provider}: deleted there")
    } else if origin.linked {
        provider.to_string()
    } else {
        format!("From {provider}")
    }
}

/// On the list row and the detail header: an imported contact at a glance.
#[component]
pub fn ProvenanceBadge(origin: Option<ImportedFrom>) -> Element {
    let Some(origin) = origin else {
        return rsx! {};
    };
    let variant = if origin.deleted_in_source {
        BadgeVariant::Orange
    } else if origin.linked {
        BadgeVariant::Blue
    } else {
        BadgeVariant::Gray
    };
    let text = badge_text(&origin);
    let explained = if is_file(&origin.provider) {
        format!("Imported from the vCard file {}", origin.account_email)
    } else {
        format!(
            "Imported from {} ({})",
            provider_name(&origin.provider),
            origin.account_email
        )
    };
    rsx! {
        span { title: "{explained}", class: "ml-2 align-middle",
            Badge { variant, "{text}" }
            span { class: "sr-only", ", {explained}" }
        }
    }
}

/// Beside a field value on the detail page: this field is locked.
#[component]
pub fn LockMarker(locked: bool) -> Element {
    if !locked {
        return rsx! {};
    }
    rsx! {
        span {
            class: "ml-2 inline-flex items-center gap-1 text-xs text-muted",
            title: "Edited in Mokosh, so the import no longer changes it",
            svg {
                "aria-hidden": "true",
                class: "w-3.5 h-3.5",
                xmlns: "http://www.w3.org/2000/svg",
                fill: "none",
                view_box: "0 0 24 24",
                stroke_width: "1.5",
                stroke: "currentColor",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    d: "M16.5 10.5V6.75a4.5 4.5 0 1 0-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 0 0 2.25-2.25v-6.75a2.25 2.25 0 0 0-2.25-2.25H6.75a2.25 2.25 0 0 0-2.25 2.25v6.75a2.25 2.25 0 0 0 2.25 2.25Z",
                }
            }
            "Locked"
        }
    }
}

#[derive(Serialize)]
struct RemoveBody {
    reason: String,
}

#[derive(Deserialize)]
struct RemoveResult {
    #[serde(default)]
    contact_deleted: bool,
}

/// The detail page's account of the import. Renders nothing for a contact
/// that was never imported.
#[component]
pub fn ImportCard(
    contact_id: String,
    provenance: Option<Provenance>,
    on_change: EventHandler<()>,
) -> Element {
    let is_admin = crate::pages::settings::use_is_admin();
    let can_mutate = crate::hooks::use_can_mutate();
    let navigator = use_navigator();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut confirm_unlink = use_signal(|| false);
    let mut confirm_remove = use_signal(|| false);
    let mut reason = use_signal(String::new);
    let mut confirm_release: Signal<Option<String>> = use_signal(|| None);
    let Some(provenance) = provenance else {
        return rsx! {};
    };
    let Some(link) = provenance.current().cloned() else {
        return rsx! {};
    };

    let (state, tone) = link_state(&link);
    let live = link.unlinked_at.is_none();
    let from_file = is_file(&link.provider);
    let provider = provider_name(&link.provider).to_string();
    let later = later_update(&link.provider);
    let card_title = if from_file {
        "vCard import".to_string()
    } else {
        format!("{provider} Contacts")
    };
    let file_name = link
        .import_file_name
        .clone()
        .unwrap_or_else(|| link.source_account_email.clone());
    let imported_by = match (
        link.import_file_uploaded_by_name.clone(),
        link.import_file_uploaded_at,
    ) {
        (Some(who), Some(at)) => {
            format!("{who}, {}", crate::utils::datetime::fmt_datetime_pref(at))
        }
        (None, Some(at)) => crate::utils::datetime::fmt_datetime_pref(at),
        (Some(who), None) => who,
        (None, None) => "Not recorded".to_string(),
    };
    let last_synced = link
        .last_synced_at
        .map(crate::utils::datetime::fmt_datetime_pref)
        .unwrap_or_else(|| "Not yet".to_string());
    let origin = match link.origin.as_deref() {
        Some("created") => "Created by the import",
        Some("linked") => "Already in Mokosh, linked by the import",
        _ => "Imported",
    };
    let disabled = busy() || !can_mutate;

    let release = {
        let contact_id = contact_id.clone();
        move |field: String| {
            let path = format!("/contacts/contacts/{contact_id}/sync/locks/{field}");
            busy.set(true);
            error.set(String::new());
            spawn(async move {
                #[cfg(feature = "app")]
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => on_change.call(()),
                    Err(e) => error.set(format!("Could not release the lock: {e}")),
                }
                #[cfg(not(feature = "app"))]
                let _ = path;
                busy.set(false);
            });
        }
    };
    let unlink = {
        let path = format!("/contacts/contacts/{contact_id}/sync/unlink");
        move || {
            let path = path.clone();
            busy.set(true);
            error.set(String::new());
            spawn(async move {
                #[cfg(feature = "app")]
                match crate::hooks::fetch::api::post_authed_no_content(&path).await {
                    Ok(()) => on_change.call(()),
                    Err(e) => error.set(format!("Could not unlink: {e}")),
                }
                #[cfg(not(feature = "app"))]
                let _ = path;
                busy.set(false);
            });
        }
    };
    let remove = {
        let path = format!("/contacts/contacts/{contact_id}/sync/remove-imported-data");
        move || {
            let path = path.clone();
            let body = RemoveBody { reason: reason() };
            busy.set(true);
            error.set(String::new());
            spawn(async move {
                #[cfg(feature = "app")]
                match crate::hooks::fetch::api::post_authed::<RemoveResult, _>(&path, &body).await {
                    Ok(result) if result.contact_deleted => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "The imported contact and its data were removed.",
                        );
                        navigator.push(Route::ContactList {});
                    }
                    Ok(_) => {
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "The imported data was removed. The contact was already in Mokosh and is kept.",
                        );
                        on_change.call(());
                    }
                    Err(e) => error.set(format!("Could not remove the imported data: {e}")),
                }
                #[cfg(not(feature = "app"))]
                let _ = (path, body, navigator);
                busy.set(false);
            });
        }
    };

    rsx! {
        Card { title: "{card_title}",
            div { class: "space-y-4 text-sm",
                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                div { class: "flex flex-wrap items-center gap-2",
                    Badge { variant: tone, "{state}" }
                }
                dl { class: "space-y-3",
                    if from_file {
                        div {
                            dt { class: "text-muted", "File" }
                            dd { class: "mt-1 text-content break-words", "{file_name}" }
                        }
                        div {
                            dt { class: "text-muted", "Uploaded by" }
                            dd { class: "mt-1 text-content", "{imported_by}" }
                        }
                    } else {
                        div {
                            dt { class: "text-muted", "Account" }
                            dd { class: "mt-1 text-content break-words", "{link.source_account_email}" }
                        }
                    }
                    div {
                        dt { class: "text-muted", "Origin" }
                        dd { class: "mt-1 text-content", "{origin}" }
                    }
                    if !from_file {
                        div {
                            dt { class: "text-muted", "Last synced" }
                            dd { class: "mt-1 text-content", "{last_synced}" }
                        }
                    }
                    if let (Some(id), Some(name)) = (link.suggested_company_id, link.suggested_company_name.clone()) {
                        div {
                            dt { class: "text-muted", "Suggested company" }
                            dd { class: "mt-1",
                                Link {
                                    to: Route::CompanyDetail { id: id.to_string() },
                                    class: "text-accent hover:opacity-90",
                                    "{name}"
                                }
                                p { class: "text-xs text-subtle",
                                    "The organisation named in {provider} matches this company. It is not linked until someone links it."
                                }
                            }
                        }
                    }
                }
                if !provenance.locks.is_empty() {
                    div { class: "border-t border-line pt-4",
                        h4 { class: "font-medium text-content", "Locked fields" }
                        p { class: "mt-1 text-muted",
                            "Someone edited these in Mokosh, so the import leaves them alone. Release one to let {later} update it again."
                        }
                        ul { class: "mt-3 space-y-2",
                            for lock in provenance.locks.clone() {
                                {
                                    let field = lock.field.clone();
                                    let label = field_label(&lock.field).to_string();
                                    let who = lock.locked_by_name.clone().unwrap_or_else(|| "someone".to_string());
                                    let when = crate::utils::datetime::fmt_datetime_pref(lock.locked_at);
                                    rsx! {
                                        li { key: "{lock.field}", class: "flex flex-wrap items-center justify-between gap-2",
                                            span { class: "text-content",
                                                "{label}"
                                                span { class: "ml-2 text-xs text-subtle", "edited by {who}, {when}" }
                                            }
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                size: ButtonSize::Small,
                                                disabled,
                                                aria_label: format!("Release the lock on {label}"),
                                                onclick: move |_| confirm_release.set(Some(field.clone())),
                                                "Release"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap gap-3 border-t border-line pt-4",
                    if live {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            disabled,
                            onclick: move |_| confirm_unlink.set(true),
                            data_testid: "contact-unlink",
                            "Unlink from {provider}"
                        }
                    }
                    if is_admin {
                        Button {
                            variant: ButtonVariant::Danger,
                            size: ButtonSize::Small,
                            disabled,
                            onclick: move |_| {
                                reason.set(String::new());
                                confirm_remove.set(true);
                            },
                            data_testid: "contact-remove-imported",
                            "Remove imported data"
                        }
                    }
                }
            }
        }
        ConfirmDialog {
            open: confirm_release().is_some(),
            title: "Release this lock?",
            message: release_message(confirm_release().as_deref().unwrap_or_default(), &link.provider),
            confirm_text: "Release",
            loading: busy(),
            onconfirm: move |_| {
                if let Some(field) = confirm_release() {
                    confirm_release.set(None);
                    let mut release = release.clone();
                    release(field);
                }
            },
            oncancel: move |_| confirm_release.set(None),
        }
        ConfirmDialog {
            open: confirm_unlink(),
            title: "Unlink from {provider}?",
            message: "This contact stays exactly as it is, as a local record. It will not be updated by {later}, and the import will not link it again.",
            confirm_text: "Unlink",
            loading: busy(),
            onconfirm: move |_| {
                confirm_unlink.set(false);
                let mut unlink = unlink.clone();
                unlink();
            },
            oncancel: move |_| confirm_unlink.set(false),
        }
        ConfirmDialog {
            open: confirm_remove(),
            title: "Remove this person's imported data?",
            message: remove_message(link.origin.as_deref(), &link.provider),
            confirm_text: "Remove",
            destructive: true,
            loading: busy(),
            blocked: false,
            body: rsx! {
                Input {
                    name: "removal_reason",
                    label: "Who asked, or why",
                    value: reason(),
                    required: true,
                    oninput: move |e: FormEvent| reason.set(e.value()),
                }
            },
            onconfirm: move |_| {
                if reason().trim().is_empty() {
                    return;
                }
                confirm_remove.set(false);
                let mut remove = remove.clone();
                remove();
            },
            oncancel: move |_| confirm_remove.set(false),
        }
    }
}

/// What releasing a lock lets happen, stated first: the edit it protects can
/// be overwritten.
pub fn release_message(field: &str, provider: &str) -> String {
    let name = provider_name(provider);
    format!(
        "{} will follow {name} again: {} may replace the value someone typed here with the one in {name}.",
        field_label(field),
        later_update(provider)
    )
}

/// What removal will do, stated before it happens (PSA-70 K).
pub fn remove_message(origin: Option<&str>, provider: &str) -> String {
    let what = match origin {
        Some("created") => "The import created this contact, so the contact is deleted.",
        _ => "This contact was already in Mokosh, so it is kept; only what the import attached is removed.",
    };
    format!(
        "{what} Its link to {} and anything the review queue holds about it are removed, and it will not be imported again. The audit log records that it happened and why, not the removed details. If tickets or invoices refer to it, nothing is removed.",
        provider_name(provider)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(json: serde_json::Value) -> ProvenanceLink {
        let mut base = serde_json::json!({
            "provider": "google",
            "source_account_email": "ops@msp.example",
        });
        for (k, v) in json.as_object().unwrap() {
            base[k] = v.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    /// A deletion in Google is a state, never a removal, and it outranks
    /// the rest.
    #[test]
    fn a_deletion_in_google_is_a_state() {
        let deleted = link(serde_json::json!({"deleted_in_source_at": "2026-09-16T00:00:00Z"}));
        assert_eq!(link_state(&deleted).0, "Deleted in Google, kept here");
        let unlinked = link(
            serde_json::json!({"unlinked_at": "2026-09-16T00:00:00Z", "unlink_reason": "unlinked"}),
        );
        assert_eq!(link_state(&unlinked).0, "Unlinked, kept as a local record");
        let disconnected = link(
            serde_json::json!({"unlinked_at": "2026-09-16T00:00:00Z", "unlink_reason": "disconnected"}),
        );
        assert!(link_state(&disconnected).0.contains("disconnected"));
        assert_eq!(link_state(&link(serde_json::json!({}))).0, "Synced");
    }

    #[test]
    fn the_badge_says_where_and_whether_still_synced() {
        let mut origin = ImportedFrom {
            provider: "google".into(),
            account_email: "ops@msp.example".into(),
            linked: true,
            deleted_in_source: false,
        };
        assert_eq!(badge_text(&origin), "Google");
        origin.linked = false;
        assert_eq!(badge_text(&origin), "From Google");
        origin.deleted_in_source = true;
        assert_eq!(badge_text(&origin), "Google: deleted there");
    }

    /// Every field the sync can lock has a label a person recognises.
    #[test]
    fn every_lockable_field_has_a_label() {
        for field in [
            "first_name",
            "last_name",
            "email",
            "title",
            "department",
            "company_name",
            "phones",
            "tags",
            "notes",
        ] {
            assert_ne!(field_label(field), field, "{field} has no label");
        }
    }

    #[test]
    fn the_removal_message_depends_on_whether_the_import_created_it() {
        assert!(remove_message(Some("created"), "google").contains("the contact is deleted"));
        assert!(remove_message(Some("linked"), "google").contains("it is kept"));
        assert!(
            remove_message(None, "google").contains("it is kept"),
            "unknown origin keeps the contact, as the server does"
        );
    }

    /// MAPPS-916: a file import is an import, not a sync, and every sentence
    /// names the source it is about rather than Google.
    #[test]
    fn a_vcard_link_reads_as_a_file_import() {
        let file = link(serde_json::json!({
            "provider": "vcard",
            "source_account_email": "Office contacts.vcf",
            "import_file_name": "Office contacts.vcf",
            "import_file_uploaded_by_name": "Ada Admin",
            "import_file_uploaded_at": "2026-09-21T10:00:00Z",
        }));
        assert_eq!(link_state(&file).0, "Imported from a file");
        assert_eq!(
            file.import_file_uploaded_by_name.as_deref(),
            Some("Ada Admin")
        );
        let origin = ImportedFrom {
            provider: "vcard".into(),
            account_email: "Office contacts.vcf".into(),
            linked: true,
            deleted_in_source: false,
        };
        assert_eq!(badge_text(&origin), "vCard");
        let release = release_message("title", "vcard");
        assert!(release.contains("a later vCard import"), "{release}");
        assert!(!release.contains("sync"), "{release}");
        assert!(remove_message(Some("created"), "vcard").contains("link to vCard"));
        assert!(!remove_message(Some("created"), "vcard").contains("Google"));
        assert!(release_message("title", "google").contains("the next Google sync"));
    }

    #[test]
    fn locks_are_found_by_field() {
        let p: Provenance = serde_json::from_value(serde_json::json!({
            "links": [],
            "locks": [{"field": "title", "locked_at": "2026-09-16T00:00:00Z"}],
        }))
        .unwrap();
        assert!(p.is_locked("title"));
        assert!(!p.is_locked("email"));
    }

    /// Removal is admin only, asks for a reason, and sends a typed body.
    #[test]
    fn removal_is_admin_only_and_asks_why() {
        let src = include_str!("contact_provenance.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if is_admin {"));
        assert!(head.contains("if reason().trim().is_empty() {"));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
    }
}

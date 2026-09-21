//! Contacts > Import review (MAPPS-810, PSA-70 D).
//!
//! Every Google contact the import was not sure about waits here for a person.
//! Nothing on this page merges anything by itself: each record is answered
//! with Link (to the candidate chosen), Create new, or Skip, and each answer
//! goes to `POST /integrations/contact-sync/review-queue/resolve` (PMS-1215).
//!
//! # Side by side, with the reason called out
//!
//! Each candidate is compared with the Google record field by field, and the
//! field that made the import ask (the shared email, the shared phone number,
//! or the name and company together) is marked in text as well as colour, so
//! the reason survives a screen reader and a colour-blind reviewer.
//!
//! # The one-to-many cases are shown, not hidden
//!
//! One Google record proposing two Mokosh contacts is two candidates on one
//! card. One Mokosh contact proposed for two Google records is marked on each
//! of them ("also proposed for 1 other Google contact"), and a candidate that
//! is already linked to a different Google record says so, because linking a
//! second record to it merges two address-book entries into one CRM contact.
//!
//! # Without a mouse
//!
//! The candidates of a record are a native radio group, so Tab reaches the
//! group and the arrow keys choose within it. While focus is inside a record,
//! `L` links it to the chosen candidate, `C` creates a new contact and `S`
//! skips it; the buttons carry the same keys in `aria-keyshortcuts` and in
//! their labels. Skip is the one answer that cannot be taken back (the record
//! is never offered again), so it asks first. After every answer the result is
//! announced in a polite live region and focus moves to the next record's
//! heading, so a queue can be cleared with the keyboard alone.

use std::collections::{BTreeMap, HashMap};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, EmptyState,
    ErrorBanner, PageHeader,
};
use crate::Route;

const QUEUE_PATH: &str = "/integrations/contact-sync/review-queue";
const RESOLVE_PATH: &str = "/integrations/contact-sync/review-queue/resolve";

/// One record waiting on a reviewer (`ReviewItem`, PMS-1215).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ReviewItem {
    /// MAPPS-916: the source the record waits in (PMS-1290). The queue spans
    /// Google and uploaded vCard files, and an answer names it back.
    #[serde(default)]
    pub connection_id: Option<Uuid>,
    /// `google` or `vcard`.
    #[serde(default)]
    pub provider: String,
    /// The Google account, or the uploaded file's name.
    #[serde(default)]
    pub source_label: String,
    pub external_id: String,
    #[serde(default)]
    pub source: SourceSnapshot,
    #[serde(default)]
    pub candidates: Vec<Candidate>,
}

impl ReviewItem {
    /// One key per record per source: the same external id could wait in two.
    pub fn key(&self) -> String {
        match self.connection_id {
            Some(id) => format!("{id}:{}", self.external_id),
            None => self.external_id.clone(),
        }
    }

    pub fn from_file(&self) -> bool {
        self.provider == "vcard"
    }

    /// The source's column heading and the word for it in a sentence.
    pub fn source_word(&self) -> &'static str {
        if self.from_file() {
            "File"
        } else {
            "Google"
        }
    }

    /// What the card heading calls the record.
    pub fn kind(&self) -> String {
        if self.from_file() {
            format!("Card from {}", self.source_label)
        } else {
            "Google contact".to_string()
        }
    }
}

/// The Google record as the import saw it (`source_snapshot`, PMS-1213).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SourceSnapshot {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub emails: Vec<String>,
    #[serde(default)]
    pub phones: Vec<SnapshotPhone>,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SnapshotPhone {
    #[serde(default)]
    pub number: String,
}

/// A Mokosh contact the record might be.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Candidate {
    pub contact_id: Uuid,
    pub match_reason: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub phones: Vec<String>,
    /// Linked to a different Google record already.
    #[serde(default)]
    pub already_linked: bool,
}

fn full_name(first: &str, last: &str) -> String {
    let name = format!("{first} {last}");
    let name = name.trim();
    if name.is_empty() {
        "(no name)".to_string()
    } else {
        name.to_string()
    }
}

impl SourceSnapshot {
    pub fn name(&self) -> String {
        let name = full_name(&self.first_name, &self.last_name);
        if name == "(no name)" {
            self.display_name.clone().unwrap_or(name)
        } else {
            name
        }
    }
}

impl Candidate {
    pub fn name(&self) -> String {
        full_name(&self.first_name, &self.last_name)
    }
}

/// The fields a comparison shows, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Name,
    Email,
    Phone,
    Company,
    Title,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Company => "Company",
            Self::Title => "Title",
        }
    }
}

/// Which fields made the import ask, for a server `match_reason`.
pub fn matched_fields(reason: &str) -> &'static [Field] {
    match reason {
        "email_ambiguous" => &[Field::Email],
        "phone" => &[Field::Phone],
        "name_company" => &[Field::Name, Field::Company],
        _ => &[],
    }
}

/// Why the import asked, in words.
pub fn reason_text(reason: &str) -> &'static str {
    match reason {
        "email_ambiguous" => "Same email address, but more than one contact could be this person",
        "phone" => "Same phone number",
        "name_company" => "Same name at the same company",
        _ => "Possible match",
    }
}

/// One row of a comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonRow {
    pub field: Field,
    pub google: String,
    pub mokosh: String,
    pub matched: bool,
}

fn or_not_set(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("Not set")
        .to_string()
}

fn joined(values: &[String]) -> String {
    let joined = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        "Not set".to_string()
    } else {
        joined
    }
}

/// The Google record beside one candidate.
pub fn comparison(source: &SourceSnapshot, candidate: &Candidate) -> Vec<ComparisonRow> {
    let matched = matched_fields(&candidate.match_reason);
    let google_emails = if source.emails.is_empty() {
        or_not_set(source.email.as_deref())
    } else {
        joined(&source.emails)
    };
    let google_phones: Vec<String> = source.phones.iter().map(|p| p.number.clone()).collect();
    [
        (Field::Name, source.name(), candidate.name()),
        (
            Field::Email,
            google_emails,
            or_not_set(candidate.email.as_deref()),
        ),
        (
            Field::Phone,
            joined(&google_phones),
            joined(&candidate.phones),
        ),
        (
            Field::Company,
            or_not_set(source.company_name.as_deref()),
            or_not_set(candidate.company_name.as_deref()),
        ),
        (
            Field::Title,
            or_not_set(source.title.as_deref()),
            or_not_set(candidate.title.as_deref()),
        ),
    ]
    .into_iter()
    .map(|(field, google, mokosh)| ComparisonRow {
        field,
        google,
        mokosh,
        matched: matched.contains(&field),
    })
    .collect()
}

/// How many records in the queue propose each Mokosh contact. More than one
/// is the "two Google contacts, one Mokosh contact" case.
pub fn proposals_per_contact(items: &[ReviewItem]) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    for item in items {
        for candidate in &item.candidates {
            *counts.entry(candidate.contact_id).or_insert(0) += 1;
        }
    }
    counts
}

/// A reviewer's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Link,
    Create,
    Skip,
}

/// The shortcut for a key pressed inside a record. Plain letters only: a
/// modifier means the key was meant for the browser or the system.
pub fn shortcut(key: &str, modified: bool) -> Option<Decision> {
    if modified {
        return None;
    }
    match key {
        "l" | "L" => Some(Decision::Link),
        "c" | "C" => Some(Decision::Create),
        "s" | "S" => Some(Decision::Skip),
        _ => None,
    }
}

/// What the live region says after an answer lands.
pub fn announcement(
    decision: Decision,
    source: &str,
    target: Option<&str>,
    remaining: usize,
) -> String {
    let done = match (decision, target) {
        (Decision::Link, Some(target)) => format!("Linked {source} to {target}."),
        (Decision::Link, None) => format!("Linked {source}."),
        (Decision::Create, _) => format!("Created a new contact for {source}."),
        (Decision::Skip, _) => format!("Skipped {source}; it will not be imported."),
    };
    let left = match remaining {
        0 => "Nothing left to review.".to_string(),
        1 => "1 left to review.".to_string(),
        n => format!("{n} left to review."),
    };
    format!("{done} {left}")
}

/// `POST .../resolve`. Typed rather than a `json!` literal (MAPPS-685).
#[derive(Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ResolveBody {
    Link {
        external_id: String,
        contact_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        connection_id: Option<Uuid>,
    },
    Create {
        external_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        connection_id: Option<Uuid>,
    },
    Skip {
        external_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        connection_id: Option<Uuid>,
    },
}

fn heading_id(index: usize) -> String {
    format!("import-review-{index}")
}

#[component]
pub fn ContactImportReviewPage() -> Element {
    use_page_title("Import review");
    let queue = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Vec<ReviewItem>>(QUEUE_PATH)
            .await
            .map_err(|e| {
                tracing::error!("import review queue load failed: {e}");
                e
            })
    });
    // The queue as the reviewer works through it: answered records leave it
    // here straight away rather than after a reload.
    let mut items: Signal<Option<Vec<ReviewItem>>> = use_signal(|| None);
    use_effect(move || {
        if let Some(Ok(loaded)) = queue.read().as_ref() {
            items.set(Some(loaded.clone()));
        }
    });
    // The chosen candidate per record; the first one until somebody picks.
    let mut chosen: Signal<BTreeMap<String, Uuid>> = use_signal(BTreeMap::new);
    let mut announced = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut confirm_skip: Signal<Option<usize>> = use_signal(|| None);
    let can_mutate = crate::hooks::use_can_mutate();

    let mut answer = move |index: usize, decision: Decision| {
        let Some(list) = items() else { return };
        let Some(item) = list.get(index).cloned() else {
            return;
        };
        let candidate = chosen
            .peek()
            .get(&item.key())
            .copied()
            .or_else(|| item.candidates.first().map(|c| c.contact_id));
        let target_name = candidate.and_then(|id| {
            item.candidates
                .iter()
                .find(|c| c.contact_id == id)
                .map(Candidate::name)
        });
        let body = match decision {
            Decision::Link => match candidate {
                Some(contact_id) => ResolveBody::Link {
                    external_id: item.external_id.clone(),
                    contact_id,
                    connection_id: item.connection_id,
                },
                None => return,
            },
            Decision::Create => ResolveBody::Create {
                external_id: item.external_id.clone(),
                connection_id: item.connection_id,
            },
            Decision::Skip => ResolveBody::Skip {
                external_id: item.external_id.clone(),
                connection_id: item.connection_id,
            },
        };
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    RESOLVE_PATH,
                    &body,
                )
                .await;
                match result {
                    Ok(_) => {
                        let mut next = items().unwrap_or_default();
                        next.retain(|i| i.key() != item.key());
                        let remaining = next.len();
                        announced.set(announcement(
                            decision,
                            &item.source.name(),
                            target_name.as_deref(),
                            remaining,
                        ));
                        items.set(Some(next));
                        // The record that slid into this position, else the
                        // one before it, else the page heading.
                        let focus_index = index.min(remaining.saturating_sub(1));
                        let target = if remaining == 0 {
                            "import-review-title".to_string()
                        } else {
                            heading_id(focus_index)
                        };
                        crate::platform::dom::focus_by_id(&target);
                    }
                    Err(e) => error.set(format!("Could not answer {}: {e}", item.source.name())),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (body, target_name, index, decision);
            busy.set(false);
        });
    };

    let loaded = queue.read_unchecked().clone();
    let list = items();

    rsx! {
        PageHeader {
            title: "Import review",
            subtitle: "Google contacts the import was not sure about. Nothing is merged until you decide.",
            breadcrumbs: rsx! {
                nav { "aria-label": "Breadcrumb", class: "text-sm text-muted",
                    Link { to: Route::ContactList {}, class: "hover:underline", "Contacts" }
                    " / Import review"
                }
            },
        }
        h2 {
            id: "import-review-title",
            class: "sr-only",
            tabindex: "-1",
            "Records waiting for review"
        }
        p {
            class: "sr-only",
            role: "status",
            "aria-live": "polite",
            "{announced}"
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match (loaded, list) {
            (None, _) | (Some(Ok(_)), None) => rsx! { crate::components::DetailSkeleton {} },
            (Some(Err(e)), _) => rsx! {
                Card { ErrorBanner { "Could not load the review queue: {e}" } }
            },
            (Some(Ok(_)), Some(list)) if list.is_empty() => rsx! {
                Card {
                    EmptyState {
                        title: "Nothing to review",
                        description: "When an import finds a Google contact that might already be in Mokosh, it waits here for a person to decide. Exact email matches link by themselves; nothing else is merged without you.",
                    }
                    if !announced().is_empty() {
                        p { class: "mt-2 text-center text-sm text-muted", "{announced}" }
                    }
                }
            },
            (Some(Ok(_)), Some(list)) => {
                let shared = proposals_per_contact(&list);
                let total = list.len();
                rsx! {
                    p { class: "mb-4 text-sm text-muted",
                        "{total} waiting. Tab moves between records, arrow keys choose a candidate, and L, C or S answers the record you are in."
                    }
                    div { class: "space-y-6",
                        for (index, item) in list.into_iter().enumerate() {
                            ReviewCard {
                                key: "{item.key()}",
                                index,
                                total,
                                item: item.clone(),
                                shared: shared.clone(),
                                chosen: chosen
                                    .read()
                                    .get(&item.key())
                                    .copied()
                                    .or_else(|| item.candidates.first().map(|c| c.contact_id)),
                                disabled: busy() || !can_mutate,
                                onchoose: move |(key, contact_id): (String, Uuid)| {
                                    chosen.write().insert(key, contact_id);
                                },
                                ondecide: move |decision: Decision| {
                                    if decision == Decision::Skip {
                                        confirm_skip.set(Some(index));
                                    } else {
                                        answer(index, decision);
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
        ConfirmDialog {
            open: confirm_skip().is_some(),
            title: "Skip this contact?",
            message: "It will not be imported, now or by any later sync or file import, and it will not be asked about again. Nothing in the source or in Mokosh is changed.",
            confirm_text: "Skip it",
            loading: busy(),
            onconfirm: move |_| {
                if let Some(index) = confirm_skip() {
                    confirm_skip.set(None);
                    answer(index, Decision::Skip);
                }
            },
            oncancel: move |_| {
                if let Some(index) = confirm_skip() {
                    confirm_skip.set(None);
                    crate::platform::dom::focus_by_id(&heading_id(index));
                }
            },
        }
    }
}

#[component]
fn ReviewCard(
    index: usize,
    total: usize,
    item: ReviewItem,
    shared: HashMap<Uuid, usize>,
    chosen: Option<Uuid>,
    disabled: bool,
    onchoose: EventHandler<(String, Uuid)>,
    ondecide: EventHandler<Decision>,
) -> Element {
    let name = item.source.name();
    let kind = item.kind();
    let source = item.source_word();
    let in_source = if item.from_file() {
        "the file"
    } else {
        "Google"
    };
    let group = format!("import-review-{index}-candidates");
    let can_link = chosen.is_some() && !item.candidates.is_empty();
    rsx! {
        section {
            class: "rounded-lg border border-line bg-surface p-6 shadow focus-within:ring-2 focus-within:ring-accent",
            "aria-labelledby": heading_id(index),
            onkeydown: move |e: KeyboardEvent| {
                if disabled {
                    return;
                }
                let modified = e.modifiers().ctrl() || e.modifiers().alt() || e.modifiers().meta();
                let key = match e.key() {
                    Key::Character(c) => c,
                    _ => return,
                };
                if let Some(decision) = shortcut(&key, modified) {
                    if decision == Decision::Link && !can_link {
                        return;
                    }
                    e.prevent_default();
                    ondecide.call(decision);
                }
            },
            div { class: "mb-4 flex flex-wrap items-baseline gap-3",
                h3 {
                    id: heading_id(index),
                    tabindex: "-1",
                    class: "text-base font-medium text-content focus:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded",
                    "{kind} {index + 1} of {total}: {name}"
                }
                for label in item.source.labels.iter() {
                    Badge { variant: BadgeVariant::Blue, "{label}" }
                }
            }
            if item.candidates.len() > 1 {
                p { class: "mb-3 text-sm text-muted",
                    "{item.candidates.len()} Mokosh contacts could be this person. Choose one to link, or create a new contact."
                }
            }
            fieldset {
                legend { class: "sr-only", "Which Mokosh contact is {name}?" }
                div { class: "space-y-4",
                    for candidate in item.candidates.clone() {
                        {
                            let others = shared
                                .get(&candidate.contact_id)
                                .copied()
                                .unwrap_or(1)
                                .saturating_sub(1);
                            let input_id = format!("{group}-{}", candidate.contact_id);
                            let rows = comparison(&item.source, &candidate);
                            let item_key = item.key();
                            let contact_id = candidate.contact_id;
                            rsx! {
                                div {
                                    key: "{candidate.contact_id}",
                                    class: "rounded-md border border-line p-4",
                                    div { class: "flex items-start gap-3",
                                        input {
                                            r#type: "radio",
                                            id: "{input_id}",
                                            name: "{group}",
                                            class: "mt-1 h-4 w-4 border-line text-accent focus:ring-2 focus:ring-accent",
                                            checked: chosen == Some(contact_id),
                                            disabled,
                                            onchange: move |_| onchoose.call((item_key.clone(), contact_id)),
                                        }
                                        div { class: "min-w-0 flex-1 space-y-2",
                                            label { r#for: "{input_id}", class: "block text-sm font-medium text-content",
                                                "{candidate.name()}"
                                                span { class: "ml-2 text-muted font-normal",
                                                    "({reason_text(&candidate.match_reason)})"
                                                }
                                            }
                                            if candidate.already_linked {
                                                p { class: "text-sm text-amber-700 dark:text-amber-300",
                                                    "Already linked to a different record from this source. Linking this one too joins two entries to one Mokosh contact."
                                                }
                                            }
                                            if others > 0 {
                                                p { class: "text-sm text-muted",
                                                    "Also proposed for {others} other record"
                                                    if others > 1 { "s" }
                                                    " in this queue."
                                                }
                                            }
                                            table { class: "w-full text-sm",
                                                caption { class: "sr-only", "{name} in {in_source} beside {candidate.name()} in Mokosh" }
                                                thead {
                                                    tr {
                                                        th { scope: "col", class: "w-24 py-1 pr-3 text-left font-normal text-muted", span { class: "sr-only", "Field" } }
                                                        th { scope: "col", class: "py-1 pr-3 text-left font-medium text-muted", "{source}" }
                                                        th { scope: "col", class: "py-1 text-left font-medium text-muted", "Mokosh" }
                                                    }
                                                }
                                                tbody {
                                                    for row in rows {
                                                        tr {
                                                            key: "{row.field.label()}",
                                                            class: if row.matched { "bg-amber-50 dark:bg-amber-950/30" } else { "" }, // theme-guard-allow: a table-row highlight for the matched field, not a banner
                                                            th { scope: "row", class: "py-1 pr-3 text-left font-normal text-muted align-top",
                                                                "{row.field.label()}"
                                                                if row.matched {
                                                                    span { class: "ml-1 text-xs font-medium text-amber-700 dark:text-amber-300", "matched" }
                                                                }
                                                            }
                                                            td { class: "py-1 pr-3 text-content align-top break-words", "{row.google}" }
                                                            td { class: "py-1 text-content align-top break-words", "{row.mokosh}" }
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
            }
            div { class: "mt-4 flex flex-wrap gap-3",
                Button {
                    disabled: disabled || !can_link,
                    onclick: move |_| ondecide.call(Decision::Link),
                    aria_label: "Link to the chosen contact (L)".to_string(),
                    data_testid: "import-review-link",
                    span { "aria-keyshortcuts": "L", "Link (L)" }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    disabled,
                    onclick: move |_| ondecide.call(Decision::Create),
                    aria_label: "Create a new contact (C)".to_string(),
                    data_testid: "import-review-create",
                    span { "aria-keyshortcuts": "C", "Create new (C)" }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    disabled,
                    onclick: move |_| ondecide.call(Decision::Skip),
                    aria_label: "Skip this Google contact (S)".to_string(),
                    data_testid: "import-review-skip",
                    span { "aria-keyshortcuts": "S", "Skip (S)" }
                }
            }
        }
    }
}

/// The contacts list's way in: a count of records waiting, shown only when
/// there are some. Reads the same status the Settings card reads, which any
/// staff member may.
#[component]
pub fn ImportReviewLink() -> Element {
    let status = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<crate::pages::settings_contact_sync::Overview>(
            "/integrations/contact-sync",
        )
        .await
        .inspect_err(|e| tracing::warn!("contact sync status load failed: {e}"))
        .ok()
    });
    let waiting = status
        .read_unchecked()
        .as_ref()
        .and_then(|s| s.as_ref())
        .and_then(|s| s.connection.as_ref())
        .map(|c| c.open_reviews)
        .unwrap_or(0);
    if waiting == 0 {
        return rsx! {};
    }
    rsx! {
        Link {
            to: Route::ContactImportReview {},
            class: "inline-flex items-center rounded-md border border-line bg-surface-2 px-4 py-2 text-sm font-medium text-content hover:opacity-90 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2",
            "Review imports ({waiting})"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u128, reason: &str) -> Candidate {
        serde_json::from_value(serde_json::json!({
            "contact_id": Uuid::from_u128(id),
            "match_reason": reason,
            "first_name": "Grace",
            "last_name": "Hopper",
            "email": "grace@navy.example",
            "company_name": "Navy",
            "phones": ["+14155550000"],
        }))
        .unwrap()
    }

    fn source() -> SourceSnapshot {
        serde_json::from_value(serde_json::json!({
            "first_name": "Grace",
            "last_name": "H",
            "emails": ["grace@navy.example", "g@home.example"],
            "phones": [{"number": "+14155550000", "phone_type": "work", "is_primary": true}],
            "company_name": "Navy",
            "labels": ["Clients"],
        }))
        .unwrap()
    }

    /// The field that made the import ask is the one marked.
    #[test]
    fn the_matched_field_is_called_out() {
        let rows = comparison(&source(), &candidate(1, "phone"));
        let matched: Vec<Field> = rows.iter().filter(|r| r.matched).map(|r| r.field).collect();
        assert_eq!(matched, vec![Field::Phone]);
        let rows = comparison(&source(), &candidate(1, "name_company"));
        let matched: Vec<Field> = rows.iter().filter(|r| r.matched).map(|r| r.field).collect();
        assert_eq!(matched, vec![Field::Name, Field::Company]);
        let email = comparison(&source(), &candidate(1, "email_ambiguous"));
        assert!(email.iter().any(|r| r.field == Field::Email && r.matched));
        assert_eq!(
            email[1].google, "grace@navy.example, g@home.example",
            "every source address is shown"
        );
    }

    /// An empty value reads as "Not set", never as a blank a reviewer could
    /// mistake for "the same".
    #[test]
    fn an_empty_value_says_not_set() {
        let rows = comparison(&source(), &candidate(1, "phone"));
        let title = rows.iter().find(|r| r.field == Field::Title).unwrap();
        assert_eq!(
            (title.google.as_str(), title.mokosh.as_str()),
            ("Not set", "Not set")
        );
    }

    /// Two Google records proposing one Mokosh contact is counted, so each
    /// card can say so.
    #[test]
    fn one_contact_proposed_twice_is_counted() {
        let items = vec![
            ReviewItem {
                connection_id: None,
                provider: "google".into(),
                source_label: "ops@msp.example".into(),
                external_id: "people/a".into(),
                source: source(),
                candidates: vec![candidate(1, "phone"), candidate(2, "phone")],
            },
            ReviewItem {
                connection_id: None,
                provider: "google".into(),
                source_label: "ops@msp.example".into(),
                external_id: "people/b".into(),
                source: source(),
                candidates: vec![candidate(1, "phone")],
            },
        ];
        let counts = proposals_per_contact(&items);
        assert_eq!(counts[&Uuid::from_u128(1)], 2);
        assert_eq!(counts[&Uuid::from_u128(2)], 1);
    }

    /// MAPPS-916: a record from an uploaded file says which file, is keyed per
    /// source, and its answer names the source back; a Google record's answer
    /// stays the shape it was.
    #[test]
    fn a_file_record_names_its_file_and_its_source() {
        let item: ReviewItem = serde_json::from_value(serde_json::json!({
            "connection_id": "00000000-0000-0000-0000-000000000007",
            "provider": "vcard",
            "source_label": "apollo.vcf",
            "external_id": "mh-1",
            "source": { "first_name": "M.", "last_name": "Hamilton" },
            "candidates": [],
        }))
        .unwrap();
        assert_eq!(item.kind(), "Card from apollo.vcf");
        assert_eq!(item.source_word(), "File");
        assert_eq!(item.key(), "00000000-0000-0000-0000-000000000007:mh-1");
        let body = serde_json::to_value(ResolveBody::Skip {
            external_id: item.external_id.clone(),
            connection_id: item.connection_id,
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "action": "skip",
                "external_id": "mh-1",
                "connection_id": "00000000-0000-0000-0000-000000000007",
            })
        );
        let google = serde_json::to_value(ResolveBody::Create {
            external_id: "people/c1".into(),
            connection_id: None,
        })
        .unwrap();
        assert_eq!(
            google,
            serde_json::json!({ "action": "create", "external_id": "people/c1" })
        );
    }

    #[test]
    fn the_shortcuts_are_l_c_s_and_ignore_modifiers() {
        assert_eq!(shortcut("l", false), Some(Decision::Link));
        assert_eq!(shortcut("C", false), Some(Decision::Create));
        assert_eq!(shortcut("s", false), Some(Decision::Skip));
        assert_eq!(shortcut("s", true), None, "Ctrl+S is the browser's");
        assert_eq!(shortcut("x", false), None);
    }

    #[test]
    fn every_answer_is_announced_with_what_is_left() {
        assert_eq!(
            announcement(Decision::Link, "Grace H", Some("Grace Hopper"), 2),
            "Linked Grace H to Grace Hopper. 2 left to review."
        );
        assert_eq!(
            announcement(Decision::Create, "Ada", None, 1),
            "Created a new contact for Ada. 1 left to review."
        );
        assert_eq!(
            announcement(Decision::Skip, "Sam", None, 0),
            "Skipped Sam; it will not be imported. Nothing left to review."
        );
    }

    #[test]
    fn a_nameless_record_falls_back_to_its_display_name() {
        let s: SourceSnapshot =
            serde_json::from_value(serde_json::json!({"display_name": "Acme Front Desk"})).unwrap();
        assert_eq!(s.name(), "Acme Front Desk");
    }

    /// Skip, the one answer that cannot be undone, asks first; the others
    /// are sent straight away. Bodies are typed.
    #[test]
    fn only_skip_asks_first_and_bodies_are_typed() {
        let src = include_str!("contact_sync_review.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if decision == Decision::Skip {"));
        assert!(head.contains("confirm_skip.set(Some(index));"));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
        assert!(head.contains("\"aria-keyshortcuts\": \"L\""));
        assert!(head.contains("\"aria-live\": \"polite\""));
    }
}

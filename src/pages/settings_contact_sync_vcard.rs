//! Settings > Integrations > vCard import (MAPPS-915, PMS-1290).
//!
//! An uploaded `.vcf` file goes through the same import the Google account
//! does: the server previews it with the sync's own decision, the admin opts
//! into the file's categories one at a time, a review step asks for the exact
//! figures, and the import runs on the server as a queued run. Records it is
//! not sure about wait in the one review queue beside Google's. What this page
//! adds in front of the Google wizard's steps is only the upload, and what it
//! reuses is everything after it: the per-category figures, the running
//! estimate and the sentence the review step states are the Google wizard's
//! own functions (`settings_contact_sync_import`).
//!
//! # Opt-in
//!
//! Nothing is selected after an upload. A card with no `CATEGORIES` is offered
//! as "No category" (the server's `mokosh:ungrouped`), listed last, so a file
//! with no categories at all can be imported and importing it is still a
//! choice rather than a default.
//!
//! # The file is untrusted
//!
//! The server refuses a file over its limit with a 413 and says why; the size
//! is checked here first only so a 50 MB mistake is not sent at all. The
//! server's own message is what is shown when it refuses.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, BannerTone, Button, ButtonVariant, Card, Checkbox,
    ContentUnavailable, ErrorBanner, FileField, PageHeader, StatusBanner, Table, TableBody,
    TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::pages::settings_contact_sync::Run;
use crate::pages::settings_contact_sync_import::{
    estimate, label_help, summary, LabelRow, Preview, Totals,
};
use crate::Route;

/// The group the server offers for cards with no `CATEGORIES`.
pub const UNGROUPED: &str = "mokosh:ungrouped";

/// The server's own file limit (`vcard::Limits::DEFAULT`), checked here only
/// to avoid sending a file that cannot be accepted.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

const UPLOADS: &str = "/integrations/contact-sync/vcard/uploads";
const POLL_MS: u32 = 3_000;

/// A card the reader could not import, or imported with something to say.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CardProblem {
    #[serde(default)]
    pub card: u32,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub reason: String,
}

/// `ImportFileView` (PMS-1290), the fields this page reads.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct UploadedFile {
    pub id: Uuid,
    pub filename: String,
    #[serde(default)]
    pub cards: i32,
    #[serde(default)]
    pub contacts: i32,
    #[serde(default)]
    pub group_cards: i32,
    #[serde(default)]
    pub failures: Vec<CardProblem>,
    #[serde(default)]
    pub warnings: Vec<CardProblem>,
}

/// The upload's answer: the stored file and what importing it would do.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Uploaded {
    pub file: UploadedFile,
    pub preview: Preview,
}

/// One row of `GET /integrations/contact-sync/vcard/uploads` (`ImportFileView`,
/// PMS-1290): the fields the "Recent uploads" list shows.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RecentUpload {
    pub id: Uuid,
    pub filename: String,
    pub uploaded_at: DateTime<Utc>,
    #[serde(default)]
    pub uploaded_by_name: Option<String>,
    #[serde(default)]
    pub discarded_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub contacts: i32,
    #[serde(default)]
    pub latest_run: Option<Run>,
}

/// What a prior upload's row says happened to it, in the order the reader
/// cares about: a run in flight or finished says more than the file's own
/// `discarded_at`.
pub fn upload_status(row: &RecentUpload) -> &'static str {
    match row.latest_run.as_ref().map(|r| r.status.as_str()) {
        Some("completed") => "Imported",
        Some("failed") => "Import failed",
        Some("cancelled") => "Cancelled",
        Some("queued") | Some("running") => "Importing…",
        _ if row.discarded_at.is_some() => "Expired",
        _ => "Not imported yet",
    }
}

#[derive(Serialize)]
struct PreviewBody {
    group_ids: Vec<String>,
}

#[derive(Serialize)]
struct ImportBody {
    group_ids: Vec<String>,
}

/// Where the page is. `Importing` holds the run it polls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Upload,
    Choose,
    Review,
    Importing(Uuid),
}

/// The file's categories as the picker lists them: by name, "No category"
/// last. Each row carries what importing that category alone would do.
pub fn category_rows(preview: &Preview) -> Vec<LabelRow> {
    let mut rows: Vec<LabelRow> = preview
        .groups
        .iter()
        .map(|g| LabelRow {
            id: g.id.clone(),
            name: g.name.clone(),
            member_count: g.member_count,
            everything: false,
            counts: estimate(&preview.records, &BTreeSet::from([g.id.clone()])),
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.id == UNGROUPED)
            .cmp(&(b.id == UNGROUPED))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

/// The review sentence, in a file's words when nothing would happen.
pub fn file_summary(totals: &Totals) -> String {
    if totals.contacts == 0 {
        "No contacts in this file carry the chosen categories, so this import would change nothing."
            .to_string()
    } else {
        summary(totals)
    }
}

/// What the file held, in one sentence, before anything is chosen.
pub fn file_headline(file: &UploadedFile) -> String {
    let mut sentence = format!(
        "{} holds {} {}.",
        file.filename,
        file.contacts,
        if file.contacts == 1 {
            "contact"
        } else {
            "contacts"
        }
    );
    let skipped = file.failures.len();
    if skipped > 0 {
        sentence.push_str(&format!(
            " {skipped} {} could not be read and will not be imported.",
            if skipped == 1 { "card" } else { "cards" }
        ));
    }
    if file.group_cards > 0 {
        sentence.push_str(&format!(
            " {} {} a group rather than a person and {} skipped.",
            file.group_cards,
            if file.group_cards == 1 {
                "card describes"
            } else {
                "cards describe"
            },
            if file.group_cards == 1 { "is" } else { "are" }
        ));
    }
    sentence
}

/// Refuse a file here only when the server certainly would.
pub fn precheck(file_name: &str, size: usize) -> Result<(), String> {
    if size == 0 {
        return Err(format!("{file_name} is empty."));
    }
    if size > MAX_BYTES {
        return Err(format!(
            "{file_name} is larger than 10 MB, the most one import accepts. Export the contacts in smaller groups and import each file."
        ));
    }
    Ok(())
}

/// A run that will not change again.
pub fn is_finished(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

/// A run's progress in words.
pub fn run_progress(run: &Run) -> String {
    match run.status.as_str() {
        "queued" => "Waiting to start. The import runs on the server; you can leave this page.".to_string(),
        "running" => match run.total {
            Some(total) => format!("Importing: {} of {total} contacts read.", run.processed),
            None => "Importing: reading the file.".to_string(),
        },
        "completed" => format!(
            "Imported. {} created, {} linked to contacts already in Mokosh, {} updated, {} waiting in the review queue.",
            run.created, run.linked, run.updated, run.queued_for_review
        ),
        "cancelled" => "The import was cancelled. What it had already written is kept.".to_string(),
        _ => run
            .error
            .clone()
            .unwrap_or_else(|| "The import did not finish.".to_string()),
    }
}

#[component]
pub fn VcardImportPage() -> Element {
    use_page_title("Import a vCard file");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Import a vCard file" } };
    }
    rsx! { VcardImportBody {} }
}

#[component]
fn VcardImportBody() -> Element {
    let mut step = use_signal(|| Step::Upload);
    let mut uploaded: Signal<Option<Uploaded>> = use_signal(|| None);
    let mut selected: Signal<BTreeSet<String>> = use_signal(BTreeSet::new);
    let mut exact: Signal<Option<Result<Totals, String>>> = use_signal(|| None);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut run: Signal<Option<Run>> = use_signal(|| None);
    let mut polling = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();
    let navigator = use_navigator();

    // The upload history for the "Recent uploads" list below the picker.
    // Re-read after each upload (`refresh` bump) so a just-uploaded file
    // shows up without a page reload.
    let mut refresh = use_signal(|| 0u32);
    let recent = crate::hooks::use_remote_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _ = refresh.read();
        crate::hooks::fetch::api::get_authed::<Vec<RecentUpload>>(UPLOADS).await
    });

    // Read the run while it can still change, and stop when it cannot.
    use_effect(move || {
        let Step::Importing(run_id) = step() else {
            return;
        };
        let finished = run().as_ref().is_some_and(|r| is_finished(&r.status));
        if finished || polling() {
            return;
        }
        polling.set(true);
        spawn(async move {
            crate::platform::timer::sleep_ms(POLL_MS).await;
            #[cfg(feature = "app")]
            {
                let path = format!("/integrations/contact-sync/runs/{run_id}");
                match crate::hooks::fetch::api::get_authed::<Run>(&path).await {
                    Ok(latest) => run.set(Some(latest)),
                    Err(e) => tracing::error!("vCard import run load failed: {e}"),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = run_id;
            polling.set(false);
        });
    });

    let upload = move |evt: FormEvent| {
        error.set(String::new());
        let Some(file) = evt.files().into_iter().next() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let file_name = file.name();
            let mime = file
                .content_type()
                .unwrap_or_else(|| "text/vcard".to_string());
            let bytes = match file.read_bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(e) => {
                    tracing::error!("reading the chosen vCard file failed: {e:?}");
                    error.set("Could not read that file.".to_string());
                    busy.set(false);
                    return;
                }
            };
            if let Err(refusal) = precheck(&file_name, bytes.len()) {
                error.set(refusal);
                busy.set(false);
                return;
            }
            #[cfg(feature = "app")]
            match crate::hooks::fetch::api::post_file_authed::<Uploaded>(
                UPLOADS, &file_name, &mime, &bytes,
            )
            .await
            {
                Ok(answer) => {
                    uploaded.set(Some(answer));
                    selected.set(BTreeSet::new());
                    step.set(Step::Choose);
                    refresh += 1;
                }
                Err(e) => {
                    tracing::error!("vCard upload failed: {e}");
                    error.set(format!("The file was not accepted: {e}"));
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (mime, bytes);
            busy.set(false);
        });
    };

    let mut review = move |file_id: Uuid, ids: BTreeSet<String>| {
        step.set(Step::Review);
        exact.set(None);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("{UPLOADS}/{file_id}/preview");
                let body = PreviewBody {
                    group_ids: ids.into_iter().collect(),
                };
                let result = crate::hooks::fetch::api::post_authed::<Preview, _>(&path, &body)
                    .await
                    .map(|p| p.totals)
                    .map_err(|e| {
                        tracing::error!("vCard import preview failed: {e}");
                        e.to_string()
                    });
                exact.set(Some(result));
            }
            #[cfg(not(feature = "app"))]
            let _ = (file_id, ids);
        });
    };

    let mut start = move |file_id: Uuid, ids: BTreeSet<String>| {
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("{UPLOADS}/{file_id}/import");
                let body = ImportBody {
                    group_ids: ids.into_iter().collect(),
                };
                match crate::hooks::fetch::api::post_authed::<Run, _>(&path, &body).await {
                    Ok(queued) => {
                        step.set(Step::Importing(queued.id));
                        run.set(Some(queued));
                    }
                    Err(e) => {
                        tracing::error!("vCard import did not start: {e}");
                        error.set(format!("The import did not start: {e}"));
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = (file_id, ids);
            busy.set(false);
        });
    };

    let mut reset = move || {
        uploaded.set(None);
        selected.set(BTreeSet::new());
        exact.set(None);
        run.set(None);
        error.set(String::new());
        step.set(Step::Upload);
    };

    let current = step();
    let chosen = selected();

    rsx! {
        PageHeader {
            title: "Import a vCard file",
            subtitle: "Bring contacts in from a .vcf file exported from a phone, Outlook, Google or any address book. Nothing is written until you start the import.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsVcardImport {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        ol { class: "mb-4 flex flex-wrap gap-4 text-sm", "aria-label": "Import steps",
            for (index, (label, here)) in [
                ("Upload a file", current == Step::Upload),
                ("Choose categories", current == Step::Choose),
                ("Review and import", matches!(current, Step::Review | Step::Importing(_))),
            ]
                .into_iter()
                .enumerate()
            {
                li {
                    key: "{index}",
                    class: if here { "font-medium text-content" } else { "text-muted" },
                    "aria-current": here.then_some("step"),
                    "{index + 1}. {label}"
                }
            }
        }
        match (current, uploaded()) {
            (Step::Upload, _) | (_, None) => rsx! {
                Card {
                    div { class: "space-y-4",
                        FileField {
                            name: "vcard_file",
                            label: "vCard file",
                            accept: ".vcf,.vcard,text/vcard,text/x-vcard",
                            disabled: busy() || !can_mutate,
                            help: "A .vcf file of one contact or a whole address book, up to 10 MB. Versions 2.1, 3.0 and 4.0 are all read.",
                            onchange: upload,
                        }
                        if busy() {
                            p { class: "text-sm text-muted", role: "status", "Reading the file…" }
                        }
                        ul { class: "list-disc space-y-1 pl-5 text-sm text-muted",
                            li { "The file is read on the server and kept only until it is imported, or for a day if it never is." }
                            li { "Addresses, birthdays, photos and other details Mokosh has no place for are not imported. A photo link in the file is never opened." }
                            li { "Importing the same file again changes nothing that is already in Mokosh." }
                        }
                    }
                }
                Card { class: "mt-6",
                    div { class: "space-y-3",
                        p { class: "text-sm font-medium text-content", "Recent uploads" }
                        if recent.is_unavailable() {
                            ContentUnavailable { title: "Recent uploads".to_string() }
                        } else if recent.is_loading() {
                            Table {
                                TableHead {
                                    TableRow {
                                        TableHeader { "File" }
                                        TableHeader { "Uploaded" }
                                        TableHeader { "Contacts" }
                                        TableHeader { "Status" }
                                    }
                                }
                                TableLoading { columns: 4 }
                            }
                        } else {
                            {
                                let rows = recent.clone().value_or_default();
                                let tz = crate::utils::datetime::user_timezone();
                                let pref = crate::utils::datetime::user_format_pref();
                                rsx! {
                                    Table {
                                        TableHead {
                                            TableRow {
                                                TableHeader { "File" }
                                                TableHeader { "Uploaded" }
                                                TableHeader { "Contacts" }
                                                TableHeader { "Status" }
                                            }
                                        }
                                        if rows.is_empty() {
                                            TableEmpty {
                                                columns: 4,
                                                title: "No uploads yet".to_string(),
                                                description: "Files uploaded here show up in this list.".to_string(),
                                            }
                                        } else {
                                            TableBody {
                                                for row in rows.iter().cloned() {
                                                    {
                                                        let uploaded_at = crate::utils::datetime::fmt_user_dt_in(
                                                            row.uploaded_at,
                                                            pref.as_deref(),
                                                            tz,
                                                            Some("%b %d, %Y %H:%M"),
                                                        );
                                                        let status = upload_status(&row);
                                                        rsx! {
                                                            TableRow { key: "{row.id}",
                                                                TableCell { "{row.filename}" }
                                                                TableCell { "{uploaded_at}" }
                                                                TableCell { "{row.contacts}" }
                                                                TableCell { "{status}" }
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
            },
            (Step::Choose, Some(data)) => {
                let rows = category_rows(&data.preview);
                let running = estimate(&data.preview.records, &chosen);
                let nothing_chosen = chosen.is_empty();
                let file_id = data.file.id;
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            p { class: "text-sm text-content", "{file_headline(&data.file)}" }
                            if !data.file.failures.is_empty() {
                                CardProblems {
                                    title: "Cards that will not be imported",
                                    tone: BannerTone::Warning,
                                    problems: data.file.failures.clone(),
                                }
                            }
                            if !data.file.warnings.is_empty() {
                                CardProblems {
                                    title: "Imported with a note",
                                    tone: BannerTone::Info,
                                    problems: data.file.warnings.clone(),
                                }
                            }
                            fieldset { class: "space-y-4",
                                legend { class: "text-base font-medium text-content",
                                    "Which contacts belong in the CRM?"
                                }
                                p { class: "text-sm text-muted",
                                    "Pick the categories your business contacts carry. Figures are what importing each category on its own would do."
                                }
                                if rows.is_empty() {
                                    StatusBanner { tone: BannerTone::Info,
                                        "Nothing in this file can be imported."
                                    }
                                }
                                for row in rows.clone() {
                                    div { key: "{row.id}",
                                        class: if row.id == UNGROUPED { "mt-6 border-t border-line pt-4" } else { "" },
                                        Checkbox {
                                            name: "category-{row.id}",
                                            label: format!(
                                                "{}{}",
                                                row.name,
                                                row.member_count.map(|n| format!(" ({n})")).unwrap_or_default()
                                            ),
                                            checked: chosen.contains(&row.id),
                                            disabled: busy(),
                                            help: label_help(&row.counts),
                                            onchange: {
                                                let id = row.id.clone();
                                                move |e: FormEvent| {
                                                    let mut next = selected();
                                                    if e.checked() {
                                                        next.insert(id.clone());
                                                    } else {
                                                        next.remove(&id);
                                                    }
                                                    selected.set(next);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Card { class: "mt-6",
                        div { class: "space-y-3", "aria-live": "polite",
                            p { class: "text-sm font-medium text-content", "Estimate for this selection" }
                            p { class: "text-sm text-muted",
                                if nothing_chosen {
                                    "Choose at least one category."
                                } else {
                                    "{file_summary(&running)}"
                                }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                Button {
                                    disabled: nothing_chosen || !can_mutate,
                                    onclick: move |_| review(file_id, selected()),
                                    data_testid: "vcard-import-review",
                                    "Review import"
                                }
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| reset(),
                                    "Choose a different file"
                                }
                            }
                        }
                    }
                }
            }
            (Step::Review, Some(data)) => {
                let names: Vec<String> = category_rows(&data.preview)
                    .into_iter()
                    .filter(|r| chosen.contains(&r.id))
                    .map(|r| r.name)
                    .collect();
                let file_id = data.file.id;
                let exact_snap = exact();
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            h2 {
                                class: "text-base font-medium text-content focus:outline-none",
                                tabindex: "-1",
                                onmounted: move |e| async move {
                                    // The step changed under the keyboard user,
                                    // so focus follows it.
                                    let _ = e.set_focus(true).await;
                                },
                                "Review the import of {data.file.filename}"
                            }
                            div { class: "flex flex-wrap gap-2",
                                for name in names {
                                    Badge { variant: BadgeVariant::Blue, "{name}" }
                                }
                            }
                            div { "aria-live": "polite",
                                match exact_snap {
                                    None => rsx! {
                                        p { class: "text-sm text-muted", role: "status",
                                            "Checking exactly what this selection would do…"
                                        }
                                    },
                                    Some(Err(e)) => rsx! {
                                        ErrorBanner { "Could not check this selection: {e}" }
                                    },
                                    Some(Ok(totals)) => rsx! {
                                        p { class: "text-sm text-content", "{file_summary(&totals)}" }
                                    },
                                }
                            }
                            ul { class: "list-disc space-y-1 pl-5 text-sm text-muted",
                                li { "No contact is merged on a name alone. Anything short of an exact email match waits in the review queue." }
                                li { "Company names are kept as text; a matching company is suggested, never linked for you." }
                                li { "A contact that is not in this file is left alone. A file is never read as the whole address book." }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                Button {
                                    disabled: busy() || !can_mutate || !matches!(exact(), Some(Ok(_))),
                                    onclick: move |_| start(file_id, selected()),
                                    data_testid: "vcard-import-start",
                                    if busy() { "Starting…" } else { "Start import" }
                                }
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    disabled: busy(),
                                    onclick: move |_| step.set(Step::Choose),
                                    "Back"
                                }
                            }
                        }
                    }
                }
            }
            (Step::Importing(_), Some(data)) => {
                let snapshot = run();
                let finished = snapshot.as_ref().is_some_and(|r| is_finished(&r.status));
                let queued = snapshot.as_ref().map_or(0, |r| r.queued_for_review);
                let tone = match snapshot.as_ref().map(|r| r.status.as_str()) {
                    Some("completed") => BannerTone::Success,
                    Some("failed") => BannerTone::Error,
                    Some("cancelled") => BannerTone::Warning,
                    _ => BannerTone::Info,
                };
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            h2 {
                                class: "text-base font-medium text-content focus:outline-none",
                                tabindex: "-1",
                                onmounted: move |e| async move {
                                    let _ = e.set_focus(true).await;
                                },
                                "Importing {data.file.filename}"
                            }
                            div { "aria-live": "polite",
                                if let Some(r) = snapshot.as_ref() {
                                    StatusBanner { tone, "{run_progress(r)}" }
                                }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                if finished && queued > 0 {
                                    Button {
                                        onclick: move |_| { navigator.push(Route::ContactImportReview {}); },
                                        "Open the review queue"
                                    }
                                }
                                if finished {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| reset(),
                                        "Import another file"
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

/// Cards the reader flagged, each located well enough to find in the file.
#[component]
fn CardProblems(title: String, tone: BannerTone, problems: Vec<CardProblem>) -> Element {
    rsx! {
        StatusBanner { tone,
            div { class: "space-y-2",
                p { class: "font-medium", "{title}" }
                ul { class: "list-disc space-y-1 pl-5",
                    for problem in problems {
                        li { key: "{problem.card}-{problem.line}",
                            "Card {problem.card} (line {problem.line}, {problem.hint}): {problem.reason}"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uploaded() -> Uploaded {
        serde_json::from_value(serde_json::json!({
            "file": {
                "id": "00000000-0000-0000-0000-000000000009",
                "filename": "Office contacts.vcf",
                "cards": 5,
                "contacts": 3,
                "group_cards": 1,
                "failures": [{"card": 2, "line": 5, "hint": "FN:Broken", "reason": "The card has no END:VCARD before the next card begins, so it was skipped."}],
                "warnings": [],
            },
            "preview": {
                "groups": [
                    {"id": UNGROUPED, "name": "No category", "member_count": 1},
                    {"id": "VIP", "name": "VIP", "member_count": 1},
                    {"id": "Client", "name": "Client", "member_count": 2},
                ],
                "records": [
                    {"group_ids": ["Client"], "outcome": "create"},
                    {"group_ids": ["Client", "VIP"], "outcome": "link"},
                    {"group_ids": [UNGROUPED], "outcome": "review"},
                ],
                "totals": {"contacts": 3},
            },
        }))
        .expect("uploaded")
    }

    /// Categories by name, "No category" last, each with its own figures.
    #[test]
    fn no_category_is_offered_last() {
        let rows = category_rows(&uploaded().preview);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["Client", "VIP", UNGROUPED]);
        assert!(rows.iter().all(|r| !r.everything));
        assert_eq!(label_help(&rows[0].counts), "1 new, 1 to link, 0 to review");
    }

    #[test]
    fn the_headline_says_what_the_file_held_and_lost() {
        assert_eq!(
            file_headline(&uploaded().file),
            "Office contacts.vcf holds 3 contacts. 1 card could not be read and will not be imported. 1 card describes a group rather than a person and is skipped."
        );
        assert!(file_summary(&Totals::default()).contains("chosen categories"));
    }

    fn recent_upload(latest_run_status: Option<&str>, discarded: bool) -> RecentUpload {
        RecentUpload {
            id: Uuid::nil(),
            filename: "Office contacts.vcf".to_string(),
            uploaded_at: Utc::now(),
            uploaded_by_name: Some("Ana".to_string()),
            discarded_at: discarded.then(Utc::now),
            contacts: 3,
            latest_run: latest_run_status.map(|status| {
                serde_json::from_value(serde_json::json!({
                    "id": "00000000-0000-0000-0000-000000000001",
                    "status": status,
                    "created": 0, "linked": 0, "updated": 0, "queued_for_review": 0,
                }))
                .expect("run")
            }),
        }
    }

    /// `GET /integrations/contact-sync/vcard/uploads` (`list_vcard_uploads`,
    /// `mokosh-server/src/modules/contact_sync/routes.rs`) is authoritative
    /// for `ImportFileView`'s shape; this client type is a read-only subset
    /// of it, so a field this page needs but the server drops would show up
    /// as a compile error here, not a silent blank column.
    #[test]
    fn the_status_label_prefers_the_latest_run_over_discarded_at() {
        assert_eq!(
            upload_status(&recent_upload(None, false)),
            "Not imported yet"
        );
        assert_eq!(upload_status(&recent_upload(None, true)), "Expired");
        assert_eq!(
            upload_status(&recent_upload(Some("queued"), false)),
            "Importing…"
        );
        assert_eq!(
            upload_status(&recent_upload(Some("running"), false)),
            "Importing…"
        );
        assert_eq!(
            upload_status(&recent_upload(Some("completed"), true)),
            "Imported"
        );
        assert_eq!(
            upload_status(&recent_upload(Some("failed"), true)),
            "Import failed"
        );
        assert_eq!(
            upload_status(&recent_upload(Some("cancelled"), true)),
            "Cancelled"
        );
    }

    /// Only what the server would certainly refuse is refused here.
    #[test]
    fn the_precheck_refuses_only_empty_and_oversized_files() {
        assert!(precheck("a.vcf", 1).is_ok());
        assert!(precheck("a.vcf", MAX_BYTES).is_ok());
        assert!(precheck("a.vcf", 0).unwrap_err().contains("empty"));
        assert!(precheck("a.vcf", MAX_BYTES + 1)
            .unwrap_err()
            .contains("larger than 10 MB"));
    }

    #[test]
    fn a_run_is_polled_until_it_cannot_change() {
        for done in ["completed", "failed", "cancelled"] {
            assert!(is_finished(done));
        }
        for live in ["queued", "running"] {
            assert!(!is_finished(live));
        }
        let run: Run = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "status": "completed",
            "created": 2, "linked": 1, "updated": 0, "queued_for_review": 1,
        }))
        .unwrap();
        assert!(run_progress(&run).contains("2 created, 1 linked"));
        assert!(run_progress(&run).contains("1 waiting in the review queue"));
    }

    /// Admin only, typed bodies, nothing selected after an upload, and no
    /// motion that ignores reduced-motion.
    #[test]
    fn the_flow_is_admin_only_opt_in_and_typed() {
        let src = include_str!("settings_contact_sync_vcard.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
        assert!(head.contains(
            "selected.set(BTreeSet::new());\n                    step.set(Step::Choose);"
        ));
        assert!(!head.contains("animate-"), "no unconditional motion");
        assert!(head.contains("post_file_authed::<Uploaded>"));
    }
}

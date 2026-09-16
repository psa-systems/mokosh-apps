//! Settings > Integrations > Google Contacts > Import (MAPPS-809, PSA-70 E).
//!
//! The first-run flow, and the place the "do not dump an address book into a
//! shared CRM" rule is enforced: choose labels, preview, review, import.
//!
//! # Opt-in, one label at a time
//!
//! Nothing is selected when the page opens on a tenant that has never
//! imported, and Google's "My Contacts" (every contact in the account) is
//! offered last, set apart and worded as the whole address book, so importing
//! everything is a choice somebody makes rather than a default somebody
//! accepts.
//!
//! # A preview that is the import
//!
//! `POST /integrations/contact-sync/preview` (PMS-1242) reads the account once
//! and runs every labelled record through the sync's own decision without
//! writing anything. It answers each record's labels and outcome - create,
//! link, review, already imported, excluded - and no names, so the per-label
//! figures and the running estimate below are totalled HERE from one read and
//! change instantly as boxes are ticked. The review step asks again for the
//! exact selection, because a selection that holds two records for one new
//! address creates one contact and asks about the other, which only a
//! simulation of that selection can count.
//!
//! # In the background
//!
//! Starting the import saves the selection and queues a run, then goes back to
//! the Google Contacts card, which polls the run. The request returns as soon
//! as the run is queued, so leaving the page does not stop anything.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, BannerTone, Button, ButtonVariant, Card, Checkbox,
    DetailSkeleton, ErrorBanner, PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// Google's system group holding every contact in the account.
pub const EVERYTHING: &str = "contactGroups/myContacts";
/// Google's starred contacts.
pub const STARRED: &str = "contactGroups/starred";

/// `POST /integrations/contact-sync/preview` (PMS-1242).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Preview {
    #[serde(default)]
    pub groups: Vec<PreviewGroup>,
    #[serde(default)]
    pub records: Vec<PreviewRecord>,
    #[serde(default)]
    pub totals: Totals,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PreviewGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub member_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PreviewRecord {
    #[serde(default)]
    pub group_ids: Vec<String>,
    pub outcome: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Totals {
    #[serde(default)]
    pub contacts: u32,
    #[serde(default)]
    pub create: u32,
    #[serde(default)]
    pub link: u32,
    #[serde(default)]
    pub review: u32,
    #[serde(default)]
    pub imported: u32,
    #[serde(default)]
    pub excluded: u32,
}

impl Totals {
    fn add(&mut self, outcome: &str) {
        self.contacts += 1;
        match outcome {
            "create" => self.create += 1,
            "link" => self.link += 1,
            "review" => self.review += 1,
            "imported" => self.imported += 1,
            _ => self.excluded += 1,
        }
    }
}

/// Every record carrying at least one selected label, counted once however
/// many of its labels are selected.
pub fn estimate(records: &[PreviewRecord], selected: &BTreeSet<String>) -> Totals {
    let mut totals = Totals::default();
    for record in records
        .iter()
        .filter(|r| r.group_ids.iter().any(|g| selected.contains(g)))
    {
        totals.add(&record.outcome);
    }
    totals
}

/// A label as the picker lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelRow {
    pub id: String,
    pub name: String,
    pub member_count: Option<u32>,
    /// The whole address book, which is offered last and set apart.
    pub everything: bool,
    pub counts: Totals,
}

/// The labels in the order they are offered: the account's own labels by
/// name, then Starred, then everything last.
pub fn label_rows(preview: &Preview) -> Vec<LabelRow> {
    let mut rows: Vec<LabelRow> = preview
        .groups
        .iter()
        .map(|g| LabelRow {
            id: g.id.clone(),
            name: if g.id == EVERYTHING {
                "Every contact in the account".to_string()
            } else {
                g.name.clone()
            },
            member_count: g.member_count,
            everything: g.id == EVERYTHING,
            counts: estimate(&preview.records, &BTreeSet::from([g.id.clone()])),
        })
        .collect();
    let rank = |row: &LabelRow| match row.id.as_str() {
        EVERYTHING => 2,
        STARRED => 1,
        _ => 0,
    };
    rows.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The sentence the review step states before anything is written.
pub fn summary(totals: &Totals) -> String {
    if totals.contacts == 0 {
        return "No contacts carry the chosen labels, so this import would change nothing."
            .to_string();
    }
    let mut parts = vec![
        format!(
            "create {}",
            plural(totals.create, "new contact", "new contacts")
        ),
        format!(
            "link {} already in Mokosh",
            plural(totals.link, "contact", "contacts")
        ),
        format!(
            "put {} in the review queue for a person to decide",
            plural(totals.review, "contact", "contacts")
        ),
    ];
    if totals.imported > 0 {
        parts.push(format!(
            "update {} imported before",
            plural(totals.imported, "contact", "contacts")
        ));
    }
    let last = parts.pop().unwrap_or_default();
    let mut sentence = format!(
        "Importing {} will {} and {}.",
        plural(totals.contacts, "contact", "contacts"),
        parts.join(", "),
        last
    );
    if totals.excluded > 0 {
        sentence.push_str(&format!(
            " {} unlinked, removed on request or skipped by a reviewer {} left out.",
            plural(totals.excluded, "contact", "contacts"),
            if totals.excluded == 1 { "is" } else { "are" }
        ));
    }
    sentence
}

#[derive(Serialize)]
struct PreviewBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    group_ids: Option<Vec<String>>,
}

#[derive(Serialize)]
struct SelectionBody {
    group_ids: Vec<String>,
}

#[derive(Serialize)]
struct NoBody {}

/// The two steps before the import starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Choose,
    Review,
}

#[component]
pub fn GoogleContactsImportPage() -> Element {
    use_page_title("Import Google Contacts");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Import Google Contacts" } };
    }
    rsx! { GoogleContactsImportBody {} }
}

#[component]
fn GoogleContactsImportBody() -> Element {
    // The current selection, so changing labels later starts from it.
    let overview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<crate::pages::settings_contact_sync::Overview>(
            "/integrations/contact-sync",
        )
        .await
        .inspect_err(|e| tracing::error!("contact sync status load failed: {e}"))
        .ok()
    });
    let preview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::post_authed::<Preview, _>(
            "/integrations/contact-sync/preview",
            &PreviewBody { group_ids: None },
        )
        .await
        .map_err(|e| {
            tracing::error!("contact sync preview failed: {e}");
            e
        })
    });
    let mut selected: Signal<Option<BTreeSet<String>>> = use_signal(|| None);
    let mut step = use_signal(|| Step::Choose);
    let mut exact: Signal<Option<Result<Totals, String>>> = use_signal(|| None);
    let mut error = use_signal(String::new);
    let mut starting = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();
    let navigator = use_navigator();

    // Seed the selection from the connection once it is known, so changing
    // labels later starts from what is saved. In an effect rather than the
    // render body: writing a signal while rendering re-renders forever.
    use_effect(move || {
        if let Some(Some(o)) = overview.read().as_ref() {
            if selected.peek().is_none() {
                let seeded: BTreeSet<String> = o
                    .connection
                    .as_ref()
                    .map(|c| c.selected_groups.iter().cloned().collect())
                    .unwrap_or_default();
                selected.set(Some(seeded));
            }
        }
    });
    let chosen = selected().unwrap_or_default();

    let mut review = move |ids: BTreeSet<String>| {
        step.set(Step::Review);
        exact.set(None);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = PreviewBody {
                    group_ids: Some(ids.into_iter().collect()),
                };
                let result = crate::hooks::fetch::api::post_authed::<Preview, _>(
                    "/integrations/contact-sync/preview",
                    &body,
                )
                .await
                .map(|p| p.totals);
                exact.set(Some(result));
            }
            #[cfg(not(feature = "app"))]
            let _ = ids;
        });
    };

    let mut start = move |ids: BTreeSet<String>| {
        starting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = SelectionBody {
                    group_ids: ids.into_iter().collect(),
                };
                let saved = crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    "/integrations/contact-sync/selection",
                    &body,
                )
                .await;
                if let Err(e) = saved {
                    error.set(format!("Could not save the labels: {e}"));
                    starting.set(false);
                    return;
                }
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    "/integrations/contact-sync/runs",
                    &NoBody {},
                )
                .await
                {
                    Ok(_) => {
                        navigator.push(Route::SettingsGoogleContacts {});
                    }
                    Err(e) => {
                        error.set(format!(
                            "The labels are saved, but the import did not start: {e}"
                        ));
                        starting.set(false);
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (ids, navigator);
                starting.set(false);
            }
        });
    };

    let preview_snap = preview.read_unchecked().clone();

    rsx! {
        PageHeader {
            title: "Import Google Contacts",
            subtitle: "Choose which Google labels to bring into Mokosh. Nothing is written until you start the import, and nothing is ever written back to Google.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsGoogleContactsImport {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        ol { class: "mb-4 flex gap-4 text-sm", "aria-label": "Import steps",
            li {
                class: if step() == Step::Choose { "font-medium text-content" } else { "text-muted" },
                "aria-current": (step() == Step::Choose).then_some("step"),
                "1. Choose labels"
            }
            li {
                class: if step() == Step::Review { "font-medium text-content" } else { "text-muted" },
                "aria-current": (step() == Step::Review).then_some("step"),
                "2. Review and import"
            }
        }
        match (preview_snap, step()) {
            (None, _) => rsx! {
                DetailSkeleton {}
                p { class: "sr-only", role: "status", "Reading the account's labels" }
            },
            (Some(Err(e)), _) => rsx! {
                Card { ErrorBanner { "Could not read the Google account: {e}" } }
            },
            (Some(Ok(data)), Step::Choose) => {
                let rows = label_rows(&data);
                let running = estimate(&data.records, &chosen);
                let nothing_chosen = chosen.is_empty();
                rsx! {
                    Card {
                        fieldset { class: "space-y-4",
                            legend { class: "text-base font-medium text-content",
                                "Which contacts belong in the CRM?"
                            }
                            p { class: "text-sm text-muted",
                                "Pick the labels your business contacts carry, such as Clients. Figures are what importing each label on its own would do."
                            }
                            if rows.is_empty() {
                                StatusBanner { tone: BannerTone::Info,
                                    "This account has no labels. Add a label to your business contacts in Google Contacts, then come back."
                                }
                            }
                            for row in rows.clone() {
                                div {
                                    key: "{row.id}",
                                    class: if row.everything { "mt-6 border-t border-line pt-4" } else { "" },
                                    if row.everything {
                                        p { class: "mb-2 text-sm text-muted",
                                            "Or import everything. Not recommended: this brings in every contact in the account, personal ones included."
                                        }
                                    }
                                    Checkbox {
                                        name: "label-{row.id}",
                                        label: format!(
                                            "{}{}",
                                            row.name,
                                            row.member_count.map(|n| format!(" ({n})")).unwrap_or_default()
                                        ),
                                        checked: chosen.contains(&row.id),
                                        disabled: starting(),
                                        help: label_help(&row.counts),
                                        onchange: {
                                            let id = row.id.clone();
                                            move |e: FormEvent| {
                                                let mut next = selected().unwrap_or_default();
                                                if e.checked() {
                                                    next.insert(id.clone());
                                                } else {
                                                    next.remove(&id);
                                                }
                                                selected.set(Some(next));
                                            }
                                        },
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
                                    "Choose at least one label."
                                } else {
                                    "{summary(&running)}"
                                }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                Button {
                                    disabled: nothing_chosen || !can_mutate,
                                    onclick: move |_| review(selected().unwrap_or_default()),
                                    data_testid: "contact-sync-import-review",
                                    "Review import"
                                }
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| { navigator.push(Route::SettingsGoogleContacts {}); },
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
            (Some(Ok(data)), Step::Review) => {
                let names: Vec<String> = label_rows(&data)
                    .into_iter()
                    .filter(|r| chosen.contains(&r.id))
                    .map(|r| r.name)
                    .collect();
                let exact_snap = exact();
                rsx! {
                    Card {
                        div { class: "space-y-4",
                            h2 {
                                class: "text-base font-medium text-content focus:outline-none",
                                tabindex: "-1",
                                onmounted: move |e| async move {
                                    // Moving to the review step moves focus to
                                    // it, so a keyboard user is not left on a
                                    // button that no longer exists.
                                    let _ = e.set_focus(true).await;
                                },
                                "Review the import"
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
                                        p { class: "text-sm text-content", "{summary(&totals)}" }
                                    },
                                }
                            }
                            ul { class: "list-disc space-y-1 pl-5 text-sm text-muted",
                                li { "No contact is merged on a name alone. Anything short of an exact email match waits in the review queue." }
                                li { "Company names are kept as text; a matching company is suggested, never linked for you." }
                                li { "The import runs on the server. You can leave this page; progress shows on the Google Contacts card." }
                            }
                            div { class: "flex flex-wrap gap-3 pt-2",
                                Button {
                                    disabled: starting()
                                        || !can_mutate
                                        || !matches!(exact(), Some(Ok(_))),
                                    onclick: move |_| start(selected().unwrap_or_default()),
                                    data_testid: "contact-sync-import-start",
                                    if starting() { "Starting…" } else { "Start import" }
                                }
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    disabled: starting(),
                                    onclick: move |_| step.set(Step::Choose),
                                    "Back"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The per-label figures under a checkbox.
pub fn label_help(counts: &Totals) -> String {
    if counts.contacts == 0 {
        return "Nobody in the account carries this label.".to_string();
    }
    let mut parts = vec![
        format!("{} new", counts.create),
        format!("{} to link", counts.link),
        format!("{} to review", counts.review),
    ];
    if counts.imported > 0 {
        parts.push(format!("{} already imported", counts.imported));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview() -> Preview {
        serde_json::from_str(
            r#"{
                "groups": [
                    {"id": "contactGroups/myContacts", "name": "My Contacts", "member_count": 2000},
                    {"id": "contactGroups/zeta", "name": "zeta", "member_count": 1},
                    {"id": "contactGroups/clients", "name": "Clients", "member_count": 3},
                    {"id": "contactGroups/starred", "name": "Starred", "member_count": 1}
                ],
                "records": [
                    {"group_ids": ["contactGroups/myContacts", "contactGroups/clients"], "outcome": "create"},
                    {"group_ids": ["contactGroups/myContacts", "contactGroups/clients", "contactGroups/starred"], "outcome": "link"},
                    {"group_ids": ["contactGroups/myContacts", "contactGroups/clients"], "outcome": "review"},
                    {"group_ids": ["contactGroups/myContacts", "contactGroups/zeta"], "outcome": "imported"},
                    {"group_ids": ["contactGroups/myContacts"], "outcome": "create"}
                ],
                "totals": {"contacts": 5}
            }"#,
        )
        .expect("preview")
    }

    /// Own labels by name, then Starred, then everything last and marked.
    #[test]
    fn everything_is_offered_last_and_set_apart() {
        let rows = label_rows(&preview());
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "contactGroups/clients",
                "contactGroups/zeta",
                STARRED,
                EVERYTHING
            ]
        );
        assert!(rows.last().unwrap().everything);
        assert_eq!(rows.last().unwrap().name, "Every contact in the account");
        assert!(rows[..3].iter().all(|r| !r.everything));
    }

    /// A record in two selected labels counts once.
    #[test]
    fn the_estimate_counts_each_record_once() {
        let p = preview();
        let both = BTreeSet::from(["contactGroups/clients".to_string(), STARRED.to_string()]);
        assert_eq!(
            estimate(&p.records, &both),
            Totals {
                contacts: 3,
                create: 1,
                link: 1,
                review: 1,
                imported: 0,
                excluded: 0
            }
        );
        assert_eq!(estimate(&p.records, &BTreeSet::new()), Totals::default());
    }

    #[test]
    fn per_label_figures_are_that_label_alone() {
        let rows = label_rows(&preview());
        let clients = rows
            .iter()
            .find(|r| r.id == "contactGroups/clients")
            .unwrap();
        assert_eq!(label_help(&clients.counts), "1 new, 1 to link, 1 to review");
        let zeta = rows.iter().find(|r| r.id == "contactGroups/zeta").unwrap();
        assert_eq!(
            label_help(&zeta.counts),
            "0 new, 0 to link, 0 to review, 1 already imported"
        );
    }

    #[test]
    fn the_summary_states_create_link_and_review() {
        let totals = Totals {
            contacts: 40,
            create: 30,
            link: 8,
            review: 2,
            imported: 0,
            excluded: 0,
        };
        assert_eq!(
            summary(&totals),
            "Importing 40 contacts will create 30 new contacts, link 8 contacts already in Mokosh and put 2 contacts in the review queue for a person to decide."
        );
        let one = Totals {
            contacts: 2,
            create: 1,
            link: 0,
            review: 0,
            imported: 1,
            excluded: 1,
        };
        let s = summary(&one);
        assert!(s.contains("create 1 new contact,"), "{s}");
        assert!(s.contains("and update 1 contact imported before."), "{s}");
        assert!(
            s.ends_with(
                "1 contact unlinked, removed on request or skipped by a reviewer is left out."
            ),
            "{s}"
        );
        assert!(summary(&Totals::default()).contains("change nothing"));
    }

    /// The flow is admin only, sends typed bodies, and starts the import by
    /// saving the selection and queuing a run, never by waiting on it.
    #[test]
    fn the_flow_is_admin_only_and_queues_rather_than_waits() {
        let src = include_str!("settings_contact_sync_import.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(
            !head.contains("json!("),
            "request bodies are typed (MAPPS-685)"
        );
        let save = head
            .find("\"/integrations/contact-sync/selection\"")
            .expect("saves the selection");
        let queue = head
            .find("\"/integrations/contact-sync/runs\"")
            .expect("queues a run");
        assert!(
            save < queue,
            "the selection is saved before the run is queued"
        );
        assert!(head.contains("navigator.push(Route::SettingsGoogleContacts {})"));
        assert!(!head.contains("animate-spin"), "no unconditional motion");
    }
}

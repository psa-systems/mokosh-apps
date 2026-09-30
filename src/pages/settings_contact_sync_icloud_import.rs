//! Settings > Integrations > iCloud Contacts > Import (PMS-1409, server PMS-1341).
//!
//! The Google picker's sibling. It reuses that page's DTOs and its pure
//! arithmetic ([`Preview`], [`estimate`], [`summary`], [`label_help`]) because
//! the preview response and the counting are the same; what is its own is the
//! copy and one row that behaves differently.
//!
//! # Ungrouped is offered, "everything" is not
//!
//! Google's picker sets "My Contacts" apart as the whole address book. iCloud has
//! no such group: an Apple address book routinely holds contacts in no group at
//! all, so the server offers `mokosh:ungrouped` beside the real groups
//! (PMS-1290's `UNGROUPED_ID`), and it is listed last and set apart for the same
//! reason Google's is - it is the row that brings in whatever was not filed, so
//! choosing it should be a decision rather than a default.
//!
//! # Every request names the provider
//!
//! The preview, the selection and the run are the SHARED routes, which default to
//! Google (server PMS-1409). Every call from this page carries
//! `?provider=icloud`, or it would read, save and import through the tenant's
//! Google connection instead.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{
    use_page_title, BannerTone, Button, ButtonVariant, Card, Checkbox, DetailSkeleton, ErrorBanner,
    PageHeader, StatusBanner,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::pages::settings_contact_sync_import::{estimate, label_help, summary, Preview, Totals};
use crate::Route;

/// The server's group for records in no group at all (`UNGROUPED_ID`).
pub const UNGROUPED: &str = "mokosh:ungrouped";

const PREVIEW_PATH: &str = "/integrations/contact-sync/preview?provider=icloud";
const SELECTION_PATH: &str = "/integrations/contact-sync/selection?provider=icloud";
const RUNS_PATH: &str = "/integrations/contact-sync/runs?provider=icloud";
const STATUS_PATH: &str = "/integrations/contact-sync";

/// One iCloud group as the picker lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRow {
    pub id: String,
    pub name: String,
    pub member_count: Option<u32>,
    /// The records in no group, which is listed last and set apart.
    pub ungrouped: bool,
    pub counts: Totals,
}

/// The groups in the order they are offered: the account's own by name, then the
/// ungrouped row last.
pub fn group_rows(preview: &Preview) -> Vec<GroupRow> {
    let mut rows: Vec<GroupRow> = preview
        .groups
        .iter()
        .map(|g| GroupRow {
            id: g.id.clone(),
            name: if g.id == UNGROUPED {
                "Contacts in no group".to_string()
            } else {
                g.name.clone()
            },
            member_count: g.member_count,
            ungrouped: g.id == UNGROUPED,
            counts: estimate(&preview.records, &BTreeSet::from([g.id.clone()])),
        })
        .collect();
    rows.sort_by(|a, b| {
        a.ungrouped
            .cmp(&b.ungrouped)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
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
pub fn ICloudContactsImportPage() -> Element {
    use_page_title("Import iCloud Contacts");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Import iCloud Contacts" } };
    }
    rsx! { ICloudContactsImportBody {} }
}

#[component]
fn ICloudContactsImportBody() -> Element {
    // The saved selection, so changing groups later starts from it.
    let overview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<crate::pages::settings_contact_sync::Overview>(
            STATUS_PATH,
        )
        .await
        .inspect_err(|e| tracing::error!("contact sync status load failed: {e}"))
        .ok()
    });
    let preview = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::post_authed::<Preview, _>(
            PREVIEW_PATH,
            &PreviewBody { group_ids: None },
        )
        .await
        .map_err(|e| {
            tracing::error!("icloud contact sync preview failed: {e}");
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

    // Seed from the saved selection once it is known. In an effect rather than
    // the render body: writing a signal while rendering re-renders forever.
    use_effect(move || {
        if let Some(Some(o)) = overview.read().as_ref() {
            if selected.peek().is_none() {
                let seeded: BTreeSet<String> = o
                    .icloud_connection
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
                let result =
                    crate::hooks::fetch::api::post_authed::<Preview, _>(PREVIEW_PATH, &body)
                        .await
                        .map(|p| p.totals);
                exact.set(Some(result.map_err(|e| e.to_string())));
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
                if let Err(e) = crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                    SELECTION_PATH,
                    &body,
                )
                .await
                {
                    error.set(format!("Could not save the groups: {e}"));
                    starting.set(false);
                    return;
                }
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    RUNS_PATH,
                    &NoBody {},
                )
                .await
                {
                    Ok(_) => {
                        navigator.push(Route::SettingsICloudContacts {});
                    }
                    Err(e) => {
                        error.set(format!(
                            "The groups are saved, but the import did not start: {e}"
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
            title: "Import iCloud Contacts",
            subtitle: "Choose which iCloud groups to bring into Mokosh. Nothing is written until you start the import, and nothing is ever written back to iCloud.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsICloudContactsImport {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        ol { class: "mb-4 flex gap-4 text-sm", "aria-label": "Import steps",
            li {
                class: if step() == Step::Choose { "font-medium text-content" } else { "text-muted" },
                "aria-current": (step() == Step::Choose).then_some("step"),
                "1. Choose groups"
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
                p { class: "sr-only", role: "status", "Reading the account's groups" }
            },
            (Some(Err(e)), _) => rsx! {
                Card { ErrorBanner { "Could not read the iCloud account: {e}" } }
            },
            (Some(Ok(data)), Step::Choose) => {
                let rows = group_rows(&data);
                let running = estimate(&data.records, &chosen);
                let nothing_chosen = chosen.is_empty();
                rsx! {
                    Card {
                        fieldset { class: "space-y-4",
                            legend { class: "text-base font-medium text-content",
                                "Which contacts belong in the CRM?"
                            }
                            p { class: "text-sm text-muted",
                                "Pick the groups your business contacts are in. Figures are what importing each group on its own would do."
                            }
                            if rows.is_empty() {
                                StatusBanner { tone: BannerTone::Info,
                                    "This account has no groups. Put your business contacts in a group in Contacts on a Mac, iPhone or iCloud.com, then come back."
                                }
                            }
                            for row in rows.clone() {
                                div {
                                    key: "{row.id}",
                                    class: if row.ungrouped { "mt-6 border-t border-line pt-4" } else { "" },
                                    if row.ungrouped {
                                        p { class: "mb-2 text-sm text-muted",
                                            "Or bring in the contacts that are in no group at all. An iCloud address book usually holds plenty, personal ones included."
                                        }
                                    }
                                    Checkbox {
                                        name: "group-{row.id}",
                                        label: format!(
                                            "{}{}",
                                            row.name,
                                            row.member_count.map(|n| format!(" ({n})")).unwrap_or_default()
                                        ),
                                        checked: chosen.contains(&row.id),
                                        disabled: starting(),
                                        help: label_help(&row.counts),
                                        onchange: move |e: FormEvent| {
                                            let mut next = selected().unwrap_or_default();
                                            if e.value() == "true" {
                                                next.insert(row.id.clone());
                                            } else {
                                                next.remove(&row.id);
                                            }
                                            selected.set(Some(next));
                                        },
                                    }
                                }
                            }
                        }
                        p { class: "mt-4 text-sm text-muted", "aria-live": "polite",
                            "data-testid": "icloud-import-running-total",
                            if nothing_chosen {
                                "Nothing chosen yet, so nothing would be imported."
                            } else {
                                "{summary(&running)}"
                            }
                        }
                        div { class: "mt-4",
                            Button {
                                disabled: nothing_chosen || starting() || !can_mutate,
                                onclick: move |_| review(chosen.clone()),
                                data_testid: "icloud-import-review",
                                "Review import"
                            }
                        }
                    }
                }
            }
            (Some(Ok(_)), Step::Review) => {
                let exact_snap = exact();
                let chosen_now = chosen.clone();
                rsx! {
                    Card {
                        match exact_snap {
                            None => rsx! {
                                p { class: "text-sm text-muted", role: "status",
                                    "Working out exactly what this selection would do…"
                                }
                            },
                            Some(Err(e)) => rsx! {
                                ErrorBanner { "Could not check the selection: {e}" }
                            },
                            Some(Ok(totals)) => rsx! {
                                p { class: "text-sm text-content", "data-testid": "icloud-import-summary",
                                    "{summary(&totals)}"
                                }
                                p { class: "mt-2 text-sm text-muted",
                                    "The import runs in the background. You can leave the page once it starts."
                                }
                            },
                        }
                        div { class: "mt-4 flex flex-wrap gap-3",
                            Button {
                                disabled: starting() || !can_mutate || !matches!(exact(), Some(Ok(_))),
                                onclick: move |_| start(chosen_now.clone()),
                                data_testid: "icloud-import-start",
                                if starting() { "Starting…" } else { "Start the import" }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: starting(),
                                onclick: move |_| step.set(Step::Choose),
                                "Back to groups"
                            }
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

    fn preview() -> Preview {
        serde_json::from_str(
            r#"{
                "groups": [
                    {"id": "mokosh:ungrouped", "name": "Ungrouped", "member_count": 900},
                    {"id": "GGGG-2", "name": "zeta", "member_count": 1},
                    {"id": "GGGG-1", "name": "Clients", "member_count": 3}
                ],
                "records": [
                    {"group_ids": ["GGGG-1"], "outcome": "create"},
                    {"group_ids": ["GGGG-1"], "outcome": "link"},
                    {"group_ids": ["GGGG-1", "GGGG-2"], "outcome": "review"},
                    {"group_ids": ["mokosh:ungrouped"], "outcome": "create"}
                ],
                "totals": {"contacts": 4}
            }"#,
        )
        .expect("preview")
    }

    /// The account's own groups by name, and the ungrouped row last and set
    /// apart: it is the one that brings in whatever nobody filed, so it should be
    /// a decision rather than the first thing a thumb lands on.
    #[test]
    fn the_ungrouped_row_is_offered_last() {
        let rows = group_rows(&preview());
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["Clients", "zeta", "Contacts in no group"]
        );
        assert!(rows.last().expect("a row").ungrouped);
        assert!(
            rows.iter().take(2).all(|r| !r.ungrouped),
            "a real group is never the ungrouped row"
        );
    }

    /// Each row's figures are what importing THAT group alone would do, counted
    /// from the one read, so a record in two groups is counted under each.
    #[test]
    fn a_rows_figures_are_that_group_on_its_own() {
        let rows = group_rows(&preview());
        let clients = rows.iter().find(|r| r.name == "Clients").expect("Clients");
        assert_eq!(
            (
                clients.counts.contacts,
                clients.counts.create,
                clients.counts.link,
                clients.counts.review
            ),
            (3, 1, 1, 1)
        );
        let zeta = rows.iter().find(|r| r.name == "zeta").expect("zeta");
        assert_eq!(zeta.counts.contacts, 1, "the shared record counts here too");
    }

    /// A record in two chosen groups is one contact, not two: the running total
    /// counts records and not memberships.
    #[test]
    fn the_running_total_counts_a_record_once() {
        let data = preview();
        let both: BTreeSet<String> = ["GGGG-1".to_string(), "GGGG-2".to_string()].into();
        assert_eq!(estimate(&data.records, &both).contacts, 3);
    }

    /// Every call this page makes names the provider. Without it the shared
    /// routes default to Google, so the page would read, save and import through
    /// the tenant's Google connection while saying iCloud.
    #[test]
    fn every_shared_route_names_the_provider() {
        for path in [PREVIEW_PATH, SELECTION_PATH, RUNS_PATH] {
            assert!(path.ends_with("?provider=icloud"), "{path}");
        }
        let src = include_str!("settings_contact_sync_icloud_import.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("use_is_admin"), "admin gate");
        assert!(
            !head.contains("\"/integrations/contact-sync/preview\""),
            "a bare shared path would act on the Google connection"
        );
        assert!(!head.contains("\"/integrations/contact-sync/selection\""));
        assert!(!head.contains("\"/integrations/contact-sync/runs\""));
    }

    /// Nothing chosen imports nothing, which the page says rather than showing a
    /// zero that reads like a failure.
    #[test]
    fn nothing_chosen_is_stated_and_not_counted() {
        let data = preview();
        let none: BTreeSet<String> = BTreeSet::new();
        assert_eq!(estimate(&data.records, &none), Totals::default());
        let src = include_str!("settings_contact_sync_icloud_import.rs");
        assert!(src.contains("Nothing chosen yet, so nothing would be imported."));
    }
}

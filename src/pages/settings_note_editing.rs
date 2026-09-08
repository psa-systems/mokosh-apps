//! Settings > Tickets > Note Editing (MAPPS-749).
//!
//! Who may edit a ticket note, the tenant setting `tickets/note_editing`
//! (PMS-974). Until this the policy was reachable only through
//! `PUT /api/v1/settings` with an admin bearer, so an MSP whose owner wanted
//! managers to be able to correct a colleague's note had nowhere in this app
//! to say so, and the tenant that wanted notes append-only had nowhere at all.
//!
//! Admin only, like the write it drives. The three policies are a closed set
//! the server refuses anything outside of, so they are written out here rather
//! than fetched; what is fetched is which one is in force. A tenant with no
//! row set is on `author_or_admin`, the server's own default, and the page
//! says as much rather than showing an empty select.
//!
//! The policy is the WHO half only. A note the customer wrote through the
//! portal, and a public note already emailed, refuse the edit whatever the
//! policy says, which is the note's own state and is stated on the page so an
//! admin does not read the setting as more than it is.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{use_page_title, Card, ErrorBanner, PageHeader, Select, SelectOption};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// One row of `GET /api/v1/settings` (`TenantSettingResponse`), narrowed to
/// what this page needs to find its own.
#[derive(Clone, Debug, Deserialize)]
struct TenantSetting {
    #[serde(default)]
    category: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: serde_json::Value,
}

/// The setting this page owns.
const CATEGORY: &str = "tickets";
const KEY: &str = "note_editing";

/// The server's default when no row exists (`NoteEditPolicy::default`).
const DEFAULT_POLICY: &str = "author_or_admin";

/// The closed set `validate_setting_value` accepts, with what each one means.
/// Written out because the server refuses anything else: a value invented here
/// would be a 422 the admin cannot act on.
const POLICIES: &[(&str, &str, &str)] = &[
    (
        "off",
        "Nobody",
        "Notes are append-only. Even the person who wrote one cannot change it; a correction is a new note.",
    ),
    (
        DEFAULT_POLICY,
        "The author, or an administrator",
        "The default. Everyone corrects their own notes, and an administrator can correct anyone's.",
    ),
    (
        "author_or_manager",
        "The author, a manager, or an administrator",
        "As above, and managers can also correct the notes of the people they manage.",
    ),
];

/// The policy in force in `settings`, or the server's default when no row is
/// set. A stored value this build does not know reads as the default too,
/// which is what the server does with one.
fn policy_in(settings: &[TenantSetting]) -> &'static str {
    settings
        .iter()
        .find(|s| s.category == CATEGORY && s.key == KEY)
        .and_then(|s| s.value.as_str())
        .and_then(|stored| POLICIES.iter().find(|(name, _, _)| *name == stored))
        .map(|(name, _, _)| *name)
        .unwrap_or(DEFAULT_POLICY)
}

#[component]
pub fn NoteEditingSettingsPage() -> Element {
    use_page_title("Note Editing");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Note Editing" } };
    }
    rsx! { NoteEditingSettingsBody {} }
}

#[component]
fn NoteEditingSettingsBody() -> Element {
    let mut settings = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_all_authed::<TenantSetting>("/settings")
            .await
            .inspect_err(|e| tracing::error!("tenant settings load failed: {e}"))
            .ok()
    });
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();
    let snap = settings.read_unchecked().clone();

    rsx! {
        PageHeader {
            title: "Note Editing",
            subtitle: "Who may correct a ticket note after it was posted. Every edit is recorded on the ticket's history with the text it replaced.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsNoteEditing {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { "{error}" }
        }
        Card {
            match snap {
                None => rsx! { p { class: "p-6 text-sm text-subtle", "Loading…" } },
                Some(None) => rsx! {
                    p { class: "p-6 text-sm text-red-600 dark:text-red-300", "Could not load the note editing policy." }
                },
                Some(Some(rows)) => {
                    let current = policy_in(&rows);
                    let described = POLICIES
                        .iter()
                        .find(|(name, _, _)| *name == current)
                        .map(|(_, _, meaning)| *meaning)
                        .unwrap_or_default();
                    rsx! {
                        div { class: "p-6 space-y-4",
                            Select {
                                name: "note_editing",
                                label: "Who may edit a note",
                                value: current.to_string(),
                                disabled: saving() || !can_mutate,
                                options: POLICIES
                                    .iter()
                                    .map(|(name, label, _)| SelectOption::new(*name, *label))
                                    .collect::<Vec<_>>(),
                                onchange: move |e: FormEvent| {
                                    let next = e.value();
                                    saving.set(true);
                                    error.set(String::new());
                                    spawn(async move {
                                        #[cfg(feature = "app")]
                                        {
                                            let body = serde_json::json!({
                                                "category": CATEGORY,
                                                "key": KEY,
                                                "value": next,
                                            });
                                            match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>("/settings", &body).await {
                                                Ok(_) => settings.restart(),
                                                Err(e) => error.set(format!("Could not change the policy: {e}")),
                                            }
                                        }
                                        #[cfg(not(feature = "app"))]
                                        let _ = next;
                                        saving.set(false);
                                    });
                                },
                            }
                            p { class: "text-sm text-muted", "{described}" }
                            p { class: "text-sm text-subtle",
                                "Two kinds of note refuse the edit whatever this says: one the customer wrote through the portal, and a public note that was already emailed, because the customer holds the original. A note logged against a time entry is edited through that entry."
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
    use super::{policy_in, TenantSetting, DEFAULT_POLICY, POLICIES};

    fn rows(json: &str) -> Vec<TenantSetting> {
        serde_json::from_str(json).expect("deserialise settings")
    }

    /// A tenant that never opened this page is on the server's default, and
    /// the select has to show that rather than an empty value: the server
    /// applies a policy whether or not a row exists.
    #[test]
    fn an_unset_tenant_reads_as_the_default() {
        assert_eq!(policy_in(&[]), DEFAULT_POLICY);
        assert_eq!(
            policy_in(&rows(
                r#"[{"category":"timesheets","key":"track_breaks","value":true}]"#
            )),
            DEFAULT_POLICY,
            "and another category's row is not this one"
        );
    }

    /// The stored value is what the select shows.
    #[test]
    fn the_stored_policy_is_the_one_shown() {
        for (name, _, _) in POLICIES {
            let stored =
                format!(r#"[{{"category":"tickets","key":"note_editing","value":"{name}"}}]"#);
            assert_eq!(policy_in(&rows(&stored)), *name);
        }
    }

    /// A value this build does not know reads as the default, the same answer
    /// the server gives one, rather than a select with nothing selected.
    #[test]
    fn an_unknown_stored_value_reads_as_the_default() {
        assert_eq!(
            policy_in(&rows(
                r#"[{"category":"tickets","key":"note_editing","value":"anyone"}]"#
            )),
            DEFAULT_POLICY
        );
        assert_eq!(
            policy_in(&rows(
                r#"[{"category":"tickets","key":"note_editing","value":true}]"#
            )),
            DEFAULT_POLICY,
            "and so does one of the wrong type"
        );
    }

    /// The page is admin only and writes the server's body shape to the
    /// generic settings endpoint.
    #[test]
    fn the_page_is_admin_only_and_writes_the_settings_body() {
        let src = include_str!("settings_note_editing.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(head.contains("\"category\": CATEGORY,"));
        assert!(head.contains("put_authed::<serde_json::Value, _>(\"/settings\", &body)"));
        assert!(
            head.contains("Ok(_) => settings.restart(),"),
            "a saved policy re-reads rather than trusting the local value"
        );
    }
}

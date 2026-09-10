//! Settings > Timesheets > Segment Editing (MAPPS-754).
//!
//! Who may correct a recorded work-day segment, the tenant setting
//! `timesheets/segment_editing` (PMS-1145). Until this the policy was
//! reachable only through `PUT /api/v1/settings` with an admin bearer, so an
//! MSP that wanted attendance append-only, or wanted managers able to fix a
//! colleague's clock, had nowhere in this app to say so.
//!
//! `settings_note_editing.rs` is the template and the parallel is followed
//! deliberately rather than invented again: admin only, the closed set written
//! out because the server refuses anything else, and a tenant with no row
//! shown the effective default rather than an empty select.
//!
//! One difference from that page is worth saying out loud on this one. Note
//! editing is the WHO half of a rule whose other half is the note's own
//! state: a customer's words and an emailed public note refuse the edit
//! whatever the policy says. A segment has no such state, so this policy is
//! the whole rule, and an admin reading across from the other page does not
//! have to wonder what else might refuse.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{use_page_title, Card, ErrorBanner, PageHeader, Select, SelectOption};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

/// One row of `GET /api/v1/settings` (`TenantSettingResponse`), narrowed to
/// what this page needs to find its own.
/// `pub(crate)` because the Work day card reads the same rows to decide
/// whether to offer a correction (MAPPS-754). One decoder and one reader, so
/// the two surfaces cannot disagree about which policy is in force.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TenantSetting {
    #[serde(default)]
    category: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: serde_json::Value,
}

/// The setting this page owns.
pub(crate) const CATEGORY: &str = "timesheets";
pub(crate) const KEY: &str = "segment_editing";

/// The server's default when no row exists (`SegmentEditPolicy::default`).
pub(crate) const DEFAULT_POLICY: &str = "owner_or_admin";

/// The closed set `validate_setting_value` accepts, with what each one means.
/// Written out because the server refuses anything else: a value invented here
/// would be a 422 the admin cannot act on.
pub(crate) const POLICIES: &[(&str, &str, &str)] = &[
    (
        "off",
        "Nobody",
        "The clock is append-only. Nobody corrects a segment, including the person whose day it is.",
    ),
    (
        DEFAULT_POLICY,
        "The person, or an administrator",
        "The default. Everyone corrects their own clock, and an administrator can correct anyone's.",
    ),
    (
        "owner_or_manager",
        "The person, a manager, or an administrator",
        "As above, and managers can also correct the clock of the people they manage.",
    ),
];

/// The policy in force in `settings`, or the server's default when no row is
/// set. A stored value this build does not know reads as the default too,
/// which is what the server does with one.
pub(crate) fn policy_in(settings: &[TenantSetting]) -> &'static str {
    settings
        .iter()
        .find(|s| s.category == CATEGORY && s.key == KEY)
        .and_then(|s| s.value.as_str())
        .and_then(|stored| POLICIES.iter().find(|(name, _, _)| *name == stored))
        .map(|(name, _, _)| *name)
        .unwrap_or(DEFAULT_POLICY)
}

#[component]
pub fn SegmentEditingSettingsPage() -> Element {
    use_page_title("Segment Editing");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Segment Editing" } };
    }
    rsx! { SegmentEditingSettingsBody {} }
}

#[component]
fn SegmentEditingSettingsBody() -> Element {
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
            title: "Segment Editing",
            subtitle: "Who may correct a clock-in or clock-out after it was recorded. Every correction is recorded with the times it replaced.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsSegmentEditing {} }
            },
        }
        if !error().is_empty() {
            ErrorBanner { "{error}" }
        }
        Card {
            match snap {
                None => rsx! { p { class: "p-6 text-sm text-subtle", "Loading…" } },
                Some(None) => rsx! {
                    p { class: "p-6 text-sm text-red-600 dark:text-red-300", "Could not load the segment editing policy." }
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
                                name: "segment_editing",
                                label: "Who may correct a clock entry",
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
                            // Said here because an admin arriving from Note
                            // Editing will expect a second half to the rule,
                            // and there isn't one.
                            p { class: "text-sm text-subtle",
                                "This is the whole rule. Nothing else refuses a correction: a clock entry has no state of its own that can, unlike a ticket note. Choosing Nobody means an operator has to fix a mis-clock in the database."
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
            "another key in the same category is not this one"
        );
        assert_eq!(
            policy_in(&rows(
                r#"[{"category":"tickets","key":"note_editing","value":"off"}]"#
            )),
            DEFAULT_POLICY,
            "and neither is the note policy, whose values overlap in shape"
        );
    }

    /// The stored value is what the select shows.
    #[test]
    fn the_stored_policy_is_the_one_shown() {
        for (name, _, _) in POLICIES {
            let stored = format!(
                r#"[{{"category":"timesheets","key":"segment_editing","value":"{name}"}}]"#
            );
            assert_eq!(policy_in(&rows(&stored)), *name);
        }
    }

    /// A value this build does not know reads as the default, the same answer
    /// the server gives one, rather than a select with nothing selected.
    #[test]
    fn an_unknown_stored_value_reads_as_the_default() {
        assert_eq!(
            policy_in(&rows(
                r#"[{"category":"timesheets","key":"segment_editing","value":"anyone"}]"#
            )),
            DEFAULT_POLICY
        );
    }

    /// The names are the server's, not this page's. A spelling invented here
    /// is a 422 an admin cannot act on, and the note-editing set is the near
    /// miss to guard against: same shape, different words.
    #[test]
    fn the_policy_names_are_the_ones_the_server_accepts() {
        let names: Vec<&str> = POLICIES.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names, vec!["off", "owner_or_admin", "owner_or_manager"]);
        assert!(
            !names.contains(&"author_or_admin"),
            "author_* belongs to note editing; a segment has an owner, not an author"
        );
    }
}

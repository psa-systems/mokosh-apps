//! PMS-481: "My approvals" page.
//!
//! Lists every pending approval the signed-in user can decide
//! (either named approver or holder of the assigned role) and lets
//! the user approve / reject inline. Reads
//! `GET /api/v1/approvals/pending`; POSTs decisions to
//! `POST /api/v1/approvals/{id}/decision`.
//!
//! The ticket-detail "Approvals" tab + "Request approval" modal +
//! top-bar pending-decisions badge land in follow-up tickets. Each
//! of those touches existing surfaces (the ticket-detail tabbed
//! section + the AppLayout chrome) which would balloon this PR; the
//! /approvals page on its own is the highest-leverage standalone
//! slice because every other surface eventually links here.

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, AlertType, Badge, BadgeVariant, Button, ButtonVariant, Card, PageHeader,
};
use crate::Route;

/// Polymorphic approval row as `/approvals/pending` returns it
/// (PMS-470 widened the schema). Every field tolerates a missing key
/// so an older server that pre-dates the polymorphic columns still
/// decodes - `target` defaults to `"ticket"` and `entity_id` falls
/// back to `ticket_id` when the wider columns are absent.
#[derive(Clone, Debug, Deserialize)]
struct PendingApproval {
    id: Uuid,
    #[serde(default = "default_target")]
    target: String,
    #[serde(default)]
    entity_id: Option<Uuid>,
    #[serde(default)]
    ticket_id: Option<Uuid>,
    /// PMS-940: the parent's human handle (a ticket number, a quote
    /// number). Null for the targets that have no number column and
    /// for a parent that has been deleted.
    #[serde(default)]
    entity_reference: Option<String>,
    /// PMS-940: the parent's title, or a time entry's duration and
    /// date. Null on a deleted parent, and absent entirely from a
    /// server that pre-dates PMS-940 - hence `default`.
    #[serde(default)]
    entity_label: Option<String>,
    #[serde(default)]
    requested_by_name: Option<String>,
    #[serde(default)]
    approver_user_name: Option<String>,
    #[serde(default)]
    approver_role: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    requested_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_target() -> String {
    "ticket".to_string()
}

impl PendingApproval {
    /// Resolved entity id, preferring the new polymorphic
    /// `entity_id` column with fallback to the legacy `ticket_id`.
    fn entity(&self) -> Option<Uuid> {
        self.entity_id.or(self.ticket_id)
    }

    /// PMS-940: the parent's handle, empty when the server sent none.
    /// Trimmed because a blank chip is worse than no chip.
    fn reference(&self) -> String {
        self.entity_reference
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    /// PMS-940: the parent's title, empty when the server sent none.
    fn label(&self) -> String {
        self.entity_label
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    /// PMS-940: what the chip shows when the server resolved neither a
    /// handle nor a title. Only a deleted parent reaches this - the
    /// server always resolves at least a title for a live one - and the
    /// full 36-character key buys nothing there, so show a prefix and
    /// leave the whole value to the tooltip. An approval with no
    /// entity id at all cannot happen against a current server, but it
    /// is what the payload's `Option` says, so it renders as a dash.
    fn fallback(&self) -> String {
        if !self.reference().is_empty() || !self.label().is_empty() {
            return String::new();
        }
        match self.entity() {
            Some(e) => e.to_string().chars().take(8).collect(),
            None => "-".to_string(),
        }
    }
}

#[component]
pub fn ApprovalsPage() -> Element {
    use_page_title("My Approvals");
    let mut version = use_signal(|| 0u32);
    let mut decision_error = use_signal(String::new);

    let mut pending_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-351: subscribe to reachability so the queue auto-refetches
        // the instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let _v = version.read();
        crate::hooks::fetch::api::get_authed::<Vec<PendingApproval>>("/approvals/pending")
            .await
            .ok()
    });

    let snap = pending_resource.read_unchecked();
    let rows: Vec<PendingApproval> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));

    // MAPPS-351: a failed load while the server is flagged down is an
    // outage, not an empty queue - show the honest unavailable state (which
    // keeps the nav + banner and offers the dashboard) instead of the
    // generic "could not load" line. A fetch that fails while the server is
    // still reachable (a 4xx) keeps the inline message below. Writes are
    // blocked while down; `can_mutate` disables the decision buttons.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "My Approvals".to_string() }
        };
    }

    let decide = move |id: Uuid, decision: &'static str| {
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({ "decision": decision });
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    &format!("/approvals/{id}/decision"),
                    &body,
                )
                .await
                {
                    Ok(_) => {
                        let label = if decision == "approve" {
                            "Approved"
                        } else {
                            "Rejected"
                        };
                        crate::hooks::toast::push_toast(AlertType::Success, label);
                        version += 1;
                        pending_resource.restart();
                    }
                    Err(e) => decision_error.set(format!("Could not record decision: {e}")),
                }
            }
        });
    };

    rsx! {
        PageHeader {
            title: "My Approvals",
            subtitle: "Pending decisions assigned to you (or to a role you hold)",
        }

        if !decision_error().is_empty() {
            Card { class: "mb-6 border-red-300 dark:border-red-700",
                p { class: "text-sm text-red-600 dark:text-red-300", "{decision_error}" }
            }
        }

        if loading {
            Card { p { class: "text-sm text-muted py-6 text-center", "Loading…" } }
        } else if fetch_failed {
            Card {
                p { class: "text-sm text-red-600 dark:text-red-300 py-6 text-center",
                    "Could not load pending approvals."
                }
            }
        } else if rows.is_empty() {
            Card {
                div { class: "py-10 text-center",
                    p { class: "text-sm text-muted",
                        "No pending approvals. You're all caught up."
                    }
                }
            }
        } else {
            div { class: "space-y-3",
                for row in rows.iter().cloned() {
                    {
                        let key = row.id.to_string();
                        let row_id = row.id;
                        let target = row.target.clone();
                        let entity = row.entity();
                        let requester = row
                            .requested_by_name
                            .clone()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| "(unknown requester)".to_string());
                        let approver_label = match (
                            row.approver_user_name.clone(),
                            row.approver_role.clone(),
                        ) {
                            (Some(n), _) if !n.trim().is_empty() => format!("To: {n}"),
                            (_, Some(r)) if !r.trim().is_empty() => format!("Role: {r}"),
                            _ => "(unassigned approver)".to_string(),
                        };
                        let when = row
                            .requested_at
                            .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                            .unwrap_or_default();
                        let notes = row.notes.clone().unwrap_or_default();
                        // Pretty target labels for the badge. Unknown
                        // targets fall through to the raw string so a
                        // future server-side surface still renders.
                        let target_label = match target.as_str() {
                            "ticket" => "Ticket",
                            "time_entry" => "Time entry",
                            "change_request" => "Change request",
                            "quote" => "Quote",
                            other => other,
                        };
                        // PMS-940: name the subject. The server
                        // resolves the parent's handle and title; a
                        // target with no number column sends only the
                        // title, and a parent that has been deleted
                        // sends neither.
                        let reference = row.reference();
                        let label = row.label();
                        let fallback = row.fallback();
                        let full_id = entity.map(|e| e.to_string()).unwrap_or_default();
                        let subject = rsx! {
                            if !reference.is_empty() {
                                span { class: "font-mono", "{reference}" }
                            }
                            if !label.is_empty() {
                                span { "{label}" }
                            }
                            if !fallback.is_empty() {
                                span {
                                    class: "font-mono text-subtle",
                                    title: "{full_id}",
                                    "{fallback}"
                                }
                            }
                        };
                        // Link to the parent ticket when target is
                        // ticket and we have an id. The other three
                        // targets render unlinked - there are still no
                        // client routes for them.
                        // MAPPS-632: routed `Link`, not a raw `<a href>` - the
                        // desktop webview refuses an internal navigation.
                        let entity_chip = match (target.as_str(), entity) {
                            ("ticket", Some(t)) => rsx! {
                                Link {
                                    to: Route::TicketDetail { id: t.to_string() },
                                    class: "text-sm text-accent hover:opacity-90 inline-flex items-baseline gap-2",
                                    {subject}
                                }
                            },
                            _ => rsx! {
                                span {
                                    class: "text-sm text-content inline-flex items-baseline gap-2",
                                    {subject}
                                }
                            },
                        };
                        rsx! {
                            Card { key: "{key}",
                                div { class: "flex items-start justify-between gap-4 flex-wrap",
                                    div { class: "min-w-0 flex-1",
                                        div { class: "flex items-center gap-2 mb-2 flex-wrap",
                                            Badge { variant: BadgeVariant::Yellow, "Pending" }
                                            Badge { variant: BadgeVariant::Gray, "{target_label}" }
                                            {entity_chip}
                                        }
                                        p { class: "text-sm text-content",
                                            strong { "From: " }
                                            "{requester}"
                                        }
                                        p { class: "text-xs text-subtle mt-1", "{approver_label}" }
                                        if !when.is_empty() {
                                            p { class: "text-xs text-subtle mt-1", "Requested {when}" }
                                        }
                                        if !notes.is_empty() {
                                            p { class: "mt-2 text-sm text-muted whitespace-pre-wrap",
                                                "{notes}"
                                            }
                                        }
                                    }
                                    div { class: "flex items-center gap-2",
                                        // MAPPS-351: block decisions while the server is
                                        // unreachable, with an explanatory tooltip, so a
                                        // click cannot silently fail (edits are discarded,
                                        // not queued - hold-and-replay is scaffolded in
                                        // crate::hooks::edit_queue for the local-first epic).
                                        if !can_mutate {
                                            span { class: "text-xs text-muted self-center mr-1",
                                                "Server unreachable" }
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            disabled: !can_mutate,
                                            title: (!can_mutate).then(|| "Can't record a decision while the server is unreachable".to_string()),
                                            onclick: move |_| decide(row_id, "reject"),
                                            "Reject"
                                        }
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            disabled: !can_mutate,
                                            title: (!can_mutate).then(|| "Can't record a decision while the server is unreachable".to_string()),
                                            onclick: move |_| decide(row_id, "approve"),
                                            "Approve"
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

#[cfg(test)]
mod mapps611_subject_tests {
    use super::PendingApproval;
    use serde_json::json;

    fn row(extra: serde_json::Value) -> PendingApproval {
        let mut base = json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "entity_id": "853a5a2f-a58e-44e5-9a80-f8f8852e1a93",
        });
        let map = base.as_object_mut().unwrap();
        for (k, v) in extra.as_object().unwrap() {
            map.insert(k.clone(), v.clone());
        }
        serde_json::from_value(base).expect("decode")
    }

    #[test]
    fn a_ticket_shows_its_number_and_title() {
        let r = row(json!({
            "entity_reference": "T000123",
            "entity_label": "Printer offline in Accounts",
        }));
        assert_eq!(r.reference(), "T000123");
        assert_eq!(r.label(), "Printer offline in Accounts");
        assert!(r.fallback().is_empty(), "a named subject needs no fallback");
    }

    #[test]
    fn a_target_with_no_number_shows_the_label_alone() {
        let r = row(json!({ "entity_label": "90 min on 2026-03-04" }));
        assert!(r.reference().is_empty());
        assert_eq!(r.label(), "90 min on 2026-03-04");
        assert!(r.fallback().is_empty());
    }

    #[test]
    fn a_deleted_parent_falls_back_to_a_short_id() {
        // The server resolves both columns through LEFT JOINs, so an
        // approval that outlived its parent arrives with neither.
        let r = row(json!({}));
        assert_eq!(r.fallback(), "853a5a2f");
    }

    #[test]
    fn a_server_predating_the_resolved_columns_still_decodes() {
        // Both fields are `#[serde(default)]`: an older server omits
        // them entirely rather than sending null, and the row must
        // still decode into the same fallback.
        let r: PendingApproval = serde_json::from_value(json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "ticket_id": "853a5a2f-a58e-44e5-9a80-f8f8852e1a93",
        }))
        .expect("decode without the PMS-940 fields");
        assert_eq!(r.fallback(), "853a5a2f");
    }
}

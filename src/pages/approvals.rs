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
    AlertType, AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, PageHeader,
};

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
}

#[component]
pub fn ApprovalsPage() -> Element {
    let mut version = use_signal(|| 0u32);
    let mut decision_error = use_signal(String::new);

    let mut pending_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
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

    let decide = move |id: Uuid, decision: &'static str| {
        spawn(async move {
            #[cfg(feature = "web")]
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
        AppLayout { title: "My Approvals",
            PageHeader {
                title: "My Approvals",
                subtitle: "Pending decisions assigned to you (or to a role you hold)",
            }

            if !decision_error().is_empty() {
                Card { class: "mb-4 border-red-300 dark:border-red-700",
                    p { class: "text-sm text-red-600 dark:text-red-300", "{decision_error}" }
                }
            }

            if loading {
                Card { p { class: "text-sm text-muted py-6 text-center", "Loading..." } }
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
                            // Link to the parent ticket when target is
                            // ticket and we have an id. For non-ticket
                            // targets we render the entity id verbatim
                            // (no client routes for those yet).
                            let entity_chip = match (target.as_str(), entity) {
                                ("ticket", Some(t)) => rsx! {
                                    a {
                                        href: "/tickets/{t}",
                                        class: "text-sm text-accent hover:opacity-90 font-mono",
                                        "{t}"
                                    }
                                },
                                (_, Some(e)) => rsx! {
                                    span { class: "text-sm text-content font-mono", "{e}" }
                                },
                                _ => rsx! { span { class: "text-sm text-subtle", "-" } },
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
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                onclick: move |_| decide(row_id, "reject"),
                                                "Reject"
                                            }
                                            Button {
                                                variant: ButtonVariant::Primary,
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
}

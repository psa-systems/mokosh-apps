//! MAPPS-875: owner-side grant management page.
//!
//! Lands at `/settings/sharing`. Owner-only surface. Two lists: pending
//! invitations (with Cancel per row) and active grants (with Revoke per
//! row). Fetches `GET /api/v1/grants?role=owner` from mokosh-server;
//! Cancel calls `DELETE /api/v1/grants/invitations/{id}` (PMS-1208
//! route); Revoke calls `DELETE /api/v1/grants/{id}` (MAPPS-875 route).
//!
//! Role-change on an active grant is a "Revoke and re-invite" flow: the
//! MVP calls Revoke, then opens the switcher's invite modal pre-filled
//! with the same email at the new role. A proper `PATCH /v1/grants/{id}`
//! is deferred to a follow-up ticket (bunyip's `mokosh_account_grants`
//! has no in-place update path today).
//!
//! Gate: `role.is_admin()` mirrors the sibling settings pages. The
//! server also gates every route on `RequireAdminUser`, so a non-admin
//! bypassing this SPA check still sees 403 from the fetch.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Button, ButtonVariant, Card, ContentUnavailable};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct OwnerOutbox {
    #[serde(default)]
    pending: Vec<PendingInvitationView>,
    #[serde(default)]
    active: Vec<ActiveGrantView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct PendingInvitationView {
    id: String,
    invitee_email: String,
    role: String,
    #[serde(default)]
    invited_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ActiveGrantView {
    id: String,
    #[serde(default)]
    grantee_email: Option<String>,
    #[serde(default)]
    grantee_name: Option<String>,
    role: String,
    #[serde(default)]
    granted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// PMS-1162 role vocabulary in title case for display.
fn humanise_role(role: &str) -> String {
    match role {
        "admin" => "Admin".to_string(),
        "manager" => "Manager".to_string(),
        "technician" => "Technician".to_string(),
        "finance" => "Finance".to_string(),
        "read_only" => "Read only".to_string(),
        other => other.to_string(),
    }
}

fn fmt_ts(ts: &Option<chrono::DateTime<chrono::Utc>>) -> String {
    ts.as_ref()
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

#[component]
pub fn SettingsSharingPage() -> Element {
    // MAPPS-602: hooks fire BEFORE the not-admin early return so the
    // component keeps a stable hook count across renders.
    let auth = crate::hooks::auth::use_auth();
    let refresh_counter = use_signal(|| 0u32);
    let outbox: Resource<Option<OwnerOutbox>> = use_resource(move || {
        let _bump = refresh_counter.read();
        async move {
            #[cfg(feature = "app")]
            {
                crate::hooks::fetch::api::get_authed::<OwnerOutbox>("/grants?role=owner")
                    .await
                    .inspect_err(|e| tracing::error!("owner outbox load failed: {e}"))
                    .ok()
            }
            #[cfg(not(feature = "app"))]
            {
                None
            }
        }
    });
    let saving = use_signal(|| false);
    let error: Signal<String> = use_signal(String::new);

    let is_admin = auth.read().is_admin();
    if !is_admin {
        return rsx! {
            ContentUnavailable {
                title: "Manage sharing".to_string(),
                show_dashboard_link: true,
            }
        };
    }

    let snap = outbox.read_unchecked();
    let loading = (*snap).is_none();
    let outbox_data: OwnerOutbox = match &*snap {
        Some(Some(o)) => o.clone(),
        _ => OwnerOutbox {
            pending: Vec::new(),
            active: Vec::new(),
        },
    };
    let unreachable = matches!(&*snap, Some(None));

    let cancel_invitation = {
        let mut refresh_counter = refresh_counter;
        let mut error = error;
        let mut saving = saving;
        move |id: String| {
            if saving() {
                return;
            }
            saving.set(true);
            error.set(String::new());
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/invitations/{id}");
                    match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                        Ok(_) => {
                            *refresh_counter.write() += 1;
                        }
                        Err(e) => error.set(e.user_message()),
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = id;
                }
                saving.set(false);
            });
        }
    };

    let revoke_grant = {
        let mut refresh_counter = refresh_counter;
        let mut error = error;
        let mut saving = saving;
        move |id: String| {
            if saving() {
                return;
            }
            saving.set(true);
            error.set(String::new());
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/{id}");
                    match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                        Ok(_) => {
                            *refresh_counter.write() += 1;
                        }
                        Err(e) => error.set(e.user_message()),
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = id;
                }
                saving.set(false);
            });
        }
    };

    rsx! {
        div { class: "container mx-auto max-w-4xl px-4 py-6 space-y-6",
            div {
                h1 { class: "text-2xl font-semibold", "Manage sharing" }
                p { class: "text-subtle text-sm mt-1",
                    "See who you've shared this account with, cancel pending invitations, or revoke access at any time."
                }
            }

            if unreachable {
                Card {
                    div { class: "p-4 text-sm text-red-600 dark:text-red-400",
                        "Could not reach the API to load your sharing outbox. Refresh to try again."
                    }
                }
            }

            if !error().is_empty() {
                Card {
                    div { role: "alert", class: "p-4 text-sm text-red-600 dark:text-red-400",
                        "{error}"
                    }
                }
            }

            // PENDING INVITATIONS ----
            Card {
                div { class: "p-4",
                    h2 { class: "text-lg font-semibold", "Pending invitations" }
                    p { class: "text-subtle text-xs mt-1",
                        "Invitations you've sent that haven't been accepted yet."
                    }
                }
                if loading {
                    div { class: "p-4 text-sm text-subtle", "Loading..." }
                } else if outbox_data.pending.is_empty() {
                    div { class: "p-4 text-sm text-subtle", "No pending invitations." }
                } else {
                    ul { class: "divide-y divide-line",
                        {outbox_data.pending.iter().map(|inv| {
                            let id = inv.id.clone();
                            let mut cancel = cancel_invitation;
                            let expires = fmt_ts(&inv.expires_at);
                            rsx! {
                                li {
                                    key: "{inv.id}",
                                    class: "flex items-center justify-between px-4 py-3 gap-4",
                                    div { class: "min-w-0 flex-1",
                                        div { class: "font-medium truncate", "{inv.invitee_email}" }
                                        div { class: "text-xs text-subtle",
                                            "Role: {humanise_role(&inv.role)}"
                                            if !expires.is_empty() {
                                                span { class: "ml-2", "Expires {expires}" }
                                            }
                                        }
                                    }
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        r#type: "button".to_string(),
                                        disabled: saving(),
                                        onclick: move |_| cancel(id.clone()),
                                        "Cancel"
                                    }
                                }
                            }
                        })}
                    }
                }
            }

            // ACTIVE GRANTS ----
            Card {
                div { class: "p-4",
                    h2 { class: "text-lg font-semibold", "Active grants" }
                    p { class: "text-subtle text-xs mt-1",
                        "People who currently have access to this account."
                    }
                }
                if loading {
                    div { class: "p-4 text-sm text-subtle", "Loading..." }
                } else if outbox_data.active.is_empty() {
                    div { class: "p-4 text-sm text-subtle", "You haven't shared this account with anyone." }
                } else {
                    ul { class: "divide-y divide-line",
                        {outbox_data.active.iter().map(|grant| {
                            let id = grant.id.clone();
                            let mut revoke = revoke_grant;
                            let granted = fmt_ts(&grant.granted_at);
                            // Display name: prefer explicit name, else email,
                            // else "Someone" (SaaS-mode rows carry neither today).
                            let display = grant
                                .grantee_name
                                .clone()
                                .or_else(|| grant.grantee_email.clone())
                                .unwrap_or_else(|| "Shared account".to_string());
                            rsx! {
                                li {
                                    key: "{grant.id}",
                                    class: "flex items-center justify-between px-4 py-3 gap-4",
                                    div { class: "min-w-0 flex-1",
                                        div { class: "font-medium truncate", "{display}" }
                                        div { class: "text-xs text-subtle",
                                            "Role: {humanise_role(&grant.role)}"
                                            if !granted.is_empty() {
                                                span { class: "ml-2", "Granted {granted}" }
                                            }
                                        }
                                    }
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        r#type: "button".to_string(),
                                        disabled: saving(),
                                        onclick: move |_| revoke(id.clone()),
                                        "Revoke"
                                    }
                                }
                            }
                        })}
                    }
                }
            }

            p { class: "text-xs text-subtle",
                "To change someone's role, revoke their access and re-invite them at the new role."
            }
        }
    }
}

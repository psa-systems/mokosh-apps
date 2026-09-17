//! MAPPS-875: owner-side grant management page.
//!
//! Lands at `/settings/sharing`. Owner-only surface. Two lists: pending
//! invitations (with Cancel per row) and active grants (with Revoke per
//! row). Fetches `GET /api/v1/grants?role=owner` from mokosh-server;
//! Cancel calls `DELETE /api/v1/grants/invitations/{id}` (PMS-1208
//! route); Revoke calls `DELETE /api/v1/grants/{id}` (MAPPS-875 route).
//!
//! Role-change on an active grant fires an in-place `PATCH
//! /api/v1/grants/{id}` (BUNYIP-748). The grantee's access continues
//! uninterrupted; their next request re-reads the mirror's role and
//! picks up the new value without re-authenticating. Same idempotency
//! shape as the other change verbs on this page: on failure the
//! error surfaces at the top and the modal stays open so the owner
//! can retry.
//!
//! Gate: `role.is_admin()` mirrors the sibling settings pages. The
//! server also gates every route on `RequireAdminUser`, so a non-admin
//! bypassing this SPA check still sees 403 from the fetch.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Button, ButtonVariant, Card, ContentUnavailable, Modal, ModalSize};

/// BUNYIP-748: request body for `PATCH /api/v1/grants/{id}`. The
/// grantee's role is the only field the endpoint moves; owner and
/// account are inferred from the URL and the caller's auth.
#[derive(Serialize)]
struct UpdateGrantRoleBody {
    role: String,
}

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
    // MAPPS-875 v2: local overlay signals for optimistic updates.
    // On a successful Cancel/Revoke we push the id here and the render
    // filters it out of the fetched list, so the row disappears
    // immediately. The next refetch (which we still fire, so a
    // concurrent change from another client is visible) picks up the
    // authoritative list and the overlay is cleared.
    let removed_pending: Signal<Vec<String>> = use_signal(Vec::new);
    let removed_active: Signal<Vec<String>> = use_signal(Vec::new);
    let outbox: Resource<Option<OwnerOutbox>> = use_resource({
        let mut removed_pending = removed_pending;
        let mut removed_active = removed_active;
        move || {
            let _bump = refresh_counter.read();
            async move {
                #[cfg(feature = "app")]
                {
                    let result =
                        crate::hooks::fetch::api::get_authed::<OwnerOutbox>("/grants?role=owner")
                            .await
                            .inspect_err(|e| tracing::error!("owner outbox load failed: {e}"))
                            .ok();
                    // Once the authoritative list has landed we don't
                    // need the local overlay anymore, and keeping it
                    // would hide a row that came back on a subsequent
                    // response for reasons the SPA didn't cause.
                    if result.is_some() {
                        removed_pending.write().clear();
                        removed_active.write().clear();
                    }
                    result
                }
                #[cfg(not(feature = "app"))]
                {
                    None
                }
            }
        }
    });
    let saving = use_signal(|| false);
    let error: Signal<String> = use_signal(String::new);
    // MAPPS-875 v2: confirm-first modal state for Revoke. The AC named
    // this explicitly ("Revoke access for X?"); Cancel on a pending
    // invitation stays one-click because the invitee has not been
    // granted anything yet.
    let confirm_revoke: Signal<Option<ActiveGrantView>> = use_signal(|| None);
    // MAPPS-875 v2: change-role modal. Open state is a
    // `Some(grant)` signal so the modal body can render the current
    // role beside the picker, and `role_pick` holds what the owner
    // is about to submit; keeping the picker's value on a separate
    // signal (rather than on the grant snapshot) means clicking the
    // picker between renders doesn't overwrite the row identity.
    let change_role_target: Signal<Option<ActiveGrantView>> = use_signal(|| None);
    let role_pick: Signal<String> = use_signal(String::new);

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

    // Apply the optimistic overlay: rows whose id is in the removed
    // list disappear from the rendered slice, so a click on Cancel or
    // Revoke feels instant. A subsequent refetch clears the overlay.
    let removed_pending_snap = removed_pending.read();
    let removed_active_snap = removed_active.read();
    let pending_view: Vec<PendingInvitationView> = outbox_data
        .pending
        .iter()
        .filter(|inv| !removed_pending_snap.contains(&inv.id))
        .cloned()
        .collect();
    let active_view: Vec<ActiveGrantView> = outbox_data
        .active
        .iter()
        .filter(|grant| !removed_active_snap.contains(&grant.id))
        .cloned()
        .collect();
    drop(removed_pending_snap);
    drop(removed_active_snap);

    let cancel_invitation = {
        let mut refresh_counter = refresh_counter;
        let mut removed_pending = removed_pending;
        let mut error = error;
        let mut saving = saving;
        move |id: String| {
            if saving() {
                return;
            }
            saving.set(true);
            error.set(String::new());
            // Optimistically hide the row. Restored on failure below.
            removed_pending.write().push(id.clone());
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/invitations/{id}");
                    match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                        Ok(_) => {
                            *refresh_counter.write() += 1;
                        }
                        Err(e) => {
                            // Restore the row and surface the message.
                            removed_pending.write().retain(|x| x != &id);
                            error.set(e.user_message());
                        }
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
        let mut removed_active = removed_active;
        let mut error = error;
        let mut saving = saving;
        move |id: String| {
            if saving() {
                return;
            }
            saving.set(true);
            error.set(String::new());
            removed_active.write().push(id.clone());
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/{id}");
                    match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                        Ok(_) => {
                            *refresh_counter.write() += 1;
                        }
                        Err(e) => {
                            removed_active.write().retain(|x| x != &id);
                            error.set(e.user_message());
                        }
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

    // BUNYIP-748: submit the in-place role change. One PATCH; the
    // grantee's session stays valid, and the mirror re-syncs via
    // bunyip's `mokosh_grant_changed` webhook so the new role is
    // visible on the grantee's next request without them having to
    // re-authenticate.
    //
    // Optimistic: mutate the local `outbox_data.active` row's role
    // in place so the modal-close refetch is cosmetic rather than a
    // visible flicker. On failure the row is restored from the
    // fetched shape (the refresh_counter bump triggers a refetch).
    let submit_change_role = {
        let mut refresh_counter = refresh_counter;
        let mut error = error;
        let mut saving = saving;
        let mut change_role_target = change_role_target;
        let mut role_pick = role_pick;
        move || {
            if saving() {
                return;
            }
            let Some(grant) = change_role_target.read().clone() else {
                return;
            };
            let new_role = role_pick.read().clone();
            if new_role.is_empty() || new_role == grant.role {
                // Nothing to do; the Save button should have been
                // disabled but a keyboard submit could still land
                // here. Close the modal quietly.
                *change_role_target.write() = None;
                return;
            }
            saving.set(true);
            error.set(String::new());
            let grant_id = grant.id.clone();
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/{grant_id}");
                    let body = UpdateGrantRoleBody {
                        role: new_role.clone(),
                    };
                    // `patch_authed_typed` expects the response to
                    // decode into T; our handler returns 204 with no
                    // body, so `serde_json::Value` accepts an empty
                    // body via serde's null-on-empty behavior. The
                    // caller doesn't need the returned shape - the
                    // refetch is the source of truth.
                    if let Err(e) = crate::hooks::fetch::api::patch_authed_typed::<
                        serde_json::Value,
                        _,
                    >(&path, &body)
                    .await
                    {
                        error.set(format!(
                            "Couldn't change this grant's role: {}",
                            e.user_message()
                        ));
                        saving.set(false);
                        return;
                    }
                    *refresh_counter.write() += 1;
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = grant_id;
                    let _ = new_role;
                }
                *change_role_target.write() = None;
                *role_pick.write() = String::new();
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
                } else if pending_view.is_empty() {
                    div { class: "p-4 flex items-center justify-between gap-4",
                        p { class: "text-sm text-subtle",
                            "No pending invitations."
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            r#type: "button".to_string(),
                            onclick: move |_| {
                                *crate::components::SHOW_INVITE_MEMBER.write() = true;
                            },
                            "Invite member"
                        }
                    }
                } else {
                    ul { class: "divide-y divide-line",
                        {pending_view.iter().map(|inv| {
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
                } else if active_view.is_empty() {
                    div { class: "p-4 flex items-center justify-between gap-4",
                        p { class: "text-sm text-subtle",
                            "You haven't shared this account with anyone."
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            r#type: "button".to_string(),
                            onclick: move |_| {
                                *crate::components::SHOW_INVITE_MEMBER.write() = true;
                            },
                            "Invite member"
                        }
                    }
                } else {
                    ul { class: "divide-y divide-line",
                        {active_view.iter().map(|grant| {
                            let grant_for_revoke = grant.clone();
                            let grant_for_change_role = grant.clone();
                            let mut confirm_revoke = confirm_revoke;
                            let mut change_role_target = change_role_target;
                            let mut role_pick = role_pick;
                            let granted = fmt_ts(&grant.granted_at);
                            // Display name: prefer explicit name, else email,
                            // else "Someone" (SaaS-mode rows carry neither today).
                            let display = grant
                                .grantee_name
                                .clone()
                                .or_else(|| grant.grantee_email.clone())
                                .unwrap_or_else(|| "Shared account".to_string());
                            // BUNYIP-748: "Change role" is always available on
                            // an active grant now that the flow is an in-place
                            // PATCH; the previous email-gate was for the
                            // interim revoke + re-invite that needed the email
                            // to re-send the invitation. PATCH takes only the
                            // grant id, so a row with no `grantee_email`
                            // (a legacy grant, or one whose grantee row was
                            // soft-deleted) still surfaces the button.
                            let can_change_role = true;
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
                                    div { class: "flex gap-2",
                                        if can_change_role {
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                r#type: "button".to_string(),
                                                disabled: saving(),
                                                onclick: move |_| {
                                                    let current = grant_for_change_role.role.clone();
                                                    *role_pick.write() = current;
                                                    *change_role_target.write() = Some(grant_for_change_role.clone());
                                                },
                                                "Change role"
                                            }
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            r#type: "button".to_string(),
                                            disabled: saving(),
                                            // MAPPS-875 v2: two-step revoke.
                                            // Clicking Revoke opens the confirm
                                            // modal (below), which then calls
                                            // `revoke_grant` on Confirm. Cancel
                                            // is one-click by design.
                                            onclick: move |_| {
                                                *confirm_revoke.write() = Some(grant_for_revoke.clone());
                                            },
                                            "Revoke"
                                        }
                                    }
                                }
                            }
                        })}
                    }
                }
            }

            // Revoke-confirmation modal. Open state is a `Some(grant)`
            // signal so the copy names WHO is about to lose access; a
            // bare `bool` would either be generic ("Revoke this grant?"
            // - not the AC named copy) or would need a second signal for
            // the row identity.
            {
                let target = confirm_revoke.read().clone();
                let is_open = target.is_some();
                let display = target.as_ref().map(|g| {
                    g.grantee_name.clone()
                        .or_else(|| g.grantee_email.clone())
                        .unwrap_or_else(|| "this account".to_string())
                }).unwrap_or_default();
                let target_id = target.as_ref().map(|g| g.id.clone()).unwrap_or_default();
                let mut confirm_revoke_close = confirm_revoke;
                let mut revoke = revoke_grant;
                rsx! {
                    Modal {
                        open: is_open,
                        title: "Revoke access".to_string(),
                        size: ModalSize::Small,
                        onclose: move |_| *confirm_revoke_close.write() = None,
                        div { class: "space-y-4",
                            p { class: "text-sm text-content",
                                "Revoke access for "
                                span { class: "font-medium", "{display}" }
                                "? They will lose access to this account immediately. You can send a fresh invitation later if you change your mind."
                            }
                            div { class: "flex gap-2 justify-end pt-2",
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    r#type: "button".to_string(),
                                    disabled: saving(),
                                    onclick: move |_| *confirm_revoke_close.write() = None,
                                    "Cancel"
                                }
                                Button {
                                    variant: ButtonVariant::Primary,
                                    r#type: "button".to_string(),
                                    disabled: saving(),
                                    loading: saving(),
                                    onclick: move |_| {
                                        let id = target_id.clone();
                                        *confirm_revoke_close.write() = None;
                                        revoke(id);
                                    },
                                    "Revoke access"
                                }
                            }
                        }
                    }
                }
            }

            // MAPPS-875 v2: change-role modal. The AC named a form
            // with the current role shown + a picker for the new
            // one; today's submit runs revoke + re-invite in
            // sequence (BUNYIP-748 replaces this with a proper
            // PATCH). Rendering under both modals so the DOM order
            // matches the visual "confirm first, then act" flow.
            {
                let target = change_role_target.read().clone();
                let is_open = target.is_some();
                let display = target.as_ref().map(|g| {
                    g.grantee_name.clone()
                        .or_else(|| g.grantee_email.clone())
                        .unwrap_or_else(|| "this account".to_string())
                }).unwrap_or_default();
                let current_role = target.as_ref().map(|g| g.role.clone()).unwrap_or_default();
                let pick = role_pick();
                let unchanged = pick.is_empty() || pick == current_role;
                let mut change_role_close = change_role_target;
                let mut role_pick = role_pick;
                let mut submit = submit_change_role;
                rsx! {
                    Modal {
                        open: is_open,
                        title: "Change role".to_string(),
                        size: ModalSize::Small,
                        onclose: move |_| {
                            *change_role_close.write() = None;
                            *role_pick.write() = String::new();
                        },
                        div { class: "space-y-4",
                            p { class: "text-sm text-content",
                                "Change the role for "
                                span { class: "font-medium", "{display}" }
                                "."
                            }
                            div { class: "space-y-1",
                                label { class: "text-xs text-subtle",
                                    "Current role: {humanise_role(&current_role)}"
                                }
                                label { class: "text-sm text-content", "New role" }
                                select {
                                    class: "block w-full rounded-md border border-line bg-surface-1 px-3 py-2 text-sm text-content focus:outline-none",
                                    value: pick.clone(),
                                    disabled: saving(),
                                    onchange: move |e: FormEvent| role_pick.set(e.value()),
                                    option { value: "admin", "Admin" }
                                    option { value: "manager", "Manager" }
                                    option { value: "technician", "Technician" }
                                    option { value: "finance", "Finance" }
                                    option { value: "read_only", "Read only" }
                                }
                            }
                            p { class: "text-xs text-subtle",
                                "Their access changes to the new role immediately. They stay signed in and will see the new permissions on their next request."
                            }
                            div { class: "flex gap-2 justify-end pt-2",
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    r#type: "button".to_string(),
                                    disabled: saving(),
                                    onclick: move |_| {
                                        *change_role_close.write() = None;
                                        *role_pick.write() = String::new();
                                    },
                                    "Cancel"
                                }
                                Button {
                                    variant: ButtonVariant::Primary,
                                    r#type: "button".to_string(),
                                    disabled: saving() || unchanged,
                                    loading: saving(),
                                    onclick: move |_| submit(),
                                    "Change role"
                                }
                            }
                        }
                    }
                }
            }

        }
    }
}

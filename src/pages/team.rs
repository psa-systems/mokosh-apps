//! Team & invitations management (PMS-247).
//!
//! Admin-only surface to grow an organization: invite people by email + role,
//! see pending invitations, and revoke them. Wires to mokosh-server's
//! invitations API (`POST`/`GET`/`DELETE /api/v1/invitations`). Acceptance is
//! login-driven on the server (PMS-244): an invitee joins on their next sign-in.

use dioxus::prelude::*;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, DataTable,
    Input, PageHeader, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::hooks::auth::use_auth;
use crate::modules::auth::UserRole;
use crate::utils::{FormGuard, Rule};

/// Subset of mokosh-server's `InvitationResponse` rendered here.
#[derive(Clone, Debug, serde::Deserialize)]
struct RemoteInvitation {
    id: uuid::Uuid,
    email: String,
    role: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, serde::Deserialize)]
struct PaginatedInvitations {
    data: Vec<RemoteInvitation>,
}

/// Whether the invite form exposes a role picker.
///
/// Role-based access is only partially implemented (admin vs non-admin plus a
/// finance/billing carve-out); the other roles in the picker have no complete
/// permission semantics yet, so assigning them is misleading and risks granting
/// unexpected access once full RBAC lands. While this is `false` the picker is
/// hidden and every invite goes out as the lowest-privilege role (Technician).
/// Flip to `true` to restore role assignment once RBAC is complete. See PMS-513.
const ROLE_ASSIGNMENT_ENABLED: bool = false;

#[component]
pub fn TeamPage() -> Element {
    use_page_title("Team");
    let auth = use_auth();
    let is_admin = {
        let a = auth.read();
        a.has_role(UserRole::Admin) || a.has_role(UserRole::SuperAdmin)
    };

    let mut email = use_signal(String::new);
    let mut role = use_signal(|| "technician".to_string());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error slot for the email field, fed by the
    // FormGuard in handle_invite. The form-level `error` banner is kept for the
    // server send failure, which has no single field to attach to.
    let mut email_error = use_signal(String::new);

    // Pending invitations for the active tenant. Re-fetches on tenant switch.
    // MAPPS-357: this is the page's primary resource. It keeps a hand-rolled
    // `use_resource` (rather than `use_remote_resource`) because the invite /
    // revoke flows call `invites.restart()`, and it subscribes to reachability
    // so the roster auto-refetches on reconnect. The fetcher keeps `.ok()`
    // (NOT `.unwrap_or_default()`) so a failed load stays distinguishable from
    // an empty roster, letting the outage render `ContentUnavailable` below.
    let mut invites = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed::<PaginatedInvitations>("/invitations")
                .await
                .map(|p| p.data)
                .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            Some(Vec::<RemoteInvitation>::new())
        }
    });

    // Built only when role assignment is enabled; the picker keeps its full
    // taxonomy for the day RBAC lands. While disabled this is empty and the
    // Select below is not rendered, so `role` keeps its "technician" default.
    let role_options = if ROLE_ASSIGNMENT_ENABLED {
        vec![
            SelectOption::new("technician", "Technician"),
            SelectOption::new("manager", "Manager"),
            SelectOption::new("admin", "Admin"),
            SelectOption::new("dispatcher", "Dispatcher"),
            SelectOption::new("sales", "Sales"),
            SelectOption::new("finance", "Finance"),
        ]
    } else {
        Vec::new()
    };

    let handle_invite = move |e: FormEvent| {
        e.prevent_default();
        let email_v = email.read().trim().to_string();

        // PMS-518: validate the required Email through the shared FormGuard so
        // the failure surfaces in the field's own inline slot and is focused.
        let mut guard = FormGuard::new();
        email_error.set(guard.field("email", &email_v, "Email", &[Rule::Required, Rule::Email]));
        if guard.blocked() {
            return;
        }
        let role_v = role.read().clone();
        is_submitting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({ "email": email_v, "role": role_v });
                #[derive(serde::Deserialize)]
                struct Created {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed::<Created, _>("/invitations", &body)
                    .await
                {
                    Ok(_) => {
                        email.set(String::new());
                        invites.restart();
                    }
                    Err(err) => error.set(format!("Could not send invite: {err}")),
                }
            }
            is_submitting.set(false);
        });
    };

    // MAPPS-377: every hook must run before the admin / outage early returns
    // below, so hoist the reachability reads and the revoke-dialog signals to
    // the top. None of them depend on post-return state.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    let mut pending_revoke = use_signal::<Option<(uuid::Uuid, String)>>(|| None);
    let mut revoking = use_signal(|| false);

    if !is_admin {
        return rsx! {
            PageHeader {
                title: "Team",
                subtitle: "Manage who can access this organization",
            }
            Card {
                p { class: "text-sm text-muted",
                    "You need an admin role to manage invitations."
                }
            }
        };
    }

    let snapshot = invites.read_unchecked();
    let is_loading = snapshot.is_none();
    let fetch_failed = matches!(*snapshot, Some(None));
    let rows = match &*snapshot {
        Some(Some(v)) => v.clone(),
        _ => Vec::new(),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty roster - render the honest unavailable state (which keeps
    // the nav + banner) instead of an empty invitations table. A fetch that
    // fails while still reachable (a 4xx) keeps the normal empty-state below.
    // Writes are blocked while down; `can_mutate` disables the buttons.
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Team".to_string() }
        };
    }

    // PMS-369: the Revoke button used to fire DELETE /invitations/{id}
    // on first click with no confirmation, so a misclick on the row
    // immediately destroyed a pending invite. The Revoke button below
    // now stages the target id and email into `pending_revoke`, which
    // opens a ConfirmDialog; only the explicit Revoke press inside the
    // dialog fires the DELETE. Cancel and the X both clear the signal.
    rsx! {
        PageHeader {
            title: "Team",
            subtitle: "Invite people to this organization and manage pending invitations",
        }

        Card {
            form { class: "space-y-4", onsubmit: handle_invite,
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                div {
                    class: if ROLE_ASSIGNMENT_ENABLED { "grid grid-cols-1 gap-4 sm:grid-cols-3 sm:items-end" } else { "grid grid-cols-1 gap-4 sm:grid-cols-2 sm:items-end" },
                    Input {
                        name: "email",
                        label: "Email",
                        r#type: "email".to_string(),
                        placeholder: "person@example.com",
                        required: true,
                        rules: vec![Rule::Required, Rule::Email],
                        error: email_error.read().clone(),
                        value: email.read().clone(),
                        oninput: move |e: FormEvent| {
                            email_error.set(String::new());
                            email.set(e.value());
                        },
                    }
                    if ROLE_ASSIGNMENT_ENABLED {
                        Select {
                            name: "role",
                            label: "Role",
                            options: role_options,
                            value: role.read().clone(),
                            onchange: move |e: FormEvent| role.set(e.value()),
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        r#type: "submit".to_string(),
                        loading: is_submitting(),
                        // MAPPS-357: block invites while the server is down.
                        disabled: is_submitting() || !can_mutate,
                        title: (!can_mutate).then(|| "Can't send an invite while the server is unreachable".to_string()),
                        "Send invite"
                    }
                }
            }
        }

        // PMS-369: Revoke confirmation. `pending_revoke` carries the
        // (id, email) of the row whose Revoke button was clicked; the
        // dialog renders with the email inlined so the user can see
        // which invite they are about to revoke. `revoking` gates the
        // dialog's loading state so a click during the in-flight
        // request does not double-fire.
        {
            let pending = pending_revoke.read().clone();
            let open = pending.is_some();
            let email_label = pending
                .as_ref()
                .map(|(_, e)| e.clone())
                .unwrap_or_default();
            let message = if email_label.is_empty() {
                "Revoke this invitation? The invitee will not be able to accept it.".to_string()
            } else {
                format!(
                    "Revoke the invitation for {email_label}? They will not be able to accept it."
                )
            };
            let mut invites_for_confirm = invites;
            let on_confirm = move |_: ()| {
                let Some((id, _)) = pending_revoke.read().clone() else {
                    return;
                };
                if revoking() {
                    return;
                }
                revoking.set(true);
                spawn(async move {
                    #[cfg(feature = "web")]
                    {
                        let _ = crate::hooks::fetch::api::delete_authed(&format!(
                            "/invitations/{id}"
                        ))
                        .await;
                        invites_for_confirm.restart();
                    }
                    #[cfg(not(feature = "web"))]
                    {
                        let _ = id;
                    }
                    revoking.set(false);
                    pending_revoke.set(None);
                });
            };
            rsx! {
                ConfirmDialog {
                    open,
                    title: "Revoke invitation".to_string(),
                    message,
                    confirm_text: "Revoke".to_string(),
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: revoking(),
                    onconfirm: on_confirm,
                    oncancel: move |_| {
                        if !revoking() {
                            pending_revoke.set(None);
                        }
                    },
                }
            }
        }

        div { class: "mt-6",
            DataTable {
                loading: is_loading,
                total_items: rows.len(),
                current_page: 1,
                per_page: 25,
                columns: 4,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Email" }
                            TableHeader { "Role" }
                            TableHeader { "Expires" }
                            TableHeader { "" }
                        }
                    }
                    if is_loading {
                        TableLoading { columns: 4, rows: 3 }
                    } else if rows.is_empty() {
                        TableEmpty {
                            columns: 4,
                            title: "No pending invitations".to_string(),
                            description: "Use the Invite form above to grant a teammate access.".to_string(),
                        }
                    } else {
                        TableBody {
                            for inv in rows.iter().cloned() {
                                TableRow { key: "{inv.id}",
                                    TableCell { "{inv.email}" }
                                    TableCell {
                                        Badge { variant: BadgeVariant::Gray, "{inv.role}" }
                                    }
                                    TableCell { "{inv.expires_at.format(\"%Y-%m-%d\")}" }
                                    TableCell {
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            // MAPPS-357: block revoke while the server is down.
                                            disabled: !can_mutate,
                                            title: (!can_mutate).then(|| "Can't revoke while the server is unreachable".to_string()),
                                            onclick: move |_| {
                                                pending_revoke.set(Some((inv.id, inv.email.clone())));
                                            },
                                            "Revoke"
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

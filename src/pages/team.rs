//! Team & invitations management (PMS-247).
//!
//! Admin-only surface to grow an organization: invite people by email + role,
//! see pending invitations, and revoke them. Wires to mokosh-server's
//! invitations API (`POST`/`GET`/`DELETE /api/v1/invitations`). Acceptance is
//! login-driven on the server (PMS-244): an invitee joins on their next sign-in.

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, DataTable, Input,
    PageHeader, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow,
};
use crate::hooks::auth::use_auth;
use crate::modules::auth::UserRole;

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

#[component]
pub fn TeamPage() -> Element {
    let auth = use_auth();
    let is_admin = {
        let a = auth.read();
        a.has_role(UserRole::Admin) || a.has_role(UserRole::SuperAdmin)
    };

    let mut email = use_signal(String::new);
    let mut role = use_signal(|| "technician".to_string());
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Pending invitations for the active tenant. Re-fetches on tenant switch.
    let mut invites = use_resource(|| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed::<PaginatedInvitations>("/invitations")
                .await
                .map(|p| p.data)
                .unwrap_or_default()
        }
        #[cfg(not(feature = "web"))]
        {
            Vec::<RemoteInvitation>::new()
        }
    });

    let role_options = vec![
        SelectOption::new("technician", "Technician"),
        SelectOption::new("manager", "Manager"),
        SelectOption::new("admin", "Admin"),
        SelectOption::new("dispatcher", "Dispatcher"),
        SelectOption::new("sales", "Sales"),
        SelectOption::new("finance", "Finance"),
    ];

    let handle_invite = move |e: FormEvent| {
        e.prevent_default();
        let email_v = email.read().trim().to_string();
        if email_v.is_empty() {
            error.set("Enter an email address.".to_string());
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

    if !is_admin {
        return rsx! {
            AppLayout { title: "Team",
                PageHeader {
                    title: "Team",
                    subtitle: "Manage who can access this organization",
                }
                Card {
                    p { class: "text-sm text-gray-500 dark:text-gray-400",
                        "You need an admin role to manage invitations."
                    }
                }
            }
        };
    }

    let snapshot = invites.read_unchecked();
    let is_loading = snapshot.is_none();
    let rows = snapshot.as_ref().cloned().unwrap_or_default();

    // PMS-369: the Revoke button used to fire DELETE /invitations/{id}
    // on first click with no confirmation, so a misclick on the row
    // immediately destroyed a pending invite. The Revoke button below
    // now stages the target id and email into `pending_revoke`, which
    // opens a ConfirmDialog; only the explicit Revoke press inside the
    // dialog fires the DELETE. Cancel and the X both clear the signal.
    let mut pending_revoke = use_signal::<Option<(uuid::Uuid, String)>>(|| None);
    let mut revoking = use_signal(|| false);

    rsx! {
        AppLayout { title: "Team",
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
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3 sm:items-end",
                        Input {
                            name: "email",
                            label: "Email",
                            r#type: "email".to_string(),
                            placeholder: "person@example.com",
                            required: true,
                            value: email.read().clone(),
                            oninput: move |e: FormEvent| email.set(e.value()),
                        }
                        Select {
                            name: "role",
                            label: "Role",
                            options: role_options,
                            value: role.read().clone(),
                            onchange: move |e: FormEvent| role.set(e.value()),
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            r#type: "submit".to_string(),
                            loading: is_submitting(),
                            disabled: is_submitting(),
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
}

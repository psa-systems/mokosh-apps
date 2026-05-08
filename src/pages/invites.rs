//! Admin-side invite UI.
//!
//! Two routes:
//!   - `InviteCreatePage` (`/settings/users/invite`) - issue a new invite
//!   - `InviteListPage`   (`/settings/users/invites`) - list / revoke /
//!     resend pending invites
//!
//! Both gate on the `admin` role via `use_require_role`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, Input, PageHeader, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{fetch::api, use_require_role};
use crate::Route;

// ---------------------------------------------------------------------------
// DTOs (mirror mokosh-server `InviteView` / `IssueInviteBody`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct InviteView {
    pub id: uuid::Uuid,
    pub email: String,
    pub role: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub invited_by: InvitedByView,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct InvitedByView {
    pub id: uuid::Uuid,
    pub email: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InvitesEnvelope {
    pub invites: Vec<InviteView>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IssueInviteSuccess {
    pub invite: InviteView,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IssueInviteBody {
    pub email: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Translate the structured backend error body into a UI-readable
/// string. The API returns either `{"error":"<code>", ...}` or a raw
/// status line.
fn parse_error_message(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        match v.get("error").and_then(|s| s.as_str()) {
            Some("invite_already_open") => {
                return "An invite already exists for this email. Use Resend or Revoke first."
                    .to_string();
            }
            Some("forbidden") => return "Only admins may issue invites.".into(),
            Some("rate_limited") => {
                return "Too many invites issued recently. Please wait a moment.".into();
            }
            Some("invalid_request") => {
                if let Some(detail) = v
                    .get("details")
                    .and_then(|d| d.as_object())
                    .and_then(|m| m.values().next())
                    .and_then(|v| v.as_str())
                {
                    return detail.to_string();
                }
                if let Some(desc) = v.get("error_description").and_then(|s| s.as_str()) {
                    return desc.to_string();
                }
                return "Invalid request.".into();
            }
            Some(other) => return format!("Server error: {other}"),
            None => {}
        }
    }
    raw.to_string()
}

fn role_label(role: &str) -> &'static str {
    match role {
        "admin" => "Admin",
        "manager" => "Manager",
        "finance" => "Finance",
        "member" => "Member",
        "readonly" => "Read only",
        _ => "Other",
    }
}

fn role_badge_variant(role: &str) -> BadgeVariant {
    match role {
        "admin" => BadgeVariant::Red,
        "manager" => BadgeVariant::Blue,
        "finance" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    }
}

fn relative(ts: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    if ts > now {
        let d = ts - now;
        if d < chrono::Duration::minutes(1) {
            "<1 min".into()
        } else if d < chrono::Duration::hours(1) {
            format!("in {} min", d.num_minutes())
        } else if d < chrono::Duration::days(1) {
            format!("in {}h", d.num_hours())
        } else {
            format!("in {}d", d.num_days())
        }
    } else {
        let d = now - ts;
        if d < chrono::Duration::minutes(1) {
            "just now".into()
        } else if d < chrono::Duration::hours(1) {
            format!("{} min ago", d.num_minutes())
        } else if d < chrono::Duration::days(1) {
            format!("{}h ago", d.num_hours())
        } else {
            format!("{}d ago", d.num_days())
        }
    }
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[component]
pub fn InviteCreatePage() -> Element {
    let _auth = use_require_role("admin");
    let navigator = use_navigator();

    let mut email = use_signal(String::new);
    let mut role = use_signal(|| "member".to_string());
    let mut note = use_signal(String::new);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut warning: Signal<Option<String>> = use_signal(|| None);
    let mut submitting = use_signal(|| false);

    let submit = move || {
        let body = IssueInviteBody {
            email: email.read().trim().to_string(),
            role: role.read().clone(),
            note: {
                let n = note.read().trim().to_string();
                if n.is_empty() {
                    None
                } else {
                    Some(n)
                }
            },
        };
        spawn(async move {
            error.set(None);
            warning.set(None);
            submitting.set(true);
            match api::post_authed::<IssueInviteSuccess, _>("/auth/invites", &body).await {
                Ok(resp) => {
                    if let Some(w) = resp.warning {
                        warning.set(Some(format!(
                            "Invite created but the email failed to send ({w}). Use Resend on the invite list."
                        )));
                        submitting.set(false);
                    } else {
                        navigator.push(Route::InviteList {});
                    }
                }
                Err(e) => {
                    error.set(Some(parse_error_message(&e)));
                    submitting.set(false);
                }
            }
        });
    };

    rsx! {
        AppLayout { title: "Invite user".to_string(),
            div { class: "max-w-2xl mx-auto",
                Card {
                    PageHeader {
                        title: "Invite a teammate".to_string(),
                        subtitle: "They'll receive an email with a one-time link.".to_string(),
                    }

                    form {
                        class: "space-y-4 mt-6",
                        onsubmit: move |e| { e.prevent_default(); submit(); },

                        if let Some(msg) = error.read().as_ref() {
                            div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                                p { class: "text-sm text-red-600 dark:text-red-400", "{msg}" }
                            }
                        }
                        if let Some(msg) = warning.read().as_ref() {
                            div { class: "bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-md p-4",
                                p { class: "text-sm text-yellow-700 dark:text-yellow-400", "{msg}" }
                            }
                        }

                        Input {
                            name: "email".to_string(),
                            label: "Email".to_string(),
                            r#type: "email".to_string(),
                            placeholder: "alice@example.com".to_string(),
                            required: true,
                            value: email.read().clone(),
                            oninput: move |e: FormEvent| email.set(e.value()),
                        }

                        div {
                            label {
                                r#for: "role",
                                class: "block text-sm font-medium text-gray-700 dark:text-gray-200 mb-1",
                                "Role"
                            }
                            select {
                                id: "role",
                                name: "role",
                                class: "block w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500",
                                value: role.read().clone(),
                                onchange: move |e: FormEvent| role.set(e.value()),
                                option { value: "member", "Member" }
                                option { value: "manager", "Manager" }
                                option { value: "finance", "Finance" }
                                option { value: "admin", "Admin" }
                                option { value: "readonly", "Read only" }
                            }
                        }

                        Input {
                            name: "note".to_string(),
                            label: "Note (optional)".to_string(),
                            r#type: "text".to_string(),
                            placeholder: "Welcome aboard!".to_string(),
                            value: note.read().clone(),
                            oninput: move |e: FormEvent| note.set(e.value()),
                            help: "Shown to the invitee on the accept page. Max 200 chars.".to_string(),
                        }

                        div { class: "flex justify-end gap-2 pt-2",
                            Button {
                                r#type: "button".to_string(),
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| { navigator.push(Route::InviteList {}); },
                                "Cancel"
                            }
                            Button {
                                r#type: "submit".to_string(),
                                variant: ButtonVariant::Primary,
                                loading: *submitting.read(),
                                "Send invite"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn InviteListPage() -> Element {
    let _auth = use_require_role("admin");
    let navigator = use_navigator();
    let mut invites: Signal<Option<Result<Vec<InviteView>, String>>> = use_signal(|| None);
    let mut bump = use_signal(|| 0u32);

    // Re-fetch when bump changes (after a row mutation).
    use_future(move || async move {
        let _ = bump.read();
        invites.set(None);
        let result = api::get_authed::<InvitesEnvelope>("/auth/invites")
            .await
            .map(|env| env.invites)
            .map_err(|e| parse_error_message(&e));
        invites.set(Some(result));
    });

    let refetch = move || { bump.with_mut(|n| *n += 1); };

    rsx! {
        AppLayout { title: "Pending invites".to_string(),
            div { class: "flex justify-between items-center mb-4",
                PageHeader { title: "Pending invites".to_string() }
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: move |_| { navigator.push(Route::InviteCreate {}); },
                    "Invite a teammate"
                }
            }

            match invites.read().clone() {
                None => rsx! { div { class: "p-8 text-center text-gray-500", "Loading..." } },
                Some(Err(msg)) => rsx! {
                    div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                        p { class: "text-sm text-red-600 dark:text-red-400", "{msg}" }
                    }
                },
                Some(Ok(rows)) if rows.is_empty() => rsx! {
                    Card {
                        div { class: "p-8 text-center",
                            p { class: "text-gray-600 dark:text-gray-300 mb-4", "No pending invites." }
                            Button {
                                variant: ButtonVariant::Primary,
                                onclick: move |_| { navigator.push(Route::InviteCreate {}); },
                                "Invite the first teammate"
                            }
                        }
                    }
                },
                Some(Ok(rows)) => rsx! {
                    Card {
                        Table {
                            TableHead {
                                tr {
                                    TableHeader { "Email" }
                                    TableHeader { "Role" }
                                    TableHeader { "Invited by" }
                                    TableHeader { "Issued" }
                                    TableHeader { "Expires" }
                                    TableHeader { "" }
                                }
                            }
                            TableBody {
                                for invite in rows {
                                    InviteRow {
                                        key: "{invite.id}",
                                        invite: invite.clone(),
                                        on_changed: refetch.clone(),
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct InviteRowProps {
    invite: InviteView,
    on_changed: EventHandler<()>,
}

#[component]
fn InviteRow(props: InviteRowProps) -> Element {
    let mut busy = use_signal(|| false);
    let invite = props.invite.clone();
    let id = invite.id;
    let on_changed_resend = props.on_changed.clone();
    let on_changed_revoke = props.on_changed.clone();

    let resend = move || {
        spawn(async move {
            busy.set(true);
            let _ = api::post_authed::<serde_json::Value, _>(
                &format!("/auth/invites/{id}/resend"),
                &serde_json::json!({}),
            )
            .await;
            busy.set(false);
            on_changed_resend.call(());
        });
    };

    let revoke = move || {
        spawn(async move {
            busy.set(true);
            let _ = api::post_authed::<serde_json::Value, _>(
                &format!("/auth/invites/{id}/revoke"),
                &serde_json::json!({"reason": ""}),
            )
            .await;
            busy.set(false);
            on_changed_revoke.call(());
        });
    };

    let issued = relative(invite.issued_at);
    let expires = if invite.expires_at < chrono::Utc::now() {
        "expired".to_string()
    } else {
        relative(invite.expires_at)
    };

    rsx! {
        TableRow {
            TableCell { "{invite.email}" }
            TableCell {
                Badge {
                    variant: role_badge_variant(&invite.role),
                    "{role_label(&invite.role)}"
                }
            }
            TableCell { "{invite.invited_by.email}" }
            TableCell { class: "text-gray-500 text-sm".to_string(), "{issued}" }
            TableCell { class: "text-gray-500 text-sm".to_string(), "{expires}" }
            TableCell {
                div { class: "flex gap-2 justify-end",
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: *busy.read(),
                        onclick: move |_| resend(),
                        "Resend"
                    }
                    Button {
                        variant: ButtonVariant::Danger,
                        disabled: *busy.read(),
                        onclick: move |_| revoke(),
                        "Revoke"
                    }
                }
            }
        }
    }
}

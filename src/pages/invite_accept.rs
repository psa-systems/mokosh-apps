//! `/invite/:token` - the page the invite recipient lands on after
//! clicking the email link. Public route (no auth). Three states:
//! Loading → Ready (form) → Done (redirect to /login). Error states
//! collapse to a single "invite not available" message regardless of
//! the underlying cause (unknown / used / revoked / expired) to match
//! the server's enumeration-resistant 404 contract.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct InvitePreview {
    /// `new_account` (collect a password, create a User+Membership)
    /// or `join_tenant` (existing user; show a confirm card and add
    /// a Membership). Older servers omit this field; default to
    /// `new_account` so the SPA still works against them.
    #[serde(default = "default_kind")]
    kind: String,
    email: String,
    role: String,
    tenant_name: String,
    invited_by_name: String,
    invited_by_email: String,
    #[serde(default)]
    note: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
}

fn default_kind() -> String {
    "new_account".to_string()
}

#[derive(Clone, Debug, Deserialize)]
struct AcceptedUser {
    email: String,
    redirect_to: String,
}

#[derive(Clone, Debug, PartialEq)]
enum AcceptState {
    Loading,
    Ready(InvitePreview),
    Submitting(InvitePreview),
    Done(String),
    Error(String),
}

fn invalid_message() -> String {
    "This invite link is invalid, expired, or has already been used.".to_string()
}

fn is_404(raw: &str) -> bool {
    raw.contains("status: 404") || raw.contains("invite_not_found")
}
fn is_409(raw: &str) -> bool {
    raw.contains("status: 409") || raw.contains("invite_already_used")
}

/// If the backend returned `{"error":"invalid_request","details":{<field>:<msg>}}`,
/// pluck (field, msg). Otherwise None.
fn parse_field_error(raw: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.get("error")?.as_str()? != "invalid_request" {
        return None;
    }
    let details = v.get("details")?.as_object()?;
    let (k, value) = details.iter().next()?;
    Some((k.clone(), value.as_str()?.to_string()))
}

fn role_label(role: &str) -> String {
    match role {
        "admin" => "Admin".into(),
        "manager" => "Manager".into(),
        "finance" => "Finance".into(),
        "member" => "Member".into(),
        "readonly" => "Read only".into(),
        other => other.trim().to_string(),
    }
}

#[component]
pub fn InviteAcceptPage(token: String) -> Element {
    let mut state = use_signal(|| AcceptState::Loading);
    let mut password = use_signal(String::new);
    let mut password2 = use_signal(String::new);
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut field_error: Signal<Option<(String, String)>> = use_signal(|| None);

    // Initial preview load. Hits the issuer URL directly: invite
    // endpoints live on the auth router which is merged at the
    // mokosh-server root, NOT under the same-origin /api/v1/* proxy
    // (that path goes to the legacy psa_router which has no
    // matching route). Public endpoint, no Bearer needed.
    let token_load = token.clone();
    use_future(move || {
        let token = token_load.clone();
        async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/invites/by-token/{token}");
            match crate::modules::oidc::issuer_get::<InvitePreview>(&cfg, &path).await {
                Ok(preview) => state.set(AcceptState::Ready(preview)),
                Err(e) => {
                    let msg = e.to_string();
                    if is_404(&msg) {
                        state.set(AcceptState::Error(invalid_message()));
                    } else {
                        state.set(AcceptState::Error(format!(
                            "Could not load invite: {msg}"
                        )));
                    }
                }
            }
        }
    });

    let token_submit = token.clone();
    let mut submit = move || {
        let preview = match state.read().clone() {
            AcceptState::Ready(p) => p,
            _ => return,
        };

        // Password check only on the new_account path. join_tenant
        // skips the password fields entirely (the user already has
        // one).
        let is_new_account = preview.kind == "new_account";
        let pw = password.read().clone();
        let pw2 = password2.read().clone();
        if is_new_account && pw != pw2 {
            field_error.set(Some(("password2".into(), "Passwords do not match".into())));
            return;
        }
        field_error.set(None);

        state.set(AcceptState::Submitting(preview));

        let token = token_submit.clone();
        let first = first_name.read().trim().to_string();
        let last = last_name.read().trim().to_string();
        spawn(async move {
            let body = if is_new_account {
                serde_json::json!({
                    "password": pw,
                    "first_name": if first.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(first) },
                    "last_name":  if last.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(last) },
                })
            } else {
                // join_tenant: server ignores password; first/last
                // are also irrelevant since the existing user record
                // already has them. Empty body is the simplest shape.
                serde_json::json!({})
            };
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/invites/by-token/{token}/accept");
            match crate::modules::oidc::issuer_post::<AcceptedUser, _>(&cfg, &path, &body).await {
                Ok(accepted) => {
                    state.set(AcceptState::Done(accepted.email.clone()));
                    // Brief delay so the success message is visible.
                    #[cfg(feature = "web")]
                    {
                        use gloo_timers::future::TimeoutFuture;
                        TimeoutFuture::new(1500).await;
                    }
                    let target = if accepted.redirect_to.starts_with('/') {
                        accepted.redirect_to
                    } else {
                        "/login".to_string()
                    };
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().set_href(&target);
                    }
                }
                Err(e) => {
                    let raw = e.to_string();
                    if let Some((field, msg)) = parse_field_error(&raw) {
                        field_error.set(Some((field, msg)));
                        let snapshot = state.read().clone();
                        if let AcceptState::Submitting(p) = snapshot {
                            state.set(AcceptState::Ready(p));
                        }
                    } else if is_404(&raw) {
                        state.set(AcceptState::Error(invalid_message()));
                    } else if is_409(&raw) {
                        state.set(AcceptState::Error(
                            "This invite has already been accepted. Please sign in.".into(),
                        ));
                    } else {
                        state.set(AcceptState::Error(format!("Could not accept invite: {raw}")));
                    }
                }
            }
        });
    };

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                match state.read().clone() {
                    AcceptState::Loading => rsx! {
                        div { class: "text-center py-12",
                            p { class: "text-gray-600 dark:text-gray-300", "Loading invite..." }
                        }
                    },
                    AcceptState::Error(msg) => rsx! {
                        div { class: "space-y-4",
                            h2 { class: "text-xl font-semibold text-gray-900 dark:text-white text-center", "Invite not available" }
                            p { class: "text-sm text-gray-600 dark:text-gray-300 text-center", "{msg}" }
                            div { class: "text-center",
                                a { href: "/login",
                                    class: "text-sm text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                    "Go to sign in"
                                }
                            }
                        }
                    },
                    AcceptState::Done(email) => rsx! {
                        div { class: "space-y-3 text-center",
                            h2 { class: "text-xl font-semibold text-gray-900 dark:text-white", "Done" }
                            p { class: "text-sm text-gray-600 dark:text-gray-300", "Welcome, {email}." }
                            p { class: "text-xs text-gray-500", "Redirecting you to sign in..." }
                        }
                    },
                    AcceptState::Ready(preview) | AcceptState::Submitting(preview) => {
                        let busy = matches!(*state.read(), AcceptState::Submitting(_));
                        let is_join = preview.kind == "join_tenant";
                        let pwd_err = field_error.read().as_ref()
                            .filter(|(f, _)| f == "password")
                            .map(|(_, m)| m.clone())
                            .unwrap_or_default();
                        let pwd2_err = field_error.read().as_ref()
                            .filter(|(f, _)| f == "password2")
                            .map(|(_, m)| m.clone())
                            .unwrap_or_default();
                        rsx! {
                            div { class: "space-y-4",
                                h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                                    "You're invited to {preview.tenant_name}"
                                }
                                div { class: "rounded-md bg-gray-50 dark:bg-gray-800 p-4 space-y-1",
                                    p { class: "text-sm",
                                        span { class: "font-medium", "Email: " }
                                        "{preview.email}"
                                    }
                                    p { class: "text-sm",
                                        span { class: "font-medium", "Role: " }
                                        "{role_label(&preview.role)}"
                                    }
                                    p { class: "text-sm",
                                        span { class: "font-medium", "Invited by: " }
                                        "{preview.invited_by_name} ({preview.invited_by_email})"
                                    }
                                    if let Some(note) = &preview.note {
                                        if !note.is_empty() {
                                            blockquote { class: "mt-2 border-l-4 border-gray-300 pl-3 italic text-sm text-gray-600 dark:text-gray-400",
                                                "{note}"
                                            }
                                        }
                                    }
                                }

                                form {
                                    class: "space-y-4",
                                    onsubmit: move |e| { e.prevent_default(); submit(); },

                                    if is_join {
                                        // Existing user: no password collection.
                                        // The user already has one; accepting
                                        // just adds them to the tenant.
                                        p { class: "text-sm text-gray-600 dark:text-gray-300",
                                            "An account with this email already exists. "
                                            "Accepting will add it to "
                                            span { class: "font-medium", "{preview.tenant_name}" }
                                            "."
                                        }
                                    } else {
                                        // New account: password setup.
                                        p { class: "text-sm text-gray-600 dark:text-gray-300",
                                            "Set a password to finish creating your account."
                                        }
                                        Input {
                                            name: "first_name".to_string(),
                                            label: "First name".to_string(),
                                            r#type: "text".to_string(),
                                            placeholder: "Optional".to_string(),
                                            value: first_name.read().clone(),
                                            oninput: move |e: FormEvent| first_name.set(e.value()),
                                        }
                                        Input {
                                            name: "last_name".to_string(),
                                            label: "Last name".to_string(),
                                            r#type: "text".to_string(),
                                            placeholder: "Optional".to_string(),
                                            value: last_name.read().clone(),
                                            oninput: move |e: FormEvent| last_name.set(e.value()),
                                        }
                                        Input {
                                            name: "password".to_string(),
                                            label: "Password".to_string(),
                                            r#type: "password".to_string(),
                                            required: true,
                                            value: password.read().clone(),
                                            oninput: move |e: FormEvent| password.set(e.value()),
                                            help: "At least 12 characters with upper, lower, digit, and symbol.".to_string(),
                                            error: pwd_err,
                                        }
                                        Input {
                                            name: "password2".to_string(),
                                            label: "Confirm password".to_string(),
                                            r#type: "password".to_string(),
                                            required: true,
                                            value: password2.read().clone(),
                                            oninput: move |e: FormEvent| password2.set(e.value()),
                                            error: pwd2_err,
                                        }
                                    }

                                    Button {
                                        r#type: "submit".to_string(),
                                        variant: ButtonVariant::Primary,
                                        class: "w-full".to_string(),
                                        loading: busy,
                                        if is_join { "Join {preview.tenant_name}" } else { "Accept invite" }
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

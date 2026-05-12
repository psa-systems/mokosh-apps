//! `/settings/sessions` - the signed-in user's "active sessions" list.
//! Shows every device currently logged into this account and lets the
//! user revoke any of them. Revoking the session the user is on
//! signs them out (server kills the OP session and refresh family;
//! the SPA then 401s on the next request and the route guards send
//! them back to /login).

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, PageHeader};
use crate::hooks::use_require_auth;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SessionView {
    id: String,
    created_at: DateTime<Utc>,
    last_active_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    ip: Option<String>,
    /// `true` for the row that matches the access token the SPA
    /// used to fetch this list. The server identifies it via the
    /// `mokosh_op_session_id` claim. Older servers omit the field;
    /// the row falls back to the standard "no badge, can revoke"
    /// presentation.
    #[serde(default)]
    is_current: bool,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SessionListBody {
    sessions: Vec<SessionView>,
}

#[component]
pub fn SessionsPage() -> Element {
    let _ = use_require_auth();
    let mut sessions: Signal<Option<Result<Vec<SessionView>, String>>> = use_signal(|| None);
    let mut revoking: Signal<Option<String>> = use_signal(|| None);

    let load = use_callback(move |_| {
        spawn(async move {
            sessions.set(None);
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let result = crate::modules::oidc::issuer_get_authed::<SessionListBody>(
                &cfg,
                "/v1/auth/sessions",
            )
            .await
            .map(|b| b.sessions)
            .map_err(|e| e.to_string());
            sessions.set(Some(result));
        });
    });

    use_effect(move || {
        load.call(());
    });

    // Callback (Copy) so each row gets a cheap reference. Captures
    // `load` and the revoking signal; spawn handles the await.
    //
    // The `is_current` flag is the trigger for a full client-side
    // sign-out: server revoke alone leaves the still-valid (up to its
    // ~10-minute exp) access token in memory + sessionStorage, so the
    // user can keep clicking around until the next refresh fails. We
    // mirror `use_logout` for that case: clear the persisted token
    // bundle and replace the history entry with /login.
    let revoke = use_callback(move |(id, is_current): (String, bool)| {
        revoking.set(Some(id.clone()));
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/sessions/{id}/revoke");
            let _ = crate::modules::oidc::issuer_post_authed_empty(&cfg, &path).await;
            if is_current {
                crate::modules::oidc::storage::clear_auth();
                crate::hooks::fetch::api::set_access_token(None);
                if let Some(win) = web_sys::window() {
                    let _ = win.location().replace("/login");
                }
                return;
            }
            revoking.set(None);
            load.call(());
        });
    });

    rsx! {
        AppLayout { title: "Active sessions",
            PageHeader {
                title: "Active sessions".to_string(),
                subtitle: "Devices currently signed in to your account".to_string(),
            }

            Card {
                match sessions.read().clone() {
                    None => rsx! {
                        p { class: "text-sm text-gray-500 p-4", "Loading..." }
                    },
                    Some(Err(e)) => rsx! {
                        p { class: "text-sm text-red-600 p-4", "Could not load sessions: {e}" }
                    },
                    Some(Ok(list)) if list.is_empty() => rsx! {
                        p { class: "text-sm text-gray-500 p-4", "No active sessions." }
                    },
                    Some(Ok(list)) => rsx! {
                        div { class: "divide-y divide-gray-200 dark:divide-gray-700",
                            for s in list {
                                SessionRow {
                                    key: "{s.id}",
                                    session: s.clone(),
                                    revoking_id: revoking.read().clone(),
                                    on_revoke: {
                                        let id = s.id.clone();
                                        let is_current = s.is_current;
                                        move |_| revoke.call((id.clone(), is_current))
                                    },
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SessionRowProps {
    session: SessionView,
    revoking_id: Option<String>,
    on_revoke: EventHandler<()>,
}

/// Best-effort short label for a UA string. We do not pull a parser
/// crate; the heuristic just picks the most relevant browser + OS
/// fragment so the Sessions row reads "Chrome on Windows" rather than
/// 200 characters of UA. Falls back to "Unknown device".
fn ua_short_label(ua: Option<&str>) -> String {
    let Some(ua) = ua else {
        return "Unknown device".into();
    };
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome/") && !ua.contains("Chromium/") {
        "Chrome"
    } else if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Safari/") && !ua.contains("Chrome/") {
        "Safari"
    } else {
        "Browser"
    };
    let os = if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "device"
    };
    format!("{browser} on {os}")
}

#[component]
fn SessionRow(props: SessionRowProps) -> Element {
    let s = props.session.clone();
    let busy = props.revoking_id.as_deref() == Some(s.id.as_str());
    let default_label = ua_short_label(s.user_agent.as_deref());
    let label = s
        .display_name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| default_label.clone());
    let ip = s.ip.clone().unwrap_or_else(|| "unknown".to_string());
    let last_active = s.last_active_at.format("%Y-%m-%d %H:%M UTC").to_string();
    let signed_in = s.created_at.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| s.display_name.clone().unwrap_or_default());
    let mut saving = use_signal(|| false);

    let id_for_save = s.id.clone();
    let save = use_callback(move |_| {
        let id = id_for_save.clone();
        let value = draft.read().trim().to_string();
        spawn(async move {
            saving.set(true);
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/sessions/{id}/rename");
            let body = serde_json::json!({
                "display_name": if value.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(value) }
            });
            let _ = crate::modules::oidc::issuer_post_authed::<serde_json::Value, _>(
                &cfg, &path, &body,
            )
            .await;
            saving.set(false);
            editing.set(false);
            // Bump the parent's list refresh via window event - simpler
            // than threading another callback through; the page re-fetches
            // on mount + when the user navigates back.
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        });
    });

    rsx! {
        div { class: "flex items-start justify-between gap-4 p-4",
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2 flex-wrap",
                    if *editing.read() {
                        input {
                            r#type: "text",
                            class: "block rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white text-sm",
                            placeholder: "{default_label}",
                            value: "{draft.read()}",
                            oninput: move |e| draft.set(e.value()),
                        }
                        button {
                            r#type: "button",
                            class: "text-xs text-blue-600 hover:text-blue-500",
                            disabled: *saving.read(),
                            onclick: move |_| save.call(()),
                            if *saving.read() { "Saving..." } else { "Save" }
                        }
                        button {
                            r#type: "button",
                            class: "text-xs text-gray-500 hover:text-gray-700",
                            onclick: move |_| {
                                editing.set(false);
                                draft.set(s.display_name.clone().unwrap_or_default());
                            },
                            "Cancel"
                        }
                    } else {
                        p { class: "text-sm font-medium text-gray-900 dark:text-white truncate",
                            "{label}"
                        }
                        button {
                            r#type: "button",
                            class: "text-xs text-gray-500 hover:text-blue-600",
                            onclick: move |_| editing.set(true),
                            "Rename"
                        }
                    }
                    if s.is_current {
                        Badge { variant: BadgeVariant::Green, "Current session" }
                    }
                }
                p { class: "mt-1 text-xs text-gray-500 dark:text-gray-400",
                    "{default_label} - IP {ip} - last active {last_active}"
                }
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "Signed in {signed_in}"
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                loading: busy,
                onclick: move |_| props.on_revoke.call(()),
                if s.is_current { "Sign out" } else { "Revoke" }
            }
        }
    }
}

//! `/settings/sessions` - the signed-in user's "active sessions" list.
//! Shows every device currently logged into this account and lets the
//! user revoke any of them. Revoking the session the user is on
//! signs them out (server kills the OP session and refresh family;
//! the SPA then 401s on the next request and the route guards send
//! them back to /login).

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Button, ButtonVariant, Card, PageHeader};
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
    let revoke = use_callback(move |id: String| {
        revoking.set(Some(id.clone()));
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = format!("/v1/auth/sessions/{id}/revoke");
            let _ = crate::modules::oidc::issuer_post_authed_empty(&cfg, &path).await;
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
                                        move |_| revoke.call(id.clone())
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

#[component]
fn SessionRow(props: SessionRowProps) -> Element {
    let s = &props.session;
    let busy = props.revoking_id.as_deref() == Some(s.id.as_str());
    let device = s
        .user_agent
        .clone()
        .unwrap_or_else(|| "Unknown device".to_string());
    let ip = s.ip.clone().unwrap_or_else(|| "unknown".to_string());
    let last_active = s.last_active_at.format("%Y-%m-%d %H:%M UTC").to_string();
    let signed_in = s.created_at.format("%Y-%m-%d %H:%M UTC").to_string();

    rsx! {
        div { class: "flex items-start justify-between gap-4 p-4",
            div { class: "min-w-0 flex-1",
                p { class: "text-sm font-medium text-gray-900 dark:text-white truncate",
                    "{device}"
                }
                p { class: "mt-1 text-xs text-gray-500 dark:text-gray-400",
                    "IP {ip} - last active {last_active}"
                }
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "Signed in {signed_in}"
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                loading: busy,
                onclick: move |_| props.on_revoke.call(()),
                "Revoke"
            }
        }
    }
}

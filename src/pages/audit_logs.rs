//! Settings -> Audit logs (admin-only). Paginated table off
//! `/v1/auth/audit-logs`.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Card, PageHeader, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AuditView {
    id: String,
    #[serde(default)]
    actor_id: Option<String>,
    event_kind: String,
    severity: String,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
struct ListBody {
    entries: Vec<AuditView>,
    limit: i64,
    offset: i64,
}

#[component]
pub fn AuditLogsPage() -> Element {
    let _auth = crate::hooks::auth::use_require_role("admin");
    let mut data: Signal<Option<Result<ListBody, String>>> = use_signal(|| None);
    let mut offset = use_signal(|| 0i64);
    let mut kind_filter = use_signal(String::new);
    let mut bump = use_signal(|| 0u32);

    use_future(move || async move {
        let _ = bump.read();
        data.set(None);
        let cfg = crate::modules::oidc::OidcConfig::from_env();
        let off = *offset.read();
        let kind = kind_filter.read().trim().to_string();
        let mut path = format!("/v1/auth/audit-logs?limit=50&offset={off}");
        if !kind.is_empty() {
            // Minimal URL-encode: the values we expect are snake_case
            // event_kind strings; we still escape the few non-alnum
            // chars conservatively so a copy-paste with whitespace
            // works.
            let encoded: String = kind
                .chars()
                .map(|c| match c {
                    'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '.' => c.to_string(),
                    _ => format!("%{:02X}", c as u32),
                })
                .collect();
            path.push_str(&format!("&kind={encoded}"));
        }
        let r = crate::modules::oidc::issuer_get_authed::<ListBody>(&cfg, &path)
            .await
            .map_err(|e| e.to_string());
        data.set(Some(r));
    });

    let on_filter = use_callback(move |_| {
        offset.set(0);
        bump.with_mut(|n| *n += 1);
    });
    let prev = use_callback(move |_| {
        let off = *offset.read();
        offset.set((off - 50).max(0));
        bump.with_mut(|n| *n += 1);
    });
    let next = use_callback(move |_| {
        let off = *offset.read();
        offset.set(off + 50);
        bump.with_mut(|n| *n += 1);
    });

    rsx! {
        AppLayout { title: "Audit logs",
            PageHeader {
                title: "Audit logs",
                subtitle: "Security-relevant events recorded by the auth subsystem",
            }
            div { class: "space-y-4",
                Card { title: "Filters",
                    form {
                        class: "flex gap-2 items-end",
                        onsubmit: move |e| {
                            e.prevent_default();
                            on_filter.call(());
                        },
                        div { class: "flex-1",
                            label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300", "Event kind" }
                            input {
                                r#type: "text",
                                class: "mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm",
                                placeholder: "e.g. login_failed, mfa_challenge_consumed",
                                value: "{kind_filter.read()}",
                                oninput: move |e| kind_filter.set(e.value()),
                            }
                        }
                        button {
                            r#type: "submit",
                            class: "inline-flex items-center px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-blue-600 hover:bg-blue-500 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500",
                            "Filter"
                        }
                    }
                }

                Card { title: "Events",
                    match &*data.read() {
                        None => rsx! { p { class: "text-sm text-gray-500", "Loading..." } },
                        Some(Err(e)) => rsx! { p { class: "text-sm text-red-600", "Failed to load: {e}" } },
                        Some(Ok(body)) => rsx! {
                            div { class: "overflow-x-auto",
                                Table {
                                    TableHead {
                                        TableRow {
                                            TableHeader { "When" }
                                            TableHeader { "Event" }
                                            TableHeader { "Severity" }
                                            TableHeader { "Actor" }
                                            TableHeader { "IP" }
                                        }
                                    }
                                    TableBody {
                                        if body.entries.is_empty() {
                                            TableRow {
                                                TableCell {
                                                    class: "text-sm text-gray-500".to_string(),
                                                    "No entries"
                                                }
                                            }
                                        } else {
                                            for row in body.entries.iter() {
                                                {
                                                    let ts = row.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                                                    rsx! {
                                                TableRow {
                                                    TableCell {
                                                        class: "text-xs whitespace-nowrap".to_string(),
                                                        "{ts}"
                                                    }
                                                    TableCell { class: "text-sm".to_string(), "{row.event_kind}" }
                                                    TableCell { class: "text-sm".to_string(),
                                                        Badge { variant: severity_badge(&row.severity), "{row.severity}" }
                                                    }
                                                    TableCell {
                                                        class: "text-xs font-mono".to_string(),
                                                        match &row.actor_id {
                                                            Some(a) => rsx! { "{a}" },
                                                            None => rsx! { span { class: "text-gray-400", "-" } },
                                                        }
                                                    }
                                                    TableCell {
                                                        class: "text-xs".to_string(),
                                                        match &row.ip {
                                                            Some(ip) => rsx! { "{ip}" },
                                                            None => rsx! { span { class: "text-gray-400", "-" } },
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
                            div { class: "flex gap-2 mt-4 justify-end items-center",
                                span { class: "text-sm text-gray-500",
                                    "Showing rows {body.offset + 1}-{body.offset + body.entries.len() as i64}"
                                }
                                button {
                                    r#type: "button",
                                    class: "inline-flex items-center px-3 py-1 border border-gray-300 rounded-md text-sm",
                                    disabled: *offset.read() == 0,
                                    onclick: move |_| prev.call(()),
                                    "Previous"
                                }
                                button {
                                    r#type: "button",
                                    class: "inline-flex items-center px-3 py-1 border border-gray-300 rounded-md text-sm",
                                    disabled: body.entries.len() < body.limit as usize,
                                    onclick: move |_| next.call(()),
                                    "Next"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

fn severity_badge(s: &str) -> BadgeVariant {
    match s {
        "critical" => BadgeVariant::Red,
        "warning" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    }
}

//! `/settings/active-tenant` - tenant switcher.
//!
//! Lists every tenant the user has an active membership in, marks the
//! current one, and lets them switch. Switching POSTs to
//! `/v1/auth/active-tenant`; the server reissues a fresh access + id
//! + refresh token bundle with the new `mokosh_active_tenant` claim,
//! we replace the in-memory + sessionStorage tokens, and reload so
//! every page refetches data scoped to the new tenant.
//!
//! Phase 4 of docs/mokosh-auth/10-memberships-and-self-signup.md.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, PageHeader};
use crate::hooks::use_require_auth;

#[derive(Clone, Debug, Deserialize)]
struct SwitchOk {
    active_tenant_id: String,
    tokens: TokenBundle,
}

#[derive(Clone, Debug, Deserialize)]
struct TokenBundle {
    access_token: String,
    id_token: String,
    refresh_token: String,
    expires_in: i64,
    scope: String,
}

#[component]
pub fn ActiveTenantPage() -> Element {
    let auth = use_require_auth();
    let mut busy: Signal<Option<String>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let mut switch = move |tenant_id: String| {
        busy.set(Some(tenant_id.clone()));
        error.set(None);
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let body = serde_json::json!({
                "tenant_id": tenant_id,
                "client_id": cfg.client_id,
            });
            match crate::modules::oidc::issuer_post_authed::<SwitchOk, _>(
                &cfg,
                "/v1/auth/active-tenant",
                &body,
            )
            .await
            {
                Ok(resp) => {
                    // Replace stored tokens with the new bundle. The
                    // expires_at is computed fresh on every issuance.
                    let new_expires =
                        Utc::now() + chrono::Duration::seconds(resp.tokens.expires_in.max(0));
                    crate::modules::oidc::storage::save_auth(
                        &crate::modules::oidc::storage::StoredTokens {
                            access_token: resp.tokens.access_token.clone(),
                            id_token: resp.tokens.id_token.clone(),
                            refresh_token: Some(resp.tokens.refresh_token.clone()),
                            expires_at: new_expires,
                            scope: resp.tokens.scope.clone(),
                        },
                    );
                    crate::hooks::fetch::api::set_access_token(Some(
                        resp.tokens.access_token.clone(),
                    ));
                    // Hard reload: every page should refetch its data
                    // under the new active tenant. Cleaner than
                    // surgically invalidating individual fetches.
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().reload();
                    }
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    busy.set(None);
                }
            }
        });
    };

    let memberships = auth.read().memberships.clone();

    rsx! {
        AppLayout { title: "Switch tenant",
            PageHeader {
                title: "Switch tenant".to_string(),
                subtitle: "Pick which tenant you want to act under.".to_string(),
            }

            if let Some(msg) = error.read().as_ref() {
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4 mb-4",
                    p { class: "text-sm text-red-600 dark:text-red-400", "Could not switch tenant: {msg}" }
                }
            }

            Card { padding: false,
                if memberships.is_empty() {
                    div { class: "p-8 text-center text-gray-500",
                        "No memberships loaded yet. Try refreshing the page."
                    }
                } else {
                    div { class: "divide-y divide-gray-200 dark:divide-gray-700",
                        for m in memberships {
                            TenantRow {
                                key: "{m.tenant_id}",
                                membership: m.clone(),
                                busy_id: busy.read().clone(),
                                on_switch: {
                                    let id = m.tenant_id.clone();
                                    move |_| switch(id.clone())
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TenantRowProps {
    membership: crate::hooks::auth::MembershipView,
    busy_id: Option<String>,
    on_switch: EventHandler<()>,
}

#[component]
fn TenantRow(props: TenantRowProps) -> Element {
    let m = &props.membership;
    let busy = props.busy_id.as_deref() == Some(m.tenant_id.as_str());
    let kind_variant = match m.tenant_kind.as_str() {
        "personal" => BadgeVariant::Blue,
        _ => BadgeVariant::Gray,
    };
    let role_label = match m.role.as_str() {
        "admin" => "Admin",
        "manager" => "Manager",
        "finance" => "Finance",
        "member" => "Member",
        "readonly" => "Read only",
        _ => "Other",
    };

    rsx! {
        div { class: "flex items-center justify-between gap-4 p-4",
            div { class: "min-w-0 flex-1 space-y-1",
                div { class: "flex items-center gap-2 flex-wrap",
                    p { class: "text-sm font-medium text-gray-900 dark:text-white",
                        "{m.tenant_name}"
                    }
                    Badge { variant: kind_variant, "{m.tenant_kind}" }
                    if m.is_active {
                        Badge { variant: BadgeVariant::Green, "Current" }
                    }
                }
                p { class: "text-xs text-gray-500 dark:text-gray-400",
                    "Your role: {role_label}"
                }
            }
            if m.is_active {
                span { class: "text-sm text-gray-500", "active" }
            } else {
                Button {
                    variant: ButtonVariant::Secondary,
                    loading: busy,
                    onclick: move |_| props.on_switch.call(()),
                    "Switch"
                }
            }
        }
    }
}

//! MAPPS-497 item 6: dedicated `/pick-tenant` route for the
//! phase-3 `needs_selection` login branch. Reads its state from the
//! `PENDING_LOGIN` global signal; redirects back to `/login` when the
//! state is empty (browser refresh, direct deep-link).
//!
//! Extracted from the phase-3 inline block in `pages/login.rs` so:
//! - The URL reflects the current step (better back-button and
//!   deep-link semantics).
//! - A refresh mid-flow lands on `/pick-tenant` -> empty signal ->
//!   redirect to `/login` (matches the expired-identity-token
//!   handling that lives on the login page already).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant};
use crate::hooks::pending_login::{PendingLogin, PENDING_LOGIN};
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::{CurrentUser, Route};

#[derive(Serialize)]
pub(crate) struct SelectTenantBody {
    pub(crate) identity_token: String,
    pub(crate) tenant_id: String,
}

#[derive(Deserialize)]
struct LoginResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    user: Option<CurrentUser>,
}

#[component]
pub fn PickTenantPage() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // Redirect to /login when the pending state is empty. Runs on
    // every render so a browser refresh (which wipes the global
    // signal) bounces the operator back to sign in cleanly.
    let pending = PENDING_LOGIN.read().clone();
    if pending.identity_token.is_none() || pending.memberships.is_empty() {
        nav.replace(Route::Login {});
        return rsx! { Fragment {} };
    }

    let mut pick = move |tenant_id: String| {
        if saving() {
            return;
        }
        let Some(token) = PENDING_LOGIN.read().identity_token.clone() else {
            nav.replace(Route::Login {});
            return;
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = SelectTenantBody {
                    identity_token: token,
                    tenant_id,
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                    "/auth/select-tenant",
                    &body,
                )
                .await
                {
                    Ok(resp) => {
                        if let Some(user) = resp.user {
                            crate::hooks::fetch::api::set_access_token(Some(
                                resp.access_token.clone(),
                            ));
                            save_standalone(&StandaloneSession {
                                access_token: resp.access_token,
                                refresh_token: resp.refresh_token,
                                expires_at: resp.expires_at,
                                user: user.clone(),
                            });
                            let active_tenant_id = Some(user.tenant_id);
                            {
                                let mut a = auth.write();
                                a.user = Some(user);
                                a.is_loading = false;
                                a.error = None;
                                a.tokens = None;
                                a.active_tenant_id = active_tenant_id;
                                a.memberships = Vec::new();
                                a.server_loaded = false;
                            }
                            *PENDING_LOGIN.write() = PendingLogin::default();
                            nav.replace(Route::Dashboard {});
                        } else {
                            error.set("Sign-in succeeded but no account was returned.".to_string());
                        }
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        // Identity token expired; wipe state and route
                        // back to /login so the operator re-enters
                        // email + password.
                        *PENDING_LOGIN.write() = PendingLogin::default();
                        nav.replace(Route::Login {});
                    }
                    Err(ApiError::Status { code: 404, .. }) => {
                        error.set(
                            "You do not have access to that workspace. Pick another.".to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "text-center mb-6",
                h1 { class: "text-2xl font-semibold text-content", "Choose a workspace" }
                p { class: "mt-2 text-sm text-content",
                    "You belong to more than one Mokosh organization. Pick one to sign in to."
                }
            }
            ul { class: "space-y-2",
                {pending.memberships.iter().map(|m| {
                    let tenant_id = m.tenant_id.clone();
                    let label_name = m.tenant_name.clone();
                    let label_role = m.role.clone();
                    rsx! {
                        li { key: "{tenant_id}",
                            button {
                                r#type: "button",
                                class: "w-full text-left px-4 py-3 rounded-md border border-line hover:bg-surface-2 focus:outline-none focus:ring-2 focus:ring-accent",
                                disabled: saving(),
                                onclick: {
                                    let tenant_id = tenant_id.clone();
                                    move |_| pick(tenant_id.clone())
                                },
                                div { class: "font-medium text-content", "{label_name}" }
                                div { class: "text-sm text-muted", "Role: {label_role}" }
                            }
                        }
                    }
                })}
            }
            if !error().is_empty() {
                p { role: "alert", class: "mt-4 text-sm text-red-600 dark:text-red-400", "{error}" }
            }
            div { class: "pt-4",
                Button {
                    variant: ButtonVariant::Secondary,
                    r#type: "button".to_string(),
                    disabled: saving(),
                    class: "w-full".to_string(),
                    onclick: move |_| {
                        *PENDING_LOGIN.write() = PendingLogin::default();
                        nav.replace(Route::Login {});
                    },
                    "Back to sign in"
                }
            }
        }
    }
}

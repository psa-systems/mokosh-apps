//! MAPPS-497 item 6: dedicated `/create-org` route for the phase-4
//! `needs_setup` login branch (zero-membership self-serve). Reads the
//! identity_token from the `PENDING_LOGIN` global signal; redirects
//! back to `/login` when the state is empty.
//!
//! Extracted from the phase-4 inline block in `pages/login.rs` for
//! the same reasons as the sibling `pick_tenant.rs`: a URL for the
//! step, better back-button + deep-link behaviour, no cross-page
//! state juggling on the login page itself.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::hooks::pending_login::{PendingLogin, PENDING_LOGIN};
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::{CurrentUser, Route};

#[derive(Serialize)]
struct SelfServeTenantBody {
    identity_token: String,
    tenant_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_slug: Option<String>,
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
pub fn CreateOrgPage() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();
    let mut name = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let has_token = PENDING_LOGIN.read().identity_token.is_some();
    if !has_token {
        nav.replace(Route::Login {});
        return rsx! { Fragment {} };
    }

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let Some(token) = PENDING_LOGIN.read().identity_token.clone() else {
            nav.replace(Route::Login {});
            return;
        };
        let n = name.read().trim().to_string();
        if n.is_empty() {
            error.set("Enter an organization name.".to_string());
            return;
        }
        let raw = slug.read().trim().to_ascii_lowercase();
        let s = if raw.is_empty() { None } else { Some(raw) };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = SelfServeTenantBody {
                    identity_token: token,
                    tenant_name: n,
                    tenant_slug: s,
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                    "/tenants/self-serve",
                    &body,
                )
                .await
                {
                    Ok(resp) => match resp.user {
                        Some(user) => {
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
                        }
                        None => {
                            error.set(
                                "Organization created but no session was returned.".to_string(),
                            );
                        }
                    },
                    Err(ApiError::Status { code: 401, .. }) => {
                        *PENDING_LOGIN.write() = PendingLogin::default();
                        nav.replace(Route::Login {});
                    }
                    Err(ApiError::Status {
                        code: 409, message, ..
                    }) => {
                        error.set(message);
                    }
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) => {
                        error.set(message);
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
                h1 { class: "text-2xl font-semibold text-content", "Create your organization" }
                p { class: "mt-2 text-sm text-content",
                    "Pick a name for your Mokosh workspace. You will be the first admin."
                }
            }
            form {
                class: "space-y-4",
                onsubmit: move |evt: Event<FormData>| {
                    evt.prevent_default();
                    submit(());
                },
                Input {
                    name: "tenant_name",
                    label: "Organization name",
                    r#type: "text".to_string(),
                    value: name(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| {
                        error.set(String::new());
                        name.set(e.value());
                    },
                }
                Input {
                    name: "tenant_slug",
                    label: "Portal slug (optional)",
                    r#type: "text".to_string(),
                    value: slug(),
                    required: false,
                    disabled: saving(),
                    oninput: move |e: FormEvent| {
                        error.set(String::new());
                        slug.set(e.value());
                    },
                }
                p { class: "text-xs text-muted",
                    "Leave blank to derive from the name. Slugs are used only for the client-facing portal URL."
                }
                if !error().is_empty() {
                    p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                div { class: "pt-2 space-y-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: saving(),
                        loading: saving(),
                        r#type: "submit".to_string(),
                        class: "w-full".to_string(),
                        "Create organization"
                    }
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
}

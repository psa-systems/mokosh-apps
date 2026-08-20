//! MAPPS-494 (MAPPS-474 phase 5): in-app tenant switcher.
//!
//! Renders a compact dropdown in the TopBar that lists every membership
//! the identity holds, marks the active one, lets the operator switch
//! between tenants without re-logging-in, and offers a "Create new
//! organization" action to spin up an additional org.
//!
//! Wire path:
//! - Switch: POST `/api/v1/auth/switch-tenant/:tenant_id` -> LoginResponse.
//!   Install the returned session (mirrors the login page's install path)
//!   and navigate to Dashboard so tenant-scoped queries refetch.
//! - Create: POST `/api/v1/tenants/additional` -> TenantResponse.
//!   Refetch the membership list so the new tenant appears in the
//!   dropdown; the operator can then switch into it.
//!
//! Standalone-mode note: the existing `use_memberships_loader` fetches
//! from bunyip's endpoint and returns a synthetic single-membership
//! fallback for standalone deployments. This component uses its own
//! mokosh-side loader so a self-hosted standalone install sees real
//! membership rows.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Button, ButtonVariant, Input, Modal, ModalSize};
use crate::hooks::auth::MembershipView;
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::{CurrentUser, Route};

/// GET /auth/memberships response shape.
#[derive(Deserialize)]
struct MembershipsResp(Vec<MembershipView>);

/// POST /auth/switch-tenant response shape (subset of LoginResponse).
#[derive(Deserialize)]
struct SwitchResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
    user: CurrentUser,
}

/// POST /tenants/additional request body.
#[derive(Serialize)]
struct AdditionalBody {
    tenant_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_slug: Option<String>,
}

/// POST /tenants/additional response shape (subset of TenantResponse).
#[derive(Deserialize)]
struct TenantResp {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[component]
pub fn TenantSwitcher() -> Element {
    let auth = crate::hooks::use_auth();
    let mut auth_write = auth;
    let nav = use_navigator();

    let mut open = use_signal(|| false);
    let mut show_create = use_signal(|| false);
    let mut new_org_name = use_signal(String::new);
    let mut new_org_slug = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    // Guards against re-firing the load-once effect after we've already
    // populated memberships from mokosh's endpoint.
    let mut load_tried = use_signal(|| false);

    // MAPPS-494 mokosh-side membership loader. Runs on mount when the
    // identity is authenticated and the current membership list is
    // empty or size 1 (the phase-3 install_session clears it for
    // standalone). Doesn't clobber a populated list.
    use_effect(move || {
        if *load_tried.peek() {
            return;
        }
        let needs = {
            let a = auth_write.read();
            a.is_authenticated() && a.memberships.is_empty()
        };
        if !needs {
            return;
        }
        load_tried.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                if let Ok(list) = crate::hooks::fetch::api::get_authed_typed::<Vec<MembershipView>>(
                    "/auth/memberships",
                )
                .await
                {
                    if !list.is_empty() {
                        let mut a = auth_write.write();
                        a.memberships = list;
                    }
                }
            }
        });
    });

    // Close on route change (mirrors UserMenu).
    let route: Route = use_route();
    use_effect(use_reactive!(|route| {
        let _ = &route;
        if *open.peek() {
            open.set(false);
        }
    }));

    let install_session = move |access_token: String,
                                refresh_token: Option<String>,
                                expires_at: chrono::DateTime<chrono::Utc>,
                                user: CurrentUser| {
        crate::hooks::fetch::api::set_access_token(Some(access_token.clone()));
        save_standalone(&StandaloneSession {
            access_token,
            refresh_token,
            expires_at,
            user: user.clone(),
        });
        let active_tenant_id = Some(user.tenant_id);
        {
            let mut a = auth_write.write();
            a.user = Some(user);
            a.is_loading = false;
            a.error = None;
            a.tokens = None;
            a.active_tenant_id = active_tenant_id;
            // Clear so the loader-effect on the next mount refetches.
            a.memberships = Vec::new();
            a.server_loaded = false;
        }
        nav.replace(Route::Dashboard {});
    };

    let mut switch_to = move |tenant_id: String| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        let mut install_session = install_session;
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let path = format!("/auth/switch-tenant/{tenant_id}");
                match crate::hooks::fetch::api::post_authed_typed::<SwitchResp, ()>(&path, &())
                    .await
                {
                    Ok(resp) => {
                        install_session(
                            resp.access_token,
                            resp.refresh_token,
                            resp.expires_at,
                            resp.user,
                        );
                        open.set(false);
                    }
                    Err(ApiError::Status { code: 404, .. }) => {
                        error.set(
                            "You no longer have access to that workspace. Refresh and try again."
                                .to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            saving.set(false);
        });
    };

    let mut submit_create = move |_| {
        if saving() {
            return;
        }
        let name = new_org_name.read().trim().to_string();
        if name.is_empty() {
            error.set("Enter an organization name.".to_string());
            return;
        }
        let raw_slug = new_org_slug.read().trim().to_ascii_lowercase();
        let slug = if raw_slug.is_empty() {
            None
        } else {
            Some(raw_slug)
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = AdditionalBody {
                    tenant_name: name,
                    tenant_slug: slug,
                };
                match crate::hooks::fetch::api::post_authed_typed::<TenantResp, _>(
                    "/tenants/additional",
                    &body,
                )
                .await
                {
                    Ok(_created) => {
                        // Refetch memberships so the new tenant shows up
                        // in the dropdown. Best-effort: dropdown will
                        // still update on the next natural mount if
                        // this fails.
                        if let Ok(list) = crate::hooks::fetch::api::get_authed_typed::<
                            Vec<MembershipView>,
                        >("/auth/memberships")
                        .await
                        {
                            let mut a = auth_write.write();
                            a.memberships = list;
                        }
                        new_org_name.set(String::new());
                        new_org_slug.set(String::new());
                        show_create.set(false);
                        open.set(false);
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

    // Read the memberships + active for render. Hide the trigger when
    // there is nothing to show (unauthenticated / no memberships).
    let (memberships, active_name, active_id_str) = {
        let a = auth.read();
        let mut list = a.memberships.clone();
        let active_id = a.active_tenant_id.map(|u| u.to_string());
        let derived_name = a.active_org_name().map(str::to_string);
        let active_name = derived_name.unwrap_or_else(|| {
            list.iter()
                .find(|m| Some(m.tenant_id.clone()) == active_id)
                .map(|m| m.tenant_name.clone())
                .unwrap_or_default()
        });
        list.sort_by(|a, b| {
            a.tenant_name
                .to_ascii_lowercase()
                .cmp(&b.tenant_name.to_ascii_lowercase())
        });
        (list, active_name, active_id)
    };

    if !auth.read().is_authenticated() {
        return rsx! { Fragment {} };
    }

    rsx! {
        div { class: "relative",
            button {
                r#type: "button",
                class: "flex items-center gap-2 px-3 py-2 rounded-md text-sm text-subtle hover:text-content hover:bg-surface-2 focus:outline-none",
                aria_label: "Switch workspace",
                title: "Switch workspace",
                aria_expanded: if open() { "true" } else { "false" },
                aria_haspopup: "menu",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                    if next { error.set(String::new()); }
                },
                span { class: "hidden md:inline max-w-[10rem] truncate", "{active_name}" }
                // Small chevron caret drawn inline (avoids a dep on an
                // icon we don't own yet).
                svg {
                    class: "w-4 h-4",
                    view_box: "0 0 20 20",
                    fill: "currentColor",
                    path {
                        d: "M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.24 4.38a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z",
                    }
                }
            }
            if *open.read() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 mt-2 w-64 rounded-md shadow-lg bg-raised ring-1 ring-black/5 z-20 p-1",
                    role: "menu",
                    div { class: "px-3 py-2 text-xs uppercase tracking-wide text-subtle",
                        "Your organizations"
                    }
                    if memberships.is_empty() {
                        div { class: "px-3 py-2 text-sm text-content", "No memberships loaded." }
                    } else {
                        {memberships.iter().cloned().map(|m| {
                            let tenant_id = m.tenant_id.clone();
                            let is_active = Some(tenant_id.clone()) == active_id_str;
                            rsx! {
                                button {
                                    key: "{tenant_id}",
                                    r#type: "button",
                                    class: if is_active {
                                        "block w-full text-left rounded-md px-3 py-2 text-sm bg-surface-2 text-content"
                                    } else {
                                        "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2"
                                    },
                                    disabled: is_active || saving(),
                                    onclick: {
                                        let tenant_id = tenant_id.clone();
                                        move |_| switch_to(tenant_id.clone())
                                    },
                                    div { class: "font-medium truncate", "{m.tenant_name}" }
                                    div { class: "text-xs text-subtle",
                                        if is_active { "Active" } else { "{m.role}" }
                                    }
                                }
                            }
                        })}
                    }
                    div { class: "border-t border-line my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        r#type: "button",
                        onclick: move |_| {
                            show_create.set(true);
                            open.set(false);
                            error.set(String::new());
                        },
                        "Create new organization"
                    }
                    if !error().is_empty() {
                        p { role: "alert", class: "px-3 py-2 text-xs text-red-600 dark:text-red-400", "{error}" }
                    }
                }
            }
            Modal {
                open: show_create(),
                title: "Create new organization".to_string(),
                size: ModalSize::Small,
                onclose: move |_| show_create.set(false),
                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        submit_create(());
                    },
                    Input {
                        name: "tenant_name",
                        label: "Organization name",
                        r#type: "text".to_string(),
                        value: new_org_name(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            new_org_name.set(e.value());
                        },
                    }
                    Input {
                        name: "tenant_slug",
                        label: "Portal slug (optional)",
                        r#type: "text".to_string(),
                        value: new_org_slug(),
                        required: false,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            new_org_slug.set(e.value());
                        },
                    }
                    p { class: "text-xs text-subtle",
                        "Leave blank to derive from the name. Slugs are only used for the client-facing portal URL."
                    }
                    if !error().is_empty() {
                        p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                    div { class: "flex gap-2 justify-end pt-2",
                        Button {
                            variant: ButtonVariant::Secondary,
                            r#type: "button".to_string(),
                            disabled: saving(),
                            onclick: move |_| {
                                show_create.set(false);
                                new_org_name.set(String::new());
                                new_org_slug.set(String::new());
                                error.set(String::new());
                            },
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            r#type: "submit".to_string(),
                            disabled: saving(),
                            loading: saving(),
                            "Create"
                        }
                    }
                }
            }
        }
    }
}

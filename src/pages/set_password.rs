//! MAPPS-552: dedicated first-time password setup page.
//!
//! Reached by the welcome-email link a fresh client-admin receives
//! after their tenant is minted (see mokosh-server's
//! `TenantService::mint_and_send_welcome`, MAPPS-448 / MAPPS-552).
//! Distinct from `pages::reset_password::StandaloneResetPassword`
//! (which handles forgot-password for an existing tenant user) so
//! the URL bar reads `/set-password/{token}`, the page heading says
//! "Set your password for [Client Name]", and the copy matches
//! what a first-time setup actually is - not a reset.
//!
//! On mount, GET /api/v1/auth/set-password/context/{token} pulls the
//! tenant's human-readable name + slug so the heading can name the
//! specific client portal the recipient is signing up for. A 404
//! (invalid / expired / redeemed token) falls back to a generic
//! "Set your password" heading so a stale link still shows a coherent
//! page instead of a spinner.
//!
//! The submit posts `POST /api/v1/auth/reset-password` (the sole
//! password-write endpoint post-MAPPS-551 - setup and forgot-password
//! share the same server handler, which writes ONLY the specific
//! users row).
//!
//! MAPPS-553 fix: after success, the "Go to sign in" button hard-navs
//! to the tenant subdomain login (`<slug>.client.<suffix>/login`),
//! NOT `/login` on the apex. Pre-553 the button routed via the
//! Dioxus navigator to `Route::Login`, which on the mokosh apex sent
//! the fresh client-admin to the platform-admin unified login and
//! made the account look like "a mokosh platform account" instead of
//! a per-client credential (operator report 2026-08-24). Post-553 the
//! button crosses origins via `window.location.href` (the Dioxus
//! router cannot cross origins), and falls back to a same-origin
//! `/login` when either the tenant slug or the portal host suffix
//! is missing (dev without env, or a deploy that does not host the
//! portal on its own suffix).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct ResetBody {
    token: String,
    new_password: String,
    confirm_password: String,
}

#[derive(Deserialize, Clone, Debug)]
struct SetPasswordContext {
    tenant_name: String,
    tenant_slug: String,
}

#[component]
pub fn SetPasswordPage(token: String) -> Element {
    let nav = use_navigator();
    let mut new_pw = use_signal(String::new);
    let mut confirm_pw = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut tenant_slug_state: Signal<Option<String>> = use_signal(|| None);

    // Fetch the tenant name so the heading can name the client
    // portal. A failure (unknown/expired/redeemed token) lands as
    // `Some(None)` and we fall back to a generic heading instead of
    // blocking the form.
    let token_for_context = token.clone();
    let context_resource: Resource<Option<SetPasswordContext>> = use_resource(move || {
        let token = token_for_context.clone();
        async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/auth/set-password/context/{}", token);
                crate::hooks::fetch::api::get_typed::<SetPasswordContext>(&path)
                    .await
                    // Best-effort: the heading falls back to generic copy and
                    // the form still works, but an unknown, expired or already
                    // redeemed token is worth naming when the user asks why the
                    // page does not name their portal.
                    .inspect_err(|e| tracing::warn!("set-password context load failed: {e}"))
                    .ok()
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = token;
                None
            }
        }
    });
    let context_snap = context_resource.read_unchecked();
    let tenant_name: Option<String> = match &*context_snap {
        Some(Some(ctx)) => {
            let name = ctx.tenant_name.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        }
        _ => None,
    };
    // MAPPS-553: remember the tenant slug so the post-success button
    // can hard-nav to the tenant subdomain login. Stored in a signal
    // rather than derived inline in the click handler so the button
    // callback doesn't have to re-borrow the resource state (which
    // moves into `spawn` awkwardly). `use_effect` runs whenever the
    // resource resolves.
    let slug_from_ctx: Option<String> = match &*context_snap {
        Some(Some(ctx)) => {
            let s = ctx.tenant_slug.trim().to_ascii_lowercase();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    };
    let slug_effect_input = slug_from_ctx.clone();
    use_effect(move || {
        let next = slug_effect_input.clone();
        if *tenant_slug_state.peek() != next {
            tenant_slug_state.set(next);
        }
    });
    let heading = match tenant_name.as_deref() {
        Some(name) => format!("Set your password for {name}"),
        None => "Set your password".to_string(),
    };

    let token_for_submit = token.clone();
    let mut submit = move |_| {
        if saving() {
            return;
        }
        let pw = new_pw.read().clone();
        let confirm = confirm_pw.read().clone();
        if pw.len() < 12 {
            error.set("Password must be at least 12 characters.".to_string());
            return;
        }
        if pw != confirm {
            error.set("Passwords do not match.".to_string());
            return;
        }
        let tok = token_for_submit.clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = ResetBody {
                    token: tok,
                    new_password: pw,
                    confirm_password: confirm,
                };
                match crate::hooks::fetch::api::post_typed_no_content("/auth/reset-password", &body)
                    .await
                {
                    Ok(()) => {
                        done.set(true);
                    }
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) => {
                        error.set(if message.is_empty() {
                            "This setup link is invalid or has expired.".to_string()
                        } else {
                            message
                        });
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (tok, pw, confirm);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Password set" }
                    p { class: "mt-2 text-sm text-content",
                        if let Some(name) = tenant_name.as_deref() {
                            "You can now sign in to {name}."
                        } else {
                            "You can now sign in with the new password."
                        }
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| {
                            // MAPPS-553: post-set-password lands on
                            // the tenant subdomain login, not the
                            // mokosh apex `/login`. Requires a hard
                            // cross-origin navigation (the Dioxus
                            // router cannot cross origins). Falls
                            // back to same-origin `/login` when
                            // either the slug (context 404 / no
                            // tenant_slug) or the portal host suffix
                            // (dev without env, non-portal deploy)
                            // is missing.
                            #[cfg(target_arch = "wasm32")]
                            {
                                // MAPPS-649: the portal is one host now,
                                // so bounce to the portal root; the
                                // visitor enters their Company ID at
                                // step 1. Fall back to same-origin
                                // /login when no portal host is
                                // configured (dev without env).
                                let _ = tenant_slug_state.peek().clone();
                                if let Some(root) = crate::modules::runtime_config::portal_root_url() {
                                    let url = format!("{root}/portal/login");
                                    if let Some(win) = web_sys::window() {
                                        let _ = win.location().set_href(&url);
                                        return;
                                    }
                                }
                                nav.replace(Route::Login {});
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                nav.replace(Route::Login {});
                            }
                        },
                        "Go to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "{heading}" }
                    p { class: "mt-2 text-sm text-content",
                        "Choose a password you don't use anywhere else. This password is scoped to this client portal only."
                    }
                }
                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        submit(());
                    },
                    Input {
                        name: "new_password",
                        label: "New password",
                        r#type: "password".to_string(),
                        value: new_pw(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            new_pw.set(e.value());
                        },
                    }
                    Input {
                        name: "confirm_password",
                        label: "Confirm password",
                        r#type: "password".to_string(),
                        value: confirm_pw(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            confirm_pw.set(e.value());
                        },
                    }
                    if !error().is_empty() {
                        p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                    div { class: "pt-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: saving(),
                            loading: saving(),
                            r#type: "submit".to_string(),
                            class: "w-full".to_string(),
                            "Set password"
                        }
                    }
                }
            }
        }
    }
}

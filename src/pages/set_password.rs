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
//! users row). On success land on `/login` (the tenant subdomain
//! itself post the MAPPS-520 root routing; on the apex the same
//! route is fine, the operator picks up their tenant on next login).

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
    #[allow(dead_code)]
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
                        onclick: move |_| { nav.replace(Route::Login {}); },
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

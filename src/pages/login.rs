//! MAPPS-368: standalone username/password login.
//!
//! Shown by `Login` / `AuthGuard` only when no OIDC issuer is configured
//! (`crate::modules::oidc::is_standalone()`) - a self-hosted deployment with no
//! bunyip OP. Posts to mokosh-server's legacy `POST /api/v1/auth/login` and, on
//! success, seeds the in-memory `AuthContext` plus a persisted standalone
//! session so a page reload survives. Deployments that DO configure an issuer
//! never reach this page; they use the bunyip OIDC redirect exactly as before.
//!
//! Deferred (follow-ups, called out in the PR): silent token refresh via
//! `POST /api/v1/auth/refresh` (standalone sessions currently last the
//! access-token TTL, ~1h), and the MFA second factor (two-factor accounts are
//! told to sign in through their identity provider).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Button, ButtonVariant, Input};
use crate::modules::oidc::storage::{save_standalone, StandaloneSession};
use crate::CurrentUser;
use crate::Route;

/// Request body for `POST /api/v1/auth/login`, a subset of mokosh-server's
/// `LoginRequest` (the server defaults the omitted optional fields).
#[derive(Serialize)]
struct LoginBody {
    email: String,
    password: String,
    remember_me: bool,
    /// MAPPS-368: the TOTP / recovery code, sent on the second step after a
    /// first attempt reported `mfa_required`. Omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
}

/// The fields of mokosh-server's `LoginResponse` this SPA consumes. Unknown
/// fields are ignored by serde.
#[derive(Deserialize)]
struct LoginResp {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    user: Option<CurrentUser>,
    #[serde(default)]
    mfa_required: bool,
}

#[component]
pub fn StandaloneLogin() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-368: the TOTP field + prompt are revealed after the first attempt
    // reports `mfa_required`; the second submit resends with the code.
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your email and password.".to_string());
            return;
        }
        let code = mfa_code.read().trim().to_string();
        let mfa = if code.is_empty() { None } else { Some(code) };
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = LoginBody {
                    email: em.clone(),
                    password: pw.clone(),
                    remember_me: false,
                    mfa_code: mfa.clone(),
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>("/auth/login", &body)
                    .await
                {
                    // MFA enrolled but no valid code yet: reveal the code field
                    // and prompt. The next submit resends with `mfa_code`.
                    Ok(resp) if resp.mfa_required => {
                        mfa_needed.set(true);
                        error
                            .set("Enter the 6-digit code from your authenticator app.".to_string());
                    }
                    Ok(resp) => match resp.user {
                        Some(user) => {
                            crate::hooks::fetch::api::set_access_token(Some(
                                resp.access_token.clone(),
                            ));
                            save_standalone(&StandaloneSession {
                                access_token: resp.access_token.clone(),
                                refresh_token: resp.refresh_token.clone(),
                                expires_at: resp.expires_at,
                                user: user.clone(),
                            });
                            let active_tenant_id = Some(user.tenant_id);
                            {
                                let mut a = auth.write();
                                a.user = Some(user);
                                a.is_loading = false;
                                a.error = None;
                                // No OIDC tokens in a standalone session; the
                                // refresh hook no-ops when `tokens` is None.
                                a.tokens = None;
                                a.active_tenant_id = active_tenant_id;
                                a.memberships = Vec::new();
                                // The post-login `/me` loader reconciles the
                                // authoritative user within a tick.
                                a.server_loaded = false;
                            }
                            nav.replace(Route::Dashboard {});
                        }
                        None => {
                            error.set(
                                "Sign-in succeeded but no account was returned. Try again."
                                    .to_string(),
                            );
                        }
                    },
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid email or password.".to_string());
                    }
                    Err(ApiError::Status { code: 429, .. }) => {
                        error.set(
                            "Too many attempts. Please wait a moment and try again.".to_string(),
                        );
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (em, pw, mfa);
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content", "Sign in to Mokosh" }
                        p { class: "mt-2 text-sm text-content",
                            "Enter your account email and password."
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
                        },

                        Input {
                            name: "email",
                            label: "Email",
                            r#type: "email".to_string(),
                            value: email(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                email.set(e.value());
                            },
                        }

                        Input {
                            name: "password",
                            label: "Password",
                            r#type: "password".to_string(),
                            value: password(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                password.set(e.value());
                            },
                        }

                        if mfa_needed() {
                            Input {
                                name: "mfa_code",
                                label: "Authentication code",
                                r#type: "text".to_string(),
                                value: mfa_code(),
                                disabled: saving(),
                                oninput: move |e: FormEvent| {
                                    error.set(String::new());
                                    mfa_code.set(e.value());
                                },
                            }
                        }

                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                        }

                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: saving(),
                                loading: saving(),
                                r#type: "submit".to_string(),
                                class: "w-full".to_string(),
                                if saving() { "Signing in..." } else { "Sign in" }
                            }
                        }
                    }
                }
            }
        }
    }
}

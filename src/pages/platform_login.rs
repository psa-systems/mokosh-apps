//! MAPPS-513 stage A client side: platform super-admin login.
//!
//! Distinct URL (`/platform/login`), distinct credential store on the
//! server (`platform_admins`), distinct JWT typ (`"platform"`). Nothing
//! about this page touches the tenant identity plane. Store the
//! returned bearer under a separate sessionStorage key so it can't
//! collide with the regular AuthContext token.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};

#[cfg(target_arch = "wasm32")]
const PLATFORM_TOKEN_KEY: &str = "mokosh:platform_token";

#[derive(Serialize)]
pub(crate) struct PlatformLoginBody {
    pub(crate) email: String,
    pub(crate) password: String,
}

#[derive(Deserialize)]
pub(crate) struct PlatformLoginResp {
    pub(crate) access_token: String,
    #[allow(dead_code)]
    pub(crate) expires_at: chrono::DateTime<chrono::Utc>,
    #[allow(dead_code)]
    pub(crate) admin: PlatformAdminProfile,
}

#[derive(Deserialize)]
pub(crate) struct PlatformAdminProfile {
    pub(crate) email: String,
    pub(crate) first_name: String,
    pub(crate) last_name: String,
}

#[component]
pub fn PlatformLoginPage() -> Element {
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut done_greeting = use_signal(String::new);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your platform admin email and password.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        // MAPPS-518 walkthrough fix: clear the success greeting on every
        // submit too. Without this a failed re-attempt renders the stale
        // "Signed in as ..." banner from a previous success alongside
        // the new "Invalid email or password." error, and the operator
        // can't tell which state they are in.
        done_greeting.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = PlatformLoginBody {
                    email: em,
                    password: pw,
                };
                match crate::hooks::fetch::api::post_typed::<PlatformLoginResp, _>(
                    "/platform/login",
                    &body,
                )
                .await
                {
                    Ok(resp) => {
                        // Stash the platform bearer under its own key.
                        // The regular tenant AuthContext is untouched;
                        // reloading the page rehydrates the tenant
                        // session, and the platform token can be read
                        // independently by any platform-scoped page
                        // (e.g. a future /platform/tenants list).
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            if let Ok(Some(store)) = win.session_storage() {
                                let _ = store.set_item(PLATFORM_TOKEN_KEY, &resp.access_token);
                            }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = &resp.access_token;
                        done_greeting.set(format!(
                            "Signed in as {} {} ({}). Platform token stored in session; the tenant surface is unchanged.",
                            resp.admin.first_name, resp.admin.last_name, resp.admin.email
                        ));
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid email or password.".to_string());
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
                h1 { class: "text-2xl font-semibold text-content", "Platform sign-in" }
                p { class: "mt-2 text-sm text-content",
                    "This is the mokosh platform super-admin login (separate from tenant admin logins)."
                }
            }
            if !done_greeting().is_empty() {
                div { class: "rounded-md bg-surface-2 p-3 mb-4 text-sm text-content",
                    "{done_greeting}"
                }
            }
            form {
                class: "space-y-4",
                onsubmit: move |evt: Event<FormData>| {
                    evt.prevent_default();
                    submit(());
                },
                Input {
                    name: "email",
                    label: "Platform admin email",
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
                        "Sign in"
                    }
                }
            }
        }
    }
}

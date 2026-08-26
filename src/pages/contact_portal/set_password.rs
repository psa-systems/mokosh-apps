//! mokosh-contact-login prompt 005: contact-plane password setup.
//!
//! Landing page for the magic link emailed by prompt 003
//! (`msp.<apex>/portal/{slug}/set-password?token=...`). Public route,
//! token arrives as a component prop (never scraped from
//! `window.location.search`).
//!
//! Prompt 005 caveat: the spec asks for an auto-login after a
//! successful set-password, but `POST /api/v1/contact/auth/set-password`
//! answers 204 with no body, and the login endpoint takes
//! `{slug, email, password}` - the email is not on the wire from the
//! setup redemption path. So this page deliberately departs from the
//! spec: on 204 we show "Password set. Sign in below." and route the
//! visitor to `/portal/{slug}/login` instead. Adding an email round-
//! trip on top of the setup redemption is out-of-scope for this
//! prompt.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct SetPasswordBody {
    token: String,
    password: String,
}

#[derive(Deserialize, Clone, Debug)]
struct HostHint {
    #[serde(default)]
    company_name: String,
}

#[component]
pub fn ContactSetPasswordPage(slug: String, token: String) -> Element {
    let nav = use_navigator();
    let mut new_pw = use_signal(String::new);
    let mut confirm_pw = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);
    let mut saving = use_signal(|| false);

    let slug_for_host = slug.clone();
    let host_resource: Resource<Option<HostHint>> = use_resource(move || {
        let slug = slug_for_host.clone();
        async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contact/portal/{slug}/host");
                crate::hooks::fetch::api::get_typed::<HostHint>(&path)
                    .await
                    .ok()
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = slug;
                None
            }
        }
    });
    let host_snap = host_resource.read_unchecked();
    let company_name = match &*host_snap {
        Some(Some(h)) => {
            let n = h.company_name.trim();
            if n.is_empty() {
                None
            } else {
                Some(n.to_string())
            }
        }
        _ => None,
    };
    let heading = match company_name.as_deref() {
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
                let body = SetPasswordBody {
                    token: tok,
                    password: pw,
                };
                match crate::hooks::fetch::api::post_typed_no_content(
                    "/contact/auth/set-password",
                    &body,
                )
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
                    Err(ApiError::Status {
                        code: 410, message, ..
                    }) => {
                        error.set(if message.is_empty() {
                            "This setup link has already been used.".to_string()
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

    let slug_for_link = slug.clone();
    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Password set" }
                    p { class: "mt-2 text-sm text-content",
                        "Sign in below with your new password."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Primary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| {
                            nav.replace(Route::ContactHandleLogin { handle: slug_for_link.clone() });
                        },
                        "Go to sign in"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "{heading}" }
                    p { class: "mt-2 text-sm text-content",
                        "Choose a password you don't use anywhere else."
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

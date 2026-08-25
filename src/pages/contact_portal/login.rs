//! mokosh-contact-login prompt 005: contact-plane login page.
//!
//! Mounted at `/portal/{slug}/login`. Public (no `AuthGuard`). The
//! slug arrives as a component prop so no `window.location` scraping
//! is needed to route the visitor back on failure. On success the
//! contact-session tokens land in the in-memory + localStorage
//! holders (`hooks::fetch::api::set_contact_*_token`) and the
//! visitor is navigated to `/dashboard`, where the same mokosh
//! workspace routes render but subsequent fetches carry the contact
//! JWT (`typ: "contact"`, minted by the server in prompt 004).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct LoginBody {
    slug: String,
    email: String,
    password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_code: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
struct LoginResp {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    mfa_required: bool,
    // Snapshot of the authenticated contact, populated on the full
    // session return. Prompt 006 pulls `caps` off this into the
    // capability hook so the sidebar and row actions gate off the
    // fresh claim without another round-trip.
    #[serde(default)]
    contact: Option<ContactSnippet>,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct ContactSnippet {
    #[serde(default)]
    caps: Vec<String>,
}

/// Public host hint served at `GET /contact/portal/{slug}/host` per
/// prompt 004. Not authenticated; used to paint the branding block
/// and the "portal not available" splash when the owning tenant is
/// not active.
#[derive(Deserialize, Clone, Debug)]
struct HostHint {
    #[serde(default)]
    company_name: String,
    #[serde(default)]
    tenant_display_name: String,
    #[serde(default)]
    tenant_status: String,
}

#[component]
pub fn ContactLoginPage(slug: String) -> Element {
    let nav = use_navigator();
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);
    let mut error = use_signal(String::new);
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
    let host: Option<HostHint> = match &*host_snap {
        Some(Some(h)) => Some(h.clone()),
        _ => None,
    };
    let company_name = host
        .as_ref()
        .map(|h| h.company_name.trim().to_string())
        .filter(|s| !s.is_empty());
    let tenant_display_name = host
        .as_ref()
        .map(|h| h.tenant_display_name.trim().to_string())
        .filter(|s| !s.is_empty());
    let tenant_status = host
        .as_ref()
        .map(|h| h.tenant_status.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let host_loaded = host.is_some();
    // MAPPS-559 shape: hide the form entirely when the owning tenant
    // is suspended / terminated. A missing host hint (network / 404)
    // falls through to the form so a bad slug still lets the visitor
    // see a coherent page (submit will 401 or fail closed).
    let tenant_inactive = host_loaded && tenant_status != "active";

    let heading = match (company_name.as_deref(), tenant_display_name.as_deref()) {
        (Some(cn), Some(tn)) => format!("Sign in to {cn} at {tn}"),
        (Some(cn), None) => format!("Sign in to {cn}"),
        (None, Some(tn)) => format!("Sign in to {tn}"),
        _ => "Client Portal".to_string(),
    };

    let slug_for_submit = slug.clone();
    let mut submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if em.is_empty() || pw.is_empty() {
            error.set("Enter your email and password.".to_string());
            return;
        }
        let mfa_raw = mfa_code.read().trim().to_string();
        let mfa = if mfa_raw.is_empty() {
            None
        } else {
            Some(mfa_raw)
        };
        let slug = slug_for_submit.clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = LoginBody {
                    slug: slug.clone(),
                    email: em,
                    password: pw,
                    mfa_code: mfa,
                };
                match crate::hooks::fetch::api::post_typed::<LoginResp, _>(
                    "/contact/auth/login",
                    &body,
                )
                .await
                {
                    Ok(resp) if resp.mfa_required => {
                        mfa_needed.set(true);
                        error
                            .set("Enter the 6-digit code from your authenticator app.".to_string());
                    }
                    Ok(resp) => {
                        let caps = resp
                            .contact
                            .as_ref()
                            .map(|c| c.caps.clone())
                            .unwrap_or_default();
                        crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
                        crate::hooks::fetch::api::set_contact_refresh_token(Some(
                            resp.refresh_token,
                        ));
                        crate::hooks::fetch::api::set_contact_last_slug(&slug);
                        crate::hooks::capabilities::set_contact_capabilities(Some(caps));
                        nav.replace(Route::Dashboard {});
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Invalid credentials.".to_string());
                    }
                    Err(ApiError::Status { code: 429, .. }) => {
                        error.set("Too many attempts; try again shortly.".to_string());
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = slug;
            }
            saving.set(false);
        });
    };

    let slug_for_forgot = slug.clone();
    rsx! {
        AuthLayout {
            if tenant_inactive {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "This portal is not available" }
                    p { class: "mt-2 text-sm text-content",
                        "Contact your account owner for help."
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "{heading}" }
                    p { class: "mt-2 text-sm text-content",
                        "Enter your email and password."
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
                        p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                    }
                    div { class: "pt-2 space-y-2",
                        Button {
                            variant: ButtonVariant::Primary,
                            disabled: saving(),
                            loading: saving(),
                            r#type: "submit".to_string(),
                            class: "w-full".to_string(),
                            "Sign in"
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: saving(),
                            r#type: "button".to_string(),
                            class: "w-full".to_string(),
                            onclick: move |_| {
                                nav.push(Route::ContactForgotPassword { slug: slug_for_forgot.clone() });
                            },
                            "Forgot password?"
                        }
                    }
                }
            }
        }
    }
}

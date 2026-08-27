//! MAPPS-589 (mokosh-contact-login prompt 011): Portal-ID-scoped
//! contact login page.
//!
//! Mounted at `/portal/{portal_id}/login` via the `ContactHandleLogin`
//! wrapper (see `src/lib.rs`), which forwards here whenever the URL
//! handle matches the 9-digit numeric Portal ID shape. Public (no
//! `AuthGuard`). Mirrors prompt 005's `ContactLoginPage` in every way
//! except that the Portal ID is a READ-ONLY field ABOVE email and the
//! login POST body carries `portal_id: i64` (no slug) so the server
//! can resolve the Company without the legacy slug lookup.
//!
//! On success the contact-session tokens land in the in-memory +
//! localStorage holders (`hooks::fetch::api::set_contact_*_token`) and
//! the visitor is navigated to `/dashboard`. During the release cycle
//! that keeps `portal_slug` around we ALSO write the deprecated
//! `mokosh:contact_last_slug` key when the server response carries a
//! slug, so a cold-load bootstrap on old code still finds a value.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// Login request body. Portal ID is parsed to `i64` at submit time so
/// the server's `ContactLoginRequest` receives the numeric shape the
/// PMS-928 DTO defines (see prompt 011 §Login endpoint change).
#[derive(Serialize)]
struct LoginBody {
    portal_id: i64,
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
    #[serde(default)]
    contact: Option<ContactSnippet>,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct ContactSnippet {
    #[serde(default)]
    caps: Vec<String>,
    /// Portal slug of the Company this session is for (kept during the
    /// prompt 011 transition so callers still bootstrap the legacy key
    /// on cold-load). Server drops this field alongside the
    /// `portal_slug` column removal in a follow-up ticket.
    #[serde(default)]
    portal_slug: String,
    /// Portal ID of the Company this session is for. Present once
    /// PMS-928 ships; the client stores it in localStorage so a
    /// subsequent cold-load reaches `/portal/{portal_id}/login`
    /// directly instead of the slug-based bounce.
    #[serde(default)]
    portal_id: Option<i64>,
    /// MAPPS-604 (prompt 013): Company UUID this session is scoped to.
    /// Optional so a pre-PMS-935 server that omits the field still
    /// deserialises; the store is left at `None` and pages fall back
    /// to their previous URL-derived path.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    /// MAPPS-609: the UUID of the Contact behind this session. Optional
    /// so a pre-PMS-937 server that omits the field still deserialises;
    /// the store is left at `None` and ticket-detail ownership gates
    /// fall closed.
    #[serde(default)]
    contact_id: Option<uuid::Uuid>,
}

/// Public host hint served at
/// `GET /contact/portal/{portal_id}/host` (PMS-928 parallel to the
/// existing slug endpoint). Not authenticated; used to paint the
/// branding block and the MAPPS-559 "portal not available" splash
/// when the owning tenant is not active.
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
pub fn ContactLoginByPortalIdPage(portal_id: String) -> Element {
    let nav = use_navigator();
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let portal_id_for_host = portal_id.clone();
    let host_resource: Resource<Option<HostHint>> = use_resource(move || {
        let pid = portal_id_for_host.clone();
        async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contact/portal/{pid}/host");
                crate::hooks::fetch::api::get_typed::<HostHint>(&path)
                    .await
                    .ok()
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = pid;
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
    // MAPPS-559 shape (mirrored from prompt 005): hide the form when
    // the owning tenant is not active. A missing host hint falls
    // through to the form so a bad Portal ID still lets the visitor
    // see a coherent page (submit will 401 or fail closed).
    let tenant_inactive = host_loaded && tenant_status != "active";

    let heading = match (company_name.as_deref(), tenant_display_name.as_deref()) {
        (Some(cn), Some(tn)) => format!("Sign in to {cn} at {tn}"),
        (Some(cn), None) => format!("Sign in to {cn}"),
        (None, Some(tn)) => format!("Sign in to {tn}"),
        _ => "Client Portal".to_string(),
    };

    let portal_id_for_submit = portal_id.clone();
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
        let pid_str = portal_id_for_submit.clone();
        let Ok(pid_i64) = pid_str.parse::<i64>() else {
            // The wrapper only forwards 9-digit numeric handles, so a
            // parse failure here means the wrapper's shape check has
            // drifted from what the server accepts. Surface as an
            // opaque error rather than firing a garbage POST.
            error.set("Invalid Portal ID.".to_string());
            return;
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = LoginBody {
                    portal_id: pid_i64,
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
                        install_session(&nav, resp, &pid_str);
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
                let _ = (pid_i64, pid_str);
            }
            saving.set(false);
        });
    };

    let portal_id_readonly = portal_id.clone();
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
                    // Portal ID displayed as a read-only field ABOVE
                    // email so the contact learns / recognises the
                    // number they can dictate over the phone (prompt
                    // 011 primary UX goal).
                    Input {
                        name: "portal_id",
                        label: "Portal ID",
                        r#type: "text".to_string(),
                        value: portal_id_readonly.clone(),
                        disabled: true,
                    }
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
                    }
                    // Magic-link escape hatch. Points at the generic
                    // three-field page (`/portal/login`), which
                    // itself carries a "Or sign in without a password"
                    // link to the actual magic-link finder. Two hops
                    // out to the finder rather than one keeps the
                    // portal-id-scoped page focused on password login
                    // and matches the prompt 011 spec wording.
                    div { class: "pt-4 text-center",
                        Link {
                            to: Route::ContactGenericLogin {},
                            class: "text-sm text-accent hover:underline",
                            "Or sign in without a password"
                        }
                    }
                }
            }
        }
    }
}

/// Install the tokens + capabilities from a successful login response
/// and hop to `/dashboard`. Kept behind `#[cfg(feature = "web")]` so
/// the non-web build compiles without touching localStorage.
#[cfg(feature = "web")]
fn install_session(nav: &dioxus::router::Navigator, resp: LoginResp, portal_id_str: &str) {
    let caps = resp
        .contact
        .as_ref()
        .map(|c| c.caps.clone())
        .unwrap_or_default();
    let slug = resp
        .contact
        .as_ref()
        .map(|c| c.portal_slug.clone())
        .unwrap_or_default();
    let response_portal_id = resp.contact.as_ref().and_then(|c| c.portal_id);
    let response_company_id = resp.contact.as_ref().and_then(|c| c.company_id);
    let response_contact_id = resp.contact.as_ref().and_then(|c| c.contact_id);
    crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
    crate::hooks::fetch::api::set_contact_refresh_token(Some(resp.refresh_token));
    // MAPPS-604: stash the session's Company UUID for scoped-URL builders.
    crate::hooks::fetch::api::set_contact_company_id(response_company_id);
    // MAPPS-609: stash the session's Contact UUID so ownership gates
    // (e.g. the ticket-detail Edit button) know who this contact is.
    crate::hooks::fetch::api::set_contact_id(response_contact_id);
    // Prefer the server-supplied portal_id (source of truth once PMS-928
    // ships); fall back to the URL handle so a pre-PMS-928 response still
    // writes the key.
    let portal_id_to_store = response_portal_id
        .map(|n| n.to_string())
        .unwrap_or_else(|| portal_id_str.to_string());
    crate::hooks::fetch::api::set_contact_last_portal_id(&portal_id_to_store);
    // Transition-window write: keep the legacy last-slug key populated
    // when the server carries a slug so a hard refresh on old client
    // code still finds a value.
    if !slug.is_empty() {
        crate::hooks::fetch::api::set_contact_last_slug(&slug);
    }
    crate::hooks::capabilities::set_contact_capabilities(Some(caps));
    nav.replace(Route::Dashboard {});
}

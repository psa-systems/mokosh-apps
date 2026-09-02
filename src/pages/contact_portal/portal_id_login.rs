//! MAPPS-589 (mokosh-contact-login prompt 011): Portal-ID-scoped
//! contact login page.
//!
//! Mounted at `/portal/{portal_id}/login` via the `ContactHandleLogin`
//! wrapper (see `src/lib.rs`), which forwards here whenever the URL
//! handle matches the 9-digit numeric Company ID shape. Public (no
//! `AuthGuard`). Mirrors prompt 005's `ContactLoginPage` in every way
//! except that the Company ID is a READ-ONLY field ABOVE email and the
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

/// Login request body. Company ID is parsed to `i64` at submit time so
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
    /// Company ID of the Company this session is for. Present once
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
    // MAPPS-616 dropped the "at {tenant_display_name}" heading suffix
    // but the server still ships the field. Keep it accepted (and
    // ignored) on the wire so a serde deserialise doesn't reject a
    // future payload that starts sending it again.
    #[serde(default, rename = "tenant_display_name")]
    #[allow(dead_code)]
    _tenant_display_name: String,
    #[serde(default)]
    tenant_status: String,
    /// MAPPS-621 (mokosh-branding prompt 005): merged brand painted
    /// on the login page before the visitor signs in. Optional so a
    /// legacy `/host` response (pre-MAPPS-617) deserializes cleanly.
    #[serde(default)]
    effective_branding: crate::hooks::branding::EffectiveBranding,
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
    // MAPPS-621: publish the merged brand into the global signal so
    // `AuthLayout` paints the logo + wordmark + support block BEFORE
    // the visitor signs in. Runs on every render, but the signal
    // dedupes writes so this stays cheap.
    #[cfg(feature = "web")]
    if let Some(Some(h)) = &*host_snap {
        crate::hooks::branding::set_effective_branding(h.effective_branding.clone());
    }
    let host: Option<HostHint> = match &*host_snap {
        Some(Some(h)) => Some(h.clone()),
        _ => None,
    };
    let company_name = host
        .as_ref()
        .map(|h| h.company_name.trim().to_string())
        .filter(|s| !s.is_empty());
    let tenant_status = host
        .as_ref()
        .map(|h| h.tenant_status.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let host_loaded = host.is_some();
    // MAPPS-559 shape (mirrored from prompt 005): hide the form when
    // the owning tenant is not active. A missing host hint falls
    // through to the form so a bad Company ID still lets the visitor
    // see a coherent page (submit will 401 or fail closed).
    let tenant_inactive = host_loaded && tenant_status != "active";

    // MAPPS-616 (prompt 014 followup): heading drops the "at
    // {tenant_display_name}" suffix from prompt 011. Every mokosh
    // instance ships with a seeded tenant literally named "Default";
    // contacts of that instance saw "Sign in to Test at Default"
    // which reads as broken.
    //
    // MAPPS-635 D2: prefer the brand's `display_name` over the raw
    // CRM `company_name`. The wordmark above the card already reads
    // the brand display name; painting a different label into the
    // heading (the CRM record's internal name, often the shorthand
    // "Test" or an unbranded value) made the two disagree on the
    // same page. Falls through to the CRM name, then to a neutral
    // "your portal" string, so a load-in-progress or brand-not-set
    // instance still renders a coherent card.
    let brand = crate::hooks::branding::EFFECTIVE_BRANDING.read();
    let heading_label = brand
        .display_name
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| company_name.clone())
        .filter(|s| !s.is_empty());
    let heading = match heading_label {
        Some(cn) => format!("Sign in to {cn}"),
        _ => "Sign in to your portal".to_string(),
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
            error.set("Invalid Company ID.".to_string());
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
    // MAPPS-615 (prompt 014): "Not your portal? Choose a different one"
    // button. Rendered above the branding header so a visitor
    // recognises they landed on the wrong portal BEFORE they type
    // credentials. Click hops back to the step-1 Company ID entry page
    // and clears the last-portal-id hint so the AuthGuard cold-load
    // bootstrap does not immediately bounce back here.
    let switch_portal = move |_| {
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::clear_contact_last_portal_id();
        }
        nav.replace(Route::ContactGenericLogin {});
    };
    rsx! {
        AuthLayout {
            div { class: "mb-4 text-center",
                button {
                    r#type: "button",
                    class: "text-sm text-accent hover:underline",
                    onclick: switch_portal,
                    "Not your portal? Choose a different one"
                }
            }
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
                    // Company ID displayed as a read-only field ABOVE
                    // email so the contact learns / recognises the
                    // number they can dictate over the phone (prompt
                    // 011 primary UX goal).
                    Input {
                        name: "portal_id",
                        label: "Company ID",
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
                    // MAPPS-615 (prompt 014): step 1 no longer carries
                    // the magic-link fallback (it's Portal-ID-only now),
                    // so this link now hops DIRECTLY to the finder
                    // instead of routing through step 1. Carries an
                    // empty email so the finder shows an empty input.
                    div { class: "pt-4 text-center",
                        Link {
                            to: Route::ContactMagicLinkLogin { email: String::new() },
                            class: "text-sm text-accent hover:underline",
                            "Or sign in without a password"
                        }
                    }
                }
            }
            // MAPPS-615: cross-plane switch. Consistent with the same
            // link on the step-1 page + the staff /login page, so a
            // visitor on the wrong plane can jump without browser-back.
            div { class: "pt-6 mt-6 border-t border-line text-center",
                Link {
                    to: Route::Login {},
                    class: "text-sm text-accent hover:underline",
                    "MSP staff sign in instead"
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
    // MAPPS-630: cross-plane isolation. Clear any staff bearer +
    // storage before writing the contact session.
    crate::hooks::fetch::api::on_contact_signin_clear_staff_side();
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

//! mokosh-contact-login prompt 005: contact-plane login page.
//!
//! Mounted at `/portal/{slug}/login` via the `ContactHandleLogin`
//! wrapper (`src/lib.rs`), which forwards here whenever the URL
//! handle does NOT match the 9-digit Company ID shape (i.e. a legacy
//! Crockford slug bookmark). Public (no `AuthGuard`). The slug
//! arrives as a component prop so no `window.location` scraping is
//! needed to route the visitor back on failure. On success the
//! contact-session tokens land in the in-memory + localStorage
//! holders (`hooks::fetch::api::set_contact_*_token`) and the
//! visitor is navigated to `/dashboard`, where the same mokosh
//! workspace routes render but subsequent fetches carry the contact
//! JWT (`typ: "contact"`, minted by the server in prompt 004).
//!
//! MAPPS-589 (prompt 011): before painting the slug-based form, this
//! page fires `GET /contact/portal/{slug}/resolve-to-portal-id` on
//! mount. On 200 (`{ portal_id: N }`), `nav.replace` the visitor
//! into the Portal-ID-scoped page so live invitation emails
//! transparently migrate. On 404 / any error, the slug form still
//! renders as a fallback so a mid-transition visitor is never
//! stranded (the server's dual-accept path still honours slug
//! logins during the compat window).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// Server response shape for
/// `GET /api/v1/contact/portal/{slug}/resolve-to-portal-id`
/// (PMS-928). 200 with a numeric `portal_id` means the slug maps
/// to a Company that has been assigned a Company ID; 404 means the
/// slug is unknown or has not been backfilled yet.
#[derive(Deserialize, Clone, Debug)]
struct ResolveToPortalIdResp {
    #[serde(default)]
    portal_id: Option<i64>,
}

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

/// Public host hint served at `GET /contact/portal/{slug}/host` per
/// prompt 004. Not authenticated; used to paint the branding block
/// and the "portal not available" splash when the owning tenant is
/// not active.
#[derive(Deserialize, Clone, Debug)]
struct HostHint {
    #[serde(default)]
    company_name: String,
    // MAPPS-616 dropped the "at {tenant_display_name}" heading suffix
    // here for parity with `portal_id_login.rs`. Field is kept on the
    // wire (accepted + ignored) so a future payload that ships it
    // does not fail deserialise.
    #[serde(default, rename = "tenant_display_name")]
    #[allow(dead_code)]
    _tenant_display_name: String,
    #[serde(default)]
    tenant_status: String,
    /// MAPPS-621 (mokosh-branding prompt 005): merged brand painted
    /// on the legacy slug login page too.
    #[serde(default)]
    effective_branding: crate::hooks::branding::EffectiveBranding,
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

    // MAPPS-589 (prompt 011): resolve-and-redirect. Fire once on
    // mount; if the server maps this slug to a Company ID, hop the
    // visitor into the Portal-ID-scoped page. Any failure (404,
    // network, or 5xx) falls through to the slug form below - the
    // server's dual-accept login still honours the slug during the
    // compat window, so a mid-transition visitor is never stranded.
    let slug_for_resolve = slug.clone();
    use_effect(move || {
        let slug = slug_for_resolve.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/contact/portal/{slug}/resolve-to-portal-id");
                if let Ok(resp) =
                    crate::hooks::fetch::api::get_typed::<ResolveToPortalIdResp>(&path).await
                {
                    if let Some(pid) = resp.portal_id {
                        nav.replace(Route::ContactHandleLogin {
                            handle: pid.to_string(),
                        });
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = slug;
            }
        });
    });

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
    // MAPPS-621: paint the merged brand into the global signal on
    // the legacy slug login page too, so a stale-URL visitor still
    // sees the branded shell.
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
    // MAPPS-559 shape: hide the form entirely when the owning tenant
    // is suspended / terminated. A missing host hint (network / 404)
    // falls through to the form so a bad slug still lets the visitor
    // see a coherent page (submit will 401 or fail closed).
    let tenant_inactive = host_loaded && tenant_status != "active";

    // MAPPS-616: drop the "at {tenant_display_name}" suffix here too
    // for parity with the Portal-ID-scoped page.
    // MAPPS-635 D2: prefer brand display_name over CRM company_name
    // so the heading matches the wordmark above the card. See the
    // same-numbered note in `portal_id_login.rs` for the rationale.
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
                        let company_id = resp.contact.as_ref().and_then(|c| c.company_id);
                        let contact_id = resp.contact.as_ref().and_then(|c| c.contact_id);
                        // MAPPS-630: the two planes are mutually exclusive
                        // within one browser origin. Clear any staff bearer
                        // + its sessionStorage bundle BEFORE we write the
                        // fresh contact session so the tab reads as a
                        // contact from this point on.
                        crate::hooks::fetch::api::on_contact_signin_clear_staff_side();
                        crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
                        crate::hooks::fetch::api::set_contact_refresh_token(Some(
                            resp.refresh_token,
                        ));
                        crate::hooks::fetch::api::set_contact_last_slug(&slug);
                        // MAPPS-604: hydrate the session's Company scope.
                        crate::hooks::fetch::api::set_contact_company_id(company_id);
                        // MAPPS-609: hydrate the session's Contact UUID
                        // so the ticket-detail Edit button can gate on
                        // ownership.
                        crate::hooks::fetch::api::set_contact_id(contact_id);
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
    // MAPPS-635 D5: `/portal/<bad-slug>/login` resolved as an
    // unbranded generic sign-in form + a submit that would always
    // 401. Detect the "host fetch resolved, no such portal" case
    // (Some(None) — distinct from None which is still-loading) and
    // render a proper "Portal not found" card that steers the visitor
    // to `/portal/login` (step 1 by Company ID). tenant_inactive still
    // wins if the host DID resolve but the tenant is suspended.
    let host_not_found = matches!(&*host_snap, Some(None));
    rsx! {
        AuthLayout {
            if host_not_found {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Portal not found" }
                    p { class: "mt-2 text-sm text-content",
                        "This portal link isn't valid. Check the URL your MSP sent you, or use your Company ID instead."
                    }
                    div { class: "mt-4",
                        Link {
                            to: Route::ContactGenericLogin {},
                            class: "text-accent hover:underline text-sm",
                            "Sign in with your Company ID"
                        }
                    }
                }
            } else if tenant_inactive {
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
                    // MAPPS-572 (prompt 010): magic-link escape hatch.
                    // Hands the typed email over via the `?email=`
                    // query so the finder can pre-fill without a
                    // re-type. Rendered as a plain link (not a full
                    // Button) so it does not compete visually with
                    // the primary sign-in / forgot-password affordances.
                    div { class: "pt-4 text-center",
                        {
                            let hop_email = email.read().trim().to_string();
                            rsx! {
                                Link {
                                    to: Route::ContactMagicLinkLogin { email: hop_email },
                                    class: "text-sm text-accent hover:underline",
                                    "Or sign in without a password"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

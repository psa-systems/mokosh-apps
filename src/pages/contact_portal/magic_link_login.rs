//! MAPPS-572 (mokosh-contact-login prompt 010): magic-link finder.
//!
//! Mounted at `/portal/login` (slug-less). Public (no `AuthGuard`).
//! Renders a single email input + "Send me a sign-in link" button that
//! POSTs `/contact/auth/login-link { email, slug }`. The endpoint is
//! fail-quiet per PMS-918 (always 204, no enumeration oracle) so the
//! SPA always renders "Check your inbox" on submit regardless of
//! outcome. If localStorage remembers the last slug this browser
//! signed in on, we pass it in the body so the server can disambiguate
//! the tenant when the visitor is on a shared marketing host; on the
//! tenant subdomain the server ignores the slug and uses Host.
//!
//! MSP display-name plumbing: the existing per-slug login page pulls
//! `/contact/portal/{slug}/host` to paint the tenant / Company name in
//! the heading. This slug-less variant has no slug and no equivalent
//! public branding endpoint has landed server-side yet (PMS-918 does
//! not add one). Rendering a specific Company / MSP display name here
//! would also be a mild pre-auth enumeration signal (which tenant
//! "owns" this host) - the spec's "no pre-auth affordance that reveals
//! any specific Company or portal" rule points the other way, so this
//! page stays deliberately neutral: "Sign in to your portal".

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

#[derive(Serialize)]
struct LoginLinkBody {
    email: String,
    /// MAPPS-637: Company ID is now required by the server. Sent as
    /// `Option<i64>` on the wire so an old client without the field
    /// still deserialises, but the SPA form validates presence
    /// client-side before submit.
    #[serde(skip_serializing_if = "Option::is_none")]
    portal_id: Option<i64>,
    /// Legacy slug hint - kept as a fallback in case a browser has
    /// no Company ID but still holds a slug in localStorage. Server
    /// treats slug as equivalent scope to portal_id (see
    /// `request_login_link` in mokosh-server).
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
}

#[component]
pub fn ContactMagicLinkLoginPage(email: String) -> Element {
    let nav = use_navigator();
    // Pre-fill from the `?email=` query param so the "Request a new
    // sign-in link" button on the picker's invalid-link branch can hand
    // the visitor's typed email back without a re-type.
    let mut email_sig = use_signal(|| email.clone());
    // MAPPS-637: Company ID is now required so the magic link is
    // scoped to a specific portal end-to-end. The bare-email
    // finder used to fan out one email per matched tenant + let
    // the redeem step aggregate contacts across Companies; the
    // Company ID upfront collapses that to a single scoped intent.
    let mut portal_id_sig = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut done = use_signal(|| false);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let em = email_sig.read().trim().to_string();
        let pid_raw = portal_id_sig.read().trim().to_string();
        if em.is_empty() {
            error.set("Enter your email.".to_string());
            return;
        }
        if pid_raw.is_empty() {
            error.set("Enter your Company ID.".to_string());
            return;
        }
        let pid = match pid_raw.parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => {
                error.set("Company ID must be a 9-digit number.".to_string());
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let slug = crate::hooks::fetch::api::current_contact_last_slug();
                let body = LoginLinkBody {
                    email: em,
                    portal_id: pid,
                    slug,
                };
                // Fail-quiet: even a 4xx / 5xx is treated as sent-anyway
                // so the response shape does not leak account existence.
                // Mirrors the forgot-password posture from prompt 005.
                let _ = crate::hooks::fetch::api::post_typed_no_content(
                    "/contact/auth/login-link",
                    &body,
                )
                .await;
                done.set(true);
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (em, pid);
                done.set(true);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            if done() {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Check your inbox" }
                    p { class: "mt-2 text-sm text-content",
                        "If that email is on file, we've sent a sign-in link."
                    }
                }
                div { class: "pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        class: "w-full".to_string(),
                        onclick: move |_| {
                            nav.replace(Route::Home {});
                        },
                        "Back to home"
                    }
                }
            } else {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content", "Sign in to your portal" }
                    p { class: "mt-2 text-sm text-content",
                        "Enter your Company ID and email; we'll send a one-click sign-in link for that specific portal."
                    }
                }
                form {
                    class: "space-y-4",
                    onsubmit: move |evt: Event<FormData>| {
                        evt.prevent_default();
                        submit(());
                    },
                    // MAPPS-637: Company ID is required so the magic
                    // link is scoped to one portal end-to-end.
                    Input {
                        name: "portal_id",
                        label: "Company ID",
                        r#type: "text".to_string(),
                        value: portal_id_sig(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            portal_id_sig.set(e.value());
                        },
                    }
                    Input {
                        name: "email",
                        label: "Email",
                        r#type: "email".to_string(),
                        value: email_sig(),
                        required: true,
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            email_sig.set(e.value());
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
                            "Send me a sign-in link"
                        }
                    }
                }
            }
        }
    }
}

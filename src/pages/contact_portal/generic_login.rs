//! MAPPS-589 (mokosh-contact-login prompt 011): generic three-field
//! contact login page.
//!
//! Mounted at `/portal/login` (no Portal ID in URL). Public (no
//! `AuthGuard`). Rendered when the visitor lands from the homepage
//! "Client portal" CTA - i.e. they do not (yet) have a specific
//! bookmarked Portal ID URL. Three visible fields: Portal ID (9-digit
//! numeric), email, password.
//!
//! No branding fetch happens here: without a Portal ID or slug the
//! server has no Company to anchor the display name to, and painting a
//! specific MSP name would leak the "who owns this host" pre-auth
//! signal the prompt 010 spec explicitly rules out. Heading stays
//! neutral ("Sign in to your portal").
//!
//! Submit shape matches the portal-id-scoped sibling page: POST
//! `/contact/auth/login { portal_id, email, password }` (no slug).
//! Session install path is identical; on success, `Route::Dashboard`.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::Route;

/// Length of the 9-digit numeric Portal ID (prompt 011 design
/// decision: `100_000_000..=999_999_999`). Kept as a const so a
/// future change to the digit count only touches this one line + the
/// tests below.
const PORTAL_ID_DIGITS: usize = 9;

/// Pure shape check: does `handle` look like a Portal ID (exactly
/// `PORTAL_ID_DIGITS` ASCII digits)? Extracted as a plain fn so unit
/// tests can pin the decision without touching web_sys / the router.
/// Also called by the `ContactHandleLogin` wrapper in `src/lib.rs`.
pub fn handle_is_portal_id_shape(handle: &str) -> bool {
    handle.len() == PORTAL_ID_DIGITS && handle.chars().all(|c| c.is_ascii_digit())
}

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
    #[serde(default)]
    portal_slug: String,
    #[serde(default)]
    portal_id: Option<i64>,
    /// MAPPS-604 (prompt 013): Company UUID this session is scoped to.
    /// Optional so a pre-PMS-935 server that omits the field still
    /// deserialises; the store is left at `None` and pages fall back
    /// to their previous URL-derived path.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
}

#[component]
pub fn ContactGenericLoginPage() -> Element {
    let nav = use_navigator();
    let mut portal_id = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut mfa_needed = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let pid_raw = portal_id.read().trim().to_string();
        let em = email.read().trim().to_string();
        let pw = password.read().clone();
        if pid_raw.is_empty() || em.is_empty() || pw.is_empty() {
            error.set("Enter your Portal ID, email, and password.".to_string());
            return;
        }
        if !handle_is_portal_id_shape(&pid_raw) {
            error.set(format!(
                "Portal ID must be exactly {PORTAL_ID_DIGITS} digits."
            ));
            return;
        }
        let Ok(pid_i64) = pid_raw.parse::<i64>() else {
            error.set(format!(
                "Portal ID must be exactly {PORTAL_ID_DIGITS} digits."
            ));
            return;
        };
        let mfa_raw = mfa_code.read().trim().to_string();
        let mfa = if mfa_raw.is_empty() {
            None
        } else {
            Some(mfa_raw)
        };
        saving.set(true);
        error.set(String::new());
        let pid_for_store = pid_raw.clone();
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
                        install_session(&nav, resp, &pid_for_store);
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
                let _ = (pid_i64, pid_for_store);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "text-center mb-6",
                h1 { class: "text-2xl font-semibold text-content", "Sign in to your portal" }
                p { class: "mt-2 text-sm text-content",
                    "Enter your Portal ID, email, and password."
                }
            }
            form {
                class: "space-y-4",
                onsubmit: move |evt: Event<FormData>| {
                    evt.prevent_default();
                    submit(());
                },
                // Portal ID is a 9-digit numeric. Rendered as a plain
                // `<input>` (not the shared `Input` component) so the
                // page can attach `inputmode="numeric"` +
                // `pattern="[0-9]{9}"` for the mobile numeric keypad
                // and browser-side rejection of non-digit shapes.
                // Matches the styling of the shared component.
                div { class: "space-y-1",
                    label {
                        r#for: "portal_id",
                        class: "block text-sm font-medium text-content",
                        "Portal ID"
                        span { class: "text-red-500 ml-1", aria_label: "required", role: "img", "*" }
                    }
                    input {
                        id: "portal_id_input",
                        // Renamed off `portal_id` because Chrome's
                        // autofill matcher latches on to any input
                        // whose id/name reads like a user identifier
                        // and prefills a saved email. Native
                        // `autocomplete="off"` alone doesn't suppress
                        // that on Chrome; combining an unfamiliar
                        // `name` with `autocomplete="off"` is what
                        // actually stops the browser from injecting an
                        // email into the Portal ID slot.
                        name: "portal_id_input",
                        r#type: "text",
                        autocomplete: "off",
                        inputmode: "numeric",
                        // Deliberately no HTML5 `pattern` attribute:
                        // native pattern validation fires BEFORE the
                        // submit handler and shows a browser-native
                        // "Please match the requested format" tooltip
                        // that overrides the friendly inline error
                        // rendered below. The Rust-side check
                        // (handle_is_portal_id_shape + parse::<i64>)
                        // covers the same validation with a message
                        // the user can actually act on.
                        maxlength: PORTAL_ID_DIGITS as i64,
                        class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                        placeholder: "555556666",
                        value: "{portal_id}",
                        aria_required: "true",
                        disabled: saving(),
                        oninput: move |e: FormEvent| {
                            error.set(String::new());
                            portal_id.set(e.value());
                        },
                    }
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
                // Magic-link escape hatch. Hops to the prompt 010
                // finder path so the two flows stay orthogonal (per
                // prompt 011 spec §generic_login guidance). Carries
                // the typed email over via the `?email=` query segment
                // so the finder can pre-fill without a re-type.
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

/// Install the session tokens + capabilities from a successful login
/// response and hop to `/dashboard`. Web-only so the non-web build
/// stays clean of `web_sys` calls.
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
    crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
    crate::hooks::fetch::api::set_contact_refresh_token(Some(resp.refresh_token));
    // MAPPS-604: stash the session's Company UUID for scoped-URL builders.
    crate::hooks::fetch::api::set_contact_company_id(response_company_id);
    let portal_id_to_store = response_portal_id
        .map(|n| n.to_string())
        .unwrap_or_else(|| portal_id_str.to_string());
    crate::hooks::fetch::api::set_contact_last_portal_id(&portal_id_to_store);
    if !slug.is_empty() {
        crate::hooks::fetch::api::set_contact_last_slug(&slug);
    }
    crate::hooks::capabilities::set_contact_capabilities(Some(caps));
    nav.replace(Route::Dashboard {});
}

#[cfg(test)]
mod tests {
    use super::handle_is_portal_id_shape;

    #[test]
    fn nine_digits_matches() {
        assert!(handle_is_portal_id_shape("555556666"));
    }

    #[test]
    fn eight_digits_rejected() {
        assert!(!handle_is_portal_id_shape("55555666"));
    }

    #[test]
    fn ten_digits_rejected() {
        assert!(!handle_is_portal_id_shape("5555566666"));
    }

    #[test]
    fn crockford_slug_rejected() {
        assert!(!handle_is_portal_id_shape("K3F9M7N2Q8XR5J4W"));
    }

    #[test]
    fn empty_rejected() {
        assert!(!handle_is_portal_id_shape(""));
    }

    #[test]
    fn mixed_digits_and_letters_rejected() {
        assert!(!handle_is_portal_id_shape("55555666a"));
    }
}

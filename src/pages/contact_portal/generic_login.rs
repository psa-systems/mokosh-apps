//! MAPPS-615 (mokosh-contact-login prompt 014): step 1 of the two-step
//! contact login flow.
//!
//! Mounted at `/portal/login` (no Portal ID in URL). Public (no
//! `AuthGuard`). Rendered when the visitor lands from the homepage
//! "Client portal" CTA or navigates here from the staff `/login` page's
//! "Sign in to a client portal instead" link.
//!
//! One field: Portal ID (9-digit numeric). On Continue, the SPA fetches
//! the branding hint via `GET /api/v1/contact/portal/{portal_id}/host`
//! (existing endpoint from prompt 004; public, no auth). On 200 the
//! visitor navigates to `/portal/{portal_id}/login` (the URL-scoped
//! branded step 2 from prompt 011) which fetches the same host hint on
//! mount, renders Company + MSP branding + the email + password fields,
//! and offers a "Choose a different portal" button that returns here.
//!
//! On 404 the visitor sees a friendly "Portal ID not found" inline,
//! matching the enum-resistance shape the /host endpoint already had.
//!
//! Prior to prompt 014 this page fit all three fields (Portal ID +
//! email + password) on one screen with no branding, which read as
//! sterile and had no way to switch portals mid-flow.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AuthLayout, Button, ButtonVariant};
use crate::Route;

/// Length of the 9-digit numeric Portal ID (prompt 011 design
/// decision: `100_000_000..=999_999_999`).
const PORTAL_ID_DIGITS: usize = 9;

/// Pure shape check: does `handle` look like a Portal ID (exactly
/// `PORTAL_ID_DIGITS` ASCII digits)? Called by the `ContactHandleLogin`
/// wrapper in `src/lib.rs` to steer the URL-scoped page between the
/// portal-id branch and the legacy slug branch.
pub fn handle_is_portal_id_shape(handle: &str) -> bool {
    handle.len() == PORTAL_ID_DIGITS && handle.chars().all(|c| c.is_ascii_digit())
}

/// Response shape from `GET /api/v1/contact/portal/{portal_id}/host`
/// (prompt 004 + prompt 011). We don't care about the payload here -
/// step 2 refetches it on mount to drive the branding render; step 1
/// just needs to know "did the endpoint return 200 or 404".
#[derive(Deserialize, Clone, Debug)]
struct PortalHostSnippet {
    #[serde(default)]
    #[allow(dead_code)]
    company_name: String,
}

/// Pure branch classifier: turn the raw fetch outcome into the render
/// decision. Kept a plain function so `#[cfg(test)]` can exercise every
/// branch without a browser (matches the picker.rs pattern from
/// prompt 010).
#[derive(Debug, PartialEq, Eq)]
pub enum VerifyBranch {
    /// Step 1 has not fired a fetch yet (idle) OR the fetch is still
    /// in flight. Render the form; disable the button while saving.
    Idle,
    /// Portal ID matched. Carries the id back to the caller so the
    /// nav.replace can build the URL.
    Hit(String),
    /// Portal ID did not match. Render the "not found" copy inline.
    Miss,
    /// Transport-layer failure (offline, DNS, 500). Render the generic
    /// fallback copy inline. Distinct from Miss so a repeated Continue
    /// against an unresolvable host does not read as "your ID is wrong".
    NetworkError,
}

/// Pure classifier for the fetch outcome. Consumed by the submit
/// handler + unit tests.
pub fn classify_verify(
    status: Option<u16>,
    network_ok: bool,
    portal_id_input: &str,
) -> VerifyBranch {
    if !network_ok {
        return VerifyBranch::NetworkError;
    }
    match status {
        Some(200) => VerifyBranch::Hit(portal_id_input.to_string()),
        Some(404) => VerifyBranch::Miss,
        Some(_) => VerifyBranch::NetworkError,
        None => VerifyBranch::Idle,
    }
}

#[component]
pub fn ContactGenericLoginPage() -> Element {
    let nav = use_navigator();
    let mut portal_id = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let mut submit = move |_| {
        if saving() {
            return;
        }
        let pid_raw = portal_id.read().trim().to_string();
        if pid_raw.is_empty() {
            error.set("Enter your Portal ID.".to_string());
            return;
        }
        if !handle_is_portal_id_shape(&pid_raw) {
            error.set(format!(
                "Portal ID must be exactly {PORTAL_ID_DIGITS} digits."
            ));
            return;
        }
        saving.set(true);
        error.set(String::new());
        let pid_for_store = pid_raw.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let path = format!("/contact/portal/{pid_for_store}/host");
                let branch = match crate::hooks::fetch::api::get_typed::<PortalHostSnippet>(&path)
                    .await
                {
                    Ok(_) => classify_verify(Some(200), true, &pid_for_store),
                    Err(ApiError::Status { code: 404, .. }) => classify_verify(Some(404), true, &pid_for_store),
                    Err(ApiError::Status { code, .. }) => {
                        classify_verify(Some(code), true, &pid_for_store)
                    }
                    Err(ApiError::Network(_)) => classify_verify(None, false, &pid_for_store),
                    Err(_) => classify_verify(None, false, &pid_for_store),
                };
                match branch {
                    VerifyBranch::Hit(pid) => {
                        crate::hooks::fetch::api::set_contact_last_portal_id(&pid);
                        nav.replace(Route::ContactHandleLogin { handle: pid });
                    }
                    VerifyBranch::Miss => {
                        error.set(
                            "Portal ID not found. Check the number your MSP sent you."
                                .to_string(),
                        );
                    }
                    VerifyBranch::NetworkError => {
                        error.set(
                            "Could not reach the portal service. Check your connection and try again."
                                .to_string(),
                        );
                    }
                    VerifyBranch::Idle => {
                        // Unreachable: the classifier only returns Idle
                        // when `status` is None + `network_ok` true,
                        // which the match above never produces.
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = pid_for_store;
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "text-center mb-6",
                h1 { class: "text-2xl font-semibold text-content", "Sign in to your portal" }
                p { class: "mt-2 text-sm text-content",
                    "Enter your Portal ID to continue. Your MSP sent it to you when portal access was granted."
                }
            }
            form {
                class: "space-y-4",
                autocomplete: "off",
                onsubmit: move |evt: Event<FormData>| {
                    evt.prevent_default();
                    submit(());
                },
                div { class: "space-y-1",
                    label {
                        r#for: "portal_id_input",
                        class: "block text-sm font-medium text-content",
                        "Portal ID"
                        span { class: "text-red-500 ml-1", aria_label: "required", role: "img", "*" }
                    }
                    input {
                        id: "portal_id_input",
                        name: "portal_id_input",
                        r#type: "text",
                        autocomplete: "off",
                        inputmode: "numeric",
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
                        "Continue"
                    }
                }
            }
            // MAPPS-615: cross-plane switch. A visitor who lands here
            // but actually needs the staff MSP console can jump without
            // browser-back-buttoning. Sits below the primary form so
            // the client-portal action stays visually dominant.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_branch_carries_id() {
        assert_eq!(
            classify_verify(Some(200), true, "555556666"),
            VerifyBranch::Hit("555556666".to_string())
        );
    }

    #[test]
    fn miss_branch_on_404() {
        assert_eq!(
            classify_verify(Some(404), true, "555556666"),
            VerifyBranch::Miss
        );
    }

    #[test]
    fn network_error_when_transport_failed() {
        assert_eq!(
            classify_verify(None, false, "555556666"),
            VerifyBranch::NetworkError
        );
    }

    #[test]
    fn network_error_on_unexpected_5xx() {
        assert_eq!(
            classify_verify(Some(503), true, "555556666"),
            VerifyBranch::NetworkError
        );
    }

    #[test]
    fn portal_id_shape_accepts_nine_digits() {
        assert!(handle_is_portal_id_shape("555556666"));
    }

    #[test]
    fn portal_id_shape_rejects_too_short_too_long_or_nonnumeric() {
        assert!(!handle_is_portal_id_shape("55555666"));
        assert!(!handle_is_portal_id_shape("5555566666"));
        assert!(!handle_is_portal_id_shape("K3F9M7N2Q8XR5J4W"));
        assert!(!handle_is_portal_id_shape(""));
    }
}

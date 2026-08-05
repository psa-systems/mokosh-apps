//! Forced-onboarding screen for freshly JIT-created Bunyip users.
//!
//! mokosh-server's `upsert_user_from_oidc` creates the row with a
//! synthetic name derived from the email local-part and leaves
//! `profile_completed_at` NULL. The SPA's `AuthGuard` redirects every
//! `profile_completed = false` user to this screen and gates all other
//! authenticated routes behind it. Submitting first + last name calls
//! `PUT /api/v1/auth/me`; the server side flips
//! `profile_completed_at = COALESCE(profile_completed_at, NOW())` and
//! `/api/v1/auth/me` starts reporting `profile_completed = true`. The
//! SPA refreshes its in-memory `CurrentUser` from the PUT response so
//! the gate stops firing on the same tick.
//!
//! Why a dedicated page (not a modal on the dashboard): the gate must
//! be impossible to bypass via direct URL or refresh, and the
//! AuthGuard render-time redirect achieves that only when the user is
//! on a distinct route the guard can identify and exempt. Modal-based
//! onboarding patterns leak content to the underlying page on cookie
//! / API hiccups.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::utils::{FormGuard, Rule};
use crate::Route;

/// What `PUT /api/v1/auth/me` accepts. Only the two required fields are
/// sent from the onboarding screen; the server's `update_user` handler
/// flips `profile_completed_at` once it sees both as non-empty.
#[derive(Clone, Debug, Serialize)]
struct OnboardingRequest {
    first_name: Option<String>,
    last_name: Option<String>,
}

/// What we read off the PUT response so we can update the in-memory
/// `CurrentUser` without a second round-trip. Mirrors the relevant
/// fields of mokosh-server's `UserResponse`.
#[derive(Clone, Debug, Deserialize)]
struct OnboardingResponse {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    /// `true` once the server has stamped `profile_completed_at`.
    /// Default `true` for backward-compat with older server builds
    /// that omit the field.
    #[serde(default = "crate::utils::default_true")]
    profile_completed: bool,
}

#[component]
pub fn Onboarding() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error slots, fed by the FormGuard in
    // handle_submit. The form-level `error` banner is kept for the server
    // save failure, which has no single field to attach to.
    let mut first_name_error = use_signal(String::new);
    let mut last_name_error = use_signal(String::new);

    // Defence in depth: if a user with `profile_completed = true`
    // hits /onboarding/profile directly (bookmark, refresh, manual
    // URL), bounce them to the dashboard. Otherwise this route would
    // let a user re-trigger the onboarding flow for no reason.
    //
    // MAPPS-317: also gated on `server_loaded`. The AuthGuard now
    // only redirects to /onboarding/profile after /me has reconciled,
    // so by the time this effect runs the auth signal is settled.
    // If this effect still fires unexpectedly the tracing line below
    // is what surfaces it in console logs for the next bug round.
    use_effect(move || {
        let a = auth.read();
        if !a.server_loaded {
            return;
        }
        if a.user.as_ref().is_some_and(|u| u.profile_completed) {
            tracing::info!(
                target: "onboarding",
                "profile_completed=true on /onboarding/profile, redirecting to /dashboard"
            );
            nav.replace(Route::Dashboard {});
        }
    });

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let first = first_name.read().trim().to_string();
        let last = last_name.read().trim().to_string();

        // PMS-518: validate each required field through the shared FormGuard so
        // both "you forgot to fill X" failures surface at once (each in its own
        // inline slot) and the first invalid field is focused.
        let mut guard = FormGuard::new();
        first_name_error.set(guard.field("first_name", &first, "First name", &[Rule::Required]));
        last_name_error.set(guard.field("last_name", &last, "Last name", &[Rule::Required]));
        if guard.blocked() {
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = OnboardingRequest {
                    first_name: Some(first.clone()),
                    last_name: Some(last.clone()),
                };
                match crate::hooks::fetch::api::put_authed_typed::<OnboardingResponse, _>(
                    "/auth/me", &body,
                )
                .await
                {
                    Ok(resp) => {
                        // Reflect the server's authoritative values back
                        // into the in-memory CurrentUser so the gate's
                        // next render sees `profile_completed = true`
                        // and the redirect below lands cleanly.
                        let mut a = auth.write();
                        if let Some(u) = a.user.as_mut() {
                            u.first_name = resp.first_name;
                            u.last_name = resp.last_name;
                            u.profile_completed = resp.profile_completed;
                        }
                        drop(a);
                        nav.replace(Route::Dashboard {});
                    }
                    Err(e) => {
                        error.set(format!("Could not save: {}", e.user_message()));
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (first, last);
            }
            saving.set(false);
        });
    };

    rsx! {
        AuthLayout {
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content",
                            "Welcome to Mokosh"
                        }
                        p { class: "mt-2 text-sm text-content",
                            "Tell us your name so we know what to call you."
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
                        },

                        Input {
                            name: "first_name",
                            label: "First name",
                            value: first_name(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: first_name_error(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                first_name_error.set(String::new());
                                first_name.set(e.value());
                            },
                        }

                        Input {
                            name: "last_name",
                            label: "Last name",
                            value: last_name(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: last_name_error(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                last_name_error.set(String::new());
                                last_name.set(e.value());
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
                                if saving() { "Saving..." } else { "Continue" }
                            }
                        }
                    }

                    p { class: "mt-6 text-center text-xs text-muted",
                        "Other profile settings (timezone, preferences) can be edited later from Profile."
                    }
        }
    }
}

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

use crate::components::{Button, ButtonVariant, Input};
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
    #[serde(default = "default_true")]
    profile_completed: bool,
}

fn default_true() -> bool {
    true
}

#[component]
pub fn Onboarding() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Defence in depth: if a user with `profile_completed = true`
    // hits /onboarding/profile directly (bookmark, refresh, manual
    // URL), bounce them to the dashboard. Otherwise this route would
    // let a user re-trigger the onboarding flow for no reason.
    use_effect(move || {
        if auth
            .read()
            .user
            .as_ref()
            .is_some_and(|u| u.profile_completed)
        {
            nav.replace(Route::Dashboard {});
        }
    });

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let first = first_name.read().trim().to_string();
        let last = last_name.read().trim().to_string();
        if first.is_empty() || last.is_empty() {
            error.set("First and last name are required.".to_string());
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
        div { class: "min-h-screen bg-gray-100 dark:bg-gray-900 flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-8",
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-gray-900 dark:text-white",
                            "Welcome to Mokosh"
                        }
                        p { class: "mt-2 text-sm text-gray-600 dark:text-gray-300",
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
                            disabled: saving(),
                            oninput: move |e: FormEvent| first_name.set(e.value()),
                        }

                        Input {
                            name: "last_name",
                            label: "Last name",
                            value: last_name(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| last_name.set(e.value()),
                        }

                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
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

                    p { class: "mt-6 text-center text-xs text-gray-500 dark:text-gray-400",
                        "Other profile settings (timezone, preferences) can be edited later from Profile."
                    }
                }
            }
        }
    }
}

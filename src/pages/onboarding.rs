//! Forced-onboarding screen for freshly JIT-created Bunyip users.
//!
//! mokosh-server's `upsert_user_from_oidc` creates the row with a
//! synthetic name derived from the email local-part and leaves
//! `profile_completed_at` NULL. The SPA's `AuthGuard` redirects every
//! `profile_completed = false` user to this screen and gates all other
//! authenticated routes behind it.
//!
//! PMS-752: this screen asks for the ORGANISATION name, not the user's
//! name, and that is not a cosmetic swap.
//!
//! It used to collect first + last name and PUT them to `/auth/me`. PMS-512
//! made bunyip the owner of those names and removed them from
//! `UpdateUserRequest`, so the values were accepted by the browser and
//! discarded by the server; worse, `profile_completed_at` moved to
//! `upsert_user_from_oidc`, stamped only on a login whose claims carry both
//! names. A user who reached this screen therefore could not leave it: the
//! one thing it could submit was the one thing that no longer completed a
//! profile. It only looked healthy because bunyip normally supplies names and
//! the screen is normally skipped.
//!
//! What it collects now is a value mokosh does own and does need: the
//! organisation name every client sees on request-form and invitation email,
//! which otherwise stays "My workspace" until someone finds Settings. Saving
//! calls `PUT /api/v1/tenants/current` (PMS-751), then
//! `POST /api/v1/auth/me/complete-onboarding` to stamp the profile. Contact
//! name and phone are MAPPS-429: they need somewhere to be written first.
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

/// What `PUT /api/v1/tenants/current` accepts from this screen.
#[derive(Clone, Debug, Serialize)]
struct OrganizationRequest {
    name: String,
}

/// What we read off `POST /api/v1/auth/me/complete-onboarding` so the
/// in-memory `CurrentUser` reflects the stamp without a second round-trip.
#[derive(Clone, Debug, Deserialize)]
struct OnboardingResponse {
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

    let mut org_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error slots, fed by the FormGuard in
    // handle_submit. The form-level `error` banner is kept for the server
    // save failure, which has no single field to attach to.
    let mut org_name_error = use_signal(String::new);

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
        let name = org_name.read().trim().to_string();

        // PMS-518: validate through the shared FormGuard so the failure lands
        // in the field's own slot and the field is focused.
        let mut guard = FormGuard::new();
        org_name_error.set(guard.field("org_name", &name, "Organization name", &[Rule::Required]));
        if guard.blocked() {
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // The organisation name first. If this fails, the profile is
                // deliberately NOT stamped: completing onboarding without the
                // one value it exists to collect would send the user on with a
                // tenant still called "My workspace" and no prompt to fix it.
                let body = OrganizationRequest { name: name.clone() };
                let saved = crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                    "/tenants/current",
                    &body,
                )
                .await;
                if let Err(e) = saved {
                    match e.field_message("name") {
                        Some(m) => org_name_error.set(m),
                        None => error.set(format!("Could not save: {}", e.user_message())),
                    }
                    saving.set(false);
                    return;
                }

                // Then the stamp. Separate call because the two are different
                // records: one is the tenant, one is the user.
                match crate::hooks::fetch::api::post_authed_typed::<OnboardingResponse, _>(
                    "/auth/me/complete-onboarding",
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(resp) => {
                        // Reflect the server's authoritative value back into
                        // the in-memory CurrentUser so the gate's next render
                        // sees `profile_completed = true` and the redirect
                        // below lands cleanly.
                        let mut a = auth.write();
                        if let Some(u) = a.user.as_mut() {
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
                let _ = name;
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
                            "What should your clients see when you email them?"
                        }
                    }

                    form {
                        class: "space-y-4",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
                        },

                        Input {
                            name: "org_name",
                            label: "Organization name",
                            value: org_name(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: org_name_error(),
                            disabled: saving(),
                            help: "Shown to your clients on the request forms you send and the email that carries them. Change it later under Settings, Organization.".to_string(),
                            oninput: move |e: FormEvent| {
                                org_name_error.set(String::new());
                                org_name.set(e.value());
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
                                "Continue"
                            }
                        }
                    }

                    p { class: "mt-6 text-center text-xs text-muted",
                        "Your name comes from the account you signed in with. Other settings (timezone, preferences) can be edited later from Profile."
                    }
        }
    }
}

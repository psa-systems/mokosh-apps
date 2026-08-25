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
//! MAPPS-524: the organisation half of that is asked only of admins.
//! `PUT /tenants/current` is admin-gated on mokosh-server (PMS-751: the name
//! is customer-facing, so it is tenant-wide configuration), and every invite
//! goes out as the lowest-privilege role (`src/pages/team.rs`). A technician
//! with no bunyip name claims therefore filled this screen in, read "Could not
//! save: Access denied" from the org save, and never reached the stamp that
//! releases the AuthGuard: MAPPS-430's failure mode, narrowed to the
//! population invites create. A non-admin now gets the confirmation screen and
//! the stamp alone.
//!
//! Why a dedicated page (not a modal on the dashboard): the gate must
//! be impossible to bypass via direct URL or refresh, and the
//! AuthGuard render-time redirect achieves that only when the user is
//! on a distinct route the guard can identify and exempt. Modal-based
//! onboarding patterns leak content to the underlying page on cookie
//! / API hiccups.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant, FileField, Input};
use crate::utils::{FormGuard, Rule};
use crate::Route;

/// What `PUT /api/v1/tenants/current` accepts from this screen.
#[derive(Clone, Debug, Serialize)]
struct OrganizationRequest {
    name: String,
    branding: OnboardingBranding,
}

/// MAPPS-429: organisation metadata, distinct from the user's own identity.
/// Contact name and phone are required here because a client who receives a
/// request form should always have someone to ask; the logo is optional and
/// uploads separately (it is a file, not a JSON field).
#[derive(Clone, Debug, Serialize)]
struct OnboardingBranding {
    support_contact_name: String,
    support_phone: String,
    /// PMS-755: optional here, unlike the phone. Demanding two channels before
    /// an MSP can finish setting up is a tax on the common case.
    #[serde(skip_serializing_if = "String::is_empty")]
    support_email: String,
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

/// One request this screen issues when its button is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnboardingCall {
    /// `PUT /tenants/current`, admin-gated on the server.
    SaveOrganization,
    /// `PUT /tenants/current/logo`, optional and non-blocking.
    UploadLogo,
    /// `POST /auth/me/complete-onboarding`, the stamp that releases the gate.
    CompleteOnboarding,
}

/// MAPPS-524: what submitting the screen sends, in order. The submit handler
/// executes exactly this list, so the role branch is one testable decision
/// rather than conditions scattered through an async block.
///
/// A non-admin sends the stamp alone: the org save would 403, and the page
/// refuses (deliberately) to stamp after a failed org save, so including it
/// would trap the user on this screen.
fn planned_calls(is_admin: bool, has_logo: bool) -> Vec<OnboardingCall> {
    if !is_admin {
        return vec![OnboardingCall::CompleteOnboarding];
    }
    let mut calls = vec![OnboardingCall::SaveOrganization];
    if has_logo {
        calls.push(OnboardingCall::UploadLogo);
    }
    calls.push(OnboardingCall::CompleteOnboarding);
    calls
}

#[component]
pub fn Onboarding() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

    // MAPPS-524: the role decides which screen this is. Read with the same
    // `use_auth()` shape as `src/pages/settings.rs`, but gated on
    // `server_loaded` (MAPPS-317): the optimistic rehydrate seeds
    // `UserRole::default()` (Technician), so an admin who lands here before
    // /me reconciles would otherwise be shown the non-admin screen and could
    // press through it, skipping the organisation entirely.
    let (server_loaded, is_admin, org) = {
        let a = auth.read();
        (
            a.server_loaded,
            a.user.as_ref().map(|u| u.role.is_admin()).unwrap_or(false),
            a.active_org_name().map(str::to_string),
        )
    };

    let mut org_name = use_signal(String::new);
    let mut contact_name = use_signal(String::new);
    let mut contact_phone = use_signal(String::new);
    let mut contact_email = use_signal(String::new);
    let mut logo: Signal<Option<(String, String, Vec<u8>)>> = use_signal(|| None);
    let mut logo_error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // PMS-518: per-field inline error slots, fed by the FormGuard in
    // handle_submit. The form-level `error` banner is kept for the server
    // save failure, which has no single field to attach to.
    let mut org_name_error = use_signal(String::new);
    let mut contact_name_error = use_signal(String::new);
    let mut contact_phone_error = use_signal(String::new);

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
        let contact = contact_name.read().trim().to_string();
        let phone = contact_phone.read().trim().to_string();
        let email = contact_email.read().trim().to_string();

        // PMS-518: validate through the shared FormGuard so every missing field
        // surfaces at once, each in its own slot, and the first is focused.
        // MAPPS-524: only the admin screen has these fields to validate.
        if is_admin {
            let mut guard = FormGuard::new();
            org_name_error.set(guard.field(
                "org_name",
                &name,
                "Organization name",
                &[Rule::Required],
            ));
            contact_name_error.set(guard.field(
                "contact_name",
                &contact,
                "Contact name",
                &[Rule::Required],
            ));
            contact_phone_error.set(guard.field(
                "contact_phone",
                &phone,
                "Contact phone",
                &[Rule::Required],
            ));
            if guard.blocked() {
                return;
            }
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // The order is the plan's order: the organisation first, then
                // the logo, then the stamp.
                for call in planned_calls(is_admin, logo.read().is_some()) {
                    match call {
                        // If this fails, the profile is deliberately NOT
                        // stamped: completing onboarding without the one value
                        // it exists to collect would send the user on with a
                        // tenant still called "My workspace" and no prompt to
                        // fix it.
                        OnboardingCall::SaveOrganization => {
                            let body = OrganizationRequest {
                                name: name.clone(),
                                branding: OnboardingBranding {
                                    support_contact_name: contact.clone(),
                                    support_phone: phone.clone(),
                                    support_email: email.clone(),
                                },
                            };
                            let saved = crate::hooks::fetch::api::put_authed_typed::<
                                serde_json::Value,
                                _,
                            >("/tenants/current", &body)
                            .await;
                            match saved {
                                // MAPPS-571: the org loader has already run and
                                // cached whatever the tenant was called before
                                // this screen, which for a fresh tenant is the
                                // seeded "My workspace" (PMS-751). Without this
                                // the admin names their company, lands on the
                                // dashboard, and the top bar shows them the
                                // placeholder they just replaced.
                                //
                                // The response is a `Value` here, so read the
                                // stored name out of it and fall back to the
                                // submitted one if the key is absent.
                                Ok(response) => {
                                    let stored = response
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(name.as_str());
                                    auth.write().set_active_org_name(stored);
                                }
                                Err(e) => {
                                    match e.field_message("name") {
                                        Some(m) => org_name_error.set(m),
                                        None => error
                                            .set(format!("Could not save: {}", e.user_message())),
                                    }
                                    saving.set(false);
                                    return;
                                }
                            }
                        }

                        // MAPPS-429: the logo, if one was chosen. After the
                        // details and before the stamp, and a failure here does
                        // NOT block the rest: a logo is the one optional thing
                        // on this screen, and refusing to complete onboarding
                        // over it would trap the user.
                        OnboardingCall::UploadLogo => {
                            if let Some((file_name, mime, bytes)) = logo.read().clone() {
                                if let Err(e) = crate::hooks::fetch::api::put_file_authed::<
                                    serde_json::Value,
                                >(
                                    "/tenants/current/logo", &file_name, &mime, &bytes
                                )
                                .await
                                {
                                    crate::hooks::push_toast(
                                        crate::components::AlertType::Warning,
                                        format!(
                                            "Saved, but the logo could not be uploaded: {}",
                                            e.user_message()
                                        ),
                                    );
                                }
                            }
                        }

                        // Then the stamp. Separate call because the two are
                        // different records: one is the tenant, one is the
                        // user.
                        OnboardingCall::CompleteOnboarding => {
                            match crate::hooks::fetch::api::post_authed_typed::<OnboardingResponse, _>(
                                "/auth/me/complete-onboarding",
                                &serde_json::json!({}),
                            )
                            .await
                            {
                                Ok(resp) => {
                                    // Reflect the server's authoritative value
                                    // back into the in-memory CurrentUser so the
                                    // gate's next render sees
                                    // `profile_completed = true` and the
                                    // redirect below lands cleanly.
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
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (name, contact, phone, email, is_admin);
            }
            saving.set(false);
        });
    };

    let brand = crate::branding::product_name();

    // MAPPS-524: hold the screen until /me has reconciled, so the branch below
    // reads the authoritative role rather than the rehydrate's default.
    if !server_loaded {
        return rsx! {
            AuthLayout {
                p { class: "text-center text-sm text-muted", "Setting up your profile…" }
            }
        };
    }

    // MAPPS-524: a non-admin is not asked for the organisation, the contact or
    // the logo, because the server would refuse every one of those writes. The
    // only action left on the screen is the stamp that releases the gate.
    if !is_admin {
        let org_line = match org.as_deref() {
            Some(name) => format!("Your account is set up under {name}."),
            None => "Your account is set up.".to_string(),
        };
        return rsx! {
            AuthLayout {
                div { class: "text-center mb-6",
                    h1 { class: "text-2xl font-semibold text-content",
                        "Welcome to {brand}"
                    }
                    p { class: "mt-2 text-sm text-content",
                        "You are all set. {org_line}"
                    }
                }

                p { class: "text-sm text-muted text-center",
                    "Your organization's details are set up by an administrator, so there is nothing for you to fill in here."
                }

                if !error().is_empty() {
                    p { role: "alert", class: "mt-4 text-sm text-red-600 dark:text-red-400", "{error}" }
                }

                div { class: "pt-6",
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: saving(),
                        loading: saving(),
                        class: "w-full".to_string(),
                        onclick: move |_| handle_submit(()),
                        "Continue"
                    }
                }

                p { class: "mt-6 text-center text-xs text-muted",
                    "Your name comes from the account you signed in with. Other settings (timezone, preferences) can be edited later from Profile."
                }
            }
        };
    }

    rsx! {
        AuthLayout {
                    div { class: "text-center mb-6",
                        h1 { class: "text-2xl font-semibold text-content",
                            "Welcome to {brand}"
                        }
                        p { class: "mt-2 text-sm text-content",
                            "Set up what your clients see when you send them a request form."
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

                        Input {
                            name: "contact_name",
                            label: "Contact name",
                            value: contact_name(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: contact_name_error(),
                            disabled: saving(),
                            help: "Who your clients should ask for. Shown on the forms you send them.".to_string(),
                            oninput: move |e: FormEvent| {
                                contact_name_error.set(String::new());
                                contact_name.set(e.value());
                            },
                        }

                        Input {
                            name: "contact_phone",
                            label: "Contact phone",
                            value: contact_phone(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: contact_phone_error(),
                            disabled: saving(),
                            help: "So a client can ask before they answer.".to_string(),
                            oninput: move |e: FormEvent| {
                                contact_phone_error.set(String::new());
                                contact_phone.set(e.value());
                            },
                        }

                        Input {
                            name: "contact_email",
                            label: "Contact email (optional)",
                            r#type: "email".to_string(),
                            value: contact_email(),
                            disabled: saving(),
                            help: "Offered to clients alongside the phone number.".to_string(),
                            oninput: move |e: FormEvent| contact_email.set(e.value()),
                        }

                        // MAPPS-429: optional, and read into memory here rather
                        // than uploaded on selection: nothing else on this
                        // screen has been saved yet, and a logo attached to a
                        // tenant whose details were never submitted is litter.
                        FileField {
                            name: "org_logo",
                            // MAPPS-443: matches the settings field. The help line
                            // below now opens with "Optional.", so keeping it in the
                            // label too said it twice on one control.
                            label: "Logo",
                            accept: "image/png,image/jpeg,image/webp,image/gif",
                            disabled: saving(),
                            error: logo_error(),
                            // MAPPS-443: verbatim from the settings logo field. The
                            // two screens render the same field with the same accept
                            // list and the same 1 MB cap, and this copy is the longer
                            // of the two because it says where the logo actually shows
                            // up, which is the part an operator needs.
                            help: "Optional. PNG, JPEG, WebP or GIF, up to 1 MB. Shown to clients at the top of the request forms you send and the email that carries them.",
                            onchange: move |evt: FormEvent| {
                                    logo_error.set(String::new());
                                    let Some(file) = evt.files().into_iter().next() else {
                                        logo.set(None);
                                        return;
                                    };
                                    spawn(async move {
                                        let file_name = file.name();
                                        let mime = file
                                            .content_type()
                                            .unwrap_or_else(|| "application/octet-stream".to_string());
                                        match file.read_bytes().await {
                                            Ok(bytes) => logo.set(Some((file_name, mime, bytes.to_vec()))),
                                            Err(_) => {
                                                logo.set(None);
                                                logo_error.set("Could not read that file.".to_string());
                                            }
                                        }
                                    });
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

#[cfg(test)]
mod tests {
    use super::OnboardingCall::*;
    use super::*;

    /// MAPPS-524: `PUT /tenants/current` is admin-gated on mokosh-server, and
    /// this page will not stamp a profile after a failed org save, so a
    /// non-admin who sent it would be stuck on this screen for good.
    #[test]
    fn a_non_admin_only_stamps_the_profile() {
        assert_eq!(planned_calls(false, false), vec![CompleteOnboarding]);
        assert_eq!(
            planned_calls(false, true),
            vec![CompleteOnboarding],
            "the non-admin screen has no logo field, and the logo endpoint is admin-gated too"
        );
    }

    /// The admin path is unchanged: the organisation is saved before the
    /// profile is stamped, with the optional logo in between.
    #[test]
    fn an_admin_saves_the_organisation_before_the_stamp() {
        assert_eq!(
            planned_calls(true, false),
            vec![SaveOrganization, CompleteOnboarding]
        );
        assert_eq!(
            planned_calls(true, true),
            vec![SaveOrganization, UploadLogo, CompleteOnboarding]
        );
    }
}

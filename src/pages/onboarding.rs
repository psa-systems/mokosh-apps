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

#[component]
pub fn Onboarding() -> Element {
    let mut auth = crate::hooks::use_auth();
    let nav = use_navigator();

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
        let mut guard = FormGuard::new();
        org_name_error.set(guard.field("org_name", &name, "Organization name", &[Rule::Required]));
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
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // The organisation name first. If this fails, the profile is
                // deliberately NOT stamped: completing onboarding without the
                // one value it exists to collect would send the user on with a
                // tenant still called "My workspace" and no prompt to fix it.
                let body = OrganizationRequest {
                    name: name.clone(),
                    branding: OnboardingBranding {
                        support_contact_name: contact.clone(),
                        support_phone: phone.clone(),
                        support_email: email.clone(),
                    },
                };
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

                // MAPPS-429: the logo, if one was chosen. After the details and
                // before the stamp, and a failure here does NOT block the rest:
                // a logo is the one optional thing on this screen, and refusing
                // to complete onboarding over it would trap the user.
                if let Some((file_name, mime, bytes)) = logo.read().clone() {
                    if let Err(e) = crate::hooks::fetch::api::put_file_authed::<serde_json::Value>(
                        "/tenants/current/logo",
                        &file_name,
                        &mime,
                        &bytes,
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
                let _ = (name, contact, phone, email);
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
                        div { class: "space-y-1",
                            label {
                                r#for: "org_logo",
                                class: "block text-sm font-medium text-content",
                                "Logo (optional)"
                            }
                            input {
                                id: "org_logo",
                                r#type: "file",
                                accept: "image/png,image/jpeg,image/webp,image/gif",
                                disabled: saving(),
                                class: "block w-full text-sm text-content file:mr-3 file:rounded-md file:border-0 file:bg-surface-2 file:px-3 file:py-1.5 file:text-sm file:font-medium",
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
                            // MAPPS-443: `role="alert"` and the settings help
                            // copy verbatim, so the same field reads and
                            // announces the same on both pages.
                            if !logo_error().is_empty() {
                                p { class: "text-xs text-red-600 dark:text-red-400", role: "alert", "{logo_error}" }
                            } else {
                                p { class: "text-xs text-muted",
                                    "Optional. PNG, JPEG, WebP or GIF, up to 1 MB. Shown to clients at the top of the request forms you send and the email that carries them."
                                }
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

                    p { class: "mt-6 text-center text-xs text-muted",
                        "Your name comes from the account you signed in with. Other settings (timezone, preferences) can be edited later from Profile."
                    }
        }
    }
}

#[cfg(test)]
mod logo_field_tests {
    /// A file's source minus its test module, so the assertions below can name
    /// the very strings they pin without matching themselves.
    fn production(src: &'static str) -> &'static str {
        src.split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .unwrap_or(src)
    }

    fn onboarding() -> &'static str {
        production(include_str!("onboarding.rs"))
    }

    fn settings() -> &'static str {
        production(include_str!("settings.rs"))
    }

    /// The string literal holding the logo field's help copy.
    fn help_copy(src: &'static str) -> &'static str {
        let start = src
            .find("PNG, JPEG, WebP or GIF")
            .expect("the logo field describes its accepted formats");
        let open = src[..start]
            .rfind('"')
            .expect("the help copy is a string literal");
        let rest = &src[open + 1..];
        let end = rest.find('"').expect("the literal is terminated");
        &rest[..end]
    }

    /// MAPPS-443: onboarding and settings render the same organization-logo
    /// field, and the copy had drifted: onboarding dropped "Optional" from an
    /// optional field and never mentioned the email the logo also appears on.
    #[test]
    fn both_logo_fields_describe_themselves_identically() {
        assert_eq!(help_copy(onboarding()), help_copy(settings()));
        assert!(help_copy(onboarding()).starts_with("Optional."));
    }

    /// MAPPS-443: a read failure is rendered in place of the help text. Without
    /// `role="alert"` a screen-reader user gets no notification at all, which
    /// is what onboarding shipped while settings announced correctly.
    #[test]
    fn the_logo_read_error_announces_itself_on_both_pages() {
        for (page, src) in [("onboarding.rs", onboarding()), ("settings.rs", settings())] {
            let mut seen = 0;
            for (idx, _) in src.match_indices("\"{logo_error}\"") {
                seen += 1;
                let open = src[..idx]
                    .rfind('{')
                    .expect("the error text sits inside an element");
                // rsx puts every attribute ahead of the children, so the
                // element's attributes are exactly what precedes the text.
                assert!(
                    src[open..idx].contains("role: \"alert\""),
                    "{page}: the logo error must carry role=\"alert\""
                );
            }
            assert_eq!(seen, 1, "{page}: renders the logo error exactly once");
        }
    }
}

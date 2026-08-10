//! PMS-729 phase 2 H3: `/portal/forgot-password`, the "email me a
//! reset link" page linked from `/portal/login`.
//!
//! The server's `POST /portal/auth/forgot-password` always returns 204
//! whether the email matches a known contact or not (enumeration-
//! resistant), so this page shows the same "check your inbox" message
//! after every successful post regardless of whether an email was
//! actually sent. Distinct from the setup-password flow: setup is
//! agent-triggered ("grant portal access"), reset is customer-
//! triggered ("I forgot my password").

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{Button, ButtonVariant, Input};
use crate::Route;

/// Request body for `POST /api/v1/portal/auth/forgot-password`,
/// matching mokosh-server's `PortalForgotPasswordRequest`. `tenant_slug`
/// is omitted on portal hosts (server resolves from Host header per
/// PMS-729); the SPA drops it via `skip_serializing_if` for the same
/// reason `PortalLoginBody` does.
#[derive(Serialize)]
struct ForgotPasswordBody {
    #[serde(skip_serializing_if = "String::is_empty")]
    tenant_slug: String,
    email: String,
}

#[component]
pub fn PortalForgotPasswordPage() -> Element {
    let mut email = use_signal(String::new);
    let mut tenant =
        use_signal(|| crate::utils::url::current_query_param("tenant").unwrap_or_default());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut sent = use_signal(|| false);

    // Kick the branding fetch on mount so the copy can address the MSP
    // by name if the SPA is on a portal host (PMS-729 phase 2 §6). Idempotent.
    #[cfg(feature = "web")]
    use_hook(crate::hooks::portal_branding::ensure_portal_branding_fetch);
    let hint_snapshot = crate::hooks::portal_branding::use_portal_host_hint();
    let host_derived = hint_snapshot.is_some();

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let em = email.read().trim().to_string();
        let slug = tenant.read().trim().to_string();
        if em.is_empty() {
            error.set("Enter your email address.".to_string());
            return;
        }
        if !host_derived && slug.is_empty() {
            error.set("Enter your account name and email.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = ForgotPasswordBody {
                    tenant_slug: if host_derived {
                        String::new()
                    } else {
                        slug.clone()
                    },
                    email: em.clone(),
                };
                // 204 on success regardless of whether the email is
                // known. Anything else is a network / server error we
                // surface generically; user-safe messages only.
                match crate::hooks::fetch::api::post_typed_no_content(
                    "/portal/auth/forgot-password",
                    &body,
                )
                .await
                {
                    Ok(()) => sent.set(true),
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (em, slug);
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4",
            div { class: "max-w-md w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    if sent() {
                        div { class: "text-center", role: "status", aria_live: "polite",
                            h1 { class: "text-2xl font-semibold text-content", "Check your inbox" }
                            p { class: "mt-2 text-sm text-content",
                                "If an account exists for that email, we've sent a link you can use to set a new password. The link is valid for 30 minutes."
                            }
                            Link {
                                to: Route::PortalLogin {},
                                class: "mt-6 inline-flex items-center justify-center rounded-md bg-accent px-4 py-2 text-sm font-medium text-on-accent hover:opacity-90",
                                "Back to sign in"
                            }
                        }
                    } else {
                        div { class: "text-center mb-6",
                            // PMS-729 phase 2 §6 slice 2: pull the same
                            // logo the login page picks so this page reads
                            // as one flow. Dark-aware; falls back to the
                            // light logo when only one is supplied.
                            if let Some(hint) = &hint_snapshot {
                                {
                                    let is_dark = crate::hooks::theme::current_is_dark();
                                    rsx! {
                                        if let Some(url) = hint.branding.logo_for(is_dark) {
                                            img {
                                                src: "{url}",
                                                alt: "{hint.name}",
                                                class: "h-14 w-auto mx-auto mb-3",
                                            }
                                        }
                                    }
                                }
                            }
                            // Match the login page's branding hint so a
                            // portal-host visitor sees the MSP name in the copy.
                            if let Some(hint) = &hint_snapshot {
                                h1 { class: "text-2xl font-semibold text-content",
                                    "Reset your {hint.name} password"
                                }
                            } else {
                                h1 { class: "text-2xl font-semibold text-content",
                                    "Reset your portal password"
                                }
                            }
                            p { class: "mt-2 text-sm text-content",
                                "Enter your email and we'll send you a link to set a new password."
                            }
                        }

                        form {
                            class: "space-y-4",
                            onsubmit: move |evt: Event<FormData>| {
                                evt.prevent_default();
                                handle_submit(());
                            },

                            // Legacy body-slug field for non-portal hosts.
                            if hint_snapshot.is_none() {
                                Input {
                                    name: "tenant_slug",
                                    label: "Account name",
                                    r#type: "text".to_string(),
                                    value: tenant(),
                                    required: true,
                                    disabled: saving(),
                                    oninput: move |e: FormEvent| {
                                        error.set(String::new());
                                        tenant.set(e.value());
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

                            if !error().is_empty() {
                                p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                            }

                            div { class: "pt-2",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    disabled: saving(),
                                    loading: saving(),
                                    r#type: "submit".to_string(),
                                    class: "w-full".to_string(),
                                    "Send reset link"
                                }
                            }

                            div { class: "text-center pt-2",
                                Link {
                                    to: Route::PortalLogin {},
                                    class: "text-sm text-accent hover:opacity-80",
                                    "Back to sign in"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

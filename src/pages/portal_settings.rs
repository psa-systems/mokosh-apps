//! PMS-729 follow-up: `/portal/settings`, the customer's own account page.
//!
//! Landing spot from the user menu's "Settings" link. Displays the signed-in
//! contact's profile (read-only for now: name/email live on the agent side,
//! not the portal) and a change-password form that hits
//! `PUT /portal/auth/me/password`. MFA + active-sessions surfaces exist on the
//! server (`/portal/auth/me/mfa/*`, `/portal/auth/me/sessions`) but are
//! deferred to a follow-up so this page ships with the single most-asked
//! affordance a customer expects on a Settings page.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{Button, ButtonVariant, Card, Input};

/// Client-side floor mirroring the server's shared `utils::password_policy`
/// (PMS-729 phase 2 H5). Server enforces this + zxcvbn strength + a common-
/// password blocklist; the constant here spares the user a round-trip when
/// they type an obviously-too-short candidate.
const MIN_PASSWORD_LEN: usize = 12;

/// Request body for `PUT /api/v1/portal/auth/me/password`, matching mokosh-
/// server's `PortalChangePasswordRequest`.
#[derive(Serialize)]
struct ChangePasswordBody {
    current_password: String,
    new_password: String,
}

#[component]
pub fn PortalSettingsPage() -> Element {
    #[cfg(feature = "web")]
    let me = crate::hooks::portal_me::use_portal_me();
    #[cfg(not(feature = "web"))]
    let me: Option<crate::hooks::portal_me::PortalMe> = None;

    let mut current = use_signal(String::new);
    let mut next = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);

    let mut handle_submit = move |_| {
        if saving() {
            return;
        }
        let c = current.read().clone();
        let n = next.read().clone();
        let cf = confirm.read().clone();
        if c.is_empty() {
            error.set("Enter your current password.".to_string());
            return;
        }
        if n.chars().count() < MIN_PASSWORD_LEN {
            error.set(format!(
                "New password must be at least {MIN_PASSWORD_LEN} characters."
            ));
            return;
        }
        if n != cf {
            error.set("The two new-password fields do not match.".to_string());
            return;
        }
        if n == c {
            error.set("New password must differ from your current password.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        done.set(false);

        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = ChangePasswordBody {
                    current_password: c.clone(),
                    new_password: n.clone(),
                };
                match crate::hooks::fetch::api::put_portal_authed_json_no_content(
                    "/portal/auth/me/password",
                    &body,
                )
                .await
                {
                    Ok(()) => {
                        current.set(String::new());
                        next.set(String::new());
                        confirm.set(String::new());
                        done.set(true);
                    }
                    // Server's shape: 400 with message on policy violation
                    // (weak password, too short, common), 401 on wrong
                    // current password. Surface the server text when it
                    // has one; otherwise a friendly fallback.
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("Your current password is not correct.".to_string());
                    }
                    Err(ApiError::Status {
                        code: 400, message, ..
                    }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (c, n);
            }
            saving.set(false);
        });
    };

    rsx! {
        crate::components::PortalLayout { title: "Settings".to_string(),
            div { class: "space-y-6",
                Card {
                    h2 { class: "text-lg font-semibold text-content mb-3", "Profile" }
                    if let Some(m) = me.as_ref() {
                        dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-2 text-sm",
                            dt { class: "text-muted", "Name" }
                            dd { class: "text-content", "{m.display_name()}" }
                            dt { class: "text-muted", "Email" }
                            dd { class: "text-content", "{m.email}" }
                        }
                        p { class: "mt-3 text-xs text-muted",
                            "Name and email are managed by your account team. Contact them to update either."
                        }
                    } else {
                        p { class: "text-sm text-muted", "Loading your profile..." }
                    }
                }

                Card {
                    h2 { class: "text-lg font-semibold text-content mb-1", "Change password" }
                    p { class: "text-xs text-muted mb-4",
                        "Enter your current password, then a new one of at least {MIN_PASSWORD_LEN} characters."
                    }
                    if done() {
                        p { class: "text-sm text-green-700 dark:text-green-400 mb-3",
                            role: "status",
                            "Password updated."
                        }
                    }
                    form {
                        class: "space-y-3",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_submit(());
                        },

                        Input {
                            name: "current_password",
                            label: "Current password",
                            r#type: "password".to_string(),
                            value: current(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                current.set(e.value());
                            },
                        }
                        Input {
                            name: "new_password",
                            label: "New password",
                            r#type: "password".to_string(),
                            value: next(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                next.set(e.value());
                            },
                        }
                        Input {
                            name: "confirm_password",
                            label: "Confirm new password",
                            r#type: "password".to_string(),
                            value: confirm(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                confirm.set(e.value());
                            },
                        }

                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400",
                                role: "alert",
                                "{error}"
                            }
                        }

                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: saving(),
                                disabled: saving(),
                                r#type: "submit".to_string(),
                                "Update password"
                            }
                        }
                    }
                }
            }
        }
    }
}

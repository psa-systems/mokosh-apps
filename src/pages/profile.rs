//! Profile page (mokosh-side).
//!
//! Edits the tenant-scoped user fields that live in mokosh-server's
//! `users` table: first name, last name, phone, mobile, title,
//! timezone. Things that belong to the cross-app Bunyip identity
//! (email, password, MFA, sessions, billing) are intentionally NOT
//! here; the UserMenu's "Account Settings" link sends the user to
//! bunyip-web's `/settings` for those.
//!
//! Reads via `GET /api/v1/auth/me`, writes via `PUT /api/v1/auth/me`.
//! The server enforces that callers cannot change their own role /
//! status from this endpoint, so the form does not expose them.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AppLayout, Button, ButtonVariant, Card, Input, PageHeader};

/// Subset of mokosh-server's `UserResponse` the profile screen reads.
/// Mirrors the shape returned by `GET /api/v1/auth/me`; fields not
/// rendered here are dropped at deserialise time.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct MeResponse {
    #[serde(default)]
    email: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    mobile: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    role: String,
}

/// Body sent on `PUT /api/v1/auth/me`. Matches mokosh-server's
/// `UpdateUserRequest`; fields not editable from this screen are
/// omitted so the server's existing validation rejects any attempt to
/// change them. Empty optionals are sent as `None` (no-op on the
/// server) rather than empty strings.
#[derive(Clone, Debug, Serialize)]
struct UpdateMeRequest {
    first_name: Option<String>,
    last_name: Option<String>,
    phone: Option<String>,
    mobile: Option<String>,
    title: Option<String>,
    timezone: Option<String>,
}

fn optional_field(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[component]
pub fn ProfilePage() -> Element {
    let me_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed::<MeResponse>("/auth/me")
                .await
                .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<MeResponse>
        }
    });

    let snap = me_resource.read_unchecked();
    rsx! {
        AppLayout { title: "Profile",
            PageHeader {
                title: "Profile",
                subtitle: "Your name, title, phone, and timezone for this organization.",
            }
            match &*snap {
                None => rsx! {
                    Card {
                        div { class: "py-12 text-center text-sm text-gray-500 dark:text-gray-400",
                            "Loading profile..."
                        }
                    }
                },
                Some(None) => rsx! {
                    Card {
                        div { class: "py-12 text-center",
                            p { class: "text-sm text-red-600 dark:text-red-300",
                                "Could not load your profile. Refresh the page to retry."
                            }
                        }
                    }
                },
                Some(Some(me)) => rsx! {
                    ProfileForm { initial: me.clone() }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProfileFormProps {
    initial: MeResponse,
}

#[component]
fn ProfileForm(props: ProfileFormProps) -> Element {
    let account_settings_url =
        crate::modules::oidc::OidcConfig::for_current_origin().hub_url("/settings");
    let mut first_name = use_signal(|| props.initial.first_name.clone());
    let mut last_name = use_signal(|| props.initial.last_name.clone());
    let mut phone = use_signal(|| props.initial.phone.clone().unwrap_or_default());
    let mut mobile = use_signal(|| props.initial.mobile.clone().unwrap_or_default());
    let mut title = use_signal(|| props.initial.title.clone().unwrap_or_default());
    let mut timezone = use_signal(|| props.initial.timezone.clone());

    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);

    let email_readonly = props.initial.email.clone();
    let role_readonly = props.initial.role.clone();

    let handle_save = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = UpdateMeRequest {
            first_name: optional_field(&first_name()),
            last_name: optional_field(&last_name()),
            phone: optional_field(&phone()),
            mobile: optional_field(&mobile()),
            title: optional_field(&title()),
            timezone: optional_field(&timezone()),
        };
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<MeResponse, _>("/auth/me", &body)
                    .await
                {
                    Ok(_) => saved.set(true),
                    Err(e) => error.set(format!("Could not save profile: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-6 p-6",
                // Read-only identity strip up top so users understand
                // which account they are editing and where the
                // identity fields actually live (Bunyip).
                div { class: "rounded-md bg-gray-50 dark:bg-gray-800 p-4 text-sm",
                    div { class: "flex flex-wrap items-center justify-between gap-2",
                        div {
                            p { class: "font-medium text-gray-900 dark:text-white",
                                "{email_readonly}"
                            }
                            p { class: "text-gray-500 dark:text-gray-400",
                                "Role: {role_readonly}"
                            }
                        }
                        a {
                            href: "{account_settings_url}",
                            class: "text-sm text-blue-600 dark:text-blue-400 hover:underline",
                            "Account Settings (Bunyip)"
                        }
                    }
                    p { class: "mt-2 text-xs text-gray-500 dark:text-gray-400",
                        "Email, password, 2FA, sessions, and billing live in Account Settings on the Bunyip hub. This page only edits your mokosh profile."
                    }
                }

                if !error().is_empty() {
                    div { class: "rounded-md bg-red-50 dark:bg-red-900/40 p-3 text-sm text-red-700 dark:text-red-300",
                        "{error}"
                    }
                }
                if saved() {
                    div { class: "rounded-md bg-green-50 dark:bg-green-900/40 p-3 text-sm text-green-700 dark:text-green-300",
                        "Profile saved."
                    }
                }

                div { class: "grid gap-4 sm:grid-cols-2",
                    Input {
                        name: "first_name",
                        label: "First name",
                        value: first_name(),
                        oninput: move |e: FormEvent| first_name.set(e.value()),
                    }
                    Input {
                        name: "last_name",
                        label: "Last name",
                        value: last_name(),
                        oninput: move |e: FormEvent| last_name.set(e.value()),
                    }
                    Input {
                        name: "title",
                        label: "Title",
                        placeholder: "e.g. Senior Technician",
                        value: title(),
                        oninput: move |e: FormEvent| title.set(e.value()),
                    }
                    Input {
                        name: "timezone",
                        label: "Timezone",
                        placeholder: "America/New_York",
                        help: "IANA timezone name. Affects appointment + dispatch grid times.",
                        value: timezone(),
                        oninput: move |e: FormEvent| timezone.set(e.value()),
                    }
                    Input {
                        name: "phone",
                        label: "Work phone",
                        r#type: "tel".to_string(),
                        value: phone(),
                        oninput: move |e: FormEvent| phone.set(e.value()),
                    }
                    Input {
                        name: "mobile",
                        label: "Mobile",
                        r#type: "tel".to_string(),
                        value: mobile(),
                        oninput: move |e: FormEvent| mobile.set(e.value()),
                    }
                }

                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: handle_save,
                        disabled: saving(),
                        if saving() { "Saving..." } else { "Save changes" }
                    }
                }
            }
        }
    }
}

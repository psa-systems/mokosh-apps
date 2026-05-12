//! Settings -> Profile: view + update the caller's profile + change
//! password.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AppLayout, Button, ButtonVariant, Card, Input, PageHeader};

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct MeBody {
    email: String,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    timezone: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    mfa_enrolled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct UpdateProfileBody {
    first_name: Option<String>,
    last_name: Option<String>,
    timezone: String,
    avatar_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ProfileFormState {
    first_name: String,
    last_name: String,
    timezone: String,
    avatar_url: String,
    saving: bool,
    saved: bool,
    error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PasswordFormState {
    current_password: String,
    new_password: String,
    password_confirmation: String,
    submitting: bool,
    success: bool,
    error: Option<String>,
}

#[component]
pub fn ProfilePage() -> Element {
    let mut me: Signal<Option<Result<MeBody, String>>> = use_signal(|| None);
    let mut profile: Signal<ProfileFormState> = use_signal(ProfileFormState::default);
    let mut password: Signal<PasswordFormState> = use_signal(PasswordFormState::default);
    let mut bump = use_signal(|| 0u32);

    use_future(move || async move {
        let _ = bump.read();
        me.set(None);
        let cfg = crate::modules::oidc::OidcConfig::from_env();
        let r = crate::modules::oidc::issuer_get_authed::<MeBody>(&cfg, "/v1/auth/me")
            .await
            .map_err(|e| e.to_string());
        if let Ok(b) = &r {
            profile.set(ProfileFormState {
                first_name: b.first_name.clone().unwrap_or_default(),
                last_name: b.last_name.clone().unwrap_or_default(),
                timezone: b.timezone.clone(),
                avatar_url: b.avatar_url.clone().unwrap_or_default(),
                ..ProfileFormState::default()
            });
        }
        me.set(Some(r));
    });

    let save_profile = use_callback(move |_| {
        let p = profile.read().clone();
        spawn(async move {
            profile.with_mut(|p| {
                p.saving = true;
                p.saved = false;
                p.error = None;
            });
            let body = UpdateProfileBody {
                first_name: option_from_str(&p.first_name),
                last_name: option_from_str(&p.last_name),
                timezone: p.timezone.trim().to_string(),
                avatar_url: option_from_str(&p.avatar_url),
            };
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let r = crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                &format!("{}/v1/auth/me/profile", cfg.issuer.trim_end_matches('/')),
                &body,
            )
            .await;
            profile.with_mut(|s| {
                s.saving = false;
                match r {
                    Ok(_) => {
                        s.saved = true;
                        s.error = None;
                    }
                    Err(e) => {
                        s.saved = false;
                        s.error = Some(format!("{e}"));
                    }
                }
            });
            bump.with_mut(|n| *n += 1);
        });
    });

    let submit_password = use_callback(move |_| {
        let p = password.read().clone();
        spawn(async move {
            password.with_mut(|s| {
                s.submitting = true;
                s.success = false;
                s.error = None;
            });
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let r = crate::modules::oidc::issuer_post_authed::<serde_json::Value, _>(
                &cfg,
                "/v1/auth/me/password",
                &serde_json::json!({
                    "current_password": p.current_password,
                    "new_password": p.new_password,
                    "password_confirmation": p.password_confirmation,
                }),
            )
            .await;
            password.with_mut(|s| {
                s.submitting = false;
                match r {
                    Ok(_) => {
                        s.success = true;
                        s.error = None;
                        s.current_password.clear();
                        s.new_password.clear();
                        s.password_confirmation.clear();
                    }
                    Err(e) => {
                        s.success = false;
                        s.error = Some(format!("{e}"));
                    }
                }
            });
        });
    });

    rsx! {
        AppLayout { title: "Profile",
            PageHeader { title: "Profile", subtitle: "Your account details and password" }

            div { class: "max-w-2xl space-y-6",
                Card { title: "Profile",
                    match &*me.read() {
                        None => rsx! { p { class: "text-sm text-gray-500", "Loading..." } },
                        Some(Err(e)) => rsx! { p { class: "text-sm text-red-600", "Failed to load: {e}" } },
                        Some(Ok(b)) => rsx! {
                            form {
                                class: "space-y-4",
                                onsubmit: move |e| {
                                    e.prevent_default();
                                    save_profile.call(());
                                },
                                p { class: "text-sm text-gray-500", "Email: {b.email}" }

                                Input {
                                    name: "first_name",
                                    label: "First name",
                                    r#type: "text",
                                    value: profile.read().first_name.clone(),
                                    oninput: move |e: FormEvent| {
                                        profile.write().first_name = e.value();
                                    },
                                }
                                Input {
                                    name: "last_name",
                                    label: "Last name",
                                    r#type: "text",
                                    value: profile.read().last_name.clone(),
                                    oninput: move |e: FormEvent| {
                                        profile.write().last_name = e.value();
                                    },
                                }
                                Input {
                                    name: "timezone",
                                    label: "Timezone",
                                    r#type: "text",
                                    placeholder: "UTC",
                                    required: true,
                                    value: profile.read().timezone.clone(),
                                    oninput: move |e: FormEvent| {
                                        profile.write().timezone = e.value();
                                    },
                                }
                                Input {
                                    name: "avatar_url",
                                    label: "Avatar URL",
                                    r#type: "url",
                                    placeholder: "https://...",
                                    value: profile.read().avatar_url.clone(),
                                    oninput: move |e: FormEvent| {
                                        profile.write().avatar_url = e.value();
                                    },
                                }

                                if let Some(err) = &profile.read().error {
                                    p { class: "text-sm text-red-600", "{err}" }
                                }
                                if profile.read().saved {
                                    p { class: "text-sm text-green-600", "Saved." }
                                }

                                Button {
                                    r#type: "submit",
                                    variant: ButtonVariant::Primary,
                                    loading: profile.read().saving,
                                    "Save profile"
                                }
                            }
                        },
                    }
                }

                Card { title: "Change password",
                    form {
                        class: "space-y-4",
                        onsubmit: move |e| {
                            e.prevent_default();
                            submit_password.call(());
                        },
                        Input {
                            name: "current_password",
                            label: "Current password",
                            r#type: "password",
                            required: true,
                            value: password.read().current_password.clone(),
                            oninput: move |e: FormEvent| {
                                password.write().current_password = e.value();
                            },
                        }
                        Input {
                            name: "new_password",
                            label: "New password",
                            r#type: "password",
                            required: true,
                            value: password.read().new_password.clone(),
                            oninput: move |e: FormEvent| {
                                password.write().new_password = e.value();
                            },
                        }
                        Input {
                            name: "password_confirmation",
                            label: "Confirm new password",
                            r#type: "password",
                            required: true,
                            value: password.read().password_confirmation.clone(),
                            oninput: move |e: FormEvent| {
                                password.write().password_confirmation = e.value();
                            },
                        }
                        if let Some(err) = &password.read().error {
                            p { class: "text-sm text-red-600", "{err}" }
                        }
                        if password.read().success {
                            p { class: "text-sm text-green-600", "Password changed." }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: password.read().submitting,
                            "Change password"
                        }
                    }
                }
            }
        }
    }
}

fn option_from_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

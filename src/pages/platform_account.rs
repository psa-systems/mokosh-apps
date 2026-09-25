//! MAPPS-946: platform-admin self-service page (password + MFA).
//!
//! PMS-1293 / PMS-1300 built the platform-admin MFA lifecycle server-side,
//! explicitly modeled on the contact plane's `/me/mfa` routes. MAPPS-830
//! built the client MFA surface for that contact plane; this page is the
//! same surface for the platform-admin plane. It renders inside the
//! shared `AppShell` for a caller who holds only a platform bearer (no
//! tenant `AuthContext`), calling exactly the four routes at
//! `mokosh-server/src/modules/platform/routes.rs:48-51`:
//! `PUT /platform/me/password`, `POST /platform/me/mfa/setup`, `/enable`,
//! `/disable`. There is no `GET /platform/me` among those routes, so this
//! page has nothing to fetch on mount; the MFA-enrolled/not-enrolled
//! state it shows is the hint cached at `/platform/login` time
//! (`hooks::fetch::api::current_platform_mfa_enabled`) and is kept in
//! sync locally after every successful setup/enable/disable.

use dioxus::prelude::*;
use serde::Serialize;

use crate::components::{
    use_page_title, BannerTone, Button, ButtonVariant, Card, ErrorBanner, Input, Modal, PageHeader,
    StatusBanner,
};

/// Body for `PUT /platform/me/password`. Matches mokosh-server's
/// `PlatformChangePasswordRequest`.
#[derive(Clone, Debug, Serialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
    confirm_password: String,
}

/// Body for `POST /platform/me/mfa/setup`. Matches mokosh-server's
/// `PlatformMfaSetupRequest`.
#[derive(Clone, Debug, Serialize)]
struct MfaSetupRequest {
    current_password: String,
}

/// `POST /platform/me/mfa/setup` response. Matches mokosh-server's
/// `PlatformMfaSetupResponse`.
#[derive(Clone, Debug, serde::Deserialize)]
struct MfaSetupResponse {
    secret: String,
    provisioning_uri: String,
}

/// Body for `POST /platform/me/mfa/enable`. Matches mokosh-server's
/// `PlatformMfaEnableRequest`.
#[derive(Clone, Debug, Serialize)]
struct MfaEnableRequest {
    current_password: String,
    code: String,
}

/// `POST /platform/me/mfa/enable` response. Matches mokosh-server's
/// `PlatformMfaEnableResponse`.
#[derive(Clone, Debug, serde::Deserialize)]
struct MfaEnableResponse {
    recovery_codes: Vec<String>,
}

/// Body for `POST /platform/me/mfa/disable`. Matches mokosh-server's
/// `PlatformMfaDisableRequest`.
#[derive(Clone, Debug, Serialize)]
struct MfaDisableRequest {
    current_password: String,
    code: String,
}

/// `/admin/platform-account`. The `AuthGuard` branch in `App` (`src/lib.rs`)
/// lets any signed-in caller (tenant or platform-bearer-only) reach the
/// shared `AppShell` this route lives in, so a tenant-only session could
/// otherwise land here with no platform bearer to call any of the four
/// routes below. Gate on holding a platform bearer and redirect to sign-in
/// instead of rendering an all-401 shell.
#[component]
pub fn PlatformAccountPage() -> Element {
    #[cfg(feature = "web")]
    {
        if crate::hooks::fetch::api::current_platform_access_token().is_none() {
            let nav = use_navigator();
            use_hook(move || {
                nav.push(crate::Route::Login {});
            });
            return rsx! {
                div { class: "p-6 text-sm text-content",
                    "Redirecting to sign-in…"
                }
            };
        }
    }
    use_page_title("Platform Admin Account");
    let mfa_enabled = use_signal(crate::hooks::fetch::api::current_platform_mfa_enabled);

    rsx! {
        PageHeader {
            title: "Platform Admin Account",
            subtitle: "Password and two-factor authentication for your platform-admin sign-in, independent of any tenant.",
        }
        PlatformPasswordCard {}
        PlatformMfaCard { mfa_enabled: mfa_enabled() }
    }
}

#[component]
fn PlatformPasswordCard() -> Element {
    let mut current_password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();

    let handle_save = move |_| {
        if saving() || !can_mutate {
            return;
        }
        if new_password() != confirm_password() {
            error.set("New password and confirmation do not match.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = ChangePasswordRequest {
            current_password: current_password(),
            new_password: new_password(),
            confirm_password: confirm_password(),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::put_platform_authed_json_no_content(
                    "/platform/me/password",
                    &body,
                )
                .await
                {
                    Ok(()) => {
                        saved.set(true);
                        current_password.set(String::new());
                        new_password.set(String::new());
                        confirm_password.set(String::new());
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-6 p-6",
                div {
                    h2 { class: "text-base font-semibold text-content", "Password" }
                    p { class: "text-sm text-muted",
                        "Change the password for this platform-admin sign-in."
                    }
                }
                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                if saved() {
                    StatusBanner { tone: BannerTone::Success, "Password changed." }
                }
                div { class: "grid gap-4 sm:grid-cols-2",
                    Input {
                        name: "current_password",
                        label: "Current password",
                        r#type: "password".to_string(),
                        value: current_password(),
                        oninput: move |e: FormEvent| current_password.set(e.value()),
                    }
                    div {}
                    Input {
                        name: "new_password",
                        label: "New password",
                        r#type: "password".to_string(),
                        value: new_password(),
                        help: "At least 12 characters.".to_string(),
                        oninput: move |e: FormEvent| new_password.set(e.value()),
                    }
                    Input {
                        name: "confirm_password",
                        label: "Confirm new password",
                        r#type: "password".to_string(),
                        value: confirm_password(),
                        oninput: move |e: FormEvent| confirm_password.set(e.value()),
                    }
                }
                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: handle_save,
                        disabled: saving()
                            || !can_mutate
                            || current_password().is_empty()
                            || new_password().is_empty()
                            || confirm_password().is_empty(),
                        loading: saving(),
                        title: (!can_mutate).then(|| "Can't change password while the server is unreachable".to_string()),
                        "Change password"
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PlatformMfaCardProps {
    mfa_enabled: bool,
}

#[component]
fn PlatformMfaCard(props: PlatformMfaCardProps) -> Element {
    let mut enabled = use_signal(|| props.mfa_enabled);
    let mut show_setup = use_signal(|| false);
    let mut show_disable = use_signal(|| false);
    let can_mutate = crate::hooks::use_can_mutate();

    let on_enabled = move |_| {
        show_setup.set(false);
        enabled.set(true);
        crate::hooks::fetch::api::set_platform_mfa_enabled(true);
    };
    let on_disabled = move |_| {
        show_disable.set(false);
        enabled.set(false);
        crate::hooks::fetch::api::set_platform_mfa_enabled(false);
    };

    rsx! {
        Card {
            div { class: "flex items-center justify-between gap-4 p-6",
                div {
                    h2 { class: "text-base font-semibold text-content",
                        "Two-factor authentication"
                    }
                    p { class: "text-sm text-muted",
                        "A TOTP code from an authenticator app, held on this platform-admin account."
                    }
                    p { class: "mt-1 text-sm font-medium text-content",
                        if enabled() { "Enabled" } else { "Not enabled" }
                    }
                }
                if enabled() {
                    Button {
                        variant: ButtonVariant::Danger,
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't change MFA while the server is unreachable".to_string()),
                        onclick: move |_| show_disable.set(true),
                        "Disable"
                    }
                } else {
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't change MFA while the server is unreachable".to_string()),
                        onclick: move |_| show_setup.set(true),
                        "Set up"
                    }
                }
            }
        }
        PlatformMfaSetupModal {
            open: show_setup(),
            onclose: move |_| show_setup.set(false),
            onenabled: on_enabled,
        }
        PlatformMfaDisableModal {
            open: show_disable(),
            onclose: move |_| show_disable.set(false),
            ondisabled: on_disabled,
        }
    }
}

#[derive(Clone, PartialEq)]
enum SetupStep {
    /// Waiting on the current-password prompt before calling
    /// `POST /platform/me/mfa/setup`.
    EnterPassword,
    Loading,
    EnterCode {
        secret: String,
        provisioning_uri: String,
    },
    Recovery {
        codes: Vec<String>,
    },
    Error(String),
}

#[derive(Props, Clone, PartialEq)]
struct PlatformMfaSetupModalProps {
    open: bool,
    onclose: EventHandler<()>,
    onenabled: EventHandler<()>,
}

/// Two password prompts, matching the server's shape: `mfa/setup` and
/// `mfa/enable` each re-verify `current_password` independently
/// (`PlatformMfaSetupRequest` / `PlatformMfaEnableRequest`), so the
/// modal collects it once up front and resends it on the confirm step.
#[component]
fn PlatformMfaSetupModal(props: PlatformMfaSetupModalProps) -> Element {
    let mut step = use_signal(|| SetupStep::EnterPassword);
    let mut password = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    let open = props.open;
    use_effect(use_reactive!(|open| {
        if !open {
            return;
        }
        step.set(SetupStep::EnterPassword);
        password.set(String::new());
        code.set(String::new());
    }));

    if !props.open {
        return rsx! {};
    }

    let onclose = props.onclose;
    let onenabled = props.onenabled;

    let start_setup = move |_| {
        if submitting() || password().is_empty() {
            return;
        }
        submitting.set(true);
        step.set(SetupStep::Loading);
        let body = MfaSetupRequest {
            current_password: password(),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_platform_authed_typed::<MfaSetupResponse, _>(
                    "/platform/me/mfa/setup",
                    &body,
                )
                .await
                {
                    Ok(resp) => step.set(SetupStep::EnterCode {
                        secret: resp.secret,
                        provisioning_uri: resp.provisioning_uri,
                    }),
                    Err(e) => step.set(SetupStep::Error(e.user_message())),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = body;
            }
            submitting.set(false);
        });
    };

    let confirm = move |_| {
        if submitting() {
            return;
        }
        let SetupStep::EnterCode { .. } = step() else {
            return;
        };
        submitting.set(true);
        let body = MfaEnableRequest {
            current_password: password(),
            code: code(),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_platform_authed_typed::<MfaEnableResponse, _>(
                    "/platform/me/mfa/enable",
                    &body,
                )
                .await
                {
                    Ok(resp) => step.set(SetupStep::Recovery {
                        codes: resp.recovery_codes,
                    }),
                    Err(e) => step.set(SetupStep::Error(e.user_message())),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = body;
            }
            submitting.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Set up two-factor authentication".to_string(),
            onclose: move |_| onclose.call(()),
            footer: match step() {
                SetupStep::Recovery { .. } => rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| onenabled.call(()),
                        "Done"
                    }
                },
                SetupStep::EnterPassword => rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: submitting(),
                        disabled: password().is_empty(),
                        onclick: start_setup,
                        "Continue"
                    }
                },
                SetupStep::EnterCode { .. } => rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: submitting(),
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: submitting(),
                        disabled: code().trim().is_empty(),
                        onclick: confirm,
                        "Confirm"
                    }
                },
                _ => rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| onclose.call(()),
                        "Close"
                    }
                },
            },
            match step() {
                SetupStep::EnterPassword => rsx! {
                    div { class: "space-y-4",
                        p { class: "text-sm text-content",
                            "Confirm your password to start setup."
                        }
                        Input {
                            name: "mfa_setup_password",
                            label: "Current password",
                            r#type: "password".to_string(),
                            value: password(),
                            oninput: move |e: FormEvent| password.set(e.value()),
                        }
                    }
                },
                SetupStep::Loading => rsx! {
                    p { class: "text-sm text-muted", "Generating a secret\u{2026}" }
                },
                SetupStep::Error(msg) => rsx! {
                    ErrorBanner { "{msg}" }
                },
                SetupStep::EnterCode { secret, provisioning_uri } => rsx! {
                    div { class: "space-y-4",
                        p { class: "text-sm text-content",
                            "Scan this into your authenticator app, or enter the secret manually."
                        }
                        div { class: "rounded-md border border-line bg-surface-2 p-3 space-y-2",
                            p { class: "text-xs uppercase text-muted", "Secret" }
                            p { class: "font-mono text-sm text-content break-all", "{secret}" }
                            p { class: "text-xs uppercase text-muted", "Provisioning URI" }
                            p { class: "font-mono text-xs text-content break-all", "{provisioning_uri}" }
                        }
                        Input {
                            name: "mfa_code",
                            label: "Code from your authenticator app",
                            value: code(),
                            oninput: move |e: FormEvent| code.set(e.value()),
                        }
                    }
                },
                SetupStep::Recovery { codes } => rsx! {
                    div { class: "space-y-3",
                        StatusBanner { tone: BannerTone::Success, "Two-factor authentication is enabled." }
                        p { class: "text-sm text-content",
                            "Save these recovery codes now. Each works once, and they will not be shown again."
                        }
                        div { class: "rounded-md border border-line bg-surface-2 p-3 font-mono text-sm text-content grid grid-cols-2 gap-2",
                            for recovery_code in codes {
                                span { "{recovery_code}" }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PlatformMfaDisableModalProps {
    open: bool,
    onclose: EventHandler<()>,
    ondisabled: EventHandler<()>,
}

#[component]
fn PlatformMfaDisableModal(props: PlatformMfaDisableModalProps) -> Element {
    let mut password = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    let open = props.open;
    use_effect(use_reactive!(|open| {
        if open {
            password.set(String::new());
            code.set(String::new());
            error.set(String::new());
        }
    }));

    if !props.open {
        return rsx! {};
    }

    let onclose = props.onclose;
    let ondisabled = props.ondisabled;

    let confirm = move |_| {
        if submitting() || password().is_empty() || code().trim().is_empty() {
            return;
        }
        submitting.set(true);
        error.set(String::new());
        let body = MfaDisableRequest {
            current_password: password(),
            code: code(),
        };
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_platform_authed_json_no_content(
                    "/platform/me/mfa/disable",
                    &body,
                )
                .await
                {
                    Ok(()) => ondisabled.call(()),
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = body;
            }
            submitting.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Disable two-factor authentication".to_string(),
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    disabled: submitting(),
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Danger,
                    loading: submitting(),
                    disabled: password().is_empty() || code().trim().is_empty(),
                    onclick: confirm,
                    "Disable"
                }
            },
            div { class: "space-y-3",
                p { class: "text-sm text-content",
                    "Confirm your password and a live code (or a recovery code) to turn off two-factor authentication."
                }
                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                Input {
                    name: "mfa_disable_password",
                    label: "Current password",
                    r#type: "password".to_string(),
                    value: password(),
                    oninput: move |e: FormEvent| password.set(e.value()),
                }
                Input {
                    name: "mfa_disable_code",
                    label: "Authenticator code or recovery code",
                    value: code(),
                    oninput: move |e: FormEvent| code.set(e.value()),
                }
            }
        }
    }
}

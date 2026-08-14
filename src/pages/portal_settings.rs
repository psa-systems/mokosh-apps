//! PMS-729 follow-up: `/portal/settings`, the customer's own account page.
//!
//! Landing spot from the user menu's "Settings" link. Renders:
//! - profile (name/email, read-only for now);
//! - change-password form -> `PUT /portal/auth/me/password`;
//! - MFA enroll / enable / disable (H4) -> `POST /portal/auth/me/mfa/{setup,enable,disable}`;
//! - active sessions list + per-session revoke (H6) -> `GET|DELETE /portal/auth/me/sessions`.
//!
//! Every mutating card is self-contained so a failed dispatch on one
//! (e.g. wrong MFA code) leaves the others intact; there is no shared
//! form-error signal to accidentally overwrite.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

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

    rsx! {
        crate::components::PortalLayout { title: "Settings".to_string(),
            div { class: "space-y-6",
                ProfileCard { me: me.clone() }

                ChangePasswordCard {}

                MfaCard { mfa_enabled: me.as_ref().map(|m| m.mfa_enabled).unwrap_or(false) }

                NotificationPreferencesCard {}

                SessionsCard {}
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProfileCardProps {
    me: Option<crate::hooks::portal_me::PortalMe>,
}

/// Profile card: self-edit of first / last name via
/// PATCH /portal/auth/me. Email stays read-only (identity is keyed
/// on it, and changing it is an agent-side workflow).
#[component]
fn ProfileCard(props: ProfileCardProps) -> Element {
    let mut editing = use_signal(|| false);
    let mut first = use_signal(String::new);
    let mut last = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Seed the inputs from `me` the first time it becomes available;
    // avoids clobbering the customer's mid-edit typing on a refetch.
    use_effect({
        let me_snap = props.me.clone();
        move || {
            if !editing() {
                if let Some(m) = me_snap.as_ref() {
                    if first.peek().is_empty() {
                        first.set(m.first_name.clone());
                    }
                    if last.peek().is_empty() {
                        last.set(m.last_name.clone());
                    }
                }
            }
        }
    });

    let mut save = move |_| {
        if saving() {
            return;
        }
        let f = first.read().trim().to_string();
        let l = last.read().trim().to_string();
        if f.is_empty() || l.is_empty() {
            error.set("First and last name are required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let body = serde_json::json!({ "first_name": f, "last_name": l });
                match crate::hooks::fetch::api::patch_portal_authed_json_no_content(
                    "/portal/auth/me",
                    &body,
                )
                .await
                {
                    Ok(()) => {
                        editing.set(false);
                        crate::hooks::portal_me::invalidate_portal_me();
                    }
                    Err(ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (f, l);
            }
            saving.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "flex items-center justify-between mb-3",
                h2 { class: "text-lg font-semibold text-content", "Profile" }
                if !editing() && props.me.is_some() {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            error.set(String::new());
                            editing.set(true);
                        },
                        "Edit"
                    }
                }
            }
            match (&props.me, editing()) {
                (Some(m), false) => rsx! {
                    dl { class: "grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-2 text-sm",
                        dt { class: "text-muted", "Name" }
                        dd { class: "text-content", "{m.display_name()}" }
                        dt { class: "text-muted", "Email" }
                        dd { class: "text-content", "{m.email}" }
                    }
                    p { class: "mt-3 text-xs text-muted",
                        "Email changes are handled by your account team. Contact them if it needs to change."
                    }
                },
                (Some(_), true) => rsx! {
                    form {
                        class: "space-y-3",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            save(());
                        },
                        Input {
                            name: "profile_first_name",
                            label: "First name",
                            value: first(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                first.set(e.value());
                            },
                        }
                        Input {
                            name: "profile_last_name",
                            label: "Last name",
                            value: last(),
                            required: true,
                            disabled: saving(),
                            oninput: move |e: FormEvent| {
                                error.set(String::new());
                                last.set(e.value());
                            },
                        }
                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                        }
                        div { class: "flex gap-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                r#type: "submit".to_string(),
                                loading: saving(),
                                disabled: saving(),
                                "Save"
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: saving(),
                                onclick: move |_| {
                                    editing.set(false);
                                    error.set(String::new());
                                },
                                "Cancel"
                            }
                        }
                    }
                },
                (None, _) => rsx! {
                    p { class: "text-sm text-muted", "Loading your profile..." }
                },
            }
        }
    }
}

#[component]
fn ChangePasswordCard() -> Element {
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

// ---- MFA (PMS-729 phase 2 H4) -------------------------------------------

/// Server payload from `POST /portal/auth/me/mfa/setup`.
#[derive(Clone, PartialEq, Deserialize)]
struct MfaSetupResponse {
    #[serde(default)]
    secret: String,
    #[serde(default)]
    provisioning_uri: String,
}

/// Server payload from `POST /portal/auth/me/mfa/enable`.
#[derive(Clone, PartialEq, Deserialize)]
struct MfaEnableResponse {
    #[serde(default)]
    recovery_codes: Vec<String>,
}

/// Local state machine for the MFA card. Cheap to represent inline;
/// no ambient store because the flow is confined to one card.
#[derive(Clone, PartialEq)]
enum MfaFlow {
    /// Enabled or disabled: idle. Buttons render "Set up two-factor
    /// auth" or "Disable two-factor auth" from `mfa_enabled` on the
    /// enclosing card.
    Idle,
    /// Post-`setup`, pre-`enable`: show the secret + code input.
    Enrolling {
        secret: String,
        provisioning_uri: String,
    },
    /// Post-`enable`: show the recovery codes exactly once.
    RecoveryCodes { codes: Vec<String> },
    /// Post-`disable`: hidden banner just to confirm the flip.
    Disabled,
}

#[derive(Props, Clone, PartialEq)]
struct MfaCardProps {
    mfa_enabled: bool,
}

#[component]
fn MfaCard(props: MfaCardProps) -> Element {
    let mut flow = use_signal(|| MfaFlow::Idle);
    let mut password = use_signal(String::new);
    let mut code = use_signal(String::new);
    let mut working = use_signal(|| false);
    let mut error = use_signal(String::new);
    let enabled = props.mfa_enabled;

    // "Start enrollment" - POST /mfa/setup.
    let mut handle_setup = move |_| {
        if working() {
            return;
        }
        let pw = password.read().clone();
        if pw.is_empty() {
            error.set("Enter your current password to start enrollment.".to_string());
            return;
        }
        working.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::{post_portal_authed_typed, ApiError};
                match post_portal_authed_typed::<MfaSetupResponse, _>(
                    "/portal/auth/me/mfa/setup",
                    &serde_json::json!({ "current_password": pw }),
                )
                .await
                {
                    Ok(resp) => {
                        password.set(String::new());
                        flow.set(MfaFlow::Enrolling {
                            secret: resp.secret,
                            provisioning_uri: resp.provisioning_uri,
                        });
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set("That password is not correct.".to_string());
                    }
                    Err(ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = pw;
            }
            working.set(false);
        });
    };

    // "Confirm enrollment" - POST /mfa/enable.
    let mut handle_enable = move |_| {
        if working() {
            return;
        }
        let pw = password.read().clone();
        let c = code.read().trim().to_string();
        if pw.is_empty() {
            error.set("Enter your current password to confirm.".to_string());
            return;
        }
        if c.is_empty() {
            error.set("Enter the 6-digit code from your authenticator app.".to_string());
            return;
        }
        working.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::{post_portal_authed_typed, ApiError};
                match post_portal_authed_typed::<MfaEnableResponse, _>(
                    "/portal/auth/me/mfa/enable",
                    &serde_json::json!({ "current_password": pw, "code": c }),
                )
                .await
                {
                    Ok(resp) => {
                        password.set(String::new());
                        code.set(String::new());
                        flow.set(MfaFlow::RecoveryCodes {
                            codes: resp.recovery_codes,
                        });
                        crate::hooks::portal_me::invalidate_portal_me();
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set(
                            "Password or code was not accepted. Double-check both and try again."
                                .to_string(),
                        );
                    }
                    Err(ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (pw, c);
            }
            working.set(false);
        });
    };

    // "Disable" - POST /mfa/disable.
    let mut handle_disable = move |_| {
        if working() {
            return;
        }
        let pw = password.read().clone();
        let c = code.read().trim().to_string();
        if pw.is_empty() || c.is_empty() {
            error.set(
                "Enter your current password AND a fresh 6-digit code to disable.".to_string(),
            );
            return;
        }
        working.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::{post_portal_authed_typed, ApiError};
                match post_portal_authed_typed::<serde_json::Value, _>(
                    "/portal/auth/me/mfa/disable",
                    &serde_json::json!({ "current_password": pw, "code": c }),
                )
                .await
                {
                    Ok(_) => {
                        password.set(String::new());
                        code.set(String::new());
                        flow.set(MfaFlow::Disabled);
                        crate::hooks::portal_me::invalidate_portal_me();
                    }
                    Err(ApiError::Status { code: 401, .. }) => {
                        error.set(
                            "Password or code was not accepted. Two-factor auth stays on."
                                .to_string(),
                        );
                    }
                    Err(ApiError::Status { message, .. }) if !message.is_empty() => {
                        error.set(message);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (pw, c);
            }
            working.set(false);
        });
    };

    let mut clear_error = move |_: FormEvent| {
        error.set(String::new());
    };

    rsx! {
        Card {
            h2 { class: "text-lg font-semibold text-content mb-1", "Two-factor auth" }

            match flow() {
                MfaFlow::Enrolling { secret, provisioning_uri } => rsx! {
                    p { class: "text-xs text-muted mb-3",
                        "Scan this in your authenticator app (1Password, Authy, Google Authenticator, ...) or enter the secret manually. Then type the 6-digit code + your current password to finish."
                    }
                    div { class: "mb-3 rounded bg-surface-2 p-3 text-sm break-all",
                        div { class: "text-xs text-muted mb-1", "Secret" }
                        code { class: "font-mono text-content", "{secret}" }
                    }
                    details { class: "mb-4",
                        summary { class: "text-xs text-muted cursor-pointer", "Show provisioning URI (for manual entry)" }
                        code { class: "block mt-2 text-xs font-mono text-muted break-all", "{provisioning_uri}" }
                    }
                    form {
                        class: "space-y-3",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_enable(());
                        },
                        Input {
                            name: "mfa_setup_password",
                            label: "Current password",
                            r#type: "password".to_string(),
                            value: password(),
                            required: true,
                            disabled: working(),
                            oninput: move |e: FormEvent| {
                                clear_error(e.clone());
                                password.set(e.value());
                            },
                        }
                        Input {
                            name: "mfa_code",
                            label: "6-digit code",
                            value: code(),
                            required: true,
                            disabled: working(),
                            oninput: move |e: FormEvent| {
                                clear_error(e.clone());
                                code.set(e.value());
                            },
                        }
                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                        }
                        div { class: "flex gap-2 pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: working(),
                                disabled: working(),
                                r#type: "submit".to_string(),
                                "Enable two-factor auth"
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: working(),
                                onclick: move |_| {
                                    password.set(String::new());
                                    code.set(String::new());
                                    error.set(String::new());
                                    flow.set(MfaFlow::Idle);
                                },
                                "Cancel"
                            }
                        }
                    }
                },
                MfaFlow::RecoveryCodes { codes } => rsx! {
                    p { class: "text-sm text-green-700 dark:text-green-400 mb-3", role: "status",
                        "Two-factor auth is on. Save these recovery codes now - each one lets you sign in once if you lose your authenticator device. We will not show them again."
                    }
                    ul { class: "mb-4 rounded bg-surface-2 p-3 font-mono text-sm text-content grid grid-cols-2 sm:grid-cols-2 gap-x-4 gap-y-1",
                        for c in codes.iter() {
                            li { key: "{c}", "{c}" }
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| flow.set(MfaFlow::Idle),
                        "I have saved these codes"
                    }
                },
                MfaFlow::Disabled => rsx! {
                    p { class: "text-sm text-content", "Two-factor auth is now off." }
                    div { class: "mt-3",
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| flow.set(MfaFlow::Idle),
                            "Close"
                        }
                    }
                },
                MfaFlow::Idle if enabled => rsx! {
                    p { class: "text-xs text-muted mb-3",
                        "Two-factor auth is on. To turn it off, enter your current password and a fresh 6-digit code."
                    }
                    form {
                        class: "space-y-3",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_disable(());
                        },
                        Input {
                            name: "mfa_disable_password",
                            label: "Current password",
                            r#type: "password".to_string(),
                            value: password(),
                            required: true,
                            disabled: working(),
                            oninput: move |e: FormEvent| {
                                clear_error(e.clone());
                                password.set(e.value());
                            },
                        }
                        Input {
                            name: "mfa_disable_code",
                            label: "6-digit code",
                            value: code(),
                            required: true,
                            disabled: working(),
                            oninput: move |e: FormEvent| {
                                clear_error(e.clone());
                                code.set(e.value());
                            },
                        }
                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                        }
                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Danger,
                                loading: working(),
                                disabled: working(),
                                r#type: "submit".to_string(),
                                "Disable two-factor auth"
                            }
                        }
                    }
                },
                MfaFlow::Idle => rsx! {
                    p { class: "text-xs text-muted mb-3",
                        "Two-factor auth is off. Add it to protect your portal account from a stolen password."
                    }
                    form {
                        class: "space-y-3",
                        onsubmit: move |evt: Event<FormData>| {
                            evt.prevent_default();
                            handle_setup(());
                        },
                        Input {
                            name: "mfa_setup_password_initial",
                            label: "Current password",
                            r#type: "password".to_string(),
                            value: password(),
                            required: true,
                            disabled: working(),
                            oninput: move |e: FormEvent| {
                                clear_error(e.clone());
                                password.set(e.value());
                            },
                        }
                        if !error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", role: "alert", "{error}" }
                        }
                        div { class: "pt-2",
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: working(),
                                disabled: working(),
                                r#type: "submit".to_string(),
                                "Set up two-factor auth"
                            }
                        }
                    }
                },
            }
        }
    }
}

// ---- Notification preferences -------------------------------------------

#[derive(Clone, PartialEq, Deserialize)]
struct NotificationEventOption {
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    channels: Vec<String>,
}

#[derive(Clone, PartialEq, Deserialize)]
struct NotificationPreference {
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    is_enabled: bool,
    #[serde(default)]
    channel_types: Vec<String>,
}

#[derive(Clone, PartialEq, Deserialize, Default)]
struct NotificationPreferencesPayload {
    #[serde(default)]
    available: Vec<NotificationEventOption>,
    #[serde(default)]
    preferences: Vec<NotificationPreference>,
}

/// Humanize a `ticket.note_added`-style event token into "Ticket note
/// added". Splits on `.` and `_`, title-cases each word.
fn humanize_event(token: &str) -> String {
    token
        .split(|c: char| c == '.' || c == '_')
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, word)| {
            let mut chars = word.chars();
            let first: String = chars
                .next()
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_default();
            if i == 0 {
                format!("{first}{}", chars.as_str())
            } else {
                format!(" {first}{}", chars.as_str())
            }
        })
        .collect()
}

#[component]
fn NotificationPreferencesCard() -> Element {
    let mut version = use_signal(|| 0u32);
    let mut error = use_signal(String::new);

    let payload = use_resource(use_reactive!(|version| async move {
        let _v = version.read();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_portal_authed::<NotificationPreferencesPayload>(
                "/portal/auth/me/notification-preferences",
            )
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<NotificationPreferencesPayload>
        }
    }));

    let snap = payload.read_unchecked();
    let data: NotificationPreferencesPayload = match &*snap {
        Some(Some(p)) => p.clone(),
        _ => NotificationPreferencesPayload::default(),
    };
    // Fast lookup: event_type -> is_enabled (true when the caller
    // has no explicit preference row, matching the server's
    // accept-all default).
    let enabled_map: std::collections::HashMap<String, bool> = data
        .preferences
        .iter()
        .map(|p| (p.event_type.clone(), p.is_enabled))
        .collect();

    let mut toggle = move |event_type: String, next: bool| {
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({
                    "event_type": event_type,
                    "is_enabled": next,
                    "channel_types": [],
                });
                match crate::hooks::fetch::api::put_portal_authed_json_no_content(
                    "/portal/auth/me/notification-preferences",
                    &body,
                )
                .await
                {
                    Ok(()) => {
                        let n = *version.peek();
                        version.set(n + 1);
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (event_type, next);
            }
        });
    };

    rsx! {
        Card {
            h2 { class: "text-lg font-semibold text-content mb-1", "Notification preferences" }
            p { class: "text-xs text-muted mb-4",
                "Choose which events you want us to notify you about. Turning an event off suppresses email and in-app notifications for it; it will still be recorded in your account."
            }
            if !error().is_empty() {
                p { class: "text-sm text-red-600 dark:text-red-400 mb-3", role: "alert", "{error}" }
            }
            match &*snap {
                None => rsx! {
                    p { class: "text-sm text-muted", "Loading preferences..." }
                },
                Some(None) => rsx! {
                    p { class: "text-sm text-red-600 dark:text-red-400",
                        "Could not load your notification preferences. Refresh in a moment."
                    }
                },
                Some(Some(_)) if data.available.is_empty() => rsx! {
                    p { class: "text-sm text-muted",
                        "Your account team has not configured any notifications yet."
                    }
                },
                Some(Some(_)) => rsx! {
                    ul { class: "divide-y divide-line",
                        for opt in data.available.iter().cloned() {
                            {
                                let label = humanize_event(&opt.event_type);
                                let channels = opt.channels.join(", ");
                                let is_on = enabled_map.get(&opt.event_type).copied().unwrap_or(true);
                                let ev_for_click = opt.event_type.clone();
                                rsx! {
                                    li { key: "{opt.event_type}",
                                        class: "py-3 flex items-center justify-between gap-4",
                                        div { class: "min-w-0 flex-1",
                                            p { class: "text-sm font-medium text-content", "{label}" }
                                            if !channels.is_empty() {
                                                p { class: "text-xs text-muted", "Via: {channels}" }
                                            }
                                        }
                                        label { class: "inline-flex items-center gap-2 text-sm",
                                            input {
                                                r#type: "checkbox",
                                                checked: is_on,
                                                onchange: move |_| {
                                                    let next = !is_on;
                                                    toggle(ev_for_click.clone(), next);
                                                },
                                            }
                                            if is_on { span { class: "text-muted", "On" } } else { span { class: "text-muted", "Off" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

// ---- Sessions (PMS-729 phase 2 H6) --------------------------------------

/// One row from `GET /portal/auth/me/sessions`. Serde-lax so a future
/// server-side addition (user_agent tokens, ip_address geoip) does not
/// break decode.
#[derive(Clone, PartialEq, Deserialize)]
struct PortalSession {
    id: uuid::Uuid,
    #[serde(default)]
    issued_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    ip_address: Option<String>,
    #[serde(default)]
    current: bool,
}

#[component]
fn SessionsCard() -> Element {
    let mut version = use_signal(|| 0u32);
    let mut error = use_signal(String::new);

    let sessions = use_resource(use_reactive!(|version| async move {
        let _v = version.read();
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_portal_authed::<Vec<PortalSession>>(
                "/portal/auth/me/sessions",
            )
            .await
            .ok()
        }
        #[cfg(not(feature = "web"))]
        {
            None::<Vec<PortalSession>>
        }
    }));

    let snap = sessions.read_unchecked();

    let mut revoke_session = move |sid: uuid::Uuid| {
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::delete_portal_authed_no_content(&format!(
                    "/portal/auth/me/sessions/{sid}"
                ))
                .await
                {
                    Ok(()) => {
                        let n = *version.peek();
                        version.set(n + 1);
                    }
                    Err(msg) => error.set(msg),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = sid;
            }
        });
    };

    rsx! {
        Card {
            h2 { class: "text-lg font-semibold text-content mb-1", "Active sessions" }
            p { class: "text-xs text-muted mb-4",
                "Devices currently signed in to your portal account. Sign out anywhere you don't recognise. To sign out this browser, use the Sign out link in the top-right menu."
            }
            if !error().is_empty() {
                p { class: "text-sm text-red-600 dark:text-red-400 mb-3", role: "alert", "{error}" }
            }
            match &*snap {
                None => rsx! {
                    p { class: "text-sm text-muted", "Loading sessions..." }
                },
                Some(None) => rsx! {
                    p { class: "text-sm text-red-600 dark:text-red-400",
                        "Could not load your sessions. Refresh in a moment."
                    }
                },
                Some(Some(rows)) if rows.is_empty() => rsx! {
                    p { class: "text-sm text-muted", "No active sessions." }
                },
                Some(Some(rows)) => rsx! {
                    ul { class: "divide-y divide-line",
                        for row in rows.iter().cloned() {
                            SessionRow {
                                key: "{row.id}",
                                session: row.clone(),
                                on_revoke: move |sid| revoke_session(sid),
                            }
                        }
                    }
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SessionRowProps {
    session: PortalSession,
    on_revoke: EventHandler<uuid::Uuid>,
}

#[component]
fn SessionRow(props: SessionRowProps) -> Element {
    let s = props.session;
    let ua = s
        .user_agent
        .clone()
        .unwrap_or_else(|| "Unknown device".to_string());
    let ip = s.ip_address.clone().unwrap_or_default();
    let issued = s
        .issued_at
        .map(|d| crate::utils::datetime::format_user_datetime(d, None))
        .unwrap_or_default();
    let expires = s
        .expires_at
        .map(|d| crate::utils::datetime::format_user_datetime(d, None))
        .unwrap_or_default();
    let sid = s.id;

    rsx! {
        li { class: "py-3 flex items-start justify-between gap-4",
            div { class: "min-w-0 flex-1",
                p { class: "text-sm text-content truncate",
                    "{ua}"
                    if s.current {
                        span { class: "ml-2 text-xs uppercase tracking-wide text-accent",
                            "This browser"
                        }
                    }
                }
                p { class: "mt-1 text-xs text-muted",
                    if !ip.is_empty() { "{ip} - " }
                    if !issued.is_empty() { "signed in {issued}" }
                    if !expires.is_empty() { " - expires {expires}" }
                }
            }
            if !s.current {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| props.on_revoke.call(sid),
                    "Sign out"
                }
            }
        }
    }
}

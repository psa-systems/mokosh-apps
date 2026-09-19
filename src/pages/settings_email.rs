//! Settings > Email (MAPPS-887).
//!
//! Deployment-wide SMTP configuration, mokosh-server PMS-638/788/1013:
//! `GET`/`PUT /settings/email`, `POST /settings/email/test-send`, and
//! `POST /settings/email/verify`. Like App name (MAPPS-885), this is a
//! system-tenant setting effective for every tenant on the deployment,
//! not a per-tenant one, so it gets its own page rather than joining
//! Organization.
//!
//! `EmailSettingsBody`'s PUT always sends every field it currently holds
//! (mirroring the Organization profile body), so a blank text field clears
//! that override back to the deployment's environment default - the same
//! "`None` keeps, explicit `""` clears" contract mokosh-server's
//! `put_email_settings` documents. The password is write-only and never
//! round-tripped: leaving it blank keeps the current one, and the "Clear
//! stored password" checkbox is the only way to send the explicit empty
//! string that removes it.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Button, ButtonVariant, Card, Checkbox, ErrorBanner, Input, PageHeader, Select,
    SelectOption,
};
use crate::pages::settings::{AdminOnlyNotice, SettingsBreadcrumb};
use crate::Route;

const EMAIL_PATH: &str = "/settings/email";
const TEST_SEND_PATH: &str = "/settings/email/test-send";
const VERIFY_PATH: &str = "/settings/email/verify";

/// `GET`/`PUT /settings/email` response, for mokosh-server's
/// `EmailSettingsView`. The password is never returned, only whether one is
/// set.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct EmailSettingsView {
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    tls: Option<String>,
    #[serde(default)]
    password_set: bool,
}

/// `PUT /settings/email` body, for mokosh-server's `EmailSettingsInput`.
#[derive(Debug, Serialize)]
struct EmailSettingsBody {
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    from: Option<String>,
    tls: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

/// `POST /settings/email/test-send` body, for mokosh-server's
/// `TestEmailRequest`.
#[derive(Debug, Serialize)]
struct TestSendBody {
    to: String,
}

/// The closed set `SmtpTls::parse` accepts, canonical value first.
const TLS_MODES: &[(&str, &str)] = &[
    ("starttls", "STARTTLS (port 587, default)"),
    ("implicit", "Implicit TLS (port 465)"),
    ("none", "None (local dev only)"),
];

const DEFAULT_TLS: &str = "starttls";

#[component]
pub fn EmailSettingsPage() -> Element {
    use_page_title("Email");
    if !crate::pages::settings::use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Email" } };
    }
    rsx! { EmailSettingsForm {} }
}

#[component]
fn EmailSettingsForm() -> Element {
    let mut host = use_signal(String::new);
    let mut port = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut from = use_signal(String::new);
    let mut tls = use_signal(|| DEFAULT_TLS.to_string());
    let mut password = use_signal(String::new);
    let mut clear_password = use_signal(|| false);
    let mut password_set = use_signal(|| false);
    let mut seeded = use_signal(|| false);

    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut port_error = use_signal(String::new);

    let mut test_to = use_signal(String::new);
    let mut test_sending = use_signal(|| false);
    let mut test_error = use_signal(String::new);

    let mut verifying = use_signal(|| false);
    let mut verify_error = use_signal(String::new);

    let can_mutate = crate::hooks::use_can_mutate();

    let settings_resource = use_resource(move || async move {
        let _reachable = crate::hooks::use_server_reachable();
        crate::pages::settings::load_operator_setting::<EmailSettingsView>(
            EMAIL_PATH,
            "email settings",
        )
        .await
    });
    let snap = settings_resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(crate::pages::settings::OperatorLoad::Failed));
    // PMS-1280: the relay belongs to the deployment's operator.
    let not_operator = matches!(
        *snap,
        Some(crate::pages::settings::OperatorLoad::NotOperator)
    );
    if !seeded() {
        if let Some(crate::pages::settings::OperatorLoad::Loaded(view)) = &*snap {
            host.set(view.host.clone().unwrap_or_default());
            port.set(view.port.map(|p| p.to_string()).unwrap_or_default());
            username.set(view.username.clone().unwrap_or_default());
            from.set(view.from.clone().unwrap_or_default());
            tls.set(view.tls.clone().unwrap_or_else(|| DEFAULT_TLS.to_string()));
            password_set.set(view.password_set);
            seeded.set(true);
        }
    }

    let handle_save = move |_| {
        if saving() {
            return;
        }
        error.set(String::new());
        port_error.set(String::new());

        let parsed_port = {
            let raw = port.read().trim().to_string();
            if raw.is_empty() {
                None
            } else {
                match raw.parse::<u16>() {
                    Ok(p) => Some(p),
                    Err(_) => {
                        port_error.set(
                            "Enter a port between 1 and 65535, or leave it blank.".to_string(),
                        );
                        return;
                    }
                }
            }
        };

        let body = EmailSettingsBody {
            host: Some(host.read().trim().to_string()),
            port: parsed_port,
            username: Some(username.read().trim().to_string()),
            from: Some(from.read().trim().to_string()),
            tls: Some(tls.read().clone()),
            password: if clear_password() {
                Some(String::new())
            } else {
                let raw = password.read().clone();
                if raw.is_empty() {
                    None
                } else {
                    Some(raw)
                }
            },
        };

        saving.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<EmailSettingsView, _>(
                    EMAIL_PATH, &body,
                )
                .await
                {
                    Ok(saved) => {
                        host.set(saved.host.clone().unwrap_or_default());
                        port.set(saved.port.map(|p| p.to_string()).unwrap_or_default());
                        username.set(saved.username.clone().unwrap_or_default());
                        from.set(saved.from.clone().unwrap_or_default());
                        tls.set(saved.tls.clone().unwrap_or_else(|| DEFAULT_TLS.to_string()));
                        password_set.set(saved.password_set);
                        password.set(String::new());
                        clear_password.set(false);
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Email settings saved.",
                        );
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    };

    let handle_test_send = move |_| {
        if test_sending() {
            return;
        }
        test_error.set(String::new());
        let to = test_to.read().trim().to_string();
        if to.is_empty() {
            test_error.set("Enter an address to send the test to.".to_string());
            return;
        }
        test_sending.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed_json_no_content(
                    TEST_SEND_PATH,
                    &TestSendBody { to },
                )
                .await
                {
                    Ok(()) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Test email sent.",
                        );
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        test_error.set(err.user_message());
                    }
                }
            }
            test_sending.set(false);
        });
    };

    let handle_verify = move |_| {
        if verifying() {
            return;
        }
        verify_error.set(String::new());
        verifying.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed_json_no_content(
                    VERIFY_PATH,
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(()) => {
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Mailer verified.",
                        );
                    }
                    Err(err) => {
                        crate::hooks::push_api_error(&err);
                        verify_error.set(err.user_message());
                    }
                }
            }
            verifying.set(false);
        });
    };

    let password_placeholder = if password_set() {
        "Set. Leave blank to keep it.".to_string()
    } else {
        "Not set".to_string()
    };

    rsx! {
        PageHeader {
            title: "Email",
            subtitle: "The SMTP server this deployment sends mail through. Applies to every tenant; a field left blank falls back to the deployment's environment configuration.",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsEmail {} }
            },
        }

        if fetch_failed {
            Card {
                div { class: "p-6", ErrorBanner { "Could not load the email settings." } }
            }
        }
        if not_operator {
            Card {
                p { class: "text-sm text-muted",
                    "Email for this deployment is set up by whoever runs it. Every organisation on the deployment sends through it, so only its operator can see or change these settings."
                }
            }
        } else {

        Card {
            div { class: "space-y-4 max-w-xl",
                if !error().is_empty() {
                    ErrorBanner { "{error()}" }
                }
                Input {
                    name: "smtp_host",
                    label: "SMTP host",
                    value: host(),
                    disabled: is_loading || saving(),
                    placeholder: "smtp.example.com",
                    oninput: move |e: FormEvent| host.set(e.value()),
                }
                Input {
                    name: "smtp_port",
                    label: "Port",
                    r#type: "number".to_string(),
                    value: port(),
                    disabled: is_loading || saving(),
                    error: port_error(),
                    placeholder: "587",
                    oninput: move |e: FormEvent| {
                        port_error.set(String::new());
                        port.set(e.value());
                    },
                }
                Select {
                    name: "smtp_tls",
                    label: "TLS",
                    value: tls(),
                    disabled: is_loading || saving(),
                    options: TLS_MODES
                        .iter()
                        .map(|(v, l)| SelectOption::new(*v, *l))
                        .collect::<Vec<_>>(),
                    onchange: move |e: FormEvent| tls.set(e.value()),
                }
                Input {
                    name: "smtp_username",
                    label: "Username",
                    value: username(),
                    disabled: is_loading || saving(),
                    oninput: move |e: FormEvent| username.set(e.value()),
                }
                Input {
                    name: "smtp_password",
                    label: "Password",
                    r#type: "password".to_string(),
                    value: password(),
                    disabled: is_loading || saving() || clear_password(),
                    placeholder: password_placeholder,
                    help: "Write-only: never shown once saved. Leave blank to keep the current password."
                        .to_string(),
                    oninput: move |e: FormEvent| password.set(e.value()),
                }
                if password_set() {
                    Checkbox {
                        name: "smtp_password_clear",
                        label: "Clear the stored password",
                        checked: clear_password(),
                        disabled: is_loading || saving(),
                        onchange: move |e: FormEvent| {
                            let checked = e.value() == "true";
                            clear_password.set(checked);
                            if checked {
                                password.set(String::new());
                            }
                        },
                    }
                }
                Input {
                    name: "smtp_from",
                    label: "From address",
                    value: from(),
                    disabled: is_loading || saving(),
                    placeholder: "no-reply@example.com",
                    oninput: move |e: FormEvent| from.set(e.value()),
                }
                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: saving(),
                        disabled: !can_mutate || is_loading,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: handle_save,
                        "Save Changes"
                    }
                }
            }
        }

        Card {
            div { class: "space-y-4 max-w-xl",
                h2 { class: "text-lg font-semibold text-content", "Send a test email" }
                if !test_error().is_empty() {
                    ErrorBanner { "{test_error()}" }
                }
                Input {
                    name: "test_email_to",
                    label: "Send to",
                    value: test_to(),
                    disabled: test_sending(),
                    placeholder: "you@example.com",
                    oninput: move |e: FormEvent| test_to.set(e.value()),
                }
                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: test_sending(),
                        disabled: !can_mutate,
                        onclick: handle_test_send,
                        "Send Test Email"
                    }
                }
            }
        }

        Card {
            div { class: "space-y-4 max-w-xl",
                h2 { class: "text-lg font-semibold text-content", "Verify" }
                p { class: "text-sm text-muted",
                    "Check the mailer's connection without sending anything."
                }
                if !verify_error().is_empty() {
                    ErrorBanner { "{verify_error()}" }
                }
                div { class: "flex justify-end",
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: verifying(),
                        disabled: !can_mutate,
                        onclick: handle_verify,
                        "Verify"
                    }
                }
            }
        }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The page is admin only and calls all three routes MAPPS-887 names.
    #[test]
    fn the_page_is_admin_only_and_calls_the_three_email_routes() {
        let src = include_str!("settings_email.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(head.contains("if !crate::pages::settings::use_is_admin() {"));
        assert!(head.contains(r#"const EMAIL_PATH: &str = "/settings/email";"#));
        assert!(head.contains(r#"const TEST_SEND_PATH: &str = "/settings/email/test-send";"#));
        assert!(head.contains(r#"const VERIFY_PATH: &str = "/settings/email/verify";"#));
        assert!(head.contains(
            "put_authed_typed::<EmailSettingsView, _>(\n                    EMAIL_PATH, &body,"
        ));
        assert!(head.contains("post_authed_json_no_content(\n                    TEST_SEND_PATH,"));
        assert!(head.contains("post_authed_json_no_content(\n                    VERIFY_PATH,"));
    }
}

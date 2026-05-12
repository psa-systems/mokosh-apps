//! Authentication pages (login, signup, password reset)

use dioxus::prelude::*;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::hooks::{
    use_auth, use_google_login, use_login_form_with_return_to, GoogleLoginStatus,
};
use crate::Route;

/// Pluck `details.<field>` out of a backend error body shaped like
/// `{"error":"invalid_request","details":{"field":"message"}}`.
/// Returns the message verbatim so the SPA can surface it inline.
fn parse_first_field_error(raw: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    if v.get("error")?.as_str()? != "invalid_request" {
        return None;
    }
    let details = v.get("details")?.as_object()?;
    let (_, value) = details.iter().next()?;
    Some(value.as_str()?.to_string())
}

/// Read `?return_to=...` from the current URL and return it only if it
/// passes the strict open-redirect guard (must be the serialized
/// authorize query produced by mokosh-auth-http; anything else is
/// rejected). Returns `None` if absent or invalid.
fn read_safe_return_to() -> Option<String> {
    #[cfg(feature = "web")]
    {
        let win = web_sys::window()?;
        let search = win.location().search().ok()?;
        let trimmed = search.strip_prefix('?').unwrap_or(&search);
        for pair in trimmed.split('&') {
            let mut it = pair.splitn(2, '=');
            if it.next()? == "return_to" {
                let raw = it.next().unwrap_or("");
                let decoded: String = js_sys::decode_uri_component(raw).ok()?.into();
                if is_safe_return_to(&decoded) {
                    return Some(decoded);
                }
                return None;
            }
        }
    }
    None
}

fn is_safe_return_to(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.contains('\n') || s.contains('\r') {
        return false;
    }
    // The only return_to we ever produce is the serialized authorize
    // query (response_type=code&client_id=...&redirect_uri=...&...).
    // Anything else is suspect.
    s.starts_with("response_type=")
}

/// Build the full authorize URL from a previously-validated `return_to`
/// payload and navigate the browser there. The OP-session cookie set
/// during login covers the issuer host, so this top-level navigation
/// completes the OIDC code-flow without any extra round-trips.
#[cfg(feature = "web")]
fn bounce_to_authorize(return_to: &str) {
    let cfg = crate::modules::oidc::OidcConfig::from_env();
    let url = format!(
        "{}/oauth2/authorize?{}",
        cfg.issuer.trim_end_matches('/'),
        return_to
    );
    if let Some(win) = web_sys::window() {
        let _ = win.location().assign(&url);
    }
}

#[cfg(not(feature = "web"))]
fn bounce_to_authorize(_return_to: &str) {}

const GOOGLE_BUTTON_CLASS: &str =
    "w-full inline-flex items-center justify-center gap-3 rounded-md border \
     border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-4 py-2 \
     text-sm font-medium text-gray-700 dark:text-gray-200 shadow-sm \
     hover:bg-gray-50 dark:hover:bg-gray-700 disabled:opacity-50 \
     disabled:cursor-not-allowed";

/// Login page component
#[component]
pub fn LoginPage() -> Element {
    let auth = use_auth();
    let navigator = use_navigator();

    // `?return_to=<authorize_query>` is set when an OIDC relying party
    // redirects an unauthenticated user through `/oauth2/authorize`,
    // which 302s here. After a successful login we navigate to
    // `<issuer>/oauth2/authorize?<return_to>`; the OP-session cookie is
    // now set on `.a8n.run`, so the authorize endpoint sees it on that
    // top-level navigation and 302s on to the RP with a code. See
    // docs/mokosh-auth/09-single-login-bridge.md.
    //
    // The guard rejects everything that does not begin with
    // `response_type=` so this query parameter cannot be used as an
    // open-redirect to an arbitrary URL.
    let return_to = read_safe_return_to();

    use_effect({
        let return_to = return_to.clone();
        move || {
            if auth.read().is_authenticated() {
                if let Some(rt) = &return_to {
                    bounce_to_authorize(rt);
                } else {
                    navigator.push(Route::Dashboard {});
                }
            }
        }
    });

    let (mut form_state, submit, submit_mfa) = use_login_form_with_return_to(return_to.clone());
    let google = use_google_login();

    // Two sign-in paths:
    //   1. Email/password form: posts directly to /v1/auth/login, which
    //      mints tokens for the SPA's first-party client in the same
    //      response. Single page, no redirect dance.
    //   2. "Sign in with Google": popup + postMessage flow.
    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                div {
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                        "Sign in to your account"
                    }
                }

                if form_state.read().mfa_challenge.is_some() {
                    form {
                        class: "space-y-4",
                        onsubmit: move |e| {
                            e.prevent_default();
                            submit_mfa.call(());
                        },

                        p { class: "text-sm text-gray-600 dark:text-gray-300",
                            "Enter the 6-digit code from your authenticator app, or paste a recovery code."
                        }

                        if let Some(error) = &form_state.read().error {
                            div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                                p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                            }
                        }

                        Input {
                            name: "mfa_code",
                            label: "Authenticator code",
                            r#type: "text",
                            placeholder: "123456",
                            required: true,
                            value: form_state.read().mfa_code.clone(),
                            oninput: move |e: FormEvent| {
                                let raw = e.value();
                                form_state.write().mfa_code = raw.clone();
                                // Auto-submit the moment the user types
                                // the 6th digit. Recovery codes (11
                                // chars with a hyphen) still need the
                                // explicit Verify button because they
                                // include alpha characters and we
                                // shouldn't fire halfway through.
                                let trimmed = raw.trim();
                                if trimmed.len() == 6
                                    && trimmed.chars().all(|c| c.is_ascii_digit())
                                    && !form_state.read().is_submitting
                                {
                                    submit_mfa.call(());
                                }
                            },
                        }

                        div { class: "flex items-center",
                            input {
                                id: "mfa_remember",
                                name: "mfa_remember",
                                r#type: "checkbox",
                                class: "h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500",
                                checked: form_state.read().remember_device,
                                onchange: move |e: FormEvent| {
                                    form_state.write().remember_device = e.value() == "true";
                                },
                            }
                            label {
                                r#for: "mfa_remember",
                                class: "ml-2 block text-sm text-gray-700 dark:text-gray-300",
                                "Trust this browser for 7 days"
                            }
                        }

                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            class: "w-full",
                            loading: form_state.read().is_submitting,
                            "Verify"
                        }

                        button {
                            r#type: "button",
                            class: "block w-full text-center text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700",
                            onclick: move |_| {
                                let mut s = form_state.write();
                                s.mfa_challenge = None;
                                s.mfa_code.clear();
                                s.error = None;
                            },
                            "Cancel and sign in again"
                        }
                    }
                } else {
                    form {
                        class: "space-y-4",
                        onsubmit: move |e| {
                            e.prevent_default();
                            submit.call(());
                        },

                        if let Some(error) = &form_state.read().error {
                            div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                                p { class: "text-sm text-red-600 dark:text-red-400",
                                    "{error}"
                                }
                            }
                        }

                        Input {
                            name: "email",
                            label: "Email address",
                            r#type: "email",
                            placeholder: "you@example.com",
                            required: true,
                            value: form_state.read().email.clone(),
                            oninput: move |e: FormEvent| {
                                form_state.write().email = e.value();
                            },
                        }

                        Input {
                            name: "password",
                            label: "Password",
                            r#type: "password",
                            placeholder: "Enter your password",
                            required: true,
                            value: form_state.read().password.clone(),
                            oninput: move |e: FormEvent| {
                                form_state.write().password = e.value();
                            },
                        }

                        div { class: "flex items-center justify-between",
                            div { class: "flex items-center",
                                input {
                                    id: "remember_me",
                                    name: "remember_me",
                                    r#type: "checkbox",
                                    class: "h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500",
                                    checked: form_state.read().remember_me,
                                    onchange: move |e: FormEvent| {
                                        form_state.write().remember_me = e.value() == "true";
                                    },
                                }
                                label {
                                    r#for: "remember_me",
                                    class: "ml-2 block text-sm text-gray-700 dark:text-gray-300",
                                    "Remember me"
                                }
                            }
                            Link {
                                to: Route::ForgotPassword {},
                                class: "text-sm text-blue-600 hover:text-blue-500 dark:text-blue-400",
                                "Forgot your password?"
                            }
                        }

                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            class: "w-full",
                            loading: form_state.read().is_submitting,
                            "Sign in"
                        }
                    }
                }

                div { class: "relative my-4",
                    div { class: "absolute inset-0 flex items-center",
                        div { class: "w-full border-t border-gray-300 dark:border-gray-600" }
                    }
                    div { class: "relative flex justify-center text-xs uppercase",
                        span { class: "bg-white dark:bg-gray-900 px-2 text-gray-500 dark:text-gray-400",
                            "or"
                        }
                    }
                }

                if let GoogleLoginStatus::Error(msg) = google.status.read().clone() {
                    div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                        p { class: "text-sm text-red-600 dark:text-red-400",
                            "{msg}"
                        }
                    }
                }

                button {
                    r#type: "button",
                    class: GOOGLE_BUTTON_CLASS,
                    disabled: matches!(google.status.read().clone(), GoogleLoginStatus::InProgress),
                    onclick: move |_| google.start.call(()),
                    // Inline Google "G" mark (multi-color SVG) per Google branding guidelines.
                    svg {
                        class: "h-5 w-5",
                        view_box: "0 0 24 24",
                        xmlns: "http://www.w3.org/2000/svg",
                        path { fill: "#4285F4", d: "M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" }
                        path { fill: "#34A853", d: "M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" }
                        path { fill: "#FBBC05", d: "M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" }
                        path { fill: "#EA4335", d: "M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" }
                    }
                    if matches!(google.status.read().clone(), GoogleLoginStatus::InProgress) {
                        "Opening Google sign-in..."
                    } else {
                        "Sign in with Google"
                    }
                }

                div { class: "text-center text-sm text-gray-600 dark:text-gray-400",
                    "Don't have an account? "
                    Link {
                        to: Route::Signup {},
                        class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                        "Sign up"
                    }
                }
            }
        }
    }
}

/// Forgot password page component
#[component]
pub fn ForgotPasswordPage() -> Element {
    let mut email = use_signal(String::new);
    let mut submitted = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);

        let addr = email.read().trim().to_string();
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let body = serde_json::json!({ "email": addr });
            // Server returns 200 whether the email is registered or
            // not. We ignore the response and render the same
            // "if an account exists" message in both cases. Network
            // failures also collapse to the same UX: the user gets a
            // "check your email" page; clicking again is rate-limited.
            let _: Result<serde_json::Value, _> =
                crate::modules::oidc::issuer_post(&cfg, "/v1/auth/password-reset", &body).await;

            is_submitting.set(false);
            submitted.set(true);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                div {
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                        "Reset your password"
                    }
                    p { class: "mt-2 text-sm text-gray-600 dark:text-gray-400 text-center",
                        "Enter your email and we'll send you a reset link."
                    }
                }

                if *submitted.read() {
                    div { class: "bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-md p-4",
                        p { class: "text-sm text-green-600 dark:text-green-400",
                            "If an account exists for that email, we've sent password reset instructions."
                        }
                    }

                    div { class: "text-center",
                        Link {
                            to: Route::Login {},
                            class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                            "Return to login"
                        }
                    }
                } else {
                    form {
                        class: "space-y-4",
                        onsubmit: handle_submit,

                        Input {
                            name: "email",
                            label: "Email address",
                            r#type: "email",
                            placeholder: "you@example.com",
                            required: true,
                            value: email.read().clone(),
                            oninput: move |e: FormEvent| {
                                email.set(e.value());
                            },
                        }

                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            class: "w-full",
                            loading: *is_submitting.read(),
                            "Send reset link"
                        }
                    }

                    div { class: "text-center",
                        Link {
                            to: Route::Login {},
                            class: "text-sm text-gray-600 hover:text-gray-500 dark:text-gray-400",
                            "Back to login"
                        }
                    }
                }
            }
        }
    }
}

/// Reset password page component
#[derive(Props, Clone, PartialEq)]
pub struct ResetPasswordPageProps {
    pub token: String,
}

#[component]
pub fn ResetPasswordPage(props: ResetPasswordPageProps) -> Element {
    let mut password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);

    let token = props.token.clone();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();

        let pw = password.read().clone();
        let confirm = confirm_password.read().clone();

        if pw != confirm {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }

        is_submitting.set(true);
        error.set(None);

        let token = token.clone();
        let pw_for_async = pw.clone();
        let confirm_for_async = confirm.clone();
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path =
                format!("/v1/auth/password-reset/by-token/{token}/complete");
            let body = serde_json::json!({
                "password": pw_for_async,
                "password_confirmation": confirm_for_async,
            });
            // We accept any JSON-shaped 2xx; the response carries
            // {user_id, redirect_to} on success but the SPA only
            // needs to know it succeeded.
            match crate::modules::oidc::issuer_post::<serde_json::Value, _>(&cfg, &path, &body).await {
                Ok(_) => {
                    success.set(true);
                }
                Err(e) => {
                    let raw = e.to_string();
                    // Server's invalid_request shape carries
                    // { details: { field: msg } }. Surface the
                    // field-specific message when present.
                    if let Some(msg) = parse_first_field_error(&raw) {
                        error.set(Some(msg));
                    } else if raw.contains("reset_not_found") {
                        error.set(Some(
                            "This reset link is invalid, expired, or has already been used.".into(),
                        ));
                    } else if raw.contains("HTTP 429") || raw.contains("rate") {
                        error.set(Some(
                            "Too many attempts. Please wait a moment and try again.".into(),
                        ));
                    } else {
                        error.set(Some(format!("Could not reset password: {raw}")));
                    }
                }
            }
            is_submitting.set(false);
        });
    };

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                div {
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                        "Set new password"
                    }
                }

                if *success.read() {
                    div { class: "bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-md p-4",
                        p { class: "text-sm text-green-600 dark:text-green-400",
                            "Your password has been reset successfully."
                        }
                    }

                    div { class: "text-center",
                        Link {
                            to: Route::Login {},
                            class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                            "Sign in with your new password"
                        }
                    }
                } else {
                    form {
                        class: "space-y-4",
                        onsubmit: handle_submit,

                        if let Some(err) = error.read().as_ref() {
                            div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                                p { class: "text-sm text-red-600 dark:text-red-400",
                                    "{err}"
                                }
                            }
                        }

                        Input {
                            name: "password",
                            label: "New password",
                            r#type: "password",
                            placeholder: "Enter new password",
                            required: true,
                            help: "Must be at least 8 characters",
                            value: password.read().clone(),
                            oninput: move |e: FormEvent| {
                                password.set(e.value());
                            },
                        }

                        Input {
                            name: "confirm_password",
                            label: "Confirm password",
                            r#type: "password",
                            placeholder: "Confirm new password",
                            required: true,
                            value: confirm_password.read().clone(),
                            oninput: move |e: FormEvent| {
                                confirm_password.set(e.value());
                            },
                        }

                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            class: "w-full",
                            loading: *is_submitting.read(),
                            "Reset password"
                        }
                    }
                }
            }
        }
    }
}

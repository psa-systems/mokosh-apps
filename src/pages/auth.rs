//! Authentication pages (login, signup, password reset)

use dioxus::prelude::*;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::hooks::{use_auth, use_google_login, use_login_form, GoogleLoginStatus};
use crate::Route;

const MOKOSH_BUTTON_CLASS: &str =
    "w-full inline-flex items-center justify-center gap-3 rounded-md \
     bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm \
     hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed";

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

    use_effect(move || {
        if auth.read().is_authenticated() {
            navigator.push(Route::Dashboard {});
        }
    });

    let (mut form_state, submit) = use_login_form();
    let google = use_google_login();

    // The SPA doesn't collect credentials. Mokosh authentication is
    // delegated to the OP (mokosh-server's /login form, reached via the
    // OIDC authorize redirect) so credentials are entered exactly once.
    // This page just offers two starting points:
    //   1. "Sign in with Mokosh" -> OIDC code+PKCE flow against
    //      mokosh-server. start_login redirects the browser; nothing
    //      else runs after.
    //   2. "Sign in with Google" -> popup + postMessage flow against
    //      Google directly (separate path; not part of OIDC).
    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                div {
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                        "Sign in to your account"
                    }
                    p { class: "mt-2 text-sm text-gray-600 dark:text-gray-400 text-center",
                        "You'll be sent to Mokosh to enter your credentials."
                    }
                }

                if let Some(error) = &form_state.read().error {
                    div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md p-4",
                        p { class: "text-sm text-red-600 dark:text-red-400",
                            "{error}"
                        }
                    }
                }

                button {
                    r#type: "button",
                    class: MOKOSH_BUTTON_CLASS,
                    disabled: form_state.read().is_submitting,
                    onclick: move |_| submit(),
                    if form_state.read().is_submitting {
                        "Redirecting to Mokosh..."
                    } else {
                        "Sign in with Mokosh"
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
                    a {
                        class: "text-blue-600 hover:text-blue-500 dark:text-blue-400",
                        "Contact us"
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

        spawn(async move {
            // TODO: Call API to send password reset email
            #[cfg(feature = "web")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(1000).await;
            }

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
#[allow(unused_variables)]
pub fn ResetPasswordPage(props: ResetPasswordPageProps) -> Element {
    let mut password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);

    let handle_submit = move |e: FormEvent| {
        e.prevent_default();

        let pw = password.read().clone();
        let confirm = confirm_password.read().clone();

        if pw.len() < 8 {
            error.set(Some("Password must be at least 8 characters".to_string()));
            return;
        }

        if pw != confirm {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }

        is_submitting.set(true);
        error.set(None);

        spawn(async move {
            // TODO: Call API to reset password with token
            #[cfg(feature = "web")]
            {
                use gloo_timers::future::TimeoutFuture;
                TimeoutFuture::new(1000).await;
            }

            is_submitting.set(false);
            success.set(true);
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

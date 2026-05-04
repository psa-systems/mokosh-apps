//! Authentication pages (login, signup, password reset)

use dioxus::prelude::*;

use crate::components::{AuthLayout, Button, ButtonVariant, Input};
use crate::hooks::{use_auth, use_login_form};
use crate::Route;

/// Login page component
#[component]
pub fn LoginPage() -> Element {
    let (mut form_state, submit) = use_login_form();

    rsx! {
        AuthLayout {
            div { class: "space-y-6",
                div {
                    h2 { class: "text-2xl font-bold text-gray-900 dark:text-white text-center",
                        "Sign in to your account"
                    }
                }

                form {
                    class: "space-y-4",
                    onsubmit: move |e| {
                        e.prevent_default();
                        submit();
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

                div { class: "text-center text-sm text-gray-600 dark:text-gray-400",
                    "Don't have an account? "
                    a {
                        href: "#",
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

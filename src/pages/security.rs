//! Settings -> Security: manage two-factor authentication.
//!
//! State machine:
//!   - `Loading`     - first fetch of /v1/auth/mfa/status
//!   - `Disabled`    - status returned mfa_enrolled=false; render the
//!                     "Enable two-factor" button
//!   - `Enrolling`   - we have a setup payload from POST /mfa/setup;
//!                     three sub-steps: ScanQr, ConfirmCode, SaveCodes
//!   - `Enrolled`    - status returned mfa_enrolled=true; show counts,
//!                     regenerate/disable buttons gated on step-up
//!
//! Step-up is a small inline flow: a "Enter your code to continue"
//! prompt + a POST to /step-up/start then /step-up/verify, yielding a
//! one-time `step_up_token` that the destructive endpoint accepts.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Button, ButtonVariant, Card, PageHeader};

#[derive(Clone, Debug, Deserialize)]
struct StatusBody {
    mfa_enrolled: bool,
    #[serde(default)]
    recovery_codes_unused: u32,
    #[serde(default)]
    low_warning: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SetupBody {
    secret: String,
    provisioning_uri: String,
    qr_svg: String,
    recovery_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ChallengeBody {
    challenge: String,
}

#[derive(Clone, Debug, Deserialize)]
struct StepUpTokenBody {
    step_up_token: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RegenerateBody {
    recovery_codes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum EnrollStep {
    ScanQr,
    ConfirmCode,
    SaveCodes,
}

#[derive(Clone, PartialEq)]
struct EnrollmentInProgress {
    setup: SetupBody,
    step: EnrollStep,
    code_input: String,
    confirm_error: Option<String>,
    submitting: bool,
}

#[derive(Clone, PartialEq)]
struct StepUpInProgress {
    /// Active challenge from /step-up/start; SPA must POST this with
    /// the code to /step-up/verify to get the step_up_token.
    challenge: String,
    code_input: String,
    error: Option<String>,
    submitting: bool,
    /// What to do after step-up succeeds: regenerate or disable.
    intent: StepUpIntent,
}

#[derive(Clone, Copy, PartialEq)]
enum StepUpIntent {
    Regenerate,
    Disable,
}

#[component]
pub fn SecurityPage() -> Element {
    let mut status: Signal<Option<Result<StatusBody, String>>> = use_signal(|| None);
    let mut enrollment: Signal<Option<EnrollmentInProgress>> = use_signal(|| None);
    let mut step_up: Signal<Option<StepUpInProgress>> = use_signal(|| None);
    let mut fresh_codes: Signal<Option<Vec<String>>> = use_signal(|| None);
    let mut bump = use_signal(|| 0u32);

    use_future(move || async move {
        let _ = bump.read();
        status.set(None);
        let cfg = crate::modules::oidc::OidcConfig::from_env();
        let r = crate::modules::oidc::issuer_get_authed::<StatusBody>(&cfg, "/v1/auth/mfa/status")
            .await
            .map_err(|e| e.to_string());
        status.set(Some(r));
    });

    let refetch = use_callback(move |_| bump.with_mut(|n| *n += 1));

    let start_enroll = use_callback(move |_| {
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let r = crate::modules::oidc::issuer_post_authed::<SetupBody, _>(
                &cfg,
                "/v1/auth/mfa/setup",
                &serde_json::json!({}),
            )
            .await;
            match r {
                Ok(setup) => enrollment.set(Some(EnrollmentInProgress {
                    setup,
                    step: EnrollStep::ScanQr,
                    code_input: String::new(),
                    confirm_error: None,
                    submitting: false,
                })),
                Err(e) => status.set(Some(Err(e.to_string()))),
            }
        });
    });

    let submit_confirm = use_callback(move |_| {
        let enr = match enrollment.read().clone() {
            Some(e) if e.step == EnrollStep::ConfirmCode => e,
            _ => return,
        };
        spawn(async move {
            enrollment.with_mut(|e| {
                if let Some(e) = e.as_mut() {
                    e.submitting = true;
                    e.confirm_error = None;
                }
            });
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let r = crate::modules::oidc::issuer_post_authed::<serde_json::Value, _>(
                &cfg,
                "/v1/auth/mfa/confirm",
                &serde_json::json!({"code": enr.code_input.trim()}),
            )
            .await;
            match r {
                Ok(_) => {
                    enrollment.with_mut(|e| {
                        if let Some(e) = e.as_mut() {
                            e.step = EnrollStep::SaveCodes;
                            e.submitting = false;
                        }
                    });
                }
                Err(e) => {
                    enrollment.with_mut(|en| {
                        if let Some(en) = en.as_mut() {
                            en.submitting = false;
                            en.confirm_error =
                                Some(format!("Code rejected: {e}"));
                        }
                    });
                }
            }
        });
    });

    let finish_enrollment = use_callback(move |_| {
        enrollment.set(None);
        refetch.call(());
    });

    let start_step_up = use_callback(move |intent: StepUpIntent| {
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let r = crate::modules::oidc::issuer_post_authed::<ChallengeBody, _>(
                &cfg,
                "/v1/auth/mfa/step-up/start",
                &serde_json::json!({}),
            )
            .await;
            match r {
                Ok(c) => step_up.set(Some(StepUpInProgress {
                    challenge: c.challenge,
                    code_input: String::new(),
                    error: None,
                    submitting: false,
                    intent,
                })),
                Err(e) => step_up.set(Some(StepUpInProgress {
                    challenge: String::new(),
                    code_input: String::new(),
                    error: Some(format!("{e}")),
                    submitting: false,
                    intent,
                })),
            }
        });
    });

    let submit_step_up = use_callback(move |_| {
        let su = match step_up.read().clone() {
            Some(s) if !s.challenge.is_empty() => s,
            _ => return,
        };
        spawn(async move {
            step_up.with_mut(|s| {
                if let Some(s) = s.as_mut() {
                    s.submitting = true;
                    s.error = None;
                }
            });
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let verify = crate::modules::oidc::issuer_post_authed::<StepUpTokenBody, _>(
                &cfg,
                "/v1/auth/mfa/step-up/verify",
                &serde_json::json!({"challenge": su.challenge, "code": su.code_input.trim()}),
            )
            .await;
            let token = match verify {
                Ok(t) => t.step_up_token,
                Err(e) => {
                    step_up.with_mut(|s| {
                        if let Some(s) = s.as_mut() {
                            s.submitting = false;
                            s.error = Some(format!("{e}"));
                        }
                    });
                    return;
                }
            };
            match su.intent {
                StepUpIntent::Regenerate => {
                    let r = crate::modules::oidc::issuer_post_authed::<RegenerateBody, _>(
                        &cfg,
                        "/v1/auth/mfa/recovery-codes/regenerate",
                        &serde_json::json!({"step_up_token": token}),
                    )
                    .await;
                    match r {
                        Ok(b) => {
                            fresh_codes.set(Some(b.recovery_codes));
                            step_up.set(None);
                            refetch.call(());
                        }
                        Err(e) => step_up.with_mut(|s| {
                            if let Some(s) = s.as_mut() {
                                s.submitting = false;
                                s.error = Some(format!("regenerate: {e}"));
                            }
                        }),
                    }
                }
                StepUpIntent::Disable => {
                    let r = crate::modules::oidc::issuer_post_authed::<serde_json::Value, _>(
                        &cfg,
                        "/v1/auth/mfa/disable",
                        &serde_json::json!({"step_up_token": token}),
                    )
                    .await;
                    match r {
                        Ok(_) => {
                            step_up.set(None);
                            fresh_codes.set(None);
                            refetch.call(());
                        }
                        Err(e) => step_up.with_mut(|s| {
                            if let Some(s) = s.as_mut() {
                                s.submitting = false;
                                s.error = Some(format!("disable: {e}"));
                            }
                        }),
                    }
                }
            }
        });
    });

    rsx! {
        AppLayout { title: "Security",
            PageHeader { title: "Security", subtitle: "Two-factor authentication and recovery codes" }

            div { class: "max-w-2xl space-y-6",
                Card { title: "Two-factor authentication",
                    match &*status.read() {
                        None => rsx! { p { class: "text-sm text-gray-500", "Loading..." } },
                        Some(Err(e)) => rsx! {
                            p { class: "text-sm text-red-600", "Failed to load: {e}" }
                        },
                        Some(Ok(s)) => {
                            if let Some(codes) = fresh_codes.read().clone() {
                                rsx! { RecoveryCodesView { codes: codes, on_done: move |_| fresh_codes.set(None) } }
                            } else if let Some(en) = enrollment.read().clone() {
                                rsx! {
                                    EnrollmentFlow {
                                        enrollment: en.clone(),
                                        on_continue_to_confirm: move |_| {
                                            enrollment.with_mut(|e| if let Some(e) = e.as_mut() { e.step = EnrollStep::ConfirmCode; });
                                        },
                                        on_code_input: move |v: String| {
                                            enrollment.with_mut(|e| if let Some(e) = e.as_mut() { e.code_input = v; });
                                        },
                                        on_submit_confirm: move |_| submit_confirm.call(()),
                                        on_finish: move |_| finish_enrollment.call(()),
                                    }
                                }
                            } else if let Some(su) = step_up.read().clone() {
                                rsx! {
                                    StepUpPrompt {
                                        step_up: su.clone(),
                                        on_code_input: move |v: String| {
                                            step_up.with_mut(|s| if let Some(s) = s.as_mut() { s.code_input = v; });
                                        },
                                        on_submit: move |_| submit_step_up.call(()),
                                        on_cancel: move |_| step_up.set(None),
                                    }
                                }
                            } else if s.mfa_enrolled {
                                rsx! {
                                    div { class: "space-y-4",
                                        p { class: "text-sm text-gray-700 dark:text-gray-200",
                                            "Two-factor authentication is "
                                            span { class: "font-medium text-green-600", "enabled" }
                                            "."
                                        }
                                        p { class: "text-sm",
                                            "{s.recovery_codes_unused} recovery codes remaining."
                                            if s.low_warning {
                                                span { class: "ml-2 text-yellow-600", "Low; regenerate now." }
                                            }
                                        }
                                        div { class: "flex gap-2",
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                onclick: move |_| start_step_up.call(StepUpIntent::Regenerate),
                                                "Regenerate recovery codes"
                                            }
                                            Button {
                                                variant: ButtonVariant::Danger,
                                                onclick: move |_| start_step_up.call(StepUpIntent::Disable),
                                                "Disable two-factor"
                                            }
                                        }
                                    }
                                }
                            } else {
                                rsx! {
                                    div { class: "space-y-4",
                                        p { class: "text-sm text-gray-700 dark:text-gray-200",
                                            "Two-factor authentication is "
                                            span { class: "font-medium", "off" }
                                            ". Enable it to add a second factor (TOTP) to your sign-in."
                                        }
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            onclick: move |_| start_enroll.call(()),
                                            "Enable two-factor"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EnrollmentFlowProps {
    enrollment: EnrollmentInProgress,
    on_continue_to_confirm: EventHandler<()>,
    on_code_input: EventHandler<String>,
    on_submit_confirm: EventHandler<()>,
    on_finish: EventHandler<()>,
}

#[component]
fn EnrollmentFlow(props: EnrollmentFlowProps) -> Element {
    let en = &props.enrollment;
    match en.step {
        EnrollStep::ScanQr => rsx! {
            div { class: "space-y-4",
                p { class: "text-sm",
                    "Scan this QR with your authenticator app (Aegis, Bitwarden, 1Password, Google Authenticator)."
                }
                div {
                    class: "mx-auto",
                    style: "max-width: 256px;",
                    dangerous_inner_html: "{en.setup.qr_svg}",
                }
                details {
                    summary { class: "text-sm cursor-pointer", "Cannot scan? Type the code instead" }
                    p { class: "text-xs font-mono mt-2 break-all", "{en.setup.secret}" }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: move |_| props.on_continue_to_confirm.call(()),
                    "I've added the code; continue"
                }
            }
        },
        EnrollStep::ConfirmCode => rsx! {
            form {
                class: "space-y-4",
                onsubmit: move |e| {
                    e.prevent_default();
                    props.on_submit_confirm.call(());
                },
                p { class: "text-sm", "Enter the 6-digit code your authenticator is currently showing." }
                if let Some(err) = &en.confirm_error {
                    div { class: "text-sm text-red-600", "{err}" }
                }
                input {
                    r#type: "text",
                    class: "block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm",
                    placeholder: "123456",
                    value: "{en.code_input}",
                    oninput: move |e| props.on_code_input.call(e.value()),
                }
                Button {
                    r#type: "submit",
                    variant: ButtonVariant::Primary,
                    loading: en.submitting,
                    "Verify and enable"
                }
            }
        },
        EnrollStep::SaveCodes => rsx! {
            div { class: "space-y-4",
                p { class: "text-sm font-medium",
                    "Save these recovery codes somewhere safe. Each one works once. You will not see them again."
                }
                ul { class: "grid grid-cols-2 gap-2 font-mono text-sm",
                    for code in en.setup.recovery_codes.iter() {
                        li { class: "bg-gray-50 dark:bg-gray-800 px-2 py-1 rounded", "{code}" }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: move |_| props.on_finish.call(()),
                    "I've saved them"
                }
            }
        },
    }
}

#[derive(Props, Clone, PartialEq)]
struct StepUpPromptProps {
    step_up: StepUpInProgress,
    on_code_input: EventHandler<String>,
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
}

#[component]
fn StepUpPrompt(props: StepUpPromptProps) -> Element {
    let su = &props.step_up;
    rsx! {
        form {
            class: "space-y-4",
            onsubmit: move |e| {
                e.prevent_default();
                props.on_submit.call(());
            },
            p { class: "text-sm", "Enter the 6-digit code from your authenticator to continue." }
            if let Some(err) = &su.error {
                div { class: "text-sm text-red-600", "{err}" }
            }
            input {
                r#type: "text",
                class: "block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm",
                placeholder: "123456",
                value: "{su.code_input}",
                oninput: move |e| props.on_code_input.call(e.value()),
            }
            div { class: "flex gap-2",
                Button {
                    r#type: "submit",
                    variant: ButtonVariant::Primary,
                    loading: su.submitting,
                    "Continue"
                }
                Button {
                    r#type: "button",
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| props.on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RecoveryCodesViewProps {
    codes: Vec<String>,
    on_done: EventHandler<()>,
}

#[component]
fn RecoveryCodesView(props: RecoveryCodesViewProps) -> Element {
    rsx! {
        div { class: "space-y-4",
            p { class: "text-sm font-medium",
                "Save these recovery codes somewhere safe. Each one works once. You will not see them again."
            }
            ul { class: "grid grid-cols-2 gap-2 font-mono text-sm",
                for code in props.codes.iter() {
                    li { class: "bg-gray-50 dark:bg-gray-800 px-2 py-1 rounded", "{code}" }
                }
            }
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| props.on_done.call(()),
                "Done"
            }
        }
    }
}

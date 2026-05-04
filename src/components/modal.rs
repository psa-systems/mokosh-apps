//! Modal dialog components

use dioxus::prelude::*;

use super::button::{Button, ButtonVariant};
use super::icons::XMarkIcon;

/// Modal size options
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ModalSize {
    Small, // max-w-md
    #[default]
    Medium, // max-w-lg
    Large, // max-w-2xl
    XLarge, // max-w-4xl
    Full,  // max-w-7xl
}

impl ModalSize {
    fn class(&self) -> &'static str {
        match self {
            ModalSize::Small => "max-w-md",
            ModalSize::Medium => "max-w-lg",
            ModalSize::Large => "max-w-2xl",
            ModalSize::XLarge => "max-w-4xl",
            ModalSize::Full => "max-w-7xl",
        }
    }
}

/// Modal dialog props
#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    /// Modal content
    children: Element,
    /// Whether modal is open
    open: bool,
    /// Modal title
    title: String,
    /// Modal size
    #[props(default)]
    size: ModalSize,
    /// Close handler
    onclose: EventHandler<()>,
    /// Optional footer actions
    footer: Option<Element>,
}

/// Modal dialog component
#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    let size_class = props.size.class();

    rsx! {
        div { class: "fixed inset-0 z-50 overflow-y-auto",
            // Backdrop
            div {
                class: "fixed inset-0 bg-gray-500 bg-opacity-75 transition-opacity",
                onclick: move |_| props.onclose.call(()),
            }

            // Modal container
            div { class: "flex min-h-full items-end justify-center p-4 text-center sm:items-center sm:p-0",
                div {
                    class: "relative transform overflow-hidden rounded-lg bg-white dark:bg-gray-800 text-left shadow-xl transition-all sm:my-8 w-full {size_class}",
                    onclick: |e| e.stop_propagation(),

                    // Header
                    div { class: "flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700",
                        h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{props.title}"
                        }
                        button {
                            class: "rounded-md text-gray-400 hover:text-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500",
                            onclick: move |_| props.onclose.call(()),
                            XMarkIcon {}
                        }
                    }

                    // Body
                    div { class: "px-4 py-4",
                        {props.children}
                    }

                    // Footer (optional)
                    if let Some(ref footer) = props.footer {
                        div { class: "px-4 py-3 border-t border-gray-200 dark:border-gray-700 flex justify-end space-x-3",
                            {footer}
                        }
                    }
                }
            }
        }
    }
}

/// Confirmation dialog props
#[derive(Props, Clone, PartialEq)]
pub struct ConfirmDialogProps {
    /// Whether dialog is open
    open: bool,
    /// Dialog title
    title: String,
    /// Dialog message
    message: String,
    /// Confirm button text
    #[props(default = "Confirm".to_string())]
    confirm_text: String,
    /// Cancel button text
    #[props(default = "Cancel".to_string())]
    cancel_text: String,
    /// Whether this is a destructive action
    #[props(default = false)]
    destructive: bool,
    /// Loading state
    #[props(default = false)]
    loading: bool,
    /// Confirm handler
    onconfirm: EventHandler<()>,
    /// Cancel/close handler
    oncancel: EventHandler<()>,
}

/// Confirmation dialog component
#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    let confirm_variant = if props.destructive {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };

    rsx! {
        Modal {
            open: props.open,
            title: props.title.clone(),
            size: ModalSize::Small,
            onclose: move |_| props.oncancel.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| props.oncancel.call(()),
                    disabled: props.loading,
                    "{props.cancel_text}"
                }
                Button {
                    variant: confirm_variant,
                    onclick: move |_| props.onconfirm.call(()),
                    loading: props.loading,
                    "{props.confirm_text}"
                }
            },
            p { class: "text-sm text-gray-500 dark:text-gray-400",
                "{props.message}"
            }
        }
    }
}

/// Alert/notification types
#[derive(Clone, Copy, PartialEq, Default)]
pub enum AlertType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl AlertType {
    fn classes(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            AlertType::Info => (
                "bg-blue-50 dark:bg-blue-900/20",
                "text-blue-400",
                "text-blue-700 dark:text-blue-300",
            ),
            AlertType::Success => (
                "bg-green-50 dark:bg-green-900/20",
                "text-green-400",
                "text-green-700 dark:text-green-300",
            ),
            AlertType::Warning => (
                "bg-yellow-50 dark:bg-yellow-900/20",
                "text-yellow-400",
                "text-yellow-700 dark:text-yellow-300",
            ),
            AlertType::Error => (
                "bg-red-50 dark:bg-red-900/20",
                "text-red-400",
                "text-red-700 dark:text-red-300",
            ),
        }
    }
}

/// Alert banner props
#[derive(Props, Clone, PartialEq)]
pub struct AlertProps {
    /// Alert type
    #[props(default)]
    alert_type: AlertType,
    /// Alert title
    #[props(default)]
    title: String,
    /// Alert message
    message: String,
    /// Whether alert can be dismissed
    #[props(default = false)]
    dismissible: bool,
    /// Dismiss handler
    #[props(default)]
    ondismiss: EventHandler<()>,
}

/// Alert banner component
#[component]
pub fn Alert(props: AlertProps) -> Element {
    let (bg_class, icon_class, text_class) = props.alert_type.classes();

    rsx! {
        div { class: "rounded-md p-4 {bg_class}",
            div { class: "flex",
                div { class: "flex-shrink-0",
                    // Icon based on type
                    match props.alert_type {
                        AlertType::Info => rsx! {
                            super::icons::InformationIcon { class: icon_class.to_string() }
                        },
                        AlertType::Success => rsx! {
                            super::icons::CheckIcon { class: icon_class.to_string() }
                        },
                        AlertType::Warning | AlertType::Error => rsx! {
                            super::icons::ExclamationIcon { class: icon_class.to_string() }
                        },
                    }
                }
                div { class: "ml-3 flex-1",
                    if !props.title.is_empty() {
                        h3 { class: "text-sm font-medium {text_class}",
                            "{props.title}"
                        }
                    }
                    p { class: "text-sm {text_class}",
                        "{props.message}"
                    }
                }
                if props.dismissible {
                    div { class: "ml-auto pl-3",
                        button {
                            class: "-mx-1.5 -my-1.5 rounded-md p-1.5 inline-flex {text_class} hover:bg-gray-100 dark:hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-green-50 focus:ring-green-600",
                            onclick: move |_| props.ondismiss.call(()),
                            XMarkIcon {}
                        }
                    }
                }
            }
        }
    }
}

/// Toast notification position
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ToastPosition {
    TopRight,
    #[default]
    BottomRight,
    TopLeft,
    BottomLeft,
    TopCenter,
    BottomCenter,
}

impl ToastPosition {
    fn class(&self) -> &'static str {
        match self {
            ToastPosition::TopRight => "top-4 right-4",
            ToastPosition::BottomRight => "bottom-4 right-4",
            ToastPosition::TopLeft => "top-4 left-4",
            ToastPosition::BottomLeft => "bottom-4 left-4",
            ToastPosition::TopCenter => "top-4 left-1/2 -translate-x-1/2",
            ToastPosition::BottomCenter => "bottom-4 left-1/2 -translate-x-1/2",
        }
    }
}

/// Toast notification data
#[derive(Clone, PartialEq)]
pub struct Toast {
    pub id: String,
    pub toast_type: AlertType,
    pub message: String,
    pub title: Option<String>,
}

/// Toast container props
#[derive(Props, Clone, PartialEq)]
pub struct ToastContainerProps {
    toasts: Vec<Toast>,
    #[props(default)]
    position: ToastPosition,
    ondismiss: EventHandler<String>,
}

/// Toast container component
#[component]
pub fn ToastContainer(props: ToastContainerProps) -> Element {
    let position_class = props.position.class();

    rsx! {
        div { class: "fixed z-50 {position_class} space-y-2 w-80",
            for toast in props.toasts.iter() {
                div { key: "{toast.id}",
                    class: "transform transition-all duration-300 ease-in-out",
                    Alert {
                        alert_type: toast.toast_type,
                        title: toast.title.clone().unwrap_or_default(),
                        message: toast.message.clone(),
                        dismissible: true,
                        ondismiss: {
                            let id = toast.id.clone();
                            move |_| props.ondismiss.call(id.clone())
                        },
                    }
                }
            }
        }
    }
}

/// Slide-over panel props
#[derive(Props, Clone, PartialEq)]
pub struct SlideOverProps {
    children: Element,
    open: bool,
    title: String,
    #[props(default)]
    subtitle: String,
    onclose: EventHandler<()>,
}

/// Slide-over panel component (slides in from right)
#[component]
pub fn SlideOver(props: SlideOverProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    rsx! {
        div { class: "fixed inset-0 z-50 overflow-hidden",
            // Backdrop
            div {
                class: "absolute inset-0 bg-gray-500 bg-opacity-75 transition-opacity",
                onclick: move |_| props.onclose.call(()),
            }

            // Panel
            div { class: "fixed inset-y-0 right-0 flex max-w-full pl-10",
                div { class: "relative w-screen max-w-md",
                    // Close button
                    div { class: "absolute left-0 top-0 -ml-8 flex pr-2 pt-4 sm:-ml-10 sm:pr-4",
                        button {
                            class: "rounded-md text-gray-300 hover:text-white focus:outline-none focus:ring-2 focus:ring-white",
                            onclick: move |_| props.onclose.call(()),
                            XMarkIcon { class: "h-6 w-6".to_string() }
                        }
                    }

                    // Panel content
                    div { class: "flex h-full flex-col overflow-y-scroll bg-white dark:bg-gray-800 shadow-xl",
                        // Header
                        div { class: "px-4 py-6 sm:px-6 border-b border-gray-200 dark:border-gray-700",
                            h2 { class: "text-lg font-medium text-gray-900 dark:text-white",
                                "{props.title}"
                            }
                            if !props.subtitle.is_empty() {
                                p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
                                    "{props.subtitle}"
                                }
                            }
                        }

                        // Body
                        div { class: "relative flex-1 px-4 py-6 sm:px-6",
                            {props.children}
                        }
                    }
                }
            }
        }
    }
}

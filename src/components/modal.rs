//! Modal dialog components

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

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

    // Render the open dialog as a child component so it mounts only while the
    // modal is open. That lets `ModalDialog` use `use_drop` to restore focus on
    // every close path (Esc, backdrop, X, Cancel, or a parent-driven close),
    // since the child unmounts whenever `open` flips to false.
    rsx! {
        ModalDialog {
            title: props.title.clone(),
            size: props.size,
            onclose: props.onclose,
            footer: props.footer.clone(),
            {props.children}
        }
    }
}

/// Open-modal panel. Only mounted while the modal is open.
#[component]
fn ModalDialog(
    children: Element,
    title: String,
    size: ModalSize,
    onclose: EventHandler<()>,
    footer: Option<Element>,
) -> Element {
    let size_class = size.class();

    // Restore focus to the control that opened the modal. Capture the active
    // element on the first render, before the dialog steals focus on mount, and
    // restore it when this component unmounts (any close path).
    let trigger = use_hook(|| {
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.active_element())
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    });
    use_drop(move || {
        if let Some(el) = trigger {
            let _ = el.focus();
        }
    });

    rsx! {
        div { class: "fixed inset-0 z-50 overflow-y-auto",
            // Backdrop. Tailwind v4 removed `bg-opacity-*`; use the
            // slash alpha syntax instead so the page stays visible
            // through a 75% gray dim instead of being fully covered.
            div {
                class: "fixed inset-0 bg-gray-500/75 transition-opacity", // theme-guard-allow: overlay scrim
                onclick: move |_| onclose.call(()),
            }

            // Modal container
            div { class: "flex min-h-full items-end justify-center p-4 text-center sm:items-center sm:p-0",
                div {
                    // Cap the panel at 90vh and lay it out as a flex column so the
                    // header and footer stay pinned (flex-shrink-0) while only the
                    // body scrolls. Keeps large modals usable on small screens with
                    // no double-axis scroll.
                    class: "relative transform flex flex-col max-h-[90vh] overflow-hidden rounded-lg bg-raised text-left shadow-xl transition-all sm:my-8 w-full {size_class}",
                    // PMS-369: Esc cancels. Focus the dialog on mount so the
                    // keydown lands here even before the user clicks anything;
                    // keydown from any focused control inside also bubbles up.
                    tabindex: "-1",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.set_focus(true).await;
                        });
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Escape {
                            onclose.call(());
                        }
                    },
                    onclick: |e| e.stop_propagation(),

                    // Header (pinned)
                    div { class: "flex-shrink-0 flex items-center justify-between px-4 py-3 border-b border-line",
                        h3 { class: "text-lg font-medium text-content",
                            "{title}"
                        }
                        button {
                            class: "rounded-md text-subtle hover:text-content focus:outline-none focus:ring-2 focus:ring-accent",
                            onclick: move |_| onclose.call(()),
                            XMarkIcon {}
                        }
                    }

                    // Body (scrolls within the capped height)
                    div { class: "flex-1 min-h-0 overflow-y-auto px-4 py-4",
                        {children}
                    }

                    // Footer (optional, pinned)
                    if let Some(footer) = footer {
                        div { class: "flex-shrink-0 px-4 py-3 border-t border-line flex justify-end space-x-3",
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
    /// PMS-369: type-to-confirm gate for catastrophic / cascading deletes.
    /// When non-empty, the destructive button stays disabled until the user
    /// types this exact phrase (e.g. the company name). Empty (default) = a
    /// plain one-click confirm, which is enough for non-cascading deletes.
    #[props(default)]
    confirm_phrase: String,
    /// Confirm handler
    onconfirm: EventHandler<()>,
    /// Cancel/close handler
    oncancel: EventHandler<()>,
}

/// PMS-369: does the typed text satisfy the type-to-confirm gate? An empty
/// `required` means no gate (always satisfied). Otherwise the typed text must
/// match after trimming, case-insensitively, so " acme corp " passes for
/// "Acme Corp".
pub fn confirm_phrase_satisfied(typed: &str, required: &str) -> bool {
    let required = required.trim();
    required.is_empty() || typed.trim().eq_ignore_ascii_case(required)
}

/// Confirmation dialog component
#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    let confirm_variant = if props.destructive {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };

    // PMS-369: type-to-confirm. `typed` tracks the gate input; the destructive
    // button is disabled until it matches `confirm_phrase`. A non-matching
    // (or empty) phrase keeps the button disabled, so a different entity's
    // name never carries over an enabled state.
    let mut typed = use_signal(String::new);
    let gated = !props.confirm_phrase.trim().is_empty();
    let satisfied = confirm_phrase_satisfied(&typed.read(), &props.confirm_phrase);
    let confirm_disabled = props.loading || !satisfied;
    let phrase = props.confirm_phrase.clone();

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
                    disabled: confirm_disabled,
                    "{props.confirm_text}"
                }
            },
            p { class: "text-sm text-muted",
                "{props.message}"
            }
            if gated {
                div { class: "mt-3 space-y-1",
                    label { class: "block text-xs font-medium text-muted",
                        "Type "
                        span { class: "font-semibold text-content", "{phrase}" }
                        " to confirm"
                    }
                    input {
                        r#type: "text",
                        class: "w-full rounded-md border border-line bg-surface px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-red-500",
                        value: "{typed}",
                        autocomplete: "off",
                        oninput: move |e| typed.set(e.value()),
                    }
                }
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
                            class: "-mx-1.5 -my-1.5 rounded-md p-1.5 inline-flex {text_class} hover:bg-surface-2 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-green-50 focus:ring-green-600",
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
    #[default]
    BottomRight,
}

impl ToastPosition {
    fn class(&self) -> &'static str {
        match self {
            ToastPosition::BottomRight => "bottom-4 right-4",
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

#[cfg(test)]
mod tests {
    use super::confirm_phrase_satisfied;

    // PMS-369: type-to-confirm gate. The destructive button is enabled only
    // when `confirm_phrase_satisfied` returns true.
    #[test]
    fn ungated_when_required_blank() {
        assert!(confirm_phrase_satisfied("", ""));
        assert!(confirm_phrase_satisfied("anything", "   "));
    }

    #[test]
    fn gated_needs_exact_trimmed_caseinsensitive_match() {
        // Wrong / partial input keeps the gate closed.
        assert!(!confirm_phrase_satisfied("", "Acme Corp"));
        assert!(!confirm_phrase_satisfied("Acme", "Acme Corp"));
        assert!(!confirm_phrase_satisfied("Acme Corpp", "Acme Corp"));
        // Exact match (with surrounding whitespace / different case) passes.
        assert!(confirm_phrase_satisfied("Acme Corp", "Acme Corp"));
        assert!(confirm_phrase_satisfied("  acme corp  ", "Acme Corp"));
    }
}

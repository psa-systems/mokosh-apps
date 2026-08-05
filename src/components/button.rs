//! Button components

use dioxus::prelude::*;

/// Button variant types
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Ghost,
    Link,
}

impl ButtonVariant {
    fn class(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "bg-accent text-on-accent hover:opacity-90 focus:ring-accent",
            ButtonVariant::Secondary => {
                "bg-surface-2 text-content border border-line hover:opacity-90 focus:ring-line-strong"
            }
            ButtonVariant::Danger => "bg-red-600 text-white hover:bg-red-700 focus:ring-red-500",
            ButtonVariant::Ghost => "bg-transparent text-content hover:bg-surface-2",
            ButtonVariant::Link => "bg-transparent text-accent hover:opacity-90",
        }
    }
}

/// Button size
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ButtonSize {
    fn class(&self) -> &'static str {
        match self {
            ButtonSize::Small => "px-2.5 py-1.5 text-xs",
            ButtonSize::Medium => "px-4 py-2 text-sm",
            ButtonSize::Large => "px-6 py-3 text-base",
        }
    }
}

/// Button component props
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    /// Button text/children
    children: Element,
    /// Button variant
    #[props(default)]
    variant: ButtonVariant,
    /// Button size
    #[props(default)]
    size: ButtonSize,
    /// Whether button is disabled
    #[props(default = false)]
    disabled: bool,
    /// Whether button shows loading state
    #[props(default = false)]
    loading: bool,
    /// Additional CSS classes
    #[props(default)]
    class: String,
    /// Button type attribute
    #[props(default = "button".to_string())]
    r#type: String,
    /// Click handler
    #[props(default)]
    onclick: EventHandler<MouseEvent>,
    /// Stable selector for browser-automation tests (PMC-111). When
    /// set, emitted as `data-testid="..."` on the rendered <button>.
    #[props(default)]
    data_testid: Option<String>,
    /// Optional native tooltip text, emitted as the `title` attribute.
    /// Used to explain why a disabled control is not actionable (MAPPS-217).
    #[props(default)]
    title: Option<String>,
}

/// Reusable button component
#[component]
pub fn Button(props: ButtonProps) -> Element {
    let base_class = "inline-flex items-center justify-center font-medium rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 transition-colors disabled:opacity-50 disabled:cursor-not-allowed";

    let class = format!(
        "{} {} {} {}",
        base_class,
        props.variant.class(),
        props.size.class(),
        props.class
    );

    let is_disabled = props.disabled || props.loading;

    rsx! {
        button {
            class: "{class}",
            r#type: "{props.r#type}",
            disabled: is_disabled,
            // Mirror the native disabled state into aria-disabled so assistive
            // tech announces the control as unavailable, paired with the `title`
            // reason for why (MAPPS-262).
            "aria-disabled": is_disabled.then_some("true"),
            title: props.title.as_deref(),
            "data-testid": props.data_testid.as_deref(),
            onclick: move |e| props.onclick.call(e),
            if props.loading {
                span { class: "mr-2",
                    Spinner {}
                }
            }
            {props.children}
        }
    }
}

/// Small loading spinner
#[component]
pub fn Spinner() -> Element {
    rsx! {
        svg {
            class: "animate-spin h-4 w-4",
            xmlns: "http://www.w3.org/2000/svg",
            fill: "none",
            view_box: "0 0 24 24",
            circle {
                class: "opacity-25",
                cx: "12",
                cy: "12",
                r: "10",
                stroke: "currentColor",
                stroke_width: "4",
            }
            path {
                class: "opacity-75",
                fill: "currentColor",
                d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
            }
        }
    }
}

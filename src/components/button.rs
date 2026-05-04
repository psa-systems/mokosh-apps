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
            ButtonVariant::Primary => "bg-blue-600 text-white hover:bg-blue-700 focus:ring-blue-500",
            ButtonVariant::Secondary => "bg-gray-200 text-gray-900 hover:bg-gray-300 focus:ring-gray-500 dark:bg-gray-700 dark:text-gray-100",
            ButtonVariant::Danger => "bg-red-600 text-white hover:bg-red-700 focus:ring-red-500",
            ButtonVariant::Ghost => "bg-transparent text-gray-700 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800",
            ButtonVariant::Link => "bg-transparent text-blue-600 hover:underline dark:text-blue-400",
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

    rsx! {
        button {
            class: "{class}",
            r#type: "{props.r#type}",
            disabled: props.disabled || props.loading,
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

/// Icon button for toolbar/action items
#[derive(Props, Clone, PartialEq)]
pub struct IconButtonProps {
    /// Icon element
    children: Element,
    /// Tooltip/title
    #[props(default)]
    title: String,
    /// Additional CSS classes
    #[props(default)]
    class: String,
    /// Whether button is disabled
    #[props(default = false)]
    disabled: bool,
    /// Click handler
    #[props(default)]
    onclick: EventHandler<MouseEvent>,
}

#[component]
pub fn IconButton(props: IconButtonProps) -> Element {
    let class = format!(
        "p-2 rounded-md text-gray-500 hover:text-gray-700 hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed dark:text-gray-400 dark:hover:text-gray-200 dark:hover:bg-gray-800 {}",
        props.class
    );

    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            title: "{props.title}",
            disabled: props.disabled,
            onclick: move |e| props.onclick.call(e),
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

/// Button group for related actions
#[derive(Props, Clone, PartialEq)]
pub struct ButtonGroupProps {
    children: Element,
    #[props(default)]
    class: String,
}

#[component]
pub fn ButtonGroup(props: ButtonGroupProps) -> Element {
    let class = format!("inline-flex rounded-md shadow-sm {}", props.class);

    rsx! {
        div { class: "{class}",
            {props.children}
        }
    }
}

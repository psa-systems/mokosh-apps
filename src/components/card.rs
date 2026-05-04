//! Card components

use dioxus::prelude::*;

/// Card component props
#[derive(Props, Clone, PartialEq)]
pub struct CardProps {
    /// Card content
    children: Element,
    /// Additional CSS classes
    #[props(default)]
    class: String,
    /// Optional header title
    #[props(default)]
    title: String,
    /// Optional header actions
    actions: Option<Element>,
    /// Whether to add padding
    #[props(default = true)]
    padding: bool,
}

/// Card container component
#[component]
pub fn Card(props: CardProps) -> Element {
    let base_class =
        "bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700";
    let padding_class = if props.padding { "p-6" } else { "" };
    let class = format!("{} {} {}", base_class, padding_class, props.class);

    let has_header = !props.title.is_empty();

    rsx! {
        div { class: "{class}",
            if has_header {
                CardHeader {
                    title: props.title,
                    actions: props.actions,
                }
            }
            {props.children}
        }
    }
}

/// Card header component
#[derive(Props, Clone, PartialEq)]
pub struct CardHeaderProps {
    title: String,
    #[props(default)]
    subtitle: String,
    actions: Option<Element>,
    #[props(default)]
    class: String,
}

#[component]
pub fn CardHeader(props: CardHeaderProps) -> Element {
    let class = format!(
        "flex items-center justify-between pb-4 border-b border-gray-200 dark:border-gray-700 {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div {
                h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                    "{props.title}"
                }
                if !props.subtitle.is_empty() {
                    p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
                        "{props.subtitle}"
                    }
                }
            }
            div { class: "flex items-center space-x-2",
                {props.actions}
            }
        }
    }
}

/// Stats card for dashboard metrics
#[derive(Props, Clone, PartialEq)]
pub struct StatCardProps {
    /// Stat label
    label: String,
    /// Stat value
    value: String,
    /// Optional icon
    icon: Option<Element>,
    /// Optional change indicator (e.g., "+12%")
    #[props(default)]
    change: String,
    /// Whether change is positive
    #[props(default = true)]
    change_positive: bool,
    /// Additional CSS classes
    #[props(default)]
    class: String,
}

#[component]
pub fn StatCard(props: StatCardProps) -> Element {
    let change_class = if props.change_positive {
        "text-green-600 dark:text-green-400"
    } else {
        "text-red-600 dark:text-red-400"
    };

    let class = format!(
        "bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-6 {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div { class: "flex items-center",
                if let Some(ref icon) = props.icon {
                    div { class: "flex-shrink-0 p-3 bg-blue-100 dark:bg-blue-900 rounded-full",
                        {icon}
                    }
                }
                div { class: if props.icon.is_some() { "ml-4" } else { "" },
                    p { class: "text-sm font-medium text-gray-500 dark:text-gray-400",
                        "{props.label}"
                    }
                    div { class: "flex items-baseline",
                        p { class: "text-2xl font-semibold text-gray-900 dark:text-white",
                            "{props.value}"
                        }
                        if !props.change.is_empty() {
                            span { class: "ml-2 text-sm font-medium {change_class}",
                                "{props.change}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Simple info card with icon
#[derive(Props, Clone, PartialEq)]
pub struct InfoCardProps {
    title: String,
    description: String,
    icon: Option<Element>,
    #[props(default)]
    class: String,
}

#[component]
pub fn InfoCard(props: InfoCardProps) -> Element {
    let class = format!(
        "bg-white dark:bg-gray-800 rounded-lg shadow border border-gray-200 dark:border-gray-700 p-6 {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div { class: "flex items-start",
                if let Some(ref icon) = props.icon {
                    div { class: "flex-shrink-0",
                        {icon}
                    }
                }
                div { class: if props.icon.is_some() { "ml-3" } else { "" },
                    h4 { class: "text-sm font-medium text-gray-900 dark:text-white",
                        "{props.title}"
                    }
                    p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
                        "{props.description}"
                    }
                }
            }
        }
    }
}

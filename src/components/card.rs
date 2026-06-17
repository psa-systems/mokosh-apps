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
    let base_class = "bg-surface rounded-lg shadow border border-line";
    let class = format!("{} {}", base_class, props.class);

    let has_header = !props.title.is_empty();
    // The header always self-pads (px-6 pt-6 pb-4) so its title and
    // actions stay aligned with table cells below regardless of whether
    // the card body keeps its own padding. The body section is padded
    // separately, so a full-bleed table (padding: false) sits flush
    // edge-to-edge while the header stays neatly inset.
    let body_class = if props.padding {
        if has_header {
            "px-6 pb-6 pt-4"
        } else {
            "p-6"
        }
    } else {
        ""
    };

    rsx! {
        div { class: "{class}",
            if has_header {
                CardHeader {
                    title: props.title,
                    actions: props.actions,
                }
            }
            div { class: "{body_class}",
                {props.children}
            }
        }
    }
}

/// Card header component
#[derive(Props, Clone, PartialEq)]
pub struct CardHeaderProps {
    title: String,
    actions: Option<Element>,
    #[props(default)]
    class: String,
}

#[component]
pub fn CardHeader(props: CardHeaderProps) -> Element {
    let class = format!(
        "flex items-center justify-between px-6 pt-6 pb-4 border-b border-line {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div {
                h3 { class: "text-lg font-medium text-content",
                    "{props.title}"
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
        "bg-surface rounded-lg shadow border border-line p-6 {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div { class: "flex items-center",
                if let Some(ref icon) = props.icon {
                    div { class: "flex-shrink-0 p-3 bg-accent-100 dark:bg-accent-900 rounded-full",
                        {icon}
                    }
                }
                div { class: if props.icon.is_some() { "ml-4" } else { "" },
                    p { class: "text-sm font-medium text-muted",
                        "{props.label}"
                    }
                    div { class: "flex items-baseline",
                        p { class: "text-2xl font-semibold text-content",
                            "{props.value}"
                        }
                        if !props.change.is_empty() {
                            // P2-17: was `text-sm` (~11px) next to a 32px
                            // stat number — easy to miss. Bump to text-base
                            // and a heavier weight so the delta reads at
                            // a glance.
                            span { class: "ml-2 text-base font-semibold {change_class}",
                                "{props.change}"
                            }
                        }
                    }
                }
            }
        }
    }
}

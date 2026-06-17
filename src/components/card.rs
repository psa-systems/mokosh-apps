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

/// Semantic tone for a [`StatCard`]'s icon container. Lets a stat that
/// carries meaning (an SLA breach, an at-risk warning) tint its icon
/// circle accordingly instead of every stat sitting in the same accent
/// circle, which previously left e.g. a red icon stranded inside an
/// accent-colored container. The tone drives both the container
/// background and the `currentColor` the icon inherits, so callers pass
/// an uncolored icon and the tone colors the glyph too.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum StatCardTone {
    #[default]
    Accent,
    Warning,
    Danger,
}

impl StatCardTone {
    fn icon_container_class(self) -> &'static str {
        match self {
            StatCardTone::Accent => "bg-accent-100 dark:bg-accent-900 text-accent",
            StatCardTone::Warning => {
                "bg-yellow-100 dark:bg-yellow-900/40 text-yellow-600 dark:text-yellow-400"
            }
            StatCardTone::Danger => "bg-red-100 dark:bg-red-900/40 text-red-600 dark:text-red-400",
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
    /// Semantic tone for the icon container (default accent). The icon
    /// inherits this color via `currentColor`, so the caller's icon
    /// should not set its own `text-*` color.
    #[props(default)]
    icon_tone: StatCardTone,
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
    let icon_container = props.icon_tone.icon_container_class();

    rsx! {
        div { class: "{class}",
            div { class: "flex items-center",
                if let Some(ref icon) = props.icon {
                    div { class: "flex-shrink-0 p-3 rounded-full {icon_container}",
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

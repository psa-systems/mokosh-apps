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
    /// PMS-765: optional line under the title, inside the header.
    ///
    /// A card that needs a sentence of explanation used to put one at the top
    /// of its body, where it lands under the header's rule with whatever
    /// padding that card happens to have: none at all on a `padding: false`
    /// card, which is how the request-form panel ended up with small grey text
    /// jammed between the header rule and a table header row. Here it is part
    /// of the heading, above the rule, at the size the rest of the app uses for
    /// descriptive copy.
    #[props(default)]
    subtitle: String,
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
                    subtitle: props.subtitle,
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
    /// See [`CardProps::subtitle`]. Empty renders nothing at all, so a card
    /// without one keeps the header it already had.
    #[props(default)]
    subtitle: String,
    actions: Option<Element>,
    #[props(default)]
    class: String,
}

#[component]
pub fn CardHeader(props: CardHeaderProps) -> Element {
    // `items-start` rather than `items-center` once a subtitle is in play: with
    // two lines of heading, centring the actions against the pair leaves them
    // floating beside the description instead of level with the title.
    let align = if props.subtitle.is_empty() {
        "items-center"
    } else {
        "items-start"
    };
    let class = format!(
        "flex {align} justify-between px-6 pt-6 pb-4 border-b border-line {}",
        props.class
    );

    rsx! {
        div { class: "{class}",
            div {
                h3 { class: "text-lg font-medium text-content",
                    "{props.title}"
                }
                if !props.subtitle.is_empty() {
                    p { class: "mt-1 text-sm text-muted",
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

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::prelude::VirtualDom;

    /// Padded on purpose: the body element then carries a class of its own
    /// (`px-6 pb-6 pt-4`), which is what lets the test tell "inside the header"
    /// from "first thing in the body". Those two are indistinguishable by
    /// document order alone, and the bug was exactly the second one.
    #[component]
    fn WithSubtitle() -> Element {
        rsx! {
            Card {
                title: "Recently sent".to_string(),
                subtitle: "SUBTITLE-MARKER".to_string(),
                "BODY-MARKER"
            }
        }
    }

    #[component]
    fn WithoutSubtitle() -> Element {
        rsx! {
            Card { title: "Recently sent".to_string(), "BODY-MARKER" }
        }
    }

    /// PMS-765 regression: a card's description belongs in the header, above
    /// the rule. As the first thing in the body it had no space above it (a
    /// `padding: false` card gives its body none at all) and a table header row
    /// directly below, so it read as small grey text wedged between two lines.
    #[test]
    fn a_subtitle_sits_in_the_header_not_at_the_top_of_the_body() {
        let mut dom = VirtualDom::new(WithSubtitle);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);

        let subtitle = html.find("SUBTITLE-MARKER").expect("the subtitle renders");
        let body_element = html
            .find("px-6 pb-6 pt-4")
            .expect("the padded body element is still there");
        let body = html.find("BODY-MARKER").expect("the body renders");

        assert!(
            subtitle < body_element,
            "the subtitle must come before the body element even opens, or it is \
             back inside the body where it started; got: {html}"
        );
        assert!(
            body_element < body,
            "and the body content is inside the body"
        );
        assert!(
            html.contains("mt-1 text-sm text-muted"),
            "descriptive copy is text-sm with room above it, not a size smaller \
             and flush; got: {html}"
        );
        assert!(
            html.contains("items-start"),
            "a two-line heading aligns its actions to the title, not to the pair; \
             got: {html}"
        );
    }

    /// The prop is optional, and a card without one keeps the header it had.
    #[test]
    fn no_subtitle_leaves_the_header_alone() {
        let mut dom = VirtualDom::new(WithoutSubtitle);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);

        assert!(!html.contains("SUBTITLE-MARKER"));
        assert!(!html.contains("mt-1 text-sm text-muted"));
        assert!(
            html.contains("items-center"),
            "a one-line heading still centres its actions against the title; got: {html}"
        );
    }
}

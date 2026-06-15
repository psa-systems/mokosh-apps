//! Render Markdown text for display (PMS-309).
//!
//! Thin wrapper over [`crate::utils::markdown::render_markdown`] (which
//! sanitizes with ammonia) plus Tailwind `prose` styling, so detail-page
//! descriptions render the same way KB article bodies do. Use for any
//! read-only free-text field whose authors may write Markdown.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct MarkdownProps {
    /// Raw Markdown source.
    content: String,
    /// Extra classes appended to the `prose` wrapper.
    #[props(default)]
    class: String,
}

#[component]
pub fn Markdown(props: MarkdownProps) -> Element {
    rsx! {
        div {
            class: "prose dark:prose-invert max-w-none {props.class}",
            dangerous_inner_html: crate::utils::markdown::render_markdown(&props.content),
        }
    }
}

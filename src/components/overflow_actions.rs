//! Header action cluster that shows its children inline on wider rows
//! and collapses them into a `...` dropdown when the row is too narrow
//! to fit them, so they never overflow the container. Uses the `sm`
//! breakpoint as the collapse threshold rather than runtime measurement.

use dioxus::prelude::*;

use super::popover::Popover;

#[component]
pub fn OverflowActions(children: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        // Inline on >= sm.
        div { class: "hidden sm:flex items-center gap-2", {children.clone()} }
        // Collapsed menu on < sm.
        div { class: "sm:hidden",
            Popover {
                open: open(),
                label: "More actions",
                title: "More",
                trigger_class: "px-2 py-1 text-muted hover:text-content",
                trigger: rsx! { "\u{22EF}" },
                width: "w-48",
                ontoggle: move |_| open.toggle(),
                onclose: move |_| open.set(false),
                div { class: "flex flex-col gap-2", {children} }
            }
        }
    }
}

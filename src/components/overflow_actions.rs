//! Header action cluster that shows its children inline on wider rows
//! and collapses them into a `...` dropdown when the row is too narrow
//! to fit them, so they never overflow the container. Uses the `sm`
//! breakpoint as the collapse threshold rather than runtime measurement.

use dioxus::prelude::*;

#[component]
pub fn OverflowActions(children: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        // Inline on >= sm.
        div { class: "hidden sm:flex items-center gap-2", {children.clone()} }
        // Collapsed menu on < sm.
        div { class: "sm:hidden relative",
            button {
                class: "px-2 py-1 text-gray-500 hover:text-gray-700",
                title: "More",
                onclick: move |_| open.toggle(),
                "\u{22EF}"
            }
            if open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| open.set(false),
                }
                div { class: "absolute right-0 z-50 mt-1 w-48 rounded-md bg-white dark:bg-gray-800 shadow-lg ring-1 ring-black/5 p-2 flex flex-col gap-2",
                    {children}
                }
            }
        }
    }
}

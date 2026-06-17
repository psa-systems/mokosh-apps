//! A collapsible side rail used by the KB reading view. On wide screens
//! it shows inline with a chevron toggle (collapsed state owned and
//! persisted by the parent); on narrow screens it becomes an edge handle
//! that opens an overlay. The caller owns `open_overlay` so only one rail
//! overlay is open at a time.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum RailSide {
    Left,
    Right,
}

#[component]
pub fn CollapsibleRail(
    side: RailSide,
    /// Wide-screen collapsed state (persisted by the parent).
    collapsed: Signal<bool>,
    /// Which rail's overlay is open on narrow screens (shared across rails).
    open_overlay: Signal<Option<RailSide>>,
    children: Element,
) -> Element {
    // `#[component]` destructures props into immutable bindings; rebind to
    // `mut` so the onclick closures can call `Signal::set` (which takes
    // `&mut self`). Signals are `Copy`, so each `move` closure captures a
    // mutable copy. Matches the DensityToggle / ReadModeButton convention.
    let mut collapsed = collapsed;
    let mut open_overlay = open_overlay;
    let is_overlay_open = open_overlay() == Some(side);
    let chevron_collapse = match side {
        RailSide::Left => "\u{2039}",  // single left-pointing angle quote
        RailSide::Right => "\u{203A}", // single right-pointing angle quote
    };
    let chevron_expand = match side {
        RailSide::Left => "\u{203A}",
        RailSide::Right => "\u{2039}",
    };
    let overlay_pos = if side == RailSide::Left {
        "left-0"
    } else {
        "right-0"
    };
    rsx! {
        // Wide screens: inline rail, collapses to a thin handle.
        div { class: "hidden lg:block relative",
            if collapsed() {
                button {
                    class: "h-full px-1 text-subtle hover:text-content",
                    title: "Expand panel",
                    onclick: move |_| collapsed.set(false),
                    "{chevron_expand}"
                }
            } else {
                div { class: "w-64 shrink-0",
                    div { class: "flex justify-end",
                        button {
                            class: "px-1 text-subtle hover:text-content",
                            title: "Collapse panel",
                            onclick: move |_| collapsed.set(true),
                            "{chevron_collapse}"
                        }
                    }
                    {children.clone()}
                }
            }
        }
        // Narrow screens: edge handle + overlay.
        div { class: "lg:hidden",
            button {
                class: "px-2 py-1 text-muted",
                title: "Open panel",
                onclick: move |_| {
                    if is_overlay_open {
                        open_overlay.set(None);
                    } else {
                        open_overlay.set(Some(side));
                    }
                },
                "{chevron_expand}"
            }
            if is_overlay_open {
                div {
                    class: "fixed inset-0 z-40 bg-black/30",
                    onclick: move |_| open_overlay.set(None),
                }
                div {
                    class: "fixed z-50 top-0 bottom-0 w-72 bg-surface shadow-xl p-4 overflow-y-auto {overlay_pos}",
                    div { class: "flex justify-end",
                        button {
                            class: "text-subtle",
                            onclick: move |_| open_overlay.set(None),
                            "\u{2715}"
                        }
                    }
                    {children}
                }
            }
        }
    }
}

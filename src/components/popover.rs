//! Shared trigger+panel popover surface (MAPPS-794).
//!
//! Before this, each action menu (the header overflow menu, a contacts row
//! menu, the KB article menu) built its own `.dropdown-panel` trigger/panel
//! pair by hand, so the offset, z-index and padding drifted between call
//! sites and only `UserMenu` carried the full `aria-expanded` /
//! `aria-haspopup` / `role="menu"` triad. `Popover` is the one place that
//! owns the geometry and the ARIA triad; every trigger/panel pair in the app
//! (the three shell menus in `layout.rs` and `tenant_switcher.rs`, and the
//! three action menus) renders through it. Open/close state stays with the
//! caller (some need it to close on route change or after an action), so
//! `Popover` is controlled: it takes `open` plus `ontoggle`/`onclose`.
//!
//! MAPPS-508 adds an optional keydown handler on the wrapper `div` so a
//! menu whose caller uses [`use_dropdown_nav`](crate::hooks::dropdown_nav)
//! can wire Escape + arrow movement without a per-site DOM listener.

use dioxus::prelude::*;

/// Props for [`Popover`].
#[derive(Props, Clone, PartialEq)]
pub struct PopoverProps {
    /// Whether the panel is currently shown.
    open: bool,
    /// Accessible name for the trigger button. Emitted as `aria-label` and,
    /// unless `title` overrides it, the hover tooltip too.
    label: String,
    /// Hover/focus tooltip, when it differs from `label` (e.g. a row menu
    /// whose tooltip reads "Actions" but whose accessible name is more
    /// specific, "Row actions").
    #[props(default)]
    title: Option<String>,
    /// The trigger button's own visual classes (background, hover, shape).
    /// `Popover` appends only the geometry it owns to the panel, not the
    /// trigger, since every site's button chrome differs.
    trigger_class: String,
    /// Content rendered inside the trigger button (icon, label text, caret).
    trigger: Element,
    /// Content rendered inside the panel.
    children: Element,
    /// Panel width class (e.g. "w-56"). Kept explicit per call site since
    /// panels are sized to their content; everything else about the panel
    /// geometry is shared.
    width: String,
    /// Fired when the trigger is clicked.
    ontoggle: EventHandler<MouseEvent>,
    /// Fired when the backdrop behind an open panel is clicked.
    onclose: EventHandler<MouseEvent>,
    /// MAPPS-508: optional keydown handler on the wrapper `div`. A caller
    /// wiring `use_dropdown_nav("...").menu()` passes the hook's
    /// `keydown_menu` here so Escape closes the panel and Up/Down move
    /// the internal highlight without a per-site listener. `None` keeps
    /// the pre-508 behaviour: no keydown listener on the wrapper.
    #[props(default)]
    onkeydown: Option<EventHandler<KeyboardEvent>>,
}

/// The one popover surface: a trigger button carrying the ARIA triad
/// (`aria-haspopup="menu"`, `aria-expanded`, and `role="menu"` on the panel)
/// plus one shared offset, z-index and padding for the panel itself.
#[component]
pub fn Popover(props: PopoverProps) -> Element {
    let title = props.title.clone().unwrap_or_else(|| props.label.clone());
    let onkeydown = props.onkeydown;
    rsx! {
        div {
            class: "relative",
            // MAPPS-508: keydown bubbles up from the focused row (or the
            // trigger) to this wrapper, and the caller's `keydown_menu`
            // sees Escape / Up / Down. Emitted only when a caller opts in.
            onkeydown: move |e: KeyboardEvent| {
                if let Some(h) = onkeydown.as_ref() {
                    h.call(e);
                }
            },
            button {
                r#type: "button",
                class: "{props.trigger_class}",
                aria_label: "{props.label}",
                title: "{title}",
                aria_haspopup: "menu",
                aria_expanded: if props.open { "true" } else { "false" },
                onclick: move |e| props.ontoggle.call(e),
                {props.trigger}
            }
            if props.open {
                // Full-viewport click-catcher behind the panel, same z-index
                // pair every call site used before consolidation: closes the
                // panel on any click outside it without a document-level
                // listener that could leak.
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e| props.onclose.call(e),
                }
                div {
                    class: "dropdown-panel absolute right-0 mt-2 z-50 p-1 {props.width}",
                    role: "menu",
                    {props.children}
                }
            }
        }
    }
}

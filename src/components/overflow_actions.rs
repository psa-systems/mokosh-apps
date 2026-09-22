//! Header action cluster that shows its children inline on wider rows
//! and collapses them into a `...` dropdown when the row is too narrow
//! to fit them, so they never overflow the container. Uses the `sm`
//! breakpoint as the collapse threshold rather than runtime measurement.

use dioxus::prelude::*;

use super::popover::Popover;
use crate::hooks::dropdown_nav::use_dropdown_nav;

#[component]
pub fn OverflowActions(children: Element) -> Element {
    // MAPPS-508: shared keyboard contract via use_dropdown_nav in menu
    // mode - Escape closes, Up/Down move the internal highlight, Tab and
    // Enter fall through to the browser (Tab walks the menu buttons,
    // Enter fires the focused one).
    let mut nav = use_dropdown_nav("overflow-actions").menu();
    // The overflow menu wraps a caller-supplied `children` Element, so
    // the exact row count is not visible here. use_dropdown_nav's
    // decide() clamps a stale index anyway, and the two arrow keys only
    // ever return an index into 0..len; passing a conservative upper
    // bound is enough for the state machine to answer "some highlight",
    // and per the ticket the visual mark on a row is call-site work the
    // wrapper cannot do without knowing its rows.
    let row_count: usize = 8;
    rsx! {
        // Inline on >= sm.
        div { class: "hidden sm:flex items-center gap-2", {children.clone()} }
        // Collapsed menu on < sm.
        div { class: "sm:hidden",
            Popover {
                open: nav.is_open(),
                label: "More actions",
                title: "More",
                trigger_class: "px-2 py-1 text-muted hover:text-content",
                trigger: rsx! { "\u{22EF}" },
                width: "w-48",
                ontoggle: move |_| {
                    if nav.is_open() {
                        nav.close();
                    } else {
                        nav.open();
                    }
                },
                onclose: move |_| nav.close(),
                onkeydown: move |e: KeyboardEvent| {
                    nav.keydown_menu(&e, row_count);
                },
                div { class: "flex flex-col gap-2", role: "none", {children} }
            }
        }
    }
}

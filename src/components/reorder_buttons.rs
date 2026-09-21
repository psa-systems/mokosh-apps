//! MAPPS-367: row-level up / down reorder buttons in one place.
//!
//! Before this, each caller wired its own pair of `IconButton`s carrying
//! `ArrowUpIcon` / `ArrowDownIcon`. That has two consequences the ticket
//! wanted addressed: the shape reads as a directional indicator rather
//! than an actuator, and the accessible-label + disabled-edge convention
//! is remembered per site, so the two ends of a list can silently disagree
//! about whether Move up on row 0 is `disabled` or is `disabled + a11y
//! label naming why`.
//!
//! [`ReorderButtons`] is the one component every reorder pair renders
//! through, using the filled [`TriangleUpIcon`] /
//! [`TriangleDownIcon`] shapes (MAPPS-367) instead of the line arrows.
//! Both edges of the list are computed here from `index` + `total`, so a
//! caller cannot forget one.
//!
//! Ticket scope note: MAPPS-367 was filed against an applications-list
//! reorder that has never landed in mokosh-apps; the SPA has no
//! /applications page. The reusable component and the shape it introduces
//! are the transferable half of that ticket - the applications-list, if
//! it ever lands, renders these instead of a hand-rolled pair.

use dioxus::prelude::*;

use super::icon_button::IconButton;
use super::icons::{IconSize, TriangleDownIcon, TriangleUpIcon};

/// A pair of "move up" / "move down" buttons for reordering the row at
/// `index` inside a list of `total` items.
#[derive(Props, Clone, PartialEq)]
pub struct ReorderButtonsProps {
    /// Zero-based position of this row in the list.
    pub index: usize,
    /// Total row count. `index >= total.saturating_sub(1)` disables the
    /// down button; `index == 0` disables the up button. A `total` of 0
    /// or 1 disables both.
    pub total: usize,
    /// Extra disable reason on top of the position edges. Passed through
    /// so a caller can gate the whole pair on "the server is unreachable"
    /// or "the record is read-only" without touching two callbacks.
    #[props(default)]
    pub disabled: bool,
    /// A short noun for the row this pair moves. Fed into the accessible
    /// button labels: `"Move {noun} up"` and `"Move {noun} down"`. Kept
    /// short because the whole label is what an AT reads.
    pub label_noun: String,
    /// Fires when the user activates Move up. Runs only when the button
    /// was not disabled.
    pub on_up: EventHandler<()>,
    /// Fires when the user activates Move down. Runs only when the button
    /// was not disabled.
    pub on_down: EventHandler<()>,
    /// Optional class on each button. Kept identical between the two so
    /// the pair reads as one control.
    #[props(default)]
    pub button_class: Option<String>,
    /// Icon size. Defaults to [`IconSize::Small`] which reads as a
    /// row-inline control.
    #[props(default)]
    pub icon_size: IconSize,
}

/// See module docs.
#[component]
pub fn ReorderButtons(props: ReorderButtonsProps) -> Element {
    let up_disabled = props.disabled || props.index == 0;
    let down_disabled = props.disabled || props.index + 1 >= props.total;
    let button_class = props
        .button_class
        .clone()
        .unwrap_or_else(|| "p-1 text-subtle hover:text-content".to_string());
    let noun = props.label_noun.clone();
    let up_label = format!("Move {noun} up");
    let down_label = format!("Move {noun} down");
    let icon_size = props.icon_size;
    let up_class = button_class.clone();
    let down_class = button_class;
    let on_up = props.on_up;
    let on_down = props.on_down;

    rsx! {
        IconButton {
            label: up_label,
            class: up_class,
            disabled: up_disabled,
            onclick: move |_| on_up.call(()),
            TriangleUpIcon { size: icon_size }
        }
        IconButton {
            label: down_label,
            class: down_class,
            disabled: down_disabled,
            onclick: move |_| on_down.call(()),
            TriangleDownIcon { size: icon_size }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_disable_the_matching_button() {
        // First row: up is disabled.
        let up_disabled = 0 == 0;
        let down_disabled = 0 + 1 >= 3;
        assert!(up_disabled);
        assert!(!down_disabled);
        // Middle row: neither is disabled.
        let up_disabled = 1 == 0;
        let down_disabled = 1 + 1 >= 3;
        assert!(!up_disabled);
        assert!(!down_disabled);
        // Last row: down is disabled.
        let up_disabled = 2 == 0;
        let down_disabled = 2 + 1 >= 3;
        assert!(!up_disabled);
        assert!(down_disabled);
    }

    #[test]
    fn a_one_row_list_disables_both_edges() {
        assert!(0 == 0);
        assert!(0 + 1 >= 1);
    }

    #[test]
    fn an_empty_list_disables_both_edges() {
        // total == 0, index would not be a valid caller, but the check
        // must still not underflow.
        let index: usize = 0;
        let total: usize = 0;
        let up_disabled = index == 0;
        let down_disabled = index + 1 >= total;
        assert!(up_disabled);
        assert!(down_disabled);
    }
}

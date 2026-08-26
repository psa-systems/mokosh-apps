//! Change-history entry rendering (MAPPS-596).
//!
//! An entity's change history is rendered on four surfaces: the project and
//! task panes (`pages/projects.rs`), the asset pane (`pages/assets.rs`) and
//! the ticket journal (`pages/tickets.rs`). All four print the same thing,
//! and before this component three of them printed it from the same block of
//! rsx, copy-pasted.
//!
//! ## Why the before/after collapses
//!
//! `fmt_change_value` cuts a value at 160 characters, so a description edit
//! renders up to 160 characters of old plus 160 of new. In a sidebar column
//! roughly 45 characters wide that is about fifteen wrapped lines for one
//! entry, and five consecutive description edits are the entire pane. That is
//! what MAPPS-596 reported.
//!
//! It collapses BY SIZE, not always. A status change is one short line, and
//! putting it behind a click would make that entry worse to serve the long
//! ones. So the before/after stays inline while the block is small and moves
//! behind a `Details` toggle once it is not; [`is_large`] is the rule and
//! [`DETAIL_CHARS`] is the number.
//!
//! Expanding one still shows both whole values side by side, which for a long
//! description is legible only in the sense that it is now optional. MAPPS-601
//! replaces that with a word-level diff.

use dioxus::prelude::*;

use crate::components::{ChevronDownIcon, ChevronRightIcon, IconSize};

/// Total before/after characters an entry may carry before its detail moves
/// behind the toggle.
///
/// Sized off the two shapes that actually occur. A single-field edit of a
/// short column ("Open" to "Closed", a priority, an assignee name) is well
/// under this and stays visible. A description or note edit is at least twice
/// it, usually far more, and collapses. A multi-field edit of short columns
/// sits between the two and collapses once it is long enough to push the
/// entries under it off the screen, which is the thing being fixed.
pub const DETAIL_CHARS: usize = 120;

/// One field's before and after, already formatted for display.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeLine {
    /// Human-readable field name, from `audit::title_field`.
    pub field: String,
    /// Formatted previous value, from `audit::fmt_change_value`.
    pub old: String,
    /// Formatted new value.
    pub new: String,
}

impl ChangeLine {
    /// Build a renderable line, or `None` when there is nothing readable to
    /// show.
    ///
    /// A foreign-key swap is recorded as two UUIDs, which
    /// `audit::fmt_change_value` renders as `(reference)` on both sides. That
    /// carries no information at all, so the line is dropped rather than
    /// printed, which is what every pane already did inline.
    pub fn build(
        field: &str,
        old: &Option<serde_json::Value>,
        new: &Option<serde_json::Value>,
    ) -> Option<Self> {
        let old = crate::modules::audit::fmt_change_value(old);
        let new = crate::modules::audit::fmt_change_value(new);
        if old == "(reference)" && new == "(reference)" {
            return None;
        }
        Some(Self {
            field: crate::modules::audit::title_field(field),
            old,
            new,
        })
    }

    /// Characters this line contributes to the size decision. The field name
    /// is excluded: it is short, bounded by the column name, and the same on
    /// every entry for a given field, so it is not what makes an entry large.
    fn weight(&self) -> usize {
        self.old.chars().count() + self.new.chars().count()
    }
}

/// Whether this entry's before/after belongs behind the toggle.
pub fn is_large(changes: &[ChangeLine]) -> bool {
    changes.iter().map(ChangeLine::weight).sum::<usize>() > DETAIL_CHARS
}

/// The before/after block for one history entry: inline when small, behind a
/// `Details` toggle when not.
#[derive(Props, Clone, PartialEq)]
pub struct ChangeDetailsProps {
    /// Lines to show. Empty renders nothing at all, not an empty toggle.
    pub changes: Vec<ChangeLine>,
}

#[component]
pub fn ChangeDetails(props: ChangeDetailsProps) -> Element {
    // Per-instance id so the toggle's `aria-controls` names this entry's body
    // and not the one above it. Atomic counter; wasm is single-threaded.
    let body_id = use_hook(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        format!("ch-detail-{}", NEXT.fetch_add(1, Ordering::Relaxed))
    });
    let mut open = use_signal(|| false);

    if props.changes.is_empty() {
        return rsx! {};
    }

    let collapsed = is_large(&props.changes);
    let show = !collapsed || open();

    rsx! {
        if collapsed {
            button {
                r#type: "button",
                class: "mt-1 flex items-center gap-1 text-xs text-muted hover:text-content",
                aria_expanded: if open() { "true" } else { "false" },
                aria_controls: "{body_id}",
                onclick: move |_| {
                    let next = !open();
                    open.set(next);
                },
                if open() {
                    ChevronDownIcon { size: IconSize::Small, class: "text-subtle".to_string() }
                } else {
                    ChevronRightIcon { size: IconSize::Small, class: "text-subtle".to_string() }
                }
                "Details"
            }
        }
        if show {
            div { id: "{body_id}",
                for c in props.changes.iter() {
                    p { class: "text-xs text-muted mt-1",
                        span { class: "font-medium", "{c.field}: " }
                        span { class: "line-through text-subtle", "{c.old}" }
                        " → "
                        span { "{c.new}" }
                    }
                }
            }
        }
    }
}

/// One row of a change-history pane: what happened, who did it, when, and the
/// before/after underneath.
#[derive(Props, Clone, PartialEq)]
pub struct ChangeHistoryEntryProps {
    /// What happened, already phrased: "Updated: Description".
    pub headline: String,
    /// Display name of whoever did it. Empty omits the line rather than
    /// printing a dash or a UUID.
    #[props(default)]
    pub who: String,
    /// Formatted timestamp, right-aligned.
    pub when: String,
    /// The before/after lines.
    #[props(default)]
    pub changes: Vec<ChangeLine>,
}

#[component]
pub fn ChangeHistoryEntry(props: ChangeHistoryEntryProps) -> Element {
    rsx! {
        div { class: "flex justify-between gap-2",
            div { class: "min-w-0",
                p { class: "text-content", "{props.headline}" }
                if !props.who.is_empty() {
                    p { class: "text-xs text-subtle", "by {props.who}" }
                }
                ChangeDetails { changes: props.changes.clone() }
            }
            span { class: "text-subtle whitespace-nowrap", "{props.when}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(field: &str, old: &str, new: &str) -> ChangeLine {
        ChangeLine {
            field: field.to_string(),
            old: old.to_string(),
            new: new.to_string(),
        }
    }

    /// The entry the reporter screenshotted: a description edit, 160
    /// characters a side once truncated. It collapses.
    #[test]
    fn a_description_edit_collapses() {
        let long = "x".repeat(160);
        assert!(is_large(&[line("Description", &long, &long)]));
    }

    /// And the entry that must NOT collapse. Hiding "Open to Closed" behind a
    /// click to solve a problem it does not have would make the pane worse for
    /// the change people read most.
    #[test]
    fn a_short_change_stays_visible() {
        assert!(!is_large(&[line("Status", "Open", "Closed")]));
        assert!(!is_large(&[line("Priority", "Normal", "High")]));
        assert!(!is_large(&[line(
            "Assigned to",
            "Ada Lovelace",
            "Alan Turing"
        )]));
    }

    /// Size is measured across every line on the entry, not per line: five
    /// short fields changed at once is a large block even though no single
    /// line is.
    #[test]
    fn size_is_measured_across_the_whole_entry() {
        let one = line("Status", "Open", "Closed");
        assert!(!is_large(std::slice::from_ref(&one)));
        let many: Vec<ChangeLine> =
            std::iter::repeat_n(line("Notes", &"y".repeat(30), &"z".repeat(30)), 3).collect();
        assert!(is_large(&many), "3 x 60 chars is over the threshold");
    }

    /// The threshold is a decision, so it is pinned. Moving it is allowed;
    /// moving it by accident is not.
    #[test]
    fn the_threshold_is_where_it_says_it_is() {
        assert_eq!(DETAIL_CHARS, 120);
        let at = line("F", &"x".repeat(60), &"y".repeat(60));
        assert_eq!(at.weight(), DETAIL_CHARS);
        assert!(!is_large(&[at]), "exactly at the threshold stays inline");
        let over = line("F", &"x".repeat(61), &"y".repeat(60));
        assert!(is_large(&[over]));
    }

    /// A FK swap is two UUIDs and reads as "(reference) → (reference)", which
    /// tells the reader nothing. Dropped, as every pane already did inline.
    #[test]
    fn a_bare_reference_swap_produces_no_line() {
        let a = json!("3f2504e0-4f89-41d3-9a0c-0305e82c3301");
        let b = json!("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert!(ChangeLine::build("company_id", &Some(a.clone()), &Some(b)).is_none());
        // One side readable is still worth showing.
        assert!(ChangeLine::build("company_id", &Some(a), &None).is_some());
    }

    /// The field name is titled and the values formatted on the way in, so a
    /// caller hands over the raw audit row and gets display text back.
    #[test]
    fn build_formats_both_sides() {
        let l = ChangeLine::build("due_date", &Some(json!("2026-03-01")), &None)
            .expect("a readable change");
        assert_eq!(l.field, "Due date");
        assert_eq!(l.old, "Mar 1, 2026");
        assert_eq!(l.new, "(empty)");
    }

    /// An entry with nothing readable on it renders no toggle. Without this
    /// the pane would grow a "Details" button that opens onto nothing.
    #[test]
    fn no_changes_means_no_toggle() {
        let src = include_str!("change_history.rs");
        let code = &src[..src.find("mod tests").expect("tests are in this file")];
        assert!(
            code.contains("if props.changes.is_empty() {\n        return rsx! {};\n    }"),
            "an empty change list returns before anything is rendered"
        );
    }

    /// Each entry owns its own open state and its own body id, so opening one
    /// does not open the entry above it and `aria-controls` points at the
    /// right region. A shared id or a hoisted signal would do both.
    #[test]
    fn each_entry_toggles_independently() {
        let src = include_str!("change_history.rs");
        let code = &src[..src.find("mod tests").expect("tests are in this file")];
        assert!(
            code.contains("let mut open = use_signal(|| false);"),
            "the open state is per component instance"
        );
        assert!(
            code.contains("NEXT.fetch_add(1, Ordering::Relaxed)"),
            "and so is the id the toggle points at"
        );
    }

    /// The toggle is a real button carrying its state, not a clickable span.
    /// It is the only way to reach the content, so a keyboard user who cannot
    /// reach it cannot read the history at all.
    #[test]
    fn the_toggle_is_reachable_and_announces_itself() {
        let src = include_str!("change_history.rs");
        let code = &src[..src.find("mod tests").expect("tests are in this file")];
        assert!(code.contains("r#type: \"button\""), "a real button");
        assert!(code.contains("aria_expanded:"), "state is announced");
        assert!(
            code.contains("aria_controls: \"{body_id}\""),
            "and names its body"
        );
    }
}

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
//! A description edit used to render up to 160 characters of old plus 160 of
//! new: in a sidebar column roughly 45 characters wide, about fifteen wrapped
//! lines for one entry, and five consecutive description edits were the entire
//! pane. That is what MAPPS-596 reported.
//!
//! It collapses BY SIZE, not always. A status change is one short line, and
//! putting it behind a click would make that entry worse to serve the long
//! ones. So the before/after stays inline while the block is small and moves
//! behind a `Details` toggle once it is not; [`is_large`] is the rule and
//! [`DETAIL_CHARS`] is the number.
//!
//! MAPPS-601 replaced the raw before/after with a word-level diff, so an
//! expanded entry shows the clause that changed rather than two near-identical
//! strings the reader has to compare by eye. Two consequences worth knowing:
//! the 160-character cap moved off `fmt_change_value` and onto the path that
//! shows a value whole, because truncating each side independently is what made
//! the comparison impossible; and [`is_large`] now measures what will be
//! RENDERED, so a long description edit that reduces to one clause stays on
//! screen rather than collapsing for a size it no longer has.

use dioxus::prelude::*;

use crate::components::{ChevronDownIcon, ChevronRightIcon, IconSize};
use crate::utils::word_diff::Piece;

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

    /// How this line will actually be rendered: a word diff when the two
    /// versions share enough to be worth interleaving, both values whole when
    /// they do not.
    ///
    /// MAPPS-601: computed at render rather than stored on the struct, which
    /// stays cheap to clone and compare. Every entry runs it at most twice, and
    /// the values are the ones the audit row already holds.
    pub fn render(&self) -> Rendering {
        match crate::utils::word_diff::diff_words(&self.old, &self.new) {
            Some(pieces) => Rendering::Diff(crate::utils::word_diff::elide(pieces, CONTEXT_WORDS)),
            None => Rendering::Replacement {
                old: crate::modules::audit::shorten(&self.old),
                new: crate::modules::audit::shorten(&self.new),
            },
        }
    }

    /// Characters this line contributes to the size decision. The field name
    /// is excluded: it is short, bounded by the column name, and the same on
    /// every entry for a given field, so it is not what makes an entry large.
    ///
    /// MAPPS-601: measured on what will be SHOWN, not on the stored values.
    /// The question `is_large` answers is whether this entry will dominate the
    /// pane, and after the diff that depends on how much changed rather than on
    /// how long the description is. Measuring the raw values would collapse an
    /// entry that renders as three words, which is both worse for the reader
    /// and plainly wrong.
    fn weight(&self) -> usize {
        match self.render() {
            Rendering::Diff(pieces) => pieces
                .iter()
                .map(|p| p.text().chars().count())
                .sum::<usize>(),
            Rendering::Replacement { old, new } => old.chars().count() + new.chars().count(),
        }
    }
}

/// How much unchanged text to keep either side of a change, in words.
///
/// Enough to locate the edit in a sentence, few enough that a one-word change
/// in a long description does not print the description twice.
const CONTEXT_WORDS: usize = 6;

/// What one changed field looks like on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Rendering {
    /// The two versions share enough to interleave: unchanged text plain,
    /// removals struck through, additions marked.
    Diff(Vec<crate::utils::word_diff::Piece>),
    /// One value replaced the other outright, shown whole. A status change, a
    /// cleared field, or a description rewritten from scratch.
    Replacement { old: String, new: String },
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
                    p { class: "text-xs text-muted mt-1 whitespace-pre-wrap",
                        span { class: "font-medium", "{c.field}: " }
                        match c.render() {
                            Rendering::Replacement { old, new } => rsx! {
                                span { class: "line-through text-subtle", "{old}" }
                                " → "
                                span { "{new}" }
                            },
                            Rendering::Diff(pieces) => rsx! {
                                for piece in pieces.iter() {
                                    match piece {
                                        // Struck through AND tinted, because
                                        // colour alone does not separate a
                                        // removal from an addition for a
                                        // red/green colour-blind reader.
                                        Piece::Removed(t) => rsx! {
                                            span {
                                                class: "line-through bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-200 rounded-sm px-0.5",
                                                "{t}"
                                            }
                                        },
                                        Piece::Added(t) => rsx! {
                                            span {
                                                class: "bg-green-100 text-green-800 dark:bg-green-900/50 dark:text-green-200 rounded-sm px-0.5",
                                                "{t}"
                                            }
                                        },
                                        Piece::Same(t) => rsx! {
                                            span { class: "text-subtle", "{t}" }
                                        },
                                    }
                                }
                            },
                        }
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

    /// A long description edit is what MAPPS-596 was reported for, and after
    /// MAPPS-601 it is no longer large: the diff reduces it to the clause that
    /// changed plus six words of context either side. Keeping it inline is the
    /// point of the diff, not a regression of the collapse.
    #[test]
    fn a_description_edit_reduces_to_its_clause_and_stays_visible() {
        let old = format!(
            "Rachel's email is not working. {} It appears all DNS entries are gone.",
            "The domain expired and then reactivated. ".repeat(6)
        );
        let new = old.replace("all DNS entries", "every MX record");
        let l = line("Description", &old, &new);
        let Rendering::Diff(pieces) = l.render() else {
            panic!("two values this close are worth diffing: {:?}", l.render());
        };
        assert!(
            pieces.iter().any(|p| matches!(p, Piece::Removed(_))),
            "{pieces:?}"
        );
        assert!(!is_large(&[l]), "the diff is small, so it stays on screen");
    }

    /// A description REPLACED rather than edited has no diff to show, so both
    /// values are rendered whole and the entry is large. That is the case the
    /// toggle still exists for.
    #[test]
    fn a_wholesale_rewrite_still_collapses() {
        let old = "alpha bravo charlie delta echo foxtrot golf hotel india juliet ".repeat(4);
        let new = "one two three four five six seven eight nine ten eleven twelve ".repeat(4);
        let l = line("Description", &old, &new);
        assert!(
            matches!(l.render(), Rendering::Replacement { .. }),
            "nothing in common is a replacement, not an edit"
        );
        assert!(is_large(&[l]));
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

    /// A short field is a replacement, and goes on reading exactly as it did
    /// before the diff existed. Interleaving "Open" and "Closed" on their
    /// shared letters would be strictly worse.
    #[test]
    fn a_short_field_is_still_a_plain_replacement() {
        assert_eq!(
            line("Status", "Open", "Closed").render(),
            Rendering::Replacement {
                old: "Open".to_string(),
                new: "Closed".to_string(),
            }
        );
    }

    /// The 160-character cap moved off the formatter and onto the path that
    /// shows a value whole, so a replacement is still capped and a diff is
    /// bounded by elision instead.
    #[test]
    fn a_replacement_is_still_capped() {
        let old = "a".repeat(400);
        let new = "b".repeat(400);
        let Rendering::Replacement { old: o, new: n } = line("F", &old, &new).render() else {
            panic!("nothing in common");
        };
        assert!(o.ends_with('…') && n.ends_with('…'), "{o} / {n}");
        assert_eq!(o.chars().count(), crate::modules::audit::VALUE_CHARS + 1);
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
        // Two values with nothing in common, so both render whole and the
        // weight is simply their combined length.
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

    /// MAPPS-601: a removal is struck through AND tinted, and an addition is
    /// tinted a different way. Colour alone does not separate the two for a
    /// red/green colour-blind reader, which is a large share of the audience
    /// for a change log.
    #[test]
    fn a_removal_is_distinguishable_without_colour() {
        let src = include_str!("change_history.rs");
        let code = &src[..src.find("mod tests").expect("tests are in this file")];
        let removed = code
            .find("Piece::Removed(t) => rsx! {")
            .expect("the removal arm");
        let added = code
            .find("Piece::Added(t) => rsx! {")
            .expect("the addition arm");
        assert!(
            code[removed..added].contains("line-through"),
            "a removal carries the strike, not just the tint"
        );
        assert!(
            !code[added..].contains("line-through"),
            "and an addition does not"
        );
    }

    /// A Markdown value is full of newlines and runs of spaces, and the diff
    /// reassembles them exactly. Collapsing them at render would show the
    /// reader text that neither version contains.
    #[test]
    fn the_diff_keeps_the_whitespace_it_carried() {
        let src = include_str!("change_history.rs");
        let code = &src[..src.find("mod tests").expect("tests are in this file")];
        assert!(
            code.contains("whitespace-pre-wrap"),
            "the block preserves newlines and repeated spaces"
        );
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

    /// MAPPS-596: the before/after block existed three times, as the same rsx
    /// copy-pasted into the project, task and asset panes. `line-through` on a
    /// value is its signature, and it belongs to this component alone now. A
    /// page that inlines the block again gets its own collapse rule, or none.
    #[test]
    fn the_before_after_block_is_rendered_here_and_nowhere_else() {
        const PAGES: [(&str, &str); 3] = [
            ("pages/projects.rs", include_str!("../pages/projects.rs")),
            ("pages/assets.rs", include_str!("../pages/assets.rs")),
            ("pages/tickets.rs", include_str!("../pages/tickets.rs")),
        ];
        for (path, src) in PAGES {
            assert!(
                !src.contains("line-through text-subtle"),
                "{path} renders its own before/after block; it should render \
                 ChangeHistoryEntry or ChangeDetails instead"
            );
        }
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

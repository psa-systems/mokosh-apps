//! MAPPS-503: the one keyboard contract every typeahead dropdown follows.
//!
//! The app's five comboboxes (`CompanyPicker`, `ContactPicker`,
//! `AssetPicker`, `SuggestInput`, `GlobalSearch`) used to open only from
//! `oninput` and handle no keys at all, so a dropdown could not be opened by
//! focusing the field, arrowed through, or committed with Tab. Rather than
//! five copies of the same state machine (which is how `GlobalSearch` ended
//! up with Escape and nobody else did), they share [`use_dropdown_nav`].
//!
//! The contract, documented in `docs/form-conventions.md`:
//! focus or click opens, Up/Down move the highlight (clamped, never
//! wrapping), Enter takes the highlighted row, Tab takes the highlighted row
//! (or the first one) and moves to the next field, Escape closes without
//! committing.
//!
//! Handlers go on the field's wrapper `div`, never on the shared [`Input`]:
//! keydown bubbles up from the focused input, and MAPPS-347 already moved a
//! handler off `Input` because handlers there interfered with inline-error
//! rendering on the ticket-create form.
//!
//! [`Input`]: crate::components::Input

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;

/// What a key press means for a dropdown in a given state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavAction {
    /// Not ours: leave the key to the browser.
    Ignore,
    /// Open the list (if closed) and highlight this row. `None` when there
    /// are no rows to highlight.
    Open(Option<usize>),
    /// Take this row. `prevent_default` is false for Tab, whose default
    /// (focus moves on) is exactly what should happen after committing.
    Commit { index: usize, prevent_default: bool },
    /// Close without committing, leaving the typed text alone.
    Close,
}

/// Move the highlight one row, clamped at both ends rather than wrapping, so
/// holding Down parks on the last row instead of hiding it by jumping back to
/// the first. An out-of-range `active` (the row set shrank under it) clamps in
/// before it moves.
pub fn step_index(active: Option<usize>, len: usize, forward: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match active {
        // Nothing highlighted yet: both directions land on the first row.
        None => 0,
        Some(i) => {
            let i = i.min(len - 1);
            if forward {
                (i + 1).min(len - 1)
            } else {
                i.saturating_sub(1)
            }
        }
    })
}

/// The row Tab takes: the highlighted one, or the first row when nothing is
/// highlighted. `None` when the list has nothing to take.
pub fn commit_index(active: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(active.filter(|i| *i < len).unwrap_or(0))
}

/// The whole key state machine, free of Dioxus and the DOM so it is unit
/// testable. `len` counts every navigable row, including a trailing inline
/// "+ Create" action.
pub fn decide(key: &Key, shift: bool, open: bool, active: Option<usize>, len: usize) -> NavAction {
    match key {
        Key::ArrowDown => NavAction::Open(step_index(active, len, true)),
        Key::ArrowUp => NavAction::Open(step_index(active, len, false)),
        // Enter only ever takes a row the user actually highlighted; with no
        // highlight it stays the form's key.
        Key::Enter => match active.filter(|i| open && *i < len) {
            Some(index) => NavAction::Commit {
                index,
                prevent_default: true,
            },
            None => NavAction::Ignore,
        },
        // Shift+Tab backs out of the field, so committing on the way out
        // would be a surprise.
        Key::Tab if open && !shift => match commit_index(active, len) {
            Some(index) => NavAction::Commit {
                index,
                prevent_default: false,
            },
            None => NavAction::Ignore,
        },
        // Closed already is fine: Escape stays available for the call site's
        // own teardown (GlobalSearch collapses its entry, MAPPS-347).
        Key::Escape => NavAction::Close,
        _ => NavAction::Ignore,
    }
}

/// Per-instance suffix for the row / panel element ids, so two pickers on one
/// form (the contact form renders a `CompanyPicker` per company link) never
/// share an `aria-activedescendant` target.
static NEXT_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Open + highlight state for one dropdown, plus the handlers and the ARIA
/// ids that go with it. `Copy`, so it drops straight into `move` closures.
#[derive(Clone, Copy)]
pub struct DropdownNav {
    open: Signal<bool>,
    active: Signal<Option<usize>>,
    base: &'static str,
    seq: usize,
}

/// Hook it up once per combobox. `id_base` only has to be readable; the
/// instance number keeps the ids unique.
pub fn use_dropdown_nav(id_base: &'static str) -> DropdownNav {
    let open = use_signal(|| false);
    let active = use_signal(|| None);
    let seq = use_hook(|| NEXT_SEQ.fetch_add(1, Ordering::Relaxed));
    DropdownNav {
        open,
        active,
        base: id_base,
        seq,
    }
}

impl DropdownNav {
    /// Is the panel showing?
    pub fn is_open(&self) -> bool {
        *self.open.read()
    }

    /// The highlighted row, if any.
    pub fn active_index(&self) -> Option<usize> {
        *self.active.read()
    }

    /// Is this row the highlighted one?
    pub fn is_active(&self, index: usize) -> bool {
        self.active_index() == Some(index)
    }

    /// `id` of the panel, and the field wrapper's `aria-controls` target.
    pub fn panel_id(&self) -> String {
        format!("{}-{}-listbox", self.base, self.seq)
    }

    /// `id` of one row, and the `aria-activedescendant` target when active.
    pub fn row_id(&self, index: usize) -> String {
        format!("{}{index}", self.row_id_prefix())
    }

    /// The row-id prefix, for a panel whose rows are rendered by a child
    /// component that only knows its own slice of the list (`GlobalSearch`
    /// groups its hits into sections).
    pub fn row_id_prefix(&self) -> String {
        format!("{}-{}-opt-", self.base, self.seq)
    }

    /// `aria-activedescendant` for the field wrapper; `None` renders nothing.
    pub fn active_descendant(&self) -> Option<String> {
        self.active_index().map(|i| self.row_id(i))
    }

    /// `aria-expanded` for the field wrapper.
    pub fn expanded(&self) -> &'static str {
        if self.is_open() {
            "true"
        } else {
            "false"
        }
    }

    /// A row's class list: the call site's own recipe plus the highlight on
    /// the active row, which is the same `bg-surface-2` the hover state uses.
    pub fn row_class(&self, index: usize, base: &str) -> String {
        if self.is_active(index) {
            format!("{base} bg-surface-2")
        } else {
            base.to_string()
        }
    }

    /// `aria-selected` for a row.
    pub fn row_selected(&self, index: usize) -> &'static str {
        if self.is_active(index) {
            "true"
        } else {
            "false"
        }
    }

    /// Show the panel, keeping any highlight. Wired to `onfocusin` and
    /// `onclick` on the field wrapper, so tabbing in and clicking both open.
    pub fn open(&mut self) {
        self.open.set(true);
    }

    /// Show the panel and drop the highlight. Wired to `oninput`, where the
    /// row set is about to change under the old index.
    pub fn open_fresh(&mut self) {
        self.open.set(true);
        self.active.set(None);
    }

    /// Hide the panel and reset the highlight, committing nothing.
    pub fn close(&mut self) {
        self.open.set(false);
        self.active.set(None);
    }

    /// The keydown handler. Attach to the field wrapper `div`; `len` is the
    /// current navigable row count and `commit` takes a row index. Returns
    /// what it did so a call site can add its own teardown on `Close`.
    pub fn keydown<F: FnOnce(usize)>(
        &mut self,
        e: &KeyboardEvent,
        len: usize,
        commit: F,
    ) -> NavAction {
        let key = e.key();
        let action = decide(
            &key,
            e.modifiers().shift(),
            self.is_open(),
            self.active_index(),
            len,
        );
        match action {
            NavAction::Ignore => {}
            NavAction::Open(index) => {
                // Without this the caret jumps to either end of the field
                // while the user is moving through the list.
                e.prevent_default();
                self.open.set(true);
                self.active.set(index);
                if let Some(i) = index {
                    self.scroll_row_into_view(i, key == Key::ArrowUp);
                }
            }
            NavAction::Commit {
                index,
                prevent_default,
            } => {
                if prevent_default {
                    e.prevent_default();
                }
                self.close();
                commit(index);
            }
            NavAction::Close => self.close(),
        }
        action
    }

    /// Follow the highlight past the edge of the panel's `max-h-*` scroll box.
    fn scroll_row_into_view(&self, index: usize, align_top: bool) {
        #[cfg(feature = "app")]
        {
            // MAPPS-504: through the platform boundary, not `web_sys`
            // directly. `feature = "app"` is the app-runtime gate and is
            // on for the desktop build too, so a direct browser call here
            // would not compile against the wasm-only bindings.
            crate::platform::dom::scroll_into_view(&self.row_id(index), align_top);
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = (index, align_top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_starts_at_the_first_row_and_clamps_at_the_last() {
        assert_eq!(step_index(None, 3, true), Some(0));
        assert_eq!(step_index(Some(0), 3, true), Some(1));
        assert_eq!(step_index(Some(2), 3, true), Some(2));
    }

    #[test]
    fn up_clamps_at_the_first_row_and_never_wraps() {
        assert_eq!(step_index(Some(2), 3, false), Some(1));
        assert_eq!(step_index(Some(0), 3, false), Some(0));
        // No highlight yet: Up lands on the first row rather than the last,
        // which is the same clamped-at-the-top rule.
        assert_eq!(step_index(None, 3, false), Some(0));
    }

    #[test]
    fn an_empty_list_has_nothing_to_highlight() {
        assert_eq!(step_index(None, 0, true), None);
        assert_eq!(step_index(Some(4), 0, false), None);
        assert_eq!(commit_index(None, 0), None);
        assert_eq!(commit_index(Some(1), 0), None);
    }

    #[test]
    fn a_stale_index_clamps_into_the_shorter_list() {
        assert_eq!(step_index(Some(9), 3, true), Some(2));
        assert_eq!(step_index(Some(9), 3, false), Some(1));
        assert_eq!(commit_index(Some(9), 3), Some(0));
    }

    #[test]
    fn tab_takes_the_first_row_when_nothing_is_highlighted() {
        assert_eq!(commit_index(None, 3), Some(0));
        assert_eq!(commit_index(Some(2), 3), Some(2));
    }

    #[test]
    fn arrows_open_a_closed_list() {
        assert_eq!(
            decide(&Key::ArrowDown, false, false, None, 3),
            NavAction::Open(Some(0))
        );
        assert_eq!(
            decide(&Key::ArrowUp, false, false, None, 3),
            NavAction::Open(Some(0))
        );
    }

    #[test]
    fn enter_commits_only_a_highlighted_row_of_an_open_list() {
        assert_eq!(
            decide(&Key::Enter, false, true, Some(1), 3),
            NavAction::Commit {
                index: 1,
                prevent_default: true,
            }
        );
        // No highlight, or a closed list: Enter belongs to the form.
        assert_eq!(decide(&Key::Enter, false, true, None, 3), NavAction::Ignore);
        assert_eq!(
            decide(&Key::Enter, false, false, Some(1), 3),
            NavAction::Ignore
        );
    }

    #[test]
    fn tab_commits_without_preventing_the_move_to_the_next_field() {
        assert_eq!(
            decide(&Key::Tab, false, true, None, 3),
            NavAction::Commit {
                index: 0,
                prevent_default: false,
            }
        );
        // Nothing to take, backing out, or already closed (Escape first):
        // Tab just leaves, with the typed text intact.
        assert_eq!(decide(&Key::Tab, false, true, None, 0), NavAction::Ignore);
        assert_eq!(decide(&Key::Tab, true, true, Some(1), 3), NavAction::Ignore);
        assert_eq!(
            decide(&Key::Tab, false, false, Some(1), 3),
            NavAction::Ignore
        );
    }

    #[test]
    fn escape_closes_and_other_keys_are_left_alone() {
        assert_eq!(
            decide(&Key::Escape, false, true, Some(1), 3),
            NavAction::Close
        );
        assert_eq!(
            decide(&Key::Character("a".into()), false, true, Some(1), 3),
            NavAction::Ignore
        );
    }
}

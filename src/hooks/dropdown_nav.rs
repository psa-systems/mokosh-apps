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
//! MAPPS-653: a picker that has to end up holding a record (company, contact,
//! asset, product) opts into [`DropdownNav::enter_takes_first_match`], so
//! Enter takes the first row when nothing is highlighted rather than falling
//! through to the form. A free-text field with optional suggestions
//! ([`SuggestInput`]) does not: there, Enter would overwrite what was typed
//! with a suggestion the user never chose.
//!
//! [`SuggestInput`]: crate::components::SuggestInput
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

/// The navigable row set of a picker whose result rows are followed by an
/// optional inline "+ Create" action. Owned here because the create action's
/// index is what makes the no-match path work: with nothing matching the typed
/// text, the create action is row 0 and Enter therefore starts a new record
/// (MAPPS-653).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavRows {
    /// Every navigable row, the create action included.
    pub len: usize,
    /// Index of the trailing create action, when the picker offers one.
    pub create_index: Option<usize>,
}

impl NavRows {
    pub fn new(results: usize, has_create: bool) -> Self {
        Self {
            len: results + usize::from(has_create),
            create_index: has_create.then_some(results),
        }
    }
}

/// Which contract [`decide`] follows. A picker (a field of values to
/// commit into a form) and a menu (a floating panel of actions) share
/// most of the key shape - Escape closes, arrows move a highlight - but
/// commit differently. In a picker Tab commits the highlighted row and
/// moves on to the next field; in a menu there is no field to move on to
/// and every row is a real `<button>` whose Enter is the browser's own,
/// so Tab and Enter must not commit through the hook (MAPPS-508).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMode {
    /// Field of values, with Tab-commits-and-moves-on and Enter-commits.
    Combobox,
    /// Floating panel of actions. Tab and Enter fall through to the
    /// browser (Tab walks the rows, Enter fires the focused button).
    Menu,
}

/// The whole key state machine, free of Dioxus and the DOM so it is unit
/// testable. `len` counts every navigable row, including a trailing inline
/// "+ Create" action. `enter_takes_first` is the picker's opt-in from
/// [`DropdownNav::enter_takes_first_match`]; `mode` is the picker/menu
/// switch [`NavMode`] documents.
pub fn decide(
    key: &Key,
    shift: bool,
    open: bool,
    active: Option<usize>,
    len: usize,
    enter_takes_first: bool,
    mode: NavMode,
) -> NavAction {
    match key {
        Key::ArrowDown => NavAction::Open(step_index(active, len, true)),
        Key::ArrowUp => NavAction::Open(step_index(active, len, false)),
        // MAPPS-653: in a record picker Enter takes the first row when nothing
        // is highlighted, so a typed name is accepted without arrowing to it
        // first. Everywhere else it only ever takes a row the user actually
        // highlighted, and otherwise stays the form's key.
        //
        // MAPPS-508: in a menu Enter stays the browser's key so a focused
        // menu button fires its own onclick; the hook owns highlight only.
        Key::Enter if open && mode == NavMode::Combobox => {
            let index = if enter_takes_first {
                commit_index(active, len)
            } else {
                active.filter(|i| *i < len)
            };
            match index {
                Some(index) => NavAction::Commit {
                    index,
                    prevent_default: true,
                },
                None => NavAction::Ignore,
            }
        }
        // Shift+Tab backs out of the field, so committing on the way out
        // would be a surprise. MAPPS-508: a menu has no field to move on
        // to, so Tab is always Ignore in menu mode - the browser walks the
        // rows on its own.
        Key::Tab if open && !shift && mode == NavMode::Combobox => {
            match commit_index(active, len) {
                Some(index) => NavAction::Commit {
                    index,
                    prevent_default: false,
                },
                None => NavAction::Ignore,
            }
        }
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
    enter_takes_first: bool,
    mode: NavMode,
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
        enter_takes_first: false,
        mode: NavMode::Combobox,
    }
}

impl DropdownNav {
    /// MAPPS-653: opt Enter into taking the first row when nothing is
    /// highlighted, for a picker whose field has to end up holding a record.
    /// Chain it onto [`use_dropdown_nav`]; it changes no state, so calling it
    /// on every render is free.
    ///
    /// Left off for a free-text field with optional suggestions, where Enter
    /// taking an unhighlighted suggestion would replace what the user typed.
    pub fn enter_takes_first_match(mut self) -> Self {
        self.enter_takes_first = true;
        self
    }

    /// MAPPS-508: opt this instance into menu semantics (Tab and Enter fall
    /// through to the browser, so Tab keeps walking the rows and Enter
    /// fires the focused button's own onclick). Chain it onto
    /// [`use_dropdown_nav`] for a floating panel of actions rather than a
    /// field of values to commit.
    pub fn menu(mut self) -> Self {
        self.mode = NavMode::Menu;
        self
    }

    /// The active mode.
    pub fn mode(&self) -> NavMode {
        self.mode
    }

    /// Is the panel showing? A tracked read, for render and event handlers.
    pub fn is_open(&self) -> bool {
        *self.open.read()
    }

    /// The read to use inside `use_effect` / `use_memo`: a tracked read there
    /// subscribes the effect to this dropdown's own state (MAPPS-964).
    pub fn is_open_untracked(&self) -> bool {
        *self.open.peek()
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
    ///
    /// Menu callers (MAPPS-508) pass a no-op `commit` because [`NavMode::Menu`]
    /// never returns `NavAction::Commit`; they can also call
    /// [`keydown_menu`](Self::keydown_menu) instead, which needs no closure.
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
            self.enter_takes_first,
            self.mode,
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

    /// MAPPS-508: the menu-mode keydown handler. Same shape as
    /// [`keydown`](Self::keydown) but takes no `commit` closure, because
    /// [`NavMode::Menu`] never returns `NavAction::Commit` - Enter and Tab
    /// are the browser's keys, so the menu row's own onclick fires on
    /// Enter when the row is focused.
    pub fn keydown_menu(&mut self, e: &KeyboardEvent, len: usize) -> NavAction {
        self.keydown(e, len, |_| {})
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

    /// The two Enter contracts, so a call site's arguments read as the picker
    /// they belong to. A record picker opts in (MAPPS-653); a free-text field
    /// with suggestions does not.
    const RECORD: bool = true;
    const FREE_TEXT: bool = false;
    /// The mode the pre-MAPPS-508 tests all ran under; kept as a name so
    /// the picker suite reads the same after threading the mode arg in.
    const PICKER: NavMode = NavMode::Combobox;
    const MENU: NavMode = NavMode::Menu;

    #[test]
    fn arrows_open_a_closed_list() {
        assert_eq!(
            decide(&Key::ArrowDown, false, false, None, 3, RECORD, PICKER),
            NavAction::Open(Some(0))
        );
        assert_eq!(
            decide(&Key::ArrowUp, false, false, None, 3, RECORD, PICKER),
            NavAction::Open(Some(0))
        );
    }

    #[test]
    fn enter_commits_the_highlighted_row() {
        for enter_takes_first in [RECORD, FREE_TEXT] {
            assert_eq!(
                decide(
                    &Key::Enter,
                    false,
                    true,
                    Some(1),
                    3,
                    enter_takes_first,
                    PICKER
                ),
                NavAction::Commit {
                    index: 1,
                    prevent_default: true,
                }
            );
            // A closed list, or an empty one: Enter belongs to the form.
            assert_eq!(
                decide(
                    &Key::Enter,
                    false,
                    false,
                    Some(1),
                    3,
                    enter_takes_first,
                    PICKER
                ),
                NavAction::Ignore
            );
            assert_eq!(
                decide(&Key::Enter, false, true, None, 0, enter_takes_first, PICKER),
                NavAction::Ignore
            );
        }
    }

    /// MAPPS-653: the reported symptom. Typing a company name and pressing
    /// Enter left the field empty, because nothing was highlighted and Enter
    /// fell through to the form.
    #[test]
    fn enter_takes_the_first_row_in_a_record_picker_with_no_highlight() {
        assert_eq!(
            decide(&Key::Enter, false, true, None, 3, RECORD, PICKER),
            NavAction::Commit {
                index: 0,
                prevent_default: true,
            }
        );
        // A stale index past the end of a shrunken list falls back to the
        // first row rather than committing nothing.
        assert_eq!(
            decide(&Key::Enter, false, true, Some(9), 3, RECORD, PICKER),
            NavAction::Commit {
                index: 0,
                prevent_default: true,
            }
        );
    }

    /// A free-text field's value is what the user typed. Enter there must not
    /// replace it with a suggestion they never highlighted.
    #[test]
    fn enter_takes_nothing_unhighlighted_in_a_free_text_field() {
        assert_eq!(
            decide(&Key::Enter, false, true, None, 3, FREE_TEXT, PICKER),
            NavAction::Ignore
        );
    }

    /// The no-match path: nothing matches the typed text, so the inline create
    /// action is the only navigable row, and Enter starts a new record with it.
    #[test]
    fn enter_on_a_no_match_query_commits_the_inline_create_row() {
        let list = NavRows::new(0, true);
        assert_eq!(list.len, 1);
        assert_eq!(list.create_index, Some(0));
        assert_eq!(
            decide(&Key::Enter, false, true, None, list.len, RECORD, PICKER),
            NavAction::Commit {
                index: list.create_index.expect("the create action is navigable"),
                prevent_default: true,
            }
        );
    }

    /// With matches, the create action is still reachable but never what Enter
    /// takes by default: the first match is.
    #[test]
    fn the_create_action_is_the_last_row_behind_the_matches() {
        let list = NavRows::new(2, true);
        assert_eq!(list.len, 3);
        assert_eq!(list.create_index, Some(2));
        assert_eq!(
            decide(&Key::Enter, false, true, None, list.len, RECORD, PICKER),
            NavAction::Commit {
                index: 0,
                prevent_default: true,
            }
        );
        // Down from the last match reaches it.
        assert_eq!(step_index(Some(1), list.len, true), list.create_index);
    }

    /// A picker without the affordance has result rows only, so no index can
    /// mean "create".
    #[test]
    fn a_picker_with_no_create_action_has_only_its_results() {
        let list = NavRows::new(2, false);
        assert_eq!(list.len, 2);
        assert_eq!(list.create_index, None);
        // And no matches means nothing at all to commit.
        let empty = NavRows::new(0, false);
        assert_eq!(empty.len, 0);
        assert_eq!(
            decide(&Key::Enter, false, true, None, empty.len, RECORD, PICKER),
            NavAction::Ignore
        );
    }

    #[test]
    fn tab_commits_without_preventing_the_move_to_the_next_field() {
        assert_eq!(
            decide(&Key::Tab, false, true, None, 3, RECORD, PICKER),
            NavAction::Commit {
                index: 0,
                prevent_default: false,
            }
        );
        // Nothing to take, backing out, or already closed (Escape first):
        // Tab just leaves, with the typed text intact.
        assert_eq!(
            decide(&Key::Tab, false, true, None, 0, RECORD, PICKER),
            NavAction::Ignore
        );
        assert_eq!(
            decide(&Key::Tab, true, true, Some(1), 3, RECORD, PICKER),
            NavAction::Ignore
        );
        assert_eq!(
            decide(&Key::Tab, false, false, Some(1), 3, RECORD, PICKER),
            NavAction::Ignore
        );
    }

    #[test]
    fn escape_closes_and_other_keys_are_left_alone() {
        assert_eq!(
            decide(&Key::Escape, false, true, Some(1), 3, RECORD, PICKER),
            NavAction::Close
        );
        assert_eq!(
            decide(
                &Key::Character("a".into()),
                false,
                true,
                Some(1),
                3,
                RECORD,
                PICKER
            ),
            NavAction::Ignore
        );
    }

    // ---- MAPPS-508: menu mode -------------------------------------------

    /// Escape and arrows keep working in menu mode - they are the two
    /// keys the ticket asks the six panels to gain.
    #[test]
    fn menu_mode_still_arrows_and_escapes() {
        assert_eq!(
            decide(&Key::ArrowDown, false, true, None, 3, FREE_TEXT, MENU),
            NavAction::Open(Some(0))
        );
        assert_eq!(
            decide(&Key::ArrowUp, false, true, Some(2), 3, FREE_TEXT, MENU),
            NavAction::Open(Some(1))
        );
        assert_eq!(
            decide(&Key::Escape, false, true, Some(1), 3, FREE_TEXT, MENU),
            NavAction::Close
        );
    }

    /// Enter in menu mode falls through to the browser: the focused
    /// menu button's own onclick fires it. The hook does not commit.
    #[test]
    fn menu_mode_leaves_enter_to_the_focused_row() {
        assert_eq!(
            decide(&Key::Enter, false, true, Some(1), 3, RECORD, MENU),
            NavAction::Ignore
        );
        assert_eq!(
            decide(&Key::Enter, false, true, None, 3, RECORD, MENU),
            NavAction::Ignore
        );
    }

    /// Tab in menu mode is also the browser's key: the ticket keeps rows
    /// in the tab order so Tab walks them, and there is no field to move
    /// on to.
    #[test]
    fn menu_mode_does_not_commit_on_tab() {
        assert_eq!(
            decide(&Key::Tab, false, true, Some(1), 3, RECORD, MENU),
            NavAction::Ignore
        );
        assert_eq!(
            decide(&Key::Tab, false, true, None, 3, RECORD, MENU),
            NavAction::Ignore
        );
    }

    // ---- MAPPS-964: effects read the open state untracked ---------------

    /// `src` with comments and string / char literals blanked (newlines kept),
    /// so the scan sees code only and its line numbers still line up.
    fn code_only(src: &str) -> String {
        let bytes = src.as_bytes();
        let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let mut out = bytes.to_vec();
        let mut i = 0;
        while i < bytes.len() {
            let rest = &src[i..];
            let token_start = i == 0 || !ident(bytes[i - 1]);
            let skip = if rest.starts_with("//") {
                rest.find('\n').unwrap_or(rest.len())
            } else if rest.starts_with("/*") {
                rest.find("*/").map_or(rest.len(), |n| n + 2)
            } else if let Some(n) = raw_string_len(rest).filter(|_| token_start) {
                n
            } else if rest.starts_with('"') {
                quoted_len(rest)
            } else if rest.starts_with('\'') {
                // No closing quote means a lifetime, which is code.
                char_literal_len(rest).unwrap_or(0)
            } else {
                0
            };
            if skip == 0 {
                i += rest.chars().next().map_or(1, char::len_utf8);
                continue;
            }
            for b in &mut out[i..i + skip] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            i += skip;
        }
        String::from_utf8(out).expect("blanking whole chars keeps UTF-8 valid")
    }

    /// Length of the `r"..."` / `r#"..."#` literal `rest` starts with, if any.
    fn raw_string_len(rest: &str) -> Option<usize> {
        let after_r = rest.strip_prefix('r')?;
        let hashes = after_r.len() - after_r.trim_start_matches('#').len();
        let body = after_r[hashes..].strip_prefix('"')?;
        let close = format!("\"{}", "#".repeat(hashes));
        let end = body.find(&close).map_or(body.len(), |n| n + close.len());
        Some(1 + hashes + 1 + end)
    }

    /// Length of the `"..."` literal `rest` starts with, escapes included.
    fn quoted_len(rest: &str) -> usize {
        let b = rest.as_bytes();
        let mut j = 1;
        while j < b.len() && b[j] != b'"' {
            j += if b[j] == b'\\' { 2 } else { 1 };
        }
        (j + 1).min(b.len())
    }

    /// Length of the char literal `rest` starts with; `None` for a lifetime.
    fn char_literal_len(rest: &str) -> Option<usize> {
        let inner = &rest[1..];
        if let Some(esc) = inner.strip_prefix('\\') {
            // Step over the escaped char so '\'' closes on its last quote.
            let first = esc.chars().next()?.len_utf8();
            esc[first..].find('\'').map(|n| 2 + first + n + 1)
        } else {
            let c = inner.chars().next()?.len_utf8();
            inner[c..].starts_with('\'').then_some(c + 2)
        }
    }

    /// Every `use_effect(` call in `src` as (line, call text), literals and
    /// comments blanked. A call ends at its matching `)`, so a handler later
    /// in the same component is not part of it.
    fn effect_calls(src: &str) -> Vec<(usize, String)> {
        let code = code_only(src);
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(rel) = code[from..].find("use_effect(") {
            let start = from + rel + "use_effect".len();
            let mut depth = 0usize;
            let mut end = code.len();
            for (i, b) in code.bytes().enumerate().skip(start) {
                match b {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let line = code[..start].matches('\n').count() + 1;
            out.push((line, code[start..end].to_string()));
            from = start;
        }
        out
    }

    /// Lines of the effect calls that make a tracked `.is_open()` read.
    fn tracked_open_reads(calls: &[(usize, String)]) -> Vec<usize> {
        calls
            .iter()
            .filter(|(_, call)| call.contains(".is_open()"))
            .map(|(line, _)| *line)
            .collect()
    }

    /// The guard's own proof: it catches the MAPPS-964 shape, and passes the
    /// fixed one even beside a comment, an unbalanced `"("` and a later handler.
    #[test]
    fn the_effect_scan_catches_a_tracked_open_read_and_nothing_else() {
        const VIOLATING: &str = r#"
            use_effect(use_reactive!(|route| {
                let _ = &route;
                if nav.is_open() {
                    nav.close();
                }
            }));
        "#;
        const COMPLIANT: &str = r#"
            use_effect(use_reactive!(|route| {
                // Naming nav.is_open() in a comment is not a read.
                let _ = (&route, "(");
                if nav.is_open_untracked() {
                    nav.close();
                }
            }));
            rsx! { Popover { ontoggle: move |_| if nav.is_open() { nav.close() } } }
        "#;
        assert_eq!(tracked_open_reads(&effect_calls(VIOLATING)), vec![2]);
        assert_eq!(effect_calls(COMPLIANT).len(), 1);
        assert!(tracked_open_reads(&effect_calls(COMPLIANT)).is_empty());
    }

    /// MAPPS-964: `is_open()` inside an effect subscribes it to the dropdown's
    /// own open state, so the user menu's close-on-navigation effect re-ran
    /// on every open and shut the menu in the same frame.
    #[test]
    fn no_effect_reads_a_dropdowns_open_state_tracked() {
        let mut offenders = Vec::new();
        let mut effects = 0usize;
        let mut stack = vec![std::path::PathBuf::from("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("read file");
                let calls = effect_calls(&src);
                effects += calls.len();
                for line in tracked_open_reads(&calls) {
                    offenders.push(format!("{}:{line}", path.display()));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "a `use_effect` reads `.is_open()`, which subscribes it to the dropdown's own \
             open state, so opening the dropdown re-runs the effect (MAPPS-964: the user \
             menu closed the instant it opened). Read it with `is_open_untracked()` \
             instead: {offenders:?}"
        );
        assert!(
            effects >= 40,
            "the scan found only {effects} use_effect calls, so it has probably stopped \
             matching them rather than proving anything"
        );
    }

    /// Lines of the effect calls that write `EFFECTIVE_BRANDING` and also read it
    /// tracked. A read-only effect (`use_apply_brand`) must track it to repaint.
    fn tracked_branding_reads(calls: &[(usize, String)]) -> Vec<usize> {
        const TRACKED: [&str; 3] = [
            "EFFECTIVE_BRANDING.read()",
            "EFFECTIVE_BRANDING()",
            "EFFECTIVE_BRANDING.with(",
        ];
        const WRITES: [&str; 3] = [
            "set_effective_branding(",
            "clear_effective_branding(",
            "EFFECTIVE_BRANDING.write()",
        ];
        calls
            .iter()
            .filter(|(_, call)| {
                let flat: String = call.split_whitespace().collect();
                TRACKED.iter().any(|shape| flat.contains(shape))
                    && WRITES.iter().any(|shape| flat.contains(shape))
            })
            .map(|(line, _)| *line)
            .collect()
    }

    /// The guard's own proof: it catches the MAPPS-968 shape, split across
    /// lines as rustfmt writes it, and passes the `peek()` form and a
    /// read-only repaint effect.
    #[test]
    fn the_effect_scan_catches_a_tracked_branding_read_and_nothing_else() {
        const VIOLATING: &str = r#"
            use_effect(|| {
                if crate::hooks::branding::EFFECTIVE_BRANDING
                    .read()
                    .display_name
                    .is_some()
                {
                    return;
                }
                spawn(async move {
                    crate::hooks::branding::set_effective_branding(t.branding);
                });
            });
        "#;
        const COMPLIANT: &str = r#"
            use_effect(|| {
                // EFFECTIVE_BRANDING.read() in a comment is not a read.
                if crate::hooks::branding::EFFECTIVE_BRANDING
                    .peek()
                    .display_name
                    .is_some()
                {
                    return;
                }
                spawn(async move {
                    crate::hooks::branding::set_effective_branding(t.branding);
                });
            });
            use_effect(move || {
                let brand = EFFECTIVE_BRANDING.read().clone();
                apply_brand_css_vars(&brand);
            });
        "#;
        assert_eq!(tracked_branding_reads(&effect_calls(VIOLATING)), vec![2]);
        assert_eq!(effect_calls(COMPLIANT).len(), 2);
        assert!(tracked_branding_reads(&effect_calls(COMPLIANT)).is_empty());
    }

    /// MAPPS-968: the shell's branding effect read `EFFECTIVE_BRANDING` tracked
    /// and wrote it, so a tenant with no display name refetched `/tenants/current`
    /// about 15 times a second for as long as a staff tab stayed open.
    #[test]
    fn no_effect_reads_the_effective_branding_tracked() {
        let mut offenders = Vec::new();
        let mut stack = vec![std::path::PathBuf::from("src")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("read file");
                for line in tracked_branding_reads(&effect_calls(&src)) {
                    offenders.push(format!("{}:{line}", path.display()));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "a `use_effect` that writes `EFFECTIVE_BRANDING` also reads it tracked, so \
             its own write re-runs it (MAPPS-968: an endless `/tenants/current` \
             refetch). Read it with `.peek()` instead: {offenders:?}"
        );
    }
}

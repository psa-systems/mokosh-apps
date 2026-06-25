//! Submit-time form validation guard (PMS-517).
//!
//! The per-field components (`components/form.rs`) validate on blur (PMS-516),
//! but a field the user never focused stays silent, and a handler that bails on
//! the first failure hides the rest (the PMS-514 bug). [`FormGuard`] is the
//! submit-time counterpart: a handler runs every required field through it, then
//! asks whether to bail. It reports ALL failures at once (each into that field's
//! own inline error slot) and focuses the first invalid field, generalizing the
//! per-field PMS-514 fix into one reusable helper.
//!
//! It is a plain helper, so it works identically for `<form onsubmit>` page
//! forms and the `onclick`-submit modals (both just call it inside the handler).
//! It is deliberately Signal-free: [`FormGuard::field`] returns the message to
//! display and the caller `set`s its own error signal, so the accumulation logic
//! is pure and unit-testable without a Dioxus runtime.
//!
//! Typical use (generalizing the new-ticket fix):
//!
//! ```ignore
//! let mut guard = FormGuard::new();
//! title_error.set(guard.field("title", &title_v, "Title", &[Rule::Required]));
//! company_error.set(guard.field("company", &company_id_v, "Company", &[Rule::Uuid]));
//! if guard.blocked() {
//!     is_submitting.set(false);
//!     return; // no POST; every failing field shows its message, first is focused
//! }
//! ```
//!
//! The `id` passed to [`FormGuard::field`] must be the field's DOM id, which the
//! shared components set from their `name` prop (`form.rs`: `id: "{props.name}"`),
//! so focus lands on the right element.

use super::validation::{validate, Rule};

/// Accumulates field validity across a submit so the handler can surface every
/// failure at once and focus the first invalid field. Construct one per submit.
#[derive(Debug, Default)]
pub struct FormGuard {
    any_invalid: bool,
    first_invalid: Option<String>,
}

impl FormGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate `value` against `rules` and return the message to show in the
    /// field's inline error slot: the first failing rule's message, or the empty
    /// string when the field is valid (so the caller clears any stale message).
    /// Records the failure and remembers the first invalid field's `id` for focus.
    ///
    /// The caller is expected to `set` its own error signal with the return value,
    /// e.g. `title_error.set(guard.field("title", &v, "Title", &[Rule::Required]))`.
    #[must_use = "set the field's error signal with the returned message"]
    pub fn field(&mut self, id: &str, value: &str, label: &str, rules: &[Rule]) -> String {
        match validate(value, label, rules) {
            Some(message) => {
                self.record_invalid(id);
                message
            }
            None => String::new(),
        }
    }

    /// Record a cross-field or non-field failure that has no inline slot (e.g. a
    /// mutually-exclusive picker pair, or a value that only fails server-side and
    /// is surfaced in the form-level banner). Blocks the submit; `focus_id` ties
    /// focus to a related field when one makes sense.
    pub fn note_invalid(&mut self, focus_id: Option<&str>) {
        self.any_invalid = true;
        if self.first_invalid.is_none() {
            if let Some(id) = focus_id {
                self.first_invalid = Some(id.to_string());
            }
        }
    }

    fn record_invalid(&mut self, id: &str) {
        self.any_invalid = true;
        if self.first_invalid.is_none() {
            self.first_invalid = Some(id.to_string());
        }
    }

    /// Whether any field failed. Pure; does not touch the DOM. Prefer
    /// [`FormGuard::blocked`] in a handler so the first invalid field is focused.
    pub fn is_valid(&self) -> bool {
        !self.any_invalid
    }

    /// The id of the first invalid field, if any (the one [`FormGuard::blocked`]
    /// focuses). Exposed for tests and callers that want custom focus handling.
    pub fn first_invalid(&self) -> Option<&str> {
        self.first_invalid.as_deref()
    }

    /// Returns `true` when the form must NOT submit, after focusing the first
    /// invalid field. Call as the bail condition: `if guard.blocked() { return; }`.
    pub fn blocked(&self) -> bool {
        if self.any_invalid {
            if let Some(id) = &self.first_invalid {
                focus_field(id);
            }
        }
        self.any_invalid
    }
}

/// Move keyboard focus to the field with the given DOM id. Gated on
/// `target_arch = "wasm32"` (the repo convention for `web_sys`, see `lib.rs`):
/// `web_sys` imports panic when called off-wasm, so on the host/test build this
/// is a no-op rather than `feature = "web"` (which is on even for host tests).
#[cfg(target_arch = "wasm32")]
fn focus_field(id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
    {
        if let Ok(html) = element.dyn_into::<web_sys::HtmlElement>() {
            let _ = html.focus();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn focus_field(_id: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_valid_does_not_block() {
        let mut guard = FormGuard::new();
        assert_eq!(
            guard.field("title", "Hello", "Title", &[Rule::Required]),
            ""
        );
        assert_eq!(
            guard.field(
                "amount",
                "10.00",
                "Amount",
                &[Rule::Number {
                    min: Some(0.0),
                    max: None,
                    max_decimals: Some(2)
                }],
            ),
            ""
        );
        assert!(guard.is_valid());
        assert!(!guard.blocked());
        assert_eq!(guard.first_invalid(), None);
    }

    #[test]
    fn reports_every_failure_at_once() {
        let mut guard = FormGuard::new();
        // Both empty: both messages come back, not just the first.
        let title_msg = guard.field("title", "", "Title", &[Rule::Required]);
        let company_msg = guard.field("company", "", "Company", &[Rule::Required]);
        assert_eq!(title_msg, "Title is required.");
        assert_eq!(company_msg, "Company is required.");
        assert!(!guard.is_valid());
        assert!(guard.blocked());
    }

    #[test]
    fn first_invalid_is_the_first_failing_field() {
        let mut guard = FormGuard::new();
        // title valid, company invalid, description invalid -> first invalid is company.
        let _ = guard.field("title", "ok", "Title", &[Rule::Required]);
        let _ = guard.field("company", "", "Company", &[Rule::Required]);
        let _ = guard.field("description", "", "Description", &[Rule::Required]);
        assert_eq!(guard.first_invalid(), Some("company"));
    }

    #[test]
    fn valid_field_returns_empty_to_clear_stale_message() {
        let mut guard = FormGuard::new();
        // A previously-shown server error should be cleared on a passing resubmit.
        assert_eq!(guard.field("email", "a@b.co", "Email", &[Rule::Email]), "");
    }

    #[test]
    fn note_invalid_blocks_and_can_set_focus() {
        let mut guard = FormGuard::new();
        guard.note_invalid(Some("approver"));
        assert!(!guard.is_valid());
        assert!(guard.blocked());
        assert_eq!(guard.first_invalid(), Some("approver"));
    }

    #[test]
    fn note_invalid_without_focus_still_blocks() {
        let mut guard = FormGuard::new();
        guard.note_invalid(None);
        assert!(!guard.is_valid());
        assert_eq!(guard.first_invalid(), None);
    }

    #[test]
    fn field_failure_precedes_later_note_invalid_for_focus() {
        let mut guard = FormGuard::new();
        let _ = guard.field("name", "", "Name", &[Rule::Required]);
        guard.note_invalid(Some("other"));
        // The field failed first, so it keeps focus priority.
        assert_eq!(guard.first_invalid(), Some("name"));
    }
}

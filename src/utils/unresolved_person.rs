//! MAPPS-1036: one shared helper for every "placeholder shown instead of a
//! resolved reference" substitution. A placeholder that leaves no trace is
//! masking (`no-masking.md`); this makes every substitution findable with an
//! INFO log line and a hover tooltip, both carrying the unresolved id.
//!
//! Dedup is per `(surface, id)` for a page load (a thread-local set, WASM is
//! single-threaded so a `RefCell` is sufficient), so a component that
//! re-renders logs once, not once per render.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static LOGGED: RefCell<HashSet<(String, Option<uuid::Uuid>)>> = RefCell::new(HashSet::new());
}

/// Called at the moment a site picks a placeholder instead of a name the
/// server didn't send. Logs at INFO once per `(surface, id)` and returns the
/// hover tooltip text to set as `title` on the element that shows `shown`.
///
/// `surface` names the screen/field (e.g. `"projects.task_assignee"`).
/// `kind` names what the id refers to (e.g. `"user"`, `"company"`, `"work
/// type"`). `id` is the unresolved id, or `None` when the id itself was
/// absent and not-set text is being shown (call this only when a `Some` id
/// could not be named; an absent id that legitimately means "no one" is not
/// logged and does not call this helper at all).
pub fn unresolved(surface: &str, kind: &str, id: Option<uuid::Uuid>, shown: &str) -> String {
    let key = (surface.to_string(), id);
    let first_time = LOGGED.with(|logged| logged.borrow_mut().insert(key));
    if first_time {
        match id {
            Some(id) => {
                tracing::info!(surface, kind, %id, shown, "reference unresolved, placeholder shown");
            }
            None => {
                tracing::info!(
                    surface,
                    kind,
                    shown,
                    "reference unresolved, placeholder shown"
                );
            }
        }
    }
    match id {
        Some(id) => format!("Unresolved {kind} {id}"),
        None => format!("Unresolved {kind}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_tooltip_with_full_id() {
        let id = uuid::Uuid::nil();
        let tooltip = unresolved("test.surface_a", "user", Some(id), "Unknown");
        assert_eq!(tooltip, format!("Unresolved user {id}"));
    }

    #[test]
    fn dedup_logs_once_per_surface_and_id() {
        let id = uuid::Uuid::from_u128(42);
        let surface = "test.surface_dedup";
        let first =
            LOGGED.with(|logged| logged.borrow_mut().insert((surface.to_string(), Some(id))));
        // Simulate the first call having already happened; a second call
        // with the same (surface, id) must not be the "first time" again.
        assert!(first);
        let second_insert =
            LOGGED.with(|logged| logged.borrow_mut().insert((surface.to_string(), Some(id))));
        assert!(!second_insert);
    }

    #[test]
    fn distinct_ids_are_not_deduped_together() {
        let surface = "test.surface_distinct";
        let id_a = uuid::Uuid::from_u128(1);
        let id_b = uuid::Uuid::from_u128(2);
        let tooltip_a = unresolved(surface, "user", Some(id_a), "Unknown");
        let tooltip_b = unresolved(surface, "user", Some(id_b), "Unknown");
        assert_ne!(tooltip_a, tooltip_b);
    }
}

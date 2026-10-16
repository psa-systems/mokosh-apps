//! MAPPS-1036 / MAPPS-1037 / MAPPS-1039: one shared helper for every
//! "placeholder shown instead of a resolved reference" substitution. A
//! placeholder that leaves no trace is masking (`no-masking.md`); this makes
//! every substitution findable with an INFO log line and a hover tooltip,
//! both carrying the unresolved id.
//!
//! Dedup is per `(surface, id)` for a page load (a thread-local set, WASM is
//! single-threaded so a `RefCell` is sufficient), so a component that
//! re-renders logs once, not once per render.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static LOGGED: RefCell<HashSet<(String, Option<uuid::Uuid>)>> = RefCell::new(HashSet::new());
    /// MAPPS-1063 non-person variant: dedup set for references keyed by a
    /// string identifier (e.g. a dashboard widget_key) rather than a Uuid.
    /// Separate from `LOGGED` so neither call site can starve the other's
    /// dedup budget.
    static LOGGED_BY_KEY: RefCell<HashSet<(String, String)>> = RefCell::new(HashSet::new());
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

/// MAPPS-1063: the same contract as [`unresolved`] but keyed on a string
/// identifier instead of a `Uuid`. For references the server tracks by
/// name (a dashboard widget_key, a feature flag, etc.) rather than by id.
/// Dedup is per `(surface, key)` for a page load.
pub fn unresolved_by_key(surface: &str, kind: &str, key: &str, shown: &str) -> String {
    let entry = (surface.to_string(), key.to_string());
    let first_time = LOGGED_BY_KEY.with(|logged| logged.borrow_mut().insert(entry));
    if first_time {
        tracing::info!(
            surface,
            kind,
            key,
            shown,
            "reference unresolved, placeholder shown",
        );
    }
    format!("Unresolved {kind} {key}")
}

/// A history/audit actor resolved to one of three states (MAPPS-1037),
/// replacing the `== "-"` display-string comparisons every call site used
/// to need: a caller matches on the enum instead of a sentinel string.
#[derive(Clone, Debug, PartialEq)]
pub enum PersonRef {
    /// No actor id at all: absence by contract (e.g. a system-issued row).
    System,
    /// Resolved to a real name, from the server, a viewer match or the
    /// roster.
    Named(String),
    /// An id is present but nothing resolved it. `shown` is the placeholder
    /// text to render ("Someone"); `tooltip` names the unresolved id.
    Unresolved { shown: String, tooltip: String },
}

impl PersonRef {
    pub fn text(&self) -> &str {
        match self {
            PersonRef::System => "System",
            PersonRef::Named(name) => name,
            PersonRef::Unresolved { shown, .. } => shown,
        }
    }

    pub fn tooltip(&self) -> Option<&str> {
        match self {
            PersonRef::Unresolved { tooltip, .. } => Some(tooltip),
            _ => None,
        }
    }

    pub fn is_system(&self) -> bool {
        matches!(self, PersonRef::System)
    }
}

/// Resolves a history/audit actor to a [`PersonRef`] in the order the server
/// name, the viewer's own name (so a self-authored row never reads as
/// unresolved just because the roster is empty), then the roster. Logs and
/// tooltips the third, terminal "nothing resolved it" state through
/// [`unresolved`].
pub fn resolve_person(
    surface: &str,
    server_name: Option<&str>,
    id: &Option<uuid::Uuid>,
    viewer: Option<(uuid::Uuid, &str)>,
    roster: &[crate::hooks::UserRow],
) -> PersonRef {
    if let Some(name) = server_name {
        let name = name.trim();
        if !name.is_empty() {
            return PersonRef::Named(name.to_string());
        }
    }
    let Some(uid) = id else {
        return PersonRef::System;
    };
    if let Some((viewer_id, viewer_name)) = viewer {
        if *uid == viewer_id && !viewer_name.trim().is_empty() {
            return PersonRef::Named(viewer_name.to_string());
        }
    }
    if let Some(name) = roster
        .iter()
        .find(|u| &u.id == uid)
        .and_then(crate::hooks::UserRow::display_name)
    {
        return PersonRef::Named(name);
    }
    let tooltip = unresolved(surface, "user", Some(*uid), "Someone");
    PersonRef::Unresolved {
        shown: "Someone".to_string(),
        tooltip,
    }
}

/// A company reference resolved to one of three states (MAPPS-1038), the
/// company-shaped sibling of [`PersonRef`].
///
/// The three states are not interchangeable and the old code could not tell
/// two of them apart: a client-side join against a fetched company list
/// answered the same "-" / "Unknown company" for "this row has no company",
/// "the list has not arrived yet" and "the id resolves to nothing". The first
/// is a fact about the row, the second is a timing artefact that should not
/// reach the screen at all now that the server sends the name, and only the
/// third is worth a log line.
#[derive(Clone, Debug, PartialEq)]
pub enum CompanyRef {
    /// No `company_id` at all: absence by contract (an internal project).
    NotSet,
    /// The name the response carried.
    Named(String),
    /// An id is present and the response carried no name for it. `shown` is
    /// the placeholder text; `tooltip` names the unresolved id.
    Unresolved { shown: String, tooltip: String },
}

impl CompanyRef {
    /// The text to render. `not_set` is the caller's existing "no company"
    /// wording, which differs per screen ("-" on the asset screens, "No
    /// company" on the projects list) and is deliberately NOT unified here:
    /// changing visible copy is not what MAPPS-1038 is for.
    pub fn text<'a>(&'a self, not_set: &'a str) -> &'a str {
        match self {
            CompanyRef::NotSet => not_set,
            CompanyRef::Named(name) => name,
            CompanyRef::Unresolved { shown, .. } => shown,
        }
    }

    pub fn tooltip(&self) -> Option<&str> {
        match self {
            CompanyRef::Unresolved { tooltip, .. } => Some(tooltip),
            _ => None,
        }
    }
}

/// Resolve a company reference from the name the response carried and the id
/// beside it (MAPPS-1038).
///
/// There is no roster fallback on purpose, which is the whole point of the
/// ticket: the server resolves the name in the same query that returns the row
/// (PMS-335 / PMS-336), so a client-side join against a separately fetched
/// company list is redundant work that also masks the server field going
/// missing. A `Some` id with no name is therefore terminal: it logs and
/// tooltips through [`unresolved`] rather than being quietly papered over.
pub fn resolve_company(
    surface: &str,
    server_name: Option<&str>,
    id: Option<uuid::Uuid>,
) -> CompanyRef {
    if let Some(name) = server_name {
        let name = name.trim();
        if !name.is_empty() {
            return CompanyRef::Named(name.to_string());
        }
    }
    let Some(cid) = id else {
        return CompanyRef::NotSet;
    };
    let shown = "Unknown company";
    CompanyRef::Unresolved {
        shown: shown.to_string(),
        tooltip: unresolved(surface, "company", Some(cid), shown),
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

    #[test]
    fn none_id_resolves_to_system() {
        let state = resolve_person("test_surface_system", None, &None, None, &[]);
        assert_eq!(state, PersonRef::System);
        assert_eq!(state.text(), "System");
        assert!(state.tooltip().is_none());
    }

    #[test]
    fn server_name_wins_over_roster() {
        let id = uuid::Uuid::new_v4();
        let roster = vec![crate::hooks::UserRow {
            id,
            full_name: "Roster Name".to_string(),
            ..Default::default()
        }];
        let state = resolve_person(
            "test_surface_server",
            Some("Server Name"),
            &Some(id),
            None,
            &roster,
        );
        assert_eq!(state, PersonRef::Named("Server Name".to_string()));
    }

    #[test]
    fn viewer_match_wins_when_no_server_name() {
        let id = uuid::Uuid::new_v4();
        let state = resolve_person(
            "test_surface_viewer",
            None,
            &Some(id),
            Some((id, "Viewer Name")),
            &[],
        );
        assert_eq!(state, PersonRef::Named("Viewer Name".to_string()));
    }

    #[test]
    fn roster_is_the_last_resort() {
        let id = uuid::Uuid::new_v4();
        let roster = vec![crate::hooks::UserRow {
            id,
            full_name: "Roster Name".to_string(),
            ..Default::default()
        }];
        let state = resolve_person("test_surface_roster", None, &Some(id), None, &roster);
        assert_eq!(state, PersonRef::Named("Roster Name".to_string()));
    }

    #[test]
    fn nothing_resolves_it_is_someone_with_a_tooltip() {
        let id = uuid::Uuid::new_v4();
        let state = resolve_person("test_surface_unresolved", None, &Some(id), None, &[]);
        assert_eq!(state.text(), "Someone");
        assert_eq!(
            state.tooltip(),
            Some(format!("Unresolved user {id}")).as_deref()
        );
    }
    #[test]
    fn a_company_name_from_the_response_is_used_as_sent() {
        let state = resolve_company(
            "test.company_named",
            Some("Main Street Dental"),
            Some(uuid::Uuid::new_v4()),
        );
        assert_eq!(state, CompanyRef::Named("Main Street Dental".to_string()));
        assert_eq!(state.text("-"), "Main Street Dental");
        assert!(state.tooltip().is_none());
    }

    /// The state the client-side join could not distinguish from "no company".
    #[test]
    fn an_id_with_no_name_is_unresolved_with_the_id_in_the_tooltip() {
        let id = uuid::Uuid::from_u128(0x1038);
        let state = resolve_company("test.company_unresolved", None, Some(id));
        assert_eq!(state.text("-"), "Unknown company");
        assert_eq!(
            state.tooltip(),
            Some(format!("Unresolved company {id}")).as_deref()
        );
    }

    /// An empty string is the server saying nothing, not a company named "".
    #[test]
    fn a_blank_name_is_not_a_name() {
        let id = uuid::Uuid::from_u128(0x1039);
        let state = resolve_company("test.company_blank", Some("   "), Some(id));
        assert_eq!(state.text("-"), "Unknown company");
        assert!(state.tooltip().is_some());
    }

    /// No id is absence by contract: each screen keeps its own wording and
    /// nothing is logged, because nothing failed.
    #[test]
    fn no_id_is_not_set_and_keeps_the_callers_wording() {
        let state = resolve_company("test.company_absent", None, None);
        assert_eq!(state, CompanyRef::NotSet);
        assert_eq!(state.text("-"), "-");
        assert_eq!(state.text("No company"), "No company");
        assert!(state.tooltip().is_none());
    }
}

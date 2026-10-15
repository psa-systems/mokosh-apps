//! Shared helper for a person reference the client could not resolve to a
//! name (MAPPS-1036): a placeholder is masking only when it leaves no trace
//! (`no-masking.md`), so every site that falls back to a placeholder calls
//! this instead of staying silent.

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// `(surface, id)` pairs already logged this page load, so a re-render
    /// does not repeat the INFO line.
    static LOGGED: RefCell<HashSet<(String, String)>> = RefCell::new(HashSet::new());
}

/// Logs the unresolved reference at INFO, once per `(surface, id)` for the
/// life of this page load, and returns the hover text for the `title` of
/// whatever element shows the placeholder. `id` is `None` only for the rare
/// surface that substitutes a placeholder with no id behind it at all; the
/// common case, an id present but unresolved, is `Some`. Call this only when
/// the id is present and unresolved, or the name came back empty: a
/// legitimately absent id (no assignee) is not a failure and must not call
/// this.
pub fn unresolved(surface: &str, kind: &str, id: Option<uuid::Uuid>, shown: &str) -> String {
    let id_text = id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "none".to_string());
    let key = (surface.to_string(), id_text.clone());
    let first_time = LOGGED.with(|seen| seen.borrow_mut().insert(key));
    if first_time {
        tracing::info!(surface, kind, id = %id_text, shown, "reference unresolved, placeholder shown");
    }
    format!("Unresolved {kind} {id_text}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_once_per_surface_and_id() {
        let id = uuid::Uuid::new_v4();
        let first = unresolved("test_surface_dedup", "user", Some(id), "Someone");
        let was_logged_first_time = LOGGED.with(|seen| {
            seen.borrow()
                .contains(&("test_surface_dedup".to_string(), id.to_string()))
        });
        let second = unresolved("test_surface_dedup", "user", Some(id), "Someone");
        assert_eq!(first, second);
        assert!(was_logged_first_time);
    }

    #[test]
    fn tooltip_names_the_full_id() {
        let id = uuid::Uuid::new_v4();
        let tooltip = unresolved("test_surface_tooltip", "user", Some(id), "Someone");
        assert_eq!(tooltip, format!("Unresolved user {id}"));
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
}

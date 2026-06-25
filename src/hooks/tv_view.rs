//! TV-view dashboard preference (MAPPS-256).
//!
//! A wall-monitor "TV view" of the dashboard, scoped to the viewer's
//! team(s). Two localStorage-backed prefs, mirroring the theme precedent
//! (`hooks/theme.rs`): a boolean that turns the entry point on, and an
//! optional team id that narrows the scope from the default "my teams"
//! union to a single team.
//!
//!  - `mokosh_tv_view` ("1"/"0"): when on, the normal dashboard and the
//!    settings hub surface a link into `/dashboard/tv`.
//!  - `mokosh_tv_team` (uuid string, empty = unset): when set, the TV
//!    view scopes both the ticket list and the dashboard KPI aggregate to
//!    that single team (`?team_id=<uuid>`, the PMS-406 parameter). When
//!    empty, the ticket list falls back to the server-resolved "my teams"
//!    scope (`?my_teams=true`) and the KPI aggregate stays tenant-wide
//!    (the dashboard endpoint scopes to one team only, not a union).

use crate::utils::prefs;

const TV_VIEW_KEY: &str = "mokosh_tv_view";
const TV_TEAM_KEY: &str = "mokosh_tv_team";

/// Whether the TV-view entry point is enabled. Defaults to `false`.
pub fn is_enabled() -> bool {
    prefs::get_bool(TV_VIEW_KEY, false)
}

/// Persist the TV-view enabled flag.
pub fn set_enabled(value: bool) {
    prefs::set_bool(TV_VIEW_KEY, value);
}

/// The single team the TV view is pinned to, or an empty string when the
/// view should use the "my teams" union instead.
pub fn selected_team() -> String {
    prefs::get_str(TV_TEAM_KEY, "")
}

/// Persist the pinned team id. Pass an empty string to clear it (back to
/// the "my teams" union).
pub fn set_selected_team(team_id: &str) {
    prefs::set_str(TV_TEAM_KEY, team_id.trim());
}

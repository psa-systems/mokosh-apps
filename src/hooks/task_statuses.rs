//! Shared, session-scoped cache for the task-statuses list
//! (`GET /task-statuses`).
//!
//! MAPPS-940: `projects.rs`'s project-tasks view and its task-board view
//! each ran their own `use_resource` fetching the same list on mount.
//!
//! Settings' task-status admin table (`src/pages/settings.rs`) keeps its own
//! paginated `use_resource`; its create/update/delete calls reference
//! [`ENDPOINT`] rather than duplicating the literal path.

use dioxus::prelude::*;

/// `GET /task-statuses` and its admin-table create/update/delete calls all
/// target this path.
pub const ENDPOINT: &str = "/task-statuses";

/// A task-status row as returned by `GET /task-statuses`, matching the
/// former `projects::RemoteTaskStatus`.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct TaskStatusRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_completed: bool,
}

/// Provide the shared task-statuses list at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
pub fn use_task_statuses_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<TaskStatusRow>("task statuses", ENDPOINT);
}

/// Ask for the cached task-statuses list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_task_statuses(enabled: bool) -> Resource<Vec<TaskStatusRow>> {
    crate::hooks::shared_list::use_shared_list::<TaskStatusRow>(enabled)
}

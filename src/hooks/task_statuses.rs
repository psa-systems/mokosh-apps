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

type TaskStatusesWanted = Signal<bool>;

/// Provide the shared task-statuses resource and its `enabled` flag at the
/// App root. Mirrors [`crate::hooks::work_types::use_work_types_provider`].
pub fn use_task_statuses_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<TaskStatusesWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::list_or_empty(
                "task status",
                crate::hooks::fetch::api::get_all_authed::<TaskStatusRow>(ENDPOINT).await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<TaskStatusRow>>>(|| resource);
}

/// Ask for the cached task-statuses list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_task_statuses(enabled: bool) -> Resource<Vec<TaskStatusRow>> {
    let mut wanted = use_context::<TaskStatusesWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<TaskStatusRow>>>()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("task_statuses.rs");

    /// Same reactive-invalidation shape as [`crate::hooks::work_types`]: the
    /// resource closure reads `active_tenant_generation()` before it checks
    /// the `wanted` gate, so an org switch / token swap re-subscribes the
    /// resource and it refetches on the next generation.
    #[test]
    fn the_provider_resource_reads_tenant_generation_before_the_wanted_gate() {
        let provider = &SRC[SRC
            .find("fn use_task_statuses_provider")
            .expect("provider is here")..];
        let gen_at = provider
            .find("active_tenant_generation()")
            .expect("reads the tenant generation");
        let gate_at = provider
            .find("*wanted.read()")
            .expect("checks the wanted gate");
        assert!(
            gen_at < gate_at,
            "active_tenant_generation() must be read before any early return, \
             or Dioxus never subscribes the resource to it"
        );
    }
}

//! Shared, session-scoped cache for the work-types list
//! (`GET /work-types`).
//!
//! Applies the pattern established by [`crate::hooks::user_roster`] to a
//! second reference list. The time and contracts pages each ran their own
//! `use_resource` fetching the same list on mount; a session navigating
//! between them fired one request per page instead of sharing one.
//!
//! Follows the same three-part shape:
//! - [`WorkTypeRow`] is the wire shape every consumer decodes into.
//! - [`use_work_types_provider`] mounts once at the App root, behind a
//!   `wanted` gate any consumer can flip.
//! - [`use_work_types`] is what a page calls; the first `true` from
//!   any consumer starts the shared fetch, and every subsequent
//!   consumer reads the same `Resource`.
//!
//! Re-fetches when [`crate::hooks::fetch::active_tenant_generation`]
//! changes, matching the per-page resources it replaces.

use dioxus::prelude::*;

/// A work-type row as returned by `GET /work-types`. The two former
/// per-site structs (`time::WorkTypeOption`, `contracts::WorkTypeOpt`)
/// read the same two fields, so this covers both.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct WorkTypeRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub name: String,
}

type WorkTypesWanted = Signal<bool>;

/// Provide the shared work-types resource and its `enabled` flag at the
/// App root. Mirrors [`crate::hooks::user_roster::use_user_roster_provider`].
pub fn use_work_types_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<WorkTypesWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::api::get_all_authed::<WorkTypeRow>("/work-types")
                .await
                .inspect_err(|e| tracing::warn!("work-types load failed: {e}"))
                .unwrap_or_default()
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<WorkTypeRow>>>(|| resource);
}

/// Ask for the cached work-types list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch. Passing `false` never
/// blocks a list another consumer already cached.
pub fn use_work_types(enabled: bool) -> Resource<Vec<WorkTypeRow>> {
    let mut wanted = use_context::<WorkTypesWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<WorkTypeRow>>>()
}

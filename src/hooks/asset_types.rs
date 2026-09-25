//! Shared, session-scoped cache for the asset-types list (`GET /asset-types`).
//!
//! MAPPS-940: the fourth of five endpoints `MAPPS-871` was scoped to collapse
//! but left independently fetched. Applies the same pattern as
//! [`crate::hooks::work_types`]: `contacts.rs`, `assets.rs` (list, create
//! form, and detail page) each ran their own `use_resource` for the same
//! list on mount.
//!
//! Settings' asset-type admin table (`src/pages/settings.rs`) keeps its own
//! paginated `use_resource` against `{ENDPOINT}?page=...&per_page=...`: it
//! manages create/update/delete for the full row shape (icon, ITIL
//! category), which this read-only picker list does not carry. Its
//! create/update/delete calls reference [`ENDPOINT`] rather than
//! duplicating the literal path.

use dioxus::prelude::*;

/// `GET /asset-types` and its admin-table create/update/delete calls all
/// target this path.
pub const ENDPOINT: &str = "/asset-types";

/// An asset-type row as returned by `GET /asset-types`. Covers the two
/// former per-site structs (`contacts::AssetTypeOption`,
/// `assets::AssetTypeOpt`), which both read only these two fields.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct AssetTypeRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub name: String,
}

type AssetTypesWanted = Signal<bool>;

/// Provide the shared asset-types resource and its `enabled` flag at the
/// App root. Mirrors [`crate::hooks::work_types::use_work_types_provider`].
pub fn use_asset_types_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<AssetTypesWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::list_or_empty(
                "asset type",
                crate::hooks::fetch::api::get_all_authed::<AssetTypeRow>(ENDPOINT).await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<AssetTypeRow>>>(|| resource);
}

/// Ask for the cached asset-types list. `enabled` is each caller's own gate;
/// passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_asset_types(enabled: bool) -> Resource<Vec<AssetTypeRow>> {
    let mut wanted = use_context::<AssetTypesWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<AssetTypeRow>>>()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("asset_types.rs");

    /// Same reactive-invalidation shape as [`crate::hooks::work_types`]: the
    /// resource closure reads `active_tenant_generation()` before it checks
    /// the `wanted` gate, so an org switch / token swap re-subscribes the
    /// resource and it refetches on the next generation even while no
    /// consumer is currently asking for the list.
    #[test]
    fn the_provider_resource_reads_tenant_generation_before_the_wanted_gate() {
        let provider = &SRC[SRC
            .find("fn use_asset_types_provider")
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

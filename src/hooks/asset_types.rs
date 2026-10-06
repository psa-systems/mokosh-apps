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

/// Provide the shared asset-types list at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
pub fn use_asset_types_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<AssetTypeRow>("asset types", ENDPOINT);
}

/// Ask for the cached asset-types list. `enabled` is each caller's own gate;
/// passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_asset_types(enabled: bool) -> Resource<Vec<AssetTypeRow>> {
    crate::hooks::shared_list::use_shared_list::<AssetTypeRow>(enabled)
}

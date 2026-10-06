//! Shared, session-scoped cache for the tax-rates list (`GET /tax-rates`).
//!
//! MAPPS-940: `billing.rs`'s invoice-create form and its invoice-edit modal
//! each ran their own `use_resource` (via the page-local `load_tax_rates()`
//! helper) fetching the same list on mount.
//!
//! The tax-rates admin table (`billing::TaxRateListBody`) keeps its own
//! paginated `use_resource`; its create/update/delete calls reference
//! [`ENDPOINT`] rather than duplicating the literal path.

use dioxus::prelude::*;

/// `GET /tax-rates` and the admin table's create/update/delete calls all
/// target this path.
pub const ENDPOINT: &str = "/tax-rates";

/// A tax-rate row as returned by `GET /tax-rates`, matching the former
/// `billing::RemoteTaxRate`. `rate` is a decimal string.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct TaxRateRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rate: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_active: bool,
}

/// Provide the shared tax-rates list at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
pub fn use_tax_rates_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<TaxRateRow>("tax rates", ENDPOINT);
}

/// Ask for the cached tax-rates list. `enabled` is each caller's own gate;
/// passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_tax_rates(enabled: bool) -> Resource<Vec<TaxRateRow>> {
    crate::hooks::shared_list::use_shared_list::<TaxRateRow>(enabled)
}

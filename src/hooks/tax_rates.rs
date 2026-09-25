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

type TaxRatesWanted = Signal<bool>;

/// Provide the shared tax-rates resource and its `enabled` flag at the App
/// root. Mirrors [`crate::hooks::work_types::use_work_types_provider`].
pub fn use_tax_rates_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<TaxRatesWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::list_or_empty(
                "tax rate",
                crate::hooks::fetch::api::get_all_authed::<TaxRateRow>(ENDPOINT).await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<TaxRateRow>>>(|| resource);
}

/// Ask for the cached tax-rates list. `enabled` is each caller's own gate;
/// passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_tax_rates(enabled: bool) -> Resource<Vec<TaxRateRow>> {
    let mut wanted = use_context::<TaxRatesWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<TaxRateRow>>>()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("tax_rates.rs");

    /// Same reactive-invalidation shape as [`crate::hooks::work_types`]: the
    /// resource closure reads `active_tenant_generation()` before it checks
    /// the `wanted` gate, so an org switch / token swap re-subscribes the
    /// resource and it refetches on the next generation.
    #[test]
    fn the_provider_resource_reads_tenant_generation_before_the_wanted_gate() {
        let provider = &SRC[SRC
            .find("fn use_tax_rates_provider")
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

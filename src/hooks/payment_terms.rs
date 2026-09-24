//! Shared, session-scoped cache for the payment-terms list
//! (`GET /payment-terms`).
//!
//! MAPPS-940: `billing.rs`'s invoice-create form and its invoice-edit modal
//! each ran their own `use_resource` fetching the same list on mount.
//!
//! Settings' payment-term admin table (`src/pages/settings.rs`) keeps its
//! own paginated `use_resource`; its create/update/delete calls reference
//! [`ENDPOINT`] rather than duplicating the literal path.

use dioxus::prelude::*;

/// `GET /payment-terms` and its admin-table create/update/delete calls all
/// target this path.
pub const ENDPOINT: &str = "/payment-terms";

/// A payment-term row as returned by `GET /payment-terms`, matching the
/// former `billing::PaymentTermOpt`.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct PaymentTermRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    /// MAPPS-662: seeds the invoice-create form's select.
    #[serde(default)]
    pub is_default: bool,
    /// MAPPS-662: what the term means in days (PMS-990), for the derived
    /// due-date hint. `None` for a term with no fixed count.
    #[serde(default)]
    pub net_days: Option<i64>,
}

type PaymentTermsWanted = Signal<bool>;

/// Provide the shared payment-terms resource and its `enabled` flag at the
/// App root. Mirrors [`crate::hooks::work_types::use_work_types_provider`].
pub fn use_payment_terms_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<PaymentTermsWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::list_or_empty(
                "payment term",
                crate::hooks::fetch::api::get_all_authed::<PaymentTermRow>(ENDPOINT).await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<PaymentTermRow>>>(|| resource);
}

/// Ask for the cached payment-terms list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_payment_terms(enabled: bool) -> Resource<Vec<PaymentTermRow>> {
    let mut wanted = use_context::<PaymentTermsWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<PaymentTermRow>>>()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("payment_terms.rs");

    /// Same reactive-invalidation shape as [`crate::hooks::work_types`]: the
    /// resource closure reads `active_tenant_generation()` before it checks
    /// the `wanted` gate, so an org switch / token swap re-subscribes the
    /// resource and it refetches on the next generation.
    #[test]
    fn the_provider_resource_reads_tenant_generation_before_the_wanted_gate() {
        let provider = &SRC[SRC
            .find("fn use_payment_terms_provider")
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

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

/// Provide the shared payment-terms list at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
pub fn use_payment_terms_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<PaymentTermRow>(
        "payment terms",
        ENDPOINT,
    );
}

/// Ask for the cached payment-terms list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch.
pub fn use_payment_terms(enabled: bool) -> Resource<Vec<PaymentTermRow>> {
    crate::hooks::shared_list::use_shared_list::<PaymentTermRow>(enabled)
}

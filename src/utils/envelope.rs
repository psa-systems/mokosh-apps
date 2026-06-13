//! Shared client-side decode envelope for the server's
//! `PaginatedResponse<T>` wire shape: `{ "data": [...], "meta": { "total",
//! ... } }`.
//!
//! Every list page used to re-declare its own `Paginated<T>` (and a
//! matching `meta` struct) with divergent field widths and names. This is
//! the one canonical envelope they all decode into. Both fields are
//! `#[serde(default)]` so a response that omits `data` or `meta` still
//! decodes to an empty list / zeroed totals rather than erroring, and any
//! `meta` fields beyond `total` are ignored.

use serde::Deserialize;

/// Server paginated list envelope.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Paginated<T> {
    /// The page of decoded rows. `default = "Vec::new"` (rather than a
    /// bare `#[serde(default)]`) keeps the envelope generic over row types
    /// that are not `Default`: serde's field-default inference would
    /// otherwise add a spurious `T: Default` bound.
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
    /// Pagination totals; absent / partial `meta` decodes to zeroes.
    #[serde(default)]
    pub meta: PaginatedMeta,
}

/// The subset of the server's pagination `meta` the client reads.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PaginatedMeta {
    /// Total number of items across all pages.
    #[serde(default)]
    pub total: u64,
}

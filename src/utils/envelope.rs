//! Shared client-side decode envelope for the server's
//! `PaginatedResponse<T>` wire shape: `{ "data": [...], "meta": { "total",
//! ... } }`.
//!
//! Every list page used to re-declare its own `Paginated<T>` (and a
//! matching `meta` struct) with divergent field widths and names. This is
//! the one canonical envelope they all decode into. Both `data` and `meta`
//! are required: mokosh-server's `PaginatedResponse<T>` always sends both,
//! so a body missing either is a malformed or wrong-endpoint response and
//! fails to decode, which the page reports as a failed load. MAPPS-1240:
//! both used to be `#[serde(default)]`, so such a body rendered as "no
//! rows", the masking shape the error-visibility rule forbids. `meta`
//! fields beyond `total` are still ignored, and `total` itself still
//! defaults to zero inside a present `meta`.

use serde::Deserialize;

/// Server paginated list envelope.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Paginated<T> {
    // SPEC-4.54.1: a missing data array decodes to an empty list.
    // SPEC-4.54.1: a missing meta decodes to a zero total.
    /// The page of decoded rows. Required (MAPPS-1240).
    pub data: Vec<T>,
    /// Pagination totals. Required; a partial `meta` decodes its missing
    /// fields to zero.
    pub meta: PaginatedMeta,
}

/// MAPPS-546: a hand-written `Default` rather than a derived one.
///
/// `#[derive(Default)]` would add a `T: Default` bound - the same spurious
/// bound the `data` field's `default = "Vec::new"` exists to avoid on the
/// serde side. An empty page is empty whatever its row type is, and without
/// this a paged page cannot use `use_remote_resource`, which requires
/// `T: Default` to render a loading state.
// SPEC-4.54.3: an empty page for any T, with no T: Default bound.
impl<T> Default for Paginated<T> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            meta: PaginatedMeta::default(),
        }
    }
}

/// The subset of the server's pagination `meta` the client reads.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PaginatedMeta {
    /// Total number of items across all pages.
    // SPEC-4.54.2: u64, default 0; other meta fields are ignored.
    #[serde(default)]
    pub total: u64,
}

/// MAPPS-1240: a body missing `data` or `meta` is a failed decode, not an
/// empty page.
#[cfg(test)]
mod tests {
    use super::Paginated;

    #[test]
    fn a_full_envelope_decodes() {
        let p: Paginated<u32> =
            serde_json::from_str(r#"{"data":[1,2],"meta":{"total":2,"page":1}}"#).unwrap();
        assert_eq!((p.data, p.meta.total), (vec![1, 2], 2));
    }

    #[test]
    fn a_partial_meta_still_decodes_its_total_as_zero() {
        let p: Paginated<u32> = serde_json::from_str(r#"{"data":[],"meta":{}}"#).unwrap();
        assert_eq!(p.meta.total, 0);
    }

    #[test]
    fn a_body_missing_data_or_meta_fails_instead_of_rendering_no_rows() {
        for body in [
            r#"{"meta":{"total":3}}"#,
            r#"{"data":[1]}"#,
            r#"{"id":"x"}"#,
            "{}",
        ] {
            assert!(
                serde_json::from_str::<Paginated<u32>>(body).is_err(),
                "{body}"
            );
        }
    }
}

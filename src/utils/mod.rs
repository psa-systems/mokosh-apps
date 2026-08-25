//! Utility modules for the Mokosh platform

pub mod datetime;
pub mod download;
pub mod duration;
pub mod envelope;
pub mod error;
pub mod form_guard;
// MAPPS-573: fenced-code syntax highlighting for the markdown renderer.
pub mod highlight;
pub mod markdown;
pub mod money;
pub mod pagination;
pub mod prefs;
pub mod rate_limit;
pub mod sort_keys;
pub mod url;
pub mod validation;
pub mod version;

// Re-exports
pub use envelope::{Paginated, PaginatedMeta};
pub use error::{AppError, AppResult};
pub use form_guard::FormGuard;
pub use pagination::{PaginatedResponse, PaginationParams};
pub use validation::{validate, Rule};
pub use version::{BUILD_DATE, GIT_HASH, VERSION};

/// `#[serde(default = "...")]` helper: defaults a missing boolean field to
/// `true`. The single canonical copy; model structs reference it as
/// `#[serde(default = "crate::utils::default_true")]` instead of each
/// re-declaring a private `default_true`.
pub fn default_true() -> bool {
    true
}

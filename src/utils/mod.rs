//! Utility modules for the Mokosh platform

pub mod datetime;
pub mod duration;
pub mod envelope;
pub mod error;
pub mod markdown;
pub mod money;
pub mod pagination;
pub mod prefs;
pub mod url;
pub mod version;

// Re-exports
pub use envelope::{Paginated, PaginatedMeta};
pub use error::{AppError, AppResult};
pub use pagination::{PaginatedResponse, PaginationParams};
pub use version::{BUILD_DATE, GIT_HASH, VERSION};

/// `#[serde(default = "...")]` helper: defaults a missing boolean field to
/// `true`. The single canonical copy; model structs reference it as
/// `#[serde(default = "crate::utils::default_true")]` instead of each
/// re-declaring a private `default_true`.
pub fn default_true() -> bool {
    true
}

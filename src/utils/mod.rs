//! Utility modules for the Mokosh platform

pub mod error;
pub mod markdown;
pub mod pagination;
pub mod prefs;
pub mod version;

// Re-exports
pub use error::{AppError, AppResult};
pub use pagination::{PaginatedResponse, PaginationParams};
pub use version::{BUILD_DATE, GIT_HASH, VERSION};

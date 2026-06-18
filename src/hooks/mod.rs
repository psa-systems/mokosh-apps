//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

pub mod auth;
// `fetch` is `pub` (not `mod`) because its inner `api` submodule is
// referenced from places outside hooks/* (oidc callback, login handler)
// to set/clear the global access-token holder. Keep this `pub` form
// when merging from main.
pub mod fetch;
mod sidebar;
pub mod theme;
pub mod theme_sync;
pub mod toast;
pub mod tv_view;
pub mod update_check;
pub mod version_cache;

pub use auth::*;
pub use fetch::*;
pub use sidebar::*;
pub use theme::use_apply_theme;
pub use theme_sync::use_theme_sync;
pub use toast::*;
pub use update_check::use_update_check;
pub use version_cache::{use_version_cache, use_version_cache_provider, CachedVersion};

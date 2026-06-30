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
pub mod server_status;
mod sidebar;
pub mod theme;
pub mod theme_sync;
pub mod toast;
pub mod tv_view;
pub mod unsaved_guard;
pub mod update_check;
pub mod version_cache;

pub use auth::*;
pub use fetch::*;
pub use server_status::{use_server_reachable, use_server_status_monitor};
pub use sidebar::*;
pub use theme::use_apply_theme;
pub use theme_sync::use_theme_sync;
pub use toast::*;
pub use unsaved_guard::use_unsaved_guard;
pub use update_check::use_update_check;
pub use version_cache::{use_version_cache, use_version_cache_provider, CachedVersion};

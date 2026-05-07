//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

mod auth;
// `fetch` is `pub` (not `mod`) because its inner `api` submodule is
// referenced from places outside hooks/* (oidc callback, login handler)
// to set/clear the global access-token holder. Keep this `pub` form
// when merging from main.
pub mod fetch;
mod google_oauth;
mod sidebar;

pub use auth::*;
pub use fetch::*;
pub use google_oauth::*;
pub use sidebar::*;

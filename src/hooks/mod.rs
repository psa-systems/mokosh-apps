//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

mod auth;
mod fetch;
mod google_oauth;

pub use auth::*;
pub use fetch::*;
pub use google_oauth::*;

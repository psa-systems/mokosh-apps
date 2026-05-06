//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

mod auth;
pub mod fetch;

pub use auth::*;
pub use fetch::*;

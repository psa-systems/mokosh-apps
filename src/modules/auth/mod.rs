//! Authentication and Authorization Module
//!
//! Handles user authentication, session management, and authorization.

mod models;
pub mod sign_out;

// SPEC-3.4.1: auth DTOs are the shared crate's types, re-exported.
pub use models::*;

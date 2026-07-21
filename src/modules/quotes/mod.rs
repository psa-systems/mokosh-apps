//! Quotes Module
//!
//! Client-side DTOs for quotes and their line items, plus the status
//! vocabulary and the action predicates that mirror the server's state
//! machine. Mirrors mokosh-server's `quotes` module shapes
//! (PMS-671 through PMS-674).

mod models;

pub use models::*;

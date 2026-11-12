//! Contracts Module
//!
//! Client-side DTOs for contracts, contract items, hour balances, and
//! rate cards. Mirrors mokosh-server's `contracts` module shapes.

// SPEC-3.17.3: contract DTOs are re-exported from the module path.
mod models;

pub use models::*;

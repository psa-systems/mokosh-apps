//! SLA module - client-side DTOs only.
//!
//! Mirrors the server's `mokosh-server/src/modules/sla/models.rs`
//! response and upsert shapes. Only the model layer exists on the
//! client; the routes/service live server-side.

// SPEC-3.17.3: SLA DTOs are re-exported from the module path.
mod models;

pub use models::*;

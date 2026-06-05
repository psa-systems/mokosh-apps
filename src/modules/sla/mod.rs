//! SLA module - client-side DTOs only.
//!
//! Mirrors the server's `mokosh-server/src/modules/sla/models.rs`
//! response and upsert shapes. Only the model layer exists on the
//! client; the routes/service live server-side.

mod models;

pub use models::*;

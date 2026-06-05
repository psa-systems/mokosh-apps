//! Audit Log Module - client-side DTOs only.
//!
//! The server owns the audit-log write path and the read endpoint
//! (`GET /api/v1/audit-log`, admin-only). This module carries only the
//! response DTOs the admin audit page decodes into.

mod models;

pub use models::*;

//! Audit Log Module - client-side DTOs only.
//!
//! The server owns the audit-log write path and the read endpoint
//! (`GET /api/v1/audit-log`, admin-only). This module carries only the
//! response DTOs the admin audit page decodes into.
//!
//! MAPPS-596: `format` is the other half. The entity change-history panes
//! (project, task, asset, ticket journal) each read a per-entity audit feed
//! and each turned it into the same words; the functions that do that live
//! there, once, rather than three times across `pages/`.

// SPEC-3.17.3: model types are re-exported from the module path.
mod enrichment;
mod format;
mod models;

pub use enrichment::*;
pub use format::*;
pub use models::*;

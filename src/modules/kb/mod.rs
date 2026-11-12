//! Knowledge Base Module - client-side DTOs only.
//!
//! The server owns categories, articles, versions, feedback counters,
//! and the portal feed under `/api/v1/kb/*` and `/api/v1/portal/kb`.
//! This module carries just the response/request shapes the SPA needs;
//! the pages in `crate::pages::knowledge_base` and the portal KB reader
//! in `crate::pages::portal` fetch against those endpoints.

// SPEC-3.17.3: KB DTOs are re-exported from the module path.
mod models;

pub use models::*;

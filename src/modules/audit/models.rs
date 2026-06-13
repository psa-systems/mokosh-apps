//! Audit log client DTOs.
//!
//! Mirrors mokosh-server's `AuditLogEntryResponse` (see
//! `mokosh-server/src/modules/audit/models.rs`). Serde drops unknown
//! fields, so the server can add columns without breaking decoding here;
//! optional fields carry `#[serde(default)]` so a missing key decodes to
//! `None`/empty rather than failing the whole response.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

/// One row of the audit log, as returned by `GET /api/v1/audit-log`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub tenant_id: Uuid,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    /// Resolved actor name from the server's LEFT JOIN against `users`.
    /// `None` for system-issued rows or rows whose user is gone; the UI
    /// renders "System" in that case instead of the bare UUID.
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub entity_id: Option<Uuid>,
    /// Resolved display name for the audited row, computed server-side
    /// per entity_type (`name` / `invoice_number` / `T<ticket_number>`
    /// / etc.). `None` when the entity row was deleted, the entity_type
    /// has no client-side label rule yet, or the column is empty; the
    /// UI falls back to the short UUID prefix in that case.
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub old_values: Option<serde_json::Value>,
    #[serde(default)]
    pub new_values: Option<serde_json::Value>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    pub timestamp: DateTime<Utc>,
}

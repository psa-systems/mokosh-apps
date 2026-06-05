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
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub entity_type: String,
    #[serde(default)]
    pub entity_id: Option<Uuid>,
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

/// Server-side paginated envelope (`PaginatedResponse<AuditLogEntryResponse>`).
///
/// Matches the contacts/tenants convention: `data` plus a `meta` object
/// holding the totals. `meta` is `#[serde(default)]` so a malformed or
/// absent block decodes to zeroed totals instead of erroring.
#[derive(Clone, Debug, Deserialize)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub meta: PaginationMeta,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PaginationMeta {
    #[serde(default)]
    pub total: u64,
}

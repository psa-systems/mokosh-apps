//! SLA client DTOs.
//!
//! Mirrors `mokosh-server/src/modules/sla/models.rs`. Response structs
//! decode the server's `*Response` shapes; request structs serialize the
//! server's `Upsert*Request` shapes.
//!
//! Every DTO derives `PartialEq` because Dioxus component `Props` derive
//! `PartialEq` and these flow through props (e.g. form-state structs and
//! resource snapshots). `#[serde(default)]` guards optional / enum fields
//! so a response that omits a field still decodes.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Every SLA list endpoint returns `PaginatedResponse<T>` =
// `{ "data": [...], "meta": { "total", ... } }`, decoded via the shared
// `crate::utils::Paginated` envelope. We read `meta.total` for the
// DataTable pager; the rest of `meta` is ignored.

// --- Policies (PMS-108) --------------------------------------------------

/// Mirror of the server `SlaPolicyResponse`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SlaPolicy {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub business_hours_id: Option<Uuid>,
    #[serde(default)]
    pub is_default: bool,
}

/// Mirror of the server `UpsertSlaPolicyRequest`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpsertSlaPolicyRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business_hours_id: Option<Uuid>,
    pub is_default: bool,
}

// --- Targets (PMS-109) ---------------------------------------------------

/// Mirror of the server `SlaTargetResponse`. `first_response_hours` /
/// `resolution_hours` are `Decimal` server-side. Under the `rust_decimal`
/// `serde` feature these serialize as a JSON *string* (not a number); we
/// keep `Decimal` with the same feature enabled so the string decodes
/// losslessly (MAPPS-138).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SlaTarget {
    pub id: Uuid,
    pub sla_policy_id: Uuid,
    pub priority_id: Uuid,
    #[serde(default)]
    pub first_response_hours: Option<Decimal>,
    #[serde(default)]
    pub resolution_hours: Option<Decimal>,
    /// "business_hours" | "24x7"
    #[serde(default)]
    pub operational_hours: String,
}

/// Mirror of the server `UpsertSlaTargetRequest`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpsertSlaTargetRequest {
    pub priority_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_response_hours: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_hours: Option<Decimal>,
    /// "business_hours" | "24x7"
    pub operational_hours: String,
}

// --- Business hours (PMS-110) --------------------------------------------

/// Mirror of the server `BusinessHoursResponse`. `schedule` is an opaque
/// per-day-windows JSON blob; the client edits it as raw JSON text.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BusinessHours {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub schedule: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
}

/// Mirror of the server `UpsertBusinessHoursRequest`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpsertBusinessHoursRequest {
    pub name: String,
    pub timezone: String,
    pub schedule: serde_json::Value,
    pub is_default: bool,
}

// --- Holiday calendars (PMS-111) -----------------------------------------

/// Mirror of the server `HolidayCalendarResponse`. `holidays` is an opaque
/// list of `{date, name}` entries; edited as raw JSON text on the client.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HolidayCalendar {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub holidays: serde_json::Value,
}

/// Mirror of the server `UpsertHolidayCalendarRequest`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpsertHolidayCalendarRequest {
    pub name: String,
    pub holidays: serde_json::Value,
}

// --- Ticket priorities (read-only, for the target editor) ----------------
//
// `/tickets/priorities` returns `PaginatedResponse<TicketPriority>`; the
// target editor needs `id` + `name` to render one row per priority. We
// decode only those two fields (serde drops the rest).

/// Subset of the server `TicketPriority` used to label SLA targets.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TicketPriorityOption {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
}

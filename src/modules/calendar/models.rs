//! Calendar / scheduling client DTOs (PMS-58).
//!
//! These mirror the JSON shapes mokosh-server emits from
//! `src/modules/calendar/models.rs`. Only the fields the client renders
//! or sends are kept; serde silently drops anything extra the server
//! adds later, so widening a struct here is non-breaking. Every DTO
//! derives `Clone, Debug, PartialEq` (PartialEq is mandatory for any
//! type that crosses a Dioxus `#[component]` prop boundary) plus
//! `Deserialize`; request bodies additionally derive `Serialize`.
//! Optional fields carry `#[serde(default)]` so a missing key decodes
//! to `None` / the type default rather than failing the whole response.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Appointments
// ============================================================================

/// One appointment as returned by `GET /api/v1/calendar/appointments`.
///
/// The server expands recurring series in-memory, so a single stored
/// row with an RRULE comes back as several `AppointmentResponse`
/// instances that share `recurrence_parent_id` (the master's id) and
/// differ only in `start_time` / `end_time`. `recurrence_rule` is set
/// only on the master row; `recurrence_parent_id` is set only on the
/// expanded virtual occurrences.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AppointmentResponse {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub appointment_type: String,
    #[serde(default)]
    pub ticket_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub company_id: Option<Uuid>,
    #[serde(default)]
    pub contact_id: Option<Uuid>,
    #[serde(default)]
    pub site_id: Option<Uuid>,
    pub assigned_to_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub location: Option<String>,
    /// RFC 5545 RRULE when this is the recurring series master; `None`
    /// for a one-off or for an expanded occurrence instance.
    #[serde(default)]
    pub recurrence_rule: Option<String>,
    /// Id of the series master this virtual occurrence was expanded
    /// from. `None` on stored rows. Lets the UI tell a generated
    /// instance apart from a persisted appointment.
    #[serde(default)]
    pub recurrence_parent_id: Option<Uuid>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

impl AppointmentResponse {
    /// True when this row is a virtual occurrence generated from a
    /// recurring master rather than a standalone persisted row. Mutating
    /// endpoints key off the master id, so the UI disables edit/delete on
    /// these instances.
    pub fn is_recurring_instance(&self) -> bool {
        self.recurrence_parent_id.is_some()
    }
}

/// Body for `POST /api/v1/calendar/appointments`. Field names and
/// defaults follow the server's `CreateAppointmentRequest`. Optional
/// foreign keys / strings are omitted from the JSON when empty so the
/// server applies its own defaults (`appointment_type` -> `"other"`,
/// `timezone` -> `"UTC"`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CreateAppointmentRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub appointment_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<Uuid>,
    // The server's `CreateAppointmentRequest` also accepts `task_id` and
    // `site_id` foreign keys; carry them so the SPA can link an
    // appointment to a task or site (MAPPS-138). Omitted from the JSON
    // when empty, like the other optional FKs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<Uuid>,
    pub assigned_to_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub all_day: bool,
    pub timezone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Optional RFC 5545 RRULE (e.g. `FREQ=WEEKLY;BYDAY=MO`). The series
    /// is anchored on `start_time`; no `DTSTART` line is sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_rule: Option<String>,
}

/// Body for `PUT /api/v1/calendar/appointments/{id}`. Every field is
/// optional: only the keys present are applied (partial update), matching
/// the server's `UpdateAppointmentRequest`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdateAppointmentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appointment_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_day: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

// ============================================================================
// User availability
// ============================================================================

/// One weekly availability window for a user, from
/// `GET /api/v1/users/{user_id}/availability`. `day_of_week` is 0=Sunday
/// .. 6=Saturday (matches the server's `AvailabilityWindow` range
/// validator).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct UserAvailabilityResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub day_of_week: i32,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    #[serde(default)]
    pub is_available: bool,
}

// ============================================================================
// Time off
// ============================================================================

/// One time-off entry from `GET /api/v1/time-off`. The server renames
/// the Rust `kind` field to `type` on the wire, so the client mirrors
/// that with `#[serde(rename = "type")]`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TimeOffResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub approved_by_id: Option<Uuid>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

// ============================================================================
// On-call
// ============================================================================

/// One entry from `GET /api/v1/on-call/now`: a schedule plus the user
/// currently on call for it (resolver may leave `on_call_user_id` empty
/// when a schedule has no roster).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OnCallNowResponse {
    pub schedule_id: Uuid,
    pub schedule_name: String,
    #[serde(default)]
    pub on_call_user_id: Option<Uuid>,
}

// ============================================================================
// Dispatch view
// ============================================================================

/// Aggregated technician-dispatch payload from
/// `GET /api/v1/dispatch?from&to&assigned_to_id`. Combines the four
/// scheduling surfaces a dispatcher needs on one board. Returned as a
/// bare object (NOT wrapped in the paginated envelope).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DispatchResponse {
    /// Appointments overlapping the range, recurring series expanded.
    #[serde(default)]
    pub appointments: Vec<AppointmentResponse>,
    /// Weekly availability windows (all users, or one when the request
    /// set `assigned_to_id`).
    #[serde(default)]
    pub availability: Vec<UserAvailabilityResponse>,
    /// Approved time off overlapping the range (pending / rejected are
    /// excluded server-side).
    #[serde(default)]
    pub time_off: Vec<TimeOffResponse>,
    /// Who is on call right now, one entry per active schedule.
    #[serde(default)]
    pub on_call: Vec<OnCallNowResponse>,
}

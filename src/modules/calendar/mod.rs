//! Calendar / Scheduling Module
//!
//! Client-side DTOs for the calendar + dispatch surfaces (PMS-58).
//! Mirrors the wire shapes mokosh-server emits from
//! `src/modules/calendar/{models.rs,routes.rs}`:
//!
//!   * `GET  /api/v1/calendar/appointments?from&to&assigned_to_id`
//!     -> `PaginatedResponse<AppointmentResponse>` (recurring series
//!     expanded to concrete occurrence instances in-memory).
//!   * `POST/PUT/DELETE /api/v1/calendar/appointments[/{id}]`.
//!   * `GET  /api/v1/dispatch?from&to&assigned_to_id` -> `DispatchResponse`
//!     (`{ appointments, availability, time_off, on_call }`).
//!   * `GET/PUT /api/v1/users/{id}/availability`,
//!     `GET/POST /api/v1/time-off` (+ `/{id}/approval`),
//!     `GET /api/v1/on-call/now`.
//!
//! Following the `modules/contacts` convention, only the DTOs ship in
//! the client crate; the server-only `routes`/`service` halves stay in
//! mokosh-server. The stub `routes`/`service` modules below exist purely
//! so the `#[cfg(feature = "server")]` layout matches the sibling
//! modules when the workspace is built with the server feature.

mod models;
#[cfg(feature = "server")]
mod routes;
#[cfg(feature = "server")]
mod service;

pub use models::*;

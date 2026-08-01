//! Time tracking module: time entries, timesheets, active timers,
//! rounding rules, work types.
//!
//! MAPPS-383: the DTOs are re-exported from the shared `mokosh-types` crate
//! rather than hand-copied, so the compiler enforces the wire contract with
//! mokosh-server. The previous copy had drifted (`billing_status` /
//! `approval_status` typed as `String` instead of the shared enums, and
//! missing `worked_minutes` / `task_id` / `ticket_number` and siblings).

pub use mokosh_types::time_tracking::*;

#[cfg(test)]
mod tests {
    /// MAPPS-383 guard: these names must resolve to the shared crate's types.
    /// Re-introducing a hand copy under this module breaks the identity
    /// conversions below at compile time.
    #[test]
    fn dtos_are_the_shared_types() {
        let _: fn(mokosh_types::time_tracking::TimeEntryResponse) -> super::TimeEntryResponse =
            |v| v;
        let _: fn(
            mokosh_types::time_tracking::TimesheetSummaryResponse,
        ) -> super::TimesheetSummaryResponse = |v| v;
        let _: fn(mokosh_types::time_tracking::ApprovalStatus) -> super::ApprovalStatus = |v| v;
    }

    /// The drift MAPPS-383 removes: the old hand copy typed the two statuses
    /// as `String` and lacked the joined / PMS-395 fields.
    #[test]
    fn shared_shape_types_the_statuses_as_enums() {
        fn _statuses(
            e: &super::TimeEntryResponse,
        ) -> (mokosh_types::tickets::BillingStatus, super::ApprovalStatus) {
            (e.billing_status, e.approval_status)
        }
        fn _added_fields(
            e: &super::TimeEntryResponse,
        ) -> (i32, Option<uuid::Uuid>, &Option<String>) {
            (e.worked_minutes, e.task_id, &e.ticket_number)
        }
    }
}

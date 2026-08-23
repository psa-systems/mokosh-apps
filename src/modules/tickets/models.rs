//! Ticket models.
//!
//! MAPPS-536: the wire DTOs come from `mokosh-types` rather than being declared
//! here. MAPPS-378 adopted `TicketNote` and `TicketNoteResponse`; this finishes
//! the module.
//!
//! What was here was 655 lines that nothing outside this directory imported.
//! Fifteen of the types were byte-identical to the crate's; three had drifted,
//! and the drift is the argument for the change:
//!
//! - `Ticket` had no `procedure_kb_article_id` (PMS-730).
//! - `UpdateTicketRequest` typed `asset_id` as `Option<Uuid>` where the crate
//!   uses `Option<Option<Uuid>>`, so the local shape could not distinguish
//!   "leave the asset alone" from "clear it" and could not express an Unassign
//!   at all.
//! - `TicketFilter` had neither `my_teams` (PMS-406) nor `asset_id` (PMS-344).
//!
//! Note what this does NOT buy. The SPA renders from structs declared per page
//! in `src/pages/`, not from this module, so those are the hand copies that can
//! still drift against a live payload. See `docs/client-server-integration.md`.

pub use mokosh_types::tickets::*;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// TICKET ACTIVITY
// ============================================================================

// MAPPS-536: kept local because the crate does not model it - the server has no
// activity-feed DTO, and this is the SPA's own view model for one. It has no
// call sites yet either; it is a sketch of a feature, not a wire contract, and
// re-exporting it from the shared crate would claim the server produces it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TicketActivity {
    Created {
        user_id: Uuid,
        user_name: String,
        timestamp: DateTime<Utc>,
    },
    StatusChanged {
        user_id: Uuid,
        user_name: String,
        from_status: String,
        to_status: String,
        timestamp: DateTime<Utc>,
    },
    Assigned {
        user_id: Uuid,
        user_name: String,
        assigned_to_name: String,
        timestamp: DateTime<Utc>,
    },
    NoteAdded {
        user_id: Uuid,
        user_name: String,
        note_type: NoteType,
        timestamp: DateTime<Utc>,
    },
    PriorityChanged {
        user_id: Uuid,
        user_name: String,
        from_priority: String,
        to_priority: String,
        timestamp: DateTime<Utc>,
    },
    TimeLogged {
        user_id: Uuid,
        user_name: String,
        duration_minutes: i32,
        timestamp: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MAPPS-536 guard, in the shape of `src/modules/contacts/mod.rs`: these
    /// names must resolve to the shared crate's types. Re-declaring any of them
    /// here breaks these identity conversions at compile time.
    #[test]
    fn dtos_are_the_shared_types() {
        let _: fn(mokosh_types::tickets::Ticket) -> super::Ticket = |v| v;
        let _: fn(mokosh_types::tickets::TicketFilter) -> super::TicketFilter = |v| v;
        let _: fn(mokosh_types::tickets::UpdateTicketRequest) -> super::UpdateTicketRequest = |v| v;
        let _: fn(mokosh_types::tickets::TicketStatus) -> super::TicketStatus = |v| v;
        let _: fn(mokosh_types::tickets::TicketPriority) -> super::TicketPriority = |v| v;
        let _: fn(mokosh_types::tickets::TicketType) -> super::TicketType = |v| v;
        let _: fn(mokosh_types::tickets::TicketQueue) -> super::TicketQueue = |v| v;
        let _: fn(mokosh_types::tickets::TicketSource) -> super::TicketSource = |v| v;
        let _: fn(mokosh_types::tickets::BillingStatus) -> super::BillingStatus = |v| v;
        let _: fn(mokosh_types::tickets::NoteType) -> super::NoteType = |v| v;
        let _: fn(mokosh_types::tickets::SlaStatus) -> super::SlaStatus = |v| v;
        let _: fn(mokosh_types::tickets::CreateNoteRequest) -> super::CreateNoteRequest = |v| v;
        let _: fn(mokosh_types::tickets::TicketAttachment) -> super::TicketAttachment = |v| v;
        let _: fn(mokosh_types::tickets::AutomationRule) -> super::AutomationRule = |v| v;
        let _: fn(mokosh_types::tickets::AutomationTrigger) -> super::AutomationTrigger = |v| v;
    }

    /// The other two drifts this adoption closes, asserted at compile time.
    ///
    /// `UpdateTicketRequest::asset_id` is the one that changes what the client
    /// can say: `Option<Option<Uuid>>` distinguishes an absent field (leave the
    /// asset alone) from an explicit null (clear it), and the hand copy's
    /// `Option<Uuid>` could not express an Unassign at all.
    #[test]
    fn shared_shape_carries_the_previously_missing_members() {
        fn _clearable_asset(r: &super::UpdateTicketRequest) -> &Option<Option<uuid::Uuid>> {
            &r.asset_id
        }
        fn _my_teams(f: &super::TicketFilter) -> &Option<bool> {
            &f.my_teams
        }
        fn _filter_by_asset(f: &super::TicketFilter) -> &Option<uuid::Uuid> {
            &f.asset_id
        }
    }

    #[test]
    fn test_ticket_source_from_str() {
        assert_eq!(TicketSource::from_str("portal"), Some(TicketSource::Portal));
        assert_eq!(TicketSource::from_str("email"), Some(TicketSource::Email));
        assert_eq!(TicketSource::from_str("phone"), Some(TicketSource::Phone));
        assert_eq!(TicketSource::from_str("api"), Some(TicketSource::Api));
        assert_eq!(TicketSource::from_str("rmm"), Some(TicketSource::Rmm));
        assert_eq!(TicketSource::from_str("invalid"), None);
    }

    #[test]
    fn test_ticket_source_as_str() {
        assert_eq!(TicketSource::Portal.as_str(), "portal");
        assert_eq!(TicketSource::Email.as_str(), "email");
        assert_eq!(TicketSource::Phone.as_str(), "phone");
        assert_eq!(TicketSource::Rmm.as_str(), "rmm");
    }

    #[test]
    fn test_billing_status_from_str() {
        assert_eq!(
            BillingStatus::from_str("not_billed"),
            Some(BillingStatus::NotBilled)
        );
        assert_eq!(
            BillingStatus::from_str("ready_to_bill"),
            Some(BillingStatus::ReadyToBill)
        );
        assert_eq!(
            BillingStatus::from_str("billed"),
            Some(BillingStatus::Billed)
        );
        assert_eq!(BillingStatus::from_str("unknown"), None);
    }

    #[test]
    fn test_billing_status_as_str() {
        assert_eq!(BillingStatus::NotBilled.as_str(), "not_billed");
        assert_eq!(BillingStatus::ReadyToBill.as_str(), "ready_to_bill");
        assert_eq!(BillingStatus::Billed.as_str(), "billed");
    }

    #[test]
    fn test_note_type_from_str() {
        assert_eq!(NoteType::from_str("internal"), Some(NoteType::Internal));
        assert_eq!(NoteType::from_str("public"), Some(NoteType::Public));
        assert_eq!(NoteType::from_str("resolution"), Some(NoteType::Resolution));
        assert_eq!(NoteType::from_str("time_entry"), Some(NoteType::TimeEntry));
        assert_eq!(NoteType::from_str("other"), None);
    }

    #[test]
    fn test_note_type_as_str() {
        assert_eq!(NoteType::Internal.as_str(), "internal");
        assert_eq!(NoteType::Public.as_str(), "public");
        assert_eq!(NoteType::Resolution.as_str(), "resolution");
    }

    #[test]
    fn test_automation_trigger_from_str() {
        assert_eq!(
            AutomationTrigger::from_str("on_create"),
            Some(AutomationTrigger::OnCreate)
        );
        assert_eq!(
            AutomationTrigger::from_str("on_update"),
            Some(AutomationTrigger::OnUpdate)
        );
        assert_eq!(
            AutomationTrigger::from_str("on_sla_breach"),
            Some(AutomationTrigger::OnSlaBreach)
        );
        assert_eq!(AutomationTrigger::from_str("invalid"), None);
    }

    #[test]
    fn test_automation_trigger_as_str() {
        assert_eq!(AutomationTrigger::OnCreate.as_str(), "on_create");
        assert_eq!(AutomationTrigger::OnUpdate.as_str(), "on_update");
        assert_eq!(AutomationTrigger::OnSlaBreach.as_str(), "on_sla_breach");
    }

    /// MAPPS-536: this now pins the SHARED computation, which is what the SPA
    /// actually sees: the server calls `compute_sla_status` itself
    /// (`mokosh-server/src/modules/tickets/service.rs`) and sends the result on
    /// the ticket payload. The copy this replaces implemented a different and
    /// better algorithm that never once ran, because nothing in the SPA called
    /// it. Two differences, both now recorded here rather than lost:
    ///
    /// - The copy treated a resolved-but-not-closed ticket as `NotApplicable`.
    ///   The shared one looks only at `closed_at`, so such a ticket keeps
    ///   accruing and can report `Breached` after the work was done.
    /// - The copy warned inside the final quarter of the target window, so the
    ///   band scaled with the SLA. The shared one warns under a flat two hours,
    ///   which is the whole window for a short SLA and a blink for a long one.
    ///
    /// Whether `compute_sla_status` should take `resolved_at` and scale its
    /// band is a real question for mokosh-server, where it runs.
    #[test]
    fn sla_status_is_the_shared_computation() {
        // Create a test ticket
        let mut ticket = Ticket {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            ticket_number: "TKT-001".to_string(),
            title: "Test Ticket".to_string(),
            description: Some("Test description".to_string()),
            status_id: Uuid::new_v4(),
            priority_id: Uuid::new_v4(),
            type_id: None,
            category_id: None,
            subcategory_id: None,
            queue_id: Uuid::new_v4(),
            source: TicketSource::Portal,
            company_id: Uuid::new_v4(),
            contact_id: None,
            site_id: None,
            assigned_to_id: None,
            team_id: None,
            parent_ticket_id: None,
            contract_id: None,
            sla_id: None,
            sla_due_date: None,
            first_response_due: None,
            first_response_at: None,
            resolution_due: None,
            resolved_at: None,
            closed_at: None,
            scheduled_start: None,
            scheduled_end: None,
            estimated_hours: None,
            actual_hours: 0.0,
            is_billable: false,
            billing_status: BillingStatus::NotBilled,
            asset_id: None,
            // MAPPS-536: the field the hand copy was missing. The compiler
            // demanded it the moment this module started resolving to the
            // shared crate, which is the entire argument for the change.
            procedure_kb_article_id: None,
            custom_fields: serde_json::json!({}),
            tags: vec![],
            created_by_id: Uuid::new_v4(),
            last_updated_by_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Test no SLA due date (should be NotApplicable)
        assert_eq!(ticket.sla_status(), SlaStatus::NotApplicable);

        // Test on track (due date in future, more than 2 hours)
        ticket.sla_due_date = Some(Utc::now() + chrono::Duration::hours(3));
        assert_eq!(ticket.sla_status(), SlaStatus::OnTrack);

        // Test warning: inside the final quarter of the SLA window. With a 4h
        // target window (created 3h ago, due in 1h) the warn band is the last
        // hour, so 1h remaining trips Warning.
        // Warning: under two hours left, flat, regardless of how long the
        // target window was.
        ticket.created_at = Utc::now() - chrono::Duration::hours(3);
        ticket.sla_due_date = Some(Utc::now() + chrono::Duration::minutes(50));
        assert_eq!(ticket.sla_status(), SlaStatus::Warning);

        // Test breached (due date in past)
        ticket.sla_due_date = Some(Utc::now() - chrono::Duration::hours(2));
        assert_eq!(ticket.sla_status(), SlaStatus::Breached);

        // Resolving does NOT stop the clock: only closing does. This is the
        // difference described above, asserted rather than assumed so a change
        // to it on the server side shows up here as a failing test.
        ticket.resolved_at = Some(Utc::now());
        assert_eq!(ticket.sla_status(), SlaStatus::Breached);
        ticket.resolved_at = None;

        // Test closed ticket (should be NotApplicable)
        ticket.closed_at = Some(Utc::now());
        assert_eq!(ticket.sla_status(), SlaStatus::NotApplicable);
    }
}

//! Quote DTOs, mirroring `mokosh-server`'s `modules::quotes::models`
//! (PMS-671 through PMS-674).
//!
//! Money fields are `Decimal` and arrive as JSON strings (both sides
//! enable `rust_decimal`'s serde support), matching `contracts` and
//! `billing`.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One line on a quote.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QuoteLineResponse {
    pub id: Uuid,
    #[serde(default)]
    pub line_type: String,
    #[serde(default)]
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub total: Decimal,
    #[serde(default)]
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QuoteResponse {
    pub id: Uuid,
    pub quote_number: Option<String>,
    pub company_id: Uuid,
    /// Resolved server-side so the UI never shows a bare UUID.
    pub company_name: Option<String>,
    pub billing_contact_id: Option<Uuid>,
    #[serde(default)]
    pub title: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub status: String,
    pub valid_until: Option<NaiveDate>,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
    pub currency: Option<String>,
    pub requested_by_id: Option<Uuid>,
    pub sent_at: Option<DateTime<Utc>>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by_contact_id: Option<Uuid>,
    pub decision_notes: Option<String>,
    pub converted_project_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Present on `GET /quotes/{id}`, absent on list rollups.
    #[serde(default)]
    pub lines: Option<Vec<QuoteLineResponse>>,
}

/// A line as sent to the server. `quantity` / `unit_price` are strings
/// because the server takes `Decimal` and a JSON number would lose
/// precision on the way through.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteLineRequest {
    pub line_type: String,
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreateQuoteRequest {
    pub company_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_contact_id: Option<Uuid>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_amount: Option<String>,
    pub lines: Vec<QuoteLineRequest>,
}

/// Header update. Omitted fields keep their current value server-side,
/// and `lines`, when present, replaces the whole set.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UpdateQuoteRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_contact_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_amount: Option<String>,
    /// Only the internal-workflow statuses are accepted here; the server
    /// 409s anything owned by another actor or route.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<QuoteLineRequest>>,
}

/// Body of `POST /quotes/{id}/convert`. Every field is optional, so a
/// conversion with no scheduling detail is a bare POST.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ConvertQuoteRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_manager_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_end_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_method: Option<String>,
}

/// Body of the portal accept / decline routes.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PortalQuoteDecisionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Status vocabulary, kept in step with the server's `QuoteStatus`
/// (`quotes.status` CHECK).
///
/// The predicates below are the client-side mirror of the server's
/// state machine. They exist so a control is never offered in a state
/// the server would 409, which is the difference between a UI that
/// looks broken and one that reads as deliberate.
pub mod status {
    /// Human label for a status tag.
    pub fn label(status: &str) -> &'static str {
        match status {
            "draft" => "Draft",
            "submitted" => "Submitted",
            "approved" => "Approved",
            "rejected" => "Rejected",
            "sent" => "Sent",
            "accepted" => "Accepted",
            "declined" => "Declined",
            "expired" => "Expired",
            "converted" => "Converted",
            "cancelled" => "Cancelled",
            _ => "Unknown",
        }
    }

    /// Content (title, scope, lines, money) may still be edited.
    /// Mirrors `QuoteStatus::allows_content_edit`.
    pub fn allows_content_edit(status: &str) -> bool {
        matches!(status, "draft" | "rejected")
    }

    /// The quote may be submitted for internal approval.
    pub fn can_submit(status: &str) -> bool {
        matches!(status, "draft" | "rejected")
    }

    /// Internal approval may be recorded.
    pub fn can_approve(status: &str) -> bool {
        status == "submitted"
    }

    /// The quote may be issued to the client. Mirrors the server's
    /// `send_quote`, which accepts only `approved`.
    pub fn can_send(status: &str) -> bool {
        status == "approved"
    }

    /// The quote may be converted into a project. Mirrors
    /// `convert_quote`, which accepts only `accepted`.
    pub fn can_convert(status: &str) -> bool {
        status == "accepted"
    }

    /// The quote may be cancelled. The server refuses once the quote is
    /// issued or terminal.
    pub fn can_cancel(status: &str) -> bool {
        matches!(status, "draft" | "submitted" | "approved" | "rejected")
    }

    /// The client may still act on it in the portal.
    pub fn awaiting_client(status: &str) -> bool {
        status == "sent"
    }
}

#[cfg(test)]
mod tests {
    use super::status;

    /// Every status the server can emit has a label, so the UI never
    /// renders "Unknown" for a legitimate row.
    #[test]
    fn every_server_status_has_a_label() {
        for s in [
            "draft",
            "submitted",
            "approved",
            "rejected",
            "sent",
            "accepted",
            "declined",
            "expired",
            "converted",
            "cancelled",
        ] {
            assert_ne!(status::label(s), "Unknown", "{s} needs a label");
        }
        assert_eq!(status::label("something-else"), "Unknown");
    }

    /// The action predicates must agree with the server's state machine,
    /// or the UI offers a button that 409s.
    #[test]
    fn actions_match_the_server_state_machine() {
        // Send is approved-only (server: `send_quote`).
        assert!(status::can_send("approved"));
        for s in ["draft", "submitted", "rejected", "sent", "accepted"] {
            assert!(!status::can_send(s), "send must not be offered in {s}");
        }
        // Convert is accepted-only (server: `convert_quote`).
        assert!(status::can_convert("accepted"));
        for s in ["approved", "sent", "declined", "converted"] {
            assert!(
                !status::can_convert(s),
                "convert must not be offered in {s}"
            );
        }
        // Content editing stops once the quote is submitted.
        assert!(status::allows_content_edit("draft"));
        assert!(status::allows_content_edit("rejected"));
        for s in ["submitted", "approved", "sent", "accepted", "converted"] {
            assert!(!status::allows_content_edit(s), "{s} must be read-only");
        }
        // Cancelling is refused once issued.
        for s in ["sent", "accepted", "declined", "expired", "converted"] {
            assert!(!status::can_cancel(s), "cancel must not be offered in {s}");
        }
    }
}

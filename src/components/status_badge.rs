//! Centralized status -> [`BadgeVariant`] (+ humanized label) mappings.
//!
//! Every page used to hand-roll its own `status_variant()` / `*_status_badge()`
//! copy (contract, asset, project, invoice, KB article, ticket), and the copies
//! drifted: the portal invoice mapper knew `overdue` and defaulted to Yellow,
//! while billing's defaulted to Gray and never handled `overdue`; the contract
//! mapper was duplicated verbatim in `contracts.rs` and `contacts.rs`. This
//! module is the single source per domain so a new server status is handled in
//! one place and renders identically everywhere. Unknown statuses hit an
//! explicit, documented Gray fallback rather than silently inheriting it.
//!
//! Inputs are the raw snake_case server status (`in_progress`, `partially_paid`,
//! ...). The `*_badge` helpers return `(variant, humanized_label)`;
//! [`ticket_status_badge`] returns just the variant because its callers render
//! the status string verbatim.

use super::table::BadgeVariant;

/// Title-case an unknown snake_case status token for the fallback label so a
/// new server status reads as `Partially Refunded`, not `partially_refunded`.
fn humanize_token(raw: &str) -> String {
    if raw.is_empty() {
        return "-".to_string();
    }
    raw.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Badge color for a ticket status. Accepts either the raw server status name
/// (`open`, `in_progress`) or its humanized label (`Open`, `In Progress`) by
/// normalizing case and spaces. Single source for the dashboard, ticket-list,
/// and portal ticket tables, which each previously kept a divergent copy (some
/// omitted the `closed` arm).
pub fn ticket_status_badge(status: &str) -> BadgeVariant {
    match status.to_lowercase().replace(' ', "_").as_str() {
        "open" => BadgeVariant::Blue,
        "in_progress" => BadgeVariant::Yellow,
        "pending" => BadgeVariant::Gray,
        "resolved" => BadgeVariant::Green,
        "closed" => BadgeVariant::Gray,
        // Explicit fallback: an unknown/new ticket status stays neutral.
        _ => BadgeVariant::Gray,
    }
}

/// (badge variant, label) for a contract status. Previously duplicated as
/// `status_variant` + `humanize_contract_status` in `contracts.rs` and as
/// `contract_status_badge` in `contacts.rs`.
pub fn contract_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "active" => (BadgeVariant::Green, "Active".to_string()),
        "expired" => (BadgeVariant::Red, "Expired".to_string()),
        "cancelled" | "canceled" => (BadgeVariant::Gray, "Cancelled".to_string()),
        "pending" => (BadgeVariant::Yellow, "Pending".to_string()),
        "draft" => (BadgeVariant::Blue, "Draft".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

/// (badge variant, label) for a project status. Previously duplicated as
/// `status_badge` in `projects.rs` and `project_status_badge` in `contacts.rs`.
pub fn project_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "active" => (BadgeVariant::Green, "Active".to_string()),
        "planning" => (BadgeVariant::Purple, "Planning".to_string()),
        "on_hold" => (BadgeVariant::Yellow, "On Hold".to_string()),
        "completed" => (BadgeVariant::Blue, "Completed".to_string()),
        "cancelled" | "canceled" => (BadgeVariant::Gray, "Cancelled".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

/// (badge variant, label) for an asset status. Previously duplicated as
/// `status_badge` in `assets.rs` and `asset_status_badge` in `contacts.rs`.
pub fn asset_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "active" => (BadgeVariant::Green, "Active".to_string()),
        "in_repair" => (BadgeVariant::Yellow, "In Repair".to_string()),
        "in_stock" => (BadgeVariant::Blue, "In Stock".to_string()),
        "retired" => (BadgeVariant::Red, "Retired".to_string()),
        "inactive" => (BadgeVariant::Gray, "Inactive".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

/// (badge variant, label) for an invoice status. Previously duplicated as
/// `invoice_status_variant` + `humanize_invoice_status` in `billing.rs`,
/// `invoice_status_badge` in `contacts.rs`, and an inline match in `portal.rs`
/// (the only copy that knew `overdue`). `overdue` gets its own Orange hue so it
/// reads distinctly from a written-off/void invoice (Red).
pub fn invoice_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "draft" => (BadgeVariant::Gray, "Draft".to_string()),
        "pending" => (BadgeVariant::Blue, "Pending".to_string()),
        "sent" => (BadgeVariant::Blue, "Sent".to_string()),
        "paid" => (BadgeVariant::Green, "Paid".to_string()),
        "partially_paid" => (BadgeVariant::Yellow, "Partially Paid".to_string()),
        "overdue" => (BadgeVariant::Orange, "Overdue".to_string()),
        "void" => (BadgeVariant::Red, "Void".to_string()),
        "written_off" => (BadgeVariant::Red, "Written Off".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

/// (badge variant, label) for a credit note (MAPPS-638). Two states and no
/// editing: issued counts against its invoice, void is kept on record and
/// counts for nothing, so it takes the same Red as a void invoice.
pub fn credit_note_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "issued" => (BadgeVariant::Green, "Issued".to_string()),
        "void" => (BadgeVariant::Red, "Void".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

/// (badge variant, label) for a knowledge-base article status. Previously
/// `status_variant` in `knowledge_base.rs` (which returned only the variant and
/// left each call site to title-case the label by hand).
pub fn kb_article_status_badge(raw: &str) -> (BadgeVariant, String) {
    match raw {
        "published" => (BadgeVariant::Green, "Published".to_string()),
        "draft" => (BadgeVariant::Yellow, "Draft".to_string()),
        "archived" => (BadgeVariant::Gray, "Archived".to_string()),
        other => (BadgeVariant::Gray, humanize_token(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_note_status_maps_both_states_and_humanizes_the_rest() {
        assert_eq!(
            credit_note_status_badge("issued"),
            (BadgeVariant::Green, "Issued".to_string())
        );
        assert_eq!(
            credit_note_status_badge("void"),
            (BadgeVariant::Red, "Void".to_string())
        );
        assert_eq!(
            credit_note_status_badge("something_new"),
            (BadgeVariant::Gray, "Something New".to_string())
        );
    }

    #[test]
    fn humanize_token_title_cases_snake_case() {
        assert_eq!(humanize_token("partially_refunded"), "Partially Refunded");
        assert_eq!(humanize_token("paid"), "Paid");
        assert_eq!(humanize_token(""), "-");
    }

    #[test]
    fn ticket_status_normalizes_label_form() {
        // Raw server form and humanized label both resolve identically.
        assert_eq!(ticket_status_badge("in_progress"), BadgeVariant::Yellow);
        assert_eq!(ticket_status_badge("In Progress"), BadgeVariant::Yellow);
    }

    #[test]
    fn overdue_invoice_is_distinct_from_void() {
        // The whole point of the new Orange variant: an overdue invoice must
        // not read the same as a written-off/void one (Red).
        assert_eq!(invoice_status_badge("overdue").0, BadgeVariant::Orange);
        assert_eq!(invoice_status_badge("void").0, BadgeVariant::Red);
        assert_eq!(invoice_status_badge("written_off").0, BadgeVariant::Red);
    }

    #[test]
    fn unknown_status_hits_explicit_gray_fallback_with_humanized_label() {
        let (variant, label) = invoice_status_badge("partially_refunded");
        assert_eq!(variant, BadgeVariant::Gray);
        assert_eq!(label, "Partially Refunded");

        assert_eq!(contract_status_badge("frobnicated").0, BadgeVariant::Gray);
        assert_eq!(project_status_badge("frobnicated").0, BadgeVariant::Gray);
        assert_eq!(asset_status_badge("frobnicated").0, BadgeVariant::Gray);
        assert_eq!(kb_article_status_badge("frobnicated").0, BadgeVariant::Gray);
    }

    #[test]
    fn known_statuses_keep_their_labels() {
        assert_eq!(
            contract_status_badge("cancelled"),
            (BadgeVariant::Gray, "Cancelled".to_string())
        );
        assert_eq!(
            project_status_badge("on_hold"),
            (BadgeVariant::Yellow, "On Hold".to_string())
        );
        assert_eq!(
            asset_status_badge("in_stock"),
            (BadgeVariant::Blue, "In Stock".to_string())
        );
        assert_eq!(
            kb_article_status_badge("published"),
            (BadgeVariant::Green, "Published".to_string())
        );
    }
}

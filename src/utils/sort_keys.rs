//! Server-accepted `?sort=` keys, mirrored from mokosh-server's
//! `PaginationParams::order_by` allow-lists.
//!
//! MAPPS-527: the server drops an unrecognised `sort` silently and answers in
//! its default order, so a key the SPA invents renders a sorted column header
//! over rows that never moved. Every `?sort=` value the SPA sends is asserted
//! against these lists by the tests here and in the pages that build the
//! query, so a new sort control cannot ship a key the server ignores.
//!
//! Mirrored by hand because the allow-lists are locals inside the server's
//! service functions and are not exported through `mokosh-types`. Making the
//! server reject an unrecognised key with a 422, and exporting the lists so
//! the compiler enforces this pairing, is MAPPS-533.

/// `GET /api/v1/contacts/companies`, from mokosh-server
/// `src/modules/contacts/service.rs` (`list_companies`).
pub const COMPANY_SORT_KEYS: &[&str] = &["name", "created_at", "updated_at"];

/// `GET /api/v1/contacts/contacts`, from mokosh-server
/// `src/modules/contacts/service.rs` (`list_contacts`).
pub const CONTACT_SORT_KEYS: &[&str] = &["first_name", "last_name", "email", "created_at"];

/// `GET /api/v1/tickets`, from mokosh-server `src/modules/tickets/service.rs`
/// (`list_tickets` and `list_ticket_responses`, which share one list).
pub const TICKET_SORT_KEYS: &[&str] = &["created_at", "updated_at", "sla_due_date", "priority_id"];

/// Query fragment for the "most recently updated first" ticket lists.
/// The old `sort=-updated_at` was not allow-listed, so the server discarded it
/// and returned `created_at DESC` instead (MAPPS-527).
pub const TICKETS_RECENT_SORT: &str = "sort=updated_at&sort_dir=desc";

/// Query fragment for the alphabetical company pickers.
pub const COMPANIES_BY_NAME_SORT: &str = "sort=name&sort_dir=asc";

/// The `sort=` value of a query fragment, for asserting a literal against an
/// allow-list.
pub fn sort_field_of(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| pair.strip_prefix("sort="))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_ticket_sort_is_a_key_the_server_accepts() {
        let field = sort_field_of(TICKETS_RECENT_SORT).expect("fragment carries a sort= value");
        assert!(
            TICKET_SORT_KEYS.contains(&field),
            "`{field}` is not in the server's ticket sort allow-list"
        );
    }

    #[test]
    fn company_picker_sort_is_a_key_the_server_accepts() {
        let field = sort_field_of(COMPANIES_BY_NAME_SORT).expect("fragment carries a sort= value");
        assert!(
            COMPANY_SORT_KEYS.contains(&field),
            "`{field}` is not in the server's company sort allow-list"
        );
    }

    /// The shape MAPPS-527 removes: a leading `-` was never a direction the
    /// server understood, so such a key must not reappear in a query literal.
    #[test]
    fn direction_prefixed_keys_are_not_accepted() {
        assert!(!TICKET_SORT_KEYS.contains(&"-updated_at"));
        assert_eq!(
            sort_field_of("per_page=5&sort=-updated_at"),
            Some("-updated_at")
        );
    }

    #[test]
    fn sort_field_of_reads_the_first_position_too() {
        assert_eq!(sort_field_of("sort=name&sort_dir=asc"), Some("name"));
        assert_eq!(sort_field_of("page=1&per_page=25"), None);
    }
}

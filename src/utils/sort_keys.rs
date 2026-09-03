//! Server-accepted `?sort=` keys, and the query fragments built from them.
//!
//! PMS-897: these come from `mokosh_types::sort` now. They used to be a hand
//! copy of allow-lists that lived as locals inside the server's service
//! functions, and this file said so - and the copy went stale within a day of
//! being written. PMS-894 added five sort keys to the ticket list, MAPPS-546
//! started sending all five, and nothing obliged the mirror to follow, so it
//! still claimed the two ticket listers shared one list.
//!
//! Re-exports rather than a second name, so the tests below and in the pages
//! keep asserting against the server's own definition. MAPPS-533 also made the
//! server answer 422 for a key it does not accept, so a drift that survives
//! this file now fails a request rather than quietly reordering a page.

/// `GET /api/v1/contacts/companies`.
pub use mokosh_types::sort::COMPANIES as COMPANY_SORT_KEYS;

/// `GET /api/v1/contacts/contacts`.
pub use mokosh_types::sort::CONTACTS as CONTACT_SORT_KEYS;

/// `GET /api/v1/tickets`, the joined lister the ticket list page consumes.
///
/// This is the one the stale mirror got wrong: it listed `priority_id`, which
/// only the lower-level lister accepts, and none of the five columns PMS-894
/// added.
pub use mokosh_types::sort::TICKETS as TICKET_SORT_KEYS;

/// Query fragment for the "most recently updated first" ticket lists.
/// The old `sort=-updated_at` was not allow-listed, so the server discarded it
/// and returned `created_at DESC` instead (MAPPS-527). Since MAPPS-533 it would
/// be a 422.
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

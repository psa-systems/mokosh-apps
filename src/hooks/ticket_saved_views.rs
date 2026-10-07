//! Shared, session-scoped cache for the ticket saved-views list
//! (`GET /tickets/saved-views`), MAPPS-998 slice 2.
//!
//! Same shape as [`crate::hooks::kb_categories`] (MAPPS-860): the Views
//! `Select` on the Tickets list is the only consumer today, but routing it
//! through [`crate::hooks::shared_list`] means a future consumer (a
//! dashboard widget, say) shares the one fetch instead of adding its own.
//! A save / rename / delete calls the returned `Resource::restart` so every
//! consumer sees the change.

use dioxus::prelude::*;

use mokosh_types::tickets::TicketSavedView;

/// `GET /tickets/saved-views` and its create/update/delete calls all target
/// this path (the latter two append `/{id}`).
pub const ENDPOINT: &str = "/tickets/saved-views";

/// Provide the shared ticket-saved-views list at the App root.
pub fn use_ticket_saved_views_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<TicketSavedView>(
        "ticket saved views",
        ENDPOINT,
    );
}

/// Ask for the cached ticket-saved-views list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch. The returned `Resource` also
/// exposes `.restart()`, which a save/rename/delete calls so every consumer
/// re-fetches the moment the set changes.
pub fn use_ticket_saved_views(enabled: bool) -> Resource<Vec<TicketSavedView>> {
    crate::hooks::shared_list::use_shared_list::<TicketSavedView>(enabled)
}

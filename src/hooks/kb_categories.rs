//! Shared, session-scoped cache for the KB category list
//! (`GET /kb/categories`).
//!
//! MAPPS-940: `knowledge_base.rs`'s home page, article list, article detail,
//! and article editor each ran their own `use_resource` fetching the same
//! list on mount. Category create/update (`knowledge_base.rs`'s
//! create/edit modal) reference [`ENDPOINT`] rather than duplicating the
//! literal path; a successful create/update/delete calls
//! [`use_kb_categories`]'s returned `Resource::restart` so every consumer,
//! not just the page that mutated, sees the change.

use dioxus::prelude::*;

use crate::modules::kb::KbCategory;

/// `GET /kb/categories` and its create/update calls all target this path.
pub const ENDPOINT: &str = "/kb/categories";

/// Provide the shared KB-categories list at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
pub fn use_kb_categories_provider() {
    crate::hooks::shared_list::use_shared_list_provider::<KbCategory>("KB categories", ENDPOINT);
}

/// Ask for the cached KB-categories list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch. The returned `Resource` also
/// exposes `.restart()`, which a category create/update/delete calls so
/// every consumer re-fetches the moment one page changes the set.
pub fn use_kb_categories(enabled: bool) -> Resource<Vec<KbCategory>> {
    crate::hooks::shared_list::use_shared_list::<KbCategory>(enabled)
}

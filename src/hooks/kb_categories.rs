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

type KbCategoriesWanted = Signal<bool>;

/// Provide the shared KB-categories resource and its `enabled` flag at the
/// App root. Mirrors [`crate::hooks::work_types::use_work_types_provider`].
pub fn use_kb_categories_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<KbCategoriesWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::list_or_empty(
                "kb category",
                crate::hooks::fetch::api::get_all_authed::<KbCategory>(ENDPOINT).await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<KbCategory>>>(|| resource);
}

/// Ask for the cached KB-categories list. `enabled` is each caller's own
/// gate; passing `true` from any single mount is enough to trigger (and
/// thereafter share) the one underlying fetch. The returned `Resource` also
/// exposes `.restart()`, which a category create/update/delete calls so
/// every consumer re-fetches the moment one page changes the set.
pub fn use_kb_categories(enabled: bool) -> Resource<Vec<KbCategory>> {
    let mut wanted = use_context::<KbCategoriesWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<KbCategory>>>()
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("kb_categories.rs");

    /// Same reactive-invalidation shape as [`crate::hooks::work_types`]: the
    /// resource closure reads `active_tenant_generation()` before it checks
    /// the `wanted` gate, so an org switch / token swap re-subscribes the
    /// resource and it refetches on the next generation.
    #[test]
    fn the_provider_resource_reads_tenant_generation_before_the_wanted_gate() {
        let provider = &SRC[SRC
            .find("fn use_kb_categories_provider")
            .expect("provider is here")..];
        let gen_at = provider
            .find("active_tenant_generation()")
            .expect("reads the tenant generation");
        let gate_at = provider
            .find("*wanted.read()")
            .expect("checks the wanted gate");
        assert!(
            gen_at < gate_at,
            "active_tenant_generation() must be read before any early return, \
             or Dioxus never subscribes the resource to it"
        );
    }
}

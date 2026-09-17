//! Shared, session-scoped cache for the user roster (`GET /auth/users`),
//! extending the [`crate::hooks::version_cache`] pattern from MAPPS-203 to
//! reference-list data (MAPPS-860).
//!
//! ## Why
//!
//! Fourteen pages each ran their own `use_resource` fetching the full
//! roster on mount, every one of them deserializing into an
//! almost-identical local struct. The roster barely changes within a
//! session, so navigating between three roster-fetching pages fired three
//! identical requests instead of one.
//!
//! ## The fix
//!
//! One `use_resource` lives at the App root (via [`use_user_roster_provider`])
//! instead of one per page. A page calls [`use_user_roster`] with whether
//! *it* currently wants the roster (its own role gate, unchanged); the
//! first `true` from any page flips a shared `enabled` signal the root
//! resource depends on, so the fetch fires once and every consumer,
//! including a fresh mount of the same or a different page, reads the same
//! cached `Resource`. The alternative of fetching eagerly at startup was
//! rejected (MAPPS-860): some consumers (the team-management modals) are
//! admin-only, so an eager fetch would run for sessions that never need it.
//!
//! Re-fetches once when [`crate::hooks::fetch::active_tenant_generation`]
//! changes (org switch / token swap), matching the per-page resources this
//! replaces.

use dioxus::prelude::*;

/// A user row as returned by `GET /auth/users`, shaped to cover every field
/// the 14 former call sites read out of their own local struct.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct UserRow {
    pub id: uuid::Uuid,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub email: String,
}

impl UserRow {
    /// `full_name`, else `first_name last_name` trimmed, else `email`, else
    /// `None` so each caller can supply its own final fallback string (the
    /// former per-site structs disagreed on it: "Unknown", "Unknown user",
    /// a short id, ...).
    pub fn display_name(&self) -> Option<String> {
        if !self.full_name.trim().is_empty() {
            return Some(self.full_name.clone());
        }
        let joined = format!("{} {}", self.first_name, self.last_name);
        let joined = joined.trim();
        if !joined.is_empty() {
            return Some(joined.to_string());
        }
        if !self.email.trim().is_empty() {
            return Some(self.email.clone());
        }
        None
    }
}

/// Shared "has any consumer asked for the roster yet" flag. Provided at the
/// App root alongside the resource itself; a page flips it via
/// [`use_user_roster`], never directly.
type RosterWanted = Signal<bool>;

/// Provide the shared roster resource and its `enabled` flag at the App
/// root. Mirrors [`crate::hooks::version_cache::use_version_cache_provider`].
pub fn use_user_roster_provider() {
    let wanted = use_signal(|| false);
    use_context_provider::<RosterWanted>(|| wanted);

    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return Vec::new();
        }
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::api::get_all_authed::<UserRow>("/auth/users")
                .await
                .inspect_err(|e| tracing::warn!("user roster load failed: {e}"))
                .unwrap_or_default()
        }
        #[cfg(not(feature = "app"))]
        {
            Vec::new()
        }
    });
    use_context_provider::<Resource<Vec<UserRow>>>(|| resource);
}

/// Ask for the cached roster. `enabled` is each caller's own gate (an admin
/// check, a role check, or unconditionally `true`), unchanged from what the
/// former per-site `use_resource` closures checked before fetching. Passing
/// `true` from any single mount is enough to trigger (and thereafter share)
/// the one underlying fetch; passing `false` never blocks a roster another
/// consumer already cached.
pub fn use_user_roster(enabled: bool) -> Resource<Vec<UserRow>> {
    let mut wanted = use_context::<RosterWanted>();
    if enabled && !*wanted.read() {
        wanted.set(true);
    }
    use_context::<Resource<Vec<UserRow>>>()
}

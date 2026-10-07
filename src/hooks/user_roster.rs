//! Shared, session-scoped cache for the user roster, extending the
//! [`crate::hooks::version_cache`] pattern from MAPPS-203 to reference-list
//! data (MAPPS-860).
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
//!
//! ## MAPPS-1010: role-aware source
//!
//! `GET /auth/users` is `RequireManager` on mokosh-server, so a technician
//! asking for it got a 403, and every name and picker on a page that reads
//! the roster went empty for that role. The fetch closure below reads
//! [`crate::hooks::auth::AuthContext::can_manage`] before choosing an
//! endpoint: a caller at or above the manager floor still reads
//! `GET /auth/users`; anyone else reads `GET /auth/directory` (PMS-921,
//! `RequireAuth`, active users only) and the response is mapped onto
//! [`UserRow`] with only `id` and `full_name` populated. The read happens
//! inside the resource's own async closure, before its first `await`, so
//! the resource subscribes to the auth signal the same way it already
//! subscribes to `active_tenant_generation`: a role change mid-session (a
//! demotion picked up by `/auth/me`) re-fetches from the right source
//! rather than keeping whichever list the session started with.

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

/// Provide the shared roster at the App root (MAPPS-1001: see
/// [`crate::hooks::shared_list`]).
///
/// MAPPS-1010: the source endpoint is chosen per fetch from the caller's own
/// role, not fixed to `/auth/users`, so a technician's fetch succeeds against
/// `/auth/directory` instead of 403ing. See the module doc.
pub fn use_user_roster_provider() {
    let auth = crate::hooks::auth::use_auth();
    crate::hooks::shared_list::use_list_provider_with::<UserRow, _, _>(move || {
        let can_manage = auth.read().can_manage();
        async move { fetch_roster(can_manage).await }
    });
}

/// Fetch the roster from whichever endpoint `can_manage` permits. See the
/// module doc for why the choice lives here rather than in a fixed endpoint.
#[cfg(feature = "app")]
async fn fetch_roster(can_manage: bool) -> Vec<UserRow> {
    if can_manage {
        crate::hooks::shared_list::fetch_list::<UserRow>("users", "/auth/users").await
    } else {
        fetch_directory_as_rows().await
    }
}

#[cfg(not(feature = "app"))]
async fn fetch_roster(_can_manage: bool) -> Vec<UserRow> {
    Vec::new()
}

/// `GET /auth/directory` (PMS-921, `RequireAuth`), mapped onto [`UserRow`].
/// The directory returns `id`, `name` and `handle` for active users only;
/// only `id` and `full_name` are populated here, so [`UserRow::display_name`]
/// falls through to the caller's own "Unknown" for a deactivated user the
/// directory omits, matching the unknown-id fallback that already covers that
/// case today.
#[cfg(feature = "app")]
async fn fetch_directory_as_rows() -> Vec<UserRow> {
    #[derive(serde::Deserialize)]
    struct DirectoryEntry {
        id: uuid::Uuid,
        #[serde(default)]
        name: String,
    }
    match crate::hooks::fetch::api::get_all_authed_typed::<DirectoryEntry>("/auth/directory").await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|d| UserRow {
                id: d.id,
                full_name: d.name,
                ..Default::default()
            })
            .collect(),
        // Refused for this role is not expected here (the directory is
        // `RequireAuth`), but treated the same as the manager-roster 403: an
        // answer, not a fault.
        Err(e) if e.status_code() == Some(403) => {
            tracing::warn!("directory load refused for this role, its pickers stay empty: {e}");
            Vec::new()
        }
        Err(e) => {
            tracing::error!("directory load failed, its pickers are empty: {e}");
            crate::hooks::push_toast(
                crate::components::AlertType::Error,
                format!("Could not load users: {}", e.user_message()),
            );
            Vec::new()
        }
    }
}

/// Ask for the cached roster. `enabled` is each caller's own gate (an admin
/// check, a role check, or unconditionally `true`), unchanged from what the
/// former per-site `use_resource` closures checked before fetching. Passing
/// `true` from any single mount is enough to trigger (and thereafter share)
/// the one underlying fetch; passing `false` never blocks a roster another
/// consumer already cached.
pub fn use_user_roster(enabled: bool) -> Resource<Vec<UserRow>> {
    crate::hooks::shared_list::use_shared_list::<UserRow>(enabled)
}

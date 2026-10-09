#![cfg(feature = "multi-tenant")]
//! Redirect stub. Superseded by `/settings/members?tab=teams` under
//! MAPPS-877 phase 2; the stand-alone teams page is retired in favor of
//! the Teams tab on the unified members page.
//!
//! Kept as a stub so old bookmarks, emails, and PR links keep resolving
//! rather than 404-ing on the day phase 2 ships. Phase 4 rebuilds the
//! Teams tab UI on `members.rs` using the module's git history as the
//! reference for the rows, modal, and form shapes the old page carried.
//! The branch tip immediately before phase 2 is the one to read.

use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn TeamsPage() -> Element {
    let auth = crate::hooks::use_auth();
    // Same gate the live Members page runs so a non-admin cannot read
    // the redirect's "going to Members / Teams" hint via this legacy
    // URL. Matches the admin-route role-gate contract (`is_admin`).
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false);
    if !is_admin {
        return rsx! {
            div { class: "max-w-7xl mx-auto p-6 text-sm text-muted",
                "You do not have access to this page."
            }
        };
    }
    let navigator = use_navigator();
    use_effect(move || {
        navigator.replace(Route::MembersPage {
            tab: "teams".to_string(),
        });
    });
    rsx! {
        div { class: "max-w-7xl mx-auto p-6 text-sm text-muted",
            "Redirecting to Members / Teams…"
        }
    }
}

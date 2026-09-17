//! MAPPS-877: redirect stub.
//!
//! Superseded by `/settings/members?tab=teams`. This file exists so a
//! bookmark, an inbound email link, or a PR reference to
//! `/admin/teams` still resolves. Feature code moved to
//! `src/pages/members.rs`; do not restore roster / picker logic here.
//!
//! The redirect fires from a `use_effect` on mount. The `is_admin`
//! gate stays in place so a non-admin who reaches this URL directly
//! sees `ContentUnavailable` instead of a query-string leak in the
//! address bar.

#![cfg(feature = "multi-tenant")]

use dioxus::prelude::*;

use crate::components::ContentUnavailable;
use crate::Route;

#[component]
pub fn TeamsPage() -> Element {
    let auth = crate::hooks::use_auth();
    let nav = use_navigator();
    let is_admin = auth.read().is_admin();

    use_effect(move || {
        if is_admin {
            nav.replace(Route::MembersPage {
                tab: "teams".to_string(),
            });
        }
    });

    if !is_admin {
        return rsx! {
            ContentUnavailable { title: "Teams".to_string() }
        };
    }
    // A frame of nothing before the redirect lands.
    rsx! {}
}

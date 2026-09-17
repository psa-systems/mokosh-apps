//! MAPPS-877: redirect stub.
//!
//! Superseded by `/settings/members?tab=people` (and the Invitations
//! tab there for pending grant invitations). This file exists so a
//! bookmark, an inbound email link, or a PR reference to
//! `/settings/sharing` still resolves. Feature code moved to
//! `src/pages/members.rs`; do not restore outbox / confirm-modal
//! logic here.
//!
//! The redirect fires from a `use_effect` on mount. The `is_admin`
//! gate stays in place so a non-admin who reaches this URL directly
//! sees `ContentUnavailable` before the redirect fires.

use dioxus::prelude::*;

use crate::components::ContentUnavailable;
use crate::Route;

#[component]
pub fn SettingsSharingPage() -> Element {
    let auth = crate::hooks::use_auth();
    let nav = use_navigator();
    let is_admin = auth.read().is_admin();

    use_effect(move || {
        if is_admin {
            nav.replace(Route::MembersPage {
                tab: "people".to_string(),
            });
        }
    });

    if !is_admin {
        return rsx! {
            ContentUnavailable {
                title: "Manage sharing".to_string(),
                show_dashboard_link: true,
            }
        };
    }
    rsx! {}
}

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
    let navigator = use_navigator();
    use_effect(move || {
        navigator.replace(Route::MembersPage {
            tab: "teams".to_string(),
        });
    });
    // Rendered for one frame before the navigator lands; a visible line
    // means a caller sees "going to Members" instead of a blank screen on
    // a slow network.
    rsx! {
        div { class: "max-w-7xl mx-auto min-h-screen flex items-center justify-center text-sm text-muted",
            "Redirecting to Members / Teams…"
        }
    }
}

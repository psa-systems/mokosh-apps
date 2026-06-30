//! Full-width "server unreachable" banner (MAPPS-333).
//!
//! Renders nothing while mokosh-server is reachable (the default), so it
//! is zero-height and absent on every healthy page. When the central
//! request layer marks the server down (transport failure, timeout, or
//! `5xx`; see [`crate::hooks::server_status`]) it shows a single
//! non-blocking bar above the rest of the layout. Pages stay visible and
//! degrade gracefully; existing per-page toasts still fire. The
//! recovery poll clears the down state on its own, so the banner
//! auto-dismisses with no user action.
//!
//! Unlike the admin [`crate::components::UpdateBanner`], this does not use
//! the reserve-then-collapse height trick: the healthy default is
//! "absent", there is no first-paint uncertainty to hide, and an outage
//! alert appearing is expected to take up space. Rendering nothing while
//! healthy also keeps the "Click to see more" link out of the tab order
//! until it is actually actionable.

use dioxus::prelude::*;

use crate::hooks::use_server_reachable;
use crate::Route;

/// App-wide banner shown while mokosh-server is unreachable.
#[component]
pub fn ServerStatusBanner() -> Element {
    // Reading the flag subscribes this component, so it re-renders (and the
    // banner appears / disappears) on every reachability transition.
    if use_server_reachable() {
        return rsx! {};
    }

    rsx! {
        div {
            // `role=status` + `aria-live=polite` so assistive tech
            // announces the outage when the bar appears, without
            // interrupting the user mid-action.
            role: "status",
            aria_live: "polite",
            class: "border-b border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 text-red-900 dark:text-red-200",
            div {
                class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-2 flex items-center justify-center gap-2 text-sm text-center",
                // User-facing copy only: never the raw CORS / "Failed to
                // fetch" internals the console may show (MAPPS-333).
                span { class: "font-medium", "Server unreachable. Reconnecting\u{2026}" }
                // Optional detail affordance: bold + underlined link to the
                // System Status page, where the live /version + /ready
                // breakdown shows which dependency is down.
                Link {
                    to: Route::SystemStatus {},
                    class: "font-bold underline hover:no-underline",
                    "Click to see more\u{2026}"
                }
            }
        }
    }
}

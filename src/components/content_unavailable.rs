//! MAPPS-351: full-page "server unreachable" content state.
//!
//! The sibling of MAPPS-333's [`crate::components::ServerStatusBanner`]:
//! the banner is the app-wide, non-blocking outage notice that sits above
//! every page; this is the *page body* a data-driven page swaps in when
//! its primary resource could not load because mokosh-server is
//! unreachable (see [`crate::hooks::RemoteData::is_unavailable`]).
//!
//! Without it a page whose fetches failed rendered empty tables and zero
//! counts - indistinguishable from a brand-new tenant with no data. This
//! renders an explicit, honest state instead, and because it renders inside
//! the persistent [`AppShell`], the sidebar nav, top bar, and outage banner
//! all stay visible: nothing "vanishes".
//!
//! It is deliberately non-blocking and self-healing: the copy tells the
//! user it will recover on its own (the MAPPS-333 recovery poll flips the
//! reachability flag back, which re-runs the page's `use_remote_resource`
//! and replaces this with the real content), so there is no retry button
//! to babysit. The optional "Go to dashboard" link is the issue's
//! keep-the-user-somewhere-coherent affordance; we never auto-redirect.
//!
//! MAPPS-366: this renders as a page body inside the persistent
//! [`crate::components::AppShell`], so the sidebar nav, top bar, and outage
//! banner stay visible around it; it sets its own tab title via
//! [`crate::components::use_page_title`]. Mirrors the
//! [`crate::components::PermissionRequired`] empty-state idiom (PageHeader +
//! centered Card) so the two read as one family.

use dioxus::prelude::*;

use crate::components::{use_page_title, Card, IconSize, PageHeader, ServerIcon};
use crate::Route;

#[derive(Props, Clone, PartialEq)]
pub struct ContentUnavailableProps {
    /// Page title, used for both the tab title and the `PageHeader`
    /// heading so the chrome stays consistent with the healthy page.
    pub title: String,
    /// Show the "Go to dashboard" affordance. Defaults to `true`; the
    /// dashboard itself passes `false` (linking to itself is pointless).
    #[props(default = true)]
    pub show_dashboard_link: bool,
}

#[component]
pub fn ContentUnavailable(props: ContentUnavailableProps) -> Element {
    // MAPPS-366: this is the whole body of a page's unavailable branch, so it
    // owns the tab title that the page's healthy branch would otherwise set.
    use_page_title(props.title.clone());
    rsx! {
        PageHeader { title: "{props.title}" }
        Card {
            // `role=status` + `aria-live=polite` so assistive tech
            // announces the outage when the body swaps in, matching
            // the MAPPS-333 banner's posture.
            div {
                role: "status",
                aria_live: "polite",
                class: "py-12 px-6 mx-auto flex max-w-md flex-col items-center text-center",
                div { class: "mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-surface-2",
                    ServerIcon { size: IconSize::Large, class: "text-subtle".to_string() }
                }
                h3 { class: "text-base font-medium text-content",
                    "Can't load this page"
                }
                p { class: "mt-2 text-sm text-muted",
                    // User-facing copy only: never the raw CORS /
                    // "Failed to fetch" internals (MAPPS-333).
                    "The server is unreachable. This page will refresh on its own once the connection is back."
                }
                if props.show_dashboard_link {
                    Link {
                        to: Route::Dashboard {},
                        class: "mt-6 inline-flex items-center rounded-md bg-surface-2 px-3 py-2 text-sm font-medium text-content hover:opacity-90",
                        "Go to dashboard"
                    }
                }
            }
        }
    }
}

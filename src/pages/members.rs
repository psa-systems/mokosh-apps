//! MAPPS-877: unified access-management page.
//!
//! `/settings/members?tab=<people|teams|invitations>`. Three tabs that
//! answer the same question ("who has access to this workspace, and
//! how") that the retired `/admin/teams` and `/settings/sharing`
//! pages split across two mental models. Each pane fetches its own
//! resources; opening a tab that has not been visited yet triggers
//! its first fetch, so a deep link to `?tab=teams` does not over-fetch
//! People or Invitations.
//!
//! Phase 2 scaffold: the tab shell + gate. Panes are placeholders
//! that phases 3-5 will replace with the real content:
//!
//! - People pane (phase 3): the unified list against
//!   `GET /api/v1/members`. Native users + placed guests + unplaced
//!   guests, one row per person, row action dispatch by kind.
//! - Teams pane (phase 4): the today-`/admin/teams` roster + user
//!   picker replacing the raw-UUID paste.
//! - Invitations pane (phase 5): today's `/settings/sharing` pending
//!   invitations list, Cancel per row.
//!
//! Gate: `is_org_tenant && is_admin` for now (mirrors the two pages
//! this supersedes). Phase 3 relaxes the outer gate to `is_manager`
//! and renders actions only for admins.

#![cfg(feature = "multi-tenant")]

use dioxus::prelude::*;

use crate::components::{use_page_title, ContentUnavailable, PageHeader};
use crate::Route;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    People,
    Teams,
    Invitations,
}

impl Tab {
    pub(crate) fn from_query(q: &str) -> Self {
        match q {
            "teams" => Tab::Teams,
            "invitations" => Tab::Invitations,
            _ => Tab::People,
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Tab::People => "people",
            Tab::Teams => "teams",
            Tab::Invitations => "invitations",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::People => "People",
            Tab::Teams => "Teams",
            Tab::Invitations => "Invitations",
        }
    }
}

#[component]
pub fn MembersPage(tab: String) -> Element {
    use_page_title("Members");
    let auth = crate::hooks::use_auth();
    let nav = use_navigator();

    // MAPPS-602: hooks fire BEFORE the early-return gates so the
    // component keeps a stable hook count across renders.
    let is_admin = auth.read().is_admin();
    let is_org_tenant = auth.read().is_org_tenant();
    let active = Tab::from_query(&tab);

    // Personal tenant: bounce with the shared unavailable splash.
    // Matches TeamsPage's pre-MAPPS-877 gate.
    if !is_org_tenant {
        return rsx! {
            ContentUnavailable { title: "Members".to_string() }
        };
    }

    // Phase 2 gate: admin only. Phase 3 relaxes to manager (read-only)
    // and gates the mutation affordances on `is_admin` inside the
    // pane instead.
    if !is_admin {
        return rsx! {
            ContentUnavailable {
                title: "Members".to_string(),
                show_dashboard_link: true,
            }
        };
    }

    rsx! {
        PageHeader {
            title: "Members",
            subtitle: "People, teams, and pending invitations for this workspace.",
        }

        div { class: "border-b border-line mb-4",
            nav { class: "flex gap-1",
                for t in [Tab::People, Tab::Teams, Tab::Invitations] {
                    button {
                        r#type: "button",
                        key: "{t.slug()}",
                        class: if active == t {
                            "px-4 py-2 text-sm font-medium border-b-2 border-accent text-content"
                        } else {
                            "px-4 py-2 text-sm text-subtle hover:text-content border-b-2 border-transparent"
                        },
                        aria_current: if active == t { "page" } else { "false" },
                        onclick: {
                            let slug = t.slug().to_string();
                            move |_| {
                                nav.replace(Route::MembersPage { tab: slug.clone() });
                            }
                        },
                        "{t.label()}"
                    }
                }
            }
        }

        match active {
            Tab::People => rsx! { PeoplePlaceholder {} },
            Tab::Teams => rsx! { TeamsPlaceholder {} },
            Tab::Invitations => rsx! { InvitationsPlaceholder {} },
        }
    }
}

#[component]
fn PeoplePlaceholder() -> Element {
    rsx! {
        div { class: "p-6 text-sm text-subtle",
            p { class: "font-medium text-content mb-1", "People pane" }
            p {
                "The unified members list lands here in the next phase. Until then, native users and guests are still manageable from the retired routes (which redirect here)."
            }
        }
    }
}

#[component]
fn TeamsPlaceholder() -> Element {
    rsx! {
        div { class: "p-6 text-sm text-subtle",
            p { class: "font-medium text-content mb-1", "Teams pane" }
            p {
                "Team roster + member management moves into this pane in the next phase. The picker replacing the raw-UUID paste lands with it."
            }
        }
    }
}

#[component]
fn InvitationsPlaceholder() -> Element {
    rsx! {
        div { class: "p-6 text-sm text-subtle",
            p { class: "font-medium text-content mb-1", "Invitations pane" }
            p {
                "Pending grant invitations move here in the next phase. Cancel per row, same behaviour as the retired Sharing page."
            }
        }
    }
}

#![cfg(feature = "multi-tenant")]
//! MAPPS-877 phase 2: unified members page shell.
//!
//! One route (`/settings/members?tab=...`), three tabs (People, Teams,
//! Invitations). Each pane is a stub in this phase; the actual content
//! lands per phase: phase 3 (People), phase 4 (Teams), phase 5
//! (Invitations). The nav and switcher labels change in phase 6.
//!
//! `tab` is a query parameter so a deep-link preserves which pane the
//! operator opened and a reload stays on it (T51). Unknown values fall
//! back to People, which is the default landing pane the design settles
//! on.

use dioxus::prelude::*;

use crate::components::{use_page_title, ContentUnavailable, PageHeader};
use crate::Route;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    People,
    Teams,
    Invitations,
}

impl Tab {
    fn from_query(q: &str) -> Self {
        match q {
            "teams" => Tab::Teams,
            "invitations" => Tab::Invitations,
            // Everything else, including an empty query string and
            // `people`, lands on People so a bookmark that drops the
            // query string still opens the default pane.
            _ => Tab::People,
        }
    }

    fn slug(self) -> &'static str {
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

#[derive(Props, Clone, PartialEq)]
pub struct MembersPageProps {
    pub tab: String,
}

#[component]
pub fn MembersPage(props: MembersPageProps) -> Element {
    use_page_title("Members");
    let auth = crate::hooks::use_auth();
    let is_admin = auth.read().is_admin();
    let is_org_tenant = auth.read().is_org_tenant();
    let navigator = use_navigator();
    let active = Tab::from_query(&props.tab);

    // Members (team roster + cross-account sharing) is an org-tenant
    // concept; a personal tenant has one user and nothing to list, so
    // the page refuses rather than renders an empty shell.
    if !is_org_tenant {
        return rsx! {
            ContentUnavailable { title: "Members".to_string() }
        };
    }
    // Phase 3 relaxes this to the manager role and renders read-only;
    // for the scaffold the controls are admin-only to match the old
    // `/admin/teams` posture the redirect lands on.
    if !is_admin {
        return rsx! {
            ContentUnavailable { title: "Members".to_string() }
        };
    }

    rsx! {
        PageHeader { title: "Members".to_string() }
        div { class: "flex gap-2 border-b border-line mb-4",
            for t in [Tab::People, Tab::Teams, Tab::Invitations] {
                {
                    let is_active = active == t;
                    let class = if is_active {
                        "px-3 py-2 text-sm font-medium border-b-2 border-accent text-content"
                    } else {
                        "px-3 py-2 text-sm text-muted hover:text-content"
                    };
                    let slug = t.slug().to_string();
                    let label = t.label();
                    rsx! {
                        button {
                            key: "{slug}",
                            r#type: "button",
                            class,
                            onclick: move |_| {
                                navigator.replace(Route::MembersPage { tab: slug.clone() });
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
        match active {
            Tab::People => rsx! {
                p { class: "text-sm text-muted",
                    "People pane coming in a later phase."
                }
            },
            Tab::Teams => rsx! {
                p { class: "text-sm text-muted",
                    "Teams pane coming in a later phase."
                }
            },
            Tab::Invitations => rsx! {
                p { class: "text-sm text-muted",
                    "Invitations pane coming in a later phase."
                }
            },
        }
    }
}

#![cfg(feature = "multi-tenant")]
//! MAPPS-877 unified members page: shell (phase 2) + People tab (phase 3).
//!
//! One route (`/settings/members?tab=...`), three tabs (People, Teams,
//! Invitations). The Teams and Invitations panes are stubs here; phases 4
//! and 5 own those. The nav / switcher labels change in phase 6.
//!
//! `tab` is a query parameter so a deep-link preserves which pane the
//! operator opened and a reload stays on it (T51). Unknown values fall
//! back to People, which is the default landing pane the design settles
//! on.
//!
//! The page gate relaxes from "admin only" to "manager and up" (phase 3
//! T28): a non-admin manager still reads the roster, but every row-level
//! destructive action is hidden. The admin keeps the actions.
//!
//! Row-action dispatch (phase 3 T21..T25): the same user-facing "change
//! role" or "remove" can land on either `/users` or `/grants` depending
//! on the row's kind. The service `change_role_path` /
//! `remove_action_path` below are the one place that mapping lives, and
//! the module-local tests pin each URL by row shape so a later edit
//! cannot silently cross the wires.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, BannerTone, ContentUnavailable, PageHeader, StatusBanner,
};
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

// ---------------------------------------------------------------------------
// Wire types mirrored from mokosh-server `crates/mokosh-types/src/members.rs`.
// Deliberately not pulled from `mokosh_types::members::*`: this file is the
// SPA's own read shape and gives it `#[serde(default)]` where the server's
// `skip_serializing_if = None` makes a key optional on the wire. If the
// server renames a field, the mismatch surfaces here at the first fetch
// rather than through a mokosh-types bump that could silently change the
// SPA's deserialize behaviour.

/// One row of the unified list.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemberRow {
    User {
        user_id: uuid::Uuid,
        #[serde(default)]
        email: String,
        #[serde(default)]
        first_name: String,
        #[serde(default)]
        last_name: String,
        #[serde(default)]
        role: String,
        #[serde(default)]
        status: String,
        #[serde(default)]
        team_memberships: Vec<TeamChip>,
        /// `Some(_)` when this `users` row was placed by a cross-account
        /// grant (BUNYIP-674). The SPA renders "Guest" beside the name
        /// and the destructive actions revoke the grant rather than
        /// deactivate the user.
        #[serde(default)]
        placed_by_grant_id: Option<String>,
    },
    UnplacedGuest {
        grant_id: String,
        #[serde(default)]
        grantee_email: Option<String>,
        #[serde(default)]
        grantee_name: Option<String>,
        #[serde(default)]
        role: String,
    },
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TeamChip {
    pub team_id: uuid::Uuid,
    #[serde(default)]
    pub team_name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MembersResponse {
    #[serde(default)]
    pub rows: Vec<MemberRow>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub bunyip_reachable: bool,
}

// ---------------------------------------------------------------------------
// Row-action dispatch. Pure functions over `MemberRow`; the test module
// below pins each URL so a future edit cannot silently route a guest's
// role change to `/users` or a native user's deactivate to `/grants`.

/// Pick the HTTP path that answers "change the role on this row". Native
/// users route through `/users`; guests (placed or unplaced) route through
/// `/grants` because the role lives on the grant, not on the users row.
pub fn change_role_path(row: &MemberRow) -> String {
    match row {
        MemberRow::User {
            user_id,
            placed_by_grant_id: None,
            ..
        } => format!("/users/{user_id}"),
        MemberRow::User {
            placed_by_grant_id: Some(grant_id),
            ..
        } => format!("/grants/{grant_id}"),
        MemberRow::UnplacedGuest { grant_id, .. } => format!("/grants/{grant_id}"),
    }
}

/// Pick the HTTP path that answers "remove this row". A native user is
/// deactivated via `/users?status=inactive`; a guest (placed or unplaced)
/// has its grant revoked.
pub fn remove_action_path(row: &MemberRow) -> String {
    match row {
        MemberRow::User {
            user_id,
            placed_by_grant_id: None,
            ..
        } => format!("/users/{user_id}"),
        MemberRow::User {
            placed_by_grant_id: Some(grant_id),
            ..
        } => format!("/grants/{grant_id}"),
        MemberRow::UnplacedGuest { grant_id, .. } => format!("/grants/{grant_id}"),
    }
}

fn row_kind_badge(row: &MemberRow) -> (BadgeVariant, &'static str) {
    match row {
        MemberRow::User {
            placed_by_grant_id: None,
            ..
        } => (BadgeVariant::Blue, "Direct"),
        MemberRow::User {
            placed_by_grant_id: Some(_),
            ..
        } => (BadgeVariant::Purple, "Guest"),
        MemberRow::UnplacedGuest { .. } => (BadgeVariant::Gray, "Awaiting first sign-in"),
    }
}

fn row_display_name(row: &MemberRow) -> String {
    match row {
        MemberRow::User {
            first_name,
            last_name,
            email,
            ..
        } => {
            let full = format!("{} {}", first_name, last_name).trim().to_string();
            if full.is_empty() {
                email.clone()
            } else {
                full
            }
        }
        MemberRow::UnplacedGuest {
            grantee_name,
            grantee_email,
            grant_id,
            ..
        } => grantee_name
            .clone()
            .or_else(|| grantee_email.clone())
            .unwrap_or_else(|| format!("Invitee {}", &grant_id[..grant_id.len().min(8)])),
    }
}

fn row_email(row: &MemberRow) -> String {
    match row {
        MemberRow::User { email, .. } => email.clone(),
        MemberRow::UnplacedGuest { grantee_email, .. } => grantee_email.clone().unwrap_or_default(),
    }
}

fn row_role(row: &MemberRow) -> &str {
    match row {
        MemberRow::User { role, .. } => role,
        MemberRow::UnplacedGuest { role, .. } => role,
    }
}

fn row_teams(row: &MemberRow) -> &[TeamChip] {
    match row {
        MemberRow::User {
            team_memberships, ..
        } => team_memberships,
        MemberRow::UnplacedGuest { .. } => &[],
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
    // Phase 3 T28: the page gate is "manager and up" so a non-admin
    // manager still reads the roster; the row-level actions below render
    // only when the stricter admin gate also holds.
    let can_manage = auth
        .read()
        .user
        .as_ref()
        .is_some_and(|u| u.role.can_manage_users());
    let is_admin = auth.read().is_admin();
    let is_org_tenant = auth.read().is_org_tenant();
    let navigator = use_navigator();
    let active = Tab::from_query(&props.tab);

    // Members is an org-tenant concept; a personal tenant has one user
    // and nothing to list, so the page refuses rather than renders an
    // empty shell.
    if !is_org_tenant {
        return rsx! {
            ContentUnavailable { title: "Members".to_string() }
        };
    }
    if !can_manage {
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
            Tab::People => rsx! { PeoplePane { can_mutate: is_admin } },
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

#[component]
fn PeoplePane(can_mutate: bool) -> Element {
    let members = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<MembersResponse>("/members?per_page=100")
            .await
            .inspect_err(|e| tracing::error!("members list load failed: {e}"))
            .ok()
    });
    let snap = members.read_unchecked();

    rsx! {
        match &*snap {
            None => rsx! {
                p { class: "text-sm text-muted", "Loading…" }
            },
            Some(None) => rsx! {
                p { class: "text-sm text-muted",
                    "Could not load the member list."
                }
            },
            Some(Some(payload)) => rsx! {
                PeopleBody { payload: payload.clone(), can_mutate }
            },
        }
    }
}

#[component]
fn PeopleBody(payload: MembersResponse, can_mutate: bool) -> Element {
    rsx! {
        if !payload.bunyip_reachable {
            StatusBanner { tone: BannerTone::Warning, class: "mb-3".to_string(),
                "Guest list unavailable: the identity service could not be reached. Native users are shown; guests may be missing."
            }
        }
        if payload.rows.is_empty() {
            p { class: "text-sm text-muted",
                "Nobody has access to this workspace yet."
            }
        } else {
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "text-left text-muted",
                        th { class: "py-2 pr-4", "Name" }
                        th { class: "py-2 pr-4", "Email" }
                        th { class: "py-2 pr-4", "Kind" }
                        th { class: "py-2 pr-4", "Role" }
                        th { class: "py-2 pr-4", "Teams" }
                        if can_mutate {
                            th { class: "py-2 pr-4", span { class: "sr-only", "Actions" } }
                        }
                    }
                }
                tbody {
                    for row in payload.rows.iter() {
                        {
                            let (variant, kind_label) = row_kind_badge(row);
                            let key = match row {
                                MemberRow::User { user_id, .. } => format!("u:{user_id}"),
                                MemberRow::UnplacedGuest { grant_id, .. } => {
                                    format!("g:{grant_id}")
                                }
                            };
                            let display_name = row_display_name(row);
                            let email = row_email(row);
                            let role = row_role(row).to_string();
                            let teams = row_teams(row).to_vec();
                            rsx! {
                                tr { key: "{key}", class: "border-t border-line",
                                    td { class: "py-2 pr-4 font-medium", "{display_name}" }
                                    td { class: "py-2 pr-4 text-muted", "{email}" }
                                    td { class: "py-2 pr-4",
                                        Badge { variant, "{kind_label}" }
                                    }
                                    td { class: "py-2 pr-4", "{role}" }
                                    td { class: "py-2 pr-4",
                                        {
                                            let shown: Vec<_> = teams.iter().take(3).cloned().collect();
                                            let overflow = teams.len().saturating_sub(shown.len());
                                            rsx! {
                                                div { class: "flex flex-wrap gap-1",
                                                    for t in shown.iter() {
                                                        {
                                                            let tk = t.team_id.to_string();
                                                            let name = t.team_name.clone();
                                                            rsx! {
                                                                Badge { key: "{tk}", variant: BadgeVariant::Gray, "{name}" }
                                                            }
                                                        }
                                                    }
                                                    if overflow > 0 {
                                                        span { class: "text-xs text-muted",
                                                            "+{overflow} more"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if can_mutate {
                                        td { class: "py-2 pr-4 text-muted text-xs",
                                            "Actions pending phase 3b"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! T21 / T22 / T23 / T24 / T25: pin the dispatch URL per row kind.
    //! These are deliberate re-assertions of the one place the server
    //! surface differs between native users and grantees. A refactor
    //! that routes a guest's role change through `/users` would
    //! silently drop a bunyip-side invariant; this is the test that
    //! catches it before CI runs.

    use super::{change_role_path, remove_action_path, MemberRow};
    use uuid::Uuid;

    fn native_user() -> MemberRow {
        MemberRow::User {
            user_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            email: "native@acme.example".to_string(),
            first_name: "Nat".to_string(),
            last_name: "Native".to_string(),
            role: "manager".to_string(),
            status: "active".to_string(),
            team_memberships: Vec::new(),
            placed_by_grant_id: None,
        }
    }

    fn placed_guest(grant_id: &str) -> MemberRow {
        MemberRow::User {
            user_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            email: "guest@partner.example".to_string(),
            first_name: "Grace".to_string(),
            last_name: "Guest".to_string(),
            role: "manager".to_string(),
            status: "active".to_string(),
            team_memberships: Vec::new(),
            placed_by_grant_id: Some(grant_id.to_string()),
        }
    }

    fn unplaced_guest(grant_id: &str) -> MemberRow {
        MemberRow::UnplacedGuest {
            grant_id: grant_id.to_string(),
            grantee_email: None,
            grantee_name: None,
            role: "finance".to_string(),
        }
    }

    #[test]
    fn t21_change_role_on_a_native_user_hits_users() {
        let row = native_user();
        assert_eq!(
            change_role_path(&row),
            "/users/11111111-1111-4111-8111-111111111111"
        );
    }

    #[test]
    fn t22_change_role_on_a_placed_guest_hits_grants() {
        let row = placed_guest("grant-abc");
        assert_eq!(change_role_path(&row), "/grants/grant-abc");
    }

    #[test]
    fn t23_change_role_on_an_unplaced_guest_hits_grants() {
        let row = unplaced_guest("grant-def");
        assert_eq!(change_role_path(&row), "/grants/grant-def");
    }

    #[test]
    fn t24_remove_a_native_user_hits_users() {
        let row = native_user();
        assert_eq!(
            remove_action_path(&row),
            "/users/11111111-1111-4111-8111-111111111111"
        );
    }

    #[test]
    fn t25_remove_a_placed_guest_hits_grants() {
        let row = placed_guest("grant-xyz");
        assert_eq!(remove_action_path(&row), "/grants/grant-xyz");
    }

    #[test]
    fn remove_an_unplaced_guest_hits_grants() {
        let row = unplaced_guest("grant-uvw");
        assert_eq!(remove_action_path(&row), "/grants/grant-uvw");
    }
}

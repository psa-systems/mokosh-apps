//! MAPPS-877: unified access-management page.
//!
//! `/settings/members?tab=<people|teams|invitations>`.
//!
//! Three panes that all answer the same question ("who has access to
//! this workspace, and how") the retired `/admin/teams` and
//! `/settings/sharing` pages split across two mental models.
//!
//! - **People** (`?tab=people`, default): unified list against
//!   `GET /api/v1/members`. Native `User` rows AND placed guests
//!   (a `User` row with `placed_by_grant_id`) AND `UnplacedGuest`
//!   rows (a grant with no matching users row yet). Row actions
//!   dispatch by kind: `PUT /users/{id}` for native, `PATCH
//!   /grants/{id}` for guest role, `DELETE /grants/{id}` for guest
//!   removal (revoke), `PUT /users/{id}` with `status: inactive`
//!   for native removal (deactivate).
//! - **Teams** (`?tab=teams`): the roster + create + edit + members
//!   surface the old `/admin/teams` page carried, with the raw-UUID
//!   paste replaced by `UserPicker` (a debounced typeahead over
//!   `GET /members?kind=user&q=<q>`) and the hardcoded `"-"` count
//!   fixed by reading the server-provided `Team.member_count`.
//! - **Invitations** (`?tab=invitations`): pending grant invitations
//!   (mokosh outbox). Cancel per row, no confirm (invitee has not
//!   accepted). Same optimistic overlay + refresh counter shape the
//!   retired Sharing page used.
//!
//! Gate: `is_org_tenant && is_admin`. Personal-tenant callers see
//! `ContentUnavailable`. Non-admins on an org tenant see the same
//! (phase 3 half of AC10; a manager-read-only rendering is the
//! deliberate follow-up).

#![cfg(feature = "multi-tenant")]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, AlertType, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog,
    ContentUnavailable, Input, Modal, ModalSize, PageHeader,
};
use crate::Route;

/// PMS-1162 role vocabulary in title case.
fn humanise_role(role: &str) -> String {
    match role {
        "super_admin" => "Super admin".to_string(),
        "admin" => "Admin".to_string(),
        "manager" => "Manager".to_string(),
        "technician" => "Technician".to_string(),
        "finance" => "Finance".to_string(),
        "read_only" => "Read only".to_string(),
        other => other.to_string(),
    }
}

/// Minimal query-string encoder: rewrites the small set of characters
/// that would break `%q=<value>&...` parsing on the server. Rich URLs
/// go through a real library elsewhere; this is a filter search box
/// on a single-tenant admin surface, so the ASCII shape is fine.
fn encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

fn role_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("admin", "Admin"),
        ("manager", "Manager"),
        ("technician", "Technician"),
        ("finance", "Finance"),
        ("read_only", "Read only"),
    ]
}

// -----------------------------------------------------------------------
// Wire shapes: mirror `mokosh_types::members`. Kept as SPA-local structs
// so `mokosh-apps` does not have to re-pin `mokosh-types` for one enum;
// the field names match verbatim so the wire is untouched.
// -----------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MemberRow {
    User {
        user_id: uuid::Uuid,
        email: String,
        first_name: String,
        last_name: String,
        role: String,
        status: String,
        #[serde(default)]
        last_login_at: Option<chrono::DateTime<chrono::Utc>>,
        #[serde(default)]
        team_memberships: Vec<TeamChip>,
        #[serde(default)]
        placed_by_grant_id: Option<uuid::Uuid>,
    },
    UnplacedGuest {
        grant_id: uuid::Uuid,
        #[serde(default)]
        grantee_email: Option<String>,
        #[serde(default)]
        grantee_name: Option<String>,
        role: String,
        #[serde(default)]
        granted_at: Option<chrono::DateTime<chrono::Utc>>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
struct TeamChip {
    team_id: uuid::Uuid,
    team_name: String,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct MembersResponse {
    rows: Vec<MemberRow>,
    total: u64,
    #[allow(dead_code)]
    page: u32,
    #[allow(dead_code)]
    per_page: u32,
    bunyip_reachable: bool,
}

// -----------------------------------------------------------------------
// Tabs
// -----------------------------------------------------------------------

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

    let is_admin = auth.read().is_admin();
    let is_org_tenant = auth.read().is_org_tenant();
    let active = Tab::from_query(&tab);

    if !is_org_tenant {
        return rsx! {
            ContentUnavailable { title: "Members".to_string() }
        };
    }
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
            Tab::People => rsx! { PeoplePane {} },
            Tab::Teams => rsx! { TeamsPane {} },
            Tab::Invitations => rsx! { InvitationsPane {} },
        }
    }
}

// -----------------------------------------------------------------------
// People pane
// -----------------------------------------------------------------------

/// The three axes a user can see:
///  - `Direct`  = native `users` row
///  - `Guest`   = `users` row placed by a grant (has `placed_by_grant_id`)
///  - `Awaiting`= grant with no `users` row yet (UnplacedGuest)
enum RowKind {
    Direct,
    Guest,
    Awaiting,
}

fn row_kind(row: &MemberRow) -> RowKind {
    match row {
        MemberRow::User {
            placed_by_grant_id: Some(_),
            ..
        } => RowKind::Guest,
        MemberRow::User { .. } => RowKind::Direct,
        MemberRow::UnplacedGuest { .. } => RowKind::Awaiting,
    }
}

/// A row's display name. `UnplacedGuest` falls back to email then to
/// "Guest" so no row ever renders a blank primary label.
fn display_name(row: &MemberRow) -> String {
    match row {
        MemberRow::User {
            first_name,
            last_name,
            email,
            ..
        } => {
            let full = format!("{} {}", first_name.trim(), last_name.trim())
                .trim()
                .to_string();
            if full.is_empty() {
                email.clone()
            } else {
                full
            }
        }
        MemberRow::UnplacedGuest {
            grantee_name,
            grantee_email,
            ..
        } => grantee_name
            .clone()
            .or_else(|| grantee_email.clone())
            .unwrap_or_else(|| "Guest".to_string()),
    }
}

fn row_email(row: &MemberRow) -> String {
    match row {
        MemberRow::User { email, .. } => email.clone(),
        MemberRow::UnplacedGuest { grantee_email, .. } => grantee_email.clone().unwrap_or_default(),
    }
}

fn row_role(row: &MemberRow) -> String {
    match row {
        MemberRow::User { role, .. } => role.clone(),
        MemberRow::UnplacedGuest { role, .. } => role.clone(),
    }
}

#[component]
fn PeoplePane() -> Element {
    let mut q = use_signal(String::new);
    let mut kind = use_signal(|| "everyone".to_string());
    let refresh = use_signal(|| 0u32);

    let members: Resource<Option<MembersResponse>> = use_resource(move || {
        let _bump = refresh.read();
        let q_val = q.read().trim().to_string();
        let kind_val = kind.read().clone();
        async move {
            #[cfg(feature = "app")]
            {
                let mut path = String::from("/members?per_page=100");
                if !q_val.is_empty() {
                    path.push_str(&format!("&q={}", encode_query(&q_val)));
                }
                if kind_val != "everyone" {
                    path.push_str(&format!("&kind={}", kind_val));
                }
                crate::hooks::fetch::api::get_authed::<MembersResponse>(&path)
                    .await
                    .inspect_err(|e| tracing::error!("members list load failed: {e}"))
                    .ok()
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (q_val, kind_val);
                None::<MembersResponse>
            }
        }
    });

    let snap = members.read_unchecked();
    let is_loading = snap.is_none();
    let data = match &*snap {
        Some(Some(r)) => r.clone(),
        _ => MembersResponse {
            rows: Vec::new(),
            total: 0,
            page: 1,
            per_page: 100,
            bunyip_reachable: true,
        },
    };
    let bunyip_reachable = data.bunyip_reachable;

    // Per-row action state.
    let confirm_change: Signal<Option<MemberRow>> = use_signal(|| None);
    let confirm_remove: Signal<Option<MemberRow>> = use_signal(|| None);
    let role_pick: Signal<String> = use_signal(String::new);
    let action_error: Signal<String> = use_signal(String::new);
    let action_saving: Signal<bool> = use_signal(|| false);

    rsx! {
        // SaaS-mode banner. Standalone mode never sets this.
        if !bunyip_reachable {
            Card {
                div { class: "flex items-center justify-between gap-4 p-4",
                    div {
                        p { class: "text-sm font-medium text-content", "Guest list unavailable" }
                        p { class: "text-xs text-subtle",
                            "We could not reach the identity service. Native users are shown; guests may be missing."
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: {
                            let mut refresh = refresh;
                            move |_| { *refresh.write() += 1; }
                        },
                        "Retry"
                    }
                }
            }
        }

        // Header controls: search + kind filter + Invite buttons.
        div { class: "flex flex-wrap items-end justify-between gap-3 mt-3 mb-3",
            div { class: "flex flex-wrap items-end gap-2",
                div { class: "w-64",
                    Input {
                        name: "member_search",
                        label: "Search",
                        r#type: "text".to_string(),
                        value: q(),
                        oninput: move |e: FormEvent| q.set(e.value()),
                    }
                }
                div { class: "flex flex-col",
                    label { class: "text-xs text-subtle mb-1", "Show" }
                    select {
                        class: "block rounded-md border border-line bg-surface-1 px-3 py-2 text-sm text-content focus:outline-none",
                        value: kind(),
                        onchange: move |e: FormEvent| kind.set(e.value()),
                        option { value: "everyone", "Everyone" }
                        option { value: "user", "Direct only" }
                        option { value: "guest", "Guests only" }
                    }
                }
            }
            div { class: "flex gap-2",
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| {
                        *crate::components::SHOW_INVITE_MEMBER.write() = true;
                    },
                    "Invite guest"
                }
            }
        }

        // Errors, then table.
        if !action_error.read().is_empty() {
            div { role: "alert", class: "text-sm text-red-600 dark:text-red-400 mb-3", "{action_error}" }
        }

        Card {
            if is_loading {
                div { class: "p-4 text-sm text-subtle", "Loading..." }
            } else if data.rows.is_empty() {
                div { class: "p-4 text-sm text-subtle",
                    "No members match the current filters."
                }
            } else {
                ul { class: "divide-y divide-line",
                    for row in data.rows.iter() {
                        MemberRowItem {
                            key: "{row_key(row)}",
                            row: row.clone(),
                            on_change_role: {
                                let mut confirm_change = confirm_change;
                                let mut role_pick = role_pick;
                                let row_clone = row.clone();
                                move |_| {
                                    *role_pick.write() = row_role(&row_clone);
                                    *confirm_change.write() = Some(row_clone.clone());
                                }
                            },
                            on_remove: {
                                let mut confirm_remove = confirm_remove;
                                let row_clone = row.clone();
                                move |_| {
                                    *confirm_remove.write() = Some(row_clone.clone());
                                }
                            },
                        }
                    }
                }
            }
        }

        p { class: "text-xs text-subtle mt-2", "{data.total} member{plural(data.total)}." }

        // Change-role modal.
        ChangeRoleModal {
            target: confirm_change,
            role_pick,
            action_error,
            action_saving,
            refresh,
        }

        // Remove / revoke modal.
        RemoveModal {
            target: confirm_remove,
            action_error,
            action_saving,
            refresh,
        }
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn row_key(row: &MemberRow) -> String {
    match row {
        MemberRow::User { user_id, .. } => format!("u-{user_id}"),
        MemberRow::UnplacedGuest { grant_id, .. } => format!("g-{grant_id}"),
    }
}

#[component]
fn MemberRowItem(
    row: MemberRow,
    on_change_role: EventHandler<()>,
    on_remove: EventHandler<()>,
) -> Element {
    let kind = row_kind(&row);
    let name = display_name(&row);
    let email = row_email(&row);
    let role = row_role(&row);
    let source_badge = match kind {
        RowKind::Direct => rsx! { Badge { variant: BadgeVariant::Gray, "Direct" } },
        RowKind::Guest => rsx! { Badge { variant: BadgeVariant::Green, "Guest" } },
        RowKind::Awaiting => rsx! { Badge { variant: BadgeVariant::Yellow, "Awaiting sign-in" } },
    };

    let teams: Vec<TeamChip> = match &row {
        MemberRow::User {
            team_memberships, ..
        } => team_memberships.clone(),
        _ => Vec::new(),
    };

    rsx! {
        li { class: "flex items-center justify-between gap-4 px-4 py-3",
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2 min-w-0",
                    span { class: "font-medium text-content truncate", "{name}" }
                    {source_badge}
                }
                div { class: "text-xs text-subtle truncate",
                    if !email.is_empty() { "{email}" } else { "" }
                    span { class: "ml-2", "Role: {humanise_role(&role)}" }
                }
                if !teams.is_empty() {
                    div { class: "flex flex-wrap gap-1 mt-1",
                        for team in teams.iter().take(3) {
                            span {
                                key: "{team.team_id}",
                                class: "inline-flex items-center gap-1 rounded-full bg-surface-2 px-2 py-0.5 text-[10px] text-content",
                                if let Some(c) = team.color.clone() {
                                    span {
                                        class: "inline-block w-2 h-2 rounded-full",
                                        style: "background-color: {c};",
                                    }
                                }
                                "{team.team_name}"
                            }
                        }
                        if teams.len() > 3 {
                            span { class: "text-[10px] text-subtle", "+{teams.len() - 3} more" }
                        }
                    }
                }
            }
            div { class: "flex gap-2",
                Button {
                    variant: ButtonVariant::Secondary,
                    r#type: "button".to_string(),
                    onclick: move |_| on_change_role.call(()),
                    "Change role"
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    r#type: "button".to_string(),
                    onclick: move |_| on_remove.call(()),
                    if matches!(kind, RowKind::Direct) { "Deactivate" } else { "Revoke" }
                }
            }
        }
    }
}

#[component]
fn ChangeRoleModal(
    target: Signal<Option<MemberRow>>,
    role_pick: Signal<String>,
    action_error: Signal<String>,
    action_saving: Signal<bool>,
    refresh: Signal<u32>,
) -> Element {
    let target_snap = target.read().clone();
    let is_open = target_snap.is_some();
    let display = target_snap.as_ref().map(display_name).unwrap_or_default();
    let current_role = target_snap.as_ref().map(row_role).unwrap_or_default();
    let pick = role_pick();
    let unchanged = pick.is_empty() || pick == current_role;
    let saving = action_saving();

    let submit = move |_| {
        let mut target = target;
        let mut role_pick = role_pick;
        let mut action_error = action_error;
        let mut action_saving = action_saving;
        let mut refresh = refresh;
        if action_saving() {
            return;
        }
        let Some(row) = target.read().clone() else {
            return;
        };
        let new_role = role_pick.read().clone();
        if new_role.is_empty() || new_role == row_role(&row) {
            *target.write() = None;
            return;
        }
        action_saving.set(true);
        action_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = match &row {
                    MemberRow::User {
                        user_id,
                        placed_by_grant_id: None,
                        ..
                    } => {
                        // Native user: PUT /users/{id} with { role }.
                        let path = format!("/users/{user_id}");
                        let body = serde_json::json!({ "role": new_role });
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                    MemberRow::User {
                        placed_by_grant_id: Some(grant_id),
                        ..
                    } => {
                        // Placed guest: PATCH /grants/{grant_id}.
                        let path = format!("/grants/{grant_id}");
                        let body = serde_json::json!({ "role": new_role });
                        crate::hooks::fetch::api::patch_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                    MemberRow::UnplacedGuest { grant_id, .. } => {
                        let path = format!("/grants/{grant_id}");
                        let body = serde_json::json!({ "role": new_role });
                        crate::hooks::fetch::api::patch_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Role updated.");
                        *target.write() = None;
                        *role_pick.write() = String::new();
                        *refresh.write() += 1;
                    }
                    Err(msg) => action_error.set(msg),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (&row, new_role);
            }
            action_saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: is_open,
            title: "Change role".to_string(),
            size: ModalSize::Small,
            onclose: move |_| {
                let mut target = target;
                let mut role_pick = role_pick;
                if !action_saving() {
                    *target.write() = None;
                    *role_pick.write() = String::new();
                }
            },
            div { class: "space-y-3",
                p { class: "text-sm text-content",
                    "Change the role for "
                    span { class: "font-medium", "{display}" }
                    "."
                }
                p { class: "text-xs text-subtle",
                    "Current role: {humanise_role(&current_role)}"
                }
                div {
                    label { class: "text-sm text-content mb-1 block", "New role" }
                    select {
                        class: "block w-full rounded-md border border-line bg-surface-1 px-3 py-2 text-sm text-content focus:outline-none",
                        value: pick.clone(),
                        disabled: saving,
                        onchange: move |e: FormEvent| {
                            let mut role_pick = role_pick;
                            role_pick.set(e.value());
                        },
                        for (v, label) in role_options() {
                            option { key: "{v}", value: "{v}", "{label}" }
                        }
                    }
                }
                p { class: "text-xs text-subtle",
                    "Their access changes immediately. They stay signed in and see the new permissions on their next request."
                }
                div { class: "flex gap-2 justify-end pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        r#type: "button".to_string(),
                        disabled: saving,
                        onclick: move |_| {
                            let mut target = target;
                            let mut role_pick = role_pick;
                            *target.write() = None;
                            *role_pick.write() = String::new();
                        },
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        r#type: "button".to_string(),
                        disabled: saving || unchanged,
                        loading: saving,
                        onclick: submit,
                        "Save"
                    }
                }
            }
        }
    }
}

#[component]
fn RemoveModal(
    target: Signal<Option<MemberRow>>,
    action_error: Signal<String>,
    action_saving: Signal<bool>,
    refresh: Signal<u32>,
) -> Element {
    let target_snap = target.read().clone();
    let is_open = target_snap.is_some();
    let display = target_snap.as_ref().map(display_name).unwrap_or_default();
    let kind = target_snap.as_ref().map(row_kind);
    let title = match kind {
        Some(RowKind::Direct) => "Deactivate user",
        Some(_) => "Revoke access",
        None => "Remove",
    };
    let message = match kind {
        Some(RowKind::Direct) => format!(
            "Deactivate {display}? Their sign-in stops working immediately. You can reactivate them later."
        ),
        Some(_) => format!(
            "Revoke access for {display}? They lose access to this workspace immediately. You can send a fresh invitation later if you change your mind."
        ),
        None => "Remove this member?".to_string(),
    };
    let confirm_text = match kind {
        Some(RowKind::Direct) => "Deactivate",
        _ => "Revoke access",
    };

    let confirm = move |_: ()| {
        let mut target = target;
        let mut action_error = action_error;
        let mut action_saving = action_saving;
        let mut refresh = refresh;
        if action_saving() {
            return;
        }
        let Some(row) = target.read().clone() else {
            return;
        };
        action_saving.set(true);
        action_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = match &row {
                    MemberRow::User {
                        user_id,
                        placed_by_grant_id: None,
                        ..
                    } => {
                        let path = format!("/users/{user_id}");
                        let body = serde_json::json!({ "status": "inactive" });
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                            .await
                            .map(|_| ())
                    }
                    MemberRow::User {
                        placed_by_grant_id: Some(grant_id),
                        ..
                    } => {
                        let path = format!("/grants/{grant_id}");
                        crate::hooks::fetch::api::delete_authed(&path).await
                    }
                    MemberRow::UnplacedGuest { grant_id, .. } => {
                        let path = format!("/grants/{grant_id}");
                        crate::hooks::fetch::api::delete_authed(&path).await
                    }
                };
                match result {
                    Ok(()) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Done.");
                        *target.write() = None;
                        *refresh.write() += 1;
                    }
                    Err(msg) => action_error.set(msg),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = &row;
            }
            action_saving.set(false);
        });
    };

    rsx! {
        ConfirmDialog {
            open: is_open,
            title: title.to_string(),
            message,
            confirm_text: confirm_text.to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            error: action_error(),
            loading: action_saving(),
            onconfirm: confirm,
            oncancel: move |_| {
                let mut target = target;
                let mut action_error = action_error;
                if !action_saving() {
                    *target.write() = None;
                    action_error.set(String::new());
                }
            },
        }
    }
}

// -----------------------------------------------------------------------
// Teams pane
// -----------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RemoteTeam {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
    is_active: bool,
    #[serde(default)]
    member_count: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RemoteTeamMember {
    user_id: uuid::Uuid,
    email: String,
    first_name: String,
    last_name: String,
    role: String,
}

#[derive(Serialize)]
struct CreateTeamBody {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

#[derive(Serialize)]
struct UpdateTeamBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_active: Option<bool>,
}

#[derive(Serialize)]
struct AddTeamMemberBody {
    user_id: uuid::Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[component]
fn TeamsPane() -> Element {
    let mut show_create = use_signal(|| false);
    let edit_target: Signal<Option<RemoteTeam>> = use_signal(|| None);
    let mut teams_resource = use_resource(|| async {
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::api::get_authed::<Vec<RemoteTeam>>("/teams")
                .await
                .inspect_err(|e| tracing::error!("team list load failed: {e}"))
                .ok()
        }
        #[cfg(not(feature = "app"))]
        {
            None::<Vec<RemoteTeam>>
        }
    });

    let snap = teams_resource.read_unchecked();
    let is_loading = snap.is_none();
    let mut teams: Vec<RemoteTeam> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    teams.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));

    rsx! {
        div { class: "flex items-center justify-between mb-3",
            p { class: "text-sm text-subtle",
                "Sub-groups of users inside your workspace. Members are people from the People tab."
            }
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| show_create.set(true),
                "Create team"
            }
        }

        Card {
            if is_loading {
                div { class: "p-4 text-sm text-subtle", "Loading..." }
            } else if teams.is_empty() {
                div { class: "p-4 text-sm text-subtle", "No teams yet." }
            } else {
                ul { class: "divide-y divide-line",
                    for team in teams.into_iter() {
                        TeamListRow {
                            key: "{team.id}",
                            team: team.clone(),
                            on_edit: {
                                let mut edit_target = edit_target;
                                let row = team.clone();
                                move |_| *edit_target.write() = Some(row.clone())
                            },
                        }
                    }
                }
            }
        }

        if show_create() {
            CreateTeamModal {
                onclose: move |_| show_create.set(false),
                onsaved: move |_| {
                    show_create.set(false);
                    teams_resource.restart();
                },
            }
        }
        if let Some(target) = edit_target() {
            EditTeamModal {
                team: target,
                onclose: {
                    let mut edit_target = edit_target;
                    move |_| *edit_target.write() = None
                },
                onsaved: {
                    let mut edit_target = edit_target;
                    move |_| {
                        *edit_target.write() = None;
                        teams_resource.restart();
                    }
                },
            }
        }
    }
}

#[component]
fn TeamListRow(team: RemoteTeam, on_edit: EventHandler<()>) -> Element {
    let color = team.color.clone().unwrap_or_else(|| "#6366F1".to_string());
    let count_label = match team.member_count {
        Some(n) => format!("{n} member{}", if n == 1 { "" } else { "s" }),
        None => "".to_string(),
    };
    let status_variant = if team.is_active {
        BadgeVariant::Green
    } else {
        BadgeVariant::Gray
    };
    let status_label = if team.is_active { "Active" } else { "Archived" };

    rsx! {
        li { class: "flex items-center justify-between gap-4 px-4 py-3",
            div { class: "flex items-center gap-3 min-w-0 flex-1",
                span {
                    class: "inline-block w-3 h-3 rounded-full shrink-0",
                    style: "background-color: {color};",
                }
                div { class: "min-w-0",
                    div { class: "font-medium text-content truncate", "{team.name}" }
                    div { class: "text-xs text-subtle",
                        if !count_label.is_empty() { "{count_label} · " } else { "" }
                        Badge { variant: status_variant, "{status_label}" }
                    }
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| on_edit.call(()),
                "Edit"
            }
        }
    }
}

#[component]
fn CreateTeamModal(onclose: EventHandler<()>, onsaved: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut color = use_signal(|| String::from("#6366F1"));
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let submit = move |_| {
        if saving() {
            return;
        }
        let n = name.read().trim().to_string();
        if n.is_empty() {
            error.set("Team name is required.".to_string());
            return;
        }
        let body = CreateTeamBody {
            name: n,
            description: {
                let d = description.read().trim().to_string();
                if d.is_empty() { None } else { Some(d) }
            },
            color: Some(color.read().clone()),
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct TeamId {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed_typed::<TeamId, _>("/teams", &body).await {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Team created.");
                        onsaved.call(());
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = &body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Create team".to_string(),
            onclose: move |_| { if !saving() { onclose.call(()); } },
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| { if !saving() { onclose.call(()); } },
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: saving(),
                    onclick: submit,
                    "Create"
                }
            },
            div { class: "space-y-3",
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                Input {
                    name: "team_name",
                    label: "Team name",
                    r#type: "text".to_string(),
                    value: name(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); name.set(e.value()); },
                }
                Input {
                    name: "team_description",
                    label: "Description (optional)",
                    r#type: "text".to_string(),
                    value: description(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
                Input {
                    name: "team_color",
                    label: "Color (hex, e.g. #6366F1)",
                    r#type: "text".to_string(),
                    value: color(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| color.set(e.value()),
                }
            }
        }
    }
}

#[component]
fn EditTeamModal(
    team: RemoteTeam,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let team_id = team.id;
    let mut name = use_signal(|| team.name.clone());
    let mut description = use_signal(|| team.description.clone().unwrap_or_default());
    let mut color = use_signal(|| team.color.clone().unwrap_or_else(|| "#6366F1".into()));
    let mut is_active = use_signal(|| team.is_active);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let submit = move |_| {
        if saving() {
            return;
        }
        let n = name.read().trim().to_string();
        if n.is_empty() {
            error.set("Team name is required.".to_string());
            return;
        }
        let body = UpdateTeamBody {
            name: Some(n),
            description: Some(description.read().trim().to_string()),
            color: Some(color.read().clone()),
            is_active: Some(is_active()),
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/teams/{team_id}");
                #[derive(serde::Deserialize)]
                struct TeamId {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::put_authed_typed::<TeamId, _>(&path, &body).await {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Team updated.");
                        onsaved.call(());
                    }
                    Err(e) => error.set(e.user_message()),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = &body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: format!("Edit team: {}", team.name),
            onclose: move |_| { if !saving() { onclose.call(()); } },
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| { if !saving() { onclose.call(()); } },
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: saving(),
                    onclick: submit,
                    "Save"
                }
            },
            div { class: "space-y-3",
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                Input {
                    name: "team_name",
                    label: "Team name",
                    r#type: "text".to_string(),
                    value: name(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); name.set(e.value()); },
                }
                Input {
                    name: "team_description",
                    label: "Description",
                    r#type: "text".to_string(),
                    value: description(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
                Input {
                    name: "team_color",
                    label: "Color (hex, e.g. #6366F1)",
                    r#type: "text".to_string(),
                    value: color(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| color.set(e.value()),
                }
                label { class: "flex items-center gap-2 text-sm text-content",
                    input {
                        r#type: "checkbox",
                        checked: is_active(),
                        disabled: saving(),
                        onchange: move |e: FormEvent| is_active.set(e.value() == "true"),
                    }
                    "Active (uncheck to archive)"
                }
            }
        }

        MembersSection { team_id, saving_parent: saving }
    }
}

#[component]
fn MembersSection(team_id: uuid::Uuid, saving_parent: Signal<bool>) -> Element {
    let mut roster = use_resource(move || async move {
        #[cfg(feature = "app")]
        {
            let path = format!("/teams/{team_id}/members");
            crate::hooks::fetch::api::get_authed::<Vec<RemoteTeamMember>>(&path)
                .await
                .ok()
        }
        #[cfg(not(feature = "app"))]
        {
            None::<Vec<RemoteTeamMember>>
        }
    });
    let snap = roster.read_unchecked();
    let members: Vec<RemoteTeamMember> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    // Picker state.
    let mut query = use_signal(String::new);
    let mut picker_results: Signal<Vec<PickedUser>> = use_signal(Vec::new);
    let mut picker_busy = use_signal(|| false);

    let mut do_search = move |q: String| {
        picker_busy.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!(
                    "/members?kind=user&q={}&per_page=10",
                    encode_query(&q)
                );
                match crate::hooks::fetch::api::get_authed::<MembersResponse>(&path).await {
                    Ok(resp) => {
                        let picks: Vec<PickedUser> = resp
                            .rows
                            .into_iter()
                            .filter_map(|r| match r {
                                MemberRow::User {
                                    user_id,
                                    email,
                                    first_name,
                                    last_name,
                                    ..
                                } => Some(PickedUser {
                                    user_id,
                                    email,
                                    display_name: format!("{} {}", first_name, last_name)
                                        .trim()
                                        .to_string(),
                                }),
                                _ => None,
                            })
                            .collect();
                        picker_results.set(picks);
                    }
                    Err(_) => picker_results.set(Vec::new()),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = q;
            }
            picker_busy.set(false);
        });
    };

    let add_member = move |uid: uuid::Uuid| {
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/teams/{team_id}/members");
                let body = AddTeamMemberBody {
                    user_id: uid,
                    role: None,
                };
                #[derive(serde::Deserialize)]
                struct TmResp {
                    #[allow(dead_code)]
                    user_id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed_typed::<TmResp, _>(&path, &body).await {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Member added.");
                        roster.restart();
                    }
                    Err(e) => {
                        crate::hooks::toast::push_toast(AlertType::Warning, e.user_message());
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = uid;
            }
        });
    };

    let remove_member = move |uid: uuid::Uuid| {
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/teams/{team_id}/members/{uid}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Member removed.");
                        roster.restart();
                    }
                    Err(msg) => crate::hooks::toast::push_toast(AlertType::Warning, msg),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = uid;
            }
        });
    };

    rsx! {
        div { class: "border-t border-line pt-3 space-y-2",
            p { class: "text-sm font-medium text-content", "Members" }
            if members.is_empty() {
                p { class: "text-xs text-subtle", "No members yet." }
            } else {
                ul { class: "space-y-1 text-sm",
                    for m in members.into_iter() {
                        li { key: "{m.user_id}", class: "flex items-center justify-between",
                            span { "{m.first_name} {m.last_name} - {m.email} ({m.role})" }
                            button {
                                r#type: "button",
                                class: "text-xs text-red-600 hover:opacity-80 dark:text-red-400",
                                onclick: {
                                    let user_id = m.user_id;
                                    move |_| remove_member(user_id)
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
            div { class: "space-y-1",
                Input {
                    name: "member_picker",
                    label: "Search users to add".to_string(),
                    r#type: "text".to_string(),
                    value: query(),
                    disabled: *saving_parent.read(),
                    oninput: move |e: FormEvent| {
                        let v: String = e.value();
                        query.set(v.clone());
                        if !v.trim().is_empty() {
                            do_search(v.trim().to_string());
                        } else {
                            picker_results.set(Vec::new());
                        }
                    },
                }
                if picker_busy() {
                    p { class: "text-xs text-subtle", "Searching..." }
                } else if !picker_results.read().is_empty() {
                    ul { class: "border border-line rounded-md bg-surface-1 divide-y divide-line",
                        for pick in picker_results.read().iter().cloned() {
                            li {
                                key: "{pick.user_id}",
                                class: "flex items-center justify-between px-3 py-2 text-sm hover:bg-surface-2",
                                div { class: "min-w-0",
                                    div { class: "font-medium truncate", "{pick.display_name}" }
                                    div { class: "text-xs text-subtle truncate", "{pick.email}" }
                                }
                                button {
                                    r#type: "button",
                                    class: "text-xs text-accent hover:opacity-80",
                                    onclick: {
                                        let uid = pick.user_id;
                                        move |_| {
                                            add_member(uid);
                                            query.set(String::new());
                                            picker_results.set(Vec::new());
                                        }
                                    },
                                    "Add"
                                }
                            }
                        }
                    }
                } else if !query.read().trim().is_empty() {
                    p { class: "text-xs text-subtle",
                        "No match. Invite the user from the People tab, then come back to add them."
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PickedUser {
    user_id: uuid::Uuid,
    email: String,
    display_name: String,
}

// -----------------------------------------------------------------------
// Invitations pane
// -----------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct OwnerOutbox {
    #[serde(default)]
    pending: Vec<PendingInvitationView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct PendingInvitationView {
    id: String,
    invitee_email: String,
    role: String,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[component]
fn InvitationsPane() -> Element {
    let refresh = use_signal(|| 0u32);
    let removed: Signal<Vec<String>> = use_signal(Vec::new);
    let error: Signal<String> = use_signal(String::new);
    let saving: Signal<bool> = use_signal(|| false);

    let outbox: Resource<Option<OwnerOutbox>> = use_resource({
        let mut removed = removed;
        move || {
            let _bump = refresh.read();
            async move {
                #[cfg(feature = "app")]
                {
                    let result = crate::hooks::fetch::api::get_authed::<OwnerOutbox>(
                        "/grants?role=owner",
                    )
                    .await
                    .inspect_err(|e| tracing::error!("outbox load failed: {e}"))
                    .ok();
                    if result.is_some() {
                        removed.write().clear();
                    }
                    result
                }
                #[cfg(not(feature = "app"))]
                {
                    None
                }
            }
        }
    });

    let snap = outbox.read_unchecked();
    let loading = snap.is_none();
    let data: OwnerOutbox = match &*snap {
        Some(Some(o)) => o.clone(),
        _ => OwnerOutbox {
            pending: Vec::new(),
        },
    };
    let unreachable = matches!(&*snap, Some(None));

    let removed_snap = removed.read();
    let pending_view: Vec<PendingInvitationView> = data
        .pending
        .iter()
        .filter(|inv| !removed_snap.contains(&inv.id))
        .cloned()
        .collect();
    drop(removed_snap);

    let cancel = {
        let mut refresh = refresh;
        let mut removed = removed;
        let mut error = error;
        let mut saving = saving;
        move |id: String| {
            if saving() {
                return;
            }
            saving.set(true);
            error.set(String::new());
            removed.write().push(id.clone());
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/invitations/{id}");
                    match crate::hooks::fetch::api::delete_authed(&path).await {
                        Ok(_) => {
                            *refresh.write() += 1;
                        }
                        Err(msg) => {
                            removed.write().retain(|x| x != &id);
                            error.set(msg);
                        }
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = id;
                }
                saving.set(false);
            });
        }
    };

    rsx! {
        if unreachable {
            Card {
                div { class: "p-4 text-sm text-red-600 dark:text-red-400",
                    "Could not reach the API to load pending invitations. Refresh to try again."
                }
            }
        }
        if !error.read().is_empty() {
            div { role: "alert", class: "text-sm text-red-600 dark:text-red-400 mb-3", "{error}" }
        }

        div { class: "flex items-center justify-between mb-3",
            p { class: "text-sm text-subtle",
                "Invitations you've sent that haven't been accepted yet."
            }
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| {
                    *crate::components::SHOW_INVITE_MEMBER.write() = true;
                },
                "Invite guest"
            }
        }

        Card {
            if loading {
                div { class: "p-4 text-sm text-subtle", "Loading..." }
            } else if pending_view.is_empty() {
                div { class: "p-4 text-sm text-subtle",
                    "No pending invitations. Invite a guest from the People tab, or use the button above."
                }
            } else {
                ul { class: "divide-y divide-line",
                    for inv in pending_view.iter() {
                        li {
                            key: "{inv.id}",
                            class: "flex items-center justify-between px-4 py-3 gap-4",
                            div { class: "min-w-0 flex-1",
                                div { class: "font-medium truncate", "{inv.invitee_email}" }
                                div { class: "text-xs text-subtle",
                                    "Role: {humanise_role(&inv.role)}"
                                    if let Some(exp) = inv.expires_at.as_ref() {
                                        span { class: "ml-2", "Expires {exp.format(\"%Y-%m-%d\")}" }
                                    }
                                }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                r#type: "button".to_string(),
                                disabled: saving(),
                                onclick: {
                                    let id = inv.id.clone();
                                    let mut cancel = cancel;
                                    move |_| cancel(id.clone())
                                },
                                "Cancel"
                            }
                        }
                    }
                }
            }
        }
    }
}

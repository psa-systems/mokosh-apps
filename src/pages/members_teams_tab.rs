//! Teams tab of `/settings/members` (MAPPS-877 phase 4).
//!
//! Lifted from the pre-phase-2 `src/pages/teams.rs` body and reshaped so it
//! renders inside the members-page Tab::Teams match arm rather than as its
//! own `/admin/teams` page. Three visible differences from the original:
//!
//! - The outer page already gated on `is_org_tenant` + `can_manage_users`,
//!   so this tab assumes the caller is at least a manager and the row
//!   actions assume admin only. No duplicate `ContentUnavailable` branch
//!   here.
//! - The Members column reads from the server's `member_count` (phase 1
//!   added the field with `#[serde(default, skip_serializing_if = None)]`
//!   on `Team`); a redeploy that drops it falls back to `-` on this side,
//!   never a stale number.
//! - The em-dash between the member's name and their email on the roster
//!   line is swapped for a hyphen, per the project's no-em-dash rule.
//!
//! The 867-line pre-phase-2 body is preserved substantially intact.
//! The Add-member flow now uses the shared `UserPicker` component
//! (`/auth/users` search), replacing the earlier raw-UUID input, and
//! the roster's user ids are passed as `exclude_ids` so an already-
//! added member doesn't show up as a selectable match.
//!
//! `/admin/teams`'s original source lives at `src/pages/teams.rs`, which
//! is now a one-component redirect stub pointing at this tab.

#![cfg(feature = "multi-tenant")]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    AlertType, Badge, BadgeVariant, Button, ButtonVariant, DataTable, Input, Modal, Select,
    SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
    TableRow,
};

/// Team row as returned by `GET /api/v1/teams` (mirror of
/// mokosh_types::teams::Team).
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RemoteTeam {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    manager_id: Option<uuid::Uuid>,
    #[serde(default)]
    color: Option<String>,
    is_active: bool,
    /// MAPPS-877 phase 1 added this on the server-side `Team` DTO. Older
    /// server responses omit it; the cell falls back to "-" so the SPA
    /// stays readable until the server redeploys.
    #[serde(default)]
    member_count: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedTeams {
    data: Vec<RemoteTeam>,
    #[serde(default)]
    meta: PaginationMeta,
}

/// Server-side paginated envelope's meta block (`PaginatedResponse::meta`).
/// Only `total` is read here; the rest of the meta shape is not needed for
/// rendering the roster.
#[derive(Clone, Debug, Default, Deserialize)]
struct PaginationMeta {
    #[serde(default)]
    total: u64,
}

/// Matches the `per_page` sent server-side, so the requested page size and
/// the `DataTable`'s pager math agree. Mirrors the tenant / invitation
/// rosters.
const PER_PAGE: usize = 25;

/// Team member with joined user fields, from `GET
/// /api/v1/teams/{id}/members`.
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
    manager_id: Option<uuid::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

#[derive(Serialize)]
struct UpdateTeamBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    // MAPPS-819: `null` (not omission) clears the manager server-side
    // (mokosh-server UpdateTeamRequest treats every field as "set to this
    // value", not "set if present"), so this is always sent, never skipped.
    manager_id: Option<uuid::Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_active: Option<bool>,
}

fn manager_options(users: &[crate::hooks::UserRow]) -> Vec<SelectOption> {
    let mut options: Vec<SelectOption> = vec![SelectOption::new("", "No manager")];
    options.extend(users.iter().map(|u| {
        let name = format!("{} {}", u.first_name, u.last_name);
        let label = if name.trim().is_empty() {
            u.email.clone()
        } else {
            name
        };
        SelectOption::new(u.id.to_string(), label)
    }));
    options
}

#[derive(Serialize)]
struct AddTeamMemberBody {
    user_id: uuid::Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

/// Teams tab rendered inside the members page.
///
/// MAPPS-877 phase 4: the outer `MembersPage` owns the personal-tenant
/// and manager-and-up gates, so this tab assumes the caller is at least
/// a manager and the row actions check for admin on their own. The
/// create button and the Edit action render only when the caller is an
/// admin; the roster itself renders for every manager.
#[component]
pub fn TeamsTab() -> Element {
    let auth = crate::hooks::use_auth();
    let mut show_create = use_signal(|| false);
    let mut edit_target: Signal<Option<RemoteTeam>> = use_signal(|| None);
    // Paging state for the roster. Read inside the resource closure below
    // (not captured by value) so a page change actually subscribes the
    // resource and re-fetches, matching the tenant / invitation rosters.
    let mut page = use_signal(|| 1usize);
    let mut teams_resource = use_resource(move || {
        let current_page = (*page.read()).max(1);
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            #[cfg(feature = "app")]
            {
                let token = crate::hooks::fetch::api::current_access_token()?;
                let path = format!("/teams?page={current_page}&per_page={PER_PAGE}");
                crate::hooks::fetch::api::get_with_auth::<PaginatedTeams>(&path, &token)
                    .await
                    .inspect_err(|e| tracing::error!("team list load failed: {e}"))
                    .ok()
            }
            #[cfg(not(feature = "app"))]
            {
                None::<PaginatedTeams>
            }
        }
    });
    let can_mutate = crate::hooks::use_can_mutate();
    let is_admin = auth.read().is_admin();

    let snap = teams_resource.read_unchecked();
    let is_loading = snap.is_none();
    let (mut teams, total): (Vec<RemoteTeam>, u64) = match &*snap {
        Some(Some(payload)) => (payload.data.clone(), payload.meta.total),
        _ => (Vec::new(), 0),
    };
    // Defensive sort by name on the SPA side so a server-side reorder
    // (phase 1's `ORDER BY name` is a soft contract that could flip
    // between releases) does not shift the row order under the user.
    teams.sort_by_key(|a| a.name.to_lowercase());
    let current_page = (*page.read()).max(1);

    rsx! {
        div { class: "flex items-center justify-between mb-3",
            div {}
            if is_admin {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    onclick: move |_| show_create.set(true),
                    "Create team"
                }
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 4,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Members" }
                        TableHeader { "Status" }
                        TableHeader { span { class: "sr-only", "Actions" } }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 3 }
                } else if teams.is_empty() {
                    TableEmpty {
                        columns: 4,
                        message: "No teams yet. Create your first team to route tickets to sub-groups of users.".to_string(),
                    }
                } else {
                    TableBody {
                        for team in teams.into_iter() {
                            TeamRow {
                                key: "{team.id}",
                                team: team.clone(),
                                is_admin,
                                on_edit: {
                                    let row = team.clone();
                                    move |_| edit_target.set(Some(row.clone()))
                                },
                            }
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
                onclose: move |_| edit_target.set(None),
                onsaved: move |_| {
                    edit_target.set(None);
                    teams_resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TeamRowProps {
    team: RemoteTeam,
    is_admin: bool,
    on_edit: EventHandler<()>,
}

#[component]
fn TeamRow(props: TeamRowProps) -> Element {
    let color = props
        .team
        .color
        .clone()
        .unwrap_or_else(|| "#6366F1".to_string());
    let status_variant = if props.team.is_active {
        BadgeVariant::Green
    } else {
        BadgeVariant::Gray
    };
    let status_label = if props.team.is_active {
        "Active"
    } else {
        "Archived"
    };

    rsx! {
        TableRow {
            TableCell {
                div { class: "flex items-center gap-3",
                    span {
                        class: "inline-block w-3 h-3 rounded-full",
                        style: "background-color: {color};",
                    }
                    // Dioxus HTML-escapes text nodes by default, so a team
                    // name containing `<script>` renders inert (security
                    // review F5 client-side pin).
                    span { class: "font-medium text-content", "{props.team.name}" }
                }
            }
            TableCell { class: "text-muted",
                {
                    // MAPPS-877 phase 4: server-side count. The `-` fallback
                    // only ever fires against a server that has not been
                    // redeployed to phase 1 (older `Team` DTO omits the
                    // field via its `skip_serializing_if = None`).
                    match props.team.member_count {
                        Some(n) => n.to_string(),
                        None => "-".to_string(),
                    }
                }
            }
            TableCell { Badge { variant: status_variant, "{status_label}" } }
            TableCell { class: "text-right",
                if props.is_admin {
                    button {
                        r#type: "button",
                        class: "text-sm text-accent hover:opacity-80",
                        onclick: move |_| props.on_edit.call(()),
                        "Edit"
                    }
                } else {
                    span { class: "text-xs text-muted", "-" }
                }
            }
        }
    }
}

#[component]
fn CreateTeamModal(onclose: EventHandler<()>, onsaved: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut manager_id = use_signal(String::new);
    let mut color = use_signal(|| String::from("#6366F1"));
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // MAPPS-860: shared roster cache, not a per-modal fetch.
    let users_resource = crate::hooks::use_user_roster(true);
    let users_snap = users_resource.read_unchecked();
    let users: Vec<crate::hooks::UserRow> = users_snap.clone().unwrap_or_default();
    let manager_select_options = manager_options(&users);

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
                if d.is_empty() {
                    None
                } else {
                    Some(d)
                }
            },
            manager_id: manager_id.read().parse::<uuid::Uuid>().ok(),
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
                match crate::hooks::fetch::api::post_authed_typed::<TeamId, _>("/teams", &body)
                    .await
                {
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
                    oninput: move |e: FormEvent| { description.set(e.value()); },
                }
                Select {
                    name: "team_manager_id",
                    label: "Manager (optional)",
                    options: manager_select_options,
                    value: manager_id(),
                    disabled: saving(),
                    onchange: move |e: FormEvent| { manager_id.set(e.value()); },
                }
                p { class: "text-xs text-muted",
                    "The manager can edit or archive this team without needing an admin role."
                }
                Input {
                    name: "team_color",
                    label: "Color (hex, e.g. #6366F1)",
                    r#type: "text".to_string(),
                    value: color(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| { color.set(e.value()); },
                }
                p { class: "text-xs text-muted",
                    "Use a 7-character hex color like #6366F1."
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
    let mut manager_id =
        use_signal(|| team.manager_id.map(|id| id.to_string()).unwrap_or_default());
    let mut color = use_signal(|| team.color.clone().unwrap_or_else(|| "#6366F1".into()));
    let mut is_active = use_signal(|| team.is_active);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    // MAPPS-860: shared roster cache, not a per-modal fetch.
    let users_resource = crate::hooks::use_user_roster(true);
    let users_snap = users_resource.read_unchecked();
    let users: Vec<crate::hooks::UserRow> = users_snap.clone().unwrap_or_default();
    let manager_select_options = manager_options(&users);

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
            manager_id: manager_id.read().parse::<uuid::Uuid>().ok(),
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
                    oninput: move |e: FormEvent| { description.set(e.value()); },
                }
                Select {
                    name: "team_manager_id",
                    label: "Manager (optional)",
                    options: manager_select_options,
                    value: manager_id(),
                    disabled: saving(),
                    onchange: move |e: FormEvent| { manager_id.set(e.value()); },
                }
                p { class: "text-xs text-muted",
                    "The manager can edit or archive this team without needing an admin role."
                }
                Input {
                    name: "team_color",
                    label: "Color (hex, e.g. #6366F1)",
                    r#type: "text".to_string(),
                    value: color(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| { color.set(e.value()); },
                }
                label { class: "flex items-center gap-2 text-sm text-content",
                    input {
                        r#type: "checkbox",
                        checked: is_active(),
                        disabled: saving(),
                        onchange: move |e: FormEvent| { is_active.set(e.value() == "true"); },
                    }
                    "Active (uncheck to archive)"
                }
                p { class: "text-xs text-muted",
                    "Archiving hides the team from selection but preserves ticket + appointment references."
                }
            }
        }

        // A minimal member management surface (add + remove) lives here
        // rather than as a separate tab; kept flat so the modal ships
        // in one PR. A tabbed refactor is a follow-up if the roster
        // grows beyond a few members per team.
        MembersSection { team_id, saving_parent: saving }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MembersSectionProps {
    team_id: uuid::Uuid,
    saving_parent: Signal<bool>,
}

#[component]
fn MembersSection(props: MembersSectionProps) -> Element {
    let team_id = props.team_id;
    let mut roster = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "app")]
        {
            let path = format!("/teams/{team_id}/members");
            crate::hooks::fetch::api::get_authed::<Vec<RemoteTeamMember>>(&path)
                .await
                .inspect_err(|e| tracing::error!("team roster load failed for {team_id}: {e}"))
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
    let exclude_ids: Vec<uuid::Uuid> = members.iter().map(|m| m.user_id).collect();
    let mut picked_user_id: Signal<Option<uuid::Uuid>> = use_signal(|| None);
    let mut picked_user_name = use_signal(String::new);
    // MAPPS-436: per-member ConfirmDialog gate for the destructive Remove.
    // `pending_remove` names the member whose confirm dialog is open;
    // `removing` is the in-flight spinner; `remove_error` surfaces the
    // server's refusal reason inside the still-open dialog.
    let mut pending_remove: Signal<Option<uuid::Uuid>> = use_signal(|| None);
    let mut removing = use_signal(|| false);
    let mut remove_error = use_signal(String::new);
    let pending_member = pending_remove
        .read()
        .and_then(|uid| members.iter().find(|m| m.user_id == uid).cloned());

    let on_confirm_remove = move |_: ()| {
        if *removing.read() {
            return;
        }
        let Some(uid) = *pending_remove.read() else {
            return;
        };
        removing.set(true);
        remove_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/teams/{team_id}/members/{uid}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Member removed.");
                        pending_remove.set(None);
                        roster.restart();
                    }
                    Err(msg) => remove_error.set(msg),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = uid;
            }
            removing.set(false);
        });
    };

    let add = move |_| {
        let Some(uid) = *picked_user_id.read() else {
            crate::hooks::toast::push_toast(
                AlertType::Warning,
                "Pick a user from the dropdown first.",
            );
            return;
        };
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
                        picked_user_id.set(None);
                        picked_user_name.set(String::new());
                    }
                    Err(e) => crate::hooks::toast::push_toast(AlertType::Warning, e.user_message()),
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
                p { class: "text-xs text-muted", "No members yet." }
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
                                    move |_| {
                                        remove_error.set(String::new());
                                        pending_remove.set(Some(user_id));
                                    }
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
            div { class: "flex items-end gap-2",
                div { class: "flex-1 min-w-0",
                    crate::components::UserPicker {
                        label: "Add user".to_string(),
                        placeholder: "Search users…".to_string(),
                        value: picked_user_name(),
                        selected_id: picked_user_id().map(|u| u.to_string()),
                        exclude_ids: exclude_ids.clone(),
                        onselect: move |(id, name): (String, String)| {
                            if let Ok(uid) = id.parse::<uuid::Uuid>() {
                                picked_user_id.set(Some(uid));
                                picked_user_name.set(name);
                            }
                        },
                        onclear: move |_| {
                            picked_user_id.set(None);
                            picked_user_name.set(String::new());
                        },
                    }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    disabled: *props.saving_parent.read() || picked_user_id.read().is_none(),
                    onclick: move |_| add(()),
                    "Add"
                }
            }
        }
        crate::components::ConfirmDialog {
            open: pending_remove.read().is_some(),
            title: "Remove team member".to_string(),
            message: match &pending_member {
                Some(m) => format!(
                    "Remove {} {} from this team? This cannot be undone.",
                    m.first_name, m.last_name
                ),
                None => "Remove this team member? This cannot be undone.".to_string(),
            },
            confirm_text: "Remove".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            error: remove_error.read().clone(),
            loading: removing(),
            onconfirm: on_confirm_remove,
            oncancel: move |_| {
                if !removing() {
                    pending_remove.set(None);
                    remove_error.set(String::new());
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MAPPS-819: `CreateTeamBody`/`UpdateTeamBody` must put `manager_id` on
    // the wire so mokosh-server's `assert_can_manage_team`
    // (routes.rs:184, `team.manager_id == Some(user.id)`) has something to
    // match against; before this change the field was never constructed,
    // so every SPA-created team's `manager_id` stayed null server-side.
    #[test]
    fn create_team_body_serializes_manager_id() {
        let manager: uuid::Uuid = "11111111-1111-1111-1111-111111111111".parse().unwrap();
        let body = CreateTeamBody {
            name: "Support".to_string(),
            description: None,
            manager_id: Some(manager),
            color: None,
        };
        let json = serde_json::to_value(&body).expect("serialise");
        assert_eq!(json["manager_id"], serde_json::json!(manager));
    }

    #[test]
    fn create_team_body_omits_manager_id_when_unset() {
        let body = CreateTeamBody {
            name: "Support".to_string(),
            description: None,
            manager_id: None,
            color: None,
        };
        let json = serde_json::to_value(&body).expect("serialise");
        assert!(json.get("manager_id").is_none());
    }

    #[test]
    fn update_team_body_serializes_manager_id() {
        let manager: uuid::Uuid = "22222222-2222-2222-2222-222222222222".parse().unwrap();
        let body = UpdateTeamBody {
            name: None,
            description: None,
            manager_id: Some(manager),
            color: None,
            is_active: None,
        };
        let json = serde_json::to_value(&body).expect("serialise");
        assert_eq!(json["manager_id"], serde_json::json!(manager));
    }

    // `UpdateTeamRequest` treats `manager_id: null` as "clear the manager",
    // so the field must stay on the wire as an explicit `null` (no
    // `skip_serializing_if`) rather than being omitted, or picking "No
    // manager" in the edit form would silently no-op instead of clearing
    // the self-service edit grant.
    #[test]
    fn update_team_body_sends_null_to_clear_manager() {
        let body = UpdateTeamBody {
            name: None,
            description: None,
            manager_id: None,
            color: None,
            is_active: None,
        };
        let json = serde_json::to_value(&body).expect("serialise");
        assert_eq!(json["manager_id"], serde_json::Value::Null);
    }

    #[test]
    fn manager_options_lists_no_manager_first_then_users() {
        let uid: uuid::Uuid = "33333333-3333-3333-3333-333333333333".parse().unwrap();
        let users = vec![crate::hooks::UserRow {
            id: uid,
            first_name: "Ada".to_string(),
            last_name: "Lovelace".to_string(),
            email: "ada@example.com".to_string(),
            ..Default::default()
        }];
        let options = manager_options(&users);
        assert_eq!(options[0].value, "");
        assert_eq!(options[0].label, "No manager");
        assert_eq!(options[1].value, uid.to_string());
        assert_eq!(options[1].label, "Ada Lovelace");
    }
}

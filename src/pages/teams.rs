//! Teams management page (PMS-791 phase 2 / MAPPS-463).
//!
//! `/admin/teams`. List teams, create, edit, archive, manage members.
//! Server side lives at /api/v1/teams (MAPPS-461).

#![cfg(feature = "multi-tenant")]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, AlertType, Badge, BadgeVariant, Button, ButtonVariant, DataTable, Input, Modal,
    PageHeader, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading,
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
}

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

/// Main teams roster page.
#[component]
pub fn TeamsPage() -> Element {
    // MAPPS-602: every hook fires BEFORE the personal-tenant early
    // return so the render that takes the exit does not leave the
    // component a hook short.
    use_page_title("Teams");
    let auth = crate::hooks::use_auth();
    let mut show_create = use_signal(|| false);
    let mut edit_target: Signal<Option<RemoteTeam>> = use_signal(|| None);
    let mut teams_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "app")]
        {
            let token = crate::hooks::fetch::api::current_access_token()?;
            crate::hooks::fetch::api::get_with_auth::<Vec<RemoteTeam>>("/teams", &token)
                .await
                .inspect_err(|e| tracing::error!("team list load failed: {e}"))
                .ok()
        }
        #[cfg(not(feature = "app"))]
        {
            None::<Vec<RemoteTeam>>
        }
    });
    let can_mutate = crate::hooks::use_can_mutate();
    let is_admin = auth.read().is_admin();
    let is_org_tenant = auth.read().is_org_tenant();

    // Personal tenant: bounce (nav should hide too but a direct URL hit
    // deserves an honest message rather than a broken create button).
    if !is_org_tenant {
        return rsx! {
            crate::components::ContentUnavailable {
                title: "Teams".to_string(),
            }
        };
    }

    // MAPPS-526: mokosh-server gates the /teams read + write endpoints on
    // `RequireAdmin`, so a non-admin who reached this URL directly (nav
    // hides the row for them) would render an empty roster and every
    // action would 403 server-side. Refuse the page instead, matching
    // the pattern audit_log.rs uses for the other /admin/* surfaces.
    if !is_admin {
        return rsx! {
            PageHeader {
                title: "Teams",
                subtitle: "Sub-groups of users inside your organization.",
            }
            crate::components::Card {
                div { class: "py-12 text-center",
                    p { class: "text-sm font-medium text-content mb-1",
                        "Admins only"
                    }
                    p { class: "text-sm text-muted",
                        "You do not have permission to manage teams."
                    }
                }
            }
        };
    }

    let snap = teams_resource.read_unchecked();
    let is_loading = snap.is_none();
    let teams: Vec<RemoteTeam> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };

    rsx! {
        PageHeader {
            title: "Teams",
            subtitle: "Sub-groups of users inside your organization.",
            actions: rsx! {
                if is_admin {
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled: !can_mutate,
                        onclick: move |_| show_create.set(true),
                        "Create team"
                    }
                }
            },
        }

        DataTable {
            loading: is_loading,
            total_items: teams.len(),
            current_page: 1,
            per_page: 50,
            columns: 4,
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
                "-"
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
                if d.is_empty() {
                    None
                } else {
                    Some(d)
                }
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
                    oninput: move |e: FormEvent| { description.set(e.value()); },
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
    let mut new_user_id = use_signal(String::new);
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
        let raw = new_user_id.read().trim().to_string();
        let Ok(uid) = raw.parse::<uuid::Uuid>() else {
            crate::hooks::toast::push_toast(
                AlertType::Warning,
                "Enter a valid user UUID (temporary UX; a user picker lands in a follow-up).",
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
                        new_user_id.set(String::new());
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
                            span { "{m.first_name} {m.last_name} — {m.email} ({m.role})" }
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
                Input {
                    name: "new_member_user_id",
                    label: "Add user (UUID)",
                    r#type: "text".to_string(),
                    value: new_user_id(),
                    disabled: *props.saving_parent.read(),
                    oninput: move |e: FormEvent| { new_user_id.set(e.value()); },
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    disabled: *props.saving_parent.read(),
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

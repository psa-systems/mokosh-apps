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
    use_page_title, AlertType, Badge, BadgeVariant, BannerTone, Button, ButtonVariant,
    ConfirmDialog, ContentUnavailable, Input, Modal, PageHeader, Select, SelectOption,
    StatusBanner,
};
use crate::Route;

/// The six-ish roles the server accepts on both `/users` and `/grants`.
/// Matches the PMS-1162 projection; a seventh role (`super_admin`) is
/// deliberately absent from the picker because it is bootstrap-only and
/// the server refuses to assign it through either endpoint. Rendered as
/// `SelectOption`s on every row's role picker.
const ROLE_PICKER: &[(&str, &str)] = &[
    ("admin", "Admin"),
    ("manager", "Manager"),
    ("technician", "Technician"),
    ("dispatcher", "Dispatcher"),
    ("sales", "Sales"),
    ("finance", "Finance"),
    ("read_only", "Read-only"),
];

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

/// Dispatch a role change on `row` to `new_role` through the right
/// endpoint for the row kind. Native users route through `PUT /users` so
/// the role lands on the users row; placed and unplaced guests route
/// through `PATCH /grants` because the role lives on the grant, not on
/// the users row. The pure path functions above are used so a refactor
/// that touches the URL table has one place to look.
#[cfg(feature = "app")]
async fn change_role(row: &MemberRow, new_role: &str) -> Result<(), String> {
    let path = change_role_path(row);
    let body = serde_json::json!({ "role": new_role });
    match row {
        MemberRow::User {
            placed_by_grant_id: None,
            ..
        } => crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
            .await
            .map(|_| ()),
        MemberRow::User {
            placed_by_grant_id: Some(_),
            ..
        }
        | MemberRow::UnplacedGuest { .. } => {
            crate::hooks::fetch::api::patch_authed::<serde_json::Value, _>(&path, &body)
                .await
                .map(|_| ())
        }
    }
}

/// Dispatch a remove on `row` through the right endpoint for the row
/// kind. A native user is DEACTIVATED (status=inactive); a guest (placed
/// or unplaced) has its grant REVOKED. The two outcomes read the same to
/// the operator (the row disappears from the roster) but differ in what
/// gets written: the users row stays, tombstoned, where the grant row is
/// updated with `revoked_at`.
#[cfg(feature = "app")]
async fn remove(row: &MemberRow) -> Result<(), String> {
    let path = remove_action_path(row);
    match row {
        MemberRow::User {
            placed_by_grant_id: None,
            ..
        } => crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
            &path,
            &serde_json::json!({ "status": "inactive" }),
        )
        .await
        .map(|_| ()),
        MemberRow::User {
            placed_by_grant_id: Some(_),
            ..
        }
        | MemberRow::UnplacedGuest { .. } => crate::hooks::fetch::api::delete_authed(&path).await,
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

    let mut show_invite_user = use_signal(|| false);

    rsx! {
        PageHeader {
            title: "Members".to_string(),
            actions: if is_admin && active == Tab::People {
                Some(rsx! {
                    // "Invite guest" would create a bunyip grant invitation
                    // that lands this tenant in another workspace's roster
                    // through the webhook that feeds `mokosh_bunyip_grants`.
                    // The endpoint is not here yet (follow-up on the server),
                    // so the affordance is shown but disabled with a hint
                    // instead of fabricating a flow.
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: true,
                        title: "Guest invitations are issued from the Bunyip hub. Coming soon.".to_string(),
                        "Invite guest"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| show_invite_user.set(true),
                        "Invite user"
                    }
                })
            } else {
                None
            },
        }
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
                crate::pages::members_teams_tab::TeamsTab {}
            },
            Tab::Invitations => rsx! {
                InvitationsTab { can_mutate: is_admin }
            },
        }
        if show_invite_user() {
            InviteUserModal {
                onclose: move |_| show_invite_user.set(false),
                onsaved: move |_| {
                    show_invite_user.set(false);
                    navigator.replace(Route::MembersPage { tab: "invitations".to_string() });
                },
            }
        }
    }
}

#[component]
fn InviteUserModal(onclose: EventHandler<()>, onsaved: EventHandler<()>) -> Element {
    let mut email = use_signal(String::new);
    let mut role = use_signal(|| String::from("technician"));
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let role_options: Vec<SelectOption> = ROLE_PICKER
        .iter()
        .map(|(v, l)| SelectOption::new(*v, *l))
        .collect();

    let submit = move |_| {
        if saving() {
            return;
        }
        let e = email.read().trim().to_string();
        if e.is_empty() {
            error.set("Email is required.".to_string());
            return;
        }
        let r = role.read().clone();
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let body = serde_json::json!({ "email": e, "role": r });
                #[derive(serde::Deserialize)]
                struct Created {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed::<Created, _>("/invitations", &body)
                    .await
                {
                    Ok(_) => {
                        crate::hooks::toast::push_toast(AlertType::Success, "Invitation sent.");
                        onsaved.call(());
                    }
                    Err(err) => error.set(format!("Could not send invite: {err}")),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (e, r);
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "Invite user".to_string(),
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
                    "Send invite"
                }
            },
            div { class: "space-y-3",
                if !error.read().is_empty() {
                    StatusBanner { tone: BannerTone::Error, class: String::new(),
                        {error.read().clone()}
                    }
                }
                Input {
                    name: "invite_email",
                    label: "Email",
                    r#type: "email".to_string(),
                    value: email(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); email.set(e.value()); },
                }
                Select {
                    name: "invite_role",
                    label: "Role",
                    options: role_options,
                    value: role(),
                    disabled: saving(),
                    onchange: move |e: FormEvent| { role.set(e.value()); },
                }
                p { class: "text-xs text-muted",
                    "The invitee receives an email with a link to accept and set their password."
                }
            }
        }
    }
}

#[component]
fn PeoplePane(can_mutate: bool) -> Element {
    let mut members = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<MembersResponse>("/members?per_page=100")
            .await
            .inspect_err(|e| tracing::error!("members list load failed: {e}"))
            .ok()
    });
    let snap = members.read_unchecked();
    // Action state: the row the operator picked for removal, and the last
    // dispatch error (rendered inside the confirm dialog so it sits next
    // to the button that produced it, matching the ConfirmDialog's
    // `error` prop contract).
    let mut remove_target: Signal<Option<MemberRow>> = use_signal(|| None);
    let mut remove_error: Signal<String> = use_signal(String::new);
    let mut remove_busy: Signal<bool> = use_signal(|| false);
    let mut role_error: Signal<String> = use_signal(String::new);

    let on_change_role = move |(row, new_role): (MemberRow, String)| {
        role_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match change_role(&row, &new_role).await {
                    Ok(()) => members.restart(),
                    Err(e) => role_error.set(format!("Could not change role: {e}")),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (row, new_role);
            }
        });
    };

    let on_confirm_remove = move |_| {
        let Some(row) = remove_target.read().clone() else {
            return;
        };
        remove_busy.set(true);
        remove_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match remove(&row).await {
                    Ok(()) => {
                        remove_target.set(None);
                        remove_busy.set(false);
                        members.restart();
                    }
                    Err(e) => {
                        remove_busy.set(false);
                        remove_error.set(format!("Could not remove: {e}"));
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = row;
                remove_busy.set(false);
            }
        });
    };

    let on_cancel_remove = move |_| {
        remove_target.set(None);
        remove_error.set(String::new());
    };

    let (confirm_title, confirm_message, confirm_text) = match &*remove_target.read() {
        Some(MemberRow::User {
            placed_by_grant_id: None,
            ..
        }) => {
            let target = remove_target.read().clone().unwrap();
            (
                format!("Deactivate {}?", row_display_name(&target)),
                "Their access to this workspace ends. You can reactivate them later.".to_string(),
                "Deactivate".to_string(),
            )
        }
        Some(row) => (
            format!("Revoke access for {}?", row_display_name(row)),
            "They lose access to this workspace immediately. You can re-invite later.".to_string(),
            "Revoke access".to_string(),
        ),
        None => (String::new(), String::new(), "Confirm".to_string()),
    };

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
                if !role_error.read().is_empty() {
                    StatusBanner { tone: BannerTone::Error, class: "mb-3".to_string(),
                        {role_error.read().clone()}
                    }
                }
                PeopleBody {
                    payload: payload.clone(),
                    can_mutate,
                    on_change_role,
                    on_remove: move |row: MemberRow| {
                        remove_error.set(String::new());
                        remove_target.set(Some(row));
                    },
                }
            },
        }
        ConfirmDialog {
            open: remove_target.read().is_some(),
            title: confirm_title,
            message: confirm_message,
            confirm_text,
            destructive: true,
            loading: *remove_busy.read(),
            error: remove_error.read().clone(),
            onconfirm: on_confirm_remove,
            oncancel: on_cancel_remove,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PeopleBodyProps {
    payload: MembersResponse,
    can_mutate: bool,
    on_change_role: EventHandler<(MemberRow, String)>,
    on_remove: EventHandler<MemberRow>,
}

#[component]
fn PeopleBody(props: PeopleBodyProps) -> Element {
    let payload = props.payload;
    let can_mutate = props.can_mutate;
    let on_change_role = props.on_change_role;
    let on_remove = props.on_remove;
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
                                        td { class: "py-2 pr-4",
                                            div { class: "flex items-center gap-2",
                                                {
                                                    let row_for_role = row.clone();
                                                    let options: Vec<SelectOption> = ROLE_PICKER
                                                        .iter()
                                                        .map(|(v, l)| SelectOption::new(*v, *l))
                                                        .collect();
                                                    rsx! {
                                                        Select {
                                                            name: format!("role-{key}"),
                                                            options,
                                                            value: role.clone(),
                                                            onchange: move |e: FormEvent| {
                                                                on_change_role
                                                                    .call((row_for_role.clone(), e.value()));
                                                            },
                                                        }
                                                    }
                                                }
                                                {
                                                    let row_for_remove = row.clone();
                                                    rsx! {
                                                        button {
                                                            r#type: "button",
                                                            class: "text-sm text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
                                                            onclick: move |_| on_remove.call(row_for_remove.clone()),
                                                            "Remove"
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
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Invitations tab (MAPPS-877 phase 5)
//
// The design calls for a cross-account grant-invitation list from
// `/grants?role=owner`, but that server surface does not exist in this
// branch: the grants mirror in `mokosh_bunyip_grants` carries only
// granted / revoked state, no pending-invitation lifecycle (PMS-1208
// filed as the follow-up for that). The pragmatic Invitations tab here
// shows the EXISTING pending team-invitations (`GET /invitations`), the
// same source `/admin/invitations` reads, so the operator has a working
// "who have I invited" view in one place inside the members page.
// Cancel per row stays optimistic (no confirm; the invitee has never
// had access, so cancelling costs nothing) matching the design's
// Cancel-is-one-click posture.
//
// When PMS-1208 ships the grant-invitation surface, this tab's fetch
// swaps to the richer `/grants?role=owner` shape and the two sources
// become one list; nothing on the SPA shell changes.

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TeamInvitation {
    id: uuid::Uuid,
    #[serde(default)]
    email: String,
    #[serde(default)]
    role: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaginatedInvitations {
    #[serde(default)]
    data: Vec<TeamInvitation>,
}

#[component]
fn InvitationsTab(can_mutate: bool) -> Element {
    let mut invites = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<PaginatedInvitations>(
            "/invitations?page=1&per_page=100",
        )
        .await
        .inspect_err(|e| tracing::error!("invitations list load failed: {e}"))
        .ok()
    });
    let snap = invites.read_unchecked();
    let mut cancel_error: Signal<String> = use_signal(String::new);
    let mut pending_cancel: Signal<Option<(uuid::Uuid, String)>> = use_signal(|| None);
    let mut cancelling: Signal<bool> = use_signal(|| false);

    let on_confirm_cancel = move |_| {
        let Some((invite_id, _email)) = pending_cancel.read().clone() else {
            return;
        };
        cancelling.set(true);
        cancel_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/invitations/{invite_id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => {
                        pending_cancel.set(None);
                        cancelling.set(false);
                        invites.restart();
                    }
                    Err(e) => {
                        cancelling.set(false);
                        cancel_error.set(format!("Could not cancel: {e}"));
                    }
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = invite_id;
                cancelling.set(false);
            }
        });
    };

    let on_cancel_dialog = move |_| {
        if !*cancelling.read() {
            pending_cancel.set(None);
            cancel_error.set(String::new());
        }
    };

    let (confirm_title, confirm_message) = match &*pending_cancel.read() {
        Some((_, email)) => (
            format!("Cancel invitation for {email}?"),
            "They will no longer be able to accept this invite. You can send a new one later."
                .to_string(),
        ),
        None => (String::new(), String::new()),
    };

    rsx! {
        if !cancel_error.read().is_empty() {
            StatusBanner { tone: BannerTone::Error, class: "mb-3".to_string(),
                {cancel_error.read().clone()}
            }
        }
        match &*snap {
            None => rsx! { p { class: "text-sm text-muted", "Loading…" } },
            Some(None) => rsx! {
                StatusBanner { tone: BannerTone::Warning, class: "mb-3".to_string(),
                    "Couldn't load pending invitations. Refresh to try again."
                }
            },
            Some(Some(payload)) if payload.data.is_empty() => rsx! {
                p { class: "text-sm text-muted",
                    "No pending invitations. Invite a user from the "
                    Link {
                        to: Route::MembersPage { tab: "people".to_string() },
                        class: "text-accent hover:opacity-90",
                        "People tab"
                    }
                    " to send one."
                }
            },
            Some(Some(payload)) => rsx! {
                table { class: "min-w-full text-sm",
                    thead {
                        tr { class: "text-left text-muted",
                            th { class: "py-2 pr-4", "Email" }
                            th { class: "py-2 pr-4", "Role" }
                            th { class: "py-2 pr-4", "Expires" }
                            if can_mutate {
                                th { class: "py-2 pr-4", span { class: "sr-only", "Actions" } }
                            }
                        }
                    }
                    tbody {
                        for invite in payload.data.iter() {
                            {
                                let id = invite.id;
                                let email = invite.email.clone();
                                let role = invite.role.clone();
                                let expires = invite.expires_at.format("%Y-%m-%d").to_string();
                                rsx! {
                                    tr { key: "{id}", class: "border-t border-line",
                                        td { class: "py-2 pr-4 font-medium", "{email}" }
                                        td { class: "py-2 pr-4 text-muted", "{role}" }
                                        td { class: "py-2 pr-4 text-muted", "{expires}" }
                                        if can_mutate {
                                            td { class: "py-2 pr-4",
                                                button {
                                                    r#type: "button",
                                                    class: "text-sm text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300",
                                                    onclick: {
                                                        let email_for_prompt = email.clone();
                                                        move |_| {
                                                            cancel_error.set(String::new());
                                                            pending_cancel.set(Some((id, email_for_prompt.clone())));
                                                        }
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
                }
            },
        }
        ConfirmDialog {
            open: pending_cancel.read().is_some(),
            title: confirm_title,
            message: confirm_message,
            confirm_text: "Cancel invitation".to_string(),
            cancel_text: "Keep invitation".to_string(),
            destructive: true,
            loading: *cancelling.read(),
            error: cancel_error.read().clone(),
            onconfirm: on_confirm_cancel,
            oncancel: on_cancel_dialog,
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

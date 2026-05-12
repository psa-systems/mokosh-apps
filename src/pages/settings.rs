//! Settings pages

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, CogIcon, IconSize, PageHeader,
    PlusIcon,
};
use crate::hooks::{use_auth, use_require_role};
use crate::Route;

/// Main settings page
#[component]
pub fn SettingsPage() -> Element {
    rsx! {
        AppLayout { title: "Settings",
            PageHeader {
                title: "Settings",
                subtitle: "Configure your Mokosh platform",
            }

            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6",
                SettingsCard {
                    title: "User Management",
                    description: "Manage users, roles, and permissions",
                    route: Route::UserManagement {},
                    icon: "users",
                }
                SettingsCard {
                    title: "Team Management",
                    description: "Configure teams and queues",
                    route: Route::TeamManagement {},
                    icon: "team",
                }
                SettingsCard {
                    title: "Notifications",
                    description: "Email templates and notification channels",
                    route: Route::NotificationSettings {},
                    icon: "bell",
                }
                SettingsCard {
                    title: "Integrations",
                    description: "RMM, email, and third-party integrations",
                    route: Route::IntegrationSettings {},
                    icon: "link",
                }
                SettingsCard {
                    title: "Billing",
                    description: "Rate cards, taxes, and billing settings",
                    route: Route::BillingSettings {},
                    icon: "billing",
                }
                SettingsCard {
                    title: "Active sessions",
                    description: "Devices currently signed in to your account",
                    route: Route::SessionsList {},
                    icon: "shield",
                }
                SettingsCard {
                    title: "Security",
                    description: "Two-factor authentication and recovery codes",
                    route: Route::Security {},
                    icon: "shield",
                }
                SettingsCard {
                    title: "Profile",
                    description: "Your name, timezone, avatar, password",
                    route: Route::Profile {},
                    icon: "users",
                }
                SettingsCard {
                    title: "Audit logs",
                    description: "Security events recorded by the auth subsystem",
                    route: Route::AuditLogs {},
                    icon: "shield",
                }
                SettingsCard {
                    title: "Switch tenant",
                    description: "Pick which tenant you want to act under",
                    route: Route::ActiveTenant {},
                    icon: "users",
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SettingsCardProps {
    title: String,
    description: String,
    route: Route,
    icon: String,
}

#[component]
fn SettingsCard(props: SettingsCardProps) -> Element {
    rsx! {
        Link { to: props.route,
            Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                div { class: "flex items-start",
                    div { class: "flex-shrink-0 w-10 h-10 bg-blue-100 dark:bg-blue-900 rounded-lg flex items-center justify-center",
                        CogIcon { class: "h-5 w-5 text-blue-600 dark:text-blue-400".to_string() }
                    }
                    div { class: "ml-4",
                        h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                            "{props.title}"
                        }
                        p { class: "text-sm text-gray-500 dark:text-gray-400 mt-1",
                            "{props.description}"
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// User management - real data
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct UserView {
    id: String,
    email: String,
    role: String,
    status: String,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    mfa_enrolled: bool,
    #[serde(default)]
    last_login_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
struct UserListBody {
    users: Vec<UserView>,
}

#[derive(Clone, Debug, Deserialize)]
struct InvitesEnvelope {
    invites: Vec<serde_json::Value>,
}

fn role_label(role: &str) -> &'static str {
    match role {
        "admin" => "Admin",
        "manager" => "Manager",
        "finance" => "Finance",
        "member" => "Member",
        "readonly" => "Read only",
        _ => "Other",
    }
}

fn role_badge(role: &str) -> BadgeVariant {
    match role {
        "admin" => BadgeVariant::Red,
        "manager" => BadgeVariant::Blue,
        "finance" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    }
}

fn status_label(s: &str) -> &'static str {
    match s {
        "active" => "Active",
        "suspended" => "Suspended",
        "pending" => "Pending",
        "deleted" => "Deleted",
        _ => "Unknown",
    }
}

fn status_badge(s: &str) -> BadgeVariant {
    match s {
        "active" => BadgeVariant::Green,
        "suspended" => BadgeVariant::Gray,
        "pending" => BadgeVariant::Blue,
        _ => BadgeVariant::Gray,
    }
}

fn display_name(u: &UserView) -> String {
    match (u.first_name.as_deref(), u.last_name.as_deref()) {
        (Some(f), Some(l)) if !f.is_empty() && !l.is_empty() => format!("{f} {l}"),
        (Some(f), _) if !f.is_empty() => f.to_string(),
        (_, Some(l)) if !l.is_empty() => l.to_string(),
        _ => u.email.clone(),
    }
}

fn relative_time(ts: DateTime<Utc>) -> String {
    let d = Utc::now() - ts;
    if d < chrono::Duration::minutes(1) {
        "just now".into()
    } else if d < chrono::Duration::hours(1) {
        format!("{} min ago", d.num_minutes())
    } else if d < chrono::Duration::days(1) {
        format!("{}h ago", d.num_hours())
    } else if d < chrono::Duration::days(30) {
        format!("{}d ago", d.num_days())
    } else {
        ts.format("%Y-%m-%d").to_string()
    }
}

/// User management page
#[component]
pub fn UserManagementPage() -> Element {
    let _auth = use_require_role("admin");
    let navigator = use_navigator();
    // Used to hide the Suspend button on the caller's own row. The
    // server enforces this and the "last active admin" guard, but
    // the UI treatment makes the trap impossible to even attempt.
    let me = use_auth();
    let my_id: String = me
        .read()
        .user
        .as_ref()
        .map(|u| u.id.to_string())
        .unwrap_or_default();
    let mut users: Signal<Option<Result<Vec<UserView>, String>>> = use_signal(|| None);
    let mut invites_count: Signal<Option<usize>> = use_signal(|| None);
    let mut bump = use_signal(|| 0u32);
    let mut busy: Signal<Option<String>> = use_signal(|| None);

    use_future(move || async move {
        let _ = bump.read();
        users.set(None);
        invites_count.set(None);
        let cfg = crate::modules::oidc::OidcConfig::from_env();
        let result = crate::modules::oidc::issuer_get_authed::<UserListBody>(
            &cfg,
            "/v1/auth/users",
        )
        .await
        .map(|b| b.users)
        .map_err(|e| e.to_string());
        users.set(Some(result));
        // Pending-invites count (best effort; do not block users list).
        if let Ok(env) = crate::modules::oidc::issuer_get_authed::<InvitesEnvelope>(
            &cfg,
            "/v1/auth/invites",
        )
        .await
        {
            invites_count.set(Some(env.invites.len()));
        }
    });

    let refetch = use_callback(move |_| { bump.with_mut(|n| *n += 1); });

    let toggle_status = use_callback(move |(id, currently_active): (String, bool)| {
        busy.set(Some(id.clone()));
        spawn(async move {
            let cfg = crate::modules::oidc::OidcConfig::from_env();
            let path = if currently_active {
                format!("/v1/auth/users/{id}/suspend")
            } else {
                format!("/v1/auth/users/{id}/reactivate")
            };
            let _ = crate::modules::oidc::issuer_post_authed_empty(&cfg, &path).await;
            busy.set(None);
            refetch.call(());
        });
    });

    rsx! {
        AppLayout { title: "User Management",
            PageHeader {
                title: "User management".to_string(),
                subtitle: "Active accounts in your organization. Use \"Invite user\" to add a new teammate.".to_string(),
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| { navigator.push(Route::InviteList {}); },
                        if let Some(n) = *invites_count.read() {
                            "Pending invites ({n})"
                        } else {
                            "Pending invites"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| { navigator.push(Route::InviteCreate {}); },
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Invite user"
                    }
                },
            }

            Card { padding: false,
                match users.read().clone() {
                    None => rsx! { div { class: "p-8 text-center text-gray-500", "Loading..." } },
                    Some(Err(msg)) => rsx! {
                        div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800",
                            p { class: "text-sm text-red-600 dark:text-red-400", "Could not load users: {msg}" }
                        }
                    },
                    Some(Ok(rows)) if rows.is_empty() => rsx! {
                        div { class: "p-8 text-center text-gray-500",
                            "No users yet. Invite your first teammate to get started."
                        }
                    },
                    Some(Ok(rows)) => rsx! {
                        table { class: "min-w-full divide-y divide-gray-200 dark:divide-gray-700",
                            thead { class: "bg-gray-50 dark:bg-gray-800",
                                tr {
                                    th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase", "User" }
                                    th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase", "Role" }
                                    th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase", "Status" }
                                    th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase", "Last login" }
                                    th { class: "px-6 py-3", }
                                }
                            }
                            tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
                                for u in rows {
                                    UserRow {
                                        key: "{u.id}",
                                        user: u.clone(),
                                        is_self: u.id == my_id,
                                        busy_id: busy.read().clone(),
                                        on_toggle: {
                                            let id = u.id.clone();
                                            let active = u.status == "active";
                                            move |_| toggle_status.call((id.clone(), active))
                                        },
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct UserRowProps {
    user: UserView,
    is_self: bool,
    busy_id: Option<String>,
    on_toggle: EventHandler<()>,
}

#[component]
fn UserRow(props: UserRowProps) -> Element {
    let u = &props.user;
    let name = display_name(u);
    let initial = name.chars().next().unwrap_or('?').to_string();
    let active = u.status == "active";
    let busy = props.busy_id.as_deref() == Some(u.id.as_str());
    let last = u
        .last_login_at
        .map(relative_time)
        .unwrap_or_else(|| "never".to_string());

    rsx! {
        tr {
            td { class: "px-6 py-4 whitespace-nowrap",
                div { class: "flex items-center",
                    div { class: "w-10 h-10 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center",
                        span { class: "text-sm font-medium text-blue-600 dark:text-blue-400",
                            "{initial}"
                        }
                    }
                    div { class: "ml-4",
                        div { class: "text-sm font-medium text-gray-900 dark:text-white",
                            "{name}"
                            if props.is_self {
                                span { class: "ml-2 text-xs text-gray-500", "(you)" }
                            }
                        }
                        div { class: "text-xs text-gray-500", "{u.email}" }
                    }
                }
            }
            td { class: "px-6 py-4 whitespace-nowrap",
                Badge { variant: role_badge(&u.role), "{role_label(&u.role)}" }
            }
            td { class: "px-6 py-4 whitespace-nowrap",
                Badge { variant: status_badge(&u.status), "{status_label(&u.status)}" }
            }
            td { class: "px-6 py-4 whitespace-nowrap text-sm text-gray-500", "{last}" }
            td { class: "px-6 py-4 whitespace-nowrap text-right text-sm",
                // No Suspend button on the caller's own row. The API
                // also rejects self-suspend (and last-active-admin
                // suspend) so this UI treatment is purely about not
                // showing an action that would be refused. Reactivate
                // is unreachable from your own row anyway since you
                // could not be signed in if you were suspended.
                if !props.is_self {
                    Button {
                        variant: if active { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                        loading: busy,
                        onclick: move |_| props.on_toggle.call(()),
                        if active { "Suspend" } else { "Reactivate" }
                    }
                }
            }
        }
    }
}

/// Team management page
#[component]
pub fn TeamManagementPage() -> Element {
    rsx! {
        AppLayout { title: "Team Management",
            PageHeader {
                title: "Teams & Queues",
                subtitle: "Configure teams and service queues",
                actions: rsx! {
                    Button { variant: ButtonVariant::Primary, "Add Team" }
                },
            }

            div { class: "space-y-6",
                Card { title: "Teams",
                    div { class: "space-y-4",
                        TeamItem {
                            name: "Level 1 Support",
                            members: 3,
                            description: "First line support and triage",
                        }
                        TeamItem {
                            name: "Level 2 Support",
                            members: 2,
                            description: "Advanced technical support",
                        }
                        TeamItem {
                            name: "Projects",
                            members: 4,
                            description: "Project implementation team",
                        }
                    }
                }

                Card { title: "Service Queues",
                    div { class: "space-y-4",
                        QueueItem {
                            name: "General Support",
                            team: "Level 1 Support",
                            open_tickets: 12,
                        }
                        QueueItem {
                            name: "Network",
                            team: "Level 2 Support",
                            open_tickets: 5,
                        }
                        QueueItem {
                            name: "Security",
                            team: "Level 2 Support",
                            open_tickets: 3,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TeamItemProps {
    name: String,
    members: u32,
    description: String,
}

#[component]
fn TeamItem(props: TeamItemProps) -> Element {
    rsx! {
        div { class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div {
                h4 { class: "font-medium text-gray-900 dark:text-white", "{props.name}" }
                p { class: "text-sm text-gray-500", "{props.description}" }
            }
            div { class: "flex items-center space-x-4",
                span { class: "text-sm text-gray-500", "{props.members} members" }
                button { class: "text-blue-600 hover:text-blue-500 text-sm", "Edit" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct QueueItemProps {
    name: String,
    team: String,
    open_tickets: u32,
}

#[component]
fn QueueItem(props: QueueItemProps) -> Element {
    rsx! {
        div { class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div {
                h4 { class: "font-medium text-gray-900 dark:text-white", "{props.name}" }
                p { class: "text-sm text-gray-500", "Assigned to: {props.team}" }
            }
            div { class: "flex items-center space-x-4",
                Badge { variant: BadgeVariant::Blue, "{props.open_tickets} open" }
                button { class: "text-blue-600 hover:text-blue-500 text-sm", "Edit" }
            }
        }
    }
}

/// Notification settings page
#[component]
pub fn NotificationSettingsPage() -> Element {
    rsx! {
        AppLayout { title: "Notification Settings",
            PageHeader {
                title: "Notifications",
                subtitle: "Configure notification channels and templates",
            }

            div { class: "space-y-6",
                Card { title: "Notification Channels",
                    div { class: "space-y-4",
                        ChannelItem { name: "Email (SMTP)", status: "Connected", is_primary: true }
                        ChannelItem { name: "Slack", status: "Connected", is_primary: false }
                        ChannelItem { name: "Microsoft Teams", status: "Not configured", is_primary: false }
                        ChannelItem { name: "SMS (Twilio)", status: "Not configured", is_primary: false }
                    }
                }

                Card { title: "Email Templates",
                    div { class: "space-y-3",
                        a { class: "block text-blue-600 hover:text-blue-500 text-sm", "New Ticket Notification" }
                        a { class: "block text-blue-600 hover:text-blue-500 text-sm", "Ticket Updated Notification" }
                        a { class: "block text-blue-600 hover:text-blue-500 text-sm", "SLA Warning" }
                        a { class: "block text-blue-600 hover:text-blue-500 text-sm", "Invoice Sent" }
                        a { class: "block text-blue-600 hover:text-blue-500 text-sm", "Password Reset" }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ChannelItemProps {
    name: String,
    status: String,
    is_primary: bool,
}

#[component]
fn ChannelItem(props: ChannelItemProps) -> Element {
    let status_variant = if props.status == "Connected" {
        BadgeVariant::Green
    } else {
        BadgeVariant::Gray
    };

    rsx! {
        div { class: "flex items-center justify-between p-4 bg-gray-50 dark:bg-gray-800 rounded-lg",
            div { class: "flex items-center",
                span { class: "font-medium text-gray-900 dark:text-white", "{props.name}" }
                if props.is_primary {
                    Badge { variant: BadgeVariant::Blue, class: "ml-2", "Primary" }
                }
            }
            div { class: "flex items-center space-x-4",
                Badge { variant: status_variant, "{props.status}" }
                button { class: "text-blue-600 hover:text-blue-500 text-sm", "Configure" }
            }
        }
    }
}

/// Integration settings page
#[component]
pub fn IntegrationSettingsPage() -> Element {
    rsx! {
        AppLayout { title: "Integrations",
            PageHeader {
                title: "Integrations",
                subtitle: "Connect third-party services",
            }

            div { class: "grid grid-cols-1 md:grid-cols-2 gap-6",
                IntegrationCard {
                    name: "Tactical RMM",
                    description: "Remote monitoring and management",
                    status: "Connected",
                    category: "RMM",
                }
                IntegrationCard {
                    name: "Stripe",
                    description: "Payment processing",
                    status: "Connected",
                    category: "Payments",
                }
                IntegrationCard {
                    name: "Microsoft 365",
                    description: "Email and calendar sync",
                    status: "Not connected",
                    category: "Email",
                }
                IntegrationCard {
                    name: "QuickBooks",
                    description: "Accounting integration",
                    status: "Not connected",
                    category: "Accounting",
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct IntegrationCardProps {
    name: String,
    description: String,
    status: String,
    category: String,
}

#[component]
fn IntegrationCard(props: IntegrationCardProps) -> Element {
    let is_connected = props.status == "Connected";

    rsx! {
        Card {
            div { class: "flex items-start justify-between",
                div {
                    div { class: "flex items-center",
                        h3 { class: "text-lg font-medium text-gray-900 dark:text-white", "{props.name}" }
                        Badge {
                            variant: if is_connected { BadgeVariant::Green } else { BadgeVariant::Gray },
                            class: "ml-2",
                            "{props.status}"
                        }
                    }
                    p { class: "text-sm text-gray-500 dark:text-gray-400 mt-1", "{props.description}" }
                    span { class: "text-xs text-gray-400 mt-2 inline-block", "{props.category}" }
                }
                Button {
                    variant: if is_connected { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                    if is_connected { "Configure" } else { "Connect" }
                }
            }
        }
    }
}

/// Billing settings page
#[component]
pub fn BillingSettingsPage() -> Element {
    rsx! {
        AppLayout { title: "Billing Settings",
            PageHeader {
                title: "Billing & Rate Cards",
                subtitle: "Configure billing rates and settings",
            }

            div { class: "space-y-6",
                Card { title: "Default Rates",
                    div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                        RateCard { label: "Standard Rate", value: "$150/hr" }
                        RateCard { label: "After Hours", value: "$225/hr" }
                        RateCard { label: "Emergency", value: "$300/hr" }
                    }
                }

                Card { title: "Tax Settings",
                    div { class: "space-y-4",
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-gray-700 dark:text-gray-300", "Default Tax Rate" }
                            span { class: "font-medium", "0%" }
                        }
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-gray-700 dark:text-gray-300", "Tax Label" }
                            span { class: "font-medium", "Tax" }
                        }
                    }
                }

                Card { title: "Invoice Settings",
                    div { class: "space-y-4",
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-gray-700 dark:text-gray-300", "Default Payment Terms" }
                            span { class: "font-medium", "Net 30" }
                        }
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-gray-700 dark:text-gray-300", "Invoice Prefix" }
                            span { class: "font-mono", "INV-" }
                        }
                        div { class: "flex items-center justify-between",
                            span { class: "text-sm text-gray-700 dark:text-gray-300", "Next Invoice Number" }
                            span { class: "font-mono", "2025-004" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RateCardProps {
    label: String,
    value: String,
}

#[component]
fn RateCard(props: RateCardProps) -> Element {
    rsx! {
        div { class: "p-4 bg-gray-50 dark:bg-gray-800 rounded-lg text-center",
            p { class: "text-sm text-gray-500 dark:text-gray-400", "{props.label}" }
            p { class: "text-2xl font-bold text-gray-900 dark:text-white mt-1", "{props.value}" }
        }
    }
}

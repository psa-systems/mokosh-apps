//! Admin pages (multi-tenant only)

use dioxus::prelude::*;
#[cfg(feature = "multi-tenant")]
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, DataTable, Input, Modal,
    PageHeader, StatCard, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader,
    TableLoading, TableRow,
};

/// Subset of mokosh-server's `TenantResponse` we render in the admin
/// tenant table. Serde drops unknown fields so adding columns later
/// just means extending this struct.
#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RemoteTenant {
    id: uuid::Uuid,
    name: String,
    slug: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    subscription_plan: Option<String>,
    /// MAPPS-396: branding lives on the tenant row so the edit modal
    /// can pre-populate its logo / color / support-email fields
    /// without a second round-trip.
    #[serde(default)]
    branding: TenantBrandingWire,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// MAPPS-396: mirror of mokosh-server's `TenantBranding` for the fields
/// this admin surface reads and writes. Both derived so the same struct
/// is used on the wire in both directions. `Default` lets serde omit
/// the field on a legacy row and still decode.
#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct TenantBrandingWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    favicon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secondary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    company_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    support_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    support_phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    portal_domain: Option<String>,
}

#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Deserialize)]
struct PaginatedTenants {
    data: Vec<RemoteTenant>,
}

#[cfg(feature = "multi-tenant")]
#[derive(Clone, Copy, Debug, PartialEq)]
enum TenantSource {
    Backend,
    Demo,
}

#[cfg(feature = "multi-tenant")]
fn humanize_plan(raw: &Option<String>) -> String {
    match raw.as_deref() {
        None | Some("") => "Trial".into(),
        Some("trial") => "Trial".into(),
        Some("professional") => "Professional".into(),
        Some("enterprise") => "Enterprise".into(),
        Some(other) => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
fn humanize_tenant_status(raw: &str) -> String {
    match raw {
        "active" => "Active".into(),
        "suspended" => "Suspended".into(),
        "cancelled" => "Cancelled".into(),
        "" => "Active".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
fn format_created(when: chrono::DateTime<chrono::Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(when, Some(fmt)),
        None => when.format("%b %-d, %Y").to_string(),
    }
}

/// Tenant management page (multi-tenant mode only)
#[cfg(feature = "multi-tenant")]
#[component]
pub fn TenantManagementPage() -> Element {
    use_page_title("Tenant Management");
    // MAPPS-396: super-admin create + edit surface. `show_create`
    // toggles the create modal; `edit_target` carries the row being
    // edited (`None` = no modal). Both modals refresh the resource
    // on save via `on_saved` so the table + stat cards re-render
    // with the new state.
    let mut show_create = use_signal(|| false);
    let mut edit_target: Signal<Option<RemoteTenant>> = use_signal(|| None);
    // Try the live tenants endpoint first; fall back to seeded demo
    // rows so the page stays demoable for envs without a live backend.
    let mut tenants_resource = use_resource(|| async {
        // F1: re-fetch on org switch / token swap so the roster reflects
        // the active scope instead of the prior tenant's cached rows.
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-351: also refetch on reconnect so the roster drops the demo
        // fallback once the real backend answers again.
        let _reachable = crate::hooks::use_server_reachable();
        let token = match crate::hooks::fetch::api::current_access_token() {
            Some(t) => t,
            None => return (Vec::<RemoteTenant>::new(), TenantSource::Demo),
        };
        match crate::hooks::fetch::api::get_with_auth::<PaginatedTenants>("/tenants", &token).await
        {
            Ok(page) => (page.data, TenantSource::Backend),
            Err(_) => (Vec::new(), TenantSource::Demo),
        }
    });

    let resource_snapshot = tenants_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let (remote_tenants, source) = match &*resource_snapshot {
        Some((rows, source)) => (rows.clone(), *source),
        None => (Vec::new(), TenantSource::Demo),
    };

    // MAPPS-351: this page intentionally falls back to demo rows when the
    // backend errors, so it stays demoable. But when the server is flagged
    // DOWN, those demo rows would masquerade as real data during an outage -
    // show the honest unavailable state instead. Gated on the reachability
    // flag so a reachable no-token / 4xx keeps the demo fallback. Clears on
    // reconnect (the resource subscribes to reachability above).
    if !crate::hooks::use_server_reachable() && source == TenantSource::Demo && !is_loading {
        return rsx! {
            crate::components::ContentUnavailable { title: "Tenant Management".to_string() }
        };
    }

    // Stat-card counts are derived from the same data the table renders,
    // so they stay in sync with the roster instead of the old hardcoded
    // 42/38/4 literals. In demo mode they match the four seeded rows
    // below. MRR has no source field on the tenants API (per-row MRR is
    // "-" too), so it is shown as "-" rather than a fabricated dollar
    // figure.
    let (total_tenants, active_count, trial_count) = if source == TenantSource::Backend {
        (
            remote_tenants.len(),
            remote_tenants
                .iter()
                .filter(|t| humanize_tenant_status(&t.status) == "Active")
                .count(),
            remote_tenants
                .iter()
                .filter(|t| humanize_plan(&t.subscription_plan) == "Trial")
                .count(),
        )
    } else {
        (4, 3, 1)
    };
    let stat = |n: usize| -> String {
        if is_loading {
            "-".to_string()
        } else {
            n.to_string()
        }
    };
    let total_tenants_label = stat(total_tenants);
    let active_label = stat(active_count);
    let trial_label = stat(trial_count);
    // MAPPS-357: block create / edit writes while the server is
    // unreachable. Reactive: re-enables on reconnect.
    let can_mutate = crate::hooks::use_can_mutate();

    rsx! {
        PageHeader {
            title: "Tenant Management",
            subtitle: "Manage tenants and subscriptions",
            // MAPPS-396: super-admin add-tenant flow. Server side is
            // `POST /api/v1/tenants` (RequireSuperAdmin) which now
            // accepts optional branding on the initial insert.
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't create tenants while the server is unreachable".to_string()),
                    onclick: move |_| show_create.set(true),
                    "Create tenant"
                }
            },
        }

        // Stats
        div { class: "grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-6",
            StatCard { label: "Total Tenants", value: "{total_tenants_label}" }
            StatCard { label: "Active", value: "{active_label}" }
            StatCard { label: "Trial", value: "{trial_label}" }
            StatCard { label: "MRR", value: "-" }
        }

        if source == TenantSource::Demo && !is_loading {
            div {
                class: "mb-3 text-xs text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                "Backend tenants API not reachable - showing demo rows."
            }
        }

        DataTable {
            loading: is_loading,
            total_items: if source == TenantSource::Backend { remote_tenants.len() } else { 4 },
            current_page: 1,
            per_page: 25,
            columns: 7,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { sortable: true, "Tenant" }
                        TableHeader { "Plan" }
                        TableHeader { sortable: true, "Users" }
                        TableHeader { "MRR" }
                        TableHeader { "Status" }
                        TableHeader { sortable: true, "Created" }
                        // MAPPS-396: per-row edit affordance
                        TableHeader { "" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 7, rows: 4 }
                } else if source == TenantSource::Backend && remote_tenants.is_empty() {
                    TableEmpty {
                        columns: 7,
                        title: "No tenants yet".to_string(),
                        description: "Tenants will appear here once they sign up or are provisioned.".to_string(),
                    }
                } else {
                    TableBody {
                        if source == TenantSource::Backend {
                            for tenant in remote_tenants.iter().cloned() {
                                TenantRow {
                                    key: "{tenant.id}",
                                    name: tenant.name.clone(),
                                    domain: tenant.slug.clone(),
                                    plan: humanize_plan(&tenant.subscription_plan),
                                    users: 0,
                                    mrr: "-".to_string(),
                                    status: humanize_tenant_status(&tenant.status),
                                    created: format_created(tenant.created_at),
                                    logo_url: tenant.branding.logo_url.clone(),
                                    editable: true,
                                    on_edit: {
                                        let row = tenant.clone();
                                        move |_| edit_target.set(Some(row.clone()))
                                    },
                                }
                            }
                        } else {
                            TenantRow {
                                name: "Acme MSP",
                                domain: "acme-msp",
                                plan: "Professional",
                                users: 8,
                                mrr: "$299",
                                status: "Active",
                                created: "Jan 15, 2024",
                            }
                            TenantRow {
                                name: "TechPro Services",
                                domain: "techpro",
                                plan: "Enterprise",
                                users: 25,
                                mrr: "$599",
                                status: "Active",
                                created: "Mar 1, 2024",
                            }
                            TenantRow {
                                name: "IT Solutions Co",
                                domain: "itsolutions",
                                plan: "Professional",
                                users: 5,
                                mrr: "$299",
                                status: "Active",
                                created: "Jun 15, 2024",
                            }
                            TenantRow {
                                name: "New MSP Trial",
                                domain: "newmsp-trial",
                                plan: "Trial",
                                users: 2,
                                mrr: "$0",
                                status: "Trial",
                                created: "Jan 10, 2025",
                            }
                        }
                    }
                }
            }
        }

        // MAPPS-396: create + edit modals. Rendered outside the
        // DataTable so they overlay everything. `show_create` /
        // `edit_target` gate their visibility; both `onsaved`
        // restart the resource so the new / edited row appears
        // without a manual refresh.
        if show_create() {
            CreateTenantModal {
                onclose: move |_| show_create.set(false),
                onsaved: move |_| {
                    show_create.set(false);
                    tenants_resource.restart();
                },
            }
        }
        if let Some(target) = edit_target() {
            EditTenantModal {
                tenant: target,
                onclose: move |_| edit_target.set(None),
                onsaved: move |_| {
                    edit_target.set(None);
                    tenants_resource.restart();
                },
            }
        }
    }
}

#[cfg(feature = "multi-tenant")]
#[derive(Props, Clone, PartialEq)]
struct TenantRowProps {
    name: String,
    domain: String,
    plan: String,
    users: u32,
    mrr: String,
    status: String,
    created: String,
    /// MAPPS-396: when set, rendered as a small avatar next to the
    /// name so the roster shows the MSP's brand at a glance rather
    /// than a plain text row.
    #[props(default)]
    logo_url: Option<String>,
    /// MAPPS-396: demo rows have no id so their edit cell renders as
    /// a disabled placeholder; live backend rows get the "Edit" link
    /// that opens the tenant modal.
    #[props(default)]
    editable: bool,
    #[props(default)]
    on_edit: Option<EventHandler<()>>,
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TenantRow(props: TenantRowProps) -> Element {
    let status_variant = match props.status.as_str() {
        "Active" => BadgeVariant::Green,
        "Trial" => BadgeVariant::Blue,
        "Suspended" => BadgeVariant::Red,
        _ => BadgeVariant::Gray,
    };

    let plan_variant = match props.plan.as_str() {
        "Enterprise" => BadgeVariant::Purple,
        "Professional" => BadgeVariant::Blue,
        "Trial" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    };

    rsx! {
        TableRow {
            TableCell {
                div { class: "flex items-center gap-3",
                    // MAPPS-396: tenant avatar. When the branding
                    // has a logo we render it; the fallback is a
                    // circle initial so the row height stays even.
                    if let Some(ref url) = props.logo_url {
                        if !url.is_empty() {
                            img {
                                src: "{url}",
                                alt: "{props.name} logo",
                                class: "h-8 w-8 rounded-full object-cover bg-surface border border-line",
                            }
                        } else {
                            {tenant_initial(&props.name)}
                        }
                    } else {
                        {tenant_initial(&props.name)}
                    }
                    div {
                        span { class: "font-medium text-content", "{props.name}" }
                        // Render the tenant's real client portal URL, not
                        // a hardcoded `{slug}.mokosh.app` that was never
                        // the actual portal host. Falls back to just the
                        // slug on a legacy deploy that has no
                        // PORTAL_HOST_SUFFIX configured (nothing to link
                        // to; a bare slug still identifies the tenant).
                        {
                            let portal_url = crate::modules::runtime_config::portal_url_for_slug(&props.domain);
                            match portal_url {
                                Some(url) => rsx! {
                                    a {
                                        class: "text-sm text-muted hover:text-accent break-all",
                                        href: "{url}",
                                        target: "_blank",
                                        rel: "noopener",
                                        "{url}"
                                    }
                                },
                                None => rsx! {
                                    p { class: "text-sm text-muted font-mono", "{props.domain}" }
                                },
                            }
                        }
                    }
                }
            }
            TableCell { Badge { variant: plan_variant, "{props.plan}" } }
            TableCell { "{props.users}" }
            TableCell { class: "font-medium", "{props.mrr}" }
            TableCell { Badge { variant: status_variant, "{props.status}" } }
            TableCell { class: "text-muted", "{props.created}" }
            TableCell { class: "text-right",
                if props.editable {
                    button {
                        r#type: "button",
                        class: "text-sm text-accent hover:opacity-80",
                        onclick: move |_| {
                            if let Some(ref h) = props.on_edit {
                                h.call(());
                            }
                        },
                        "Edit"
                    }
                } else {
                    span { class: "text-xs text-muted", "-" }
                }
            }
        }
    }
}

/// MAPPS-396: fallback avatar for tenants without a logo. Renders the
/// first character of the tenant name in a colored circle so the row
/// height stays even regardless of branding state.
#[cfg(feature = "multi-tenant")]
fn tenant_initial(name: &str) -> Element {
    let ch = name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    rsx! {
        span { class: "h-8 w-8 rounded-full bg-accent/10 text-accent flex items-center justify-center text-xs font-semibold border border-line",
            "{ch}"
        }
    }
}

// ============================================================================
// MAPPS-396: tenant create + edit modals
// ============================================================================

/// Request body for `POST /api/v1/tenants`. Mirrors mokosh-server's
/// `CreateTenantRequest` for the fields this modal collects. Branding
/// is optional; omitted (`None`) lands the tenant with the empty-object
/// default.
#[cfg(feature = "multi-tenant")]
#[derive(Serialize)]
struct CreateTenantBody {
    name: String,
    slug: String,
    admin_email: String,
    admin_first_name: String,
    admin_last_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    billing_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branding: Option<TenantBrandingWire>,
}

/// Request body for `PUT /api/v1/tenants/{id}`. Mirrors the subset of
/// `UpdateTenantRequest` this modal edits: display name and branding.
#[cfg(feature = "multi-tenant")]
#[derive(Serialize)]
struct UpdateTenantBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// MAPPS-449: renames the tenant's portal subdomain. Server
    /// slugifies + re-checks uniqueness. Old slug immediately 404s.
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branding: Option<TenantBrandingWire>,
}

/// MAPPS-450: mirror of `mokosh_types::tenants::TenantAdminInfo`. Read
/// via `GET /tenants/{id}/admin` when the modal opens so the SPA knows
/// whether the email input should be editable (pending) or disabled
/// (active).
#[cfg(feature = "multi-tenant")]
#[derive(Clone, Debug, Deserialize)]
struct TenantAdminInfo {
    #[allow(dead_code)]
    user_id: uuid::Uuid,
    email: String,
    first_name: String,
    last_name: String,
    /// Raw `users.status` column. `pending` allows email edits + welcome
    /// resends; anything else disables the email input and hides the
    /// resend checkbox.
    status: String,
}

/// MAPPS-450: PUT body for `/tenants/{id}/admin`. Every field is optional
/// so the SPA sends only what changed; `resend_welcome` piggybacks on the
/// same call so an email-change common case is a single round-trip.
#[cfg(feature = "multi-tenant")]
#[derive(Serialize)]
struct UpdateTenantAdminBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_name: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    resend_welcome: bool,
}

/// Normalize a text field: trim + return `None` on empty so the request
/// omits the field rather than sending an empty string the server
/// would either persist as an empty column or reject at validation.
#[cfg(feature = "multi-tenant")]
fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Collect the four branding text inputs into a `TenantBrandingWire`.
/// Returns `None` when every field is blank so the wire omits the
/// object entirely (matches "leave branding as-is / default").
#[cfg(feature = "multi-tenant")]
fn collect_branding(
    logo_url: &str,
    primary_color: &str,
    support_email: &str,
    favicon_url: &str,
) -> Option<TenantBrandingWire> {
    let logo = opt(logo_url);
    let color = opt(primary_color);
    let support = opt(support_email);
    let favicon = opt(favicon_url);
    if logo.is_none() && color.is_none() && support.is_none() && favicon.is_none() {
        return None;
    }
    Some(TenantBrandingWire {
        logo_url: logo,
        favicon_url: favicon,
        primary_color: color,
        secondary_color: None,
        company_name: None,
        support_email: support,
        support_phone: None,
        portal_domain: None,
    })
}

#[cfg(feature = "multi-tenant")]
#[component]
fn CreateTenantModal(onclose: EventHandler<()>, onsaved: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut admin_email = use_signal(String::new);
    let mut admin_first = use_signal(String::new);
    let mut admin_last = use_signal(String::new);
    let mut billing_email = use_signal(String::new);
    let mut logo_url = use_signal(String::new);
    let mut primary_color = use_signal(String::new);
    let mut support_email = use_signal(String::new);
    let mut favicon_url = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Post-create success dialog: renders once the server returns the
    // fresh tenant id, before the list refreshes and the modal closes.
    // Surfaces the portal URL the super-admin needs to give the new
    // tenant's admin (also stated in the admin welcome email, but the
    // super-admin sees it here immediately without having to ask
    // their tenant to check their inbox).
    let mut created_slug = use_signal(String::new);
    let mut created_admin_email = use_signal(String::new);

    let submit = move |_| {
        if saving() {
            return;
        }
        let n = name.read().trim().to_string();
        let s = slug.read().trim().to_ascii_lowercase();
        let ae = admin_email.read().trim().to_string();
        if n.is_empty() || s.is_empty() || ae.is_empty() {
            error.set("Tenant name, slug and admin email are required.".to_string());
            return;
        }
        let body = CreateTenantBody {
            name: n,
            slug: s.clone(),
            admin_email: ae.clone(),
            admin_first_name: admin_first.read().trim().to_string(),
            admin_last_name: admin_last.read().trim().to_string(),
            billing_email: opt(&billing_email.read()),
            branding: collect_branding(
                &logo_url.read(),
                &primary_color.read(),
                &support_email.read(),
                &favicon_url.read(),
            ),
        };
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                #[derive(serde::Deserialize)]
                struct TenantId {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                match crate::hooks::fetch::api::post_authed_typed::<TenantId, _>("/tenants", &body)
                    .await
                {
                    Ok(_) => {
                        created_slug.set(s);
                        created_admin_email.set(ae);
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

    // While the customer is typing the slug, show a live preview of
    // the portal URL. Returns None on a legacy deploy that has no
    // configured portal host suffix; the modal then hides the hint
    // block entirely rather than promise a URL that would 404.
    let slug_preview = slug.read().trim().to_ascii_lowercase();
    let portal_preview = crate::modules::runtime_config::portal_url_for_slug(&slug_preview);

    // Post-create success view. Renders once the server has returned
    // the fresh tenant id; the create form is hidden so the super-
    // admin sees the portal URL prominently and can copy it before
    // dismissing.
    let created_slug_str = created_slug.read().clone();
    let is_created = !created_slug_str.is_empty();
    let created_portal_url = crate::modules::runtime_config::portal_url_for_slug(&created_slug_str);
    let created_admin_email_str = created_admin_email.read().clone();

    let modal_title = if is_created {
        "Tenant created".to_string()
    } else {
        "Create tenant".to_string()
    };
    let modal_footer = if is_created {
        rsx! {
            Button {
                variant: ButtonVariant::Primary,
                onclick: move |_| onsaved.call(()),
                "Done"
            }
        }
    } else {
        rsx! {
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| {
                    if !saving() {
                        onclose.call(());
                    }
                },
                "Cancel"
            }
            Button {
                variant: ButtonVariant::Primary,
                loading: saving(),
                onclick: submit,
                "Create"
            }
        }
    };

    rsx! {
        Modal {
            open: true,
            title: modal_title,
            onclose: move |_| {
                if !saving() {
                    if is_created {
                        onsaved.call(());
                    } else {
                        onclose.call(());
                    }
                }
            },
            footer: modal_footer,
            if is_created {
                div { class: "space-y-4",
                    p { class: "text-sm text-content",
                        "The tenant is provisioned and its admin has been emailed a setup link."
                    }
                    if let Some(url) = created_portal_url.as_ref() {
                        div { class: "rounded-md border border-line bg-surface-2 p-3",
                            p { class: "text-xs uppercase text-muted mb-1", "Client portal URL" }
                            p { class: "font-mono text-sm text-content break-all", "{url}" }
                            p { class: "mt-2 text-xs text-muted",
                                "Send this to the tenant's admin. Their clients will sign in here with the accounts the admin creates."
                            }
                        }
                    }
                    if !created_admin_email_str.is_empty() {
                        div { class: "rounded-md border border-line bg-surface-2 p-3",
                            p { class: "text-xs uppercase text-muted mb-1", "Admin welcome sent to" }
                            p { class: "font-mono text-sm text-content break-all", "{created_admin_email_str}" }
                            p { class: "mt-2 text-xs text-muted",
                                "The admin has 7 days to open the emailed link and set their password. If they miss the window, they can use Forgot password."
                            }
                        }
                    }
                }
            } else { div { class: "space-y-3",
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                Input {
                    name: "tenant_name",
                    label: "Tenant name",
                    r#type: "text".to_string(),
                    value: name(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); name.set(e.value()); },
                }
                Input {
                    name: "tenant_slug",
                    label: "Slug (e.g. acme; used in the portal URL)",
                    r#type: "text".to_string(),
                    value: slug(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); slug.set(e.value()); },
                }
                // Live preview of the portal URL the new tenant will
                // inherit. Hidden entirely on a legacy deploy that
                // has no configured PORTAL_HOST_SUFFIX so the modal
                // never promises a URL that would 404.
                if let Some(url) = portal_preview.as_ref() {
                    p { class: "-mt-2 text-xs text-muted",
                        "Client portal will be at "
                        span { class: "font-mono text-content", "{url}" }
                    }
                } else if !slug_preview.is_empty() {
                    p { class: "-mt-2 text-xs text-muted",
                        "The client portal URL cannot be previewed on this deploy (PORTAL_HOST_SUFFIX not configured)."
                    }
                }
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                    Input {
                        name: "admin_first",
                        label: "Admin first name",
                        r#type: "text".to_string(),
                        value: admin_first(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { admin_first.set(e.value()); },
                    }
                    Input {
                        name: "admin_last",
                        label: "Admin last name",
                        r#type: "text".to_string(),
                        value: admin_last(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { admin_last.set(e.value()); },
                    }
                }
                Input {
                    name: "admin_email",
                    label: "Admin email",
                    r#type: "email".to_string(),
                    value: admin_email(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); admin_email.set(e.value()); },
                }
                Input {
                    name: "billing_email",
                    label: "Billing email (optional)",
                    r#type: "email".to_string(),
                    value: billing_email(),
                    disabled: saving(),
                    oninput: move |e: FormEvent| { billing_email.set(e.value()); },
                }
                div { class: "border-t border-line pt-3",
                    p { class: "text-sm font-medium text-content mb-2", "Branding (optional)" }
                    Input {
                        name: "logo_url",
                        label: "Logo URL",
                        r#type: "url".to_string(),
                        value: logo_url(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { logo_url.set(e.value()); },
                    }
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3 mt-2",
                        Input {
                            name: "primary_color",
                            label: "Primary color (#hex)",
                            r#type: "text".to_string(),
                            value: primary_color(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| { primary_color.set(e.value()); },
                        }
                        Input {
                            name: "support_email",
                            label: "Support email",
                            r#type: "email".to_string(),
                            value: support_email(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| { support_email.set(e.value()); },
                        }
                    }
                    Input {
                        name: "favicon_url",
                        label: "Favicon URL",
                        r#type: "url".to_string(),
                        value: favicon_url(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { favicon_url.set(e.value()); },
                    }
                }
            } }
        }
    }
}

#[cfg(feature = "multi-tenant")]
#[component]
fn EditTenantModal(
    tenant: RemoteTenant,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let tenant_id = tenant.id;
    let mut name = use_signal(|| tenant.name.clone());
    // MAPPS-449: the super-admin can rename the tenant's portal subdomain.
    // Original slug pinned so the submit can detect a change and surface
    // the "old URL now 404s" warning banner only when it actually flipped.
    let original_slug = tenant.slug.clone();
    let mut slug = use_signal(|| tenant.slug.clone());
    let mut slug_changed_warning = use_signal(|| false);
    let mut logo_url = use_signal(|| tenant.branding.logo_url.clone().unwrap_or_default());
    let mut primary_color =
        use_signal(|| tenant.branding.primary_color.clone().unwrap_or_default());
    let mut support_email =
        use_signal(|| tenant.branding.support_email.clone().unwrap_or_default());
    let mut favicon_url = use_signal(|| tenant.branding.favicon_url.clone().unwrap_or_default());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    // MAPPS-450: super-admin edits the tenant admin's email + name pair.
    // Fields hydrate from a `GET /tenants/{id}/admin` fetch when the
    // modal opens; before hydration the inputs render disabled so a
    // save mid-load cannot overwrite the loaded values with blanks.
    let mut admin_email = use_signal(String::new);
    let mut admin_first_name = use_signal(String::new);
    let mut admin_last_name = use_signal(String::new);
    // Snapshot of the loaded row: original strings for change-detection
    // and status for the email-editable gate.
    let mut admin_snapshot: Signal<Option<TenantAdminInfo>> = use_signal(|| None);
    // MAPPS-450: default the "re-send welcome" checkbox to ON when the
    // email is dirty. Rendered only when admin is `pending` AND email
    // changed, so a stale ON on a name-only edit does nothing.
    let mut admin_resend_on_save = use_signal(|| true);
    #[cfg(feature = "web")]
    use_hook(move || {
        spawn(async move {
            let path = format!("/tenants/{tenant_id}/admin");
            if let Ok(info) =
                crate::hooks::fetch::api::get_authed_typed::<TenantAdminInfo>(&path).await
            {
                admin_email.set(info.email.clone());
                admin_first_name.set(info.first_name.clone());
                admin_last_name.set(info.last_name.clone());
                admin_snapshot.set(Some(info));
            }
        });
    });
    // MAPPS-448: super-admin re-issues the tenant admin's setup email
    // when the original went missing or the setup link expired. Server
    // 409s when the admin has already redeemed - the error surfaces
    // via a toast so a stale button click is self-explaining, not
    // silent.
    let mut resending = use_signal(|| false);
    let resend_welcome = move |_| {
        if resending() || saving() {
            return;
        }
        resending.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/tenants/{tenant_id}/admin/resend-welcome");
                match crate::hooks::fetch::api::post_authed_no_content(&path).await {
                    Ok(_) => crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        "Welcome email re-sent to the tenant admin.",
                    ),
                    Err(msg) => crate::hooks::toast::push_toast(
                        crate::components::AlertType::Warning,
                        msg,
                    ),
                }
            }
            resending.set(false);
        });
    };

    let submit = move |_| {
        if saving() {
            return;
        }
        let n = name.read().trim().to_string();
        if n.is_empty() {
            error.set("Tenant name is required.".to_string());
            return;
        }
        // MAPPS-449: only send `slug` when the operator actually changed
        // it (trim + lowercase mirror server-side slugify's rough shape).
        // Sending the unchanged slug would still succeed server-side
        // (idempotent probe), but omitting it keeps the request payload
        // honest and avoids a spurious "old URL 404s" banner.
        let s_raw = slug.read().trim().to_string();
        if s_raw.is_empty() {
            error.set("Slug is required.".to_string());
            return;
        }
        let s_changed = s_raw.to_ascii_lowercase() != original_slug.to_ascii_lowercase();
        let body = UpdateTenantBody {
            name: Some(n),
            slug: if s_changed { Some(s_raw) } else { None },
            branding: collect_branding(
                &logo_url.read(),
                &primary_color.read(),
                &support_email.read(),
                &favicon_url.read(),
            ),
        };
        saving.set(true);
        error.set(String::new());
        // MAPPS-450: diff the admin fields against the loaded snapshot
        // and build a PATCH body of ONLY the changed fields. The whole
        // block short-circuits when the snapshot never hydrated (offline
        // load) or when nothing differs, so a branding-only edit stays a
        // single request.
        let admin_body = {
            let snap = admin_snapshot.read().clone();
            snap.and_then(|s| {
                let email_new = admin_email.read().trim().to_string();
                let first_new = admin_first_name.read().trim().to_string();
                let last_new = admin_last_name.read().trim().to_string();
                let email_changed = !email_new.eq_ignore_ascii_case(&s.email)
                    && !email_new.is_empty();
                let first_changed = first_new != s.first_name.trim();
                let last_changed = last_new != s.last_name.trim();
                if !email_changed && !first_changed && !last_changed {
                    None
                } else {
                    Some((
                        UpdateTenantAdminBody {
                            email: email_changed.then_some(email_new),
                            first_name: first_changed.then_some(first_new),
                            last_name: last_changed.then_some(last_new),
                            resend_welcome: email_changed
                                && s.status == "pending"
                                && admin_resend_on_save(),
                        },
                        s.status,
                    ))
                }
            })
        };
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/tenants/{tenant_id}");
                #[derive(serde::Deserialize)]
                struct TenantId {
                    #[allow(dead_code)]
                    id: uuid::Uuid,
                }
                let tenant_ok = match crate::hooks::fetch::api::put_authed_typed::<TenantId, _>(
                    &path, &body,
                )
                .await
                {
                    Ok(_) => true,
                    Err(e) => {
                        error.set(e.user_message());
                        false
                    }
                };
                if tenant_ok {
                    // MAPPS-450: chase the tenant PUT with the admin PUT
                    // when any admin field is dirty. A 409 here (email
                    // change on already-activated admin) rewinds to the
                    // error banner; the tenant edit is not rolled back
                    // (they are separate scopes) but the modal stays open
                    // so the operator can undo the email edit.
                    let admin_ok = if let Some((abody, _status)) = admin_body.as_ref() {
                        let apath = format!("/tenants/{tenant_id}/admin");
                        match crate::hooks::fetch::api::put_authed_typed::<
                            TenantAdminInfo,
                            _,
                        >(&apath, abody)
                        .await
                        {
                            Ok(info) => {
                                admin_email.set(info.email.clone());
                                admin_first_name.set(info.first_name.clone());
                                admin_last_name.set(info.last_name.clone());
                                admin_snapshot.set(Some(info));
                                true
                            }
                            Err(e) => {
                                error.set(e.user_message());
                                false
                            }
                        }
                    } else {
                        true
                    };
                    if admin_ok {
                        // MAPPS-449: hoist the "old URL now 404s" warning
                        // banner before dismissing the modal via onsaved
                        // so the operator sees it. Skipped when the slug
                        // was not changed to keep no-op saves quiet.
                        if s_changed {
                            slug_changed_warning.set(true);
                        } else {
                            onsaved.call(());
                        }
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = &body;
                let _ = &admin_body;
            }
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: format!("Edit tenant: {}", tenant.name),
            onclose: move |_| {
                if !saving() {
                    onclose.call(());
                }
            },
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| {
                        if !saving() {
                            onclose.call(());
                        }
                    },
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
                    name: "tenant_name",
                    label: "Tenant name",
                    r#type: "text".to_string(),
                    value: name(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); name.set(e.value()); },
                }
                // MAPPS-449: slug (URL) is editable. Server slugifies +
                // re-checks uniqueness; a collision returns 409 and lands
                // in the `error` banner above. Live URL preview under the
                // input catches typos before the save round-trip.
                Input {
                    name: "tenant_slug",
                    label: "Slug (URL)",
                    r#type: "text".to_string(),
                    value: slug(),
                    required: true,
                    disabled: saving(),
                    oninput: move |e: FormEvent| { error.set(String::new()); slug.set(e.value()); },
                }
                {
                    let s = slug.read().trim().to_ascii_lowercase();
                    if s.is_empty() {
                        rsx! {}
                    } else if let Some(url) = crate::modules::runtime_config::portal_url_for_slug(&s) {
                        rsx! {
                            p { class: "text-xs text-muted",
                                "Portal URL: "
                                span { class: "font-mono text-content", "{url}" }
                            }
                        }
                    } else {
                        rsx! {
                            p { class: "text-xs text-muted",
                                "Portal URL cannot be previewed on this deploy (PORTAL_HOST_SUFFIX not configured)."
                            }
                        }
                    }
                }
                if slug_changed_warning() {
                    div { class: "rounded-md border border-amber-300 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/30 px-3 py-2 text-xs text-amber-800 dark:text-amber-200 space-y-2",
                        p {
                            "The tenant's portal URL has changed. The old URL now returns 404. Any pending portal-invite emails carrying the old slug will not work; re-send them from Contacts."
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                slug_changed_warning.set(false);
                                onsaved.call(());
                            },
                            "Got it"
                        }
                    }
                }
                div { class: "border-t border-line pt-3",
                    p { class: "text-sm font-medium text-content mb-2", "Branding" }
                    Input {
                        name: "logo_url",
                        label: "Logo URL",
                        r#type: "url".to_string(),
                        value: logo_url(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { logo_url.set(e.value()); },
                    }
                    // MAPPS-396: preview the pasted URL so the operator
                    // catches a broken link before saving.
                    if !logo_url.read().is_empty() {
                        div { class: "mt-2 flex items-center gap-2",
                            span { class: "text-xs text-muted", "Preview:" }
                            img {
                                src: "{logo_url()}",
                                alt: "logo preview",
                                class: "h-10 max-w-32 object-contain bg-surface border border-line rounded",
                            }
                        }
                    }
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3 mt-2",
                        Input {
                            name: "primary_color",
                            label: "Primary color (#hex)",
                            r#type: "text".to_string(),
                            value: primary_color(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| { primary_color.set(e.value()); },
                        }
                        Input {
                            name: "support_email",
                            label: "Support email",
                            r#type: "email".to_string(),
                            value: support_email(),
                            disabled: saving(),
                            oninput: move |e: FormEvent| { support_email.set(e.value()); },
                        }
                    }
                    Input {
                        name: "favicon_url",
                        label: "Favicon URL",
                        r#type: "url".to_string(),
                        value: favicon_url(),
                        disabled: saving(),
                        oninput: move |e: FormEvent| { favicon_url.set(e.value()); },
                    }
                }
                // MAPPS-450: tenant admin contact fields. Rendered only
                // once the admin fetch resolves so the operator does not
                // race a save against a mid-load blank. Email input is
                // disabled when admin.status != 'pending' with a tooltip
                // explaining why (mirrors the server-side 409 guard).
                {
                    let snap = admin_snapshot.read().clone();
                    let is_pending = snap.as_ref().map(|s| s.status == "pending").unwrap_or(false);
                    let email_dirty = snap
                        .as_ref()
                        .map(|s| !admin_email.read().trim().eq_ignore_ascii_case(&s.email))
                        .unwrap_or(false);
                    let email_disabled_reason = if snap.is_none() {
                        Some("Loading admin details…".to_string())
                    } else if !is_pending {
                        Some(
                            "This tenant admin has already activated their account; they must change their own email from Settings.".to_string(),
                        )
                    } else {
                        None
                    };
                    rsx! {
                        div { class: "border-t border-line pt-3 space-y-2",
                            p { class: "text-sm font-medium text-content", "Admin contact" }
                            if snap.is_none() {
                                p { class: "text-xs text-muted", "Loading admin details…" }
                            } else {
                                Input {
                                    name: "admin_email",
                                    label: "Admin email",
                                    r#type: "email".to_string(),
                                    value: admin_email(),
                                    disabled: saving() || email_disabled_reason.is_some(),
                                    oninput: move |e: FormEvent| { error.set(String::new()); admin_email.set(e.value()); },
                                }
                                if let Some(ref reason) = email_disabled_reason {
                                    p { class: "text-xs text-muted", "{reason}" }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                                    Input {
                                        name: "admin_first_name",
                                        label: "First name",
                                        r#type: "text".to_string(),
                                        value: admin_first_name(),
                                        disabled: saving(),
                                        oninput: move |e: FormEvent| { error.set(String::new()); admin_first_name.set(e.value()); },
                                    }
                                    Input {
                                        name: "admin_last_name",
                                        label: "Last name",
                                        r#type: "text".to_string(),
                                        value: admin_last_name(),
                                        disabled: saving(),
                                        oninput: move |e: FormEvent| { error.set(String::new()); admin_last_name.set(e.value()); },
                                    }
                                }
                                // Re-send welcome checkbox is only meaningful when
                                // the email is dirty AND the admin is still
                                // pending. Off-check the flag if the operator
                                // reverts the email so the state stays honest.
                                if email_dirty && is_pending {
                                    label { class: "flex items-center gap-2 text-xs text-muted",
                                        input {
                                            r#type: "checkbox",
                                            checked: admin_resend_on_save(),
                                            disabled: saving(),
                                            onchange: move |e: FormEvent| {
                                                admin_resend_on_save.set(e.value() == "true");
                                            },
                                        }
                                        "Re-send welcome email to the new address after saving."
                                    }
                                }
                            }
                        }
                    }
                }
                // MAPPS-448: super-admin re-issues the tenant admin's setup
                // email. Server rejects with 409 if the admin has already
                // redeemed (surfaced as a toast). Deliberately placed at the
                // bottom of the modal so the primary field edits sit above
                // the tenant-lifecycle actions.
                div { class: "border-t border-line pt-3",
                    p { class: "text-sm font-medium text-content mb-1",
                        "Admin welcome email"
                    }
                    p { class: "text-xs text-muted mb-2",
                        "Re-send the initial setup email if the tenant admin never received it or the setup link expired. The old link stops working after this."
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: resending(),
                        disabled: resending() || saving(),
                        onclick: resend_welcome,
                        "Resend welcome email"
                    }
                }
            }
        }
    }
}

// Fallback for single-tenant mode
#[cfg(not(feature = "multi-tenant"))]
#[component]
pub fn TenantManagementPage() -> Element {
    rsx! {
        div { "Tenant management is not available in single-tenant mode." }
    }
}

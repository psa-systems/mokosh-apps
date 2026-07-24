//! Centralized Settings hub (MAPPS-169).
//!
//! Type-2 "configuration" surfaces - the things you edit to shape how
//! the rest of the app behaves, as opposed to individual business
//! entities - are gathered behind a single left-nav "Settings" entry.
//!
//! `SettingsHomePage` is a grouped card index. Each card links to a
//! sub-route under `/settings/*`:
//!
//!   - Re-homed surfaces (SLA, rate cards, tax rates, payment gateways)
//!     render the SAME page components as their original routes, which
//!     stay in place (MAPPS-169 chose "keep old links + add Settings").
//!   - Net-new "standard type" editors (work types, task statuses, asset
//!     types) live here. They wire to the existing server CRUD endpoints
//!     (`/work-types`, `/task-statuses`, `/asset-types`) - no server
//!     change was needed.
//!
//! The ticket lookup editors (statuses, priorities, types, queues,
//! categories) live here too (MAPPS-172), wired to the management CRUD
//! that shipped in PMS-321. The project-type editor (MAPPS-173) wires to
//! the PMS-322 lookup table that replaced the old hardcoded enum; its
//! seeded `is_system` rows are name-locked and undeletable in the UI.
//!
//! CRUD conventions mirror `src/pages/billing.rs` (`TaxRateListPage`):
//! page-local `Deserialize` row structs, `active_tenant_generation()`
//! read inside each `use_resource` so an org switch re-fetches, and a
//! single create/edit modal driven by an `Option<FormState>` signal.

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, BreadcrumbItem, Breadcrumbs, Button, ButtonVariant, Card,
    Checkbox, DataTable, IconSize, Input, PageHeader, PlusIcon, SearchInput, Select, SelectOption,
    SettingFormModal, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader,
    TableLoading, TableRow, ThemePicker,
};
use crate::utils::money::format_money_str;
use crate::utils::Paginated;
use crate::Route;

/// Rows per page for the settings list views.
const PER_PAGE: usize = 25;

/// True when the signed-in user is an admin/super_admin. The Settings
/// surfaces configure tenant-wide behavior, so they match the same gate
/// the sidebar uses to show the Settings entry. Server endpoints re-check,
/// so this is a UX affordance, not a security boundary.
fn use_is_admin() -> bool {
    let auth = crate::hooks::use_auth();
    let auth_state = auth.read();
    auth_state
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false)
}

/// Shown in place of a settings page when a non-admin lands on one.
#[component]
fn AdminOnlyNotice(title: String) -> Element {
    use_page_title(title.clone());
    rsx! {
        PageHeader { title, subtitle: "Settings" }
        Card {
            div { class: "text-sm text-content",
                "You need an administrator role to manage these settings."
            }
        }
    }
}

// ============================================================================
// Settings hub
// ============================================================================

/// `/settings` - grouped index of every configuration surface.
/// Per-user appearance settings (MAPPS-259): base mode + accent color.
/// Not admin-gated; every user personalizes their own theme. Embeds the
/// same `ThemePicker` the top-bar swatch modal uses.
#[component]
pub fn AppearanceSettingsPage() -> Element {
    // MAPPS-357: N/A - per-user appearance (theme + accent) has no server-backed
    // resource; the ThemePicker persists to the account with no page fetch that
    // an outage could blank, so there is no unavailable state to render.
    use_page_title("Appearance");
    rsx! {
        PageHeader {
            title: "Appearance",
            subtitle: "Theme and accent color, saved to your account",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsAppearance {} }
            },
        }
        Card {
            div { class: "p-6",
                ThemePicker {}
            }
        }
    }
}

/// `/settings/tv-view` - per-user wall-monitor TV-view toggle (MAPPS-256).
/// Not admin-gated; every user personalizes their own view. Writes the
/// `mokosh_tv_view` boolean and the optional `mokosh_tv_team` team id via
/// `prefs.rs` (through the `tv_view` hook), mirroring the theme precedent.
#[component]
pub fn TvViewSettingsPage() -> Element {
    // MAPPS-357: N/A - TV-view prefs are browser-local (prefs.rs), not fetched
    // from the server, so this page has no remote resource an outage can blank.
    let mut enabled = use_signal(crate::hooks::tv_view::is_enabled);
    let mut team = use_signal(crate::hooks::tv_view::selected_team);

    use_page_title("TV View");
    rsx! {
        PageHeader {
            title: "TV View",
            subtitle: "Full-screen wall-monitor dashboard, scoped to your team, saved to this browser",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTvView {} }
            },
        }
        Card {
            div { class: "p-6 space-y-6 max-w-xl",
                Checkbox {
                    name: "tv_view_enabled",
                    label: "Enable TV view",
                    checked: enabled(),
                    help: "Shows a link into the full-screen dashboard from your normal dashboard.",
                    onchange: move |e: FormEvent| {
                        let on = e.checked();
                        enabled.set(on);
                        crate::hooks::tv_view::set_enabled(on);
                    },
                }

                div {
                    Input {
                        name: "tv_view_team",
                        label: "Team id (optional)",
                        value: team(),
                        placeholder: "Leave blank to show all your teams",
                        help: "Pin the board to a single team's tickets and KPIs. Blank uses the union of every team you belong to.",
                        oninput: move |e: FormEvent| {
                            let v = e.value();
                            team.set(v.clone());
                            crate::hooks::tv_view::set_selected_team(&v);
                        },
                    }
                }

                if enabled() {
                    Link {
                        to: Route::DashboardTv {},
                        class: "inline-flex items-center rounded-md bg-accent text-on-accent px-4 py-2 text-sm font-medium hover:opacity-90",
                        "Open TV view"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Settings taxonomy (MAPPS-258)
//
// One const table is the single source of truth for the settings
// information architecture: it drives the grouped index, the per-group
// landing pages, the breadcrumb trail on every sub-page, and the search
// filter. Adding a surface here gives it a card, a breadcrumb, and a search
// hit for free, with no per-page wiring to drift out of sync.
// ============================================================================

/// Top-level settings domain. Each admin group is reached through its own
/// landing route so the `/settings` index lists groups rather than all the
/// leaves at once (Windows/JetBrains-style nesting). Personalization is not
/// nested: its single surface (Appearance) shows directly on the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsGroupKey {
    Personalization,
    ServiceTypes,
    Billing,
    Tickets,
    Integrations,
    Data,
}

impl SettingsGroupKey {
    /// Heading on the index and the middle crumb in a leaf's breadcrumb.
    fn title(self) -> &'static str {
        match self {
            SettingsGroupKey::Personalization => "Personalization",
            SettingsGroupKey::ServiceTypes => "Service & Asset Types",
            SettingsGroupKey::Billing => "Billing & SLA",
            SettingsGroupKey::Tickets => "Tickets",
            SettingsGroupKey::Integrations => "Integrations",
            SettingsGroupKey::Data => "Data",
        }
    }

    /// One-line summary shown on the group card on the index.
    fn description(self) -> &'static str {
        match self {
            SettingsGroupKey::Personalization => "Theme and accent color for your account.",
            SettingsGroupKey::ServiceTypes => {
                "Work types, task statuses, asset types, and project types."
            }
            SettingsGroupKey::Billing => {
                "SLA, scheduling, time tracking, rate cards, taxes, gateways, and payment terms."
            }
            SettingsGroupKey::Tickets => "Statuses, priorities, types, queues, and categories.",
            SettingsGroupKey::Integrations => "RMM connections, device mappings, and alert rules.",
            SettingsGroupKey::Data => "Export and import this tenant's data.",
        }
    }

    /// Landing route for the group, or `None` for Personalization which is
    /// not nested (its surfaces render directly on the index).
    fn landing(self) -> Option<Route> {
        match self {
            SettingsGroupKey::Personalization => None,
            SettingsGroupKey::ServiceTypes => Some(Route::SettingsGroupServiceTypes {}),
            SettingsGroupKey::Billing => Some(Route::SettingsGroupBilling {}),
            SettingsGroupKey::Tickets => Some(Route::SettingsGroupTickets {}),
            SettingsGroupKey::Integrations => Some(Route::SettingsGroupIntegrations {}),
            SettingsGroupKey::Data => Some(Route::SettingsGroupData {}),
        }
    }

    /// Color identity tying the group to its left-nav area (MAPPS-257).
    /// Drives the group heading tint and each card's left-accent border.
    fn color(self) -> SectionColor {
        match self {
            SettingsGroupKey::Personalization => SectionColor::Rose,
            SettingsGroupKey::ServiceTypes => SectionColor::Emerald,
            SettingsGroupKey::Billing => SectionColor::Amber,
            SettingsGroupKey::Tickets => SectionColor::Blue,
            SettingsGroupKey::Integrations => SectionColor::Violet,
            SettingsGroupKey::Data => SectionColor::Teal,
        }
    }

    /// Content-heavy groups render prominent: fewer, wider columns with
    /// larger padding so each card claims more of the row (MAPPS-257). The
    /// Service & Asset Types and Billing & SLA landings carry the most
    /// surfaces, so they read as the larger sections.
    fn prominent(self) -> bool {
        matches!(
            self,
            SettingsGroupKey::ServiceTypes | SettingsGroupKey::Billing
        )
    }
}

/// The nested admin groups, in index display order. Personalization is
/// rendered separately (it is a direct leaf, not a group card).
const SETTINGS_GROUP_ORDER: &[SettingsGroupKey] = &[
    SettingsGroupKey::ServiceTypes,
    SettingsGroupKey::Billing,
    SettingsGroupKey::Tickets,
    SettingsGroupKey::Integrations,
    SettingsGroupKey::Data,
];

/// One configuration surface (a leaf settings page).
struct SettingsSurface {
    route: Route,
    title: &'static str,
    description: &'static str,
    group: SettingsGroupKey,
    /// Hidden in basic mode; shown only when "Show advanced settings" is on.
    advanced: bool,
}

/// localStorage key for the basic/advanced toggle (MAPPS-258). Defaults to
/// basic (false) so casual users are not shown the advanced surfaces.
const PREF_SHOW_ADVANCED: &str = "settings_show_advanced";

/// Single source of truth for every settings surface. Order within a group
/// is the display order on the index, group landing, and search results.
/// The advanced set (per MAPPS-258) is Tax Rates, Payment Gateways, Ticket
/// Categories, and the three RMM surfaces.
const SETTINGS_SURFACES: &[SettingsSurface] = &[
    SettingsSurface {
        route: Route::SettingsAppearance {},
        title: "Appearance",
        description: "Theme (light, dark, or system) and accent color for your account.",
        group: SettingsGroupKey::Personalization,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTvView {},
        title: "TV View",
        description:
            "Full-screen wall-monitor dashboard, scoped to your team. Toggle it on and pick a team.",
        group: SettingsGroupKey::Personalization,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsWorkTypes {},
        title: "Work Types",
        description: "Billable work categories used when logging time entries.",
        group: SettingsGroupKey::ServiceTypes,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTaskStatuses {},
        title: "Task Statuses",
        description: "Workflow states a project task can move through.",
        group: SettingsGroupKey::ServiceTypes,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsAssetTypes {},
        title: "Asset Types",
        description: "Categories for the assets you track per company.",
        group: SettingsGroupKey::ServiceTypes,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsCompanyIndustries {},
        title: "Company Industries",
        description: "Suggested industries when categorizing a company.",
        group: SettingsGroupKey::ServiceTypes,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsProjectTypes {},
        title: "Project Types",
        description: "Classify projects (e.g. client vs internal).",
        group: SettingsGroupKey::ServiceTypes,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsSla {},
        title: "SLA Management",
        description: "Service-level policies, business hours, and holiday calendars.",
        group: SettingsGroupKey::Billing,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsScheduling {},
        title: "Scheduling",
        description: "Standard due date applied to new tasks and tickets when none is set.",
        group: SettingsGroupKey::Billing,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTimeTracking {},
        title: "Time Tracking",
        description: "Maximum hours a user may log against a single day.",
        group: SettingsGroupKey::Billing,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsRateCards {},
        title: "Rate Cards",
        description: "Hourly rates billed per work type.",
        group: SettingsGroupKey::Billing,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTaxRates {},
        title: "Tax Rates",
        description: "Tax rates applied to invoice line items.",
        group: SettingsGroupKey::Billing,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsGateways {},
        title: "Payment Gateways",
        description: "Connect and configure payment providers.",
        group: SettingsGroupKey::Billing,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsPaymentTerms {},
        title: "Payment Terms",
        description: "Options for the invoice payment-terms dropdown.",
        group: SettingsGroupKey::Billing,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTicketStatuses {},
        title: "Ticket Statuses",
        description: "Workflow states a ticket can move through.",
        group: SettingsGroupKey::Tickets,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTicketPriorities {},
        title: "Ticket Priorities",
        description: "Priority levels and their SLA multipliers.",
        group: SettingsGroupKey::Tickets,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTicketTypes {},
        title: "Ticket Types",
        description: "Categories for the kind of work a ticket represents.",
        group: SettingsGroupKey::Tickets,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTicketQueues {},
        title: "Ticket Queues",
        description: "Queues tickets are routed into.",
        group: SettingsGroupKey::Tickets,
        advanced: false,
    },
    SettingsSurface {
        route: Route::SettingsTicketCategories {},
        title: "Ticket Categories",
        description: "Hierarchical categories for classifying tickets.",
        group: SettingsGroupKey::Tickets,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsRmmConnections {},
        title: "RMM Connections",
        description: "Connect remote monitoring providers and test reachability.",
        group: SettingsGroupKey::Integrations,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsRmmDeviceMappings {},
        title: "RMM Device Mappings",
        description: "Map monitored RMM devices to assets and companies.",
        group: SettingsGroupKey::Integrations,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsRmmAlertRules {},
        title: "RMM Alert Rules",
        description: "Turn RMM alerts into tickets automatically.",
        group: SettingsGroupKey::Integrations,
        advanced: true,
    },
    SettingsSurface {
        route: Route::SettingsImportExport {},
        title: "Import & Export",
        description:
            "Download a snapshot of this tenant's data, or restore it from a previous export.",
        group: SettingsGroupKey::Data,
        advanced: false,
    },
];

/// Surfaces filed under `group`, honoring the basic/advanced filter.
fn surfaces_in_group(
    group: SettingsGroupKey,
    show_advanced: bool,
) -> impl Iterator<Item = &'static SettingsSurface> {
    SETTINGS_SURFACES
        .iter()
        .filter(move |s| s.group == group && (show_advanced || !s.advanced))
}

/// `/settings` - the hub index. With no search query it shows the
/// Personalization leaf plus the nested group cards; typing a query
/// flattens to matching leaf cards across every group (MAPPS-258).
#[component]
pub fn SettingsHomePage() -> Element {
    // MAPPS-357: N/A - the hub index is driven by the static SETTINGS_SURFACES
    // taxonomy plus a local search/advanced toggle, with no server fetch that an
    // outage could blank, so there is no unavailable state to render.
    let mut query = use_signal(String::new);
    let mut show_advanced = use_signal(|| crate::utils::prefs::get_bool(PREF_SHOW_ADVANCED, false));

    let adv = *show_advanced.read();
    let q = query.read().trim().to_lowercase();

    // Personalization renders as a direct leaf (it is not nested).
    let personalization: Vec<&SettingsSurface> =
        surfaces_in_group(SettingsGroupKey::Personalization, adv).collect();

    // A group card is shown only when the group has at least one visible
    // surface in the current mode (so an all-advanced group like
    // Integrations drops off the basic index instead of leading to an
    // empty landing).
    let group_cards: Vec<(SettingsGroupKey, Route, &'static str, &'static str)> =
        SETTINGS_GROUP_ORDER
            .iter()
            .copied()
            .filter(|g| surfaces_in_group(*g, adv).next().is_some())
            .filter_map(|g| {
                g.landing()
                    .map(|route| (g, route, g.title(), g.description()))
            })
            .collect();

    // Search reaches every leaf across all groups (bypassing the nesting),
    // matching title or description case-insensitively.
    let results: Vec<&SettingsSurface> = if q.is_empty() {
        Vec::new()
    } else {
        SETTINGS_SURFACES
            .iter()
            .filter(|s| adv || !s.advanced)
            .filter(|s| {
                s.title.to_lowercase().contains(&q) || s.description.to_lowercase().contains(&q)
            })
            .collect()
    };

    use_page_title("Settings");
    rsx! {
        PageHeader {
            title: "Settings",
            subtitle: "Manage the standard types and configuration that shape how your workspace behaves",
        }

        // Constrain the index to roughly 80% of the max-w-7xl content
        // box (max-w-5xl is 64rem vs 80rem) so it reads as a focused
        // settings surface rather than spanning full-bleed (MAPPS-257).
        div { class: "mx-auto w-full max-w-5xl",

        div { class: "mb-8 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
            div { class: "w-full sm:max-w-xs",
                SearchInput {
                    value: query.read().clone(),
                    placeholder: "Search settings...".to_string(),
                    oninput: move |e: FormEvent| query.set(e.value()),
                }
            }
            label { class: "flex shrink-0 items-center gap-2 text-sm text-content select-none",
                input {
                    r#type: "checkbox",
                    class: "h-4 w-4 rounded border-line text-accent focus:ring-accent",
                    checked: adv,
                    onchange: move |_| {
                        let next = !*show_advanced.read();
                        show_advanced.set(next);
                        crate::utils::prefs::set_bool(PREF_SHOW_ADVANCED, next);
                    },
                }
                "Show advanced settings"
            }
        }

        if q.is_empty() {
            if !personalization.is_empty() {
                SettingsGroup {
                    heading: SettingsGroupKey::Personalization.title().to_string(),
                    color: Some(SettingsGroupKey::Personalization.color()),
                    for surface in personalization {
                        SettingsCard {
                            to: surface.route.clone(),
                            title: surface.title.to_string(),
                            description: surface.description.to_string(),
                        }
                    }
                }
            }
            SettingsGroup { heading: "Configuration".to_string(),
                for (group, route, title, description) in group_cards {
                    SettingsCard {
                        to: route,
                        title: title.to_string(),
                        description: description.to_string(),
                        color: Some(group.color()),
                    }
                }
            }
        } else if results.is_empty() {
            Card {
                div { class: "py-12 text-center text-sm text-muted",
                    "No settings match \"{query}\"."
                }
            }
        } else {
            div { class: "{group_grid_class(false)}",
                for surface in results {
                    SettingsCard {
                        to: surface.route.clone(),
                        title: surface.title.to_string(),
                        description: surface.description.to_string(),
                        color: Some(surface.group.color()),
                    }
                }
            }
        }

        }
    }
}

// MAPPS-359: `SectionColor` (the per-nav-area accent palette, originally
// MAPPS-257) moved to `crate::modules::theme::sections` so the sidebar
// categories and these Settings landings share one source of truth. The
// Settings groups still map to the same five hues they always used
// (Emerald/Amber/Blue/Violet/Rose via `SettingsGroupKey::color`); the four
// hues added for the sidebar are simply unused here.
use crate::modules::theme::SectionColor;

/// Card grid columns. Prominent (content-heavy) groups get fewer, wider
/// columns so each card claims more of the row; `items-stretch` keeps every
/// card the same height regardless of description length (MAPPS-257).
fn group_grid_class(prominent: bool) -> &'static str {
    if prominent {
        "grid grid-cols-1 gap-4 items-stretch sm:grid-cols-2"
    } else {
        "grid grid-cols-1 gap-4 items-stretch sm:grid-cols-2 lg:grid-cols-3"
    }
}

/// Breadcrumb trail for a leaf settings page, derived from the taxonomy:
/// Settings > {Group} > {Leaf}, with every ancestor a clickable `Link`.
#[component]
fn SettingsBreadcrumb(current: Route) -> Element {
    let mut items = vec![BreadcrumbItem {
        label: "Settings".to_string(),
        route: Some(Route::SettingsHome {}),
    }];
    if let Some(surface) = SETTINGS_SURFACES.iter().find(|s| s.route == current) {
        if let Some(landing) = surface.group.landing() {
            items.push(BreadcrumbItem {
                label: surface.group.title().to_string(),
                route: Some(landing),
            });
        }
        items.push(BreadcrumbItem {
            label: surface.title.to_string(),
            route: None,
        });
    }
    rsx! {
        Breadcrumbs { items }
    }
}

/// Shared body for a group landing page: a breadcrumb (Settings > Group),
/// a back button, and the group's surfaces as cards (advanced filter
/// honored from the persisted pref).
#[component]
fn SettingsGroupLanding(group: SettingsGroupKey) -> Element {
    // Color and size the landing's cards by the group's nav-area identity
    // (MAPPS-257), shared down to each `SettingsCard` via context.
    use_context_provider(|| GroupStyle {
        color: group.color(),
        prominent: group.prominent(),
    });
    let show_advanced = crate::utils::prefs::get_bool(PREF_SHOW_ADVANCED, false);
    let title = group.title();
    let visible: Vec<&SettingsSurface> = surfaces_in_group(group, show_advanced).collect();

    use_page_title(title.to_string());
    rsx! {
        PageHeader {
            title: title.to_string(),
            subtitle: group.description().to_string(),
            breadcrumbs: rsx! {
                Breadcrumbs {
                    items: vec![
                        BreadcrumbItem { label: "Settings".to_string(), route: Some(Route::SettingsHome {}) },
                        BreadcrumbItem { label: title.to_string(), route: None },
                    ],
                }
            },
            actions: rsx! {
                Link { to: Route::SettingsHome {},
                    Button { variant: ButtonVariant::Secondary, "Back to Settings" }
                }
            },
        }
        if visible.is_empty() {
            Card {
                div { class: "p-6 text-sm text-muted",
                    "These settings are marked advanced. Turn on \"Show advanced settings\" on the "
                    Link {
                        to: Route::SettingsHome {},
                        class: "font-medium text-accent hover:opacity-90",
                        "Settings home"
                    }
                    " to view them."
                }
            }
        } else {
            div { class: "{group_grid_class(group.prominent())}",
                for surface in visible {
                    SettingsCard {
                        to: surface.route.clone(),
                        title: surface.title.to_string(),
                        description: surface.description.to_string(),
                    }
                }
            }
        }
    }
}

/// Group styling shared down to each `SettingsCard` via context so the
/// card call sites stay free of repeated color/size props.
#[derive(Clone, Copy, PartialEq)]
struct GroupStyle {
    color: SectionColor,
    prominent: bool,
}

/// `/settings/group/service-types` - Service & Asset Types landing.
#[component]
pub fn ServiceTypesGroupPage() -> Element {
    // MAPPS-357: N/A - static group landing (card list from the taxonomy), no
    // server resource to fail.
    rsx! {
        SettingsGroupLanding { group: SettingsGroupKey::ServiceTypes }
    }
}

/// `/settings/group/billing` - Billing & SLA landing.
#[component]
pub fn BillingGroupPage() -> Element {
    // MAPPS-357: N/A - static group landing (card list from the taxonomy), no
    // server resource to fail.
    rsx! {
        SettingsGroupLanding { group: SettingsGroupKey::Billing }
    }
}

/// `/settings/group/tickets` - Tickets landing.
#[component]
pub fn TicketsGroupPage() -> Element {
    // MAPPS-357: N/A - static group landing (card list from the taxonomy), no
    // server resource to fail.
    rsx! {
        SettingsGroupLanding { group: SettingsGroupKey::Tickets }
    }
}

/// `/settings/group/integrations` - Integrations landing.
#[component]
pub fn IntegrationsGroupPage() -> Element {
    // MAPPS-357: N/A - static group landing (card list from the taxonomy), no
    // server resource to fail.
    rsx! {
        SettingsGroupLanding { group: SettingsGroupKey::Integrations }
    }
}

/// `/settings/group/data` - Data (import/export) landing (MAPPS-364).
#[component]
pub fn DataGroupPage() -> Element {
    // MAPPS-357: N/A - static group landing (card list from the taxonomy), no
    // server resource to fail.
    rsx! {
        SettingsGroupLanding { group: SettingsGroupKey::Data }
    }
}

/// Per-table row-count summary returned by `POST /api/v1/data/import`
/// (MAPPS-364). `imported` maps each restored table to the number of rows
/// written; a `BTreeMap` so the summary renders in a stable, sorted order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ImportSummary {
    #[serde(default)]
    imported: std::collections::BTreeMap<String, i64>,
}

/// `/settings/import-export` (MAPPS-364, server PMS-646). Admin-only. Export
/// downloads a JSON snapshot of the tenant's data; import wipes and replaces
/// the tenant's data from a previously-exported file, gated behind a typed
/// tenant-name confirmation that matches the backend `confirm` guard.
#[component]
pub fn ImportExportSettingsPage() -> Element {
    use_page_title("Import & Export");
    // MAPPS-377: read auth before the admin gate so the hook set stays stable
    // across renders; it only reads context and feeds the tenant-name
    // confirmation used past the gate.
    let auth = crate::hooks::use_auth();
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Import & Export" } };
    }

    // The import confirmation must equal the tenant's name (the backend compares
    // `confirm` to `tenants.name`); use the active org name the user already
    // sees in the org switcher.
    let tenant_name = auth
        .read()
        .active_org_name()
        .map(str::to_string)
        .unwrap_or_default();

    rsx! {
        PageHeader {
            title: "Import & Export",
            subtitle: "Download a snapshot of this tenant's data, or restore it from a previous export",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsImportExport {} }
            },
        }
        div { class: "space-y-6",
            SeedDemoPanel {}
            ExportPanel {}
            ImportPanel { tenant_name }
        }
    }
}

#[component]
fn ExportPanel() -> Element {
    let mut downloading = use_signal(|| false);
    let mut error = use_signal(String::new);

    let on_download = move |_| {
        if *downloading.read() {
            return;
        }
        error.set(String::new());
        downloading.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::get_authed_bytes("/data/export").await {
                    Ok((bytes, filename)) => {
                        let name = filename.unwrap_or_else(|| "mokosh-export.json".to_string());
                        if let Err(e) = crate::utils::download::save_bytes_as_file(&bytes, &name) {
                            error.set(format!("Fetched the export but could not save it: {e}"));
                        }
                    }
                    Err(e) => error.set(format!("Could not export: {e}")),
                }
            }
            downloading.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-3",
                h3 { class: "text-base font-semibold text-content", "Export" }
                p { class: "text-sm text-muted",
                    "Download a JSON snapshot of all of this tenant's data. The file contains your records. Store it somewhere safe."
                }
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *downloading.read(),
                    onclick: on_download,
                    "Download export"
                }
            }
        }
    }
}

#[component]
fn ImportPanel(tenant_name: String) -> Element {
    let expected = tenant_name;
    let mut confirm = use_signal(String::new);
    let mut file_text = use_signal(|| None::<String>);
    let mut file_name = use_signal(String::new);
    let mut importing = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut summary = use_signal(|| None::<ImportSummary>);
    // MAPPS-357: block the destructive write while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    let confirm_matches = !expected.is_empty() && confirm.read().trim() == expected.trim();
    let has_file = file_text.read().is_some();
    let can_import = confirm_matches && has_file && can_mutate && !*importing.read();

    let on_import = move |_| {
        if !can_import {
            return;
        }
        error.set(String::new());
        summary.set(None);
        let Some(text) = file_text.read().clone() else {
            return;
        };
        let export_json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                error.set(format!("The selected file is not valid JSON: {e}"));
                return;
            }
        };
        let confirm_val = confirm.read().trim().to_string();
        importing.set(true);
        let body = serde_json::json!({ "confirm": confirm_val, "export": export_json });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_authed_typed::<ImportSummary, _>(
                    "/data/import",
                    &body,
                )
                .await
                {
                    Ok(s) => summary.set(Some(s)),
                    Err(e) => error.set(format!("Import failed: {}", e.user_message())),
                }
            }
            importing.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-4",
                h3 { class: "text-base font-semibold text-content", "Import" }
                div {
                    class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                    strong { "This replaces all current data for this tenant." }
                    " Importing wipes every existing record and restores from the uploaded file. This cannot be undone. Export a fresh snapshot first."
                }

                div { class: "space-y-1",
                    label {
                        r#for: "import_file",
                        class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                        "Import file"
                    }
                    input {
                        id: "import_file",
                        r#type: "file",
                        accept: "application/json,.json",
                        class: "block w-full text-sm text-gray-700 dark:text-gray-300 file:mr-3 file:rounded-md file:border-0 file:bg-gray-100 dark:file:bg-gray-700 file:px-3 file:py-1.5 file:text-sm file:font-medium",
                        onchange: move |evt: FormEvent| {
                            error.set(String::new());
                            summary.set(None);
                            let Some(file) = evt.files().into_iter().next() else {
                                return;
                            };
                            file_name.set(file.name());
                            spawn(async move {
                                match file.read_string().await {
                                    Ok(text) => file_text.set(Some(text)),
                                    Err(_) => {
                                        error.set("Could not read the selected file.".to_string())
                                    }
                                }
                            });
                        },
                    }
                    if !file_name.read().is_empty() {
                        p { class: "text-xs text-muted", "Selected: {file_name.read()}" }
                    }
                }

                crate::components::Input {
                    name: "import_confirm",
                    label: "Type the tenant name to confirm",
                    placeholder: expected.clone(),
                    help: if expected.is_empty() {
                        "No active tenant name is available; import is unavailable.".to_string()
                    } else {
                        format!("Type \"{expected}\" exactly to enable import.")
                    },
                    value: confirm.read().clone(),
                    oninput: move |e: FormEvent| confirm.set(e.value()),
                }

                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }

                if let Some(s) = summary.read().clone() {
                    div {
                        class: "text-sm text-green-800 dark:text-green-300 bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-900 rounded-md px-3 py-2 space-y-2",
                        p { class: "font-medium", "Import complete." }
                        ul { class: "list-disc pl-5",
                            for (table , count) in s.imported.iter() {
                                li { key: "{table}", "{table}: {count}" }
                            }
                        }
                        p {
                            "Every record was replaced and re-keyed. Reload the app so the rest of the pages show the imported data."
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                #[cfg(feature = "web")]
                                if let Some(w) = web_sys::window() {
                                    let _ = w.location().reload();
                                }
                            },
                            "Reload the app"
                        }
                    }
                }

                Button {
                    variant: ButtonVariant::Danger,
                    disabled: !can_import,
                    loading: *importing.read(),
                    onclick: on_import,
                    "Import and replace all data"
                }
            }
        }
    }
}

/// Response from `POST /api/v1/data/seed-demo` (PMS-679). `seeded` is `false`
/// (not an error) when the tenant already had data and nothing was loaded.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct SeedDemoResult {
    #[serde(default)]
    seeded: bool,
    #[serde(default)]
    message: String,
}

/// Additive "Load demo data" panel (PMS-679, server `/data/seed-demo`). Loads
/// the built-in sample dataset into an empty tenant so a fresh account can be
/// explored; refuses (without wiping) when the tenant already has data.
#[component]
fn SeedDemoPanel() -> Element {
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut result = use_signal(|| None::<SeedDemoResult>);
    // MAPPS-357: block the write while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    let on_load = move |_| {
        if *loading.read() || !can_mutate {
            return;
        }
        error.set(String::new());
        result.set(None);
        loading.set(true);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::post_authed_typed::<SeedDemoResult, _>(
                    "/data/seed-demo",
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(r) => result.set(Some(r)),
                    Err(e) => error.set(format!("Could not load demo data: {}", e.user_message())),
                }
            }
            loading.set(false);
        });
    };

    rsx! {
        Card {
            div { class: "space-y-3",
                h3 { class: "text-base font-semibold text-content", "Load demo data" }
                p { class: "text-sm text-muted",
                    "Populate this tenant with a small sample dataset (a company, two contacts, and a few tickets) so you can explore Mokosh. This only loads into an empty tenant. It never overwrites existing data."
                }
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                if let Some(r) = result.read().clone() {
                    if r.seeded {
                        div {
                            class: "text-sm text-green-800 dark:text-green-300 bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-900 rounded-md px-3 py-2",
                            "{r.message}"
                        }
                    } else {
                        div {
                            class: "text-sm text-amber-800 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                            "{r.message}"
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    loading: *loading.read(),
                    onclick: on_load,
                    "Load demo data"
                }
            }
        }
    }
}

#[component]
fn SettingsGroup(
    heading: String,
    // A color ties the group to its nav area (MAPPS-257); `None` leaves
    // the heading neutral, used by the synthetic "Configuration" umbrella
    // whose cards are each colored by their own target domain instead.
    #[props(default)] color: Option<SectionColor>,
    #[props(default)] prominent: bool,
    children: Element,
) -> Element {
    use_context_provider(|| GroupStyle {
        color: color.unwrap_or(SectionColor::Blue),
        prominent,
    });
    let heading_class = match color {
        Some(c) => c.heading_class(),
        None => "text-muted",
    };
    rsx! {
        div { class: "mb-8",
            h2 { class: "text-sm font-semibold uppercase tracking-wide mb-3 {heading_class}",
                "{heading}"
            }
            div { class: "{group_grid_class(prominent)}", {children} }
        }
    }
}

#[component]
fn SettingsCard(
    to: Route,
    title: String,
    description: String,
    // Per-card color override. Cards inside a single-domain group inherit
    // the group's color from context; mixed grids (the Configuration
    // umbrella and search results) pass each card its own domain color.
    #[props(default)] color: Option<SectionColor>,
) -> Element {
    // Default keeps the card sensible if ever rendered outside a group.
    let style = try_use_context::<GroupStyle>().unwrap_or(GroupStyle {
        color: SectionColor::Blue,
        prominent: false,
    });
    let border = color.unwrap_or(style.color).card_border_class();
    let padding = if style.prominent { "p-6" } else { "p-4" };
    rsx! {
        Link {
            to,
            class: "block h-full rounded-lg border border-line border-l-4 bg-surface {padding} {border} hover:shadow-sm transition-colors",
            div { class: "font-medium text-content", "{title}" }
            div { class: "mt-1 text-sm text-muted", "{description}" }
        }
    }
}

// ============================================================================
// Scheduling: standard due date (MAPPS-345 / PMS-345)
// GET/PUT `/settings/scheduling/default_due_business_days`
// ============================================================================

/// Shape of the per-key tenant setting response. Only `value` is read; the
/// server stores the business-day offset as a JSON integer.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct SettingValueRow {
    #[serde(default)]
    value: serde_json::Value,
}

const DEFAULT_DUE_SETTING_PATH: &str = "/settings/scheduling/default_due_business_days";
const MAX_DUE_BUSINESS_DAYS: u32 = 365;

/// `/settings/scheduling` - edit the tenant-wide standard due date that the
/// server applies to new tasks (and tickets with no SLA-derived due date)
/// when no explicit due date is given. `0` disables the default. Mirrors the
/// `scheduling/default_due_business_days` tenant setting and the
/// `validate_setting_value` range (0..=365) on the server (PMS-345).
#[component]
pub fn SchedulingSettingsPage() -> Element {
    use_page_title("Scheduling");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Scheduling" } };
    }

    rsx! { SchedulingSettingsBody {} }
}

#[component]
fn SchedulingSettingsBody() -> Element {
    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the setting auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            // A missing setting (404) is the unconfigured state, not an
            // error: treat it as `0` (no default due date). Any other
            // failure is surfaced so the form does not silently seed 0.
            match crate::hooks::fetch::api::get_authed_typed::<SettingValueRow>(
                DEFAULT_DUE_SETTING_PATH,
            )
            .await
            {
                Ok(row) => Some(
                    row.value
                        .as_u64()
                        .unwrap_or(0)
                        .min(u64::from(MAX_DUE_BUSINESS_DAYS)) as u32,
                ),
                Err(e) if e.status_code() == Some(404) => Some(0u32),
                Err(_) => None,
            }
        }
        #[cfg(not(feature = "web"))]
        {
            None::<u32>
        }
    });

    let snap = resource.read_unchecked();

    // MAPPS-357: a load failure while the server is flagged down is an outage,
    // not a real error - render the honest unavailable state instead of the
    // generic "could not load" card. A failure while still reachable keeps the
    // error card below. The write control lives in the child form, disabled via
    // `use_can_mutate` there.
    let reachable = crate::hooks::use_server_reachable();
    if matches!(*snap, Some(None)) && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Scheduling".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Scheduling",
            subtitle: "Standard due date applied to new work when none is set",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsScheduling {} }
            },
        }
        match &*snap {
            // PMS-353: shared detail skeleton instead of a bare text line.
            None => rsx! {
                crate::components::DetailSkeleton {}
            },
            Some(None) => rsx! {
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300",
                            "Could not load scheduling settings. Refresh the page to retry."
                        }
                    }
                }
            },
            Some(Some(days)) => rsx! {
                StandardDueDateForm { initial: *days }
            },
        }
    }
}

#[component]
fn StandardDueDateForm(initial: u32) -> Element {
    let mut value = use_signal(|| initial.to_string());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);
    // MAPPS-357: gate the Save control on server reachability.
    let can_mutate = crate::hooks::use_can_mutate();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        let raw = value.read().trim().to_string();
        // Mirror the server-side validation (0..=365) so the user gets an
        // inline message instead of a 422 round-trip.
        let parsed = match raw.parse::<u32>() {
            Ok(n) if n <= MAX_DUE_BUSINESS_DAYS => n,
            _ => {
                error.set(format!(
                    "Enter a whole number of business days from 0 to {MAX_DUE_BUSINESS_DAYS} (0 disables the default)."
                ));
                saved.set(false);
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = serde_json::json!({ "value": parsed });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<SettingValueRow, _>(
                    DEFAULT_DUE_SETTING_PATH,
                    &body,
                )
                .await
                {
                    Ok(_) => saved.set(true),
                    Err(e) => error.set(format!(
                        "Could not save scheduling settings: {}",
                        e.user_message()
                    )),
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
        Card {
            div { class: "space-y-4 p-6",
                if saved() {
                    div { class: "text-sm text-green-700 dark:text-green-300", "Scheduling settings saved." }
                }
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error}"
                    }
                }
                crate::components::Input {
                    name: "default_due_business_days",
                    label: "Default due date (business days)",
                    r#type: "number",
                    min: "0".to_string(),
                    max: MAX_DUE_BUSINESS_DAYS.to_string(),
                    step: "1".to_string(),
                    value: value.read().clone(),
                    oninput: move |e: FormEvent| {
                        saved.set(false);
                        value.set(e.value());
                    },
                }
                p { class: "text-sm text-muted",
                    "New tasks without a due date, and tickets that match no SLA, default to this many business days from the day they are created (weekends skipped). Set 0 to leave new work with no default due date."
                }
                div {
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *saving.read(),
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: handle_save,
                        "Save Changes"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Time tracking: max hours per day (MAPPS-244 / PMS-396)
// GET/PUT `/settings/time_tracking/max_hours_per_day`
// ============================================================================

const MAX_HOURS_PER_DAY_SETTING_PATH: &str = "/settings/time_tracking/max_hours_per_day";
/// Server stores the cap as an integer number of hours validated `1..=24`;
/// an unconfigured tenant (404) falls back to a full 24-hour day.
const MAX_HOURS_PER_DAY_CEILING: u32 = 24;
const DEFAULT_MAX_HOURS_PER_DAY: u32 = 24;

/// `/settings/time-tracking` - edit the tenant-wide cap on how many hours of
/// time entries a single user may log against one day. The server enforces
/// the cap and `validate_setting_value` mirrors the `1..=24` range (PMS-396);
/// this page mirrors that range client-side for an inline error instead of a
/// 422 round-trip.
#[component]
pub fn MaxHoursPerDaySettingsPage() -> Element {
    use_page_title("Time Tracking");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Time Tracking" } };
    }

    rsx! { MaxHoursPerDaySettingsBody {} }
}

#[component]
fn MaxHoursPerDaySettingsBody() -> Element {
    let resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the setting auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        #[cfg(feature = "web")]
        {
            // A missing setting (404) is the unconfigured state, not an
            // error: treat it as the full 24-hour default. Any other failure
            // is surfaced so the form does not silently seed a wrong cap.
            match crate::hooks::fetch::api::get_authed_typed::<SettingValueRow>(
                MAX_HOURS_PER_DAY_SETTING_PATH,
            )
            .await
            {
                Ok(row) => Some(
                    row.value
                        .as_u64()
                        .unwrap_or(u64::from(DEFAULT_MAX_HOURS_PER_DAY))
                        .clamp(1, u64::from(MAX_HOURS_PER_DAY_CEILING)) as u32,
                ),
                Err(e) if e.status_code() == Some(404) => Some(DEFAULT_MAX_HOURS_PER_DAY),
                Err(_) => None,
            }
        }
        #[cfg(not(feature = "web"))]
        {
            None::<u32>
        }
    });

    let snap = resource.read_unchecked();

    // MAPPS-357: a load failure while the server is flagged down is an outage,
    // not a real error - render the honest unavailable state instead of the
    // generic "could not load" card. A failure while still reachable keeps the
    // error card below. The write control lives in the child form, disabled via
    // `use_can_mutate` there.
    let reachable = crate::hooks::use_server_reachable();
    if matches!(*snap, Some(None)) && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Time Tracking".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Time Tracking",
            subtitle: "Maximum hours a user may log against a single day",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTimeTracking {} }
            },
        }
        match &*snap {
            None => rsx! {
                crate::components::DetailSkeleton {}
            },
            Some(None) => rsx! {
                Card {
                    div { class: "py-12 text-center",
                        p { class: "text-sm text-red-600 dark:text-red-300",
                            "Could not load time tracking settings. Refresh the page to retry."
                        }
                    }
                }
            },
            Some(Some(hours)) => rsx! {
                MaxHoursPerDayForm { initial: *hours }
            },
        }
    }
}

#[component]
fn MaxHoursPerDayForm(initial: u32) -> Element {
    let mut value = use_signal(|| initial.to_string());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut saved = use_signal(|| false);
    // MAPPS-357: gate the Save control on server reachability.
    let can_mutate = crate::hooks::use_can_mutate();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        let raw = value.read().trim().to_string();
        // Mirror the server-side validation (1..=24) so the user gets an
        // inline message instead of a 422 round-trip.
        let parsed = match raw.parse::<u32>() {
            Ok(n) if (1..=MAX_HOURS_PER_DAY_CEILING).contains(&n) => n,
            _ => {
                error.set(format!(
                    "Enter a whole number of hours from 1 to {MAX_HOURS_PER_DAY_CEILING}."
                ));
                saved.set(false);
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        saved.set(false);
        let body = serde_json::json!({ "value": parsed });
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match crate::hooks::fetch::api::put_authed_typed::<SettingValueRow, _>(
                    MAX_HOURS_PER_DAY_SETTING_PATH,
                    &body,
                )
                .await
                {
                    Ok(_) => saved.set(true),
                    Err(e) => error.set(format!(
                        "Could not save time tracking settings: {}",
                        e.user_message()
                    )),
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
        Card {
            div { class: "space-y-4 p-6",
                if saved() {
                    div { class: "text-sm text-green-700 dark:text-green-300", "Time tracking settings saved." }
                }
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error}"
                    }
                }
                crate::components::Input {
                    name: "max_hours_per_day",
                    label: "Maximum hours per day",
                    r#type: "number",
                    min: "1".to_string(),
                    max: MAX_HOURS_PER_DAY_CEILING.to_string(),
                    step: "1".to_string(),
                    value: value.read().clone(),
                    oninput: move |e: FormEvent| {
                        saved.set(false);
                        value.set(e.value());
                    },
                }
                p { class: "text-sm text-gray-500 dark:text-gray-400",
                    "A user's time entries for any one day cannot total more than this many hours. The log-time screen warns before submitting once a day's entries would exceed the cap."
                }
                div {
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: *saving.read(),
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: handle_save,
                        "Save Changes"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Work types  (GET/POST `/work-types`, PUT/DELETE `/work-types/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct WorkTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default_billable: bool,
    // Server serializes `Decimal` as a JSON string, so mirror it as one.
    #[serde(default)]
    default_rate: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn WorkTypesSettingsPage() -> Element {
    use_page_title("Work Types");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Work Types" } };
    }

    rsx! { WorkTypesSettingsBody {} }
}

#[component]
fn WorkTypesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<WorkTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/work-types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<WorkTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Work Types".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Work Types",
            subtitle: "Billable work categories used when logging time entries",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsWorkTypes {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(WorkTypeFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Work Type"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "work types" }
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
                        TableHeader { "Billable" }
                        TableHeader { class: "text-right", "Default Rate" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 4,
                        message: "No work types yet. Click New Work Type to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = WorkTypeFormState::from_existing(&row);
                                let name = row.name.clone();
                                let billable = row.default_billable;
                                let active = row.is_active;
                                let rate = row.default_rate.clone().unwrap_or_default();
                                let rate_display = if rate.trim().is_empty() {
                                    "-".to_string()
                                } else {
                                    // PMS-365: shared formatter for consistent $1,234.00.
                                    format_money_str(&rate)
                                };
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if billable {
                                                Badge { variant: BadgeVariant::Blue, "Billable" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "Non-billable" }
                                            }
                                        }
                                        TableCell { class: "text-right", "{rate_display}" }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            WorkTypeFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct WorkTypeFormState {
    id: Option<String>,
    name: String,
    description: String,
    default_billable: bool,
    default_rate: String,
    is_active: bool,
    sort_order: String,
}

impl WorkTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            default_billable: true,
            default_rate: String::new(),
            is_active: true,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &WorkTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            default_billable: r.default_billable,
            default_rate: r.default_rate.clone().unwrap_or_default(),
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct WorkTypeFormModalProps {
    state: WorkTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn WorkTypeFormModal(props: WorkTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut default_billable = use_signal(|| initial.default_billable);
    let mut default_rate = use_signal(|| initial.default_rate.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let rate = default_rate.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "description": opt_str(&desc),
            "default_billable": *default_billable.read(),
            // Server parses the rate string into `rust_decimal::Decimal`.
            "default_rate": opt_str(&rate),
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/work-types", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save work type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/work-types").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete work type: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Work Type".to_string() } else { "New Work Type".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Work Type".to_string(),
            crate::components::Input {
                name: "work_type_name",
                label: "Name",
                placeholder: "e.g. On-site Support",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_rate",
                label: "Default rate",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                placeholder: "e.g. 150.00",
                value: default_rate.read().clone(),
                oninput: move |e: FormEvent| default_rate.set(e.value()),
            }
            crate::components::Input {
                name: "work_type_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "work_type_billable",
                label: "Billable by default",
                checked: *default_billable.read(),
                onchange: move |_| {
                    let next = !*default_billable.read();
                    default_billable.set(next);
                },
            }
            crate::components::Checkbox {
                name: "work_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Task statuses  (GET/POST `/task-statuses`, PUT/DELETE `/task-statuses/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TaskStatusRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    is_completed: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TaskStatusesSettingsPage() -> Element {
    use_page_title("Task Statuses");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Task Statuses" } };
    }

    rsx! { TaskStatusesSettingsBody {} }
}

#[component]
fn TaskStatusesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TaskStatusFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/task-statuses?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TaskStatusRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TaskStatusRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Task Statuses".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Task Statuses",
            subtitle: "Workflow states a project task can move through",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTaskStatuses {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TaskStatusFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Status"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "task statuses" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Color" }
                        TableHeader { "Completed" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No task statuses yet. Click New Status to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TaskStatusFormState::from_existing(&row);
                                let name = row.name.clone();
                                let color = row.color.clone();
                                let completed = row.is_completed;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell { ColorSwatch { color } }
                                        TableCell {
                                            if completed {
                                                Badge { variant: BadgeVariant::Green, "Completed" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "Open" }
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

        if let Some(state) = editing.read().clone() {
            TaskStatusFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TaskStatusFormState {
    id: Option<String>,
    name: String,
    color: String,
    is_completed: bool,
    sort_order: String,
}

impl TaskStatusFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            color: "#6b7280".to_string(),
            is_completed: false,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TaskStatusRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            color: if r.color.trim().is_empty() {
                "#6b7280".to_string()
            } else {
                r.color.clone()
            },
            is_completed: r.is_completed,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TaskStatusFormModalProps {
    state: TaskStatusFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TaskStatusFormModal(props: TaskStatusFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut color = use_signal(|| initial.color.clone());
    let mut is_completed = use_signal(|| initial.is_completed);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        let color_val = color.read().trim().to_string();
        if color_val.is_empty() {
            error.set("Color is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "color": color_val,
            "is_completed": *is_completed.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/task-statuses", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save task status: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/task-statuses").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete task status: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Task Status".to_string() } else { "New Task Status".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Status".to_string(),
            crate::components::Input {
                name: "task_status_name",
                label: "Name",
                placeholder: "e.g. In Progress",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "task_status_color",
                label: "Color",
                r#type: "color",
                value: color.read().clone(),
                oninput: move |e: FormEvent| color.set(e.value()),
            }
            crate::components::Input {
                name: "task_status_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "task_status_completed",
                label: "Counts as completed",
                help: "Tasks in this status are treated as done.",
                checked: *is_completed.read(),
                onchange: move |_| {
                    let next = !*is_completed.read();
                    is_completed.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Asset types  (GET/POST `/asset-types`, PUT/DELETE `/asset-types/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct AssetTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    is_active: bool,
    /// PMS-476: ITIL CI class tag (hardware / software / service /
    /// network / document / location, or a tenant-coined value).
    /// Optional so types created before PMS-456 still deserialise.
    #[serde(default)]
    itil_category: Option<String>,
}

#[component]
pub fn AssetTypesSettingsPage() -> Element {
    use_page_title("Asset Types");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Asset Types" } };
    }

    rsx! { AssetTypesSettingsBody {} }
}

#[component]
fn AssetTypesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<AssetTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/asset-types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<AssetTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<AssetTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Asset Types".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Asset Types",
            subtitle: "Categories for the assets you track per company",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsAssetTypes {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(AssetTypeFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Asset Type"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "asset types" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Icon" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No asset types yet. Click New Asset Type to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = AssetTypeFormState::from_existing(&row);
                                let name = row.name.clone();
                                let icon = row.icon.clone().unwrap_or_default();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if icon.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { class: "text-sm text-content", "{icon}" }
                                            }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            AssetTypeFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AssetTypeFormState {
    id: Option<String>,
    name: String,
    icon: String,
    is_active: bool,
    /// PMS-476: free-text ITIL category. Empty = unset on the wire.
    itil_category: String,
}

impl AssetTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            icon: String::new(),
            is_active: true,
            itil_category: String::new(),
        }
    }

    fn from_existing(r: &AssetTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            icon: r.icon.clone().unwrap_or_default(),
            is_active: r.is_active,
            itil_category: r.itil_category.clone().unwrap_or_default(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AssetTypeFormModalProps {
    state: AssetTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn AssetTypeFormModal(props: AssetTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut icon = use_signal(|| initial.icon.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut itil_category = use_signal(|| initial.itil_category.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let icon_val = icon.read().trim().to_string();
        let itil_val = itil_category.read().trim().to_string();
        // `parent_type_id` is always sent null: this v1 editor manages
        // flat (top-level) asset types only. Nested types are a follow-up.
        // PMS-476: `itil_category` is free-text so the tenant can coin
        // a new value; the help text on the input surfaces the
        // standard ITIL classes as suggestions.
        let body = serde_json::json!({
            "name": name.read().trim(),
            "icon": opt_str(&icon_val),
            "parent_type_id": serde_json::Value::Null,
            "is_active": *is_active.read(),
            "itil_category": opt_str(&itil_val),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/asset-types", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save asset type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/asset-types").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete asset type: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Asset Type".to_string() } else { "New Asset Type".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Asset Type".to_string(),
            crate::components::Input {
                name: "asset_type_name",
                label: "Name",
                placeholder: "e.g. Laptop",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "asset_type_icon",
                label: "Icon",
                placeholder: "Optional icon name",
                value: icon.read().clone(),
                oninput: move |e: FormEvent| icon.set(e.value()),
            }
            // PMS-476: ITIL CI class. Free-text so a tenant can
            // coin a new value; the help text shows the standard
            // classes from the v3 service-asset-and-configuration
            // mgmt taxonomy as suggestions.
            crate::components::Input {
                name: "asset_type_itil_category",
                label: "ITIL category",
                placeholder: "e.g. hardware",
                help: "Standard: hardware, software, service, network, document, location. Leave blank for unclassified.".to_string(),
                value: itil_category.read().clone(),
                oninput: move |e: FormEvent| itil_category.set(e.value()),
            }
            crate::components::Checkbox {
                name: "asset_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Company industries (GET/POST `/contacts/company-industries`,
// PUT/DELETE `/contacts/company-industries/{id}`)
//
// PMS-601 lookup that feeds the company Industry combobox's suggestions.
// Name + Active only; admin-managed.
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct CompanyIndustryRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_active: bool,
}

#[component]
pub fn CompanyIndustriesSettingsPage() -> Element {
    use_page_title("Company Industries");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Company Industries" } };
    }

    rsx! { CompanyIndustriesSettingsBody {} }
}

#[component]
fn CompanyIndustriesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<CompanyIndustryFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/contacts/company-industries?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyIndustryRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<CompanyIndustryRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Company Industries".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Company Industries",
            subtitle: "Suggested industries when categorizing a company",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsCompanyIndustries {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(CompanyIndustryFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Industry"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "company industries" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 2,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 2, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 2,
                        message: "No industries yet. Click New Industry to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = CompanyIndustryFormState::from_existing(&row);
                                let name = row.name.clone();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            CompanyIndustryFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CompanyIndustryFormState {
    id: Option<String>,
    name: String,
    is_active: bool,
}

impl CompanyIndustryFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            is_active: true,
        }
    }

    fn from_existing(r: &CompanyIndustryRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            is_active: r.is_active,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CompanyIndustryFormModalProps {
    state: CompanyIndustryFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn CompanyIndustryFormModal(props: CompanyIndustryFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "is_active": *is_active.read(),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/contacts/company-industries", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save industry: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else {
            return;
        };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/contacts/company-industries").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete industry: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Industry".to_string() } else { "New Industry".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Industry".to_string(),
            crate::components::Input {
                name: "company_industry_name",
                label: "Name",
                placeholder: "e.g. Healthcare",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Checkbox {
                name: "company_industry_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Project types  (GET/POST `/project-types`, PUT/DELETE `/project-types/{id}`)
//
// PMS-322 lookup table replacing the old hardcoded `client`/`internal` enum.
// `is_system` rows (the seeded `client`/`internal`) are read-only: the server
// refuses to delete them, so the editor hides Delete and disables the name
// field for them (the other fields stay editable).
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ProjectTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
    #[serde(default)]
    is_system: bool,
}

#[component]
pub fn ProjectTypesSettingsPage() -> Element {
    use_page_title("Project Types");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Project Types" } };
    }

    rsx! { ProjectTypesSettingsBody {} }
}

#[component]
fn ProjectTypesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<ProjectTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/project-types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<ProjectTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<ProjectTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Project Types".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Project Types",
            subtitle: "Classify projects (e.g. client vs internal)",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsProjectTypes {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(ProjectTypeFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Project Type"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "project types" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Default" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No project types yet. Click New Project Type to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = ProjectTypeFormState::from_existing(&row);
                                let name = row.name.clone();
                                let default = row.is_default;
                                let active = row.is_active;
                                let system = row.is_system;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            div { class: "flex items-center gap-2",
                                                span { class: "font-medium text-accent", "{name}" }
                                                if system {
                                                    Badge { variant: BadgeVariant::Gray, "System" }
                                                }
                                            }
                                        }
                                        TableCell {
                                            if default {
                                                Badge { variant: BadgeVariant::Blue, "Default" }
                                            }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            ProjectTypeFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectTypeFormState {
    id: Option<String>,
    name: String,
    is_default: bool,
    is_active: bool,
    sort_order: String,
    is_system: bool,
}

impl ProjectTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            is_default: false,
            is_active: true,
            sort_order: "0".to_string(),
            is_system: false,
        }
    }

    fn from_existing(r: &ProjectTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            is_default: r.is_default,
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
            is_system: r.is_system,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProjectTypeFormModalProps {
    state: ProjectTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn ProjectTypeFormModal(props: ProjectTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();
    let is_system = initial.is_system;

    let mut name = use_signal(|| initial.name.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        // `is_system` is server-owned and not sent.
        let body = serde_json::json!({
            "name": name.read().trim(),
            "is_default": *is_default.read(),
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/project-types", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save project type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/project-types").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete project type: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Project Type".to_string() } else { "New Project Type".to_string() },
            is_edit,
            deletable: !is_system,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Project Type".to_string(),
            if is_system {
                div {
                    class: "text-xs text-muted bg-app border border-line rounded-md px-3 py-2",
                    "System project type: its name is fixed and it cannot be deleted. You can still change default, active, and sort order."
                }
            }
            crate::components::Input {
                name: "project_type_name",
                label: "Name",
                placeholder: "e.g. Client",
                required: true,
                disabled: is_system,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "project_type_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "project_type_default",
                label: "Default project type",
                help: "Applied to new projects when none is chosen.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
            crate::components::Checkbox {
                name: "project_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Payment terms  (GET/POST `/payment-terms`, PUT/DELETE `/payment-terms/{id}`)
//
// PMS-333 lookup backing the invoice payment-terms dropdown (MAPPS-170).
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct PaymentTermRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn PaymentTermsSettingsPage() -> Element {
    use_page_title("Payment Terms");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Payment Terms" } };
    }

    rsx! { PaymentTermsSettingsBody {} }
}

#[component]
fn PaymentTermsSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<PaymentTermFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/payment-terms?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<PaymentTermRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<PaymentTermRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Payment Terms".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Payment Terms",
            subtitle: "Options for the invoice payment-terms dropdown",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsPaymentTerms {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(PaymentTermFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Payment Term"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "payment terms" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Default" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No payment terms yet. Click New Payment Term to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = PaymentTermFormState::from_existing(&row);
                                let name = row.name.clone();
                                let default = row.is_default;
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if default {
                                                Badge { variant: BadgeVariant::Blue, "Default" }
                                            }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            PaymentTermFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PaymentTermFormState {
    id: Option<String>,
    name: String,
    is_default: bool,
    is_active: bool,
    sort_order: String,
}

impl PaymentTermFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            is_default: false,
            is_active: true,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &PaymentTermRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            is_default: r.is_default,
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PaymentTermFormModalProps {
    state: PaymentTermFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn PaymentTermFormModal(props: PaymentTermFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "is_default": *is_default.read(),
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/payment-terms", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save payment term: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/payment-terms").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete payment term: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Payment Term".to_string() } else { "New Payment Term".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Payment Term".to_string(),
            crate::components::Input {
                name: "payment_term_name",
                label: "Name",
                placeholder: "e.g. Net 30",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "payment_term_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "payment_term_default",
                label: "Default payment term",
                help: "Applied to new invoices when none is chosen.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
            crate::components::Checkbox {
                name: "payment_term_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Ticket statuses  (GET/POST `/tickets/statuses`, PUT/DELETE `/tickets/statuses/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TicketStatusRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    is_closed: bool,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TicketStatusesSettingsPage() -> Element {
    use_page_title("Ticket Statuses");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Ticket Statuses" } };
    }

    rsx! { TicketStatusesSettingsBody {} }
}

#[component]
fn TicketStatusesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TicketStatusFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/tickets/statuses?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TicketStatusRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TicketStatusRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket Statuses".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Ticket Statuses",
            subtitle: "Workflow states a ticket can move through",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTicketStatuses {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TicketStatusFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Status"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "ticket statuses" }
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
                        TableHeader { "Color" }
                        TableHeader { "Closed" }
                        TableHeader { "Default" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 4,
                        message: "No ticket statuses yet. Click New Status to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TicketStatusFormState::from_existing(&row);
                                let name = row.name.clone();
                                let color = row.color.clone();
                                let closed = row.is_closed;
                                let default = row.is_default;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell { ColorSwatch { color } }
                                        TableCell {
                                            if closed {
                                                Badge { variant: BadgeVariant::Gray, "Closed" }
                                            } else {
                                                Badge { variant: BadgeVariant::Green, "Open" }
                                            }
                                        }
                                        TableCell {
                                            if default {
                                                Badge { variant: BadgeVariant::Blue, "Default" }
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

        if let Some(state) = editing.read().clone() {
            TicketStatusFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TicketStatusFormState {
    id: Option<String>,
    name: String,
    color: String,
    is_closed: bool,
    is_default: bool,
    sort_order: String,
}

impl TicketStatusFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            color: "#6b7280".to_string(),
            is_closed: false,
            is_default: false,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TicketStatusRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            color: non_empty_color(&r.color),
            is_closed: r.is_closed,
            is_default: r.is_default,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketStatusFormModalProps {
    state: TicketStatusFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TicketStatusFormModal(props: TicketStatusFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut color = use_signal(|| initial.color.clone());
    let mut is_closed = use_signal(|| initial.is_closed);
    let mut is_default = use_signal(|| initial.is_default);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let body = serde_json::json!({
            "name": name.read().trim(),
            "color": color.read().trim(),
            "is_closed": *is_closed.read(),
            "is_default": *is_default.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = save_lookup(id, "/tickets/statuses", &body).await;
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save status: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/tickets/statuses").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete status: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Ticket Status".to_string() } else { "New Ticket Status".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Status".to_string(),
            crate::components::Input {
                name: "ticket_status_name",
                label: "Name",
                placeholder: "e.g. Waiting on Customer",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_status_color",
                label: "Color",
                r#type: "color",
                value: color.read().clone(),
                oninput: move |e: FormEvent| color.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_status_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "ticket_status_closed",
                label: "Counts as closed",
                help: "Tickets in this status are treated as resolved/closed.",
                checked: *is_closed.read(),
                onchange: move |_| {
                    let next = !*is_closed.read();
                    is_closed.set(next);
                },
            }
            crate::components::Checkbox {
                name: "ticket_status_default",
                label: "Default status",
                help: "Applied to new tickets when none is chosen.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Ticket priorities  (GET/POST `/tickets/priorities`, PUT/DELETE `/tickets/priorities/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TicketPriorityRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    sla_multiplier: f64,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TicketPrioritiesSettingsPage() -> Element {
    use_page_title("Ticket Priorities");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Ticket Priorities" } };
    }

    rsx! { TicketPrioritiesSettingsBody {} }
}

#[component]
fn TicketPrioritiesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TicketPriorityFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/tickets/priorities?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TicketPriorityRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TicketPriorityRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket Priorities".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Ticket Priorities",
            subtitle: "Priority levels and their SLA multipliers",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTicketPriorities {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TicketPriorityFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Priority"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "ticket priorities" }
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
                        TableHeader { "Color" }
                        TableHeader { class: "text-right", "SLA x" }
                        TableHeader { "Default" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 4,
                        message: "No ticket priorities yet. Click New Priority to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TicketPriorityFormState::from_existing(&row);
                                let name = row.name.clone();
                                let color = row.color.clone();
                                let sla = format!("{:.2}x", row.sla_multiplier);
                                let default = row.is_default;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell { ColorSwatch { color } }
                                        TableCell { class: "text-right", "{sla}" }
                                        TableCell {
                                            if default {
                                                Badge { variant: BadgeVariant::Blue, "Default" }
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

        if let Some(state) = editing.read().clone() {
            TicketPriorityFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TicketPriorityFormState {
    id: Option<String>,
    name: String,
    color: String,
    icon: String,
    sla_multiplier: String,
    is_default: bool,
    sort_order: String,
}

impl TicketPriorityFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            color: "#6b7280".to_string(),
            icon: String::new(),
            sla_multiplier: "1.0".to_string(),
            is_default: false,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TicketPriorityRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            color: non_empty_color(&r.color),
            icon: r.icon.clone().unwrap_or_default(),
            sla_multiplier: r.sla_multiplier.to_string(),
            is_default: r.is_default,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketPriorityFormModalProps {
    state: TicketPriorityFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TicketPriorityFormModal(props: TicketPriorityFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut color = use_signal(|| initial.color.clone());
    let mut icon = use_signal(|| initial.icon.clone());
    let mut sla_multiplier = use_signal(|| initial.sla_multiplier.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        // Validate the multiplier client-side so an out-of-range or
        // non-numeric entry gets a clear message instead of a raw 400
        // (server accepts 0.0..=9.99).
        let sla = match sla_multiplier.read().trim().parse::<f64>() {
            Ok(v) if (0.0..=9.99).contains(&v) => v,
            _ => {
                error.set("SLA multiplier must be a number between 0 and 9.99.".to_string());
                return;
            }
        };
        saving.set(true);
        error.set(String::new());
        let icon_val = icon.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "color": color.read().trim(),
            "icon": opt_str(&icon_val),
            "sla_multiplier": sla,
            "is_default": *is_default.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/tickets/priorities", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save priority: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/tickets/priorities").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete priority: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Ticket Priority".to_string() } else { "New Ticket Priority".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Priority".to_string(),
            crate::components::Input {
                name: "ticket_priority_name",
                label: "Name",
                placeholder: "e.g. Urgent",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_priority_color",
                label: "Color",
                r#type: "color",
                value: color.read().clone(),
                oninput: move |e: FormEvent| color.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_priority_sla",
                label: "SLA multiplier",
                r#type: "number",
                step: "0.01".to_string(),
                min: "0".to_string(),
                max: "9.99".to_string(),
                help: "Scales SLA target durations (e.g. 0.5 = half the time).",
                value: sla_multiplier.read().clone(),
                oninput: move |e: FormEvent| sla_multiplier.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_priority_icon",
                label: "Icon",
                placeholder: "Optional icon name",
                value: icon.read().clone(),
                oninput: move |e: FormEvent| icon.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_priority_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "ticket_priority_default",
                label: "Default priority",
                help: "Applied to new tickets when none is chosen.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Ticket types  (GET/POST `/tickets/types`, PUT/DELETE `/tickets/types/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TicketTypeRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TicketTypesSettingsPage() -> Element {
    use_page_title("Ticket Types");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Ticket Types" } };
    }

    rsx! { TicketTypesSettingsBody {} }
}

#[component]
fn TicketTypesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TicketTypeFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/tickets/types?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TicketTypeRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TicketTypeRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket Types".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Ticket Types",
            subtitle: "Categories for the kind of work a ticket represents",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTicketTypes {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TicketTypeFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Type"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "ticket types" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Description" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No ticket types yet. Click New Type to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TicketTypeFormState::from_existing(&row);
                                let name = row.name.clone();
                                let desc = row.description.clone().unwrap_or_default();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            span { class: "text-sm text-muted", "{desc}" }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            TicketTypeFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TicketTypeFormState {
    id: Option<String>,
    name: String,
    description: String,
    icon: String,
    is_active: bool,
    sort_order: String,
}

impl TicketTypeFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            icon: String::new(),
            is_active: true,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TicketTypeRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            icon: r.icon.clone().unwrap_or_default(),
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketTypeFormModalProps {
    state: TicketTypeFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TicketTypeFormModal(props: TicketTypeFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut icon = use_signal(|| initial.icon.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let icon_val = icon.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "description": opt_str(&desc),
            "icon": opt_str(&icon_val),
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/tickets/types", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save type: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/tickets/types").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete type: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Ticket Type".to_string() } else { "New Ticket Type".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Type".to_string(),
            crate::components::Input {
                name: "ticket_type_name",
                label: "Name",
                placeholder: "e.g. Incident",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_type_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_type_icon",
                label: "Icon",
                placeholder: "Optional icon name",
                value: icon.read().clone(),
                oninput: move |e: FormEvent| icon.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_type_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "ticket_type_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Ticket queues  (GET/POST `/tickets/queues`, PUT/DELETE `/tickets/queues/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TicketQueueRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TicketQueuesSettingsPage() -> Element {
    use_page_title("Ticket Queues");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Ticket Queues" } };
    }

    rsx! { TicketQueuesSettingsBody {} }
}

#[component]
fn TicketQueuesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TicketQueueFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/tickets/queues?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TicketQueueRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TicketQueueRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket Queues".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Ticket Queues",
            subtitle: "Queues tickets are routed into",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTicketQueues {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TicketQueueFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Queue"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "ticket queues" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Color" }
                        TableHeader { "Default" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No ticket queues yet. Click New Queue to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TicketQueueFormState::from_existing(&row);
                                let name = row.name.clone();
                                let color = row.color.clone().unwrap_or_default();
                                let default = row.is_default;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if color.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                ColorSwatch { color }
                                            }
                                        }
                                        TableCell {
                                            if default {
                                                Badge { variant: BadgeVariant::Blue, "Default" }
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

        if let Some(state) = editing.read().clone() {
            TicketQueueFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TicketQueueFormState {
    id: Option<String>,
    name: String,
    description: String,
    color: String,
    icon: String,
    is_default: bool,
    sort_order: String,
}

impl TicketQueueFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            color: "#6b7280".to_string(),
            icon: String::new(),
            is_default: false,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TicketQueueRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            color: non_empty_color(r.color.as_deref().unwrap_or_default()),
            icon: r.icon.clone().unwrap_or_default(),
            is_default: r.is_default,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketQueueFormModalProps {
    state: TicketQueueFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TicketQueueFormModal(props: TicketQueueFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut color = use_signal(|| initial.color.clone());
    let mut icon = use_signal(|| initial.icon.clone());
    let mut is_default = use_signal(|| initial.is_default);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let icon_val = icon.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "description": opt_str(&desc),
            "color": color.read().trim(),
            "icon": opt_str(&icon_val),
            "is_default": *is_default.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/tickets/queues", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save queue: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/tickets/queues").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete queue: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Ticket Queue".to_string() } else { "New Ticket Queue".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Queue".to_string(),
            crate::components::Input {
                name: "ticket_queue_name",
                label: "Name",
                placeholder: "e.g. Tier 1 Support",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_queue_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_queue_color",
                label: "Color",
                r#type: "color",
                value: color.read().clone(),
                oninput: move |e: FormEvent| color.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_queue_icon",
                label: "Icon",
                placeholder: "Optional icon name",
                value: icon.read().clone(),
                oninput: move |e: FormEvent| icon.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_queue_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "ticket_queue_default",
                label: "Default queue",
                help: "New tickets land here when no queue is chosen.",
                checked: *is_default.read(),
                onchange: move |_| {
                    let next = !*is_default.read();
                    is_default.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Ticket categories  (GET/POST `/tickets/categories`, PUT/DELETE `/tickets/categories/{id}`)
// ============================================================================

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct TicketCategoryRow {
    id: Uuid,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sort_order: i64,
}

#[component]
pub fn TicketCategoriesSettingsPage() -> Element {
    use_page_title("Ticket Categories");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "Ticket Categories" } };
    }

    rsx! { TicketCategoriesSettingsBody {} }
}

#[component]
fn TicketCategoriesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<TicketCategoryFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/tickets/categories?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<TicketCategoryRow>>(&path)
            .await
            .ok()
    });

    // The full category set, independent of the table's current page, so
    // the Parent column and the parent picker can resolve a parent that
    // lives on another page. Server caps per_page at 100; categories
    // beyond that are not expected for a single tenant.
    let mut all_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: secondary lookup (parent-name resolution + parent picker);
        // subscribe so it also repopulates on reconnect. It still degrades to
        // an empty set on failure - the primary resource drives the outage view.
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Paginated<TicketCategoryRow>>(
            "/tickets/categories?per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<TicketCategoryRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket Categories".to_string() }
        };
    }

    // Resolve parent names for the table, and feed the parent picker,
    // from the full set rather than just the current page.
    let all_snap = all_resource.read_unchecked();
    let all_cats: Vec<TicketCategoryRow> = match &*all_snap {
        Some(list) => list.clone(),
        None => Vec::new(),
    };
    let name_by_id: std::collections::HashMap<Uuid, String> =
        all_cats.iter().map(|r| (r.id, r.name.clone())).collect();

    rsx! {
        PageHeader {
            title: "Ticket Categories",
            subtitle: "Hierarchical categories for classifying tickets",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsTicketCategories {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(TicketCategoryFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Category"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "ticket categories" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 3,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Parent" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 3, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 3,
                        message: "No ticket categories yet. Click New Category to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = TicketCategoryFormState::from_existing(&row);
                                let name = row.name.clone();
                                let parent = row
                                    .parent_id
                                    .and_then(|pid| name_by_id.get(&pid).cloned())
                                    .unwrap_or_default();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if parent.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { class: "text-sm text-content", "{parent}" }
                                            }
                                        }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            TicketCategoryFormModal {
                state,
                all: all_cats.clone(),
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                    all_resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TicketCategoryFormState {
    id: Option<String>,
    name: String,
    description: String,
    parent_id: String,
    is_active: bool,
    sort_order: String,
}

impl TicketCategoryFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            parent_id: String::new(),
            is_active: true,
            sort_order: "0".to_string(),
        }
    }

    fn from_existing(r: &TicketCategoryRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            parent_id: r.parent_id.map(|p| p.to_string()).unwrap_or_default(),
            is_active: r.is_active,
            sort_order: r.sort_order.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketCategoryFormModalProps {
    state: TicketCategoryFormState,
    all: Vec<TicketCategoryRow>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TicketCategoryFormModal(props: TicketCategoryFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut description = use_signal(|| initial.description.clone());
    let mut parent_id = use_signal(|| initial.parent_id.clone());
    let mut is_active = use_signal(|| initial.is_active);
    let mut sort_order = use_signal(|| initial.sort_order.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    // Parent options: every category except the one being edited (the
    // server also rejects self-parenting and deeper cycles).
    let self_id = initial.id.clone();
    let mut parent_options = vec![SelectOption {
        value: String::new(),
        label: "None (top level)".to_string(),
        disabled: false,
    }];
    for cat in props.all.iter() {
        let cat_id = cat.id.to_string();
        if self_id.as_deref() == Some(cat_id.as_str()) {
            continue;
        }
        parent_options.push(SelectOption {
            value: cat_id,
            label: cat.name.clone(),
            disabled: false,
        });
    }

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let desc = description.read().trim().to_string();
        let parent = parent_id.read().trim().to_string();
        let body = serde_json::json!({
            "name": name.read().trim(),
            "description": opt_str(&desc),
            "parent_id": opt_str(&parent),
            "is_active": *is_active.read(),
            "sort_order": sort_order.read().trim().parse::<i64>().unwrap_or(0),
        });
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match save_lookup(id, "/tickets/categories", &body).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save category: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/tickets/categories").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete category: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Ticket Category".to_string() } else { "New Ticket Category".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Category".to_string(),
            crate::components::Input {
                name: "ticket_category_name",
                label: "Name",
                placeholder: "e.g. Hardware",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_category_description",
                label: "Description",
                placeholder: "Optional",
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            Select {
                name: "ticket_category_parent",
                label: "Parent category",
                options: parent_options,
                value: parent_id.read().clone(),
                onchange: move |e: FormEvent| parent_id.set(e.value()),
            }
            crate::components::Input {
                name: "ticket_category_sort_order",
                label: "Sort order",
                r#type: "number",
                min: "0".to_string(),
                max: "2147483647".to_string(),
                value: sort_order.read().clone(),
                oninput: move |e: FormEvent| sort_order.set(e.value()),
            }
            crate::components::Checkbox {
                name: "ticket_category_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// RMM integration (MAPPS-199)
//
// Admin-facing UI for the RMM subsystem whose backend shipped under
// PMS-102/103/104/105 but had no frontend. Three surfaces, all admin-gated
// to match the server's `RequireAdmin` on these routes:
//
//   - Connections   GET/POST `/rmm/connections`, GET/PUT/DELETE
//                    `/rmm/connections/{id}`, POST `/rmm/connections/{id}/test`
//   - Device maps    GET/POST `/rmm/device-mappings`, DELETE
//                    `/rmm/device-mappings/{id}`
//   - Alert rules    GET/POST `/rmm/alert-rules`, DELETE `/rmm/alert-rules/{id}`
//
// Provider credentials (`api_key` / `api_secret`) are write-only: the server
// `RmmConnectionResponse` never echoes them back, so they are only ever sent
// (never displayed). On edit, leaving the credential fields blank keeps the
// stored values (server `UpdateRmmConnectionRequest` only re-encrypts when a
// field is supplied). Mirrors the credential-vault reveal pattern (PMS-342).
// ============================================================================

/// Supported `RmmProvider` variants, mirrored from the server allowlist in
/// `mokosh-server src/modules/rmm/routes.rs::create_connection`. Tuple is
/// `(wire value, human label)`.
const RMM_PROVIDERS: &[(&str, &str)] = &[
    ("tactical_rmm", "Tactical RMM"),
    ("mesh_central", "MeshCentral"),
    ("datto", "Datto"),
    ("connectwise", "ConnectWise"),
    ("ninja_rmm", "NinjaRMM"),
];

/// Human label for a stored provider wire value, falling back to the raw
/// value if the server ever returns one this client does not know about.
fn rmm_provider_label(value: &str) -> String {
    RMM_PROVIDERS
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| value.to_string())
}

/// A coloured badge for a connection's last sync/test status.
#[component]
fn RmmStatusBadge(status: String) -> Element {
    let variant = match status.as_str() {
        "success" => BadgeVariant::Green,
        "failed" | "error" => BadgeVariant::Red,
        "syncing" | "pending" => BadgeVariant::Yellow,
        _ => BadgeVariant::Gray,
    };
    let label = if status.trim().is_empty() {
        "-".to_string()
    } else {
        status
    };
    rsx! {
        Badge { variant, "{label}" }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RmmConnectionRow {
    id: Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    api_url: String,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    sync_interval_minutes: i64,
    #[serde(default)]
    sync_status: String,
    #[serde(default)]
    last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Connections  (GET/POST `/rmm/connections`, GET/PUT/DELETE
// `/rmm/connections/{id}`, POST `/rmm/connections/{id}/test`)
// ---------------------------------------------------------------------------

#[component]
pub fn RmmConnectionsSettingsPage() -> Element {
    use_page_title("RMM Connections");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "RMM Connections" } };
    }

    rsx! { RmmConnectionsSettingsBody {} }
}

#[component]
fn RmmConnectionsSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<RmmConnectionFormState>);
    let current_page = (*page.read()).max(1);

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/rmm/connections?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<RmmConnectionRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RmmConnectionRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "RMM Connections".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "RMM Connections",
            subtitle: "Connect remote monitoring providers and test reachability",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsRmmConnections {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block creates while the server is unreachable.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(RmmConnectionFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Connection"
                }
            },
        }

        if fetch_failed {
            LoadError { what: "RMM connections" }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 5,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Provider" }
                        TableHeader { "API URL" }
                        TableHeader { "Sync status" }
                        TableHeader { "Active" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 5, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 5,
                        message: "No RMM connections yet. Click New Connection to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = RmmConnectionFormState::from_existing(&row);
                                let name = row.name.clone();
                                let provider = rmm_provider_label(&row.provider);
                                let api_url = row.api_url.clone();
                                let status = row.sync_status.clone();
                                let active = row.is_active;
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| editing.set(Some(edit_state.clone())),
                                        TableCell {
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell { "{provider}" }
                                        TableCell {
                                            span { class: "text-muted break-all", "{api_url}" }
                                        }
                                        TableCell { RmmStatusBadge { status } }
                                        TableCell { ActiveBadge { active } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            RmmConnectionFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

/// Form state for the connection modal. Note the absence of `api_key` /
/// `api_secret`: the server never returns them, so the form starts those
/// fields blank (they live as modal-local signals) and only sends them when
/// the admin types a value.
#[derive(Clone, Debug, PartialEq)]
struct RmmConnectionFormState {
    id: Option<String>,
    name: String,
    provider: String,
    api_url: String,
    is_active: bool,
    sync_interval_minutes: String,
}

impl RmmConnectionFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            provider: RMM_PROVIDERS[0].0.to_string(),
            api_url: String::new(),
            is_active: true,
            sync_interval_minutes: "60".to_string(),
        }
    }

    fn from_existing(r: &RmmConnectionRow) -> Self {
        Self {
            id: Some(r.id.to_string()),
            name: r.name.clone(),
            provider: r.provider.clone(),
            api_url: r.api_url.clone(),
            is_active: r.is_active,
            sync_interval_minutes: r.sync_interval_minutes.to_string(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RmmConnectionFormModalProps {
    state: RmmConnectionFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RmmConnectionFormModal(props: RmmConnectionFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut provider = use_signal(|| initial.provider.clone());
    let mut api_url = use_signal(|| initial.api_url.clone());
    let mut api_key = use_signal(String::new);
    let mut api_secret = use_signal(String::new);
    let mut is_active = use_signal(|| initial.is_active);
    let mut sync_interval = use_signal(|| initial.sync_interval_minutes.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut testing = use_signal(|| false);
    // (reachable, message) of the most recent test, or None when untested.
    let mut test_result = use_signal(|| None::<(bool, String)>);
    // MAPPS-357: the reachability test POSTs to the server, so gate it (and the
    // save/delete buttons the shared SettingFormModal owns) on can_mutate.
    let can_mutate = crate::hooks::use_can_mutate();

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let provider_options: Vec<SelectOption> = RMM_PROVIDERS
        .iter()
        .map(|(v, l)| SelectOption::new(*v, *l))
        .collect();

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        if api_url.read().trim().is_empty() {
            error.set("API URL is required.".to_string());
            return;
        }
        let key = api_key.read().trim().to_string();
        // Credentials are write-only and never returned, so a fresh
        // connection must carry an API key; an edit may omit it to keep the
        // stored one.
        if save_id.is_none() && key.is_empty() {
            error.set("API key is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let interval = sync_interval.read().trim().parse::<i32>().unwrap_or(60);
        let secret = api_secret.read().trim().to_string();
        let id = save_id.clone();
        let provider_v = provider.read().clone();
        let name_v = name.read().trim().to_string();
        let url_v = api_url.read().trim().to_string();
        let active_v = *is_active.read();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match id {
                    None => {
                        let body = serde_json::json!({
                            "name": name_v,
                            "provider": provider_v,
                            "api_url": url_v,
                            "api_key": key,
                            "api_secret": opt_str(&secret),
                            "is_active": active_v,
                            "sync_interval_minutes": interval,
                        });
                        crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                            "/rmm/connections",
                            &body,
                        )
                        .await
                        .map(|_| ())
                    }
                    Some(id) => {
                        // PUT leaves omitted fields untouched; only send the
                        // credential fields when the admin actually typed one.
                        let mut body = serde_json::json!({
                            "name": name_v,
                            "api_url": url_v,
                            "is_active": active_v,
                            "sync_interval_minutes": interval,
                        });
                        if !key.is_empty() {
                            body["api_key"] = serde_json::Value::String(key);
                        }
                        if !secret.is_empty() {
                            body["api_secret"] = serde_json::Value::String(secret);
                        }
                        crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                            &format!("/rmm/connections/{id}"),
                            &body,
                        )
                        .await
                        .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save connection: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                match delete_lookup(&id, "/rmm/connections").await {
                    Ok(true) => onsaved.call(()),
                    Ok(false) => {}
                    Err(err) => error.set(format!("Could not delete connection: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    let test_id = initial.id.clone();
    let handle_test = move |_| {
        let Some(id) = test_id.clone() else { return };
        if *testing.read() {
            return;
        }
        testing.set(true);
        test_result.set(None);
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/rmm/connections/{id}/test");
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    &path,
                    &serde_json::json!({}),
                )
                .await
                {
                    Ok(v) => {
                        let reachable = v
                            .get("reachable")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false);
                        let msg = if reachable {
                            let status = v.get("status").and_then(|s| s.as_u64()).unwrap_or(0);
                            format!("Reachable (HTTP {status}).")
                        } else {
                            let err = v
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("not reachable");
                            format!("Not reachable: {err}")
                        };
                        test_result.set(Some((reachable, msg)));
                    }
                    Err(err) => test_result.set(Some((false, format!("Test failed: {err}")))),
                }
            }
            testing.set(false);
        });
    };

    let key_placeholder = if is_edit {
        "Leave blank to keep the current key"
    } else {
        "Provider API key"
    };
    let secret_placeholder = if is_edit {
        "Leave blank to keep the current secret"
    } else {
        "Optional provider API secret"
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit RMM Connection".to_string() } else { "New RMM Connection".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            create_label: "Create Connection".to_string(),
            crate::components::Input {
                name: "rmm_conn_name",
                label: "Name",
                placeholder: "e.g. Production Tactical RMM",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            // Provider is immutable after creation (the server
            // `UpdateRmmConnectionRequest` has no `provider` field), so the
            // dropdown is disabled on edit and just shows the stored value.
            Select {
                name: "rmm_conn_provider",
                label: "Provider",
                options: provider_options,
                required: true,
                disabled: is_edit,
                value: provider.read().clone(),
                onchange: move |e: FormEvent| provider.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_conn_api_url",
                label: "API URL",
                r#type: "url",
                placeholder: "https://rmm.example.com",
                required: true,
                value: api_url.read().clone(),
                oninput: move |e: FormEvent| api_url.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_conn_api_key",
                label: "API key",
                r#type: "password",
                placeholder: key_placeholder.to_string(),
                required: !is_edit,
                value: api_key.read().clone(),
                oninput: move |e: FormEvent| api_key.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_conn_api_secret",
                label: "API secret",
                r#type: "password",
                placeholder: secret_placeholder.to_string(),
                value: api_secret.read().clone(),
                oninput: move |e: FormEvent| api_secret.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_conn_sync_interval",
                label: "Sync interval (minutes)",
                r#type: "number",
                min: "1".to_string(),
                step: "1".to_string(),
                value: sync_interval.read().clone(),
                oninput: move |e: FormEvent| sync_interval.set(e.value()),
            }
            crate::components::Checkbox {
                name: "rmm_conn_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
            // Test reachability against a saved connection. Only available on
            // edit because the server keys the test off the stored, encrypted
            // credentials.
            if is_edit {
                div { class: "pt-2 border-t border-line",
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *testing.read(),
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't test while the server is unreachable".to_string()),
                        onclick: handle_test,
                        "Test connection"
                    }
                    if let Some((reachable, msg)) = test_result.read().clone() {
                        p {
                            class: if reachable {
                                "mt-2 text-sm text-green-700 dark:text-green-300"
                            } else {
                                "mt-2 text-sm text-red-700 dark:text-red-300"
                            },
                            "{msg}"
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Device mappings  (GET/POST `/rmm/device-mappings`, DELETE
// `/rmm/device-mappings/{id}`). Create + delete only (no server update route).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RmmDeviceMappingRow {
    id: Uuid,
    #[serde(default)]
    rmm_connection_id: Option<Uuid>,
    #[serde(default)]
    rmm_device_id: String,
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    sync_status: String,
}

#[component]
pub fn RmmDeviceMappingsSettingsPage() -> Element {
    use_page_title("RMM Device Mappings");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "RMM Device Mappings" } };
    }

    rsx! { RmmDeviceMappingsSettingsBody {} }
}

#[component]
fn RmmDeviceMappingsSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut creating = use_signal(|| false);
    let mut deleting_id = use_signal(|| None::<Uuid>);
    let mut row_error = use_signal(String::new);
    let current_page = (*page.read()).max(1);

    // Connections drive the create dropdown and the per-row "connection"
    // column label.
    let conns_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: secondary lookup for connection labels + the create
        // dropdown; subscribe so it repopulates on reconnect. It degrades to an
        // empty set on failure - the primary resource drives the outage view.
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Paginated<RmmConnectionRow>>(
            "/rmm/connections?page=1&per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
    });
    let connections: Vec<RmmConnectionRow> = conns_resource
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/rmm/device-mappings?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<RmmDeviceMappingRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RmmDeviceMappingRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "RMM Device Mappings".to_string() }
        };
    }

    let conn_label = |id: Option<Uuid>| -> String {
        match id {
            Some(id) => connections
                .iter()
                .find(|c| c.id == id)
                .map(|c| {
                    if c.name.trim().is_empty() {
                        id.to_string()
                    } else {
                        c.name.clone()
                    }
                })
                .unwrap_or_else(|| id.to_string()),
            None => "-".to_string(),
        }
    };

    let no_connections = connections.is_empty();

    rsx! {
        PageHeader {
            title: "RMM Device Mappings",
            subtitle: "Map monitored RMM devices to assets and companies",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsRmmDeviceMappings {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: also block creates while the server is unreachable.
                    disabled: no_connections || !can_mutate,
                    title: if !can_mutate {
                        Some("Can't do this while the server is unreachable".to_string())
                    } else {
                        no_connections
                            .then(|| "Add an RMM connection first, then map its devices here.".to_string())
                    },
                    onclick: move |_| creating.set(true),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Mapping"
                }
            },
        }

        if no_connections {
            div {
                class: "mb-3 text-sm text-content bg-surface-2 border border-line rounded-md px-3 py-2",
                "Add an RMM connection first, then map its devices here."
            }
        }
        if fetch_failed {
            LoadError { what: "RMM device mappings" }
        }
        if !row_error.read().is_empty() {
            div {
                class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                "{row_error}"
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 5,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Device" }
                        TableHeader { "Device ID" }
                        TableHeader { "Connection" }
                        TableHeader { "Sync status" }
                        TableHeader { class: "text-right", "" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 5, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 5,
                        message: "No device mappings yet. Click New Mapping to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let rid = row.id;
                                let device = row.device_name.clone().unwrap_or_default();
                                let device_display = if device.trim().is_empty() {
                                    "-".to_string()
                                } else {
                                    device
                                };
                                let device_id = row.rmm_device_id.clone();
                                let connection = conn_label(row.rmm_connection_id);
                                let status = row.sync_status.clone();
                                let is_deleting = *deleting_id.read() == Some(rid);
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium", "{device_display}" }
                                        }
                                        TableCell {
                                            span { class: "font-mono text-xs text-muted break-all", "{device_id}" }
                                        }
                                        TableCell { "{connection}" }
                                        TableCell { RmmStatusBadge { status } }
                                        TableCell { class: "text-right",
                                            Button {
                                                variant: ButtonVariant::Danger,
                                                size: crate::components::ButtonSize::Small,
                                                loading: is_deleting,
                                                // MAPPS-357: block deletes while the server is unreachable.
                                                disabled: !can_mutate,
                                                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                                                onclick: move |_| {
                                                    if deleting_id.read().is_some() {
                                                        return;
                                                    }
                                                    deleting_id.set(Some(rid));
                                                    row_error.set(String::new());
                                                    spawn(async move {
                                                        #[cfg(feature = "web")]
                                                        {
                                                            match delete_lookup(&rid.to_string(), "/rmm/device-mappings").await {
                                                                Ok(true) => resource.restart(),
                                                                Ok(false) => {}
                                                                Err(err) => row_error.set(format!("Could not delete mapping: {err}")),
                                                            }
                                                        }
                                                        deleting_id.set(None);
                                                    });
                                                },
                                                "Delete"
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

        if *creating.read() {
            RmmDeviceMappingFormModal {
                connections: connections.clone(),
                onclose: move |_| creating.set(false),
                onsaved: move |_| {
                    creating.set(false);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RmmDeviceMappingFormModalProps {
    connections: Vec<RmmConnectionRow>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RmmDeviceMappingFormModal(props: RmmDeviceMappingFormModalProps) -> Element {
    let first_conn = props
        .connections
        .first()
        .map(|c| c.id.to_string())
        .unwrap_or_default();

    let mut connection_id = use_signal(|| first_conn.clone());
    let mut device_id = use_signal(String::new);
    let mut device_name = use_signal(String::new);
    // Asset / company are optional links, captured via the shared pickers.
    let mut asset_id = use_signal(String::new);
    let mut asset_name = use_signal(String::new);
    let mut company_id = use_signal(String::new);
    let mut company_name = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let conn_options: Vec<SelectOption> = props
        .connections
        .iter()
        .map(|c| {
            let label = if c.name.trim().is_empty() {
                c.id.to_string()
            } else {
                c.name.clone()
            };
            SelectOption::new(c.id.to_string(), label)
        })
        .collect();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        if connection_id.read().trim().is_empty() {
            error.set("Select a connection.".to_string());
            return;
        }
        if device_id.read().trim().is_empty() {
            error.set("Device ID is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let conn = connection_id.read().trim().to_string();
        let dev = device_id.read().trim().to_string();
        let dev_name = device_name.read().trim().to_string();
        let asset = asset_id.read().trim().to_string();
        let company = company_id.read().trim().to_string();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({
                    "rmm_connection_id": conn,
                    "rmm_device_id": dev,
                    "device_name": opt_str(&dev_name),
                    "asset_id": opt_str(&asset),
                    "company_id": opt_str(&company),
                });
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    "/rmm/device-mappings",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save mapping: {err}")),
                }
            }
            saving.set(false);
        });
    };

    let company_selected_id = {
        let v = company_id.read().clone();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };
    let asset_selected_id = {
        let v = asset_id.read().clone();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };

    rsx! {
        SettingFormModal {
            title: "New Device Mapping".to_string(),
            is_edit: false,
            saving: *saving.read(),
            deleting: false,
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: move |_| {},
            create_label: "Create Mapping".to_string(),
            Select {
                name: "rmm_map_connection",
                label: "Connection",
                options: conn_options,
                required: true,
                value: connection_id.read().clone(),
                onchange: move |e: FormEvent| connection_id.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_map_device_id",
                label: "RMM device ID",
                placeholder: "Provider's device identifier",
                required: true,
                value: device_id.read().clone(),
                oninput: move |e: FormEvent| device_id.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_map_device_name",
                label: "Device name",
                placeholder: "Optional friendly name",
                value: device_name.read().clone(),
                oninput: move |e: FormEvent| device_name.set(e.value()),
            }
            crate::components::CompanyPicker {
                value: company_name.read().clone(),
                selected_id: company_selected_id,
                label: "Company (optional)".to_string(),
                onselect: move |(id, name): (String, String)| {
                    company_id.set(id);
                    company_name.set(name);
                },
                onclear: move |_| {
                    company_id.set(String::new());
                    company_name.set(String::new());
                },
            }
            crate::components::AssetPicker {
                value: asset_name.read().clone(),
                selected_id: asset_selected_id,
                label: "Asset (optional)".to_string(),
                onselect: move |(id, name): (String, String)| {
                    asset_id.set(id);
                    asset_name.set(name);
                },
                onclear: move |_| {
                    asset_id.set(String::new());
                    asset_name.set(String::new());
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Alert rules  (GET/POST `/rmm/alert-rules`, DELETE `/rmm/alert-rules/{id}`).
// Create + delete only (no server update route).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RmmAlertRuleRow {
    id: Uuid,
    #[serde(default)]
    rmm_connection_id: Option<Uuid>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    alert_type: Option<String>,
    #[serde(default)]
    auto_create_ticket: bool,
    #[serde(default)]
    is_active: bool,
}

#[component]
pub fn RmmAlertRulesSettingsPage() -> Element {
    use_page_title("RMM Alert Rules");
    if !use_is_admin() {
        return rsx! { AdminOnlyNotice { title: "RMM Alert Rules" } };
    }

    rsx! { RmmAlertRulesSettingsBody {} }
}

#[component]
fn RmmAlertRulesSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut creating = use_signal(|| false);
    let mut deleting_id = use_signal(|| None::<Uuid>);
    let mut row_error = use_signal(String::new);
    let current_page = (*page.read()).max(1);

    let conns_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: secondary lookup for connection labels + the create
        // dropdown; subscribe so it repopulates on reconnect. It degrades to an
        // empty set on failure - the primary resource drives the outage view.
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<Paginated<RmmConnectionRow>>(
            "/rmm/connections?page=1&per_page=100",
        )
        .await
        .ok()
        .map(|p| p.data)
    });
    let connections: Vec<RmmConnectionRow> = conns_resource
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();

    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // MAPPS-357: subscribe to reachability so the list auto-refetches the
        // instant the server comes back (paired with the recovery poll).
        let _reachable = crate::hooks::use_server_reachable();
        let path = format!("/rmm/alert-rules?page={current_page}&per_page={PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<Paginated<RmmAlertRuleRow>>(&path)
            .await
            .ok()
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RmmAlertRuleRow>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: a failed load while the server is flagged down is an outage,
    // not an empty list - render the honest unavailable state (keeps the nav +
    // banner) instead of an empty table. A 4xx while still reachable keeps the
    // LoadError line below. Writes are blocked while down via `can_mutate`.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "RMM Alert Rules".to_string() }
        };
    }

    let conn_label = |id: Option<Uuid>| -> String {
        match id {
            Some(id) => connections
                .iter()
                .find(|c| c.id == id)
                .map(|c| {
                    if c.name.trim().is_empty() {
                        id.to_string()
                    } else {
                        c.name.clone()
                    }
                })
                .unwrap_or_else(|| id.to_string()),
            None => "-".to_string(),
        }
    };

    let no_connections = connections.is_empty();

    rsx! {
        PageHeader {
            title: "RMM Alert Rules",
            subtitle: "Turn RMM alerts into tickets automatically",
            breadcrumbs: rsx! {
                SettingsBreadcrumb { current: Route::SettingsRmmAlertRules {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: also block creates while the server is unreachable.
                    disabled: no_connections || !can_mutate,
                    title: if !can_mutate {
                        Some("Can't do this while the server is unreachable".to_string())
                    } else {
                        no_connections
                            .then(|| "Add an RMM connection first, then define alert rules for it.".to_string())
                    },
                    onclick: move |_| creating.set(true),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Alert Rule"
                }
            },
        }

        if no_connections {
            div {
                class: "mb-3 text-sm text-content bg-surface-2 border border-line rounded-md px-3 py-2",
                "Add an RMM connection first, then define alert rules for it."
            }
        }
        if fetch_failed {
            LoadError { what: "RMM alert rules" }
        }
        if !row_error.read().is_empty() {
            div {
                class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                "{row_error}"
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 5,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Connection" }
                        TableHeader { "Alert type" }
                        TableHeader { "Auto-create ticket" }
                        TableHeader { class: "text-right", "" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 5, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 5,
                        message: "No alert rules yet. Click New Alert Rule to add one.".to_string(),
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let rid = row.id;
                                let name = row.name.clone();
                                let connection = conn_label(row.rmm_connection_id);
                                let alert_type = row.alert_type.clone().unwrap_or_default();
                                let alert_type_display = if alert_type.trim().is_empty() {
                                    "Any".to_string()
                                } else {
                                    alert_type
                                };
                                let auto = row.auto_create_ticket;
                                let is_deleting = *deleting_id.read() == Some(rid);
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium", "{name}" }
                                        }
                                        TableCell { "{connection}" }
                                        TableCell { "{alert_type_display}" }
                                        TableCell {
                                            if auto {
                                                Badge { variant: BadgeVariant::Green, "Yes" }
                                            } else {
                                                Badge { variant: BadgeVariant::Gray, "No" }
                                            }
                                        }
                                        TableCell { class: "text-right",
                                            Button {
                                                variant: ButtonVariant::Danger,
                                                size: crate::components::ButtonSize::Small,
                                                loading: is_deleting,
                                                // MAPPS-357: block deletes while the server is unreachable.
                                                disabled: !can_mutate,
                                                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                                                onclick: move |_| {
                                                    if deleting_id.read().is_some() {
                                                        return;
                                                    }
                                                    deleting_id.set(Some(rid));
                                                    row_error.set(String::new());
                                                    spawn(async move {
                                                        #[cfg(feature = "web")]
                                                        {
                                                            match delete_lookup(&rid.to_string(), "/rmm/alert-rules").await {
                                                                Ok(true) => resource.restart(),
                                                                Ok(false) => {}
                                                                Err(err) => row_error.set(format!("Could not delete rule: {err}")),
                                                            }
                                                        }
                                                        deleting_id.set(None);
                                                    });
                                                },
                                                "Delete"
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

        if *creating.read() {
            RmmAlertRuleFormModal {
                connections: connections.clone(),
                onclose: move |_| creating.set(false),
                onsaved: move |_| {
                    creating.set(false);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct RmmAlertRuleFormModalProps {
    connections: Vec<RmmConnectionRow>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn RmmAlertRuleFormModal(props: RmmAlertRuleFormModalProps) -> Element {
    let first_conn = props
        .connections
        .first()
        .map(|c| c.id.to_string())
        .unwrap_or_default();

    let mut connection_id = use_signal(|| first_conn.clone());
    let mut name = use_signal(String::new);
    let mut alert_type = use_signal(String::new);
    let mut auto_create_ticket = use_signal(|| true);
    let mut is_active = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let conn_options: Vec<SelectOption> = props
        .connections
        .iter()
        .map(|c| {
            let label = if c.name.trim().is_empty() {
                c.id.to_string()
            } else {
                c.name.clone()
            };
            SelectOption::new(c.id.to_string(), label)
        })
        .collect();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        if connection_id.read().trim().is_empty() {
            error.set("Select a connection.".to_string());
            return;
        }
        if name.read().trim().is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        saving.set(true);
        error.set(String::new());
        let conn = connection_id.read().trim().to_string();
        let name_v = name.read().trim().to_string();
        let alert = alert_type.read().trim().to_string();
        let auto = *auto_create_ticket.read();
        let active = *is_active.read();
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let body = serde_json::json!({
                    "rmm_connection_id": conn,
                    "name": name_v,
                    "alert_type": opt_str(&alert),
                    "auto_create_ticket": auto,
                    "is_active": active,
                });
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    "/rmm/alert-rules",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save rule: {err}")),
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: "New Alert Rule".to_string(),
            is_edit: false,
            saving: *saving.read(),
            deleting: false,
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: move |_| {},
            create_label: "Create Alert Rule".to_string(),
            Select {
                name: "rmm_rule_connection",
                label: "Connection",
                options: conn_options,
                required: true,
                value: connection_id.read().clone(),
                onchange: move |e: FormEvent| connection_id.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_rule_name",
                label: "Name",
                placeholder: "e.g. Disk space critical",
                required: true,
                value: name.read().clone(),
                oninput: move |e: FormEvent| name.set(e.value()),
            }
            crate::components::Input {
                name: "rmm_rule_alert_type",
                label: "Alert type",
                placeholder: "Optional - leave blank to match any alert type",
                value: alert_type.read().clone(),
                oninput: move |e: FormEvent| alert_type.set(e.value()),
            }
            crate::components::Checkbox {
                name: "rmm_rule_auto_create",
                label: "Auto-create ticket on matching alert",
                checked: *auto_create_ticket.read(),
                onchange: move |_| {
                    let next = !*auto_create_ticket.read();
                    auto_create_ticket.set(next);
                },
            }
            crate::components::Checkbox {
                name: "rmm_rule_active",
                label: "Active",
                checked: *is_active.read(),
                onchange: move |_| {
                    let next = !*is_active.read();
                    is_active.set(next);
                },
            }
        }
    }
}

// ============================================================================
// Shared bits
// ============================================================================

/// Inline error banner shown above a list when its fetch fails.
#[component]
fn LoadError(what: String) -> Element {
    rsx! {
        div {
            class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
            "Could not load {what}. Refresh the page to retry."
        }
    }
}

/// A color dot plus its hex value, for the lookup tables that carry a color.
#[component]
fn ColorSwatch(color: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2",
            span {
                class: "inline-block h-4 w-4 rounded-full border border-line",
                style: "background-color: {color}",
            }
            span { class: "text-xs text-muted", "{color}" }
        }
    }
}

/// Green "Active" / gray "Inactive" badge, shared by the lookups with an
/// `is_active` flag.
#[component]
fn ActiveBadge(active: bool) -> Element {
    rsx! {
        if active {
            Badge { variant: BadgeVariant::Green, "Active" }
        } else {
            Badge { variant: BadgeVariant::Gray, "Inactive" }
        }
    }
}

/// Map a free-text field to a JSON value: an empty/whitespace string
/// becomes `null` (the server treats a missing optional as `None`),
/// otherwise the trimmed string.
fn opt_str(s: &str) -> serde_json::Value {
    let t = s.trim();
    if t.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(t.to_string())
    }
}

/// Fall back to a neutral gray when a stored color is empty, so the
/// `type="color"` input always has a valid hex to show.
fn non_empty_color(c: &str) -> String {
    if c.trim().is_empty() {
        "#6b7280".to_string()
    } else {
        c.to_string()
    }
}

/// POST (create) or PUT (update) a lookup row. `base` is the collection
/// path (e.g. `/tickets/statuses`); update targets `{base}/{id}`.
#[cfg(feature = "web")]
async fn save_lookup(
    id: Option<String>,
    base: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    match id {
        None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(base, body)
            .await
            .map(|_| ()),
        Some(id) => {
            let path = format!("{base}/{id}");
            crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, body)
                .await
                .map(|_| ())
        }
    }
}

/// DELETE a lookup row at `{base}/{id}`. Returns `Ok(true)` when deleted,
/// `Err` on failure. Confirmation is handled up-front by the styled
/// `SettingFormModal` ConfirmDialog (MAPPS-189), so this no longer prompts.
#[cfg(feature = "web")]
async fn delete_lookup(id: &str, base: &str) -> Result<bool, String> {
    crate::hooks::fetch::api::delete_authed(&format!("{base}/{id}"))
        .await
        .map(|_| true)
}

// `SettingFormModal` (the shared create/edit modal chrome) now lives in
// `crate::components` so both these editors and the rate-card editor can
// reuse it (MAPPS-160).

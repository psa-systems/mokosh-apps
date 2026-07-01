//! Layout components

use dioxus::prelude::*;

use super::global_search::GlobalSearch;
use super::icons::*;
use super::theme_picker::ThemePickerButton;
use crate::Route;

/// Main application layout with sidebar
#[derive(Props, Clone, PartialEq)]
pub struct AppLayoutProps {
    children: Element,
    #[props(default)]
    title: String,
}

#[component]
pub fn AppLayout(props: AppLayoutProps) -> Element {
    let mut sidebar_open = use_signal(|| false);

    // MAPPS-287: keep `document.title` in sync with the page title every
    // route hands to AppLayout. Without this the tab title was always the
    // fixed `_mokosh_config` index value ("Mokosh Platform") regardless of
    // route, which broke browser history, bookmarks, tab identification,
    // and the title announcement screen readers make on navigation.
    // Format mirrors Bunyip's pattern: `<page> | Mokosh Platform` when a
    // page provides a title, plain `Mokosh Platform` otherwise. Runs on
    // every render so a page that mutates its own `title` prop (e.g. a
    // detail page swapping "Loading..." for the record name) propagates.
    {
        // Set the tab title directly in the component body, NOT via use_effect.
        // The effect version captured `title` by value and read no signal, so it
        // fired once on mount and never re-ran when a detail page swapped its
        // loading placeholder for the real record name - leaving tabs stuck on
        // "Loading… | Mokosh Platform" even after the record loaded. The body
        // runs on every render with the current props.title, and set_title is
        // idempotent. Both loading placeholders the pages use - "Loading…"
        // (U+2026) and ASCII "Loading..." - are treated as "no title yet" so the
        // tab shows a clean "Mokosh Platform" until the real title arrives.
        #[cfg(feature = "web")]
        {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                let t = props.title.trim();
                let next = if t.is_empty() || t == "Loading…" || t == "Loading..." {
                    "Mokosh Platform".to_string()
                } else {
                    format!("{} | Mokosh Platform", t)
                };
                doc.set_title(&next);
            }
        }
    }

    rsx! {
        // Single full-height viewport: top bar spans the full width
        // (brand + page title + user menu live together), and the
        // sidebar + main content sit side-by-side beneath it. Only the
        // main panel scrolls; the sidebar gets its own internal scroll
        // surface with a thin scrollbar (input.css `.scrollbar-thin`)
        // so users see the affordance when nav overflows on short
        // viewports.
        div { class: "h-screen flex flex-col bg-app overflow-hidden",
            // App-wide "server unreachable" banner (MAPPS-333). Sits at
            // the very top so an outage is the first thing surfaced;
            // renders nothing while the server is reachable, so healthy
            // layouts are unaffected.
            super::ServerStatusBanner {}

            // Admin-only update-available banner. Sits above the top
            // bar (page-wide) and renders nothing for non-admins or
            // when no update is published, so non-admin layouts are
            // unaffected.
            super::UpdateBanner {}

            // Persistent top bar: brand on the left (above the sidebar
            // column on lg+), page title in the middle, search +
            // notifications + user menu on the right.
            TopBar {
                title: props.title.clone(),
                on_menu_click: move |_| sidebar_open.set(true),
            }

            // Below the top bar: sidebar + main, side-by-side.
            div { class: "flex flex-1 overflow-hidden",
                // Mobile sidebar backdrop
                if *sidebar_open.read() {
                    div {
                        class: "fixed inset-0 z-40 bg-gray-600/75 lg:hidden", // theme-guard-allow: mobile nav overlay scrim
                        onclick: move |_| sidebar_open.set(false),
                    }
                }

                // Sidebar - nav only, no brand block.
                Sidebar {
                    open: *sidebar_open.read(),
                    onclose: move |_| sidebar_open.set(false),
                }

                // Main content - the only vertical scroll surface in the layout
                main { class: "flex-1 overflow-y-auto overscroll-contain py-6",
                    div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                        {props.children}
                    }
                }
            }

            // Mount the global toast surface once at the layout level
            // so any page or hook can push notifications without
            // wiring its own container.
            crate::hooks::toast::ToastRoot {}
        }
    }
}

/// Sidebar navigation
#[derive(Props, Clone, PartialEq)]
pub struct SidebarProps {
    open: bool,
    onclose: EventHandler<()>,
}

#[component]
pub fn Sidebar(props: SidebarProps) -> Element {
    let mobile_class = if props.open {
        "translate-x-0"
    } else {
        "-translate-x-full"
    };

    // MAPPS-250: outer-level rail collapse. App-root-owned so the choice
    // survives the per-navigation AppLayout re-mount, exactly like the
    // scroll offset and per-section collapse. Only the desktop rail honors
    // it; the mobile drawer is unchanged.
    let mut rail_collapsed = crate::hooks::use_sidebar_collapsed();
    let collapsed = rail_collapsed.read().0;
    // Expanded keeps today's lg:w-64; collapsed shrinks to a narrow
    // icon-only strip.
    let desktop_width = if collapsed { "lg:w-16" } else { "lg:w-64" };
    let toggle_glyph = if collapsed { "\u{203A}" } else { "\u{2039}" };
    let toggle_title = if collapsed {
        "Expand sidebar"
    } else {
        "Collapse sidebar"
    };

    rsx! {
        // Mobile sidebar drawer (full-height, slides in from left).
        // No brand block - the brand lives in the top bar now. The
        // mobile drawer overlaps the top bar so it gets its own close
        // button at the top.
        //
        // MAPPS-285: when the drawer is CLOSED, it is still in the DOM
        // and rendered off-canvas via `-translate-x-full` for the slide
        // animation. `lg:hidden` removes it from the accessibility tree
        // at the lg+ breakpoint (CSS `display: none`), but on smaller
        // viewports it is still semantically visible - a screen reader
        // tabs through every link, and the user hears the whole menu a
        // second time after the (visible) desktop sidebar at the
        // breakpoint edge. `aria_hidden` ties the AT-visibility to the
        // `open` state too so the drawer is exposed to AT only while
        // the user has actually opened it. Same pattern Material UI's
        // `Drawer` and HeadlessUI's `Dialog.Panel` use.
        aside {
            class: "fixed inset-y-0 left-0 z-50 w-64 bg-surface-2 border-r border-line transform transition-transform duration-300 ease-in-out flex flex-col lg:hidden {mobile_class}",
            aria_hidden: if props.open { "false" } else { "true" },
            div { class: "flex items-center justify-end h-12 px-2",
                button {
                    class: "p-2 text-subtle hover:text-content",
                    aria_label: "Close navigation",
                    title: "Close navigation",
                    onclick: move |_| props.onclose.call(()),
                    XMarkIcon { size: IconSize::Large }
                }
            }
            // Mobile drawer closes on every navigation (its open state
            // lives in the re-mounting AppLayout), so there is no scroll
            // position worth preserving here.
            SidebarContent { persist_scroll: false, collapsed: false }
        }

        // Desktop sidebar - sits below the top bar in the flex column
        // (no fixed positioning needed). Border-r separates it from
        // the main content. Uses `scrollbar-thin` (not `scrollbar-hide`)
        // so when the viewport is short enough that the nav list must
        // scroll, the user sees the affordance instead of silently
        // clipping behind the main panel's own scrollbar (PMC-22).
        aside { class: "hidden lg:flex {desktop_width} lg:flex-col bg-surface-2 border-r border-line overflow-y-auto overscroll-contain scrollbar-thin",
            // Outer rail collapse toggle. Mirrors the chevron-glyph
            // "Collapse panel" / "Expand panel" affordance from
            // CollapsibleRail (collapsible_rail.rs:48-63). Sits above the
            // nav so it stays put while the nav below scrolls.
            div {
                class: if collapsed { "flex justify-center px-1 py-2 shrink-0" } else { "flex justify-end px-2 py-2 shrink-0" },
                button {
                    class: "p-1 rounded-md text-subtle hover:text-content focus:outline-none",
                    aria_label: "{toggle_title}",
                    aria_expanded: if collapsed { "false" } else { "true" },
                    title: "{toggle_title}",
                    onclick: move |_| {
                        let now = rail_collapsed.read().0;
                        rail_collapsed.set(crate::hooks::SidebarCollapsed(!now));
                    },
                    "{toggle_glyph}"
                }
            }
            SidebarContent { persist_scroll: true, collapsed }
        }
    }
}

/// DOM id of the desktop sidebar's scroll container, used to restore and
/// record its scroll offset across re-mounts (MAPPS-203). Only the
/// persistent desktop instance carries this id, so `getElementById` is
/// unambiguous even though the mobile drawer renders the same component.
const SIDEBAR_NAV_ID: &str = "mokosh-sidebar-nav";

/// Read the current scroll offset of the sidebar nav from the DOM.
fn read_sidebar_scroll() -> Option<i32> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SIDEBAR_NAV_ID))
        .map(|el| el.scroll_top())
}

/// Restore a previously recorded scroll offset onto the sidebar nav.
fn restore_sidebar_scroll(top: i32) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SIDEBAR_NAV_ID))
    {
        el.set_scroll_top(top);
    }
}

#[component]
fn SidebarContent(persist_scroll: bool, collapsed: bool) -> Element {
    // Admin-only nav (audit log, SLA management): rendered only for
    // admin/super_admin users (reactive on sign-in). The pages re-check
    // server-side, so this is a UX affordance, not a security boundary.
    let auth = crate::hooks::use_auth();
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false);
    // Manager+ (manager/admin/super_admin) see the timesheet approvals queue,
    // matching the server's RequireManager gate on approve/reject (MAPPS-194).
    let can_manage = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_users())
        .unwrap_or(false);

    // MAPPS-203: App-root-owned scroll offset that survives the
    // AppLayout re-mount on each navigation. Only the persistent desktop
    // sidebar (`persist_scroll`) reads/writes it; the mobile drawer
    // closes on nav so it has nothing to preserve.
    let mut sidebar_scroll = crate::hooks::use_sidebar_scroll();
    // Carry the id only on the persistent instance so the two SidebarContent
    // mounts (mobile + desktop) never share a DOM id.
    let nav_id = if persist_scroll { SIDEBAR_NAV_ID } else { "" };

    rsx! {
        div { class: "flex flex-col flex-1 min-h-0",
            // The nav itself is the scroll container (flex-1 + min-h-0 +
            // overflow-y-auto) so the nav list scrolls when it is taller
            // than the sidebar - notably in the mobile drawer, whose
            // `aside` has no overflow of its own, where the lower groups
            // (Analytics / Admin) were otherwise unreachable. The footer
            // below stays pinned.
            nav {
                id: nav_id,
                // Collapsed: tighter horizontal padding so icons center in the
                // narrow strip; expanded keeps the original px-2.
                class: if collapsed { "flex-1 min-h-0 overflow-y-auto overscroll-contain scrollbar-thin px-1 py-4 space-y-1" } else { "flex-1 min-h-0 overflow-y-auto overscroll-contain scrollbar-thin px-2 py-4 space-y-1" },
                // On mount of the persistent desktop sidebar, jump straight
                // to the offset recorded before the last navigation so the
                // re-mount is invisible. `peek` so reading it here never
                // subscribes this component to its own scroll writes.
                onmounted: move |_| {
                    if persist_scroll {
                        let top = sidebar_scroll.peek().0;
                        if top != 0 {
                            restore_sidebar_scroll(top);
                        }
                    }
                },
                // Record every scroll so the next re-mount can restore it.
                onscroll: move |_| {
                    if persist_scroll {
                        if let Some(top) = read_sidebar_scroll() {
                            sidebar_scroll.set(crate::hooks::SidebarScroll(top));
                        }
                    }
                },
                NavItem { to: Route::Dashboard {}, icon: rsx!(HomeIcon {}), label: "Dashboard", collapsed }

            NavSection { title: "Service Desk", rail_collapsed: collapsed,
                NavItem { to: Route::TicketList {}, icon: rsx!(TicketIcon {}), label: "Tickets", collapsed }
                NavItem { to: Route::TimeEntryList {}, icon: rsx!(ClockIcon {}), label: "Time Entries", collapsed }
                NavItem { to: Route::Timesheets {}, icon: rsx!(DocumentIcon {}), label: "Timesheets", collapsed }
                if can_manage {
                    NavItem { to: Route::TimesheetApprovals {}, icon: rsx!(DocumentIcon {}), label: "Timesheet Approvals", collapsed }
                }
            }

            NavSection { title: "Projects", rail_collapsed: collapsed,
                NavItem { to: Route::ProjectList {}, icon: rsx!(FolderIcon {}), label: "Projects", collapsed }
            }

            NavSection { title: "CRM", rail_collapsed: collapsed,
                NavItem { to: Route::CompanyList {}, icon: rsx!(BuildingIcon {}), label: "Companies", collapsed }
                NavItem { to: Route::ContactList {}, icon: rsx!(UsersIcon {}), label: "Contacts", collapsed }
            }

            NavSection { title: "Operations", rail_collapsed: collapsed,
                NavItem { to: Route::Calendar {}, icon: rsx!(CalendarIcon {}), label: "Calendar", collapsed }
                NavItem { to: Route::DispatchBoard {}, icon: rsx!(TruckIcon {}), label: "Dispatch", collapsed }
                NavItem { to: Route::SchedulingTemplates {}, icon: rsx!(SwatchIcon {}), label: "Scheduling Templates", collapsed }
            }

            NavSection { title: "Contracts & Billing", rail_collapsed: collapsed,
                NavItem { to: Route::ContractList {}, icon: rsx!(DocumentIcon {}), label: "Contracts", collapsed }
                NavItem { to: Route::RateCardList {}, icon: rsx!(DocumentIcon {}), label: "Rate Cards", collapsed }
                NavItem { to: Route::InvoiceList {}, icon: rsx!(CurrencyIcon {}), label: "Invoices", collapsed }
                NavItem { to: Route::PaymentList {}, icon: rsx!(CurrencyIcon {}), label: "Payments", collapsed }
            }

            NavSection { title: "Assets", rail_collapsed: collapsed,
                NavItem { to: Route::AssetList {}, icon: rsx!(ServerIcon {}), label: "Assets", collapsed }
            }

            NavSection { title: "Knowledge", rail_collapsed: collapsed,
                NavItem { to: Route::KBHome {}, icon: rsx!(BookIcon {}), label: "Knowledge Base", collapsed }
            }

            NavSection { title: "Analytics", rail_collapsed: collapsed,
                NavItem { to: Route::Reports {}, icon: rsx!(ChartIcon {}), label: "Reports", collapsed }
            }

            if is_admin {
                NavSection { title: "Admin", rail_collapsed: collapsed,
                    // MAPPS-329: Team nav is hidden by default and only
                    // renders when the operator sets
                    // `MOKOSH_TEAM_ENABLED=true` (or `=1`) on the
                    // mokosh-www container. `Route::Team`, the page, and
                    // its API are intentionally left intact - typing
                    // `/team` still works - so flipping the flag is a
                    // zero-code unlock when the feature is ready.
                    if crate::modules::runtime_config::flag_enabled("team_enabled") {
                        NavItem { to: Route::Team {}, icon: rsx!(UsersIcon {}), label: "Team", collapsed }
                    }
                    NavItem { to: Route::AuditLog {}, icon: rsx!(DocumentIcon {}), label: "Audit Log", collapsed }
                    NavItem { to: Route::SlaManagement {}, icon: rsx!(DocumentIcon {}), label: "SLA Management", collapsed }
                    // MAPPS-169: single entry into the centralized Settings hub.
                    NavItem { to: Route::SettingsHome {}, icon: rsx!(CogIcon {}), label: "Settings", collapsed }
                }
            }

            }
            // Hide the build-info line in the narrow collapsed strip: it
            // wraps illegibly at lg:w-16. It returns when the rail expands.
            if !collapsed {
                VersionFooter {}
            }
        }
    }
}

/// Compact build-info line shown in every layout footer so support and
/// developers can identify the running build at a glance.
#[component]
fn VersionFooter() -> Element {
    use crate::utils::version::{BUILD_DATE, GIT_HASH, VERSION};
    rsx! {
        p {
            class: "px-3 py-2 text-xs text-muted text-center break-words",
            title: "{VERSION} commit {GIT_HASH} built {BUILD_DATE}",
            span { "{VERSION}" }
            span { class: "mx-1", "-" }
            span { class: "font-mono", "{GIT_HASH}" }
            span { class: "mx-1", "-" }
            span { "{BUILD_DATE}" }
        }
    }
}

/// Collapsible navigation section. Header is a clickable button with a
/// chevron that toggles the children's visibility. Open/closed state is
/// keyed by section title and lives in App-root context, so it persists
/// across SPA route changes (each navigation re-mounts AppLayout, but
/// the context signal sits at App level).
#[derive(Props, Clone, PartialEq)]
struct NavSectionProps {
    title: String,
    /// MAPPS-250: when the WHOLE rail is collapsed to the icon-only strip,
    /// the section header (title + per-section chevron) is hidden and the
    /// child icons render directly, since there is no room for the title and
    /// the per-section toggle is unreachable in the narrow strip. Defaults
    /// to false so the mobile drawer and expanded desktop rail keep the
    /// existing two-level header.
    #[props(default)]
    rail_collapsed: bool,
    children: Element,
}

#[component]
fn NavSection(props: NavSectionProps) -> Element {
    let mut state = crate::hooks::use_sidebar_state();
    let title = props.title.clone();
    let collapsed = crate::hooks::is_section_collapsed(&state.read(), &title);

    // Outer rail collapsed: drop the section header entirely and show the
    // child icons. The inner per-section collapse state is preserved (we
    // never touch it here) so re-expanding the rail restores each section's
    // prior open/closed state.
    if props.rail_collapsed {
        return rsx! {
            div { class: "pt-2 space-y-1",
                {props.children}
            }
        };
    }

    let toggle_title = title.clone();
    let toggle = move |_| {
        let mut s = state.write();
        let new_value = !s.collapsed.get(&toggle_title).copied().unwrap_or(false);
        s.collapsed.insert(toggle_title.clone(), new_value);
    };

    rsx! {
        div { class: "pt-4",
            button {
                class: "w-full flex items-center justify-between px-3 py-1 text-xs font-semibold text-subtle uppercase tracking-wider hover:text-content focus:outline-none",
                aria_expanded: if collapsed { "false" } else { "true" },
                onclick: toggle,
                span { "{props.title}" }
                // Chevron: pointing down when expanded, right when
                // collapsed. Keeps the affordance unambiguous.
                if collapsed {
                    ChevronRightIcon { size: IconSize::Small, class: "text-muted".to_string() }
                } else {
                    ChevronDownIcon { size: IconSize::Small, class: "text-muted".to_string() }
                }
            }
            if !collapsed {
                div { class: "mt-1 space-y-1",
                    {props.children}
                }
            }
        }
    }
}

/// Navigation item
#[derive(Props, Clone, PartialEq)]
struct NavItemProps {
    to: Route,
    icon: Element,
    label: String,
    /// MAPPS-250: render as an icon-only link (label hidden, label moved to
    /// the `title` tooltip) when the whole rail is collapsed. Defaults to
    /// false so the mobile drawer and expanded desktop rail show full labels.
    #[props(default)]
    collapsed: bool,
}

/// Map a route to the nav list it lives under, so a detail page keeps its
/// parent section highlighted (PMS-312). Routes without a list parent map to
/// themselves, preserving exact-match highlighting for top-level pages.
fn section_route(route: &Route) -> Route {
    match route {
        Route::TicketDetail { .. } => Route::TicketList {},
        Route::ProjectDetail { .. } => Route::ProjectList {},
        Route::CompanyDetail { .. } => Route::CompanyList {},
        Route::ContactDetail { .. } => Route::ContactList {},
        Route::ContractDetail { .. } => Route::ContractList {},
        Route::RateCardDetail { .. } => Route::RateCardList {},
        // `/rate-cards/new` renders the list page with the create modal open,
        // so it stays under the Rate Cards section (MAPPS-217).
        Route::RateCardNew { .. } => Route::RateCardList {},
        Route::InvoiceDetail { .. } => Route::InvoiceList {},
        Route::AssetDetail { .. } => Route::AssetList {},
        Route::KBArticleDetail { .. } => Route::KBHome {},
        Route::ReportDetail { .. } => Route::Reports {},
        other => other.clone(),
    }
}

#[component]
fn NavItem(props: NavItemProps) -> Element {
    // Highlight the item whose `to` matches the current route, treating a
    // detail page as its parent list (PMS-312) so e.g. RateCardDetail keeps
    // "Rate Cards" highlighted.
    let current_route: Route = use_route();
    let is_active = section_route(&current_route) == props.to;

    // Collapsed rail: icon-only link, centered in the narrow strip, with the
    // label exposed as a `title` tooltip for discoverability (AC4). The link
    // stays a working router `Link` so navigation works from the strip.
    if props.collapsed {
        let class = if is_active {
            "group flex items-center justify-center px-2 py-2 rounded-md bg-surface text-content border-l-2 border-accent"
        } else {
            "group flex items-center justify-center px-2 py-2 rounded-md text-muted hover:bg-surface hover:text-content"
        };
        return rsx! {
            Link {
                to: props.to,
                class: "{class}",
                title: "{props.label}",
                aria_label: "{props.label}",
                span { class: "text-subtle group-hover:text-content",
                    {props.icon}
                }
            }
        };
    }

    let class = if is_active {
        "group flex items-center px-3 py-2 text-sm font-medium rounded-md bg-surface text-content border-l-2 border-accent"
    } else {
        "group flex items-center px-3 py-2 text-sm font-medium rounded-md text-muted hover:bg-surface hover:text-content"
    };

    rsx! {
        Link {
            to: props.to,
            class: "{class}",
            span { class: "mr-3 text-subtle group-hover:text-content",
                {props.icon}
            }
            "{props.label}"
        }
    }
}

/// Persistent top bar.
///
/// Brand sits on the left, occupying the same column-width as the
/// sidebar below (lg:w-64) so the visual L of brand + sidebar feels
/// continuous. Page title middle, search + notifications + user menu
/// right. Single h-16 strip across the full viewport width.
#[derive(Props, Clone, PartialEq)]
pub struct TopBarProps {
    #[props(default)]
    title: String,
    on_menu_click: EventHandler<()>,
}

#[component]
pub fn TopBar(props: TopBarProps) -> Element {
    let auth = crate::hooks::use_auth();
    let active_org = auth.read().active_org_name().map(str::to_string);
    rsx! {
        header { class: "h-16 flex items-center bg-surface-2 border-b border-line shrink-0 z-20",
            // Brand block - same width as the sidebar below it.
            // Includes the mobile hamburger so the brand area also opens
            // the drawer on small screens.
            div { class: "flex items-center h-full lg:w-64 px-4 lg:px-6 border-r border-line",
                button {
                    class: "lg:hidden p-2 mr-2 rounded-md text-subtle hover:text-content hover:bg-surface-2",
                    aria_label: "Open navigation",
                    title: "Open navigation",
                    onclick: move |_| props.on_menu_click.call(()),
                    MenuIcon { size: IconSize::Large }
                }
                Link {
                    to: Route::Dashboard {},
                    class: "flex items-center gap-2 min-w-0",
                    img {
                        src: asset!("/assets/icon-192.png"),
                        alt: "Mokosh",
                        class: "h-8 w-8 shrink-0",
                    }
                    div { class: "flex flex-col leading-tight min-w-0",
                        span { class: "text-base font-bold text-content truncate",
                            "Mokosh Platform"
                        }
                        // Active-org indicator. Hidden until auth resolves an
                        // active tenant - we don't show a "no org" state in
                        // the brand slot to avoid a confusing flash on load.
                        if let Some(name) = active_org.as_deref() {
                            span { class: "text-xs text-subtle truncate", "{name}" }
                        }
                    }
                }
            }

            // Page title + global search. Title gets a fixed slot on
            // lg+ so the search box doesn't push it around; on smaller
            // viewports the title takes the row and search hides.
            div { class: "flex-1 px-4 sm:px-6 lg:px-8 min-w-0 flex items-center gap-4",
                h1 { class: "text-xl font-semibold text-content truncate shrink-0",
                    "{props.title}"
                }
                // MAPPS-298: top-bar global search. Mounted at lg+ so
                // the search field has room next to the page title;
                // narrow viewports keep just the title.
                div { class: "hidden lg:flex flex-1 max-w-md",
                    GlobalSearch {}
                }
            }

            // Right side actions
            div { class: "flex items-center px-4 sm:px-6 lg:px-8 space-x-4",
                // Theme + accent picker (MAPPS-259), opens a centered modal.
                ThemePickerButton {}

                // Notifications bell + inbox dropdown, wired to the
                // server `notifications` module (MAPPS-132).
                NotificationBell {}

                // PMS-486: pending-approvals chip. Polls
                // `/approvals/pending` and renders a count badge that
                // links to the standalone /approvals queue.
                ApprovalsBadge {}

                // User menu (P3-26 avatar dropdown)
                UserMenu {}
            }
        }
    }
}

/// Avatar-button + dropdown for the top-bar user menu. P3-26 from the
/// audit: previously the avatar was a plain button with no dropdown,
/// leaving users hunting for Logout / Profile.
#[component]
fn UserMenu() -> Element {
    let mut open = use_signal(|| false);
    // No `mut auth` binding here on purpose. `use_auth` is the read-only
    // hook; logout deliberately does NOT mutate the auth signal (see the
    // ordering comment on the `logout` closure below).
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    // Two distinct entries:
    //   - "Profile" -> mokosh's own `/profile` page, edits the
    //     tenant-scoped user row (name, title, phone, mobile,
    //     timezone). Internal route, uses `Link`.
    //   - "Account Settings" -> bunyip-web's `/settings`, edits the
    //     cross-app identity (email, password, MFA, sessions,
    //     billing). External origin, uses `<a>` so the browser
    //     actually navigates instead of resolving the URL against
    //     this SPA's Route enum.
    let hub_account_settings = cfg.hub_url("/settings");
    // MAPPS-324: the "Apps" launcher link to the bunyip hub dashboard is
    // hidden for now (multi-app switching isn't a flow we want to surface
    // yet). Restore by reinstating this binding and the `a` block below.
    // let hub_dashboard = cfg.hub_url("/dashboard");
    // RP-initiated logout via bunyip-api's `OptionalUser`-backed
    // endpoint. bunyip-api's `GET /v1/auth/logout?url=<absolute>`
    // clears the .a8n.systems-scoped cookies via Set-Cookie, then
    // 302s the browser straight to `url`. The companion bunyip change
    // (fix/logout-honors-final-url) replaced the old "bounce through
    // /login" handler with a direct redirect, so this URL is now the
    // user's actual landing page after logout.
    //
    // Land them on this SPA's own origin root (msp.<tld>/) so they
    // see mokosh's public landing page, signed out. Falls back to
    // the hub root when the browser origin is somehow unavailable
    // (server-side render path, mostly unreachable in `web` builds).
    let issuer = cfg.issuer.trim_end_matches('/');
    let post_logout_target = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .map(|origin| format!("{}/", origin.trim_end_matches('/')))
        .unwrap_or_else(|| cfg.hub_url("/"));
    let hub_logout = format!(
        "{issuer}/v1/auth/logout?url={}",
        js_sys::encode_uri_component(&post_logout_target)
            .as_string()
            .unwrap_or(post_logout_target)
    );

    let logout = move |_| {
        open.set(false);
        // Order matters here: any write to the auth signal BEFORE
        // `location.replace` schedules
        // a Dioxus re-render. On that re-render `use_require_auth` (the
        // route guard) sees `user = None` on `/dashboard` and calls
        // `navigator.push(Route::Login {})`, which puts `/login` on TOP
        // of `/dashboard` in history. The subsequent `location.replace`
        // then races with the router push; the user ends up navigated
        // away from the hub logout URL and back onto an authenticated-
        // looking dashboard view. So: clear sessionStorage, navigate
        // away, let the full page reload reset all in-memory state.
        //
        // MAPPS-336: revoke the refresh token family at the OP BEFORE
        // wiping local sessionStorage. The local clear alone left the
        // rotated-but-not-revoked family alive until natural expiry, so
        // a leaked / stolen refresh token survived the user clicking
        // "Log out". `revoke_refresh_token` is fire-and-forget per RFC
        // 7009 (`/oauth2/revoke` returns 200 for unknown tokens too) so
        // a transient network failure doesn't block the local cleanup.
        // The await runs on a fresh task so the closure stays sync.
        if let Some(tokens) = crate::modules::oidc::storage::load_auth() {
            if let Some(refresh) = tokens.refresh_token {
                let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
                spawn(async move {
                    let _ = crate::modules::oidc::flow::revoke_refresh_token(&cfg, &refresh).await;
                });
            }
        }
        crate::modules::oidc::storage::clear_auth();
        if let Some(win) = web_sys::window() {
            let _ = win.location().replace(&hub_logout);
        }
    };

    rsx! {
        div { class: "relative",
            button {
                class: "flex items-center focus:outline-none",
                aria_label: "User menu",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                UserCircleIcon { size: IconSize::Large, class: "text-subtle".to_string() }
            }
            if *open.read() {
                div {
                    class: "absolute right-0 mt-2 w-52 rounded-md shadow-lg bg-raised ring-1 ring-black ring-opacity-5 z-20 p-1",
                    role: "menu",
                    // Profile is a mokosh-side route, served by this
                    // SPA. Use the router `Link` so the SPA does an
                    // internal transition instead of a full reload.
                    Link {
                        to: Route::Profile {},
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        onclick: move |_| open.set(false),
                        "Profile"
                    }
                    // Account Settings lives on the bunyip hub;
                    // cross-origin top-level <a> so the browser
                    // navigates instead of resolving against this
                    // SPA's Route enum.
                    a {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        href: "{hub_account_settings}",
                        "Account Settings"
                    }
                    // MAPPS-324: "Apps" launcher link removed (hidden for
                    // now); see the commented `hub_dashboard` binding above.
                    // System Status is a mokosh-side route (build
                    // versions + live API/dependency health), so route
                    // internally with `Link` (PMS-237).
                    Link {
                        to: Route::SystemStatus {},
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        onclick: move |_| open.set(false),
                        "System Status"
                    }
                    div { class: "border-t border-line my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-surface-2",
                        onclick: logout,
                        "Logout"
                    }
                }
            }
        }
    }
}

/// One in-app inbox item from `GET /api/v1/notifications`.
///
/// Mirrors the server `NotificationInboxItemResponse`, decoding only
/// the fields the dropdown renders. The server already filters to the
/// `in_app` channel and orders newest-first, so the client just shows
/// the list as received.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct NotificationItem {
    id: uuid::Uuid,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Envelope for the paginated inbox response (`{ data, meta }`); the
/// bell only needs the `data` array.
#[derive(Clone, Debug, serde::Deserialize)]
struct NotificationPage {
    #[serde(default)]
    data: Vec<NotificationItem>,
}

/// Render a UTC instant in the viewer's local timezone, honouring the
/// per-user `date_format_string` preference when set (PMS-253).
///
/// Thin wrapper around [`crate::utils::datetime::format_user_datetime`]:
/// pulls the active user's format pref off the AuthContext, then
/// delegates. The instant is rendered in the user's profile timezone
/// (MAPPS-208); users without a format preference get a locale
/// rendering still pinned to that timezone.
fn format_local_datetime(dt: chrono::DateTime<chrono::Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    crate::utils::datetime::format_user_datetime(dt, pref.as_deref())
}

/// Top-bar notification bell with an inbox dropdown.
///
/// Fetches the in-app inbox on mount (and after each mark-read) so the
/// red unread dot reflects real state instead of the old hard-coded
/// stub. Clicking an unread item POSTs `.../{id}/read` and refetches.
#[component]
fn NotificationBell() -> Element {
    let mut open = use_signal(|| false);
    // `use_resource` runs on mount and whenever `.restart()` is called
    // (after marking an item read). A failed fetch degrades to an empty
    // inbox rather than surfacing an error in the top bar.
    let mut inbox = use_resource(|| async {
        // Subscribe to the active-tenant generation so the inbox
        // refetches when the user switches org (notifications are
        // tenant-scoped), matching the dashboard/list pages.
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<NotificationPage>("/notifications")
            .await
            .ok()
            .map(|page| page.data)
            .unwrap_or_default()
    });

    let items = inbox.read_unchecked().clone().unwrap_or_default();
    let unread = items.iter().filter(|i| i.read_at.is_none()).count();

    rsx! {
        div { class: "relative",
            button {
                r#type: "button",
                aria_label: "Notifications",
                title: "Notifications",
                class: "p-2 rounded-full text-subtle hover:text-content hover:bg-surface-2 relative",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                BellIcon {}
                // Red dot only when something is actually unread. MAPPS-261:
                // the dot's meaning must not rely on color alone, so it carries
                // an sr-only textual equivalent that names the unread count.
                if unread > 0 {
                    span { class: "absolute top-1 right-1 block h-2 w-2 rounded-full bg-red-400",
                        span { class: "sr-only", "{unread} unread notifications" }
                    }
                }
            }
            if *open.read() {
                // Full-viewport click-catcher behind the panel. Sits
                // below the panel's z-index so any click outside the
                // dropdown (including a second click on the bell) closes
                // it; clicks on the panel itself land above this and are
                // unaffected.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 mt-2 w-80 max-h-96 overflow-y-auto rounded-md shadow-lg bg-raised ring-1 ring-black ring-opacity-5 z-20",
                    role: "menu",
                    div { class: "px-4 py-2 border-b border-line text-sm font-semibold text-content",
                        "Notifications"
                    }
                    if items.is_empty() {
                        div { class: "px-4 py-6 text-sm text-muted text-center",
                            "No notifications yet"
                        }
                    } else {
                        for item in items.iter().cloned() {
                            NotificationRow { item, on_read: move |_| inbox.restart() }
                        }
                    }
                }
            }
        }
    }
}

/// PMS-486: top-bar pending-approvals chip. Polls `/approvals/pending`
/// on mount + on every active-org switch (cheap on the server thanks
/// to the PMS-451 partial indexes on `(approver_user_id) WHERE state =
/// 'pending'` etc.). The badge collapses to nothing when there is no
/// pending decision so the chrome stays clean for non-approvers.
#[component]
fn ApprovalsBadge() -> Element {
    let inbox = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Vec<serde_json::Value>>("/approvals/pending")
            .await
            .ok()
            .unwrap_or_default()
    });
    let count = inbox.read_unchecked().clone().unwrap_or_default().len();
    if count == 0 {
        return rsx! { span {} };
    }
    rsx! {
        Link {
            to: Route::Approvals {},
            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900/40 dark:text-yellow-200 hover:opacity-90",
            aria_label: "{count} pending approvals",
            title: "Pending approvals",
            "Approvals "
            span { class: "font-bold", "{count}" }
        }
    }
}

/// A single inbox row. Unread rows are tinted and, on click, POST a
/// mark-read then ask the parent to refetch via `on_read`.
#[component]
fn NotificationRow(item: NotificationItem, on_read: EventHandler<()>) -> Element {
    let is_unread = item.read_at.is_none();
    let id = item.id;
    let subject = item.subject.clone().unwrap_or_default();
    let when = format_local_datetime(item.created_at);
    let unread_bg = if is_unread {
        "bg-accent-50 dark:bg-accent-900/40"
    } else {
        ""
    };

    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left px-4 py-3 border-b border-line hover:bg-surface-2 {unread_bg}",
            onclick: move |_| {
                if is_unread {
                    spawn(async move {
                        let _ = crate::hooks::fetch::api::post_authed_no_content(
                                &format!("/notifications/{id}/read"),
                            )
                            .await;
                        on_read.call(());
                    });
                }
            },
            if !subject.is_empty() {
                div { class: "text-sm font-medium text-content", "{subject}" }
            }
            div { class: "text-sm text-muted", "{item.body}" }
            div { class: "mt-1 text-xs text-subtle", "{when}" }
        }
    }
}

/// Portal layout (simpler, for client portal)
#[derive(Props, Clone, PartialEq)]
pub struct PortalLayoutProps {
    children: Element,
    #[props(default)]
    title: String,
}

#[component]
pub fn PortalLayout(props: PortalLayoutProps) -> Element {
    rsx! {
        div { class: "min-h-screen bg-app",
            // Portal header
            header { class: "bg-surface shadow",
                div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                    div { class: "flex items-center justify-between h-16",
                        // Logo
                        Link {
                            to: Route::PortalHome {},
                            class: "flex items-center",
                            span { class: "text-xl font-bold text-accent",
                                "Client Portal"
                            }
                        }

                        // Navigation
                        nav { class: "hidden md:flex space-x-8",
                            Link {
                                to: Route::PortalTicketList {},
                                class: "text-content hover:text-accent",
                                "Tickets"
                            }
                            Link {
                                to: Route::PortalInvoiceList {},
                                class: "text-content hover:text-accent",
                                "Invoices"
                            }
                            Link {
                                to: Route::PortalKB {},
                                class: "text-content hover:text-accent",
                                "Knowledge Base"
                            }
                        }

                        // User menu
                        div { class: "flex items-center",
                            PortalUserMenu {}
                        }
                    }
                }
            }

            // Main content
            main { class: "py-10",
                div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                    if !props.title.is_empty() {
                        h1 { class: "text-2xl font-bold text-content mb-6",
                            "{props.title}"
                        }
                    }
                    {props.children}
                }
            }

            // Portal footer
            footer { class: "bg-surface border-t border-line",
                div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6",
                    p { class: "text-sm text-muted text-center",
                        "Powered by Mokosh Platform"
                    }
                    VersionFooter {}
                }
            }
        }
    }
}

/// Portal top-bar user menu: avatar button + dropdown. The portal header
/// previously rendered a bare avatar icon with no way to reach account
/// settings or sign out (MAPPS-140); this mirrors the app-side
/// [`UserMenu`] with the entries that apply to a portal (client) user.
#[component]
fn PortalUserMenu() -> Element {
    let mut open = use_signal(|| false);
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    // Account Settings lives on the bunyip hub (cross-origin identity:
    // email, password, MFA), so it is a top-level `<a>` rather than an
    // in-SPA `Link`.
    let hub_account_settings = cfg.hub_url("/settings");
    // RP-initiated logout, identical to the app-side `UserMenu`: bunyip's
    // `GET /v1/auth/logout?url=<absolute>` clears the shared cookies and
    // 302s back to this SPA's origin root, signed out.
    let issuer = cfg.issuer.trim_end_matches('/');
    let post_logout_target = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .map(|origin| format!("{}/", origin.trim_end_matches('/')))
        .unwrap_or_else(|| cfg.hub_url("/"));
    let hub_logout = format!(
        "{issuer}/v1/auth/logout?url={}",
        js_sys::encode_uri_component(&post_logout_target)
            .as_string()
            .unwrap_or(post_logout_target)
    );

    let logout = move |_| {
        open.set(false);
        // Same ordering rule as `UserMenu::logout`: clear storage, then
        // hard-navigate so the full reload resets in-memory auth state
        // without a router re-render racing the redirect.
        crate::modules::oidc::storage::clear_auth();
        if let Some(win) = web_sys::window() {
            let _ = win.location().replace(&hub_logout);
        }
    };

    rsx! {
        div { class: "relative",
            button {
                class: "flex items-center focus:outline-none",
                aria_label: "User menu",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                UserCircleIcon { size: IconSize::Large, class: "text-subtle".to_string() }
            }
            if *open.read() {
                div {
                    class: "absolute right-0 mt-2 w-52 rounded-md shadow-lg bg-raised ring-1 ring-black ring-opacity-5 z-20 p-1",
                    role: "menu",
                    a {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        href: "{hub_account_settings}",
                        "Account Settings"
                    }
                    div { class: "border-t border-line my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-surface-2",
                        onclick: logout,
                        "Logout"
                    }
                }
            }
        }
    }
}

/// Auth layout (login, signup, password reset)
#[derive(Props, Clone, PartialEq)]
pub struct AuthLayoutProps {
    children: Element,
}

#[component]
pub fn AuthLayout(props: AuthLayoutProps) -> Element {
    rsx! {
        div { class: "min-h-screen flex flex-col justify-center py-12 sm:px-6 lg:px-8 bg-app",
            div { class: "sm:mx-auto sm:w-full sm:max-w-md",
                // Logo
                div { class: "flex flex-col items-center gap-3",
                    img {
                        src: asset!("/assets/icon-192.png"),
                        alt: "Mokosh",
                        class: "h-16 w-16",
                    }
                    span { class: "text-3xl font-bold text-accent",
                        "Mokosh Platform"
                    }
                }
            }

            div { class: "mt-8 sm:mx-auto sm:w-full sm:max-w-md",
                div { class: "bg-surface py-8 px-4 shadow sm:rounded-lg sm:px-10",
                    {props.children}
                }
            }

            div { class: "mt-6 sm:mx-auto sm:w-full sm:max-w-md",
                VersionFooter {}
            }
        }
    }
}

/// Page header with title and actions
#[derive(Props, Clone, PartialEq)]
pub struct PageHeaderProps {
    title: String,
    #[props(default)]
    subtitle: String,
    actions: Option<Element>,
    breadcrumbs: Option<Element>,
}

#[component]
pub fn PageHeader(props: PageHeaderProps) -> Element {
    rsx! {
        div { class: "mb-6",
            if let Some(ref breadcrumbs) = props.breadcrumbs {
                div { class: "mb-2",
                    {breadcrumbs}
                }
            }
            div { class: "md:flex md:items-center md:justify-between",
                div { class: "min-w-0 flex-1",
                    // `leading-7` (28px) was paired with `sm:text-3xl`
                    // (30px font) plus `sm:truncate` (overflow:hidden),
                    // which clipped the descenders of g/j/p/q/y on every
                    // page title at sm and up. Bump line-height to
                    // `leading-9` (36px) at the same breakpoint as the
                    // larger font so descenders sit inside the line box.
                    h2 { class: "text-2xl font-bold leading-7 text-content sm:truncate sm:text-3xl sm:leading-9 sm:tracking-tight",
                        "{props.title}"
                    }
                    if !props.subtitle.is_empty() {
                        p { class: "mt-1 text-sm text-muted",
                            "{props.subtitle}"
                        }
                    }
                }
                if let Some(ref actions) = props.actions {
                    // `shrink-0` so the action cluster keeps its width and
                    // the title (min-w-0 + sm:truncate above) is what gives
                    // way on a tight row, instead of the buttons collapsing
                    // into or colliding with a long dynamic title.
                    div { class: "mt-4 flex shrink-0 md:ml-4 md:mt-0 space-x-3",
                        {actions}
                    }
                }
            }
        }
    }
}

/// Breadcrumb navigation
#[derive(Clone, PartialEq)]
pub struct BreadcrumbItem {
    pub label: String,
    pub route: Option<Route>,
}

#[derive(Props, Clone, PartialEq)]
pub struct BreadcrumbsProps {
    items: Vec<BreadcrumbItem>,
}

#[component]
pub fn Breadcrumbs(props: BreadcrumbsProps) -> Element {
    rsx! {
        nav { class: "flex", aria_label: "Breadcrumb",
            ol { class: "flex items-center space-x-2",
                for (i, item) in props.items.iter().enumerate() {
                    li { class: "flex items-center",
                        if i > 0 {
                            ChevronRightIcon { size: IconSize::Small, class: "text-subtle mx-2".to_string() }
                        }
                        if let Some(route) = &item.route {
                            Link {
                                to: route.clone(),
                                class: "text-sm font-medium text-muted hover:text-content",
                                "{item.label}"
                            }
                        } else {
                            span { class: "text-sm font-medium text-content",
                                "{item.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Empty state component
#[derive(Props, Clone, PartialEq)]
pub struct EmptyStateProps {
    title: String,
    #[props(default)]
    description: String,
    icon: Option<Element>,
    actions: Option<Element>,
}

#[component]
pub fn EmptyState(props: EmptyStateProps) -> Element {
    rsx! {
        div { class: "text-center py-12",
            if let Some(ref icon) = props.icon {
                div { class: "mx-auto h-12 w-12 text-subtle",
                    {icon}
                }
            }
            h3 { class: "mt-2 text-sm font-semibold text-content",
                "{props.title}"
            }
            if !props.description.is_empty() {
                p { class: "mt-1 text-sm text-muted",
                    "{props.description}"
                }
            }
            if let Some(ref actions) = props.actions {
                div { class: "mt-6",
                    {actions}
                }
            }
        }
    }
}

//! Layout components

use dioxus::prelude::*;

use super::global_search::GlobalSearch;
use super::icons::*;
use super::tenant_switcher::TenantSwitcher;
use super::theme_picker::ThemePickerButton;
/// MAPPS-518: the sessionStorage key where `/platform/login` stashes
/// the platform-admin bearer (mirrors
/// `pages::platform_login::PLATFORM_TOKEN_KEY`).
const PLATFORM_TOKEN_KEY: &str = "mokosh:platform_token";

/// MAPPS-518: is the platform-admin bearer present in sessionStorage?
/// Used to gate the Tenants nav item (and any other UI that requires
/// a platform-admin session, distinct from a tenant admin session).
fn platform_bearer_present() -> bool {
    #[cfg(feature = "web")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(Some(store)) = win.session_storage() {
                if let Ok(Some(token)) = store.get_item(PLATFORM_TOKEN_KEY) {
                    return !token.trim().is_empty();
                }
            }
        }
    }
    false
}

use crate::modules::theme::SectionColor;
use crate::Route;

// ---------------------------------------------------------------------------
// MAPPS-366: persistent application shell.
//
// Before MAPPS-366 an `AppLayout` component wrapped each page individually, so
// every SPA navigation re-mounted the whole shell (top bar, sidebar, banners)
// and the screen visibly repainted - the text blanked on the user<->admin
// dashboard switch while the cached SVG icons did not. `AppShell` is that same
// chrome hoisted into a Dioxus `#[layout]`: it renders once, stays mounted
// across navigations, and swaps only the routed subtree through `Outlet`. Pages
// render their body directly and set their title via `use_page_title` instead
// of an `AppLayout { title }` prop.
// ---------------------------------------------------------------------------

/// The current page's title, provided at the App root and set by each page's
/// body via [`use_page_title`]. A newtype so the context lookup cannot collide
/// with any other `String` signal placed at the root.
#[derive(Clone, PartialEq, Default)]
pub struct PageTitle(pub String);

/// Provide the page-title signal at the App root. Mirrors
/// [`crate::hooks::use_sidebar_provider`]; mount once in `App`.
pub fn use_page_title_provider() -> Signal<PageTitle> {
    let state = use_signal(PageTitle::default);
    use_context_provider(|| state);
    state
}

/// Read the current page title inside the persistent [`AppShell`].
pub fn use_current_page_title() -> Signal<PageTitle> {
    use_context::<Signal<PageTitle>>()
}

/// Set the current page's title from a page body. Replaces the old
/// `AppLayout { title: "..." }` prop: the persistent shell reads the same
/// signal for the top bar and `document.title`. Writes only when the value
/// changes so it never loops on the render it runs in; a page that swaps a
/// "Loading..." placeholder for the real record name just calls it again with
/// the new value.
pub fn use_page_title(title: impl Into<String>) {
    let mut sig = use_context::<Signal<PageTitle>>();
    let title = title.into();
    // `peek` so reading the current value here never subscribes this caller
    // to its own write.
    if sig.peek().0 != title {
        sig.set(PageTitle(title));
    }
}

/// MAPPS-366: the persistent app shell, mounted as a Dioxus `#[layout]` so it
/// survives navigation. Holds the chrome (top bar, sidebar, banners, toast +
/// account-deleted overlays) and renders the routed page through `Outlet`, so
/// navigation swaps only the routed subtree while the chrome stays mounted.
///
/// It deliberately does NOT read the page-title signal: `TopBar` reads it
/// itself, so a page's `use_page_title` write re-renders only the bar, never
/// this shell or the routed page below (which would otherwise loop when a page
/// and its `ContentUnavailable` / `PermissionRequired` branch set the title).
#[component]
pub fn AppShell() -> Element {
    let mut sidebar_open = use_signal(|| false);

    rsx! {
        div { class: "h-screen flex flex-col bg-app overflow-hidden",
            super::ServerStatusBanner {}
            super::UpdateBanner {}
            TopBar {
                on_menu_click: move |_| sidebar_open.set(true),
            }
            div { class: "flex flex-1 overflow-hidden",
                if *sidebar_open.read() {
                    div {
                        class: "fixed inset-0 z-40 bg-gray-600/75 lg:hidden", // theme-guard-allow: mobile nav overlay scrim
                        onclick: move |_| sidebar_open.set(false),
                    }
                }
                Sidebar {
                    open: *sidebar_open.read(),
                    onclose: move |_| sidebar_open.set(false),
                }
                // The routed page renders here. Only this subtree swaps on
                // navigation; the chrome above and below stays mounted.
                main { class: "flex-1 overflow-y-auto overscroll-contain py-6",
                    div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                        Outlet::<crate::Route> {}
                    }
                }
            }
            crate::hooks::toast::ToastRoot {}
            super::AccountDeletedOverlay {}
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
        // (no fixed positioning needed). MAPPS-346: the collapse toggle
        // moved out of an in-sidebar row (which left dead space under the
        // top bar) onto a half-circle handle straddling the right border,
        // and the sidebar's scrollbar is hidden (`scrollbar-hide`) so the
        // rail reads as a clean surface. The relative wrapper is NOT
        // clipped so the handle can protrude past the border; the aside
        // inside owns the (hidden) scroll.
        div { class: "relative hidden lg:flex {desktop_width} shrink-0 lg:flex-col transition-[width] duration-200 ease-in-out",
            aside { class: "flex-1 min-h-0 flex flex-col bg-surface-2 border-r border-line overflow-y-auto overscroll-contain scrollbar-hide",
                SidebarContent { persist_scroll: true, collapsed }
            }
            // Half-circle collapse handle on the right border. The chevron
            // points the way the click moves the rail: right to expand when
            // collapsed, left (rotate-180) to collapse when open.
            button {
                class: "absolute top-1/2 right-0 -translate-y-1/2 translate-x-full z-30 flex h-10 w-5 items-center justify-center rounded-r-full border border-l-0 border-line bg-surface-2 text-subtle shadow-sm hover:text-content focus:outline-none",
                aria_label: "{toggle_title}",
                aria_expanded: if collapsed { "false" } else { "true" },
                title: "{toggle_title}",
                onclick: move |_| {
                    let now = rail_collapsed.read().0;
                    rail_collapsed.set(crate::hooks::SidebarCollapsed(!now));
                },
                svg {
                    class: if collapsed { "w-4 h-4" } else { "w-4 h-4 rotate-180" },
                    xmlns: "http://www.w3.org/2000/svg",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke_width: "2",
                    stroke: "currentColor",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "m8.25 4.5 7.5 7.5-7.5 7.5",
                    }
                }
            }
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

/// MAPPS-358: while mokosh-server is unreachable, the sidebar collapses to
/// just the Dashboard entry - every other destination renders the
/// [`crate::components::ContentUnavailable`] "cannot connect" body during an
/// outage, so offering their links only invites dead-end navigation. The
/// Dashboard is the one coherent place to keep the user, matching the
/// "return to dashboard" affordance on the outage page.
///
/// Returns whether the full section list (everything below the always-present
/// Dashboard link) should render. Factored out as a pure function so the
/// down / recovery transitions are unit-testable without a renderer, mirroring
/// [`crate::hooks::classify_remote`].
pub(crate) fn full_nav_visible(server_reachable: bool) -> bool {
    server_reachable
}

#[component]
fn SidebarContent(persist_scroll: bool, collapsed: bool) -> Element {
    // MAPPS-358: reading the reachability flag subscribes this sidebar, so it
    // collapses to Dashboard-only the instant the server goes down and
    // restores every section the instant the MAPPS-333 recovery poll flips the
    // flag back. `use_server_reachable` is `true` on non-web builds.
    let show_full_nav = full_nav_visible(crate::hooks::use_server_reachable());

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
    // MAPPS-447 (revised for MAPPS-518): the Tenants nav opens the
    // platform-admin console. Post MAPPS-518 the server gates
    // `POST /api/v1/tenants` (and the other tenant-management
    // endpoints) on `RequirePlatformAdmin` (a `typ="platform"` JWT
    // from /platform/login), NOT on `users.role = 'super_admin'`.
    // Gate the nav item on the presence of the platform bearer in
    // sessionStorage so the item only appears once the operator has
    // signed in on the platform plane.
    let is_platform_admin = platform_bearer_present();
    // PMS-791 phase 2 / MAPPS-463: Teams nav is org-tenants-only per Q4
    // default. Personal tenants (`kind='personal'`, single owner) hide
    // the item entirely; a personal tenant hitting /admin/teams directly
    // sees a ContentUnavailable page.
    let is_org_tenant = auth.read().is_org_tenant();

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
                // Collapsed: flex column that keeps today's tight gap-0.5 as the
                // floor and uses justify-between to spread the icons across any
                // spare height. When the rail overflows, space-between degrades to
                // flex-start (no top clipping) so it just scrolls at the minimum
                // density. Expanded keeps the original block + space-y-1.
                class: if collapsed { "flex-1 min-h-0 overflow-y-auto overscroll-contain scrollbar-hide flex flex-col justify-between gap-0.5 px-1 pt-1 pb-2" } else { "flex-1 min-h-0 overflow-y-auto overscroll-contain scrollbar-hide px-2 pt-1 pb-4 space-y-1" },
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

            // MAPPS-358: every section below is hidden while the server is
            // unreachable, leaving Dashboard as the only navigable
            // destination. The links return the instant the recovery poll
            // marks the server reachable again.
            if show_full_nav {

            NavSection { title: "Service Desk", rail_collapsed: collapsed, color: SectionColor::Blue,
                NavItem { to: Route::TicketList {}, icon: rsx!(TicketIcon {}), label: "Tickets", collapsed }
                NavItem { to: Route::TimeEntryList {}, icon: rsx!(ClockIcon {}), label: "Time Entries", collapsed }
                NavItem { to: Route::Timesheets {}, icon: rsx!(DocumentIcon {}), label: "Timesheets", collapsed }
                if can_manage {
                    NavItem { to: Route::TimesheetApprovals {}, icon: rsx!(DocumentCheckIcon {}), label: "Timesheet Approvals", collapsed }
                }
            }

            NavSection { title: "Projects", rail_collapsed: collapsed, color: SectionColor::Indigo,
                NavItem { to: Route::ProjectList {}, icon: rsx!(FolderIcon {}), label: "Projects", collapsed }
            }

            NavSection { title: "CRM", rail_collapsed: collapsed, color: SectionColor::Cyan,
                NavItem { to: Route::CompanyList {}, icon: rsx!(BuildingIcon {}), label: "Companies", collapsed }
                NavItem { to: Route::ContactList {}, icon: rsx!(UsersIcon {}), label: "Contacts", collapsed }
            }

            NavSection { title: "Operations", rail_collapsed: collapsed, color: SectionColor::Emerald,
                NavItem { to: Route::Calendar {}, icon: rsx!(CalendarIcon {}), label: "Calendar", collapsed }
                NavItem { to: Route::DispatchBoard {}, icon: rsx!(TruckIcon {}), label: "Dispatch", collapsed }
                NavItem { to: Route::SchedulingTemplates {}, icon: rsx!(SwatchIcon {}), label: "Scheduling Templates", collapsed }
            }

            NavSection { title: "Contracts & Billing", rail_collapsed: collapsed, color: SectionColor::Amber,
                NavItem { to: Route::ContractList {}, icon: rsx!(ScaleIcon {}), label: "Contracts", collapsed }
                NavItem { to: Route::QuoteList {}, icon: rsx!(DocumentIcon {}), label: "Quotes", collapsed }
                NavItem { to: Route::RateCardList {}, icon: rsx!(TagIcon {}), label: "Rate Cards", collapsed }
                NavItem { to: Route::InvoiceList {}, icon: rsx!(CurrencyIcon {}), label: "Invoices", collapsed }
                NavItem { to: Route::PaymentList {}, icon: rsx!(CreditCardIcon {}), label: "Payments", collapsed }
            }

            NavSection { title: "Assets", rail_collapsed: collapsed, color: SectionColor::Teal,
                NavItem { to: Route::AssetList {}, icon: rsx!(ServerIcon {}), label: "Assets", collapsed }
            }

            NavSection { title: "Knowledge", rail_collapsed: collapsed, color: SectionColor::Fuchsia,
                NavItem { to: Route::KBHome {}, icon: rsx!(BookIcon {}), label: "Knowledge Base", collapsed }
            }

            NavSection { title: "Analytics", rail_collapsed: collapsed, color: SectionColor::Rose,
                NavItem { to: Route::Reports {}, icon: rsx!(ChartIcon {}), label: "Reports", collapsed }
            }

            // MAPPS-520 walkthrough: the platform super-admin has its
            // OWN nav section (Tenants) that renders whenever a
            // platform bearer is present in sessionStorage. Split out
            // of the tenant "Admin" section below so a pure platform
            // admin (users row deleted by migration 133; no
            // tenant-plane `AuthContext`) still sees the top-level
            // action they own instead of a completely empty admin
            // area. The section renders WITHOUT gating on
            // `is_admin` (which reads `auth.user.role.is_admin()` and
            // is false when there is no users row at all).
            if is_platform_admin {
                NavSection { title: "Platform", rail_collapsed: collapsed, color: SectionColor::Violet,
                    // MAPPS-447 / MAPPS-518: only the platform-admin
                    // persona (`platform_admins` row + `/login`
                    // bearer) can create / suspend / edit tenants
                    // ("client portals" in the mokosh vocabulary).
                    // Server side is gated on `RequirePlatformAdmin`;
                    // this nav item mirrors the same gate.
                    TenantsNavItem { visible: true, collapsed }
                }
            }

            // Tenant-scoped admin surface (Teams, Invitations, Audit
            // Log, Request Forms, SLA, Settings). Renders when
            // EITHER the signed-in `users` row carries an admin-ish
            // role OR the caller holds a platform bearer.
            //
            // Pre-MAPPS-518 the mokosh super-admin was a
            // `users.role='super_admin'` row and these items were
            // part of their nav; post-518 the persona moved into
            // `platform_admins` and the super-admin's tenant users
            // row was deleted by migration 133, which hid the whole
            // section for a pure platform admin. Adding the
            // platform-admin gate here restores that visibility so
            // the operator sees the full super-admin surface they
            // had before the split.
            //
            // Individual items behind here still call tenant-scoped
            // endpoints that authenticate against the tenant
            // `AuthContext`, not the platform bearer. A pure platform
            // admin who does not also hold a tenant admin users row
            // will see the items but may hit an "auth required"
            // screen after navigating. Teaching each admin route to
            // accept a platform bearer (mirroring the MAPPS-518
            // `TenantOrPlatformCaller` pattern already on the 5
            // dual-check tenant handlers) is tracked separately.
            if is_admin || is_platform_admin {
                NavSection { title: "Admin", rail_collapsed: collapsed, color: SectionColor::Violet,
                    // PMS-791 phase 2: Teams (was "Team", which was
                    // actually the invitations page — see the
                    // Invitations item below). Org tenants only per Q4
                    // default = A. The `team_enabled` runtime flag was
                    // retired: Teams is now core, not a preview.
                    TeamsNavItem { visible: is_org_tenant, collapsed }
                    NavItem { to: Route::Invitations {}, icon: rsx!(UserGroupIcon {}), label: "Invitations", collapsed }
                    NavItem { to: Route::AuditLog {}, icon: rsx!(ClipboardDocumentListIcon {}), label: "Audit Log", collapsed }
                    NavItem { to: Route::FormsBuilder {}, icon: rsx!(ClipboardDocumentListIcon {}), label: "Request Forms", collapsed }
                    NavItem { to: Route::SlaManagement {}, icon: rsx!(ShieldCheckIcon {}), label: "SLA Management", collapsed }
                    // MAPPS-169: single entry into the centralized Settings hub.
                    NavItem { to: Route::SettingsHome {}, icon: rsx!(CogIcon {}), label: "Settings", collapsed }
                }
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

/// Compact version indicator in the layout footer. Shows the release version
/// alone (PMS-712) and links through to System Status, which carries the full
/// build metadata - commit hash and build date for both the client and the API.
/// David's call on the 2026-07-31 standup: the version number is the part useful
/// to a general user (and the fastest way to confirm which build someone is on),
/// while the build detail belongs on the status page, its agreed single home -
/// so it is moved there rather than duplicated into a footer tooltip.
#[component]
fn VersionFooter() -> Element {
    use crate::utils::version::VERSION;
    rsx! {
        p {
            class: "px-3 py-2 text-xs text-muted text-center",
            Link {
                to: Route::SystemStatus {},
                class: "hover:text-content transition-colors",
                title: "View system status and build details",
                "v{VERSION}"
            }
        }
    }
}

/// MAPPS-359: the accent hue for the category a [`NavItem`] sits in.
/// Provided by the enclosing [`NavSection`] and consumed by [`NavItem`] to
/// tint its icon, so a whole category reads as one color family (Google
/// Cloud console style) applied as an accent - the icon and the section
/// header - rather than flooding the row. A newtype so it never collides
/// with any other `SectionColor` placed in context.
#[derive(Clone, Copy, PartialEq)]
struct NavCategoryColor(SectionColor);

/// Collapsible navigation section. Header is a clickable button with a
/// chevron that toggles the children's visibility. Open/closed state is
/// keyed by section title and lives in App-root context, so it persists
/// across SPA route changes (each navigation re-mounts AppLayout, but
/// the context signal sits at App level).
#[derive(Props, Clone, PartialEq)]
struct NavSectionProps {
    title: String,
    /// MAPPS-359: this category's accent hue. Colors the section header and
    /// is handed down to each child [`NavItem`] via context to tint its
    /// icon.
    color: SectionColor,
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
    // MAPPS-359: hand this category's hue down to every child NavItem so its
    // icon is tinted with the category color. Provided unconditionally (a
    // hook) before any early return, so the collapsed-rail branch below
    // still colors its icons.
    use_context_provider(|| NavCategoryColor(props.color));
    let title = props.title.clone();
    let collapsed = crate::hooks::is_section_collapsed(&state.read(), &title);

    // Outer rail collapsed: drop the section header entirely and show the
    // child icons. The inner per-section collapse state is preserved (we
    // never touch it here) so re-expanding the rail restores each section's
    // prior open/closed state.
    if props.rail_collapsed {
        // MAPPS-346: `display: contents` so this section's icons become
        // direct flex children of the nav. That lets the nav's
        // justify-between distribute every icon with one consistent gap
        // instead of spacing the section groups apart.
        return rsx! {
            div { class: "contents",
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

    // MAPPS-359: tint the header label with the category hue (accent), with a
    // lighter dark-mode shade. Hover still lifts to the primary content color
    // so the toggle affordance stays obvious.
    let header_color = props.color.heading_class();

    rsx! {
        div { class: "pt-4",
            button {
                class: "w-full flex items-center justify-between px-3 py-1 text-xs font-semibold {header_color} uppercase tracking-wider hover:text-content focus:outline-none",
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
        // Quote detail / editor keep the Quotes item highlighted.
        Route::QuoteDetail { .. } => Route::QuoteList {},
        Route::QuoteEdit { .. } => Route::QuoteList {},
        Route::QuoteNew { .. } => Route::QuoteList {},
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

    // MAPPS-359: tint the icon with the enclosing category's accent hue
    // (provided by NavSection). Items rendered outside any NavSection
    // (Dashboard) have no NavCategoryColor in context, so they fall back to
    // the neutral subtle icon that lifts to the content color on hover.
    let icon_color = try_use_context::<NavCategoryColor>()
        .map(|c| c.0.heading_class())
        .unwrap_or("text-subtle group-hover:text-content");

    // Collapsed rail: icon-only link, centered in the narrow strip, with the
    // label exposed as a `title` tooltip for discoverability (AC4). The link
    // stays a working router `Link` so navigation works from the strip.
    if props.collapsed {
        let class = if is_active {
            "group flex items-center justify-center px-2 py-1 rounded-md bg-surface text-content border-l-2 border-accent"
        } else {
            "group flex items-center justify-center px-2 py-1 rounded-md text-muted hover:bg-surface hover:text-content"
        };
        return rsx! {
            Link {
                to: props.to,
                class: "{class}",
                title: "{props.label}",
                aria_label: "{props.label}",
                span { class: "{icon_color}",
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
            span { class: "mr-3 {icon_color}",
                {props.icon}
            }
            "{props.label}"
        }
    }
}

/// MAPPS-447: sidebar entry that opens the tenant roster (and its
/// Create-tenant modal). Hoisted into its own component so it can be
/// cfg-gated on `multi-tenant`: `Route::TenantManagement` only exists
/// in that build, and referencing it from the always-compiled sidebar
/// would break the `single-tenant` binary. The stub at the bottom of
/// this pair keeps the call site identical across features.
#[derive(Props, Clone, PartialEq)]
struct TenantsNavItemProps {
    visible: bool,
    collapsed: bool,
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TenantsNavItem(props: TenantsNavItemProps) -> Element {
    if !props.visible {
        return rsx! {};
    }
    rsx! {
        NavItem {
            to: Route::TenantManagement {},
            icon: rsx!(BuildingIcon {}),
            // MAPPS-547: the mokosh super-admin manages "Clients" here
            // (MSPs paying for the platform). "Tenant" is schema jargon
            // (`tenants` table, `tenants.kind`); the operator-facing
            // label is "Client". Route + component + Rust type names
            // (Route::TenantManagement, TenantManagementPage) stay
            // unchanged so bookmarks and internal code references
            // still work.
            label: "Clients",
            collapsed: props.collapsed,
        }
    }
}

#[cfg(not(feature = "multi-tenant"))]
#[component]
fn TenantsNavItem(props: TenantsNavItemProps) -> Element {
    let _ = props;
    rsx! {}
}

/// PMS-791 phase 2 / MAPPS-463: Teams nav item. Uses the same cfg-gated
/// pattern as TenantsNavItem so `single-tenant` builds do not need to
/// know Route::Teams exists.
#[derive(Props, Clone, PartialEq)]
struct TeamsNavItemProps {
    visible: bool,
    collapsed: bool,
}

#[cfg(feature = "multi-tenant")]
#[component]
fn TeamsNavItem(props: TeamsNavItemProps) -> Element {
    if !props.visible {
        return rsx! {};
    }
    rsx! {
        NavItem {
            to: Route::Teams {},
            icon: rsx!(UserGroupIcon {}),
            label: "Teams",
            collapsed: props.collapsed,
        }
    }
}

#[cfg(not(feature = "multi-tenant"))]
#[component]
fn TeamsNavItem(props: TeamsNavItemProps) -> Element {
    let _ = props;
    rsx! {}
}

/// Persistent top bar.
///
/// Brand sits on the left, occupying the same column-width as the
/// sidebar below (lg:w-64) so the visual L of brand + sidebar feels
/// continuous. Page title middle, search + notifications + user menu
/// right. Single h-16 strip across the full viewport width.
#[derive(Props, Clone, PartialEq)]
pub struct TopBarProps {
    on_menu_click: EventHandler<()>,
}

#[component]
pub fn TopBar(props: TopBarProps) -> Element {
    let auth = crate::hooks::use_auth();
    let active_org = auth.read().active_org_name().map(str::to_string);
    // MAPPS-366: the page title lives in a shared signal each page sets via
    // use_page_title. Reading it HERE (in TopBar, a child of AppShell) rather
    // than in AppShell means a title change re-renders only the top bar, not
    // the Outlet/page - so a page and its ContentUnavailable / PermissionRequired
    // branch can both call use_page_title without fighting into a render loop.
    let title = use_current_page_title().read().0.clone();
    // MAPPS-287: keep document.title in sync. Both loading placeholders
    // ("Loading…" U+2026 and ASCII "Loading...") read as "no title yet" so the
    // tab shows a clean "Mokosh Platform" until the real title arrives.
    #[cfg(feature = "web")]
    {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let t = title.trim();
            let next = if t.is_empty() || t == "Loading…" || t == "Loading..." {
                "Mokosh Platform".to_string()
            } else {
                format!("{} | Mokosh Platform", t)
            };
            doc.set_title(&next);
        }
    }
    rsx! {
        header { class: "h-16 flex items-center bg-surface-2 border-b border-line shrink-0 z-20",
            // Brand block - same width as the sidebar below it.
            // Includes the mobile hamburger so the brand area also opens
            // the drawer on small screens.
            div { class: "flex items-center h-full lg:w-64 px-4 lg:px-6",
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

            // Page title. MAPPS-346: the global search moved out to the
            // action cluster (it now collapses to an icon), freeing this
            // slot so the title can center across the bar.
            div { class: "flex-1 px-4 sm:px-6 lg:px-8 min-w-0 flex items-center justify-center",
                h1 { class: "text-xl font-semibold text-content truncate",
                    "{title}"
                }
            }

            // Right side actions
            div { class: "flex items-center px-4 sm:px-6 lg:px-8 space-x-4",
                // MAPPS-346: global search, collapsed to a magnifier icon
                // that expands the text entry leftward. Sits to the left of
                // the theme picker.
                GlobalSearch {}

                // Theme + accent picker (MAPPS-259), opens a centered modal.
                ThemePickerButton {}

                // Notifications bell + inbox dropdown, wired to the
                // server `notifications` module (MAPPS-132).
                NotificationBell {}

                // PMS-486: pending-approvals chip. Polls
                // `/approvals/pending` and renders a count badge that
                // links to the standalone /approvals queue.
                ApprovalsBadge {}

                // MAPPS-494 (MAPPS-474 phase 5): tenant switcher.
                // Dropdown listing every membership the identity holds
                // + a "Create new organization" action.
                TenantSwitcher {}

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
    // MAPPS-384: dismiss the dropdown on navigation. UserMenu lives in the
    // persistent AppShell (MAPPS-366), so a route change re-renders the routed
    // subtree WITHOUT re-mounting this component; without this the menu would
    // stay open across screens. `use_reactive!` re-runs the effect whenever the
    // current route changes, and only then.
    let route: Route = use_route();
    use_effect(use_reactive!(|route| {
        // `route` is the reactive dependency: every navigation changes it and
        // re-fires this effect to close the menu. `peek` reads `open` without
        // subscribing the effect to it (which would defeat the purpose).
        let _ = &route;
        if *open.peek() {
            open.set(false);
        }
    }));
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
    // MAPPS-520 walkthrough fix: with no bunyip issuer configured
    // (standalone dev / self-hosted no-OP mode), `issuer` is the empty
    // string and the format below collapsed to `/v1/auth/logout?url=...`
    // - a same-origin path the browser hit on localhost:4301 and the
    // SPA rendered as a 404 (no client route matches). Fall back to
    // the SPA login URL directly when there is no OP to bounce
    // through; the local session clear below is what actually signs
    // the user out in standalone mode.
    let hub_logout = if issuer.is_empty() {
        "/login".to_string()
    } else {
        format!(
            "{issuer}/v1/auth/logout?url={}",
            js_sys::encode_uri_component(&post_logout_target)
                .as_string()
                .unwrap_or(post_logout_target)
        )
    };

    let logout = move |_| {
        open.set(false);
        // Order matters here: any write to the auth signal BEFORE
        // `location.replace` schedules
        // a Dioxus re-render. On that re-render `use_require_auth` (the
        // route guard) sees `user = None` on `/dashboard` and calls
        // `navigator.push(Route::Login {})`, which puts /client/login
        // (post MAPPS-518 URL swap; was /login) on TOP of `/dashboard`
        // in history. The subsequent `location.replace`
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
            // MAPPS-384: match the sibling top-bar icons (theme picker /
            // notification bell, MAPPS-359 surface tokens) so the profile
            // control highlights on hover and carries a tooltip. `title` is the
            // hover/focus tooltip; `aria_label` the accessible name. IconButton
            // was considered but its `rounded-md` + blue focus-ring base would
            // visually diverge from the `rounded-full` top-bar icons this is
            // meant to sit beside, so matching the sibling convention wins.
            button {
                r#type: "button",
                class: "p-2 rounded-full text-subtle hover:text-content hover:bg-surface-2 focus:outline-none",
                aria_label: "User menu",
                title: "User menu",
                aria_expanded: if open() { "true" } else { "false" },
                aria_haspopup: "menu",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                // No color class on the icon: it inherits `currentColor` from
                // the button (`text-subtle`, `hover:text-content`) so it
                // brightens on hover like the sibling top-bar icons. Pinning
                // `text-subtle` here would override the button's hover color and
                // leave the icon looking dead on hover (MAPPS-384 follow-up).
                UserCircleIcon { size: IconSize::Large }
            }
            if *open.read() {
                // MAPPS-384: full-screen outside-click backdrop, same pattern as
                // GlobalSearch (MAPPS-346). Sits below the dropdown (z-10 < z-20)
                // so menu entries stay clickable while any click elsewhere hits
                // this and dismisses. It unmounts with the dropdown, so there is
                // no document-level listener that could leak.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 mt-2 w-52 rounded-md shadow-lg bg-raised ring-1 ring-black/5 z-20 p-1",
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
                    // MAPPS-497 item 1: create-org lives here too so a
                    // single-membership identity (switcher trigger
                    // hidden) can still start a new org from the top
                    // bar. Same global signal the switcher dropdown
                    // uses; the modal itself is mounted inside
                    // TenantSwitcher and reacts to the signal.
                    button {
                        r#type: "button",
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        onclick: move |_| {
                            *crate::components::tenant_switcher::SHOW_CREATE_ORG.write() = true;
                            open.set(false);
                        },
                        "Create new organization"
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
                    class: "absolute right-0 mt-2 w-80 max-h-96 overflow-y-auto rounded-md shadow-lg bg-raised ring-1 ring-black/5 z-20",
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
                        if let Err(err) = crate::hooks::fetch::api::post_authed_no_content(
                                &format!("/notifications/{id}/read"),
                            )
                            .await
                        {
                            tracing::warn!("failed to mark notification {id} read: {err}");
                        }
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
    // Nudge the shared branding fetch on mount so a customer who lands
    // on `/portal/tickets` directly (bypassing the login page) still
    // sees the MSP name / logo in the header. Idempotent; on a non-
    // portal host (`on_portal_host()` false) it is a no-op.
    #[cfg(feature = "web")]
    use_hook(crate::hooks::portal_branding::ensure_portal_branding_fetch);
    let hint = crate::hooks::portal_branding::use_portal_host_hint();

    // PMS-729 follow-up: fetch the authenticated portal contact so the
    // user menu can render the customer's name instead of a bare icon.
    // Idempotent + one-shot latched; no-op for the login-page path where
    // there is no session yet.
    #[cfg(feature = "web")]
    use_hook(crate::hooks::portal_me::ensure_portal_me_fetch);

    // PMS-729 phase 2 H2: mount the auto-refresh background loop for
    // every authenticated portal session. Rotates the access + refresh
    // token pair every ~12 min so a 15-min access token never lapses
    // while the customer is active. Unmounts on navigation to
    // /portal/login (this component's parent guard bounces there when
    // has_portal_session goes false).
    crate::hooks::portal_auth::use_portal_auto_refresh();

    // PMS-729 phase 2 §6 slice 3: portal-scoped theme. Applies on boot
    // and re-applies when the OS `prefers-color-scheme` flips for the
    // `System` branch. Independent of the agent-side theme, keyed
    // under `mokosh_portal_theme`.
    #[cfg(feature = "web")]
    crate::hooks::portal_theme::use_apply_portal_theme();

    // PMS-729 phase 2 §6 slice 4: sidebar drawer open state. Same
    // pattern as `AppShell`: a full-height mobile drawer that slides
    // in from the left, a persistent rail on md:+ that shows the
    // portal nav all the time. Every route change closes the drawer
    // (the sidebar itself closes on link click via `onclose`).
    let mut sidebar_open = use_signal(|| false);
    // Dismiss the mobile drawer whenever the route changes so the
    // customer does not have to close it manually after tapping a
    // link. `use_reactive!` re-runs only when the route identity
    // shifts, and `peek` reads the current open state without
    // subscribing this effect back to itself.
    let route: Route = use_route();
    use_effect(use_reactive!(|route| {
        let _ = &route;
        if *sidebar_open.peek() {
            sidebar_open.set(false);
        }
    }));

    // Prefer the tenant name in the wordmark; fall back to the generic
    // "Client Portal" copy when the branding hint has not landed yet
    // (or the SPA is on a non-portal host).
    let brand_label = hint
        .as_ref()
        .map(|h| h.name.clone())
        .unwrap_or_else(|| "Client Portal".to_string());
    let hint_for_logo = hint.clone();
    let hint_for_footer = hint.clone();

    rsx! {
        div { class: "h-screen flex flex-col bg-app overflow-hidden print:h-auto print:overflow-visible",
            div { class: "no-print",
                super::ServerStatusBanner {}
            }

            // Portal top bar. Hamburger (mobile only) opens the
            // sidebar drawer. Brand block on the left; theme toggle +
            // user menu on the right. Page title stays inline in the
            // main region (via PortalPageHeader), not the top bar, so
            // a customer's eye lands on the H1 next to the primary
            // action rather than up in the chrome.
            header { class: "h-16 flex items-center bg-surface-2 border-b border-line shrink-0 z-20 no-print",
                // PMS-729 follow-up: the brand block used to be capped at
                // `md:w-56` (224px), which truncated any tenant whose logo
                // plus name pushed past that cap (e.g. the ACME fixture's
                // 128px wordmark logo + "Acme MSP" spilled into "Ac..."
                // ellipsis). Drop the width cap so the block auto-sizes to
                // its content; the `flex-1` spacer below absorbs the rest.
                div { class: "flex items-center h-full px-4 md:px-6 gap-2",
                    button {
                        class: "md:hidden p-2 mr-2 rounded-md text-subtle hover:text-content hover:bg-surface",
                        aria_label: "Open navigation",
                        title: "Open navigation",
                        onclick: move |_| sidebar_open.set(true),
                        MenuIcon { size: IconSize::Large }
                    }
                    Link {
                        to: Route::PortalHome {},
                        class: "flex items-center gap-2 min-w-0",
                        // Dark-aware logo, if the tenant supplied one.
                        {
                            let is_dark = crate::hooks::portal_theme::current_is_dark();
                            let logo = hint_for_logo
                                .as_ref()
                                .and_then(|h| h.branding.logo_for(is_dark).map(str::to_string));
                            let alt = brand_label.clone();
                            rsx! {
                                if let Some(url) = logo {
                                    img {
                                        src: "{url}",
                                        alt: "{alt}",
                                        class: "h-8 w-auto shrink-0",
                                    }
                                }
                            }
                        }
                        // No `truncate` here now that the brand block
                        // auto-sizes. `whitespace-nowrap` keeps the label
                        // on one line if the sidebar rail is narrow.
                        span { class: "text-lg font-bold text-accent whitespace-nowrap",
                            "{brand_label}"
                        }
                    }
                }
                div { class: "flex-1" }
                div { class: "flex items-center px-4 md:px-6 gap-2",
                    // PMS-729 phase 2 §7 slice B / I12: inbox bell.
                    PortalInboxBell {}
                    PortalThemeToggle {}
                    PortalUserMenu {}
                }
            }

            div { class: "flex flex-1 overflow-hidden",
                // Mobile overlay dims the page while the drawer is open.
                if *sidebar_open.read() {
                    div {
                        class: "fixed inset-0 z-40 bg-gray-600/75 md:hidden", // theme-guard-allow: mobile nav overlay scrim
                        onclick: move |_| sidebar_open.set(false),
                    }
                }
                div { class: "no-print contents",
                    PortalSidebar {
                        open: *sidebar_open.read(),
                        onclose: move |_| sidebar_open.set(false),
                    }
                }
                // Main content region. `overflow-y-auto` scrolls
                // independently of the sidebar (which never scrolls with
                // only four items). Same max-w-7xl inner container the
                // pre-slice-4 layout used so page bodies keep their
                // spacing without every page having to re-set it.
                main { class: "flex-1 overflow-y-auto overscroll-contain py-8 print:overflow-visible print:py-0",
                    div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                        if !props.title.is_empty() {
                            PortalPageHeader { title: props.title.clone() }
                        }
                        {props.children}

                        // Portal footer sits INSIDE the scroll region
                        // so it appears at the natural bottom of the
                        // page content, not as a fixed strip that
                        // shifts the layout. Mirrors the AppShell
                        // pattern of chrome-inside-scroll.
                        footer { class: "mt-12 border-t border-line pt-6 no-print",
                            {
                                let footer = hint_for_footer
                                    .as_ref()
                                    .and_then(|h| h.branding.footer_text.clone())
                                    .unwrap_or_else(|| "Powered by Mokosh Platform".to_string());
                                rsx! {
                                    p { class: "text-sm text-muted text-center", "{footer}" }
                                }
                            }
                            VersionFooter {}
                        }
                    }
                }
            }
            crate::hooks::toast::ToastRoot {}
        }
    }
}

/// PMS-729 phase 2 §6 slice 4: portal page header primitive. Rendered
/// automatically by [`PortalLayout`] when the `title` prop is set, and
/// exported for pages that want to attach a description or primary
/// action alongside the title.
///
/// The API deliberately mirrors the agent-side pattern of "H1 on the
/// left, primary action on the right" so a portal page reads as one
/// visual family with the agent shell (D8: parity with the agent SPA).
#[derive(Props, Clone, PartialEq)]
pub struct PortalPageHeaderProps {
    title: String,
    #[props(default)]
    description: Option<String>,
    #[props(default)]
    action: Option<Element>,
}

#[component]
pub fn PortalPageHeader(props: PortalPageHeaderProps) -> Element {
    // Also write the title into the shared page-title signal (used by
    // `document.title` in the top bar) so a portal page participates
    // in the same title convention as the agent-side pages.
    use_page_title(props.title.clone());

    rsx! {
        div { class: "mb-6 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between",
            div { class: "min-w-0",
                h1 { class: "text-2xl font-bold text-content truncate",
                    "{props.title}"
                }
                if let Some(desc) = &props.description {
                    p { class: "mt-1 text-sm text-muted", "{desc}" }
                }
            }
            if let Some(action) = props.action {
                div { class: "shrink-0", {action} }
            }
        }
    }
}

/// PMS-729 phase 2 §6 slice 4: portal sidebar. Same mobile-drawer /
/// desktop-rail split as [`Sidebar`], but with the flat portal nav
/// (Home, Tickets, Invoices, Knowledge Base) instead of the agent's
/// grouped Service Desk / CRM / Contracts sections.
///
/// Two-mode layout: mobile drawer (fixed, slides in from left,
/// `md:hidden`) and persistent desktop rail (`hidden md:flex`). Both
/// render the same nav rows so a link's affordance is identical on
/// either viewport. There is deliberately NO collapse toggle: four
/// entries fit comfortably in a `w-56` rail on any laptop, and the
/// added toggle chrome would double the primitives for a
/// four-destination nav.
#[derive(Props, Clone, PartialEq)]
pub struct PortalSidebarProps {
    open: bool,
    onclose: EventHandler<()>,
}

#[component]
pub fn PortalSidebar(props: PortalSidebarProps) -> Element {
    let mobile_class = if props.open {
        "translate-x-0"
    } else {
        "-translate-x-full"
    };

    rsx! {
        // Mobile drawer: full-height, slides in from left, includes
        // its own close button because it overlaps the top bar.
        aside {
            class: "fixed inset-y-0 left-0 z-50 w-64 bg-surface-2 border-r border-line transform transition-transform duration-300 ease-in-out flex flex-col md:hidden {mobile_class}",
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
            PortalSidebarContent {}
        }

        // Desktop rail: sits below the top bar in the flex column.
        // `md:w-56` matches the top bar's brand block so the visual L
        // (brand + sidebar) is aligned.
        aside {
            class: "hidden md:flex md:w-56 shrink-0 flex-col bg-surface-2 border-r border-line",
            PortalSidebarContent {}
        }
    }
}

#[component]
fn PortalSidebarContent() -> Element {
    rsx! {
        nav { class: "flex-1 min-h-0 overflow-y-auto overscroll-contain px-2 pt-3 pb-4 space-y-1",
            PortalNavItem { to: Route::PortalHome {}, icon: rsx!(HomeIcon {}), label: "Home" }
            PortalNavItem { to: Route::PortalTicketList {}, icon: rsx!(TicketIcon {}), label: "Tickets" }
            PortalNavItem { to: Route::PortalInvoiceList {}, icon: rsx!(CurrencyIcon {}), label: "Invoices" }
            // MAPPS-565: previously hidden; only reachable by typing the URL.
            // The Quotes page renders an accept/decline flow when a quote is
            // available; hiding it from the sidebar meant customers never
            // saw pending quotes.
            PortalNavItem { to: Route::PortalQuoteList {}, icon: rsx!(DocumentIcon {}), label: "Quotes" }
            PortalNavItem { to: Route::PortalKB {}, icon: rsx!(BookIcon {}), label: "Knowledge Base" }
            PortalNavItem { to: Route::PortalFormList {}, icon: rsx!(ClipboardDocumentListIcon {}), label: "Forms" }
            // PMS-729 phase 2 §7 slice C.
            PortalNavItem { to: Route::PortalAssetList {}, icon: rsx!(ServerIcon {}), label: "Assets" }
            PortalNavItem { to: Route::PortalContractList {}, icon: rsx!(ScaleIcon {}), label: "Contracts" }
            PortalNavItem { to: Route::PortalTimeEntryList {}, icon: rsx!(ClockIcon {}), label: "Time entries" }
            PortalNavItem { to: Route::PortalProjectList {}, icon: rsx!(FolderIcon {}), label: "Projects" }
            // PMS-729 phase 2 §7 slice D.
            PortalNavItem { to: Route::PortalApprovalList {}, icon: rsx!(DocumentCheckIcon {}), label: "Approvals" }
            PortalNavItem { to: Route::PortalCompany {}, icon: rsx!(UserGroupIcon {}), label: "Company" }
            PortalNavItem { to: Route::PortalExport {}, icon: rsx!(DocumentIcon {}), label: "Data export" }
            PortalNavItem { to: Route::PortalSearch {}, icon: rsx!(MagnifyingGlassIcon {}), label: "Search" }
            // MAPPS-565: previously hidden. The bell in the header opens a
            // dropdown but the full inbox at /portal/notifications was
            // only reachable via URL. Sidebar entry so a customer looking
            // for their notification history can find it.
            PortalNavItem { to: Route::PortalNotifications {}, icon: rsx!(BellIcon {}), label: "Notifications" }
            // MAPPS-565: previously hidden. The user menu in the header
            // links to Settings, but sidebar discoverability matters
            // when the menu is closed.
            PortalNavItem { to: Route::PortalSettings {}, icon: rsx!(CogIcon {}), label: "Settings" }
        }
    }
}

/// Portal nav row. Simpler than the agent-side [`NavItem`]: no
/// per-section accent, no collapsed-rail branch, no detail-page
/// parent mapping. Highlights when the current route matches exactly
/// (the portal has no detail<->list nesting to keep highlighted).
#[derive(Props, Clone, PartialEq)]
struct PortalNavItemProps {
    to: Route,
    icon: Element,
    label: String,
}

#[component]
fn PortalNavItem(props: PortalNavItemProps) -> Element {
    let current: Route = use_route();
    let is_active = current == props.to;
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

// PMS-729 phase 2 §7 slice B / I12: portal inbox surfaces ------------------

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
struct PortalInboxPayload {
    #[serde(default)]
    notifications: Vec<PortalInboxRow>,
    #[serde(default)]
    unread_count: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PortalInboxRow {
    id: uuid::Uuid,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Portal-scoped inbox bell. Fetches `/portal/notifications` on mount
/// and after every mark-read; renders a red dot when the server-side
/// unread_count > 0.
#[component]
fn PortalInboxBell() -> Element {
    let mut open = use_signal(|| false);
    let mut inbox = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_portal_authed::<PortalInboxPayload>("/portal/notifications")
            .await
            .ok()
            .unwrap_or_default()
    });
    let payload = inbox.read_unchecked().clone().unwrap_or_default();

    rsx! {
        div { class: "relative",
            button {
                r#type: "button",
                aria_label: "Notifications",
                title: "Notifications",
                class: "p-2 rounded-full text-subtle hover:text-content hover:bg-surface relative",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                BellIcon {}
                if payload.unread_count > 0 {
                    span { class: "absolute top-1 right-1 block h-2 w-2 rounded-full bg-red-400",
                        span { class: "sr-only", "{payload.unread_count} unread notifications" }
                    }
                }
            }
            if *open.read() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 mt-2 w-80 max-h-96 overflow-y-auto rounded-md shadow-lg bg-raised ring-1 ring-black/5 z-20",
                    role: "menu",
                    div { class: "px-4 py-2 border-b border-line text-sm font-semibold text-content",
                        "Notifications"
                    }
                    if payload.notifications.is_empty() {
                        div { class: "px-4 py-6 text-sm text-muted text-center",
                            "No notifications yet"
                        }
                    } else {
                        for row in payload.notifications.iter().cloned() {
                            PortalInboxRowView {
                                row,
                                on_read: move |_| inbox.restart(),
                            }
                        }
                    }
                    // "See all" footer takes the customer to the full
                    // paginated inbox at /portal/notifications, where
                    // older rows (beyond the bell's first-page snapshot)
                    // are reachable.
                    div { class: "px-4 py-2 border-t border-line text-center bg-raised sticky bottom-0",
                        Link {
                            to: Route::PortalNotifications {},
                            class: "text-sm text-accent hover:opacity-80",
                            onclick: move |_| open.set(false),
                            "See all notifications"
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PortalInboxRowViewProps {
    row: PortalInboxRow,
    on_read: EventHandler<()>,
}

#[component]
fn PortalInboxRowView(props: PortalInboxRowViewProps) -> Element {
    let is_unread = props.row.read_at.is_none();
    let id = props.row.id;
    let subject = props.row.subject.clone().unwrap_or_default();
    let body = props.row.body.clone();
    let when = props
        .row
        .created_at
        .map(|d| crate::utils::datetime::format_user_datetime(d, None))
        .unwrap_or_default();
    let unread_bg = if is_unread {
        "bg-accent-50 dark:bg-accent-900/40"
    } else {
        ""
    };
    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left px-4 py-3 border-b border-line hover:bg-surface {unread_bg}",
            onclick: move |_| {
                if is_unread {
                    spawn(async move {
                        let path = format!("/portal/notifications/{id}/read");
                        let _ = crate::hooks::fetch::api::put_portal_authed_no_content(&path).await;
                        props.on_read.call(());
                    });
                }
            },
            if !subject.is_empty() {
                div { class: "text-sm font-medium text-content", "{subject}" }
            }
            div { class: "text-sm text-muted whitespace-pre-wrap", "{body}" }
            if !when.is_empty() {
                div { class: "mt-1 text-xs text-subtle", "{when}" }
            }
        }
    }
}

/// PMS-729 phase 2 §6 slice 3: cycle Light -> Dark -> System with a
/// single click. Sits in the portal header, next to the user menu.
/// The icon shows the CURRENT state (sun for Light, moon for Dark,
/// desktop for System) so a customer can see which mode they are in
/// at a glance; the tooltip announces the transition the click will
/// perform, matching the accepted pattern from the agent-side
/// theme picker.
#[component]
fn PortalThemeToggle() -> Element {
    // Local reactive mirror of the persisted preference so this
    // component re-renders when the cycle button flips it. Reading
    // the storage directly on every render would work but would not
    // subscribe to updates, so a click would not repaint the icon
    // without a route change.
    let mut current = use_signal(crate::hooks::portal_theme::current);

    let cur = *current.read();
    let (label, tooltip): (Element, &'static str) = match cur {
        crate::hooks::theme::Theme::Light => (
            rsx! { SunIcon { size: IconSize::Medium } },
            "Light theme. Click to switch to dark.",
        ),
        crate::hooks::theme::Theme::Dark => (
            rsx! { MoonIcon { size: IconSize::Medium } },
            "Dark theme. Click to follow system.",
        ),
        crate::hooks::theme::Theme::System => (
            rsx! { ComputerDesktopIcon { size: IconSize::Medium } },
            "Follow system theme. Click to switch to light.",
        ),
    };

    rsx! {
        button {
            r#type: "button",
            class: "p-2 rounded-full text-subtle hover:text-content hover:bg-surface-2 focus:outline-none",
            aria_label: "{tooltip}",
            title: "{tooltip}",
            onclick: move |_| {
                let next = crate::hooks::portal_theme::next_in_cycle(*current.read());
                crate::hooks::portal_theme::set(next);
                current.set(next);
            },
            {label}
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
    // MAPPS-384: dismiss on navigation, same as the app-side `UserMenu`. The
    // portal chrome is likewise persistent, so a route change does not re-mount
    // this component.
    let route: Route = use_route();
    use_effect(use_reactive!(|route| {
        let _ = &route;
        if *open.peek() {
            open.set(false);
        }
    }));
    let nav = use_navigator();

    // PMS-729 phase 2 H1: portal accounts do NOT federate through bunyip
    // (they authenticate via `POST /portal/auth/login` against a
    // `contacts` row), so the agent-side hub-logout URL does not apply.
    let logout = move |_| {
        open.set(false);
        // Revoke the refresh token server-side + clear the in-memory
        // holders, then navigate to the portal login page. Nothing to
        // persist across the transition; the portal never wrote a token
        // to localStorage.
        spawn(async move {
            crate::hooks::portal_auth::portal_logout().await;
            // PMS-729 follow-up: clear the cached /portal/auth/me
            // snapshot too so a fresh login re-fetches the identity.
            #[cfg(feature = "web")]
            crate::hooks::portal_me::clear_portal_me();
            nav.replace(Route::PortalLogin {});
        });
    };

    // PMS-729 follow-up: pull the authenticated contact so the button
    // renders the customer's name and the dropdown header shows name +
    // email. Missing (pre-fetch tick, non-web build) falls through to
    // the plain-icon shape.
    #[cfg(feature = "web")]
    let me = crate::hooks::portal_me::use_portal_me();
    #[cfg(not(feature = "web"))]
    let me: Option<crate::hooks::portal_me::PortalMe> = None;

    let name_for_button = me.as_ref().map(|m| m.display_name()).unwrap_or_default();
    let initials = me.as_ref().map(|m| m.initials()).unwrap_or_default();

    rsx! {
        div { class: "relative",
            // MAPPS-384: same hover-highlight + tooltip treatment as the
            // app-side `UserMenu`, matching the sibling top-bar icons.
            // PMS-729 follow-up: shows the customer's name next to the
            // avatar when the /portal/auth/me fetch has landed; falls
            // back to the icon-only shape until then.
            button {
                r#type: "button",
                class: "flex items-center gap-2 px-2 py-1 rounded-md text-subtle hover:text-content hover:bg-surface-2 focus:outline-none",
                aria_label: if name_for_button.is_empty() { "User menu".to_string() } else { format!("User menu for {name_for_button}") },
                title: if name_for_button.is_empty() { "User menu".to_string() } else { name_for_button.clone() },
                aria_expanded: if open() { "true" } else { "false" },
                aria_haspopup: "menu",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                if !initials.is_empty() {
                    // Initials avatar. `bg-accent/10 text-accent` gives
                    // it a light chip look that inherits the tenant's
                    // primary color (branding hook overrides --accent).
                    span { class: "flex items-center justify-center h-8 w-8 rounded-full bg-accent/10 text-accent text-xs font-semibold",
                        "{initials}"
                    }
                } else {
                    UserCircleIcon { size: IconSize::Large }
                }
                if !name_for_button.is_empty() {
                    // The name label collapses on narrow screens so the
                    // top bar never wraps; the initials avatar stays
                    // visible as the tap target.
                    span { class: "hidden sm:inline text-sm text-content font-medium max-w-40 truncate",
                        "{name_for_button}"
                    }
                }
            }
            if *open.read() {
                // MAPPS-384: outside-click backdrop (see `UserMenu`).
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "absolute right-0 mt-2 w-64 rounded-md shadow-lg bg-raised ring-1 ring-black/5 z-20 p-1",
                    role: "menu",
                    // PMS-729 follow-up: identity header. Shows the
                    // signed-in customer's name + email so the visitor
                    // knows which account they are about to configure
                    // or sign out of.
                    if let Some(m) = me.as_ref() {
                        div { class: "px-3 py-2 border-b border-line mb-1",
                            p { class: "text-sm font-medium text-content truncate",
                                "{m.display_name()}"
                            }
                            p { class: "text-xs text-muted truncate",
                                "{m.email}"
                            }
                        }
                    }
                    // PMS-729 follow-up: Settings link. Lands on the
                    // portal-owned settings page (password change, MFA,
                    // sessions) so a customer never has to hunt through
                    // the sidebar to reach account controls.
                    Link {
                        to: Route::PortalSettings {},
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-content hover:bg-surface-2",
                        onclick: move |_| open.set(false),
                        "Settings"
                    }
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

#[cfg(test)]
mod tests {
    use super::full_nav_visible;
    use crate::components::icons::{
        CLIPBOARD_DOCUMENT_LIST_PATH, CREDIT_CARD_PATH, CURRENCY_PATH, DOCUMENT_CHECK_PATH,
        DOCUMENT_PATH, SCALE_PATH, SHIELD_CHECK_PATH, TAG_PATH, USERS_PATH, USER_GROUP_PATH,
    };
    use crate::modules::theme::SectionColor;

    /// MAPPS-359 AC1: every sidebar row must render a distinct icon. Before
    /// this change six rows shared `DocumentIcon`, two shared `UsersIcon`,
    /// and two shared `CurrencyIcon`, so the rail read as blocks of identical
    /// glyphs. This walks the icons that now occupy those formerly-colliding
    /// families and asserts they are all distinct, keyed on the SVG path each
    /// icon actually renders (the exported `*_PATH` consts, so the guard can
    /// never drift from the rendered glyph). If a reassignment is ever
    /// reverted to a shared icon, this fails.
    #[test]
    fn distinct_icons() {
        // (label, rendered icon path) for every row that sat in a family that
        // previously reused one glyph. All ten must be unique.
        let rows = [
            ("Timesheets", DOCUMENT_PATH),
            ("Timesheet Approvals", DOCUMENT_CHECK_PATH),
            ("Contracts", SCALE_PATH),
            ("Rate Cards", TAG_PATH),
            ("Audit Log", CLIPBOARD_DOCUMENT_LIST_PATH),
            ("SLA Management", SHIELD_CHECK_PATH),
            ("Contacts", USERS_PATH),
            ("Team", USER_GROUP_PATH),
            ("Invoices", CURRENCY_PATH),
            ("Payments", CREDIT_CARD_PATH),
        ];
        for (i, (label_a, path_a)) in rows.iter().enumerate() {
            for (label_b, path_b) in &rows[i + 1..] {
                assert_ne!(
                    path_a, path_b,
                    "sidebar rows {label_a} and {label_b} render the same icon"
                );
            }
        }
    }

    /// MAPPS-359 AC2/AC4: each top-level category is themed with a distinct
    /// accent hue, and every hue carries both a light and a dark-mode class
    /// so the rail is legible in either base mode. The (category, color)
    /// pairs mirror the `NavSection { color: ... }` assignments in
    /// `SidebarContent`; keep them in sync.
    #[test]
    fn category_colors_are_distinct_and_dual_mode() {
        let categories = [
            ("Service Desk", SectionColor::Blue),
            ("Projects", SectionColor::Indigo),
            ("CRM", SectionColor::Cyan),
            ("Operations", SectionColor::Emerald),
            ("Contracts & Billing", SectionColor::Amber),
            ("Assets", SectionColor::Teal),
            ("Knowledge", SectionColor::Fuchsia),
            ("Analytics", SectionColor::Rose),
            ("Admin", SectionColor::Violet),
        ];
        for (i, (cat_a, color_a)) in categories.iter().enumerate() {
            // Both base modes are themed: a light-mode tint plus a `dark:`
            // override.
            let cls = color_a.heading_class();
            assert!(
                !cls.starts_with("dark:") && cls.contains("dark:text-"),
                "category {cat_a} is missing a light or dark tint: {cls}"
            );
            // Distinct hue per category.
            for (cat_b, color_b) in &categories[i + 1..] {
                assert_ne!(
                    color_a.heading_class(),
                    color_b.heading_class(),
                    "categories {cat_a} and {cat_b} share a color"
                );
            }
        }
    }

    #[test]
    fn sidebar_down_then_recovery_transition() {
        // MAPPS-358 AC: cover the server-down and recovery transitions.
        // 1. Healthy: the full navigation renders.
        assert!(
            full_nav_visible(true),
            "reachable server must show the full sidebar"
        );
        // 2. Server goes down: every section below Dashboard is hidden, so
        //    Dashboard is the only navigable destination.
        assert!(
            !full_nav_visible(false),
            "unreachable server must collapse the sidebar to Dashboard-only"
        );
        // 3. Recovery poll marks the server reachable again: the full
        //    navigation is restored (UI returns to normal).
        assert!(
            full_nav_visible(true),
            "recovered server must restore the full sidebar"
        );
    }
}

/// MAPPS-366: the page-title plumbing that replaced the per-page `AppLayout`
/// title prop. A page now sets its title with `use_page_title`; the shared
/// `PageTitle` signal (provided at the App root, read by `TopBar` inside the
/// persistent `AppShell`) must carry that value to the reader. This renders a
/// page plus a `TopBar`-style reader under one provider and asserts the reader
/// observes the page's title.
///
/// The complementary "the shell does not re-mount across navigation" property
/// is a structural guarantee of the `#[layout(AppShell)]` router construct (a
/// Dioxus `#[layout]` is a single scope the router keeps mounted across its
/// child routes), not an emergent behaviour of this module, so it is enforced
/// by the route wiring in `lib.rs` rather than asserted here.
#[cfg(test)]
mod page_title_tests {
    use super::{use_current_page_title, use_page_title, use_page_title_provider};
    use dioxus::prelude::*;
    use std::sync::Mutex;

    // What the reader component last observed, captured out of the render so
    // the test can assert on it.
    static OBSERVED: Mutex<String> = Mutex::new(String::new());

    #[component]
    fn Page() -> Element {
        // Stand-in for an authenticated page: sets its title, renders a body.
        use_page_title("Tickets");
        rsx! { "page body" }
    }

    #[component]
    fn Reader() -> Element {
        // Stand-in for TopBar: reads the shared title signal and records it.
        let title = use_current_page_title().read().0.clone();
        *OBSERVED.lock().unwrap() = title;
        rsx! {}
    }

    #[component]
    fn Harness() -> Element {
        use_page_title_provider();
        rsx! {
            Page {}
            Reader {}
        }
    }

    #[test]
    fn use_page_title_reaches_the_shared_signal() {
        *OBSERVED.lock().unwrap() = String::new();
        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        assert_eq!(
            *OBSERVED.lock().unwrap(),
            "Tickets",
            "a page's use_page_title must reach the shared PageTitle signal that TopBar reads"
        );
    }
}

//! Layout components

use dioxus::prelude::*;

use super::icons::*;
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

    rsx! {
        // Single full-height viewport: top bar spans the full width
        // (brand + page title + user menu live together), and the
        // sidebar + main content sit side-by-side beneath it. Only the
        // main panel scrolls; the sidebar gets its own internal scroll
        // surface with a thin scrollbar (input.css `.scrollbar-thin`)
        // so users see the affordance when nav overflows on short
        // viewports.
        div { class: "h-screen flex flex-col bg-gray-100 dark:bg-gray-900 overflow-hidden",
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
                        class: "fixed inset-0 z-40 bg-gray-600/75 lg:hidden",
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

    rsx! {
        // Mobile sidebar drawer (full-height, slides in from left).
        // No brand block - the brand lives in the top bar now. The
        // mobile drawer overlaps the top bar so it gets its own close
        // button at the top.
        aside {
            class: "fixed inset-y-0 left-0 z-50 w-64 bg-gray-900 border-r border-gray-700 transform transition-transform duration-300 ease-in-out flex flex-col lg:hidden {mobile_class}",
            div { class: "flex items-center justify-end h-12 px-2",
                button {
                    class: "p-2 text-gray-400 hover:text-white",
                    aria_label: "Close navigation",
                    onclick: move |_| props.onclose.call(()),
                    XMarkIcon { size: IconSize::Large }
                }
            }
            // Mobile drawer closes on every navigation (its open state
            // lives in the re-mounting AppLayout), so there is no scroll
            // position worth preserving here.
            SidebarContent { persist_scroll: false }
        }

        // Desktop sidebar - sits below the top bar in the flex column
        // (no fixed positioning needed). Border-r separates it from
        // the main content. Uses `scrollbar-thin` (not `scrollbar-hide`)
        // so when the viewport is short enough that the nav list must
        // scroll, the user sees the affordance instead of silently
        // clipping behind the main panel's own scrollbar (PMC-22).
        aside { class: "hidden lg:flex lg:w-64 lg:flex-col bg-gray-900 border-r border-gray-700 overflow-y-auto overscroll-contain scrollbar-thin",
            SidebarContent { persist_scroll: true }
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
fn SidebarContent(persist_scroll: bool) -> Element {
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
                class: "flex-1 min-h-0 overflow-y-auto overscroll-contain scrollbar-thin px-2 py-4 space-y-1",
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
                NavItem { to: Route::Dashboard {}, icon: rsx!(HomeIcon {}), label: "Dashboard" }

            NavSection { title: "Service Desk",
                NavItem { to: Route::TicketList {}, icon: rsx!(TicketIcon {}), label: "Tickets" }
                NavItem { to: Route::TimeEntryList {}, icon: rsx!(ClockIcon {}), label: "Time Entries" }
                NavItem { to: Route::Timesheets {}, icon: rsx!(DocumentIcon {}), label: "Timesheets" }
            }

            NavSection { title: "Projects",
                NavItem { to: Route::ProjectList {}, icon: rsx!(FolderIcon {}), label: "Projects" }
            }

            NavSection { title: "CRM",
                NavItem { to: Route::CompanyList {}, icon: rsx!(BuildingIcon {}), label: "Companies" }
                NavItem { to: Route::ContactList {}, icon: rsx!(UsersIcon {}), label: "Contacts" }
            }

            NavSection { title: "Operations",
                NavItem { to: Route::Calendar {}, icon: rsx!(CalendarIcon {}), label: "Calendar" }
                NavItem { to: Route::DispatchBoard {}, icon: rsx!(CalendarIcon {}), label: "Dispatch" }
            }

            NavSection { title: "Contracts & Billing",
                NavItem { to: Route::ContractList {}, icon: rsx!(DocumentIcon {}), label: "Contracts" }
                NavItem { to: Route::RateCardList {}, icon: rsx!(DocumentIcon {}), label: "Rate Cards" }
                NavItem { to: Route::InvoiceList {}, icon: rsx!(CurrencyIcon {}), label: "Invoices" }
                NavItem { to: Route::PaymentList {}, icon: rsx!(CurrencyIcon {}), label: "Payments" }
            }

            NavSection { title: "Assets",
                NavItem { to: Route::AssetList {}, icon: rsx!(ServerIcon {}), label: "Assets" }
            }

            NavSection { title: "Knowledge",
                NavItem { to: Route::KBHome {}, icon: rsx!(BookIcon {}), label: "Knowledge Base" }
            }

            NavSection { title: "Analytics",
                NavItem { to: Route::Reports {}, icon: rsx!(ChartIcon {}), label: "Reports" }
            }

            if is_admin {
                NavSection { title: "Admin",
                    NavItem { to: Route::Team {}, icon: rsx!(UsersIcon {}), label: "Team" }
                    NavItem { to: Route::AuditLog {}, icon: rsx!(DocumentIcon {}), label: "Audit Log" }
                    NavItem { to: Route::SlaManagement {}, icon: rsx!(DocumentIcon {}), label: "SLA Management" }
                    // MAPPS-169: single entry into the centralized Settings hub.
                    NavItem { to: Route::SettingsHome {}, icon: rsx!(CogIcon {}), label: "Settings" }
                }
            }

            }
            VersionFooter {}
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
            class: "px-3 py-2 text-xs text-gray-500 dark:text-gray-500 text-center break-words",
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
    children: Element,
}

#[component]
fn NavSection(props: NavSectionProps) -> Element {
    let mut state = crate::hooks::use_sidebar_state();
    let title = props.title.clone();
    let collapsed = crate::hooks::is_section_collapsed(&state.read(), &title);

    let toggle_title = title.clone();
    let toggle = move |_| {
        let mut s = state.write();
        let new_value = !s.collapsed.get(&toggle_title).copied().unwrap_or(false);
        s.collapsed.insert(toggle_title.clone(), new_value);
    };

    rsx! {
        div { class: "pt-4",
            button {
                class: "w-full flex items-center justify-between px-3 py-1 text-xs font-semibold text-gray-400 uppercase tracking-wider hover:text-gray-200 focus:outline-none",
                aria_expanded: if collapsed { "false" } else { "true" },
                onclick: toggle,
                span { "{props.title}" }
                // Chevron: pointing down when expanded, right when
                // collapsed. Keeps the affordance unambiguous.
                if collapsed {
                    ChevronRightIcon { size: IconSize::Small, class: "text-gray-500".to_string() }
                } else {
                    ChevronDownIcon { size: IconSize::Small, class: "text-gray-500".to_string() }
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

    let class = if is_active {
        "group flex items-center px-3 py-2 text-sm font-medium rounded-md bg-gray-800 text-white border-l-2 border-blue-500"
    } else {
        "group flex items-center px-3 py-2 text-sm font-medium rounded-md text-gray-300 hover:bg-gray-700 hover:text-white"
    };

    rsx! {
        Link {
            to: props.to,
            class: "{class}",
            span { class: "mr-3 text-gray-400 group-hover:text-gray-300",
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
        header { class: "h-16 flex items-center bg-gray-800 border-b border-gray-700 shrink-0 z-20",
            // Brand block - same width as the sidebar below it.
            // Includes the mobile hamburger so the brand area also opens
            // the drawer on small screens.
            div { class: "flex items-center h-full lg:w-64 px-4 lg:px-6 border-r border-gray-700",
                button {
                    class: "lg:hidden p-2 mr-2 rounded-md text-gray-400 hover:text-white hover:bg-gray-700",
                    aria_label: "Open navigation",
                    onclick: move |_| props.on_menu_click.call(()),
                    MenuIcon { size: IconSize::Large }
                }
                Link {
                    to: Route::Dashboard {},
                    class: "flex flex-col leading-tight min-w-0",
                    span { class: "text-base font-bold text-white truncate",
                        "Mokosh Platform"
                    }
                    // Active-org indicator. Hidden until auth resolves an
                    // active tenant - we don't show a "no org" state in
                    // the brand slot to avoid a confusing flash on load.
                    if let Some(name) = active_org.as_deref() {
                        span { class: "text-xs text-gray-400 truncate", "{name}" }
                    }
                }
            }

            // Page title
            div { class: "flex-1 px-4 sm:px-6 lg:px-8 min-w-0",
                h1 { class: "text-xl font-semibold text-white truncate",
                    "{props.title}"
                }
            }

            // Right side actions
            div { class: "flex items-center px-4 sm:px-6 lg:px-8 space-x-4",
                // Notifications bell + inbox dropdown, wired to the
                // server `notifications` module (MAPPS-132).
                NotificationBell {}

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
    let hub_dashboard = cfg.hub_url("/dashboard");
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
                UserCircleIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
            }
            if *open.read() {
                div {
                    class: "absolute right-0 mt-2 w-52 rounded-md shadow-lg bg-white dark:bg-gray-800 ring-1 ring-black ring-opacity-5 z-20 p-1",
                    role: "menu",
                    // Profile is a mokosh-side route, served by this
                    // SPA. Use the router `Link` so the SPA does an
                    // internal transition instead of a full reload.
                    Link {
                        to: Route::Profile {},
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        onclick: move |_| open.set(false),
                        "Profile"
                    }
                    // Account Settings + Apps live on the bunyip hub;
                    // cross-origin top-level <a> so the browser
                    // navigates instead of resolving against this
                    // SPA's Route enum.
                    a {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        href: "{hub_account_settings}",
                        "Account Settings"
                    }
                    a {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        href: "{hub_dashboard}",
                        "Apps"
                    }
                    // System Status is a mokosh-side route (build
                    // versions + live API/dependency health), so route
                    // internally with `Link` (PMS-237).
                    Link {
                        to: Route::SystemStatus {},
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        onclick: move |_| open.set(false),
                        "System Status"
                    }
                    div { class: "border-t border-gray-200 dark:border-gray-700 my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700",
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
                class: "p-2 rounded-full text-gray-400 hover:text-white hover:bg-gray-700 relative",
                onclick: move |_| {
                    let next = !*open.read();
                    open.set(next);
                },
                BellIcon {}
                // Red dot only when something is actually unread.
                if unread > 0 {
                    span { class: "absolute top-1 right-1 block h-2 w-2 rounded-full bg-red-400" }
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
                    class: "absolute right-0 mt-2 w-80 max-h-96 overflow-y-auto rounded-md shadow-lg bg-white dark:bg-gray-800 ring-1 ring-black ring-opacity-5 z-20",
                    role: "menu",
                    div { class: "px-4 py-2 border-b border-gray-200 dark:border-gray-700 text-sm font-semibold text-gray-700 dark:text-gray-200",
                        "Notifications"
                    }
                    if items.is_empty() {
                        div { class: "px-4 py-6 text-sm text-gray-500 dark:text-gray-400 text-center",
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

/// A single inbox row. Unread rows are tinted and, on click, POST a
/// mark-read then ask the parent to refetch via `on_read`.
#[component]
fn NotificationRow(item: NotificationItem, on_read: EventHandler<()>) -> Element {
    let is_unread = item.read_at.is_none();
    let id = item.id;
    let subject = item.subject.clone().unwrap_or_default();
    let when = format_local_datetime(item.created_at);
    let unread_bg = if is_unread {
        "bg-blue-50 dark:bg-gray-700/40"
    } else {
        ""
    };

    rsx! {
        button {
            r#type: "button",
            class: "block w-full text-left px-4 py-3 border-b border-gray-100 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-700 {unread_bg}",
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
                div { class: "text-sm font-medium text-gray-900 dark:text-gray-100", "{subject}" }
            }
            div { class: "text-sm text-gray-600 dark:text-gray-300", "{item.body}" }
            div { class: "mt-1 text-xs text-gray-400", "{when}" }
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
        div { class: "min-h-screen bg-gray-50 dark:bg-gray-900",
            // Portal header
            header { class: "bg-white dark:bg-gray-800 shadow",
                div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8",
                    div { class: "flex items-center justify-between h-16",
                        // Logo
                        Link {
                            to: Route::PortalHome {},
                            class: "flex items-center",
                            span { class: "text-xl font-bold text-blue-600 dark:text-blue-400",
                                "Client Portal"
                            }
                        }

                        // Navigation
                        nav { class: "hidden md:flex space-x-8",
                            Link {
                                to: Route::PortalTicketList {},
                                class: "text-gray-700 dark:text-gray-300 hover:text-blue-600",
                                "Tickets"
                            }
                            Link {
                                to: Route::PortalInvoiceList {},
                                class: "text-gray-700 dark:text-gray-300 hover:text-blue-600",
                                "Invoices"
                            }
                            Link {
                                to: Route::PortalKB {},
                                class: "text-gray-700 dark:text-gray-300 hover:text-blue-600",
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
                        h1 { class: "text-2xl font-bold text-gray-900 dark:text-white mb-6",
                            "{props.title}"
                        }
                    }
                    {props.children}
                }
            }

            // Portal footer
            footer { class: "bg-white dark:bg-gray-800 border-t border-gray-200 dark:border-gray-700",
                div { class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6",
                    p { class: "text-sm text-gray-500 dark:text-gray-400 text-center",
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
                UserCircleIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
            }
            if *open.read() {
                div {
                    class: "absolute right-0 mt-2 w-52 rounded-md shadow-lg bg-white dark:bg-gray-800 ring-1 ring-black ring-opacity-5 z-20 p-1",
                    role: "menu",
                    a {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        href: "{hub_account_settings}",
                        "Account Settings"
                    }
                    div { class: "border-t border-gray-200 dark:border-gray-700 my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700",
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
        div { class: "min-h-screen flex flex-col justify-center py-12 sm:px-6 lg:px-8 bg-gray-50 dark:bg-gray-900",
            div { class: "sm:mx-auto sm:w-full sm:max-w-md",
                // Logo
                div { class: "text-center",
                    span { class: "text-3xl font-bold text-blue-600 dark:text-blue-400",
                        "Mokosh Platform"
                    }
                }
            }

            div { class: "mt-8 sm:mx-auto sm:w-full sm:max-w-md",
                div { class: "bg-white dark:bg-gray-800 py-8 px-4 shadow sm:rounded-lg sm:px-10",
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
                    h2 { class: "text-2xl font-bold leading-7 text-gray-900 dark:text-white sm:truncate sm:text-3xl sm:leading-9 sm:tracking-tight",
                        "{props.title}"
                    }
                    if !props.subtitle.is_empty() {
                        p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
                            "{props.subtitle}"
                        }
                    }
                }
                if let Some(ref actions) = props.actions {
                    div { class: "mt-4 flex md:ml-4 md:mt-0 space-x-3",
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
                            ChevronRightIcon { size: IconSize::Small, class: "text-gray-400 mx-2".to_string() }
                        }
                        if let Some(route) = &item.route {
                            Link {
                                to: route.clone(),
                                class: "text-sm font-medium text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                                "{item.label}"
                            }
                        } else {
                            span { class: "text-sm font-medium text-gray-900 dark:text-white",
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
                div { class: "mx-auto h-12 w-12 text-gray-400",
                    {icon}
                }
            }
            h3 { class: "mt-2 text-sm font-semibold text-gray-900 dark:text-white",
                "{props.title}"
            }
            if !props.description.is_empty() {
                p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
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

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
        // surface (with the scrollbar visually hidden, see
        // input.css `.scrollbar-hide`).
        div { class: "h-screen flex flex-col bg-gray-100 dark:bg-gray-900 overflow-hidden",
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
                        class: "fixed inset-0 z-40 bg-gray-600 bg-opacity-75 lg:hidden",
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
            SidebarContent {}
        }

        // Desktop sidebar - sits below the top bar in the flex column
        // (no fixed positioning needed). Border-r separates it from
        // the main content. Scrollbar hidden via .scrollbar-hide.
        aside { class: "hidden lg:flex lg:w-64 lg:flex-col bg-gray-900 border-r border-gray-700 overflow-y-auto overscroll-contain scrollbar-hide",
            SidebarContent {}
        }
    }
}

#[component]
fn SidebarContent() -> Element {
    rsx! {
        nav { class: "flex-1 px-2 py-4 space-y-1",
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

            NavSection { title: "Configuration",
                NavItem { to: Route::Settings {}, icon: rsx!(CogIcon {}), label: "Settings" }
            }
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

#[component]
fn NavItem(props: NavItemProps) -> Element {
    // Highlight the item whose `to` matches the current route. Exact
    // PartialEq match for now — detail pages (e.g. TicketDetail) will
    // not highlight their parent list nav until a path-prefix check is
    // added later.
    let current_route: Route = use_route();
    let is_active = current_route == props.to;

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
                    class: "text-xl font-bold text-white truncate",
                    "Mokosh Platform"
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
                // Search (desktop only)
                div { class: "hidden md:block",
                    super::form::SearchInput {
                        placeholder: "Search...",
                    }
                }

                // Notifications
                button {
                    class: "p-2 rounded-full text-gray-400 hover:text-white hover:bg-gray-700 relative",
                    BellIcon {}
                    span { class: "absolute top-1 right-1 block h-2 w-2 rounded-full bg-red-400" }
                }

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
    let mut auth = crate::hooks::use_auth();
    let navigator = use_navigator();

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
                // Inset items by 1 unit so each hover bg can be rounded
                // independently inside the rounded shell - keeps the
                // corners visually rounded even when hovering the
                // first/last items. Avoids overflow-hidden, which would
                // clip the menu's shadow.
                div {
                    class: "absolute right-0 mt-2 w-44 rounded-md shadow-lg bg-white dark:bg-gray-800 ring-1 ring-black ring-opacity-5 z-20 p-1",
                    role: "menu",
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        onclick: move |_| {
                            open.set(false);
                            navigator.push(Route::Settings {});
                        },
                        "Profile"
                    }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700",
                        onclick: move |_| {
                            open.set(false);
                            navigator.push(Route::Settings {});
                        },
                        "Settings"
                    }
                    div { class: "border-t border-gray-200 dark:border-gray-700 my-1" }
                    button {
                        class: "block w-full text-left rounded-md px-3 py-2 text-sm text-red-600 dark:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-700",
                        onclick: move |_| {
                            open.set(false);
                            auth.write().user = None;
                            navigator.push(Route::Login {});
                        },
                        "Logout"
                    }
                }
            }
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
                            UserCircleIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
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

/// Loading spinner overlay
#[component]
pub fn LoadingOverlay() -> Element {
    rsx! {
        div { class: "fixed inset-0 z-50 flex items-center justify-center bg-gray-900 bg-opacity-50",
            div { class: "bg-white dark:bg-gray-800 rounded-lg p-6 flex items-center space-x-3",
                super::button::Spinner {}
                span { class: "text-gray-700 dark:text-gray-300", "Loading..." }
            }
        }
    }
}

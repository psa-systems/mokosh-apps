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
        div { class: "min-h-screen bg-gray-100 dark:bg-gray-900",
            // Mobile sidebar backdrop
            if *sidebar_open.read() {
                div {
                    class: "fixed inset-0 z-40 bg-gray-600 bg-opacity-75 lg:hidden",
                    onclick: move |_| sidebar_open.set(false),
                }
            }

            // Sidebar
            Sidebar {
                open: *sidebar_open.read(),
                onclose: move |_| sidebar_open.set(false),
            }

            // Main content area
            div { class: "lg:pl-64 flex flex-col min-h-screen",
                // Top header bar
                Header {
                    title: props.title.clone(),
                    on_menu_click: move |_| sidebar_open.set(true),
                }

                // Main content
                main { class: "flex-1 py-6",
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
        // Mobile sidebar
        div {
            class: "fixed inset-y-0 left-0 z-50 w-64 bg-gray-900 transform transition-transform duration-300 ease-in-out lg:hidden {mobile_class}",
            div { class: "flex items-center justify-between h-16 px-4 bg-gray-800",
                span { class: "text-xl font-bold text-white", "Mokosh Platform" }
                button {
                    class: "text-gray-400 hover:text-white",
                    onclick: move |_| props.onclose.call(()),
                    XMarkIcon { size: IconSize::Large }
                }
            }
            SidebarContent {}
        }

        // Desktop sidebar
        div { class: "hidden lg:fixed lg:inset-y-0 lg:flex lg:w-64 lg:flex-col",
            div { class: "flex flex-col flex-grow bg-gray-900 overflow-y-auto",
                div { class: "flex items-center h-16 px-4 bg-gray-800",
                    span { class: "text-xl font-bold text-white", "Mokosh Platform" }
                }
                SidebarContent {}
            }
        }
    }
}

#[component]
fn SidebarContent() -> Element {
    rsx! {
        nav { class: "flex-1 px-2 py-4 space-y-1",
            NavItem { to: Route::Dashboard {}, icon: rsx!(HomeIcon {}), label: "Dashboard" }

            NavSection { title: "Service Desk" }
            NavItem { to: Route::TicketList {}, icon: rsx!(TicketIcon {}), label: "Tickets" }
            NavItem { to: Route::TimeEntryList {}, icon: rsx!(ClockIcon {}), label: "Time Entries" }
            NavItem { to: Route::Timesheets {}, icon: rsx!(DocumentIcon {}), label: "Timesheets" }

            NavSection { title: "Projects" }
            NavItem { to: Route::ProjectList {}, icon: rsx!(FolderIcon {}), label: "Projects" }

            NavSection { title: "CRM" }
            NavItem { to: Route::CompanyList {}, icon: rsx!(BuildingIcon {}), label: "Companies" }
            NavItem { to: Route::ContactList {}, icon: rsx!(UsersIcon {}), label: "Contacts" }

            NavSection { title: "Operations" }
            NavItem { to: Route::Calendar {}, icon: rsx!(CalendarIcon {}), label: "Calendar" }
            NavItem { to: Route::DispatchBoard {}, icon: rsx!(CalendarIcon {}), label: "Dispatch" }

            NavSection { title: "Contracts & Billing" }
            NavItem { to: Route::ContractList {}, icon: rsx!(DocumentIcon {}), label: "Contracts" }
            NavItem { to: Route::InvoiceList {}, icon: rsx!(CurrencyIcon {}), label: "Invoices" }
            NavItem { to: Route::PaymentList {}, icon: rsx!(CurrencyIcon {}), label: "Payments" }

            NavSection { title: "Assets" }
            NavItem { to: Route::AssetList {}, icon: rsx!(ServerIcon {}), label: "Assets" }

            NavSection { title: "Knowledge" }
            NavItem { to: Route::KBHome {}, icon: rsx!(BookIcon {}), label: "Knowledge Base" }

            NavSection { title: "Analytics" }
            NavItem { to: Route::Reports {}, icon: rsx!(ChartIcon {}), label: "Reports" }

            NavSection { title: "Configuration" }
            NavItem { to: Route::Settings {}, icon: rsx!(CogIcon {}), label: "Settings" }
        }
    }
}

/// Navigation section header
#[derive(Props, Clone, PartialEq)]
struct NavSectionProps {
    title: String,
}

#[component]
fn NavSection(props: NavSectionProps) -> Element {
    rsx! {
        div { class: "pt-4 pb-2 px-3",
            h3 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wider",
                "{props.title}"
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
    // TODO: Check if current route matches
    let is_active = false;

    let class = if is_active {
        "group flex items-center px-3 py-2 text-sm font-medium rounded-md bg-gray-800 text-white"
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

/// Top header bar
#[derive(Props, Clone, PartialEq)]
pub struct HeaderProps {
    #[props(default)]
    title: String,
    on_menu_click: EventHandler<()>,
}

#[component]
pub fn Header(props: HeaderProps) -> Element {
    rsx! {
        header { class: "sticky top-0 z-10 bg-white dark:bg-gray-800 shadow",
            div { class: "flex items-center justify-between h-16 px-4 sm:px-6 lg:px-8",
                // Mobile menu button
                button {
                    class: "lg:hidden p-2 rounded-md text-gray-400 hover:text-gray-500 hover:bg-gray-100",
                    onclick: move |_| props.on_menu_click.call(()),
                    MenuIcon { size: IconSize::Large }
                }

                // Page title
                h1 { class: "text-xl font-semibold text-gray-900 dark:text-white hidden sm:block",
                    "{props.title}"
                }

                // Right side actions
                div { class: "flex items-center space-x-4",
                    // Search (desktop only)
                    div { class: "hidden md:block",
                        super::form::SearchInput {
                            placeholder: "Search...",
                        }
                    }

                    // Notifications
                    button {
                        class: "p-2 rounded-full text-gray-400 hover:text-gray-500 hover:bg-gray-100 relative",
                        BellIcon {}
                        span { class: "absolute top-1 right-1 block h-2 w-2 rounded-full bg-red-400" }
                    }

                    // User menu
                    div { class: "relative",
                        button { class: "flex items-center",
                            UserCircleIcon { size: IconSize::Large, class: "text-gray-400".to_string() }
                        }
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
                    h2 { class: "text-2xl font-bold leading-7 text-gray-900 dark:text-white sm:truncate sm:text-3xl sm:tracking-tight",
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

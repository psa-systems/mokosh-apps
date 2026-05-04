//! Table components for data display

use dioxus::prelude::*;

use super::button::Spinner;
use super::icons::{ChevronDownIcon, ChevronRightIcon};

/// Table container props
#[derive(Props, Clone, PartialEq)]
pub struct TableProps {
    children: Element,
    #[props(default)]
    class: String,
}

/// Table wrapper with styling
#[component]
pub fn Table(props: TableProps) -> Element {
    let class = format!(
        "min-w-full divide-y divide-gray-200 dark:divide-gray-700 {}",
        props.class
    );

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "{class}",
                {props.children}
            }
        }
    }
}

/// Table head props
#[derive(Props, Clone, PartialEq)]
pub struct TableHeadProps {
    children: Element,
}

#[component]
pub fn TableHead(props: TableHeadProps) -> Element {
    rsx! {
        thead { class: "bg-gray-50 dark:bg-gray-800",
            {props.children}
        }
    }
}

/// Table body props
#[derive(Props, Clone, PartialEq)]
pub struct TableBodyProps {
    children: Element,
}

#[component]
pub fn TableBody(props: TableBodyProps) -> Element {
    rsx! {
        tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
            {props.children}
        }
    }
}

/// Table row props
#[derive(Props, Clone, PartialEq)]
pub struct TableRowProps {
    children: Element,
    #[props(default = false)]
    clickable: bool,
    #[props(default)]
    onclick: EventHandler<MouseEvent>,
}

#[component]
pub fn TableRow(props: TableRowProps) -> Element {
    let class = if props.clickable {
        "hover:bg-gray-50 dark:hover:bg-gray-800 cursor-pointer"
    } else {
        ""
    };

    rsx! {
        tr {
            class: "{class}",
            onclick: move |e| props.onclick.call(e),
            {props.children}
        }
    }
}

/// Table header cell props
#[derive(Props, Clone, PartialEq)]
pub struct TableHeaderProps {
    children: Element,
    #[props(default = false)]
    sortable: bool,
    #[props(default)]
    sort_direction: Option<SortDirection>,
    #[props(default)]
    onsort: EventHandler<()>,
    #[props(default)]
    class: String,
}

/// Sort direction
#[derive(Clone, Copy, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[component]
pub fn TableHeader(props: TableHeaderProps) -> Element {
    let base_class = "px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider";
    let sortable_class = if props.sortable {
        "cursor-pointer hover:text-gray-700 dark:hover:text-gray-200"
    } else {
        ""
    };
    let class = format!("{} {} {}", base_class, sortable_class, props.class);

    rsx! {
        th {
            class: "{class}",
            onclick: move |_| if props.sortable { props.onsort.call(()) },
            div { class: "flex items-center space-x-1",
                {props.children}
                if props.sortable {
                    span { class: "text-gray-400",
                        match props.sort_direction {
                            Some(SortDirection::Ascending) => rsx! {
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M5 15l7-7 7 7",
                                    }
                                }
                            },
                            Some(SortDirection::Descending) => rsx! {
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M19 9l-7 7-7-7",
                                    }
                                }
                            },
                            None => rsx! {
                                svg {
                                    class: "w-4 h-4 opacity-0 group-hover:opacity-100",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4",
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// Table cell props
#[derive(Props, Clone, PartialEq)]
pub struct TableCellProps {
    children: Element,
    #[props(default)]
    class: String,
}

#[component]
pub fn TableCell(props: TableCellProps) -> Element {
    let class = format!(
        "px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 {}",
        props.class
    );

    rsx! {
        td { class: "{class}",
            {props.children}
        }
    }
}

/// Table loading state
#[derive(Props, Clone, PartialEq)]
pub struct TableLoadingProps {
    #[props(default = 5)]
    columns: usize,
    #[props(default = 5)]
    rows: usize,
}

#[component]
pub fn TableLoading(props: TableLoadingProps) -> Element {
    rsx! {
        TableBody {
            for _ in 0..props.rows {
                TableRow {
                    for _ in 0..props.columns {
                        TableCell {
                            div { class: "h-4 bg-gray-200 dark:bg-gray-700 rounded animate-pulse" }
                        }
                    }
                }
            }
        }
    }
}

/// Table empty state
#[derive(Props, Clone, PartialEq)]
pub struct TableEmptyProps {
    #[props(default = 5)]
    columns: usize,
    #[props(default = "No data available".to_string())]
    message: String,
}

#[component]
pub fn TableEmpty(props: TableEmptyProps) -> Element {
    rsx! {
        TableBody {
            tr {
                td {
                    colspan: "{props.columns}",
                    class: "px-6 py-12 text-center text-gray-500 dark:text-gray-400",
                    "{props.message}"
                }
            }
        }
    }
}

/// Pagination props
#[derive(Props, Clone, PartialEq)]
pub struct PaginationProps {
    /// Current page (1-indexed)
    current_page: usize,
    /// Total number of items
    total_items: usize,
    /// Items per page
    per_page: usize,
    /// Page change handler
    onpagechange: EventHandler<usize>,
}

#[component]
pub fn Pagination(props: PaginationProps) -> Element {
    let total_pages = (props.total_items + props.per_page - 1) / props.per_page;
    let start_item = (props.current_page - 1) * props.per_page + 1;
    let end_item = std::cmp::min(props.current_page * props.per_page, props.total_items);

    if total_pages <= 1 {
        return rsx! {};
    }

    let page_numbers: Vec<usize> = {
        let mut pages = Vec::new();
        let start = std::cmp::max(1, props.current_page.saturating_sub(2));
        let end = std::cmp::min(total_pages, props.current_page + 2);
        for i in start..=end {
            pages.push(i);
        }
        pages
    };

    rsx! {
        div { class: "flex items-center justify-between border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 px-4 py-3 sm:px-6",
            // Mobile view
            div { class: "flex flex-1 justify-between sm:hidden",
                button {
                    class: "relative inline-flex items-center rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: props.current_page <= 1,
                    onclick: move |_| props.onpagechange.call(props.current_page - 1),
                    "Previous"
                }
                button {
                    class: "relative ml-3 inline-flex items-center rounded-md border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: props.current_page >= total_pages,
                    onclick: move |_| props.onpagechange.call(props.current_page + 1),
                    "Next"
                }
            }

            // Desktop view
            div { class: "hidden sm:flex sm:flex-1 sm:items-center sm:justify-between",
                div {
                    p { class: "text-sm text-gray-700 dark:text-gray-300",
                        "Showing "
                        span { class: "font-medium", "{start_item}" }
                        " to "
                        span { class: "font-medium", "{end_item}" }
                        " of "
                        span { class: "font-medium", "{props.total_items}" }
                        " results"
                    }
                }
                div {
                    nav { class: "isolate inline-flex -space-x-px rounded-md shadow-sm", aria_label: "Pagination",
                        // Previous button
                        button {
                            class: "relative inline-flex items-center rounded-l-md px-2 py-2 text-gray-400 ring-1 ring-inset ring-gray-300 hover:bg-gray-50 focus:z-20 focus:outline-offset-0 disabled:opacity-50 disabled:cursor-not-allowed dark:ring-gray-600 dark:hover:bg-gray-800",
                            disabled: props.current_page <= 1,
                            onclick: move |_| props.onpagechange.call(props.current_page - 1),
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }

                        // Page numbers
                        for page in page_numbers.iter() {
                            if *page == props.current_page {
                                span {
                                    class: "relative z-10 inline-flex items-center bg-blue-600 px-4 py-2 text-sm font-semibold text-white focus:z-20 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600",
                                    "{page}"
                                }
                            } else {
                                button {
                                    class: "relative inline-flex items-center px-4 py-2 text-sm font-semibold text-gray-900 ring-1 ring-inset ring-gray-300 hover:bg-gray-50 focus:z-20 focus:outline-offset-0 dark:text-gray-100 dark:ring-gray-600 dark:hover:bg-gray-800",
                                    onclick: {
                                        let page = *page;
                                        move |_| props.onpagechange.call(page)
                                    },
                                    "{page}"
                                }
                            }
                        }

                        // Next button
                        button {
                            class: "relative inline-flex items-center rounded-r-md px-2 py-2 text-gray-400 ring-1 ring-inset ring-gray-300 hover:bg-gray-50 focus:z-20 focus:outline-offset-0 disabled:opacity-50 disabled:cursor-not-allowed dark:ring-gray-600 dark:hover:bg-gray-800",
                            disabled: props.current_page >= total_pages,
                            onclick: move |_| props.onpagechange.call(props.current_page + 1),
                            ChevronRightIcon { class: "h-5 w-5".to_string() }
                        }
                    }
                }
            }
        }
    }
}

/// Data table with built-in loading, empty, and pagination states
#[derive(Props, Clone, PartialEq)]
pub struct DataTableProps {
    children: Element,
    #[props(default = false)]
    loading: bool,
    #[props(default = 0)]
    total_items: usize,
    #[props(default = 1)]
    current_page: usize,
    #[props(default = 25)]
    per_page: usize,
    #[props(default = 5)]
    columns: usize,
    #[props(default)]
    onpagechange: EventHandler<usize>,
}

#[component]
pub fn DataTable(props: DataTableProps) -> Element {
    rsx! {
        div { class: "overflow-hidden shadow ring-1 ring-black ring-opacity-5 sm:rounded-lg",
            Table {
                {props.children}
            }

            if !props.loading && props.total_items > props.per_page {
                Pagination {
                    current_page: props.current_page,
                    total_items: props.total_items,
                    per_page: props.per_page,
                    onpagechange: move |page| props.onpagechange.call(page),
                }
            }
        }
    }
}

/// Badge/tag component for status display
#[derive(Clone, Copy, PartialEq, Default)]
pub enum BadgeVariant {
    #[default]
    Gray,
    Blue,
    Green,
    Yellow,
    Red,
    Purple,
}

impl BadgeVariant {
    fn class(&self) -> &'static str {
        match self {
            BadgeVariant::Gray => "bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300",
            BadgeVariant::Blue => "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-300",
            BadgeVariant::Green => {
                "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300"
            }
            BadgeVariant::Yellow => {
                "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-300"
            }
            BadgeVariant::Red => "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-300",
            BadgeVariant::Purple => {
                "bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-300"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BadgeProps {
    children: Element,
    #[props(default)]
    variant: BadgeVariant,
    #[props(default)]
    class: String,
}

#[component]
pub fn Badge(props: BadgeProps) -> Element {
    let class = format!(
        "inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium {} {}",
        props.variant.class(),
        props.class
    );

    rsx! {
        span { class: "{class}",
            {props.children}
        }
    }
}

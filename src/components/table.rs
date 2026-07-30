//! Table components for data display

use dioxus::prelude::*;

use super::icons::ChevronRightIcon;

/// Table container props
#[derive(Props, Clone, PartialEq)]
pub struct TableProps {
    children: Element,
    #[props(default)]
    class: String,
    /// MAPPS-263: opt-in zebra striping for dense list tables. Tints odd body
    /// rows with the subtle `surface-2` token so long lists are easier to
    /// scan. Row hover (see `TableRow`) still applies on top.
    #[props(default = false)]
    striped: bool,
}

/// Table wrapper with styling
#[component]
pub fn Table(props: TableProps) -> Element {
    // Apply striping via a child selector on the table element so it cascades
    // to every body row regardless of how the caller composes `TableBody`.
    let striped = if props.striped {
        "[&_tbody_tr:nth-child(odd)]:bg-surface-2"
    } else {
        ""
    };
    let class = format!(
        "min-w-full divide-y divide-line {} {}",
        striped, props.class
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
        thead { class: "bg-surface-2",
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
        tbody { class: "bg-surface divide-y divide-line",
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
    /// Stable selector for browser-automation tests (PMC-111).
    #[props(default)]
    data_testid: Option<String>,
    /// MAPPS-249: extra classes on the `tr` (e.g. `group` so per-row hover
    /// affordances can reveal themselves via `group-hover`).
    #[props(default)]
    class: String,
}

#[component]
pub fn TableRow(props: TableRowProps) -> Element {
    // MAPPS-263: every row gets a subtle hover (not just clickable ones) so
    // dense, non-clickable tables are easier to scan. Clickable rows add the
    // pointer cursor on top. `transition-colors` softens the change; on touch
    // devices (no hover) this is simply inert.
    let base_class = if props.clickable {
        "hover:bg-surface-2 cursor-pointer transition-colors"
    } else {
        "hover:bg-surface-2 transition-colors"
    };
    let class = format!("{} {}", base_class, props.class);

    rsx! {
        tr {
            class: "{class}",
            "data-testid": props.data_testid.as_deref(),
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
    let base_class = "px-6 py-3 text-left text-xs font-medium text-muted uppercase tracking-wider";
    let sortable_class = if props.sortable {
        "cursor-pointer hover:text-content"
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
                    span { class: "text-subtle",
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
                            // MAPPS-263: faint but always-visible up/down
                            // chevron so sortability is discoverable without
                            // hover (the old `opacity-0 group-hover` trick was
                            // invisible on touch and never fired anyway since
                            // the `th` carries no `group` class). The active
                            // asc/desc arrows above render at full strength.
                            None => rsx! {
                                svg {
                                    class: "w-4 h-4 opacity-40",
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
    // MAPPS-152: use `break-words` instead of `whitespace-nowrap` so a long
    // value (e.g. a 255-char company name, including an unbroken single token)
    // wraps and lets its column shrink, rather than forcing the whole table to
    // overflow horizontally and push other columns off-screen. Callers that
    // need single-line cells opt back in via `class` (e.g. `whitespace-nowrap`
    // or `truncate`), which is appended last and wins.
    let class = format!("px-6 py-4 break-words text-sm text-content {}", props.class);

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
                            div { class: "h-4 bg-surface-2 rounded animate-pulse" }
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
    /// Back-compat single-line message. Used only when `title` is empty (e.g.
    /// sub-resource tables, error rows, filtered-out states with no CTA).
    #[props(default = "No data available".to_string())]
    message: String,
    /// PMS-354: when set, the row renders the shared `EmptyState` (title +
    /// next-action body + optional CTA) inside the table's `colspan` cell so
    /// every list's empty state follows the same helpful pattern.
    #[props(default)]
    title: String,
    #[props(default)]
    description: String,
    /// Optional primary CTA element (e.g. a "New <thing>" button/link).
    actions: Option<Element>,
}

#[component]
pub fn TableEmpty(props: TableEmptyProps) -> Element {
    let rich = !props.title.is_empty();
    rsx! {
        TableBody {
            tr {
                td {
                    colspan: "{props.columns}",
                    class: if rich { "px-6" } else { "px-6 py-12 text-center text-muted" },
                    if rich {
                        super::layout::EmptyState {
                            title: props.title.clone(),
                            description: props.description.clone(),
                            actions: props.actions,
                        }
                    } else {
                        "{props.message}"
                    }
                }
            }
        }
    }
}

/// Full-width empty-state row props (MAPPS-388).
#[derive(Props, Clone, PartialEq)]
pub struct TableEmptyRowProps {
    /// Number of columns to span so the message centers across the whole
    /// table instead of sitting left-aligned in the first cell.
    columns: usize,
    children: Element,
    /// Extra classes (e.g. `text-subtle italic`) appended after the base
    /// centering classes, so each caller keeps its own text treatment.
    #[props(default)]
    class: String,
}

/// A single horizontally-centered empty-state row, for use INSIDE an existing
/// `TableBody { if empty { .. } }` branch (MAPPS-388). Unlike [`TableEmpty`] it
/// does not wrap its own `TableBody`, so it drops into the empty branch without
/// nesting one body inside another. The cell spans every column (`colspan`) and
/// centers its text, fixing the left-aligned "No … yet." states on the list and
/// dashboard tables.
#[component]
pub fn TableEmptyRow(props: TableEmptyRowProps) -> Element {
    let class = format!("px-6 py-8 text-center text-sm {}", props.class);
    rsx! {
        tr {
            td { colspan: "{props.columns}", class: "{class}",
                {props.children}
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
    // Guard zero inputs so a direct caller cannot panic: per_page=0 would
    // divide-by-zero in div_ceil and current_page=0 would underflow the usize
    // subtraction. DataTable always passes >=1, but clamp defensively.
    let per_page = props.per_page.max(1);
    let current_page = props.current_page.max(1);
    let total_pages = props.total_items.div_ceil(per_page);
    let start_item = (current_page - 1) * per_page + 1;
    let end_item = std::cmp::min(current_page * per_page, props.total_items);

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
        div { class: "flex items-center justify-between border-t border-line bg-surface px-4 py-3 sm:px-6",
            // Mobile view
            div { class: "flex flex-1 justify-between sm:hidden",
                button {
                    class: "relative inline-flex items-center rounded-md border border-line bg-surface px-4 py-2 text-sm font-medium text-content hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: props.current_page <= 1,
                    onclick: move |_| props.onpagechange.call(props.current_page - 1),
                    "Previous"
                }
                button {
                    class: "relative ml-3 inline-flex items-center rounded-md border border-line bg-surface px-4 py-2 text-sm font-medium text-content hover:bg-surface-2 disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: props.current_page >= total_pages,
                    onclick: move |_| props.onpagechange.call(props.current_page + 1),
                    "Next"
                }
            }

            // Desktop view
            div { class: "hidden sm:flex sm:flex-1 sm:items-center sm:justify-between",
                div {
                    p { class: "text-sm text-content",
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
                        // Previous button. P3-22: explicit class shift on
                        // boundary so the disabled state is unambiguously
                        // distinct from the active state (audit: "Looks
                        // identical to active > button").
                        button {
                            class: if props.current_page <= 1 {
                                "relative inline-flex items-center rounded-l-md px-2 py-2 text-subtle ring-1 ring-inset ring-line cursor-not-allowed bg-app"
                            } else {
                                "relative inline-flex items-center rounded-l-md px-2 py-2 text-subtle ring-1 ring-inset ring-line hover:bg-surface-2 focus:z-20 focus:outline-offset-0"
                            },
                            disabled: props.current_page <= 1,
                            onclick: move |_| props.onpagechange.call(props.current_page - 1),
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }

                        // Page numbers
                        for page in page_numbers.iter() {
                            if *page == props.current_page {
                                span {
                                    class: "relative z-10 inline-flex items-center bg-accent px-4 py-2 text-sm font-semibold text-on-accent focus:z-20 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                                    "{page}"
                                }
                            } else {
                                button {
                                    class: "relative inline-flex items-center px-4 py-2 text-sm font-semibold text-content ring-1 ring-inset ring-line hover:bg-surface-2 focus:z-20 focus:outline-offset-0",
                                    onclick: {
                                        let page = *page;
                                        move |_| props.onpagechange.call(page)
                                    },
                                    "{page}"
                                }
                            }
                        }

                        // Next button (mirror of prev for P3-22).
                        button {
                            class: if props.current_page >= total_pages {
                                "relative inline-flex items-center rounded-r-md px-2 py-2 text-subtle ring-1 ring-inset ring-line cursor-not-allowed bg-app"
                            } else {
                                "relative inline-flex items-center rounded-r-md px-2 py-2 text-subtle ring-1 ring-inset ring-line hover:bg-surface-2 focus:z-20 focus:outline-offset-0"
                            },
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
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BadgeVariant {
    #[default]
    Gray,
    Blue,
    Green,
    Yellow,
    Orange,
    Red,
    Purple,
}

impl BadgeVariant {
    fn class(&self) -> &'static str {
        match self {
            BadgeVariant::Gray => "bg-surface-2 text-content",
            BadgeVariant::Blue => "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-300",
            BadgeVariant::Green => {
                "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300"
            }
            BadgeVariant::Yellow => {
                "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-300"
            }
            BadgeVariant::Orange => {
                "bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-300"
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

#[cfg(test)]
mod tests {
    use super::TableEmptyRow;
    use dioxus::prelude::*;

    #[component]
    fn Harness() -> Element {
        rsx! {
            TableEmptyRow { columns: 6, class: "text-subtle italic", "No time logged yet." }
        }
    }

    #[test]
    fn empty_row_spans_all_columns_and_centers() {
        // MAPPS-388 regression: the shared empty-state row (used by the time,
        // dashboard, timesheet-approval, and assets tables) must span every
        // column and center its text rather than sit left-aligned in the first
        // cell. Render to HTML and assert the colspan + centering survive.
        let mut dom = VirtualDom::new(Harness);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(
            html.contains(r#"colspan="6""#),
            "empty row must span all 6 columns; got: {html}"
        );
        assert!(
            html.contains("text-center"),
            "empty row must be horizontally centered; got: {html}"
        );
        assert!(
            html.contains("No time logged yet."),
            "empty row must render its message; got: {html}"
        );
    }
}

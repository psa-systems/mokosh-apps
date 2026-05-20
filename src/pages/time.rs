//! Time tracking pages

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use dioxus::prelude::*;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ChevronRightIcon, DataTable,
    IconSize, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::Route;

/// Start of the Monday-Sunday week that contains `date`.
fn monday_of_week(date: NaiveDate) -> NaiveDate {
    let offset = match date.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    date - Duration::days(offset)
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Time entry list page
#[component]
pub fn TimeEntryListPage() -> Element {
    let mut date_range = use_signal(|| "this_week".to_string());
    let mut user_filter = use_signal(String::new);

    let date_options = vec![
        SelectOption::new("today", "Today"),
        SelectOption::new("this_week", "This Week"),
        SelectOption::new("last_week", "Last Week"),
        SelectOption::new("this_month", "This Month"),
        SelectOption::new("last_month", "Last Month"),
    ];

    let user_options = vec![
        SelectOption::new("", "All Users"),
        SelectOption::new("1", "John Smith"),
        SelectOption::new("2", "Jane Doe"),
    ];

    rsx! {
        AppLayout { title: "Time Entries",
            PageHeader {
                title: "Time Entries",
                subtitle: "Track and manage time spent on work",
                actions: rsx! {
                    Link {
                        to: Route::TimeEntryNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Log Time"
                        }
                    }
                },
            }

            // Stats
            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Today" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "4.5h" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "This Week" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "32.5h" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Billable" }
                    p { class: "text-2xl font-bold text-green-600", "28.0h" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Non-Billable" }
                    p { class: "text-2xl font-bold text-gray-500", "4.5h" }
                }
            }

            // Filters
            Card { class: "mb-6",
                div { class: "flex flex-col sm:flex-row gap-4",
                    Select {
                        name: "date_range",
                        options: date_options,
                        value: date_range.read().clone(),
                        onchange: move |e: FormEvent| date_range.set(e.value()),
                    }
                    Select {
                        name: "user",
                        options: user_options,
                        value: user_filter.read().clone(),
                        placeholder: "User",
                        onchange: move |e: FormEvent| user_filter.set(e.value()),
                    }
                }
            }

            // Time entries table
            DataTable {
                total_items: 25,
                current_page: 1,
                per_page: 25,
                columns: 6,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { sortable: true, "Date" }
                            TableHeader { sortable: true, "User" }
                            TableHeader { "Work Item" }
                            TableHeader { "Description" }
                            TableHeader { sortable: true, "Hours" }
                            TableHeader { "Billable" }
                        }
                    }
                    TableBody {
                        TimeEntryRow {
                            date: "Jan 15, 2025",
                            user: "John Smith",
                            work_item: "TKT-1234",
                            work_item_title: "Email server issue",
                            description: "Troubleshooting and restart of Exchange services",
                            hours: "1.5",
                            billable: true,
                        }
                        TimeEntryRow {
                            date: "Jan 15, 2025",
                            user: "John Smith",
                            work_item: "TKT-1233",
                            work_item_title: "User setup",
                            description: "New employee onboarding - IT setup",
                            hours: "2.0",
                            billable: true,
                        }
                        TimeEntryRow {
                            date: "Jan 15, 2025",
                            user: "Jane Doe",
                            work_item: "PRJ-101",
                            work_item_title: "Network Upgrade",
                            description: "Planning and documentation",
                            hours: "3.0",
                            billable: true,
                        }
                        TimeEntryRow {
                            date: "Jan 14, 2025",
                            user: "John Smith",
                            work_item: "Internal",
                            work_item_title: "",
                            description: "Team meeting",
                            hours: "1.0",
                            billable: false,
                        }
                        TimeEntryRow {
                            date: "Jan 14, 2025",
                            user: "Jane Doe",
                            work_item: "TKT-1230",
                            work_item_title: "License renewal",
                            description: "Research and procurement",
                            hours: "0.5",
                            billable: true,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimeEntryRowProps {
    date: String,
    user: String,
    work_item: String,
    work_item_title: String,
    description: String,
    hours: String,
    billable: bool,
}

#[component]
fn TimeEntryRow(props: TimeEntryRowProps) -> Element {
    // TKT-#### / PRJ-#### / etc. work_item codes should jump to the
    // matching record so the time entry list doubles as a "what was I
    // working on" navigation surface. Strip the prefix to get the id.
    let work_link: Option<Route> = if let Some(id) = props.work_item.strip_prefix("TKT-") {
        Some(Route::TicketDetail { id: id.to_string() })
    } else {
        props
            .work_item
            .strip_prefix("PRJ-")
            .map(|id| Route::ProjectDetail { id: id.to_string() })
    };

    rsx! {
        TableRow {
            TableCell { class: "text-gray-500", "{props.date}" }
            TableCell { "{props.user}" }
            TableCell {
                div {
                    if let Some(route) = work_link.clone() {
                        Link {
                            to: route,
                            class: "font-medium text-blue-600 hover:text-blue-500",
                            "{props.work_item}"
                        }
                    } else {
                        span { class: "font-medium text-gray-700 dark:text-gray-300", "{props.work_item}" }
                    }
                    if !props.work_item_title.is_empty() {
                        p { class: "text-gray-500 text-xs", "{props.work_item_title}" }
                    }
                }
            }
            TableCell { class: "max-w-xs truncate", "{props.description}" }
            TableCell { class: "font-medium", "{props.hours}h" }
            TableCell {
                if props.billable {
                    Badge { variant: BadgeVariant::Green, "Billable" }
                } else {
                    Badge { variant: BadgeVariant::Gray, "Non-Billable" }
                }
            }
        }
    }
}

/// New time entry page
#[component]
pub fn TimeEntryNewPage() -> Element {
    let mut work_item = use_signal(String::new);
    let mut hours = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut is_billable = use_signal(|| true);
    let mut is_submitting = use_signal(|| false);

    let work_item_options = vec![
        SelectOption::new("tkt-1234", "TKT-1234: Email server issue"),
        SelectOption::new("tkt-1233", "TKT-1233: User setup"),
        SelectOption::new("prj-101", "PRJ-101: Network Upgrade"),
        SelectOption::new("internal", "Internal (Non-billable)"),
    ];

    rsx! {
        AppLayout { title: "Log Time",
            PageHeader {
                title: "Log Time",
                subtitle: "Record time spent on work",
            }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        is_submitting.set(true);
                        // P1-04: mock submit while server time-tracking
                        // module is still placeholder.
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use gloo_timers::future::TimeoutFuture;
                                TimeoutFuture::new(1000).await;
                            }
                            is_submitting.set(false);
                            dioxus::prelude::navigator().push(Route::TimeEntryList {});
                        });
                    },

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "work_item",
                            label: "Work Item",
                            options: work_item_options,
                            value: work_item.read().clone(),
                            placeholder: "Select ticket or project",
                            required: true,
                            onchange: move |e: FormEvent| work_item.set(e.value()),
                        }

                        crate::components::Input {
                            name: "hours",
                            label: "Hours",
                            r#type: "number",
                            placeholder: "0.00",
                            required: true,
                            value: hours.read().clone(),
                            oninput: move |e: FormEvent| hours.set(e.value()),
                        }
                    }

                    crate::components::Textarea {
                        name: "description",
                        label: "Description",
                        placeholder: "What did you work on?",
                        rows: 3,
                        required: true,
                        value: description.read().clone(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }

                    crate::components::Checkbox {
                        name: "billable",
                        label: "Billable",
                        checked: *is_billable.read(),
                        help: "Mark this time entry as billable to the customer",
                        onchange: move |_| {
                            let current = *is_billable.read();
                            is_billable.set(!current);
                        },
                    }

                    div { class: "flex justify-end space-x-3",
                        Link {
                            to: Route::TimeEntryList {},
                            Button {
                                variant: ButtonVariant::Secondary,
                                "Cancel"
                            }
                        }
                        Button {
                            r#type: "submit",
                            variant: ButtonVariant::Primary,
                            loading: *is_submitting.read(),
                            "Save Time Entry"
                        }
                    }
                }
            }
        }
    }
}

/// Timesheets page
#[component]
pub fn TimesheetsPage() -> Element {
    // Default to the week of Jan 13 2025 so the existing demo grid lines
    // up. Move to chrono::Local::now() when a real timesheet backend lands.
    let initial = NaiveDate::from_ymd_opt(2025, 1, 13)
        .unwrap_or_else(|| chrono::Local::now().naive_local().date());
    let mut week_start = use_signal(|| monday_of_week(initial));

    let label = {
        let start = week_start();
        let end = start + Duration::days(6);
        format!(
            "Week of {} {}-{}, {}",
            month_name(start.month()),
            start.day(),
            end.day(),
            start.year()
        )
    };

    rsx! {
        AppLayout { title: "Timesheets",
            PageHeader {
                title: "Timesheets",
                subtitle: "Weekly timesheet management",
                // Audit P1-07: Submit Timesheet button was decorative (no
                // onclick, no submission workflow). Hidden until timesheet
                // approval flow ships.
            }

            // Week selector
            Card { class: "mb-6",
                div { class: "flex items-center justify-between",
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Previous week",
                        onclick: move |_| {
                            week_start.set(week_start() - Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                    }
                    span { class: "text-lg font-medium text-gray-900 dark:text-white",
                        "{label}"
                    }
                    button {
                        r#type: "button",
                        class: "p-2 text-gray-400 hover:text-gray-600",
                        title: "Next week",
                        onclick: move |_| {
                            week_start.set(week_start() + Duration::days(7));
                        },
                        ChevronRightIcon { class: "h-5 w-5".to_string() }
                    }
                }
            }

            // Weekly grid
            Card { padding: false,
                div { class: "overflow-x-auto",
                    table { class: "min-w-full divide-y divide-gray-200 dark:divide-gray-700",
                        thead { class: "bg-gray-50 dark:bg-gray-800",
                            tr {
                                th { class: "px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider",
                                    "Work Item"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Mon"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Tue"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Wed"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Thu"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Fri"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Sat"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Sun"
                                }
                                th { class: "px-4 py-3 text-center text-xs font-medium text-gray-500 uppercase tracking-wider w-20",
                                    "Total"
                                }
                            }
                        }
                        tbody { class: "bg-white dark:bg-gray-900 divide-y divide-gray-200 dark:divide-gray-700",
                            TimesheetRow {
                                work_item: "TKT-1234: Email server",
                                hours: vec!["2.0", "1.5", "", "", "", "", ""],
                            }
                            TimesheetRow {
                                work_item: "TKT-1233: User setup",
                                hours: vec!["", "2.0", "3.0", "", "", "", ""],
                            }
                            TimesheetRow {
                                work_item: "PRJ-101: Network Upgrade",
                                hours: vec!["4.0", "4.0", "4.0", "4.0", "4.0", "", ""],
                            }
                            TimesheetRow {
                                work_item: "Internal",
                                hours: vec!["1.0", "0.5", "1.0", "0.5", "1.0", "", ""],
                            }
                            // Totals row
                            tr { class: "bg-gray-50 dark:bg-gray-800 font-medium",
                                td { class: "px-6 py-3 text-sm text-gray-900 dark:text-white",
                                    "Daily Total"
                                }
                                td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white", "7.0" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white", "8.0" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white", "8.0" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white", "4.5" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-900 dark:text-white", "5.0" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-500", "0" }
                                td { class: "px-4 py-3 text-center text-sm text-gray-500", "0" }
                                td { class: "px-4 py-3 text-center text-sm font-bold text-blue-600", "32.5" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimesheetRowProps {
    work_item: String,
    hours: Vec<&'static str>,
}

#[component]
fn TimesheetRow(props: TimesheetRowProps) -> Element {
    let total: f32 = props
        .hours
        .iter()
        .filter_map(|h| h.parse::<f32>().ok())
        .sum();

    rsx! {
        tr {
            td { class: "px-6 py-3 text-sm text-gray-900 dark:text-white",
                "{props.work_item}"
            }
            for hours in props.hours.iter() {
                td { class: "px-4 py-3 text-center text-sm",
                    if hours.is_empty() {
                        span { class: "text-gray-300 dark:text-gray-600", "-" }
                    } else {
                        span { class: "text-gray-900 dark:text-white", "{hours}" }
                    }
                }
            }
            td { class: "px-4 py-3 text-center text-sm font-medium text-gray-900 dark:text-white",
                "{total}"
            }
        }
    }
}

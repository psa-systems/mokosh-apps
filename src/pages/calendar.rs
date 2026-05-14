//! Calendar and dispatch pages
//!
//! The calendar grid is generated from the `active_month` signal so
//! prev/next/today actually navigate. Per-day event content is still
//! demo data (no backend yet); we surface that with disabled stubs on
//! the "New Appointment" CTA and the Week/Day view toggles. Stubs use
//! `<button disabled title="Coming soon">` so users see them as
//! intentional placeholders.

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use dioxus::prelude::*;

use crate::components::{
    AppLayout, Button, ButtonVariant, Card, ChevronRightIcon, IconSize, PageHeader, PlusIcon,
};

/// Pin demo events to specific January-2025 days. Everything else
/// renders an empty grid; clearer than leaving Jan-2025 events showing
/// when the user navigates to March.
fn demo_events_for(date: NaiveDate) -> Vec<&'static str> {
    if date.year() != 2025 || date.month() != 1 {
        return vec![];
    }
    match date.day() {
        1 => vec!["New Year Holiday"],
        6 => vec!["Acme Corp - Onsite"],
        8 => vec!["TechStart Meeting"],
        10 => vec!["Team Standup", "Client Call"],
        13 => vec!["Network Upgrade"],
        15 => vec!["Acme Onsite", "Quarterly Review"],
        21 => vec!["Server Migration"],
        _ => vec![],
    }
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

/// Add `delta` months to `date`, clamping the day to the new month's
/// last valid day. Used by the prev/next month buttons.
fn shift_months(date: NaiveDate, delta: i32) -> NaiveDate {
    let total = date.year() * 12 + (date.month() as i32 - 1) + delta;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12);
    // Day 1 is always valid; we only navigate by month and don't care
    // about preserving the day-of-month since the grid is regenerated.
    NaiveDate::from_ymd_opt(year, (month0 + 1) as u32, 1).unwrap_or(date)
}

/// Return 35 cells (5 rows × 7 cols) covering the calendar grid for
/// the given month, starting on the Sunday before (or on) the 1st.
/// Each cell is (date, is_in_active_month).
fn calendar_cells(active: NaiveDate) -> Vec<(NaiveDate, bool)> {
    let first_of_month =
        NaiveDate::from_ymd_opt(active.year(), active.month(), 1).unwrap_or(active);
    let weekday = first_of_month.weekday();
    // Sunday = 0, Saturday = 6 in our header order.
    let lead = match weekday {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    };
    let grid_start = first_of_month - Duration::days(lead);
    // 5 weeks is enough for any month layout when started on the
    // preceding Sunday; six only matters for months where the 1st is
    // Friday/Saturday AND the month has 31 days. Use 6 rows to be safe.
    (0..42)
        .map(|i| {
            let d = grid_start + Duration::days(i as i64);
            (d, d.month() == active.month())
        })
        .collect()
}

/// Calendar page
#[component]
pub fn CalendarPage() -> Element {
    // Default to Jan 2025 so the existing demo data shows up. Real
    // calendar would default to Local::now() once we have a backend.
    let initial =
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap_or_else(|| Local::now().naive_local().date());
    let mut active_month = use_signal(|| initial);
    let today_real = Local::now().naive_local().date();

    let title = {
        let m = active_month();
        format!("{} {}", month_name(m.month()), m.year())
    };

    rsx! {
        AppLayout { title: "Calendar",
            PageHeader {
                title: "Calendar",
                actions: rsx! {
                    // Disabled stub: creating appointments requires the
                    // calendar API on mokosh-server, which isn't wired
                    // yet. Surface as "Coming soon" rather than a
                    // silent no-op.
                    // TODO(calendar-api): wire up once /v1/calendar
                    // endpoints exist on mokosh-server.
                    button {
                        r#type: "button",
                        disabled: true,
                        title: "Coming soon - calendar API not wired yet",
                        class: "inline-flex items-center justify-center font-medium rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 transition-colors disabled:opacity-50 disabled:cursor-not-allowed bg-blue-600 text-white px-4 py-2 text-sm",
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Appointment"
                    }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-4 gap-6",
                // Calendar grid
                div { class: "lg:col-span-3",
                    Card { padding: false,
                        // Calendar header
                        div { class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                            div { class: "flex items-center space-x-4",
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    title: "Previous month",
                                    onclick: move |_| {
                                        let prev = shift_months(active_month(), -1);
                                        active_month.set(prev);
                                    },
                                    ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                                }
                                h2 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                    "{title}"
                                }
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    title: "Next month",
                                    onclick: move |_| {
                                        let next = shift_months(active_month(), 1);
                                        active_month.set(next);
                                    },
                                    ChevronRightIcon { class: "h-5 w-5".to_string() }
                                }
                            }
                            div { class: "flex space-x-2",
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        active_month.set(today_real);
                                    },
                                    "Today"
                                }
                                div { class: "flex border border-gray-300 dark:border-gray-600 rounded-md",
                                    // Month is the only implemented view today; Week/Day
                                    // are placeholders. Keep all three visible as roadmap
                                    // signal, but disable the unimplemented two.
                                    button {
                                        r#type: "button",
                                        class: "px-3 py-1 text-sm bg-blue-600 text-white rounded-l-md cursor-default",
                                        aria_pressed: true,
                                        "Month"
                                    }
                                    // TODO(calendar-views): week / day layouts.
                                    button {
                                        r#type: "button",
                                        disabled: true,
                                        title: "Coming soon - week view not implemented",
                                        class: "px-3 py-1 text-sm text-gray-700 dark:text-gray-300 disabled:opacity-50 disabled:cursor-not-allowed",
                                        "Week"
                                    }
                                    button {
                                        r#type: "button",
                                        disabled: true,
                                        title: "Coming soon - day view not implemented",
                                        class: "px-3 py-1 text-sm text-gray-700 dark:text-gray-300 disabled:opacity-50 disabled:cursor-not-allowed rounded-r-md",
                                        "Day"
                                    }
                                }
                            }
                        }

                        // Calendar grid
                        div { class: "p-4",
                            // Day headers
                            div { class: "grid grid-cols-7 gap-px mb-2",
                                for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                                    div { class: "text-center text-sm font-medium text-gray-500 dark:text-gray-400 py-2",
                                        "{day}"
                                    }
                                }
                            }

                            div { class: "grid grid-cols-7 gap-px bg-gray-200 dark:bg-gray-700 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden",
                                for (date, in_active) in calendar_cells(active_month()) {
                                    CalendarDay {
                                        day: date.day(),
                                        is_other_month: !in_active,
                                        is_today: date == today_real,
                                        events: demo_events_for(date).into_iter().map(String::from).collect(),
                                    }
                                }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    // Today's schedule (demo data; matches the Jan 15 2025 layout)
                    Card { title: "Today's Schedule",
                        div { class: "space-y-3",
                            ScheduleEvent {
                                time: "9:00 AM",
                                title: "Onsite: Acme Corp",
                                event_type: "onsite",
                            }
                            ScheduleEvent {
                                time: "11:30 AM",
                                title: "Quarterly Review",
                                event_type: "meeting",
                            }
                            ScheduleEvent {
                                time: "2:00 PM",
                                title: "Remote Support",
                                event_type: "remote",
                            }
                            ScheduleEvent {
                                time: "4:00 PM",
                                title: "Team Standup",
                                event_type: "internal",
                            }
                        }
                    }

                    // Upcoming (demo data; static for now)
                    Card { title: "Upcoming",
                        div { class: "space-y-2 text-sm",
                            p { class: "text-gray-600 dark:text-gray-400",
                                span { class: "font-medium", "Jan 21: " }
                                "Server Migration"
                            }
                            p { class: "text-gray-600 dark:text-gray-400",
                                span { class: "font-medium", "Jan 28: " }
                                "Monthly Review"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CalendarDayProps {
    day: u32,
    #[props(default = false)]
    is_other_month: bool,
    #[props(default = false)]
    is_today: bool,
    events: Vec<String>,
}

#[component]
fn CalendarDay(props: CalendarDayProps) -> Element {
    let bg_class = if props.is_today {
        "bg-blue-50 dark:bg-blue-900/20"
    } else {
        "bg-white dark:bg-gray-800"
    };

    let text_class = if props.is_other_month {
        "text-gray-400 dark:text-gray-600"
    } else if props.is_today {
        "text-blue-600 dark:text-blue-400 font-bold"
    } else {
        "text-gray-900 dark:text-white"
    };

    rsx! {
        div { class: "min-h-24 p-2 {bg_class}",
            span { class: "text-sm {text_class}", "{props.day}" }
            div { class: "mt-1 space-y-1",
                for (i, event) in props.events.iter().enumerate() {
                    if i < 2 {
                        div { class: "text-xs truncate px-1 py-0.5 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded",
                            "{event}"
                        }
                    }
                }
                if props.events.len() > 2 {
                    {
                        let remaining = props.events.len() - 2;
                        rsx! { span { class: "text-xs text-gray-500", "+{remaining} more" } }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ScheduleEventProps {
    time: String,
    title: String,
    event_type: String,
}

#[component]
fn ScheduleEvent(props: ScheduleEventProps) -> Element {
    let color_class = match props.event_type.as_str() {
        "onsite" => "border-l-green-500 bg-green-50 dark:bg-green-900/20",
        "meeting" => "border-l-blue-500 bg-blue-50 dark:bg-blue-900/20",
        "remote" => "border-l-purple-500 bg-purple-50 dark:bg-purple-900/20",
        _ => "border-l-gray-500 bg-gray-50 dark:bg-gray-800",
    };

    rsx! {
        div { class: "border-l-4 {color_class} p-3 rounded-r",
            p { class: "text-xs text-gray-500 dark:text-gray-400", "{props.time}" }
            p { class: "font-medium text-gray-900 dark:text-white", "{props.title}" }
        }
    }
}

/// Dispatch board page. Day-by-day navigation works (prev/next/today);
/// scheduling new appointments is stubbed because there's no backend yet.
#[component]
pub fn DispatchBoardPage() -> Element {
    let today_real = Local::now().naive_local().date();
    // Default to Jan 15 2025 so the demo data lines up with the page text.
    let initial = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap_or(today_real);
    let mut active_day = use_signal(|| initial);

    let title = active_day().format("%A, %B %-d, %Y").to_string();

    rsx! {
        AppLayout { title: "Dispatch Board",
            PageHeader {
                title: "Dispatch Board",
                subtitle: "Manage technician schedules and appointments",
                actions: rsx! {
                    // TODO(calendar-api): schedule new appointments via
                    // mokosh-server once the endpoint exists.
                    button {
                        r#type: "button",
                        disabled: true,
                        title: "Coming soon - dispatch API not wired yet",
                        class: "inline-flex items-center justify-center font-medium rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 transition-colors disabled:opacity-50 disabled:cursor-not-allowed bg-blue-600 text-white px-4 py-2 text-sm",
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Schedule Appointment"
                    }
                },
            }

            Card { padding: false,
                // Header
                div { class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                    div { class: "flex items-center space-x-4",
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            title: "Previous day",
                            onclick: move |_| {
                                active_day.set(active_day() - Duration::days(1));
                            },
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }
                        h2 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                            "{title}"
                        }
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            title: "Next day",
                            onclick: move |_| {
                                active_day.set(active_day() + Duration::days(1));
                            },
                            ChevronRightIcon { class: "h-5 w-5".to_string() }
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| { active_day.set(today_real); },
                        "Today"
                    }
                }

                // Dispatch grid
                div { class: "overflow-x-auto",
                    div { class: "min-w-[800px]",
                        // Time headers
                        div { class: "grid grid-cols-[200px_repeat(9,1fr)] border-b border-gray-200 dark:border-gray-700",
                            div { class: "p-2 bg-gray-50 dark:bg-gray-800 font-medium text-sm text-gray-500", "Technician" }
                            for hour in 8..=16 {
                                div { class: "p-2 bg-gray-50 dark:bg-gray-800 text-center text-sm text-gray-500 border-l border-gray-200 dark:border-gray-700",
                                    if hour <= 12 {
                                        "{hour}:00 AM"
                                    } else {
                                        "{hour - 12}:00 PM"
                                    }
                                }
                            }
                        }

                        // Technician rows
                        TechnicianRow {
                            name: "John Smith",
                            appointments: vec![
                                ("8:00 AM", "10:00 AM", "Acme Corp - Server Maint.", "onsite"),
                                ("10:30 AM", "12:00 PM", "TechStart - Network Issue", "remote"),
                                ("2:00 PM", "4:00 PM", "Global Widgets - Setup", "onsite"),
                            ],
                        }
                        TechnicianRow {
                            name: "Jane Doe",
                            appointments: vec![
                                ("9:00 AM", "11:00 AM", "Meeting - Quarterly Review", "meeting"),
                                ("1:00 PM", "3:00 PM", "New Venture - Site Survey", "onsite"),
                            ],
                        }
                        TechnicianRow {
                            name: "Mike Wilson",
                            appointments: vec![
                                ("8:00 AM", "12:00 PM", "Acme Corp - Migration", "onsite"),
                            ],
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TechnicianRowProps {
    name: String,
    appointments: Vec<(&'static str, &'static str, &'static str, &'static str)>,
}

#[component]
fn TechnicianRow(props: TechnicianRowProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-[200px_repeat(9,1fr)] border-b border-gray-200 dark:border-gray-700 min-h-16",
            // Technician name
            div { class: "p-2 flex items-center",
                div { class: "flex items-center",
                    div { class: "w-8 h-8 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center mr-2",
                        span { class: "text-sm font-medium text-blue-600 dark:text-blue-400",
                            {props.name.chars().next().unwrap_or('?').to_string()}
                        }
                    }
                    span { class: "font-medium text-sm text-gray-900 dark:text-white", "{props.name}" }
                }
            }

            // Time slots (simplified visual representation)
            for _ in 0..9 {
                div { class: "border-l border-gray-200 dark:border-gray-700 relative" }
            }
        }
    }
}

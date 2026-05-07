//! Calendar and dispatch pages

use dioxus::prelude::*;

use crate::components::{
    AppLayout, Button, ButtonVariant, Card, ChevronRightIcon, IconSize, PageHeader, PlusIcon,
};

/// Calendar page
#[component]
pub fn CalendarPage() -> Element {
    rsx! {
        AppLayout { title: "Calendar",
            PageHeader {
                title: "Calendar",
                actions: rsx! {
                    Button { variant: ButtonVariant::Primary,
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
                                button { class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                                }
                                h2 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                    "January 2025"
                                }
                                button { class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    ChevronRightIcon { class: "h-5 w-5".to_string() }
                                }
                            }
                            div { class: "flex space-x-2",
                                Button { variant: ButtonVariant::Secondary, "Today" }
                                div { class: "flex border border-gray-300 dark:border-gray-600 rounded-md",
                                    button { class: "px-3 py-1 text-sm bg-blue-600 text-white rounded-l-md", "Month" }
                                    button { class: "px-3 py-1 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800", "Week" }
                                    button { class: "px-3 py-1 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-r-md", "Day" }
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

                            // Calendar days (simplified - showing first two weeks)
                            div { class: "grid grid-cols-7 gap-px bg-gray-200 dark:bg-gray-700 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden",
                                // Week 1 (Dec 29 - Jan 4)
                                CalendarDay { day: 29, is_other_month: true, events: vec![] }
                                CalendarDay { day: 30, is_other_month: true, events: vec![] }
                                CalendarDay { day: 31, is_other_month: true, events: vec![] }
                                CalendarDay { day: 1, is_other_month: false, events: vec!["New Year Holiday".to_string()] }
                                CalendarDay { day: 2, is_other_month: false, events: vec![] }
                                CalendarDay { day: 3, is_other_month: false, events: vec![] }
                                CalendarDay { day: 4, is_other_month: false, events: vec![] }

                                // Week 2
                                CalendarDay { day: 5, is_other_month: false, events: vec![] }
                                CalendarDay { day: 6, is_other_month: false, events: vec!["Acme Corp - Onsite".to_string()] }
                                CalendarDay { day: 7, is_other_month: false, events: vec![] }
                                CalendarDay { day: 8, is_other_month: false, events: vec!["TechStart Meeting".to_string()] }
                                CalendarDay { day: 9, is_other_month: false, events: vec![] }
                                CalendarDay { day: 10, is_other_month: false, events: vec!["Team Standup".to_string(), "Client Call".to_string()] }
                                CalendarDay { day: 11, is_other_month: false, events: vec![] }

                                // Week 3
                                CalendarDay { day: 12, is_other_month: false, events: vec![] }
                                CalendarDay { day: 13, is_other_month: false, events: vec!["Network Upgrade".to_string()] }
                                CalendarDay { day: 14, is_other_month: false, events: vec![] }
                                CalendarDay { day: 15, is_other_month: false, is_today: true, events: vec!["Acme Onsite".to_string(), "Quarterly Review".to_string()] }
                                CalendarDay { day: 16, is_other_month: false, events: vec![] }
                                CalendarDay { day: 17, is_other_month: false, events: vec![] }
                                CalendarDay { day: 18, is_other_month: false, events: vec![] }

                                // Week 4
                                CalendarDay { day: 19, is_other_month: false, events: vec![] }
                                CalendarDay { day: 20, is_other_month: false, events: vec![] }
                                CalendarDay { day: 21, is_other_month: false, events: vec!["Server Migration".to_string()] }
                                CalendarDay { day: 22, is_other_month: false, events: vec![] }
                                CalendarDay { day: 23, is_other_month: false, events: vec![] }
                                CalendarDay { day: 24, is_other_month: false, events: vec![] }
                                CalendarDay { day: 25, is_other_month: false, events: vec![] }

                                // Week 5
                                CalendarDay { day: 26, is_other_month: false, events: vec![] }
                                CalendarDay { day: 27, is_other_month: false, events: vec![] }
                                CalendarDay { day: 28, is_other_month: false, events: vec![] }
                                CalendarDay { day: 29, is_other_month: false, events: vec![] }
                                CalendarDay { day: 30, is_other_month: false, events: vec![] }
                                CalendarDay { day: 31, is_other_month: false, events: vec![] }
                                CalendarDay { day: 1, is_other_month: true, events: vec![] }
                            }
                        }
                    }
                }

                // Sidebar
                div { class: "space-y-6",
                    // Today's schedule
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

                    // Mini calendar (placeholder)
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

/// Dispatch board page
#[component]
pub fn DispatchBoardPage() -> Element {
    rsx! {
        AppLayout { title: "Dispatch Board",
            PageHeader {
                title: "Dispatch Board",
                subtitle: "Manage technician schedules and appointments",
                actions: rsx! {
                    Button { variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Schedule Appointment"
                    }
                },
            }

            Card { padding: false,
                // Header
                div { class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                    div { class: "flex items-center space-x-4",
                        button { class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }
                        h2 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                            "Wednesday, January 15, 2025"
                        }
                        button { class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            ChevronRightIcon { class: "h-5 w-5".to_string() }
                        }
                    }
                    Button { variant: ButtonVariant::Secondary, "Today" }
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

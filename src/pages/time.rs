//! Time tracking pages

use chrono::{Datelike, Duration, NaiveDate, Utc, Weekday};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, ChevronRightIcon, DataTable,
    IconSize, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody, TableCell, TableHead,
    TableHeader, TableRow,
};
use crate::Route;

/// `PaginatedResponse<T>` envelope (`{ "data": [...], "meta": {...} }`);
/// serde drops `meta`.
#[derive(Clone, Debug, Deserialize)]
struct Paginated<T> {
    data: Vec<T>,
}

/// A time entry (`GET /api/v1/time-entries`). Names aren't joined into the
/// response, so the list renders ids/links rather than ticket titles.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTimeEntry {
    date: NaiveDate,
    #[serde(default)]
    duration_minutes: i64,
    #[serde(default)]
    ticket_id: Option<uuid::Uuid>,
    #[serde(default)]
    project_id: Option<uuid::Uuid>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_billable: bool,
    #[serde(default)]
    billing_status: String,
}

/// A ticket, used to populate the Work Item picker. Selecting one supplies
/// both `ticket_id` and the required `company_id` for the create request.
#[derive(Clone, Debug, Deserialize)]
struct TicketOption {
    id: uuid::Uuid,
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    company_id: uuid::Uuid,
}

/// A work type (`GET /api/v1/work-types`) for the required `work_type_id`.
#[derive(Clone, Debug, Deserialize)]
struct WorkTypeOption {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

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
    let entries_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<RemoteTimeEntry>>("/time-entries")
            .await
            .ok()
            .map(|p| p.data)
    });

    let snapshot = entries_resource.read_unchecked().clone();
    // `None` while loading; `Some(None)` on fetch failure; `Some(Some(rows))`.
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let entries: Vec<RemoteTimeEntry> = snapshot.flatten().unwrap_or_default();

    // Stat cards computed from the fetched entries (no hardcoded totals).
    let today = Utc::now().date_naive();
    let week_start = monday_of_week(today);
    let hours = |m: i64| format!("{:.1}h", m as f64 / 60.0);
    let today_h = hours(sum_minutes(&entries, |e| e.date == today));
    let week_h = hours(sum_minutes(&entries, |e| e.date >= week_start));
    let billable_h = hours(sum_minutes(&entries, |e| e.is_billable));
    let nonbillable_h = hours(sum_minutes(&entries, |e| !e.is_billable));
    let total = entries.len();

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

            div { class: "grid grid-cols-1 gap-5 sm:grid-cols-4 mb-6",
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Today" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "{today_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "This Week" }
                    p { class: "text-2xl font-bold text-gray-900 dark:text-white", "{week_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Billable" }
                    p { class: "text-2xl font-bold text-green-600", "{billable_h}" }
                }
                Card { class: "text-center",
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Non-Billable" }
                    p { class: "text-2xl font-bold text-gray-500", "{nonbillable_h}" }
                }
            }

            if load_failed {
                Card { class: "mb-6",
                    p { class: "text-sm text-yellow-600 dark:text-yellow-400",
                        "Could not load time entries from the server."
                    }
                }
            }

            DataTable {
                total_items: total,
                current_page: 1,
                per_page: if total == 0 { 25 } else { total },
                columns: 5,
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Date" }
                            TableHeader { "Work Item" }
                            TableHeader { "Description" }
                            TableHeader { "Hours" }
                            TableHeader { "Billable" }
                        }
                    }
                    TableBody {
                        if is_loading {
                            TableRow { TableCell { class: "text-gray-400", "Loading…" } }
                        } else if entries.is_empty() {
                            TableRow {
                                TableCell { class: "text-gray-400 italic", "No time logged yet." }
                            }
                        } else {
                            for e in entries.iter() {
                                {
                                    let hrs = hours(e.duration_minutes);
                                    let note = e
                                        .notes
                                        .clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| "-".to_string());
                                    let status = e.billing_status.clone();
                                    rsx! {
                                        TableRow {
                                            TableCell { class: "text-gray-500", "{e.date}" }
                                            TableCell {
                                                if let Some(tid) = e.ticket_id {
                                                    Link {
                                                        to: Route::TicketDetail { id: tid.to_string() },
                                                        class: "font-medium text-blue-600 hover:text-blue-500",
                                                        "Ticket"
                                                    }
                                                } else if e.project_id.is_some() {
                                                    span { class: "font-medium text-gray-700 dark:text-gray-300", "Project" }
                                                } else {
                                                    span { class: "text-gray-400", "-" }
                                                }
                                            }
                                            TableCell { class: "max-w-xs truncate", "{note}" }
                                            TableCell { class: "font-medium", "{hrs}" }
                                            TableCell {
                                                if e.is_billable {
                                                    Badge { variant: BadgeVariant::Green, "Billable" }
                                                } else {
                                                    Badge { variant: BadgeVariant::Gray, "Non-Billable" }
                                                }
                                                if !status.is_empty() && status != "not_billed" {
                                                    span { class: "ml-2 text-xs text-gray-400", "{status}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Sum `duration_minutes` over the entries matching `pred`.
fn sum_minutes(entries: &[RemoteTimeEntry], pred: impl Fn(&RemoteTimeEntry) -> bool) -> i64 {
    entries
        .iter()
        .filter(|e| pred(e))
        .map(|e| e.duration_minutes)
        .sum()
}

/// New time entry page
#[component]
pub fn TimeEntryNewPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    let mut work_item = use_signal(String::new);
    let mut work_type = use_signal(String::new);
    let mut hours = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut is_billable = use_signal(|| true);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Real pickers: a ticket supplies ticket_id + the required company_id; a
    // work type supplies the required work_type_id.
    let tickets_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<TicketOption>>("/tickets")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let work_types_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<WorkTypeOption>>("/work-types")
            .await
            .ok()
            .map(|p| p.data)
            .unwrap_or_default()
    });
    let tickets = tickets_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let work_types = work_types_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();

    let mut work_item_options = vec![SelectOption::new("", "Select a ticket")];
    work_item_options.extend(tickets.iter().map(|t| {
        SelectOption::new(
            t.id.to_string(),
            format!("{}: {}", t.ticket_number, t.title),
        )
    }));
    let mut work_type_options = vec![SelectOption::new("", "Select a work type")];
    work_type_options.extend(
        work_types
            .iter()
            .map(|w| SelectOption::new(w.id.to_string(), w.name.clone())),
    );

    let tickets_for_submit = tickets.clone();
    let err = error.read().clone();

    rsx! {
        AppLayout { title: "Log Time",
            PageHeader { title: "Log Time", subtitle: "Record time spent on work" }

            Card {
                form {
                    class: "space-y-6",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        error.set(String::new());
                        let tid = work_item.read().clone();
                        let wtid = work_type.read().clone();
                        let hrs = hours.read().clone();
                        let desc = description.read().clone();
                        let billable = *is_billable.read();

                        if tid.is_empty() {
                            error.set("Please pick a work item (ticket).".to_string());
                            return;
                        }
                        if wtid.is_empty() {
                            error.set("Please pick a work type.".to_string());
                            return;
                        }
                        let hours_val: f64 = match hrs.trim().parse() {
                            Ok(h) if h > 0.0 => h,
                            _ => {
                                error.set("Enter hours greater than 0.".to_string());
                                return;
                            }
                        };
                        let company_id = match tickets_for_submit.iter().find(|t| t.id.to_string() == tid) {
                            Some(t) => t.company_id,
                            None => {
                                error.set("Could not resolve the ticket's company.".to_string());
                                return;
                            }
                        };
                        let user_id = match auth.read().user.as_ref().map(|u| u.id) {
                            Some(id) => id,
                            None => {
                                error.set("Not signed in.".to_string());
                                return;
                            }
                        };
                        let duration_minutes = (hours_val * 60.0).round() as i64;
                        let date = Utc::now().date_naive().to_string();

                        is_submitting.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let body = serde_json::json!({
                                    "user_id": user_id,
                                    "date": date,
                                    "duration_minutes": duration_minutes,
                                    "work_type_id": wtid,
                                    "ticket_id": tid,
                                    "company_id": company_id,
                                    "notes": desc,
                                    "is_billable": billable,
                                });
                                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                                    "/time-entries",
                                    &body,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        dioxus::prelude::navigator().push(Route::TimeEntryList {});
                                    }
                                    Err(e) => {
                                        error.set(format!("Could not save time entry: {e}"));
                                    }
                                }
                            }
                            is_submitting.set(false);
                        });
                    },

                    if !err.is_empty() {
                        div { class: "rounded-md bg-red-50 dark:bg-red-900/20 p-3",
                            p { class: "text-sm text-red-600 dark:text-red-400", "{err}" }
                        }
                    }

                    div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                        Select {
                            name: "work_item",
                            label: "Work Item",
                            options: work_item_options,
                            value: work_item.read().clone(),
                            placeholder: "Select ticket",
                            required: true,
                            onchange: move |e: FormEvent| work_item.set(e.value()),
                        }
                        Select {
                            name: "work_type",
                            label: "Work Type",
                            options: work_type_options,
                            value: work_type.read().clone(),
                            placeholder: "Select work type",
                            required: true,
                            onchange: move |e: FormEvent| work_type.set(e.value()),
                        }
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

//! Calendar and dispatch pages.
//!
//! Both pages are wired to the real mokosh-server calendar API (PMS-58):
//!
//!   * The calendar fetches appointments for the visible range from
//!     `GET /api/v1/calendar/appointments?from=<rfc3339>&to=<rfc3339>`.
//!     The server expands recurring series (RRULE) into concrete
//!     occurrence instances in-memory, so the grid just renders whatever
//!     comes back; no client-side recurrence math is needed. Month,
//!     Week, and Day views all share that one resource and only differ
//!     in how they lay the returned appointments out.
//!   * "New Appointment" / "Schedule Appointment" POST to
//!     `/api/v1/calendar/appointments`; editing an existing one-off PUTs
//!     to `/{id}` and deleting DELETEs it. Expanded recurring instances
//!     are read-only (their id is the master's) so edit/delete is
//!     disabled on them.
//!   * The dispatch board fetches the aggregated
//!     `GET /api/v1/dispatch?from&to` payload (appointments + weekly
//!     availability + approved time off + current on-call) and groups
//!     appointments per technician.
//!
//! The previous demo-data fallback and the wrong `/calendar/events`
//! path are gone: an empty or failed fetch now renders an empty grid
//! plus an inline error, never seeded fixtures.

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone,
    Timelike, Utc, Weekday,
};
use dioxus::prelude::*;

use crate::components::{
    AppLayout, Button, ButtonVariant, Card, ChevronRightIcon, IconSize, Input, Modal, ModalSize,
    PageHeader, PlusIcon, Select, SelectOption, Textarea,
};
use crate::modules::calendar::{
    AppointmentResponse, CreateAppointmentRequest, DispatchResponse, OnCallNowResponse,
    TimeOffResponse, UpdateAppointmentRequest, UserAvailabilityResponse,
};

// ============================================================================
// Shared helpers
// ============================================================================

/// The day-view / week-view time grid spans 7:00 .. 19:00 (12 hours).
/// Appointments outside that window are clamped to the edges so they
/// stay visible rather than overflowing the grid.
const GRID_START_HOUR: u32 = 7;
const GRID_END_HOUR: u32 = 19;
const GRID_TOTAL_HOURS: f64 = (GRID_END_HOUR - GRID_START_HOUR) as f64;

/// Which calendar layout is active. Month is the default; Week and Day
/// are now real views (previously disabled "coming soon" stubs).
#[derive(Clone, Copy, Debug, PartialEq)]
enum CalendarView {
    Month,
    Week,
    Day,
}

/// Subset of the server's `PaginatedResponse<AppointmentResponse>`
/// envelope. The appointment range endpoint wraps its rows in
/// `{ data, meta }` exactly like the contacts list endpoints, so we
/// decode the same shape and only read `data` (the range is bounded by
/// `from`/`to`, so a single page covers it for the views we render).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PaginatedAppointments {
    #[serde(default)]
    data: Vec<AppointmentResponse>,
}

/// One user as returned by `GET /api/v1/users` (subset). Used to label
/// appointments by technician and to populate the assignee dropdown.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct RemoteUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

impl RemoteUser {
    fn display_name(&self) -> String {
        if !self.full_name.trim().is_empty() {
            return self.full_name.clone();
        }
        let joined = format!("{} {}", self.first_name, self.last_name);
        let joined = joined.trim();
        if joined.is_empty() {
            "Unknown".to_string()
        } else {
            joined.to_string()
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PaginatedUsers {
    #[serde(default)]
    data: Vec<RemoteUser>,
}

/// Add `delta` months to `date`, anchored on day 1 of the result (the
/// grid is regenerated from the month, so the day-of-month is moot).
fn shift_months(date: NaiveDate, delta: i32) -> NaiveDate {
    let total = date.year() * 12 + (date.month() as i32 - 1) + delta;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12);
    NaiveDate::from_ymd_opt(year, (month0 + 1) as u32, 1).unwrap_or(date)
}

/// Number of leading days before `first_of_month` so the grid starts on
/// a Sunday (Sun=0 .. Sat=6).
fn sunday_lead(weekday: Weekday) -> i64 {
    match weekday {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    }
}

/// 42 cells (6 rows x 7 cols) covering the month grid, starting on the
/// Sunday on/before the 1st. Each cell is `(date, is_in_active_month)`.
fn calendar_cells(active: NaiveDate) -> Vec<(NaiveDate, bool)> {
    let first_of_month =
        NaiveDate::from_ymd_opt(active.year(), active.month(), 1).unwrap_or(active);
    let grid_start = first_of_month - Duration::days(sunday_lead(first_of_month.weekday()));
    (0..42)
        .map(|i| {
            let d = grid_start + Duration::days(i);
            (d, d.month() == active.month())
        })
        .collect()
}

/// The seven dates of the week containing `date`, Sunday first.
fn week_dates(date: NaiveDate) -> Vec<NaiveDate> {
    let start = date - Duration::days(sunday_lead(date.weekday()));
    (0..7).map(|i| start + Duration::days(i)).collect()
}

/// Inclusive-exclusive local-date span the given view covers, used to
/// build the `from`/`to` query the server expands recurrence over. We
/// pad the month view to whole grid weeks so appointments on
/// leading/trailing days from adjacent months still show.
fn visible_range(active: NaiveDate, view: CalendarView) -> (NaiveDate, NaiveDate) {
    match view {
        CalendarView::Month => {
            let cells = calendar_cells(active);
            let first = cells.first().map(|c| c.0).unwrap_or(active);
            let last = cells.last().map(|c| c.0).unwrap_or(active);
            (first, last + Duration::days(1))
        }
        CalendarView::Week => {
            let dates = week_dates(active);
            let first = dates.first().copied().unwrap_or(active);
            (first, first + Duration::days(7))
        }
        CalendarView::Day => (active, active + Duration::days(1)),
    }
}

/// Convert a local date to the UTC instant at its 00:00 local boundary.
/// `from`/`to` go to the server as RFC 3339 UTC; building them from the
/// local midnight keeps the visible local range aligned with what the
/// user sees regardless of the browser's offset.
fn local_date_start_utc(d: NaiveDate) -> DateTime<Utc> {
    let naive = d.and_hms_opt(0, 0, 0).unwrap_or_else(|| {
        // 00:00 is always valid; this branch is unreachable in practice.
        d.and_time(chrono::NaiveTime::MIN)
    });
    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
        // DST gap: fall back to treating the naive value as UTC.
        chrono::LocalResult::None => Utc.from_utc_datetime(&naive),
    }
}

/// Parse a `<input type="datetime-local">` value (`YYYY-MM-DDTHH:MM`,
/// optionally with seconds) as a local wall-clock time and convert to
/// UTC for the API. Returns `None` on a malformed / empty value so the
/// form can surface a validation error instead of sending garbage.
fn parse_local_datetime_to_utc(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()?;
    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => Some(Utc.from_utc_datetime(&naive)),
    }
}

/// Format a UTC instant as the `YYYY-MM-DDTHH:MM` string a
/// `datetime-local` input expects, in the browser's local zone.
fn utc_to_datetime_local_value(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local)
        .format("%Y-%m-%dT%H:%M")
        .to_string()
}

/// Client `maxlength` caps for the appointment text fields (MAPPS-219). The
/// server stays the source of truth; these are UX nicety bounds that stop a
/// field from growing without limit before submit.
const APPT_TITLE_MAX: i64 = 255;
const APPT_LOCATION_MAX: i64 = 255;
const APPT_DESCRIPTION_MAX: i64 = 2000;

/// Validate a recurrence value as an RFC 5545 RRULE (MAPPS-219). The server
/// expands the series from this rule, so a malformed rule would be persisted
/// and then fail (or silently misbehave) during expansion with no feedback to
/// the user. We parse it client-side and reject before submit; the server
/// stays the source of truth.
///
/// This validates structure (a `;`-separated list of `KEY=VALUE` parts), that
/// a valid `FREQ` is present, that every key is a known RRULE part, that
/// `COUNT` and `UNTIL` are not both set, and that the common numeric / keyword
/// values are well-formed. It does not evaluate the rule semantically.
fn validate_rrule(rule: &str) -> Result<(), String> {
    // Tolerate an optional `RRULE:` prefix (some pastes include it).
    let body = rule
        .trim()
        .strip_prefix("RRULE:")
        .or_else(|| rule.trim().strip_prefix("rrule:"))
        .unwrap_or(rule.trim())
        .trim();
    if body.is_empty() {
        return Err("Enter an RFC 5545 recurrence rule, e.g. FREQ=WEEKLY;BYDAY=MO.".to_string());
    }

    const FREQ_VALUES: [&str; 7] = [
        "SECONDLY", "MINUTELY", "HOURLY", "DAILY", "WEEKLY", "MONTHLY", "YEARLY",
    ];
    const KNOWN_PARTS: [&str; 14] = [
        "FREQ",
        "UNTIL",
        "COUNT",
        "INTERVAL",
        "BYSECOND",
        "BYMINUTE",
        "BYHOUR",
        "BYDAY",
        "BYMONTHDAY",
        "BYYEARDAY",
        "BYWEEKNO",
        "BYMONTH",
        "BYSETPOS",
        "WKST",
    ];
    const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

    let mut has_freq = false;
    let mut has_count = false;
    let mut has_until = false;

    for part in body.split(';') {
        if part.is_empty() {
            return Err("Recurrence rule has an empty part; remove the stray ';'.".to_string());
        }
        let Some((key_raw, value)) = part.split_once('=') else {
            return Err(format!(
                "Recurrence part '{part}' is not in KEY=VALUE form."
            ));
        };
        let key = key_raw.to_ascii_uppercase();
        if !KNOWN_PARTS.contains(&key.as_str()) {
            return Err(format!("'{key_raw}' is not a valid RRULE part."));
        }
        if value.is_empty() {
            return Err(format!("Recurrence part '{key}' has no value."));
        }
        let upper = value.to_ascii_uppercase();
        match key.as_str() {
            "FREQ" => {
                if !FREQ_VALUES.contains(&upper.as_str()) {
                    return Err(format!("FREQ '{value}' is not a valid frequency."));
                }
                has_freq = true;
            }
            "COUNT" => {
                has_count = true;
                if !matches!(value.parse::<u32>(), Ok(n) if n >= 1) {
                    return Err("COUNT must be a positive whole number.".to_string());
                }
            }
            "INTERVAL" => {
                if !matches!(value.parse::<u32>(), Ok(n) if n >= 1) {
                    return Err("INTERVAL must be a positive whole number.".to_string());
                }
            }
            "UNTIL" => {
                has_until = true;
                if !is_valid_until(&upper) {
                    return Err(
                        "UNTIL must be an RFC 5545 date or date-time, e.g. 20261231T235959Z."
                            .to_string(),
                    );
                }
            }
            "BYDAY" => {
                for token in value.split(',') {
                    if !is_valid_byday(token, &WEEKDAYS) {
                        return Err(format!("BYDAY value '{token}' is invalid."));
                    }
                }
            }
            "WKST" => {
                if !WEEKDAYS.contains(&upper.as_str()) {
                    return Err(format!("WKST '{value}' must be a weekday like MO."));
                }
            }
            // Remaining BY* parts are comma-separated, optionally signed integers.
            _ => {
                for token in value.split(',') {
                    let digits = token.strip_prefix(['+', '-']).unwrap_or(token);
                    if digits.is_empty() || digits.parse::<u32>().is_err() {
                        return Err(format!("{key} value '{token}' must be a whole number."));
                    }
                }
            }
        }
    }

    if !has_freq {
        return Err("Recurrence rule must include a FREQ, e.g. FREQ=WEEKLY.".to_string());
    }
    if has_count && has_until {
        return Err("Recurrence rule cannot set both COUNT and UNTIL.".to_string());
    }
    Ok(())
}

/// A `BYDAY` token: an optional signed ordinal followed by a two-letter
/// weekday code (e.g. `MO`, `2MO`, `-1SU`).
fn is_valid_byday(token: &str, weekdays: &[&str; 7]) -> bool {
    let t = token.to_ascii_uppercase();
    if !t.is_ascii() || t.len() < 2 {
        return false;
    }
    let (ord, day) = t.split_at(t.len() - 2);
    if !weekdays.contains(&day) {
        return false;
    }
    if ord.is_empty() {
        return true;
    }
    let digits = ord.strip_prefix(['+', '-']).unwrap_or(ord);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// An `UNTIL` value: an RFC 5545 DATE (`YYYYMMDD`) or DATE-TIME
/// (`YYYYMMDDTHHMMSS`, optionally suffixed `Z` for UTC).
fn is_valid_until(value: &str) -> bool {
    let (date, time) = match value.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (value, None),
    };
    if date.len() != 8 || !date.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match time {
        None => true,
        Some(t) => {
            let t = t.strip_suffix('Z').unwrap_or(t);
            t.len() == 6 && t.chars().all(|c| c.is_ascii_digit())
        }
    }
}

/// Local clock label like `9:00 AM` for an appointment start/end.
fn time_label(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&Local).format("%-I:%M %p").to_string()
}

/// Local date an appointment falls on (used to bucket into day cells).
fn local_date(dt: DateTime<Utc>) -> NaiveDate {
    dt.with_timezone(&Local).date_naive()
}

/// Local hour-of-day as a float (e.g. 14.5 for 2:30 PM) for positioning
/// blocks in the week/day time grids.
fn local_hour_f(dt: DateTime<Utc>) -> f64 {
    let local = dt.with_timezone(&Local);
    local.hour() as f64 + local.minute() as f64 / 60.0
}

/// Tailwind block color keyed on appointment type. Falls back to slate
/// for unknown types so a future server-side type still renders.
fn type_color(appointment_type: &str) -> &'static str {
    match appointment_type {
        "ticket" => "bg-blue-500",
        "project" => "bg-green-500",
        "meeting" => "bg-purple-500",
        "other" => "bg-gray-500",
        _ => "bg-slate-500",
    }
}

/// Lighter type color for the month-grid chips (which sit on a white
/// cell and need a tinted, not solid, background).
fn type_chip_class(appointment_type: &str) -> &'static str {
    match appointment_type {
        "ticket" => "bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300",
        "project" => "bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300",
        "meeting" => "bg-purple-100 dark:bg-purple-900 text-purple-700 dark:text-purple-300",
        _ => "bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300",
    }
}

/// Appointment type options shared by the create/edit form and any
/// type-driven UI. Values match the server's CHECK constraint
/// (`ticket`, `project`, `meeting`, `other`).
fn appointment_type_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("meeting", "Meeting"),
        SelectOption::new("ticket", "Ticket"),
        SelectOption::new("project", "Project"),
        SelectOption::new("other", "Other"),
    ]
}

/// Appointment status options (server CHECK: scheduled / in_progress /
/// completed / cancelled). Only surfaced in the edit form.
fn appointment_status_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("scheduled", "Scheduled"),
        SelectOption::new("in_progress", "In Progress"),
        SelectOption::new("completed", "Completed"),
        SelectOption::new("cancelled", "Cancelled"),
    ]
}

/// Fetch the tenant's users once for assignee dropdowns / technician
/// labels. Returns an empty vec on failure (the form falls back to
/// "Me"). Reads the tenant generation so it re-runs on an org switch.
///
/// `GET /api/v1/auth/users` is server-gated to Admin / Manager (see
/// `src/modules/auth/routes.rs::list_users`), so signed-in users with
/// lower roles would get a 403. Skip the fetch for them: the dropdown
/// just shows the "Select technician..." placeholder and the user can
/// only self-assign anyway. Saves a noisy console error per page load.
fn use_users_resource() -> Resource<Vec<RemoteUser>> {
    let auth = crate::hooks::use_auth();
    use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let can_manage = auth
            .read()
            .user
            .as_ref()
            .is_some_and(|u| u.role.can_manage_users());
        if !can_manage {
            return Vec::<RemoteUser>::new();
        }
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::get_authed::<PaginatedUsers>("/auth/users?per_page=100")
                .await
                .map(|p| p.data)
                .unwrap_or_default()
        }
        #[cfg(not(feature = "web"))]
        {
            Vec::<RemoteUser>::new()
        }
    })
}

// ============================================================================
// Calendar page
// ============================================================================

/// Calendar page: month / week / day views over real appointments.
#[component]
pub fn CalendarPage() -> Element {
    let today_real = Local::now().naive_local().date();
    let mut active_date = use_signal(|| today_real);
    let mut view = use_signal(|| CalendarView::Month);

    // Modal state: None = closed, Some(None) = creating, Some(Some(appt))
    // = editing that appointment.
    let mut form_state = use_signal(|| None::<Option<AppointmentResponse>>);

    let users_resource = use_users_resource();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    // Range the active view covers, used both to build the query and to
    // re-fetch when the user navigates or switches view.
    let (range_from_date, range_to_date) = visible_range(active_date(), view());
    let from_utc = local_date_start_utc(range_from_date);
    let to_utc = local_date_start_utc(range_to_date);

    let mut appts_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            // Emit the UTC offset as `Z`, not `+00:00`. A literal `+`
            // in a query string URL-decodes to a space on the server
            // side, so `to_rfc3339()` produces a string that fails
            // chrono's `DateTime<Utc>` parser after decoding and the
            // request 400s. `SecondsFormat::Secs, use_z=true` yields
            // `2026-05-31T04:00:00Z` which round-trips cleanly.
            let path = format!(
                "/calendar/appointments?from={}&to={}",
                from_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
                to_utc.to_rfc3339_opts(SecondsFormat::Secs, true)
            );
            crate::hooks::fetch::api::get_authed::<PaginatedAppointments>(&path)
                .await
                .map(|p| p.data)
        }
        #[cfg(not(feature = "web"))]
        {
            let _ = (from_utc, to_utc);
            Ok::<Vec<AppointmentResponse>, String>(Vec::new())
        }
    });

    let appts_snapshot = appts_resource.read_unchecked();
    let is_loading = appts_snapshot.is_none();
    let fetch_failed = matches!(*appts_snapshot, Some(Err(_)));
    let appointments: Vec<AppointmentResponse> = match &*appts_snapshot {
        Some(Ok(list)) => list.clone(),
        _ => Vec::new(),
    };

    let header_label = {
        let d = active_date();
        match view() {
            CalendarView::Month => format!(
                "{} {}",
                crate::utils::datetime::month_name(d.month()),
                d.year()
            ),
            CalendarView::Week => {
                let dates = week_dates(d);
                let (first, last) = (
                    dates.first().copied().unwrap_or(d),
                    dates.last().copied().unwrap_or(d),
                );
                format!(
                    "{} {} - {} {}, {}",
                    crate::utils::datetime::month_name(first.month()),
                    first.day(),
                    crate::utils::datetime::month_name(last.month()),
                    last.day(),
                    last.year()
                )
            }
            CalendarView::Day => d.format("%A, %B %-d, %Y").to_string(),
        }
    };

    // Prev/next step depends on the active view.
    let go_prev = move |_| {
        let d = active_date();
        let next = match view() {
            CalendarView::Month => shift_months(d, -1),
            CalendarView::Week => d - Duration::days(7),
            CalendarView::Day => d - Duration::days(1),
        };
        active_date.set(next);
    };
    let go_next = move |_| {
        let d = active_date();
        let next = match view() {
            CalendarView::Month => shift_months(d, 1),
            CalendarView::Week => d + Duration::days(7),
            CalendarView::Day => d + Duration::days(1),
        };
        active_date.set(next);
    };

    rsx! {
        AppLayout { title: "Calendar",
            PageHeader {
                title: "Calendar",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| form_state.set(Some(None)),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Appointment"
                    }
                },
            }

            div { class: "grid grid-cols-1 lg:grid-cols-4 gap-6",
                div { class: "lg:col-span-3",
                    Card { padding: false,
                        // Toolbar
                        div { class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                            div { class: "flex items-center space-x-4",
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    title: "Previous",
                                    onclick: go_prev,
                                    ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                                }
                                h2 { class: "text-lg font-semibold text-gray-900 dark:text-white",
                                    "{header_label}"
                                }
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                                    title: "Next",
                                    onclick: go_next,
                                    ChevronRightIcon { class: "h-5 w-5".to_string() }
                                }
                            }
                            div { class: "flex space-x-2",
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| active_date.set(today_real),
                                    "Today"
                                }
                                div { class: "flex border border-gray-300 dark:border-gray-600 rounded-md overflow-hidden",
                                    ViewToggleButton {
                                        label: "Month",
                                        active: view() == CalendarView::Month,
                                        onclick: move |_| view.set(CalendarView::Month),
                                    }
                                    ViewToggleButton {
                                        label: "Week",
                                        active: view() == CalendarView::Week,
                                        onclick: move |_| view.set(CalendarView::Week),
                                    }
                                    ViewToggleButton {
                                        label: "Day",
                                        active: view() == CalendarView::Day,
                                        onclick: move |_| view.set(CalendarView::Day),
                                    }
                                }
                            }
                        }

                        div { class: "p-4",
                            if fetch_failed {
                                div {
                                    class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                                    "Could not load appointments. Refresh the page to retry."
                                }
                            }
                            if is_loading {
                                div { class: "py-12 text-center text-sm text-gray-500", "Loading appointments..." }
                            } else {
                                match view() {
                                    CalendarView::Month => rsx! {
                                        MonthGrid {
                                            active_date: active_date(),
                                            today: today_real,
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                        }
                                    },
                                    CalendarView::Week => rsx! {
                                        WeekGrid {
                                            active_date: active_date(),
                                            today: today_real,
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                        }
                                    },
                                    CalendarView::Day => rsx! {
                                        DayGrid {
                                            active_date: active_date(),
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                        }
                                    },
                                }
                            }
                        }
                    }
                }

                // Sidebar: agenda for the focused day (the active date).
                div { class: "space-y-6",
                    AgendaCard {
                        date: active_date(),
                        appointments: appointments.clone(),
                        users: users.clone(),
                    }
                }
            }
        }

        // Create / edit appointment modal.
        if let Some(editing) = form_state.read().clone() {
            AppointmentFormModal {
                existing: editing,
                users: users.clone(),
                default_date: active_date(),
                onclose: move |_| form_state.set(None),
                onsaved: move |_| {
                    form_state.set(None);
                    appts_resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ViewToggleButtonProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn ViewToggleButton(props: ViewToggleButtonProps) -> Element {
    let class = if props.active {
        "px-3 py-1 text-sm bg-blue-600 text-white"
    } else {
        "px-3 py-1 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800"
    };
    rsx! {
        button {
            r#type: "button",
            class: "{class}",
            aria_pressed: props.active,
            onclick: move |e| props.onclick.call(e),
            "{props.label}"
        }
    }
}

// ============================================================================
// Month view
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct MonthGridProps {
    active_date: NaiveDate,
    today: NaiveDate,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn MonthGrid(props: MonthGridProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-7 gap-px mb-2",
            for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                div { class: "text-center text-sm font-medium text-gray-500 dark:text-gray-400 py-2",
                    "{day}"
                }
            }
        }
        div { class: "grid grid-cols-7 gap-px bg-gray-200 dark:bg-gray-700 border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden",
            for (date, in_active) in calendar_cells(props.active_date) {
                {
                    let day_appts: Vec<AppointmentResponse> = props
                        .appointments
                        .iter()
                        .filter(|a| local_date(a.start_time) == date)
                        .cloned()
                        .collect();
                    rsx! {
                        MonthDayCell {
                            key: "{date}",
                            day: date.day(),
                            is_other_month: !in_active,
                            is_today: date == props.today,
                            appointments: day_appts,
                            onpick: move |a| props.onpick.call(a),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MonthDayCellProps {
    day: u32,
    is_other_month: bool,
    is_today: bool,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn MonthDayCell(props: MonthDayCellProps) -> Element {
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
    let total = props.appointments.len();

    rsx! {
        div { class: "min-h-24 p-2 {bg_class}",
            span { class: "text-sm {text_class}", "{props.day}" }
            div { class: "mt-1 space-y-1",
                for (i, appt) in props.appointments.iter().enumerate() {
                    if i < 3 {
                        {
                            let chip = type_chip_class(&appt.appointment_type);
                            let appt_clone = appt.clone();
                            let label = format!("{} {}", time_label(appt.start_time), appt.title);
                            rsx! {
                                button {
                                    r#type: "button",
                                    class: "w-full text-left text-xs truncate px-1 py-0.5 rounded {chip} hover:opacity-80",
                                    title: "{label}",
                                    onclick: move |_| props.onpick.call(appt_clone.clone()),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                if total > 3 {
                    {
                        let remaining = total - 3;
                        rsx! { span { class: "text-xs text-gray-500", "+{remaining} more" } }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Week view
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct WeekGridProps {
    active_date: NaiveDate,
    today: NaiveDate,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn WeekGrid(props: WeekGridProps) -> Element {
    let dates = week_dates(props.active_date);
    rsx! {
        div { class: "overflow-x-auto",
            div { class: "min-w-[700px]",
                // Day-of-week header row (time gutter + 7 day columns).
                div { class: "grid grid-cols-[60px_repeat(7,1fr)] border-b border-gray-200 dark:border-gray-700",
                    div { class: "p-2" }
                    for d in dates.iter() {
                        {
                            let is_today = *d == props.today;
                            let head_class = if is_today {
                                "p-2 text-center text-sm font-semibold text-blue-600 dark:text-blue-400 border-l border-gray-200 dark:border-gray-700"
                            } else {
                                "p-2 text-center text-sm font-medium text-gray-500 dark:text-gray-400 border-l border-gray-200 dark:border-gray-700"
                            };
                            let weekday = d.format("%a").to_string();
                            let daynum = d.day();
                            rsx! {
                                div { class: "{head_class}",
                                    div { "{weekday}" }
                                    div { class: "text-lg", "{daynum}" }
                                }
                            }
                        }
                    }
                }
                // Body: hour-labeled gutter + 7 positioned day columns.
                div { class: "grid grid-cols-[60px_repeat(7,1fr)]",
                    // Hour gutter.
                    div { class: "relative",
                        for hour in GRID_START_HOUR..GRID_END_HOUR {
                            {
                                let label = hour_label(hour);
                                rsx! {
                                    div { class: "h-12 text-right pr-2 text-xs text-gray-400 -mt-2",
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    for d in dates.iter() {
                        {
                            let day = *d;
                            let day_appts: Vec<AppointmentResponse> = props
                                .appointments
                                .iter()
                                .filter(|a| local_date(a.start_time) == day)
                                .cloned()
                                .collect();
                            rsx! {
                                DayColumn {
                                    key: "{day}",
                                    appointments: day_appts,
                                    onpick: move |a| props.onpick.call(a),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Hour label like `7 AM` / `12 PM` for the time gutter.
fn hour_label(hour: u32) -> String {
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let h12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    format!("{h12} {suffix}")
}

#[derive(Props, Clone, PartialEq)]
struct DayColumnProps {
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

/// One day's column in the week view: an absolutely-positioned stack of
/// appointment blocks over hour divider lines.
#[component]
fn DayColumn(props: DayColumnProps) -> Element {
    let rows = (GRID_END_HOUR - GRID_START_HOUR) as usize;
    rsx! {
        div { class: "relative border-l border-gray-200 dark:border-gray-700",
            style: "height: {rows as f64 * 3.0}rem;",
            // Hour grid lines.
            for _ in 0..rows {
                div { class: "h-12 border-b border-gray-100 dark:border-gray-800" }
            }
            // Appointment blocks.
            for appt in props.appointments.iter() {
                {
                    let (top_pct, height_pct) = block_geometry(appt);
                    let color = type_color(&appt.appointment_type);
                    let appt_clone = appt.clone();
                    let label = appt.title.clone();
                    let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                    rsx! {
                        button {
                            r#type: "button",
                            class: "absolute left-0.5 right-0.5 rounded px-1 py-0.5 text-[10px] leading-tight text-white text-left overflow-hidden shadow-sm hover:opacity-90 {color}",
                            style: "top: {top_pct:.4}%; height: {height_pct:.4}%;",
                            title: "{time}: {label}",
                            onclick: move |_| props.onpick.call(appt_clone.clone()),
                            div { class: "font-medium truncate", "{label}" }
                            div { class: "truncate opacity-90", "{time}" }
                        }
                    }
                }
            }
        }
    }
}

/// Top offset + height as percentages of the GRID_START_HOUR..GRID_END_HOUR
/// window for an appointment, clamped so out-of-window events stay visible.
fn block_geometry(appt: &AppointmentResponse) -> (f64, f64) {
    let start = local_hour_f(appt.start_time).clamp(GRID_START_HOUR as f64, GRID_END_HOUR as f64);
    let end = local_hour_f(appt.end_time).clamp(start, GRID_END_HOUR as f64);
    let top = (start - GRID_START_HOUR as f64) / GRID_TOTAL_HOURS * 100.0;
    // Floor the visible height so a zero-length / all-day item still
    // shows a tappable sliver.
    let height = (((end - start) / GRID_TOTAL_HOURS) * 100.0).max(3.0);
    (top.max(0.0), height)
}

// ============================================================================
// Day view
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct DayGridProps {
    active_date: NaiveDate,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn DayGrid(props: DayGridProps) -> Element {
    let day = props.active_date;
    let day_appts: Vec<AppointmentResponse> = props
        .appointments
        .iter()
        .filter(|a| local_date(a.start_time) == day)
        .cloned()
        .collect();
    let rows = (GRID_END_HOUR - GRID_START_HOUR) as usize;

    rsx! {
        if day_appts.is_empty() {
            div { class: "mb-3 text-sm text-gray-500", "No appointments scheduled for this day." }
        }
        div { class: "grid grid-cols-[80px_1fr]",
            // Hour gutter.
            div {
                for hour in GRID_START_HOUR..GRID_END_HOUR {
                    {
                        let label = hour_label(hour);
                        rsx! {
                            div { class: "h-16 text-right pr-3 text-xs text-gray-400 -mt-2", "{label}" }
                        }
                    }
                }
            }
            // Single positioned column (taller rows than the week view).
            div { class: "relative border-l border-gray-200 dark:border-gray-700",
                style: "height: {rows as f64 * 4.0}rem;",
                for _ in 0..rows {
                    div { class: "h-16 border-b border-gray-100 dark:border-gray-800" }
                }
                for appt in day_appts.iter() {
                    {
                        let (top_pct, height_pct) = block_geometry(appt);
                        let color = type_color(&appt.appointment_type);
                        let appt_clone = appt.clone();
                        let label = appt.title.clone();
                        let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                        let location = appt.location.clone().unwrap_or_default();
                        rsx! {
                            button {
                                r#type: "button",
                                class: "absolute left-2 right-2 rounded-md px-2 py-1 text-xs text-white text-left overflow-hidden shadow-sm hover:opacity-90 {color}",
                                style: "top: {top_pct:.4}%; height: {height_pct:.4}%;",
                                title: "{time}: {label}",
                                onclick: move |_| props.onpick.call(appt_clone.clone()),
                                div { class: "font-medium truncate", "{label}" }
                                div { class: "opacity-90", "{time}" }
                                if !location.is_empty() {
                                    div { class: "truncate opacity-90", "{location}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Sidebar agenda
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct AgendaCardProps {
    date: NaiveDate,
    appointments: Vec<AppointmentResponse>,
    users: Vec<RemoteUser>,
}

#[component]
fn AgendaCard(props: AgendaCardProps) -> Element {
    let mut day_appts: Vec<AppointmentResponse> = props
        .appointments
        .iter()
        .filter(|a| local_date(a.start_time) == props.date)
        .cloned()
        .collect();
    day_appts.sort_by_key(|a| a.start_time);
    let heading = props.date.format("%A, %b %-d").to_string();

    rsx! {
        Card { title: "{heading}",
            if day_appts.is_empty() {
                p { class: "text-sm text-gray-500 dark:text-gray-400", "Nothing scheduled." }
            } else {
                div { class: "space-y-3",
                    for appt in day_appts.iter() {
                        {
                            let border = match appt.appointment_type.as_str() {
                                "ticket" => "border-l-blue-500",
                                "project" => "border-l-green-500",
                                "meeting" => "border-l-purple-500",
                                _ => "border-l-gray-500",
                            };
                            let time = time_label(appt.start_time);
                            let title = appt.title.clone();
                            let who = props
                                .users
                                .iter()
                                .find(|u| u.id == appt.assigned_to_id)
                                .map(|u| u.display_name())
                                .unwrap_or_default();
                            rsx! {
                                div { class: "border-l-4 {border} bg-gray-50 dark:bg-gray-800 p-3 rounded-r",
                                    p { class: "text-xs text-gray-500 dark:text-gray-400", "{time}" }
                                    p { class: "font-medium text-gray-900 dark:text-white", "{title}" }
                                    if !who.is_empty() {
                                        p { class: "text-xs text-gray-500 dark:text-gray-400", "{who}" }
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

// ============================================================================
// Create / edit appointment modal
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct AppointmentFormModalProps {
    /// `None` => create a new appointment; `Some(appt)` => edit it.
    existing: Option<AppointmentResponse>,
    users: Vec<RemoteUser>,
    /// Date to seed a brand-new appointment with (the active calendar day).
    default_date: NaiveDate,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn AppointmentFormModal(props: AppointmentFormModalProps) -> Element {
    // Read the signed-in user's id unconditionally (rules of hooks) to
    // default the assignee on a new appointment.
    let auth = crate::hooks::use_auth();
    let signed_in_user_id = auth.read().user.as_ref().map(|u| u.id);

    let existing = props.existing.clone();
    let is_edit = existing.is_some();
    // Expanded recurring instances cannot be edited individually: their
    // id is the series master's, so a PUT/DELETE would hit the master.
    let is_recurring_instance = existing
        .as_ref()
        .map(|a| a.is_recurring_instance())
        .unwrap_or(false);
    let modal_title = if is_edit {
        "Edit Appointment"
    } else {
        "New Appointment"
    };

    // Seed defaults: editing pulls from the appointment; creating uses a
    // 9-10am block on the active day.
    let default_start = props
        .default_date
        .and_hms_opt(9, 0, 0)
        .map(|n| match Local.from_local_datetime(&n) {
            chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
            chrono::LocalResult::None => Utc.from_utc_datetime(&n),
        })
        .unwrap_or_else(Utc::now);
    let default_end = default_start + Duration::hours(1);

    let init_title = existing
        .as_ref()
        .map(|a| a.title.clone())
        .unwrap_or_default();
    let init_desc = existing
        .as_ref()
        .and_then(|a| a.description.clone())
        .unwrap_or_default();
    let init_type = existing
        .as_ref()
        .map(|a| a.appointment_type.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "meeting".to_string());
    let init_status = existing
        .as_ref()
        .map(|a| a.status.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "scheduled".to_string());
    let init_location = existing
        .as_ref()
        .and_then(|a| a.location.clone())
        .unwrap_or_default();
    let init_start = existing
        .as_ref()
        .map(|a| a.start_time)
        .unwrap_or(default_start);
    let init_end = existing.as_ref().map(|a| a.end_time).unwrap_or(default_end);
    let init_assignee = existing
        .as_ref()
        .map(|a| a.assigned_to_id)
        .or(signed_in_user_id)
        .map(|id| id.to_string())
        .unwrap_or_default();

    let mut title = use_signal(|| init_title);
    let mut description = use_signal(|| init_desc);
    let mut appointment_type = use_signal(|| init_type);
    let mut status = use_signal(|| init_status);
    let mut location = use_signal(|| init_location);
    let mut start_value = use_signal(|| utc_to_datetime_local_value(init_start));
    let mut end_value = use_signal(|| utc_to_datetime_local_value(init_end));
    let mut assignee = use_signal(|| init_assignee);
    let mut recurrence = use_signal(String::new);
    let mut recurrence_error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    let assignee_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "Select technician...")];
        for u in props.users.iter() {
            opts.push(SelectOption::new(u.id.to_string(), u.display_name()));
        }
        opts
    };

    let onsaved = props.onsaved;
    let onclose = props.onclose;
    let edit_id = existing.as_ref().map(|a| a.id);

    // ---- Save (create or update) ----
    let handle_save = move |_| {
        if saving() || deleting() {
            return;
        }
        let title_val = title.read().trim().to_string();
        if title_val.is_empty() {
            error.set("Title is required.".to_string());
            return;
        }
        let assignee_str = assignee.read().clone();
        let Some(assigned_to_id) = uuid::Uuid::parse_str(assignee_str.trim()).ok() else {
            error.set("Please pick a technician to assign.".to_string());
            return;
        };
        let Some(start_time) = parse_local_datetime_to_utc(&start_value.read()) else {
            error.set("Please enter a valid start time.".to_string());
            return;
        };
        let Some(end_time) = parse_local_datetime_to_utc(&end_value.read()) else {
            error.set("Please enter a valid end time.".to_string());
            return;
        };
        if end_time < start_time {
            error.set("End time must be on or after the start time.".to_string());
            return;
        }

        let desc = optional(&description.read());
        let loc = optional(&location.read());
        let type_val = appointment_type.read().clone();
        let status_val = status.read().clone();
        let rrule = optional(&recurrence.read());

        // Reject a malformed RRULE at the field before submit so an invalid
        // rule is never persisted (MAPPS-219). Empty -> one-off, no rule.
        if let Some(rule) = rrule.as_deref() {
            if let Err(msg) = validate_rrule(rule) {
                recurrence_error.set(msg);
                return;
            }
        }
        recurrence_error.set(String::new());

        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), crate::hooks::fetch::api::ApiError> = match edit_id {
                    None => {
                        let body = CreateAppointmentRequest {
                            title: title_val,
                            description: desc,
                            appointment_type: type_val,
                            ticket_id: None,
                            task_id: None,
                            site_id: None,
                            project_id: None,
                            company_id: None,
                            contact_id: None,
                            assigned_to_id,
                            start_time,
                            end_time,
                            all_day: false,
                            timezone: "UTC".to_string(),
                            location: loc,
                            recurrence_rule: rrule,
                        };
                        crate::hooks::fetch::api::post_authed_typed::<AppointmentResponse, _>(
                            "/appointments",
                            &body,
                        )
                        .await
                        .map(|_| ())
                    }
                    Some(id) => {
                        let body = UpdateAppointmentRequest {
                            title: Some(title_val),
                            description: desc,
                            appointment_type: Some(type_val),
                            assigned_to_id: Some(assigned_to_id),
                            start_time: Some(start_time),
                            end_time: Some(end_time),
                            all_day: Some(false),
                            timezone: None,
                            status: Some(status_val),
                            location: loc,
                        };
                        let path = format!("/appointments/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<AppointmentResponse, _>(
                            &path, &body,
                        )
                        .await
                        .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(e) => {
                        error.set(format!("Could not save appointment: {}", e.user_message()))
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (
                    edit_id,
                    title_val,
                    desc,
                    type_val,
                    status_val,
                    loc,
                    rrule,
                    assigned_to_id,
                    start_time,
                    end_time,
                );
            }
            saving.set(false);
        });
    };

    // ---- Delete (edit mode only, non-recurring) ----
    // MAPPS-189: the Delete button opens the styled ConfirmDialog instead
    // of the native window.confirm(); the DELETE fires from
    // `on_confirm_delete` once the user confirms.
    let mut confirming_delete = use_signal(|| false);
    let handle_delete = move |_| {
        if edit_id.is_none() || saving() || deleting() {
            return;
        }
        confirming_delete.set(true);
    };
    let on_confirm_delete = move |_: ()| {
        let Some(id) = edit_id else { return };
        if deleting() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/appointments/{id}");
                match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                    Ok(()) => onsaved.call(()),
                    Err(e) => error.set(format!(
                        "Could not delete appointment: {}",
                        e.user_message()
                    )),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = id;
            }
            deleting.set(false);
            confirming_delete.set(false);
        });
    };

    let footer = rsx! {
        if is_edit && !is_recurring_instance {
            Button {
                variant: ButtonVariant::Danger,
                loading: *deleting.read(),
                onclick: handle_delete,
                "Delete"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        if !is_recurring_instance {
            Button {
                variant: ButtonVariant::Primary,
                loading: *saving.read(),
                onclick: handle_save,
                if is_edit { "Save Changes" } else { "Create Appointment" }
            }
        }
    };

    rsx! {
        Modal {
            open: true,
            title: modal_title,
            size: ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if is_recurring_instance {
                    div {
                        class: "text-sm text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-md px-3 py-2",
                        "This is an occurrence of a recurring series. Editing individual occurrences isn't supported yet; edit the series from its first appointment."
                    }
                }
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                Input {
                    name: "appt_title",
                    label: "Title",
                    placeholder: "e.g. Onsite: Acme Corp",
                    required: true,
                    maxlength: APPT_TITLE_MAX,
                    value: title.read().clone(),
                    oninput: move |e: FormEvent| title.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    Input {
                        name: "appt_start",
                        label: "Start",
                        r#type: "datetime-local".to_string(),
                        required: true,
                        value: start_value.read().clone(),
                        oninput: move |e: FormEvent| start_value.set(e.value()),
                    }
                    Input {
                        name: "appt_end",
                        label: "End",
                        r#type: "datetime-local".to_string(),
                        required: true,
                        value: end_value.read().clone(),
                        oninput: move |e: FormEvent| end_value.set(e.value()),
                    }
                    Select {
                        name: "appt_type",
                        label: "Type",
                        options: appointment_type_options(),
                        value: appointment_type.read().clone(),
                        onchange: move |e: FormEvent| appointment_type.set(e.value()),
                    }
                    Select {
                        name: "appt_assignee",
                        label: "Assigned to",
                        options: assignee_options.clone(),
                        value: assignee.read().clone(),
                        onchange: move |e: FormEvent| assignee.set(e.value()),
                    }
                    if is_edit {
                        Select {
                            name: "appt_status",
                            label: "Status",
                            options: appointment_status_options(),
                            value: status.read().clone(),
                            onchange: move |e: FormEvent| status.set(e.value()),
                        }
                    }
                    Input {
                        name: "appt_location",
                        label: "Location",
                        placeholder: "e.g. Client site / Remote",
                        maxlength: APPT_LOCATION_MAX,
                        value: location.read().clone(),
                        oninput: move |e: FormEvent| location.set(e.value()),
                    }
                }
                if !is_edit {
                    Input {
                        name: "appt_recurrence",
                        label: "Recurrence (RRULE, optional)",
                        placeholder: "e.g. FREQ=WEEKLY;BYDAY=MO",
                        help: "RFC 5545 rule. Leave blank for a one-off. The series is anchored on the start time."
                            .to_string(),
                        error: recurrence_error.read().clone(),
                        value: recurrence.read().clone(),
                        oninput: move |e: FormEvent| {
                            recurrence_error.set(String::new());
                            recurrence.set(e.value());
                        },
                    }
                }
                Textarea {
                    name: "appt_description",
                    label: "Description",
                    rows: 3,
                    maxlength: APPT_DESCRIPTION_MAX,
                    value: description.read().clone(),
                    oninput: move |e: FormEvent| description.set(e.value()),
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete appointment".to_string(),
            message: "Delete this appointment? This cannot be undone.".to_string(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: *deleting.read(),
            onconfirm: on_confirm_delete,
            oncancel: move |_| {
                if !*deleting.read() {
                    confirming_delete.set(false);
                }
            },
        }
    }
}

/// Trim a field and return `None` when empty so optional request fields
/// are omitted (and the server keeps its existing value on update).
fn optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ============================================================================
// Dispatch board page
// ============================================================================

/// Dispatch board: aggregated technician view over `GET /api/v1/dispatch`.
/// Day-by-day navigation; appointments are grouped per assignee and laid
/// out on a shared 7am-7pm timeline, with availability, time-off, and
/// on-call context surfaced alongside.
#[component]
pub fn DispatchBoardPage() -> Element {
    let today_real = Local::now().naive_local().date();
    let mut active_day = use_signal(|| today_real);
    let mut form_state = use_signal(|| None::<Option<AppointmentResponse>>);

    let users_resource = use_users_resource();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    let day = active_day();

    let mut dispatch_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // Read the `active_day` signal INSIDE the resource closure so the
        // resource subscribes to it and refetches when the next/previous/
        // Today controls change the date. Computing the range outside the
        // closure (from a value-captured `DateTime`) advanced the header
        // but never re-ran the fetch, so the board stayed on the first
        // day it loaded (MAPPS-153).
        let day = active_day();
        let from_utc = local_date_start_utc(day);
        let to_utc = local_date_start_utc(day + Duration::days(1));
        #[cfg(feature = "web")]
        {
            // Emit the UTC offset as `Z`, not `+00:00`. A literal `+` in
            // a query string URL-decodes to a space server-side, so
            // `to_rfc3339()` produces a value that fails chrono's
            // `DateTime<Utc>` parser after decoding and the request 400s
            // (same fix as the calendar list call above).
            let path = format!(
                "/dispatch?from={}&to={}",
                from_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
                to_utc.to_rfc3339_opts(SecondsFormat::Secs, true)
            );
            crate::hooks::fetch::api::get_authed::<DispatchResponse>(&path).await
        }
        #[cfg(not(feature = "web"))]
        {
            let _ = (from_utc, to_utc);
            Ok::<DispatchResponse, String>(DispatchResponse {
                appointments: Vec::new(),
                availability: Vec::new(),
                time_off: Vec::new(),
                on_call: Vec::new(),
            })
        }
    });

    let snapshot = dispatch_resource.read_unchecked();
    let is_loading = snapshot.is_none();
    let fetch_failed = matches!(*snapshot, Some(Err(_)));
    let dispatch: Option<DispatchResponse> = match &*snapshot {
        Some(Ok(d)) => Some(d.clone()),
        _ => None,
    };

    let title = day.format("%A, %B %-d, %Y").to_string();

    rsx! {
        AppLayout { title: "Dispatch Board",
            PageHeader {
                title: "Dispatch Board",
                subtitle: "Manage technician schedules and appointments",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| form_state.set(Some(None)),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Schedule Appointment"
                    }
                },
            }

            // On-call banner (who is covering right now).
            if let Some(d) = dispatch.as_ref() {
                if !d.on_call.is_empty() {
                    OnCallBanner { on_call: d.on_call.clone(), users: users.clone() }
                }
            }

            Card { padding: false,
                div { class: "flex items-center justify-between p-4 border-b border-gray-200 dark:border-gray-700",
                    div { class: "flex items-center space-x-4",
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            title: "Previous day",
                            onclick: move |_| active_day.set(active_day() - Duration::days(1)),
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }
                        h2 { class: "text-lg font-semibold text-gray-900 dark:text-white", "{title}" }
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded",
                            title: "Next day",
                            onclick: move |_| active_day.set(active_day() + Duration::days(1)),
                            ChevronRightIcon { class: "h-5 w-5".to_string() }
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| active_day.set(today_real),
                        "Today"
                    }
                }

                div { class: "p-4",
                    if fetch_failed {
                        div {
                            class: "mb-3 text-xs text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                            "Could not load the dispatch board. Refresh the page to retry."
                        }
                    }
                    if is_loading {
                        div { class: "py-12 text-center text-sm text-gray-500", "Loading dispatch board..." }
                    } else if let Some(d) = dispatch.as_ref() {
                        DispatchTimeline {
                            day,
                            dispatch: d.clone(),
                            users: users.clone(),
                            onpick: move |a| form_state.set(Some(Some(a))),
                        }
                    }
                }
            }
        }

        if let Some(editing) = form_state.read().clone() {
            AppointmentFormModal {
                existing: editing,
                users: users.clone(),
                default_date: active_day(),
                onclose: move |_| form_state.set(None),
                onsaved: move |_| {
                    form_state.set(None);
                    dispatch_resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct OnCallBannerProps {
    on_call: Vec<OnCallNowResponse>,
    users: Vec<RemoteUser>,
}

#[component]
fn OnCallBanner(props: OnCallBannerProps) -> Element {
    rsx! {
        div { class: "mb-4 rounded-md bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 px-4 py-3",
            p { class: "text-sm font-medium text-amber-800 dark:text-amber-300 mb-1", "On call now" }
            div { class: "flex flex-wrap gap-x-6 gap-y-1 text-sm text-amber-700 dark:text-amber-300",
                for entry in props.on_call.iter() {
                    {
                        let who = entry
                            .on_call_user_id
                            .and_then(|id| props.users.iter().find(|u| u.id == id))
                            .map(|u| u.display_name())
                            .unwrap_or_else(|| "Unassigned".to_string());
                        let name = entry.schedule_name.clone();
                        rsx! {
                            span { key: "{entry.schedule_id}",
                                span { class: "font-medium", "{name}: " }
                                "{who}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DispatchTimelineProps {
    day: NaiveDate,
    dispatch: DispatchResponse,
    users: Vec<RemoteUser>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn DispatchTimeline(props: DispatchTimelineProps) -> Element {
    // Which user ids to show as rows: everyone who has an appointment,
    // an availability window, or time off today. Sorted by display name
    // for a stable layout.
    let mut user_ids: Vec<uuid::Uuid> = Vec::new();
    for a in props.dispatch.appointments.iter() {
        if !user_ids.contains(&a.assigned_to_id) {
            user_ids.push(a.assigned_to_id);
        }
    }
    for av in props.dispatch.availability.iter() {
        if !user_ids.contains(&av.user_id) {
            user_ids.push(av.user_id);
        }
    }
    for t in props.dispatch.time_off.iter() {
        if !user_ids.contains(&t.user_id) {
            user_ids.push(t.user_id);
        }
    }
    let name_for = |id: uuid::Uuid| {
        props
            .users
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.display_name())
            .unwrap_or_else(|| "Unknown".to_string())
    };
    user_ids.sort_by_key(|id| name_for(*id));

    // 0=Sunday .. 6=Saturday for matching availability windows.
    let dow = props.day.weekday().num_days_from_sunday() as i32;

    if user_ids.is_empty() {
        return rsx! {
            div { class: "py-12 text-center text-sm text-gray-500",
                "No technicians scheduled for this day."
            }
        };
    }

    rsx! {
        div { class: "overflow-x-auto",
            div { class: "min-w-[800px]",
                // Hour header.
                div { class: "grid border-b border-gray-200 dark:border-gray-700",
                    style: "grid-template-columns: 200px repeat({GRID_END_HOUR - GRID_START_HOUR}, 1fr);",
                    div { class: "p-2 bg-gray-50 dark:bg-gray-800 font-medium text-sm text-gray-500", "Technician" }
                    for hour in GRID_START_HOUR..GRID_END_HOUR {
                        {
                            let label = hour_label(hour);
                            rsx! {
                                div { class: "p-2 bg-gray-50 dark:bg-gray-800 text-center text-xs text-gray-500 border-l border-gray-200 dark:border-gray-700",
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                for id in user_ids.iter() {
                    {
                        let uid = *id;
                        let name = name_for(uid);
                        let row_appts: Vec<AppointmentResponse> = props
                            .dispatch
                            .appointments
                            .iter()
                            .filter(|a| a.assigned_to_id == uid)
                            .cloned()
                            .collect();
                        let windows: Vec<UserAvailabilityResponse> = props
                            .dispatch
                            .availability
                            .iter()
                            .filter(|w| w.user_id == uid && w.day_of_week == dow && w.is_available)
                            .cloned()
                            .collect();
                        let time_off: Vec<TimeOffResponse> = props
                            .dispatch
                            .time_off
                            .iter()
                            .filter(|t| t.user_id == uid)
                            .cloned()
                            .collect();
                        rsx! {
                            DispatchRow {
                                key: "{uid}",
                                name,
                                appointments: row_appts,
                                availability: windows,
                                time_off,
                                onpick: move |a| props.onpick.call(a),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DispatchRowProps {
    name: String,
    appointments: Vec<AppointmentResponse>,
    availability: Vec<UserAvailabilityResponse>,
    time_off: Vec<TimeOffResponse>,
    onpick: EventHandler<AppointmentResponse>,
}

#[component]
fn DispatchRow(props: DispatchRowProps) -> Element {
    let cols = GRID_END_HOUR - GRID_START_HOUR;
    let off_today = !props.time_off.is_empty();
    let off_kind = props
        .time_off
        .first()
        .map(|t| t.kind.clone())
        .unwrap_or_default();

    rsx! {
        div { class: "grid border-b border-gray-200 dark:border-gray-700 min-h-16",
            style: "grid-template-columns: 200px repeat({cols}, 1fr);",
            // Technician name + status.
            div { class: "p-2 flex items-center",
                div { class: "flex items-center",
                    div { class: "w-8 h-8 rounded-full bg-blue-100 dark:bg-blue-900 flex items-center justify-center mr-2",
                        span { class: "text-sm font-medium text-blue-600 dark:text-blue-400",
                            {props.name.chars().next().unwrap_or('?').to_string()}
                        }
                    }
                    div {
                        span { class: "font-medium text-sm text-gray-900 dark:text-white", "{props.name}" }
                        if off_today {
                            div { class: "text-xs text-amber-600 dark:text-amber-400", "Off: {off_kind}" }
                        }
                    }
                }
            }

            // Timeline area spanning the hour columns.
            div { class: "relative border-l border-gray-200 dark:border-gray-700",
                style: "grid-column: 2 / -1; min-height: 4rem;",
                // Hour divider lines.
                div { class: "absolute inset-0 grid",
                    style: "grid-template-columns: repeat({cols}, 1fr);",
                    for _ in 0..cols {
                        div { class: "border-l border-gray-100 dark:border-gray-800 first:border-l-0" }
                    }
                }
                // Availability shading (one band per available window today).
                for w in props.availability.iter() {
                    {
                        let (left, width) = availability_geometry(w);
                        rsx! {
                            div {
                                class: "absolute top-0 bottom-0 bg-green-100/50 dark:bg-green-900/20 pointer-events-none",
                                style: "left: {left:.4}%; width: {width:.4}%;",
                            }
                        }
                    }
                }
                // Appointment blocks.
                for appt in props.appointments.iter() {
                    {
                        let (left, width) = appointment_h_geometry(appt);
                        let color = type_color(&appt.appointment_type);
                        let appt_clone = appt.clone();
                        let label = appt.title.clone();
                        let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                        rsx! {
                            button {
                                r#type: "button",
                                class: "absolute top-1 bottom-1 rounded-md px-2 py-1 text-xs text-white shadow-sm overflow-hidden text-left hover:opacity-90 {color}",
                                style: "left: {left:.4}%; width: {width:.4}%;",
                                title: "{time}: {label}",
                                onclick: move |_| props.onpick.call(appt_clone.clone()),
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Horizontal left/width percentages for an appointment on the
/// GRID_START_HOUR..GRID_END_HOUR dispatch timeline.
fn appointment_h_geometry(appt: &AppointmentResponse) -> (f64, f64) {
    let start = local_hour_f(appt.start_time).clamp(GRID_START_HOUR as f64, GRID_END_HOUR as f64);
    let end = local_hour_f(appt.end_time).clamp(start, GRID_END_HOUR as f64);
    let left = (start - GRID_START_HOUR as f64) / GRID_TOTAL_HOURS * 100.0;
    let width = (((end - start) / GRID_TOTAL_HOURS) * 100.0).max(2.0);
    (left.max(0.0), width)
}

/// Horizontal band geometry for an availability window (NaiveTime based).
fn availability_geometry(w: &UserAvailabilityResponse) -> (f64, f64) {
    let start = (w.start_time.hour() as f64 + w.start_time.minute() as f64 / 60.0)
        .clamp(GRID_START_HOUR as f64, GRID_END_HOUR as f64);
    let end = (w.end_time.hour() as f64 + w.end_time.minute() as f64 / 60.0)
        .clamp(start, GRID_END_HOUR as f64);
    let left = (start - GRID_START_HOUR as f64) / GRID_TOTAL_HOURS * 100.0;
    let width = ((end - start) / GRID_TOTAL_HOURS) * 100.0;
    (left.max(0.0), width.max(0.0))
}

#[cfg(test)]
mod rrule_tests {
    use super::validate_rrule;

    #[test]
    fn accepts_valid_rules() {
        assert!(validate_rrule("FREQ=WEEKLY;BYDAY=MO").is_ok());
        assert!(validate_rrule("FREQ=DAILY").is_ok());
        assert!(validate_rrule("FREQ=MONTHLY;BYDAY=-1FR;INTERVAL=2").is_ok());
        assert!(validate_rrule("FREQ=WEEKLY;COUNT=10;WKST=SU").is_ok());
        assert!(validate_rrule("FREQ=YEARLY;UNTIL=20271231").is_ok());
        assert!(validate_rrule("FREQ=DAILY;UNTIL=20271231T235959Z").is_ok());
        // Optional RRULE: prefix and lowercase keys/values tolerated.
        assert!(validate_rrule("RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR").is_ok());
        assert!(validate_rrule("freq=weekly;byday=mo").is_ok());
    }

    #[test]
    fn rejects_garbage() {
        assert!(validate_rrule("GARBAGE NOT A RULE").is_err());
        assert!(validate_rrule("").is_err());
        assert!(validate_rrule("   ").is_err());
    }

    #[test]
    fn requires_freq() {
        assert!(validate_rrule("COUNT=5").is_err());
        assert!(validate_rrule("INTERVAL=2;BYDAY=MO").is_err());
    }

    #[test]
    fn rejects_unknown_part_and_bad_values() {
        assert!(validate_rrule("FREQ=WEEKLY;BOGUS=1").is_err());
        assert!(validate_rrule("FREQ=FORTNIGHTLY").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;COUNT=0").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;INTERVAL=x").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;BYDAY=XX").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;UNTIL=notadate").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;BYMONTH=abc").is_err());
        // Malformed structure.
        assert!(validate_rrule("FREQ=WEEKLY;;BYDAY=MO").is_err());
        assert!(validate_rrule("FREQ").is_err());
        assert!(validate_rrule("FREQ=WEEKLY;BYDAY=").is_err());
    }

    #[test]
    fn rejects_count_and_until_together() {
        assert!(validate_rrule("FREQ=DAILY;COUNT=5;UNTIL=20271231").is_err());
    }
}

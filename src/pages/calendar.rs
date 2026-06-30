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
    DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Timelike, Utc,
    Weekday,
};
use dioxus::prelude::*;

use crate::utils::datetime::{user_timezone, user_today};

use crate::components::{
    AppLayout, Button, ButtonVariant, Card, ChevronRightIcon, EmptyState, IconSize, Input, Modal,
    ModalSize, PageHeader, PencilIcon, PlusIcon, Select, SelectOption, SwatchIcon, Textarea,
};
use crate::modules::calendar::{
    AppointmentResponse, CreateAppointmentRequest, CreateSchedulingTemplateRequest,
    DispatchResponse, OnCallNowResponse, SchedulingTemplateResponse, TimeOffResponse,
    UpdateAppointmentRequest, UpdateSchedulingTemplateRequest, UserAvailabilityResponse,
};
use crate::Route;

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

/// Subset of the server's `PaginatedResponse<SchedulingTemplateResponse>`
/// envelope (MAPPS-253). The scheduling-templates list endpoint wraps its
/// rows in `{ data, meta }` like the other list endpoints; we only read
/// `data` (the picker fetches up to `per_page=100`, which covers any
/// realistic per-tenant template count).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct PaginatedTemplates {
    #[serde(default)]
    data: Vec<SchedulingTemplateResponse>,
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
    match user_timezone().from_local_datetime(&naive) {
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
    match user_timezone().from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        chrono::LocalResult::None => Some(Utc.from_utc_datetime(&naive)),
    }
}

/// Format a UTC instant as the `YYYY-MM-DDTHH:MM` string a
/// `datetime-local` input expects, in the browser's local zone.
fn utc_to_datetime_local_value(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&user_timezone())
        .format("%Y-%m-%dT%H:%M")
        .to_string()
}

/// Format a UTC instant as the `YYYY-MM-DD` string a `date` input expects,
/// in the browser's local zone. Used by the all-day / multi-day editor.
fn utc_to_date_value(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&user_timezone())
        .format("%Y-%m-%d")
        .to_string()
}

/// Parse a `<input type="date">` value (`YYYY-MM-DD`) into a `NaiveDate`.
/// Returns `None` on a malformed / empty value so the form can surface a
/// validation error.
fn parse_date_value(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Sentinel value the duration `Select` uses for the "Custom" path, where
/// the user picks an explicit End instead of a preset increment.
const DURATION_CUSTOM: &str = "custom";

/// Preset duration increments offered by the appointment form, plus a
/// "Custom" entry that switches to an explicit End-time picker. The integer
/// values are minutes; `DURATION_CUSTOM` is the sentinel for the custom
/// path (MAPPS-252).
fn duration_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("15", "15 min"),
        SelectOption::new("30", "30 min"),
        SelectOption::new("45", "45 min"),
        SelectOption::new("60", "1 hour"),
        SelectOption::new("90", "1 hour 30 min"),
        SelectOption::new("120", "2 hours"),
        SelectOption::new(DURATION_CUSTOM, "Custom"),
    ]
}

/// Add `minutes` to a `datetime-local` Start string and re-render the result
/// as a `datetime-local` value in the browser's local zone. Parses via the
/// same local path as Start, so the derived End display stays consistent with
/// what the user typed. Returns `None` when Start is malformed/empty (MAPPS-252).
fn add_minutes_to_local(start_value: &str, minutes: i64) -> Option<String> {
    let start = parse_local_datetime_to_utc(start_value)?;
    Some(utc_to_datetime_local_value(
        start + Duration::minutes(minutes),
    ))
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

/// Profile-tz clock label like `9:00 AM` for an appointment start/end.
fn time_label(dt: DateTime<Utc>) -> String {
    dt.with_timezone(&user_timezone())
        .format("%-I:%M %p")
        .to_string()
}

/// Profile-tz date an appointment falls on (used to bucket into day cells).
fn local_date(dt: DateTime<Utc>) -> NaiveDate {
    dt.with_timezone(&user_timezone()).date_naive()
}

/// Profile-tz hour-of-day as a float (e.g. 14.5 for 2:30 PM) for positioning
/// blocks in the week/day time grids.
fn local_hour_f(dt: DateTime<Utc>) -> f64 {
    let local = dt.with_timezone(&user_timezone());
    local.hour() as f64 + local.minute() as f64 / 60.0
}

/// Tailwind block color keyed on appointment type. Falls back to slate
/// for unknown types so a future server-side type still renders.
fn type_color(appointment_type: &str) -> &'static str {
    match appointment_type {
        "ticket" => "bg-blue-500",
        "project" => "bg-green-500",
        "meeting" => "bg-purple-500",
        "other" => "bg-gray-500", // theme-guard-allow: event-type data-viz dot palette
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
        _ => "bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300", // theme-guard-allow: event-type data-viz chip palette (unknown fallback, sibling of blue/green/purple)
    }
}

/// PMS-599: a Tailwind opacity fragment for appointments whose end time has
/// already passed, so elapsed events read as muted across every view. Empty for
/// current/future appointments. Paired with the blocks' existing `hover:opacity-*`
/// so hovering a past event un-mutes it. Back-dating is intentional (tracking),
/// so past appointments are shown, just dimmed.
fn past_class(appt: &AppointmentResponse) -> &'static str {
    if appt.end_time < Utc::now() {
        "opacity-50"
    } else {
        ""
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

/// Scheduling-template `kind` options for the management form (MAPPS-253).
/// Values match the server CHECK (`dispatch` / `calendar`).
fn template_kind_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("dispatch", "Dispatch (on-site work)"),
        SelectOption::new("calendar", "Calendar (client calls / status updates)"),
    ]
}

/// Human label for a template `kind` value, used in picker option labels and
/// the management list. Falls back to the raw value for an unknown kind.
fn template_kind_label(kind: &str) -> &'static str {
    match kind {
        "dispatch" => "Dispatch",
        "calendar" => "Calendar",
        _ => "Template",
    }
}

/// Render a duration in minutes as a compact `2h`, `1h 30m`, or `45m` label
/// for template picker options (MAPPS-253).
fn humanize_minutes(minutes: i32) -> String {
    let minutes = minutes.max(0);
    let h = minutes / 60;
    let m = minutes % 60;
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// Picker option label for a template, e.g.
/// `Dispatch: On-site visit (2h)` (MAPPS-253). Lets the dispatcher tell the
/// kind and length apart at a glance.
fn template_option_label(t: &SchedulingTemplateResponse) -> String {
    format!(
        "{}: {} ({})",
        template_kind_label(&t.kind),
        t.name,
        humanize_minutes(t.duration_minutes)
    )
}

/// Advisory helper text describing a dispatch template's travel buffers, or
/// `None` when both are zero / the template is a calendar template
/// (MAPPS-253). The buffers are display-only for this issue: the saved
/// appointment spans only the on-site duration.
fn travel_buffer_help(t: &SchedulingTemplateResponse) -> Option<String> {
    if t.kind != "dispatch" {
        return None;
    }
    let before = t.travel_before_minutes.max(0);
    let after = t.travel_after_minutes.max(0);
    match (before, after) {
        (0, 0) => None,
        (b, 0) => Some(format!("Includes {} travel before.", humanize_minutes(b))),
        (0, a) => Some(format!("Includes {} travel after.", humanize_minutes(a))),
        (b, a) => Some(format!(
            "Includes {} travel before and {} after.",
            humanize_minutes(b),
            humanize_minutes(a)
        )),
    }
}

/// Fetch the tenant's scheduling templates for the picker / management page
/// (MAPPS-253). `kind` filters server-side to one kind (`dispatch` /
/// `calendar`); `None` fetches both. Returns an empty vec on failure so the
/// picker just shows the blank-appointment default. Reads the tenant
/// generation so it re-runs on an org switch, mirroring
/// [`use_users_resource`].
fn use_templates_resource(kind: Option<&'static str>) -> Resource<Vec<SchedulingTemplateResponse>> {
    use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "web")]
        {
            let path = match kind {
                Some(k) => format!("/scheduling-templates?kind={k}&per_page=100"),
                None => "/scheduling-templates?per_page=100".to_string(),
            };
            crate::hooks::fetch::api::get_authed::<PaginatedTemplates>(&path)
                .await
                .map(|p| p.data)
                .unwrap_or_default()
        }
        #[cfg(not(feature = "web"))]
        {
            let _ = kind;
            Vec::<SchedulingTemplateResponse>::new()
        }
    })
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
    let today_real = user_today();
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
                        div { class: "flex items-center justify-between p-4 border-b border-line",
                            div { class: "flex items-center space-x-4",
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-surface-2 rounded",
                                    title: "Previous",
                                    aria_label: "Previous",
                                    onclick: go_prev,
                                    ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                                }
                                h2 { class: "text-lg font-semibold text-content",
                                    "{header_label}"
                                }
                                button {
                                    r#type: "button",
                                    class: "p-2 hover:bg-surface-2 rounded",
                                    title: "Next",
                                    aria_label: "Next",
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
                                div { class: "flex border border-line rounded-md overflow-hidden",
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
                                div { class: "py-12 text-center text-sm text-muted", "Loading appointments..." }
                            } else {
                                match view() {
                                    CalendarView::Month => rsx! {
                                        MonthGrid {
                                            active_date: active_date(),
                                            today: today_real,
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                            oncreate: move |d: NaiveDate| {
                                                active_date.set(d);
                                                form_state.set(Some(None));
                                            },
                                        }
                                    },
                                    CalendarView::Week => rsx! {
                                        WeekGrid {
                                            active_date: active_date(),
                                            today: today_real,
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                            oncreate: move |d: NaiveDate| {
                                                active_date.set(d);
                                                form_state.set(Some(None));
                                            },
                                        }
                                    },
                                    CalendarView::Day => rsx! {
                                        DayGrid {
                                            active_date: active_date(),
                                            appointments: appointments.clone(),
                                            onpick: move |a| form_state.set(Some(Some(a))),
                                            oncreate: move |d: NaiveDate| {
                                                active_date.set(d);
                                                form_state.set(Some(None));
                                            },
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
        "px-3 py-1 text-sm bg-accent text-on-accent"
    } else {
        "px-3 py-1 text-sm text-content hover:bg-surface-2"
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
    /// MAPPS-319: open the New Appointment modal pre-filled with the
    /// clicked day. Fired from `MonthDayCell` on empty-area click;
    /// appointment chips inside the cell stop propagation so their own
    /// onpick handler runs instead.
    oncreate: EventHandler<NaiveDate>,
}

#[component]
fn MonthGrid(props: MonthGridProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-7 gap-px mb-2",
            for day in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] {
                div { class: "text-center text-sm font-medium text-muted py-2",
                    "{day}"
                }
            }
        }
        div { class: "grid grid-cols-7 gap-px bg-surface-2 border border-line rounded-lg overflow-hidden",
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
                            date,
                            day: date.day(),
                            is_other_month: !in_active,
                            is_today: date == props.today,
                            appointments: day_appts,
                            onpick: move |a| props.onpick.call(a),
                            oncreate: move |d| props.oncreate.call(d),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MonthDayCellProps {
    date: NaiveDate,
    day: u32,
    is_other_month: bool,
    is_today: bool,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
    oncreate: EventHandler<NaiveDate>,
}

#[component]
fn MonthDayCell(props: MonthDayCellProps) -> Element {
    // MAPPS-301: the today highlight was a flat `bg-accent-50`, which is
    // near-white in Light mode (so today blends into the surrounding
    // `bg-surface` cells - the QA "very white" report) and jarringly
    // bright in Dark mode (no dark variant). Use the project's standard
    // accent-50-light + accent-900/30-dark pair (already used by
    // `layout.rs::active_nav_class` and the inline-create dropdown rows
    // in the pickers) plus an inset accent ring so today is unambiguous
    // at WCAG AA contrast against the neutral cells in both themes.
    let bg_class = if props.is_today {
        "bg-accent-50 dark:bg-accent-900/30 ring-1 ring-inset ring-accent"
    } else {
        "bg-surface"
    };
    let text_class = if props.is_other_month {
        "text-subtle"
    } else if props.is_today {
        "text-accent font-bold"
    } else {
        "text-content"
    };
    let total = props.appointments.len();
    let cell_date = props.date;

    rsx! {
        // MAPPS-319: the whole cell is clickable; the click opens the
        // New Appointment modal pre-filled with this day. Appointment
        // chips below stop propagation so their own onpick fires instead.
        div {
            class: "min-h-24 p-2 cursor-pointer {bg_class}",
            role: "button",
            tabindex: "0",
            aria_label: "Create appointment on this day",
            onclick: move |_| props.oncreate.call(cell_date),
            span { class: "text-sm {text_class}", "{props.day}" }
            div { class: "mt-1 space-y-1",
                for (i, appt) in props.appointments.iter().enumerate() {
                    if i < 3 {
                        {
                            let chip = type_chip_class(&appt.appointment_type);
                            let past = past_class(appt);
                            let appt_clone = appt.clone();
                            let label = format!("{} {}", time_label(appt.start_time), appt.title);
                            rsx! {
                                button {
                                    r#type: "button",
                                    class: "w-full text-left text-xs truncate px-1 py-0.5 rounded {chip} {past} hover:opacity-80",
                                    title: "{label}",
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        props.onpick.call(appt_clone.clone());
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                if total > 3 {
                    {
                        let remaining = total - 3;
                        rsx! { span { class: "text-xs text-muted", "+{remaining} more" } }
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
    /// MAPPS-319: open the New Appointment modal pre-filled with the
    /// clicked column's date. Fired from `DayColumn` on empty-area
    /// click; appointment blocks stop propagation.
    oncreate: EventHandler<NaiveDate>,
}

#[component]
fn WeekGrid(props: WeekGridProps) -> Element {
    let dates = week_dates(props.active_date);
    rsx! {
        div { class: "overflow-x-auto",
            div { class: "min-w-[700px]",
                // Day-of-week header row (time gutter + 7 day columns).
                div { class: "grid grid-cols-[60px_repeat(7,1fr)] border-b border-line",
                    div { class: "p-2" }
                    for d in dates.iter() {
                        {
                            let is_today = *d == props.today;
                            let head_class = if is_today {
                                "p-2 text-center text-sm font-semibold text-accent border-l border-line"
                            } else {
                                "p-2 text-center text-sm font-medium text-muted border-l border-line"
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
                                    div { class: "h-12 text-right pr-2 text-xs text-subtle -mt-2",
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
                                    day,
                                    appointments: day_appts,
                                    onpick: move |a| props.onpick.call(a),
                                    oncreate: move |d| props.oncreate.call(d),
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
    day: NaiveDate,
    appointments: Vec<AppointmentResponse>,
    onpick: EventHandler<AppointmentResponse>,
    oncreate: EventHandler<NaiveDate>,
}

/// One day's column in the week view: an absolutely-positioned stack of
/// appointment blocks over hour divider lines.
#[component]
fn DayColumn(props: DayColumnProps) -> Element {
    let rows = (GRID_END_HOUR - GRID_START_HOUR) as usize;
    let day = props.day;
    rsx! {
        // MAPPS-319: column-level onclick opens New Appointment pre-
        // filled with this day. Appointment blocks below stop
        // propagation so their own onpick (edit) fires instead.
        div {
            class: "relative border-l border-line cursor-pointer",
            style: "height: {rows as f64 * 3.0}rem;",
            role: "button",
            tabindex: "0",
            aria_label: "Create appointment on this day",
            onclick: move |_| props.oncreate.call(day),
            // Hour grid lines.
            for _ in 0..rows {
                div { class: "h-12 border-b border-line" }
            }
            // Appointment blocks.
            for appt in props.appointments.iter() {
                {
                    let (top_pct, height_pct) = block_geometry(appt);
                    let color = type_color(&appt.appointment_type);
                    let past = past_class(appt);
                    let appt_clone = appt.clone();
                    let label = appt.title.clone();
                    let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                    rsx! {
                        button {
                            r#type: "button",
                            class: "absolute left-0.5 right-0.5 rounded px-1 py-0.5 text-[10px] leading-tight text-white text-left overflow-hidden shadow-sm hover:opacity-90 {color} {past}",
                            style: "top: {top_pct:.4}%; height: {height_pct:.4}%;",
                            title: "{time}: {label}",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                props.onpick.call(appt_clone.clone());
                            },
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
    /// MAPPS-319: open the New Appointment modal pre-filled with
    /// `active_date` on empty-area click. Appointment blocks stop
    /// propagation so the edit path fires for them instead.
    oncreate: EventHandler<NaiveDate>,
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
            div { class: "mb-3 text-sm text-muted", "No appointments scheduled for this day." }
        }
        div { class: "grid grid-cols-[80px_1fr]",
            // Hour gutter.
            div {
                for hour in GRID_START_HOUR..GRID_END_HOUR {
                    {
                        let label = hour_label(hour);
                        rsx! {
                            div { class: "h-16 text-right pr-3 text-xs text-subtle -mt-2", "{label}" }
                        }
                    }
                }
            }
            // Single positioned column (taller rows than the week view).
            // MAPPS-319: column-level onclick opens New Appointment.
            div {
                class: "relative border-l border-line cursor-pointer",
                style: "height: {rows as f64 * 4.0}rem;",
                role: "button",
                tabindex: "0",
                aria_label: "Create appointment on this day",
                onclick: move |_| props.oncreate.call(day),
                for _ in 0..rows {
                    div { class: "h-16 border-b border-line" }
                }
                for appt in day_appts.iter() {
                    {
                        let (top_pct, height_pct) = block_geometry(appt);
                        let color = type_color(&appt.appointment_type);
                        let past = past_class(appt);
                        let appt_clone = appt.clone();
                        let label = appt.title.clone();
                        let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                        let location = appt.location.clone().unwrap_or_default();
                        rsx! {
                            button {
                                r#type: "button",
                                class: "absolute left-2 right-2 rounded-md px-2 py-1 text-xs text-white text-left overflow-hidden shadow-sm hover:opacity-90 {color} {past}",
                                style: "top: {top_pct:.4}%; height: {height_pct:.4}%;",
                                title: "{time}: {label}",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    props.onpick.call(appt_clone.clone());
                                },
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
                p { class: "text-sm text-muted", "Nothing scheduled." }
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
                            let past = past_class(appt);
                            let time = time_label(appt.start_time);
                            let title = appt.title.clone();
                            let who = props
                                .users
                                .iter()
                                .find(|u| u.id == appt.assigned_to_id)
                                .map(|u| u.display_name())
                                .unwrap_or_default();
                            rsx! {
                                div { class: "border-l-4 {border} bg-surface-2 p-3 rounded-r {past}",
                                    p { class: "text-xs text-muted", "{time}" }
                                    p { class: "font-medium text-content", "{title}" }
                                    if !who.is_empty() {
                                        p { class: "text-xs text-muted", "{who}" }
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
        .map(|n| match user_timezone().from_local_datetime(&n) {
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
    // The RRULE lives only on the series master; expanded occurrences carry
    // `None`, so editing one shows an empty field (MAPPS-236).
    let init_recurrence = existing
        .as_ref()
        .and_then(|a| a.recurrence_rule.clone())
        .unwrap_or_default();

    let mut title = use_signal(|| init_title);
    let mut description = use_signal(|| init_desc);
    let mut appointment_type = use_signal(|| init_type);
    let mut status = use_signal(|| init_status);
    let mut location = use_signal(|| init_location);
    // Start + duration editing model (MAPPS-252). End is derived from
    // Start + `duration_minutes`, never stored independently, so editing
    // Start shifts End by the same delta with the duration intact. The
    // "Custom" path falls back to an explicit End signal and back-computes
    // the duration on save.
    let init_all_day = existing.as_ref().map(|a| a.all_day).unwrap_or(false);
    let init_duration_minutes = (init_end - init_start).num_minutes().max(0);
    // A new appointment defaults to the 9-10am block => 60 minutes; an
    // existing one seeds from its persisted span.
    let seed_duration = if is_edit { init_duration_minutes } else { 60 };
    // Pick the matching preset when the seed duration is one of the offered
    // increments, otherwise drop into the Custom path with an explicit End.
    const PRESETS: [i64; 6] = [15, 30, 45, 60, 90, 120];
    let seed_is_preset = PRESETS.contains(&seed_duration);
    // MAPPS-299: compute the timezone-dependent seed strings BEFORE the
    // `use_signal` calls. The conversion routines reach into
    // `user_timezone()` which calls `try_use_context::<Signal<AuthContext>>()`
    // - itself a hook. Running them inside a `use_signal` initialiser
    // therefore violates the rules of hooks ("hook list already
    // borrowed; using a hook inside a hook") and panics on first
    // mount in the Calendar parent (Dispatch happens to dodge the
    // same crash because its mount path runs the modal's hook setup
    // when the auth context is in a different borrow state). Hoisting
    // the timezone reads above the `use_signal` calls keeps every
    // initialiser pure.
    let init_start_local = utc_to_datetime_local_value(init_start);
    let init_end_local = utc_to_datetime_local_value(init_end);
    let init_start_date = utc_to_date_value(init_start);
    let init_end_date = {
        let inclusive_end = (init_end - Duration::days(1)).max(init_start);
        utc_to_date_value(inclusive_end)
    };
    let mut start_value = use_signal(|| init_start_local.clone());
    let mut end_value = use_signal(|| init_end_local.clone());
    let mut duration_minutes = use_signal(|| seed_duration);
    // Which entry the duration `Select` shows: a preset string or the
    // `DURATION_CUSTOM` sentinel.
    let mut duration_choice = use_signal(|| {
        if seed_is_preset {
            seed_duration.to_string()
        } else {
            DURATION_CUSTOM.to_string()
        }
    });
    let mut all_day = use_signal(|| init_all_day);
    let mut start_date_value = use_signal(|| init_start_date.clone());
    // All-day End date is inclusive in the UI: the persisted End is local
    // 00:00 of the day AFTER this date, so a single-day span reads as one day.
    let mut end_date_value = use_signal(|| init_end_date.clone());
    let mut assignee = use_signal(|| init_assignee);
    let mut recurrence = use_signal(|| init_recurrence);
    let mut recurrence_error = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);

    // Template picker (create mode only; MAPPS-253). Fetched unconditionally
    // to satisfy the rules of hooks, but only rendered when creating. The
    // selected template's id drives the picker `Select` value; applying it
    // pre-fills the existing field signals and stashes the template's linked
    // ticket so the create body can carry it. Buffers are surfaced as
    // advisory helper text only.
    let templates_resource = use_templates_resource(None);
    let templates = templates_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let mut selected_template = use_signal(String::new);
    let mut template_ticket_id = use_signal(|| None::<uuid::Uuid>);
    // Travel-buffer advisory text for the currently selected dispatch
    // template, recomputed from the picker selection.
    let selected_buffer_help: Option<String> = {
        let sel = selected_template.read().clone();
        templates
            .iter()
            .find(|t| t.id.to_string() == sel)
            .and_then(travel_buffer_help)
    };

    let assignee_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "Select technician...")];
        for u in props.users.iter() {
            opts.push(SelectOption::new(u.id.to_string(), u.display_name()));
        }
        opts
    };

    // Picker options: a blank-appointment default plus one entry per
    // template, labeled by kind and length (MAPPS-253).
    let template_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "None (blank appointment)")];
        for t in templates.iter() {
            opts.push(SelectOption::new(
                t.id.to_string(),
                template_option_label(t),
            ));
        }
        opts
    };
    // Snapshot the templates the picker's onchange handler needs so the
    // closure owns its data (it cannot borrow `templates` across the move).
    let pickable_templates = templates.clone();

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
        let is_all_day = all_day();
        // Resolve the start/end instants from whichever editor is active:
        // an all-day date span, or the timed start + duration model.
        let (start_time, end_time) = if is_all_day {
            let Some(start_date) = parse_date_value(&start_date_value.read()) else {
                error.set("Please enter a valid start date.".to_string());
                return;
            };
            let Some(end_date) = parse_date_value(&end_date_value.read()) else {
                error.set("Please enter a valid end date.".to_string());
                return;
            };
            if end_date < start_date {
                error.set("End date must be on or after the start date.".to_string());
                return;
            }
            // Span whole local days: Start at local 00:00 of the start date,
            // End at local 00:00 of the day after the (inclusive) end date,
            // so a single-day all-day event is a 24h span the grids render.
            let start = local_date_start_utc(start_date);
            let end = local_date_start_utc(end_date + Duration::days(1));
            (start, end)
        } else {
            let Some(start) = parse_local_datetime_to_utc(&start_value.read()) else {
                error.set("Please enter a valid start time.".to_string());
                return;
            };
            // End follows Start + the chosen duration. The custom path stores
            // an explicit End in `end_value`; presets store minutes directly.
            let end = if duration_choice.read().as_str() == DURATION_CUSTOM {
                let Some(custom_end) = parse_local_datetime_to_utc(&end_value.read()) else {
                    error.set("Please enter a valid end time.".to_string());
                    return;
                };
                custom_end
            } else {
                start + Duration::minutes(duration_minutes())
            };
            (start, end)
        };
        // Defensive guard: a non-positive custom duration would invert the
        // span. Presets and the all-day path can never trip this.
        if end_time < start_time {
            error.set("End time must be on or after the start time.".to_string());
            return;
        }

        let desc = optional(&description.read());
        let loc = optional(&location.read());
        let type_val = appointment_type.read().clone();
        let status_val = status.read().clone();
        let rrule = optional(&recurrence.read());
        // Linked ticket from a selected template (create mode only;
        // MAPPS-253). `None` for a blank appointment or in edit mode.
        let ticket_id = *template_ticket_id.read();

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
                            ticket_id,
                            task_id: None,
                            site_id: None,
                            project_id: None,
                            company_id: None,
                            contact_id: None,
                            assigned_to_id,
                            start_time,
                            end_time,
                            all_day: is_all_day,
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
                            all_day: Some(is_all_day),
                            timezone: None,
                            status: Some(status_val),
                            location: loc,
                            recurrence_rule: rrule,
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
                // Template picker + scheduling-vs-template explainer
                // (create mode only; MAPPS-253). Hidden when editing an
                // existing appointment, where a template does not apply.
                if !is_edit {
                    div {
                        class: "text-sm text-blue-700 dark:text-blue-300 bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-900 rounded-md px-3 py-2",
                        "You are scheduling an appointment now. To save a reusable shape (type, duration, title) for next time, create a template on the Scheduling Templates page instead."
                    }
                    Select {
                        name: "appt_template",
                        label: "Start from a template",
                        options: template_options.clone(),
                        help: "Templates pre-fill an appointment; pick one, then choose a start time. Dispatch templates are for on-site work, calendar templates for client calls and status updates."
                            .to_string(),
                        value: selected_template.read().clone(),
                        onchange: move |e: FormEvent| {
                            let id = e.value();
                            selected_template.set(id.clone());
                            let Some(t) = pickable_templates
                                .iter()
                                .find(|t| t.id.to_string() == id)
                            else {
                                // "None (blank appointment)": clear the linked
                                // ticket but leave the user's other edits.
                                template_ticket_id.set(None);
                                return;
                            };
                            // Pre-fill type / title / location from the
                            // template defaults; blanks fall back to the
                            // existing field value rather than clobbering it.
                            if !t.appointment_type.trim().is_empty() {
                                appointment_type.set(t.appointment_type.clone());
                            }
                            if let Some(dt) = t.default_title.as_ref().filter(|s| !s.trim().is_empty())
                            {
                                title.set(dt.clone());
                            }
                            if let Some(dl) = t.default_location.as_ref() {
                                location.set(dl.clone());
                            }
                            template_ticket_id.set(t.default_ticket_id);
                            // Duration model (MAPPS-252): land on the matching
                            // preset, or fall into the Custom path with an
                            // explicit End computed from the current Start.
                            let mins = i64::from(t.duration_minutes.max(0));
                            duration_minutes.set(mins);
                            if PRESETS.contains(&mins) {
                                duration_choice.set(mins.to_string());
                            } else {
                                if let Some(end) =
                                    add_minutes_to_local(&start_value.read(), mins)
                                {
                                    end_value.set(end);
                                }
                                duration_choice.set(DURATION_CUSTOM.to_string());
                            }
                        },
                    }
                    div { class: "text-sm",
                        Link {
                            to: Route::SchedulingTemplates {},
                            class: "text-accent hover:opacity-90",
                            "Manage templates..."
                        }
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
                crate::components::Checkbox {
                    name: "appt_all_day",
                    label: "All day",
                    checked: all_day(),
                    help: "Switch to a multi-day date span. Off keeps a single date with start and end times.",
                    onchange: move |e: FormEvent| all_day.set(e.checked()),
                }
                if all_day() {
                    // Multi-day all-day span: Start date + (inclusive) End date.
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        Input {
                            name: "appt_start_date",
                            label: "Start date",
                            r#type: "date".to_string(),
                            required: true,
                            value: start_date_value.read().clone(),
                            oninput: move |e: FormEvent| start_date_value.set(e.value()),
                        }
                        Input {
                            name: "appt_end_date",
                            label: "End date",
                            r#type: "date".to_string(),
                            required: true,
                            help: "Inclusive. The event spans whole days through this date.".to_string(),
                            value: end_date_value.read().clone(),
                            oninput: move |e: FormEvent| end_date_value.set(e.value()),
                        }
                    }
                } else {
                    // Timed single-date: Start + duration; End is derived
                    // (preset) or explicit (Custom).
                    div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                        Input {
                            name: "appt_start",
                            label: "Start",
                            r#type: "datetime-local".to_string(),
                            required: true,
                            value: start_value.read().clone(),
                            oninput: move |e: FormEvent| {
                                let next = e.value();
                                // Keep the Custom End anchored to the new Start
                                // by shifting it by the same delta, preserving
                                // the custom duration.
                                if duration_choice.read().as_str() == DURATION_CUSTOM {
                                    if let Some(shifted) =
                                        add_minutes_to_local(&next, duration_minutes())
                                    {
                                        end_value.set(shifted);
                                    }
                                }
                                start_value.set(next);
                            },
                        }
                        Select {
                            name: "appt_duration",
                            label: "Duration",
                            options: duration_options(),
                            help: "End time follows the start time; change the duration to resize."
                                .to_string(),
                            value: duration_choice.read().clone(),
                            onchange: move |e: FormEvent| {
                                let choice = e.value();
                                if choice.as_str() == DURATION_CUSTOM {
                                    // Seed the explicit End from the current
                                    // Start + last known duration.
                                    if let Some(end) =
                                        add_minutes_to_local(&start_value.read(), duration_minutes())
                                    {
                                        end_value.set(end);
                                    }
                                } else if let Ok(mins) = choice.parse::<i64>() {
                                    duration_minutes.set(mins);
                                }
                                duration_choice.set(choice);
                            },
                        }
                        if duration_choice.read().as_str() == DURATION_CUSTOM {
                            Input {
                                name: "appt_end",
                                label: "End",
                                r#type: "datetime-local".to_string(),
                                required: true,
                                value: end_value.read().clone(),
                                oninput: move |e: FormEvent| {
                                    let next = e.value();
                                    // Back-compute the duration so a later Start
                                    // edit preserves this custom span.
                                    if let (Some(start), Some(end)) = (
                                        parse_local_datetime_to_utc(&start_value.read()),
                                        parse_local_datetime_to_utc(&next),
                                    ) {
                                        duration_minutes.set((end - start).num_minutes());
                                    }
                                    end_value.set(next);
                                },
                            }
                        } else {
                            // Read-only derived End display in the local zone.
                            div { class: "space-y-1",
                                label { class: "block text-sm font-medium text-content", "End" }
                                div {
                                    class: "block w-full rounded-md border-line bg-surface text-muted sm:text-sm px-3 py-2",
                                    {
                                        add_minutes_to_local(&start_value.read(), duration_minutes())
                                            .unwrap_or_else(|| "-".to_string())
                                    }
                                }
                            }
                        }
                    }
                    // Advisory travel-buffer note for a selected dispatch
                    // template (MAPPS-253). Display-only: the saved
                    // appointment still spans just the on-site duration.
                    if let Some(buffer_help) = selected_buffer_help.clone() {
                        p { class: "text-sm leading-5 text-muted", "{buffer_help}" }
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
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
    let today_real = user_today();
    let mut active_day = use_signal(|| today_real);
    let mut form_state = use_signal(|| None::<Option<AppointmentResponse>>);
    // MAPPS-280: Day / Week / Month view-mode toggle. Day stays the
    // per-technician swimlane (the rich existing render); Week and
    // Month re-use the calendar's WeekGrid / MonthGrid against the
    // dispatch appointments so a dispatcher can plan a week without
    // navigating away. The data range expands with the view so the
    // grid has every appointment in scope.
    let mut view = use_signal(|| CalendarView::Day);

    let users_resource = use_users_resource();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    let day = active_day();

    let mut dispatch_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        // Read the `active_day` + `view` signals INSIDE the resource closure
        // so the resource subscribes to them and refetches when the
        // next/previous/Today/view controls change the range. Computing
        // outside the closure (from a value-captured `DateTime`) advanced
        // the header but never re-ran the fetch (MAPPS-153 fix).
        let day = active_day();
        let view = *view.read();
        let (from_date, to_date) = match view {
            CalendarView::Day => (day, day + Duration::days(1)),
            CalendarView::Week => {
                let monday = day - Duration::days(day.weekday().num_days_from_monday() as i64);
                (monday, monday + Duration::days(7))
            }
            CalendarView::Month => {
                let first = day.with_day(1).unwrap_or(day);
                let next_month = if first.month() == 12 {
                    chrono::NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
                        .unwrap_or(first + Duration::days(31))
                } else {
                    chrono::NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
                        .unwrap_or(first + Duration::days(31))
                };
                (first, next_month)
            }
        };
        let from_utc = local_date_start_utc(from_date);
        let to_utc = local_date_start_utc(to_date);
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

    // MAPPS-280: title reflects the active view's range.
    let title = match *view.read() {
        CalendarView::Day => day.format("%A, %B %-d, %Y").to_string(),
        CalendarView::Week => {
            let monday = day - Duration::days(day.weekday().num_days_from_monday() as i64);
            let sunday = monday + Duration::days(6);
            format!(
                "{} - {}",
                monday.format("%b %-d"),
                sunday.format("%b %-d, %Y")
            )
        }
        CalendarView::Month => day.format("%B %Y").to_string(),
    };

    rsx! {
        AppLayout { title: "Dispatch Board",
            PageHeader {
                title: "Dispatch Board",
                subtitle: "A per-technician day view: each technician's appointments, availability, time off, and on-call status laid out on one timeline. Schedule on-site work here, or start from a dispatch template.",
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
                div { class: "flex items-center justify-between p-4 border-b border-line",
                    div { class: "flex items-center space-x-4",
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-surface-2 rounded",
                            title: "Previous",
                            aria_label: "Previous",
                            onclick: move |_| {
                                let v = *view.read();
                                let step = match v {
                                    CalendarView::Day => Duration::days(1),
                                    CalendarView::Week => Duration::days(7),
                                    CalendarView::Month => Duration::days(28),
                                };
                                active_day.set(active_day() - step);
                            },
                            ChevronRightIcon { class: "h-5 w-5 rotate-180".to_string() }
                        }
                        h2 { class: "text-lg font-semibold text-content", "{title}" }
                        button {
                            r#type: "button",
                            class: "p-2 hover:bg-surface-2 rounded",
                            title: "Next",
                            aria_label: "Next",
                            onclick: move |_| {
                                let v = *view.read();
                                let step = match v {
                                    CalendarView::Day => Duration::days(1),
                                    CalendarView::Week => Duration::days(7),
                                    CalendarView::Month => Duration::days(28),
                                };
                                active_day.set(active_day() + step);
                            },
                            ChevronRightIcon { class: "h-5 w-5".to_string() }
                        }
                    }
                    div { class: "flex space-x-2",
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| active_day.set(today_real),
                            "Today"
                        }
                        // MAPPS-280: Day / Week / Month view toggle.
                        // Day = per-technician swimlane (existing rich
                        // render). Week / Month re-use the calendar
                        // grids over the dispatch appointments so a
                        // dispatcher can plan a week without leaving
                        // the surface; per-technician week swimlanes
                        // are tracked as a follow-up under this ticket.
                        div { class: "flex border border-line rounded-md overflow-hidden",
                            ViewToggleButton {
                                label: "Day",
                                active: view() == CalendarView::Day,
                                onclick: move |_| view.set(CalendarView::Day),
                            }
                            ViewToggleButton {
                                label: "Week",
                                active: view() == CalendarView::Week,
                                onclick: move |_| view.set(CalendarView::Week),
                            }
                            ViewToggleButton {
                                label: "Month",
                                active: view() == CalendarView::Month,
                                onclick: move |_| view.set(CalendarView::Month),
                            }
                        }
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
                        div { class: "py-12 text-center text-sm text-muted", "Loading dispatch board..." }
                    } else if let Some(d) = dispatch.as_ref() {
                        match view() {
                            CalendarView::Day => rsx! {
                                DispatchTimeline {
                                    day,
                                    dispatch: d.clone(),
                                    users: users.clone(),
                                    onpick: move |a| form_state.set(Some(Some(a))),
                                }
                            },
                            CalendarView::Week => rsx! {
                                WeekGrid {
                                    active_date: day,
                                    today: today_real,
                                    appointments: d.appointments.clone(),
                                    onpick: move |a| form_state.set(Some(Some(a))),
                                    oncreate: move |d: NaiveDate| {
                                        active_day.set(d);
                                        form_state.set(Some(None));
                                    },
                                }
                            },
                            CalendarView::Month => rsx! {
                                MonthGrid {
                                    active_date: day,
                                    today: today_real,
                                    appointments: d.appointments.clone(),
                                    onpick: move |a| form_state.set(Some(Some(a))),
                                    oncreate: move |d: NaiveDate| {
                                        active_day.set(d);
                                        form_state.set(Some(None));
                                    },
                                }
                            },
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
            div { class: "py-12 text-center text-sm text-muted",
                "No technicians scheduled for this day."
            }
        };
    }

    rsx! {
        div { class: "overflow-x-auto",
            div { class: "min-w-[800px]",
                // Hour header.
                div { class: "grid border-b border-line",
                    style: "grid-template-columns: 200px repeat({GRID_END_HOUR - GRID_START_HOUR}, 1fr);",
                    div { class: "p-2 bg-surface-2 font-medium text-sm text-muted", "Technician" }
                    for hour in GRID_START_HOUR..GRID_END_HOUR {
                        {
                            let label = hour_label(hour);
                            rsx! {
                                div { class: "p-2 bg-surface-2 text-center text-xs text-muted border-l border-line",
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
        div { class: "grid border-b border-line min-h-16",
            style: "grid-template-columns: 200px repeat({cols}, 1fr);",
            // Technician name + status.
            div { class: "p-2 flex items-center",
                div { class: "flex items-center",
                    div { class: "w-8 h-8 rounded-full bg-accent-100 flex items-center justify-center mr-2",
                        span { class: "text-sm font-medium text-accent",
                            {props.name.chars().next().unwrap_or('?').to_string()}
                        }
                    }
                    div {
                        span { class: "font-medium text-sm text-content", "{props.name}" }
                        if off_today {
                            div { class: "text-xs text-amber-600 dark:text-amber-400", "Off: {off_kind}" }
                        }
                    }
                }
            }

            // Timeline area spanning the hour columns.
            div { class: "relative border-l border-line",
                style: "grid-column: 2 / -1; min-height: 4rem;",
                // Hour divider lines.
                div { class: "absolute inset-0 grid",
                    style: "grid-template-columns: repeat({cols}, 1fr);",
                    for _ in 0..cols {
                        div { class: "border-l border-line first:border-l-0" }
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
                        let past = past_class(appt);
                        let appt_clone = appt.clone();
                        let label = appt.title.clone();
                        let time = format!("{} - {}", time_label(appt.start_time), time_label(appt.end_time));
                        rsx! {
                            button {
                                r#type: "button",
                                class: "absolute top-1 bottom-1 rounded-md px-2 py-1 text-xs text-white shadow-sm overflow-hidden text-left hover:opacity-90 {color} {past}",
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

// ============================================================================
// Scheduling templates management page
// ============================================================================

/// Client `maxlength` caps for the template text fields (MAPPS-253), matching
/// the server's `validator` length bounds.
const TEMPLATE_NAME_MAX: i64 = 100;
const TEMPLATE_TITLE_MAX: i64 = 255;

/// Scheduling templates management page (MAPPS-253). Lists the tenant's
/// dispatch and calendar templates and lets a user create, edit, and delete
/// as many as they want via `GET|POST|PUT|DELETE /api/v1/scheduling-templates`.
/// Reachable from the sidebar and from the appointment form's picker.
#[component]
pub fn SchedulingTemplatesPage() -> Element {
    // Modal state: None = closed, Some(None) = creating, Some(Some(t)) =
    // editing that template (mirrors the appointment form's `form_state`).
    let mut form_state = use_signal(|| None::<Option<SchedulingTemplateResponse>>);

    let mut templates_resource = use_templates_resource(None);
    let snapshot = templates_resource.read_unchecked();
    let is_loading = snapshot.is_none();
    let templates: Vec<SchedulingTemplateResponse> = snapshot.clone().unwrap_or_default();

    rsx! {
        AppLayout { title: "Scheduling Templates",
            PageHeader {
                title: "Scheduling Templates",
                subtitle: "Reusable appointment shapes. Pick one on the appointment form to pre-fill the type, duration, title, and location, then just choose a start time. Dispatch templates are for on-site work; calendar templates are for client calls and status updates.",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| form_state.set(Some(None)),
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Template"
                    }
                },
            }

            Card { padding: false,
                if is_loading {
                    div { class: "py-12 text-center text-sm text-muted", "Loading templates..." }
                } else if templates.is_empty() {
                    div { class: "p-6",
                        EmptyState {
                            icon: rsx! { SwatchIcon { size: IconSize::Large } },
                            title: "No templates yet".to_string(),
                            description: "Create a dispatch or calendar template to speed up scheduling. Templates pre-fill an appointment so you only pick a start time.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    onclick: move |_| form_state.set(Some(None)),
                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                    "New Template"
                                }
                            },
                        }
                    }
                } else {
                    ul { class: "divide-y divide-line",
                        for t in templates.iter() {
                            {
                                let row = t.clone();
                                let edit_row = t.clone();
                                let kind_label = template_kind_label(&t.kind);
                                let type_label = t.appointment_type.clone();
                                let duration = humanize_minutes(t.duration_minutes);
                                let buffer = travel_buffer_help(t);
                                let name = t.name.clone();
                                rsx! {
                                    li { key: "{row.id}", class: "flex items-center justify-between gap-4 p-4",
                                        div { class: "min-w-0",
                                            div { class: "flex items-center gap-2",
                                                span { class: "font-medium text-content", "{name}" }
                                                span { class: "text-xs rounded-full bg-surface-2 text-muted px-2 py-0.5", "{kind_label}" }
                                            }
                                            p { class: "text-sm text-muted",
                                                "{type_label} - {duration}"
                                            }
                                            if let Some(b) = buffer {
                                                p { class: "text-xs text-muted", "{b}" }
                                            }
                                        }
                                        div { class: "flex items-center gap-2 shrink-0",
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                onclick: move |_| form_state.set(Some(Some(edit_row.clone()))),
                                                PencilIcon { size: IconSize::Small, class: "mr-1".to_string() }
                                                "Edit"
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

        if let Some(editing) = form_state.read().clone() {
            TemplateFormModal {
                existing: editing,
                onclose: move |_| form_state.set(None),
                onsaved: move |_| {
                    form_state.set(None);
                    templates_resource.restart();
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TemplateFormModalProps {
    /// `None` => create a new template; `Some(t)` => edit it.
    existing: Option<SchedulingTemplateResponse>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn TemplateFormModal(props: TemplateFormModalProps) -> Element {
    let existing = props.existing.clone();
    let is_edit = existing.is_some();
    let modal_title = if is_edit {
        "Edit Template"
    } else {
        "New Template"
    };

    let init_name = existing
        .as_ref()
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let init_kind = existing
        .as_ref()
        .map(|t| t.kind.clone())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| "dispatch".to_string());
    let init_type = existing
        .as_ref()
        .map(|t| t.appointment_type.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "meeting".to_string());
    let init_duration = existing
        .as_ref()
        .map(|t| t.duration_minutes.max(0))
        .unwrap_or(60);
    let init_before = existing
        .as_ref()
        .map(|t| t.travel_before_minutes.max(0))
        .unwrap_or(0);
    let init_after = existing
        .as_ref()
        .map(|t| t.travel_after_minutes.max(0))
        .unwrap_or(0);
    let init_title = existing
        .as_ref()
        .and_then(|t| t.default_title.clone())
        .unwrap_or_default();
    let init_location = existing
        .as_ref()
        .and_then(|t| t.default_location.clone())
        .unwrap_or_default();
    let init_notes = existing
        .as_ref()
        .and_then(|t| t.notes.clone())
        .unwrap_or_default();

    let mut name = use_signal(|| init_name);
    let mut kind = use_signal(|| init_kind);
    let mut appointment_type = use_signal(|| init_type);
    let mut duration_value = use_signal(|| init_duration.to_string());
    let mut before_value = use_signal(|| init_before.to_string());
    let mut after_value = use_signal(|| init_after.to_string());
    let mut default_title = use_signal(|| init_title);
    let mut default_location = use_signal(|| init_location);
    let mut notes = use_signal(|| init_notes);
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut confirming_delete = use_signal(|| false);

    let onsaved = props.onsaved;
    let onclose = props.onclose;
    let edit_id = existing.as_ref().map(|t| t.id);
    let is_dispatch = kind.read().as_str() == "dispatch";

    let handle_save = move |_| {
        if saving() || deleting() {
            return;
        }
        let name_val = name.read().trim().to_string();
        if name_val.is_empty() {
            error.set("Name is required.".to_string());
            return;
        }
        let Some(duration) = parse_positive_i32(&duration_value.read()) else {
            error.set("Duration must be a whole number of minutes greater than 0.".to_string());
            return;
        };
        // Buffers are dispatch-only: a calendar template stores zeroes. A
        // blank buffer reads as 0.
        let (before, after) = if is_dispatch {
            let Some(b) = parse_nonneg_i32(&before_value.read()) else {
                error.set("Travel before must be 0 or more minutes.".to_string());
                return;
            };
            let Some(a) = parse_nonneg_i32(&after_value.read()) else {
                error.set("Travel after must be 0 or more minutes.".to_string());
                return;
            };
            (b, a)
        } else {
            (0, 0)
        };

        let kind_val = kind.read().clone();
        let type_val = appointment_type.read().clone();
        let title = optional(&default_title.read());
        let location = optional(&default_location.read());
        let notes_val = optional(&notes.read());

        saving.set(true);
        error.set(String::new());

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result: Result<(), crate::hooks::fetch::api::ApiError> = match edit_id {
                    None => {
                        let body = CreateSchedulingTemplateRequest {
                            name: name_val,
                            kind: kind_val,
                            appointment_type: type_val,
                            duration_minutes: duration,
                            travel_before_minutes: before,
                            travel_after_minutes: after,
                            default_title: title,
                            default_location: location,
                            default_ticket_id: None,
                            notes: notes_val,
                        };
                        crate::hooks::fetch::api::post_authed_typed::<SchedulingTemplateResponse, _>(
                            "/scheduling-templates",
                            &body,
                        )
                        .await
                        .map(|_| ())
                    }
                    Some(id) => {
                        // `default_ticket_id` is intentionally omitted: this
                        // form has no ticket picker, and a missing field is
                        // COALESCE'd to the stored value server-side, so a
                        // ticket set via the API is preserved across edits.
                        let body = UpdateSchedulingTemplateRequest {
                            name: Some(name_val),
                            kind: Some(kind_val),
                            appointment_type: Some(type_val),
                            duration_minutes: Some(duration),
                            travel_before_minutes: Some(before),
                            travel_after_minutes: Some(after),
                            default_title: title,
                            default_location: location,
                            default_ticket_id: None,
                            notes: notes_val,
                        };
                        let path = format!("/scheduling-templates/{id}");
                        crate::hooks::fetch::api::put_authed_typed::<SchedulingTemplateResponse, _>(
                            &path, &body,
                        )
                        .await
                        .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(e) => error.set(format!("Could not save template: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = (
                    edit_id, name_val, kind_val, type_val, duration, before, after, title,
                    location, notes_val,
                );
            }
            saving.set(false);
        });
    };

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
                let path = format!("/scheduling-templates/{id}");
                match crate::hooks::fetch::api::delete_authed_typed(&path).await {
                    Ok(()) => onsaved.call(()),
                    Err(e) => error.set(format!("Could not delete template: {}", e.user_message())),
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
        if is_edit {
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
        Button {
            variant: ButtonVariant::Primary,
            loading: *saving.read(),
            onclick: handle_save,
            if is_edit { "Save Changes" } else { "Create Template" }
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
                if !error.read().is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{error.read()}"
                    }
                }
                Input {
                    name: "tmpl_name",
                    label: "Name",
                    placeholder: "e.g. On-site visit",
                    required: true,
                    maxlength: TEMPLATE_NAME_MAX,
                    value: name.read().clone(),
                    oninput: move |e: FormEvent| name.set(e.value()),
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    Select {
                        name: "tmpl_kind",
                        label: "Kind",
                        options: template_kind_options(),
                        help: "Dispatch templates are for on-site work (with optional travel buffers); calendar templates are for client calls and status updates."
                            .to_string(),
                        value: kind.read().clone(),
                        onchange: move |e: FormEvent| kind.set(e.value()),
                    }
                    Select {
                        name: "tmpl_type",
                        label: "Appointment type",
                        options: appointment_type_options(),
                        value: appointment_type.read().clone(),
                        onchange: move |e: FormEvent| appointment_type.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                    Input {
                        name: "tmpl_duration",
                        label: "Duration (minutes)",
                        r#type: "number".to_string(),
                        min: "1".to_string(),
                        required: true,
                        value: duration_value.read().clone(),
                        oninput: move |e: FormEvent| duration_value.set(e.value()),
                    }
                    if is_dispatch {
                        Input {
                            name: "tmpl_before",
                            label: "Travel before (min)",
                            r#type: "number".to_string(),
                            min: "0".to_string(),
                            value: before_value.read().clone(),
                            oninput: move |e: FormEvent| before_value.set(e.value()),
                        }
                        Input {
                            name: "tmpl_after",
                            label: "Travel after (min)",
                            r#type: "number".to_string(),
                            min: "0".to_string(),
                            value: after_value.read().clone(),
                            oninput: move |e: FormEvent| after_value.set(e.value()),
                        }
                    }
                }
                Input {
                    name: "tmpl_default_title",
                    label: "Default title",
                    placeholder: "Pre-fills the appointment title (optional)",
                    maxlength: TEMPLATE_TITLE_MAX,
                    value: default_title.read().clone(),
                    oninput: move |e: FormEvent| default_title.set(e.value()),
                }
                Input {
                    name: "tmpl_default_location",
                    label: "Default location",
                    placeholder: "e.g. Client site / Remote (optional)",
                    value: default_location.read().clone(),
                    oninput: move |e: FormEvent| default_location.set(e.value()),
                }
                Textarea {
                    name: "tmpl_notes",
                    label: "Notes",
                    rows: 3,
                    help: "Internal notes about this template (optional).".to_string(),
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
        crate::components::ConfirmDialog {
            open: confirming_delete(),
            title: "Delete template".to_string(),
            message: "Delete this template? This cannot be undone. Existing appointments are not affected.".to_string(),
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

/// Parse a positive whole-number minutes string from a `type="number"` input.
/// `None` for empty / non-numeric / non-positive values so the form surfaces
/// a validation error (MAPPS-253).
fn parse_positive_i32(s: &str) -> Option<i32> {
    let n: i32 = s.trim().parse().ok()?;
    (n > 0).then_some(n)
}

/// Parse a non-negative whole-number minutes string; a blank value reads as
/// `0` (an omitted travel buffer). `None` for non-numeric / negative input.
fn parse_nonneg_i32(s: &str) -> Option<i32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    let n: i32 = trimmed.parse().ok()?;
    (n >= 0).then_some(n)
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

#[cfg(test)]
mod duration_tests {
    use super::{add_minutes_to_local, parse_local_datetime_to_utc};

    // Outside a Dioxus runtime `user_timezone()` falls back to UTC, so the
    // local parse/format round-trips through UTC deterministically here.

    #[test]
    fn adds_preset_duration_to_start() {
        // Start + 60 minutes => End one hour later.
        assert_eq!(
            add_minutes_to_local("2026-06-18T09:00", 60).as_deref(),
            Some("2026-06-18T10:00")
        );
        // 90 minutes crosses the hour boundary.
        assert_eq!(
            add_minutes_to_local("2026-06-18T09:00", 90).as_deref(),
            Some("2026-06-18T10:30")
        );
        // Crossing midnight rolls the date forward.
        assert_eq!(
            add_minutes_to_local("2026-06-18T23:30", 60).as_deref(),
            Some("2026-06-19T00:30")
        );
    }

    #[test]
    fn shifting_start_preserves_duration() {
        // The derived End is always Start + duration, so moving Start by 30
        // minutes shifts End by the same delta with the duration intact.
        let early = add_minutes_to_local("2026-06-18T09:00", 60).unwrap();
        let late = add_minutes_to_local("2026-06-18T09:30", 60).unwrap();
        let early_end = parse_local_datetime_to_utc(&early).unwrap();
        let late_end = parse_local_datetime_to_utc(&late).unwrap();
        assert_eq!((late_end - early_end).num_minutes(), 30);
    }

    #[test]
    fn back_computes_custom_duration() {
        // The custom End path back-computes minutes from (end - start).
        let start = parse_local_datetime_to_utc("2026-06-18T09:00").unwrap();
        let end = parse_local_datetime_to_utc("2026-06-18T11:15").unwrap();
        assert_eq!((end - start).num_minutes(), 135);
    }

    #[test]
    fn rejects_malformed_start() {
        assert_eq!(add_minutes_to_local("", 60), None);
        assert_eq!(add_minutes_to_local("not-a-date", 60), None);
    }
}

#[cfg(test)]
mod template_tests {
    use super::{
        humanize_minutes, parse_nonneg_i32, parse_positive_i32, template_kind_label,
        template_option_label, travel_buffer_help,
    };
    use crate::modules::calendar::SchedulingTemplateResponse;

    fn template(
        kind: &str,
        name: &str,
        duration: i32,
        before: i32,
        after: i32,
    ) -> SchedulingTemplateResponse {
        SchedulingTemplateResponse {
            id: uuid::Uuid::nil(),
            name: name.to_string(),
            kind: kind.to_string(),
            appointment_type: "meeting".to_string(),
            duration_minutes: duration,
            travel_before_minutes: before,
            travel_after_minutes: after,
            default_title: None,
            default_location: None,
            default_ticket_id: None,
            notes: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn humanizes_durations() {
        assert_eq!(humanize_minutes(45), "45m");
        assert_eq!(humanize_minutes(60), "1h");
        assert_eq!(humanize_minutes(90), "1h 30m");
        assert_eq!(humanize_minutes(120), "2h");
        // Defensive: a negative value clamps to zero rather than panicking.
        assert_eq!(humanize_minutes(-5), "0m");
    }

    #[test]
    fn labels_picker_option_by_kind_and_length() {
        let t = template("dispatch", "On-site visit", 120, 30, 30);
        assert_eq!(template_option_label(&t), "Dispatch: On-site visit (2h)");
        let c = template("calendar", "Discovery call", 60, 0, 0);
        assert_eq!(template_option_label(&c), "Calendar: Discovery call (1h)");
    }

    #[test]
    fn kind_label_falls_back_for_unknown() {
        assert_eq!(template_kind_label("dispatch"), "Dispatch");
        assert_eq!(template_kind_label("calendar"), "Calendar");
        assert_eq!(template_kind_label("mystery"), "Template");
    }

    #[test]
    fn travel_buffer_help_only_for_dispatch_with_buffers() {
        // Calendar templates never surface buffers.
        assert_eq!(
            travel_buffer_help(&template("calendar", "Call", 60, 30, 30)),
            None
        );
        // Dispatch with zero buffers => no note.
        assert_eq!(
            travel_buffer_help(&template("dispatch", "Visit", 60, 0, 0)),
            None
        );
        // Both buffers set.
        assert_eq!(
            travel_buffer_help(&template("dispatch", "Visit", 60, 30, 30)).as_deref(),
            Some("Includes 30m travel before and 30m after.")
        );
        // Only before.
        assert_eq!(
            travel_buffer_help(&template("dispatch", "Visit", 60, 15, 0)).as_deref(),
            Some("Includes 15m travel before.")
        );
        // Only after.
        assert_eq!(
            travel_buffer_help(&template("dispatch", "Visit", 60, 0, 45)).as_deref(),
            Some("Includes 45m travel after.")
        );
    }

    #[test]
    fn parses_positive_duration() {
        assert_eq!(parse_positive_i32("60"), Some(60));
        assert_eq!(parse_positive_i32("  90 "), Some(90));
        assert_eq!(parse_positive_i32("0"), None);
        assert_eq!(parse_positive_i32("-5"), None);
        assert_eq!(parse_positive_i32(""), None);
        assert_eq!(parse_positive_i32("abc"), None);
    }

    #[test]
    fn parses_nonneg_buffer_with_blank_as_zero() {
        assert_eq!(parse_nonneg_i32("0"), Some(0));
        assert_eq!(parse_nonneg_i32("30"), Some(30));
        // Blank reads as an omitted (zero) buffer.
        assert_eq!(parse_nonneg_i32(""), Some(0));
        assert_eq!(parse_nonneg_i32("   "), Some(0));
        assert_eq!(parse_nonneg_i32("-1"), None);
        assert_eq!(parse_nonneg_i32("x"), None);
    }
}

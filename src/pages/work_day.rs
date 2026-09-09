//! MAPPS-730: the work day, on the Time page. Server side is PMS-950: a day
//! is a chain of `work` and `break` segments (`work_day_segments`), clocking
//! in opens a `work` segment, a break closes it and opens a `break` one in
//! one transaction, clocking out closes whichever is open, and
//! `GET /api/v1/workday` reads the day back with the time entries logged
//! that day grouped by what they are attached to, plus the signed gap
//! between clocked and logged minutes. The server reports the gap and never
//! resolves it; this strip shows it and offers "Log time" for the rest.
//!
//! Every route sits behind the time-tracking AND timesheets module gates and
//! answers 404 with either off, exactly as an unmounted route does. That 404
//! is the signal here: the strip renders nothing rather than an error. The
//! two break routes 404 on their own when the tenant's `track_breaks`
//! setting is off, and the day view says so in `track_breaks`, which is what
//! hides the break buttons.
//!
//! MAPPS-753 re-weighted the card around its actual job. Three things about
//! it are decisions rather than styling, and each is easy to undo by accident.
//!
//! **The elapsed figure is anchored on a server value, not on the client
//! clock.** `live_minutes` is the server's own number plus the wall time since
//! this client received it, so a client whose clock is wrong by ten minutes
//! still shows the right elapsed - only the RATE of the local clock is
//! trusted, never its absolute reading. Computing `now - started_at` instead
//! would be simpler and would inherit every skew. This is billing-adjacent
//! data; it must not be invented here.
//!
//! **The state is readable without colour.** `ButtonVariant::Primary` is
//! `bg-accent` and the accent is chosen by the user from fourteen options, one
//! of which is `red`; `ButtonVariant::Danger` is a hardcoded `red-600`. On a
//! red-accent tenant `Clock in` and `Clock out` are nearly the same colour, so
//! the difference between the two states is carried by structure (a timer
//! present or absent, border weight) and by words, with colour as
//! reinforcement only.
//!
//! MAPPS-754 added the correction. PMS-1145 gave the server
//! `PUT`/`DELETE /workday/segments/{id}`; until this, nothing in this app
//! could reach them, so a mis-tapped clock-in was visible on this card and
//! not fixable from it. Two things about how it is gated.
//!
//! **Whether to draw the control is decided here, and whether to allow it is
//! decided by the server.** `timesheets/segment_editing` is fetched once and
//! evaluated against the segment's owner and the caller's role, so a control
//! is offered only where the request would succeed - the `can_edit` idea from
//! PMS-974 without a server field to carry it. That means the rule exists in
//! two places, which is a real cost and is accepted deliberately: the server
//! still enforces, this only decides what to render, and a disagreement
//! surfaces as the server's own refusal rather than as a wrong write.
//!
//! **Reopening is its own action.** On the wire `ended_at` is a double
//! option: absent leaves the end alone, an explicit `null` reopens the
//! segment. A bare empty field meaning "undo the clock-out" is not something
//! anyone discovers, so "Reopen" is a button that sends the null.
//!
//! **A refetch does not blank the card.** The resource restarts on a timer,
//! and a restarted resource reads `None` again, so rendering nothing while
//! pending made the whole strip disappear on a cycle. The last loaded day is
//! held and re-rendered while the next one is in flight; only the very first
//! load renders nothing, which is what MAPPS-746 decided and its test pins.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    Button, ButtonSize, ButtonVariant, Card, ClockIcon, ErrorBanner, ExclamationIcon, IconSize,
};
use crate::utils::duration::fmt_duration;
use crate::Route;

/// One `work` or `break` segment as `GET /workday` returns it. `ended_at`
/// is null while the segment is open; `minutes` counts up to now then.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteSegment {
    id: uuid::Uuid,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    minutes: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct RemoteTicketLine {
    ticket_id: uuid::Uuid,
    #[serde(default)]
    ticket_number: Option<String>,
    #[serde(default)]
    ticket_title: Option<String>,
    #[serde(default)]
    minutes: i64,
    #[serde(default)]
    entry_count: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct RemoteProjectLine {
    project_id: uuid::Uuid,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    minutes: i64,
    #[serde(default)]
    entry_count: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct RemoteBucket {
    #[serde(default)]
    minutes: i64,
    #[serde(default)]
    entry_count: i64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct RemoteBreakdown {
    #[serde(default)]
    tickets: Vec<RemoteTicketLine>,
    #[serde(default)]
    projects: Vec<RemoteProjectLine>,
    #[serde(default)]
    administrative: RemoteBucket,
    #[serde(default)]
    unattached: RemoteBucket,
}

/// `WorkDayResponse`, the server's view of one person's day.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteWorkDay {
    /// MAPPS-754: whose day this is. Every segment on it belongs to this
    /// person, so the correction policy needs no per-segment owner. `Option`
    /// because an older server may not send it, and a day with no owner
    /// offers no correction rather than guessing one.
    #[serde(default)]
    user_id: Option<uuid::Uuid>,
    #[serde(default)]
    date: String,
    #[serde(default)]
    is_clocked_in: bool,
    #[serde(default)]
    on_break: bool,
    #[serde(default)]
    track_breaks: bool,
    #[serde(default)]
    segments: Vec<RemoteSegment>,
    #[serde(default)]
    clocked_minutes: i64,
    #[serde(default)]
    break_minutes: i64,
    #[serde(default)]
    logged_minutes: i64,
    #[serde(default)]
    unlogged_minutes: i64,
    #[serde(default)]
    breakdown: RemoteBreakdown,
}

/// A staff user for the admin's picker (`GET /auth/users`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct DayUser {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: String,
}

impl DayUser {
    fn display_name(&self) -> String {
        if !self.full_name.trim().is_empty() {
            return self.full_name.clone();
        }
        let joined = format!("{} {}", self.first_name, self.last_name);
        let joined = joined.trim();
        if joined.is_empty() {
            self.email.clone()
        } else {
            joined.to_string()
        }
    }
}

/// What the strip loaded: the day, or nothing because the modules are off.
#[derive(Clone, Debug, PartialEq)]
enum DayLoad {
    /// The day, and the instant this client received it. The second half is
    /// what [`live_minutes`] counts from: the server's figures are true as of
    /// that moment and nothing else about the local clock is trusted.
    Day(Box<RemoteWorkDay>, DateTime<Utc>),
    /// The 404 both module gates answer with. Not an error: the feature is
    /// off for this tenant and the strip stays out of the page.
    ModulesOff,
}

/// MAPPS-753: what the reconciliation slot says about the signed gap.
///
/// `unlogged` is the server's own signed figure: positive when time was
/// clocked and not attributed to any work item, negative when more was logged
/// than clocked. The wording drops "accounted for", which MAPPS-751 called
/// jargon and which is: it is an accounting word for a technician-facing
/// sentence, and the state it names most often is the one needing an action.
pub(crate) fn unlogged_label(unlogged: i64) -> String {
    match unlogged {
        0 => "All clocked time is logged".to_string(),
        n if n > 0 => format!("{} not yet logged", fmt_duration(n)),
        n => format!("{} logged beyond the clock", fmt_duration(-n)),
    }
}

/// A clock running longer than this is almost certainly a forgotten
/// clock-out rather than a shift.
///
/// Sixteen hours: long enough that a genuine long day or a double shift does
/// not trip it, short enough that a Monday-evening miss is flagged on Tuesday
/// morning rather than on Wednesday. Hardcoded on purpose for now - a real
/// threshold belongs to the tenant and to a server that can act on it
/// (PMS-1146), and inventing a tenant setting the server does not read would
/// be a promise this client cannot keep.
pub(crate) const STALE_AFTER_HOURS: i64 = 16;

/// The elapsed value to show, anchored on what the server said.
///
/// `base` is the server's own figure for this segment or day, true as of
/// `fetched_at`; the return is that plus the wall time since. The local clock
/// is trusted for how fast time passes and never for what time it is, so a
/// client whose clock is off by ten minutes still shows the right elapsed.
/// Clamped at `base`, because a clock that jumped backwards must not make an
/// elapsed figure shrink.
pub(crate) fn live_minutes(base: i64, fetched_at: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    let since = (now - fetched_at).num_minutes();
    base + since.max(0)
}

/// What the card is: out, on the clock, or on a break.
///
/// A break is still clocked in on the wire (`is_clocked_in` is any open
/// segment, `on_break` narrows it), and the two need different treatments, so
/// the three cases are named once here rather than re-derived at each use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClockState {
    Out,
    In,
    OnBreak,
}

impl ClockState {
    pub(crate) fn read(is_clocked_in: bool, on_break: bool) -> Self {
        match (is_clocked_in, on_break) {
            (_, true) => ClockState::OnBreak,
            (true, false) => ClockState::In,
            (false, false) => ClockState::Out,
        }
    }

    /// The words. Half of the non-colour distinction; the other half is that
    /// only the two running states carry a timer at all.
    pub(crate) fn label(self) -> &'static str {
        match self {
            ClockState::Out => "Clocked out",
            ClockState::In => "Clocked in",
            ClockState::OnBreak => "On break",
        }
    }

    /// The container treatment. Border WEIGHT carries the distinction so it
    /// survives greyscale; the tint only reinforces it.
    pub(crate) fn container_class(self) -> &'static str {
        match self {
            ClockState::Out => "border border-line bg-surface-2",
            ClockState::In => "border-2 border-accent bg-accent-50 dark:bg-surface-2",
            ClockState::OnBreak => "border-2 border-dashed border-line-strong bg-surface-2",
        }
    }

    pub(crate) fn is_running(self) -> bool {
        matches!(self, ClockState::In | ClockState::OnBreak)
    }
}

/// MAPPS-754: an `HH:MM` on a given day, in the viewer's zone, as an instant.
///
/// A `<input type="time">` yields a wall clock and nothing else, and the
/// segment's timestamps are instants, so the day and the zone are what join
/// them. The zone is the user's profile zone - the same one the segment times
/// are RENDERED in - and not the browser's, or a correction typed as "09:14"
/// would land at a different instant from the "09:14" shown beside it.
///
/// `None` for a time that does not exist in that zone (the hour a DST jump
/// skips) or is ambiguous, rather than silently picking one: the caller shows
/// the refusal instead of writing a time the person did not mean.
pub(crate) fn local_time_to_utc(
    date: NaiveDate,
    hhmm: &str,
    tz: chrono_tz::Tz,
) -> Option<DateTime<Utc>> {
    let time = chrono::NaiveTime::parse_from_str(hhmm, "%H:%M").ok()?;
    let naive = date.and_time(time);
    tz.from_local_datetime(&naive)
        .single()
        .map(|local| local.with_timezone(&Utc))
}

/// The `HH:MM` an instant reads as in the viewer's zone, for prefilling the
/// input above. Round-trips with `local_time_to_utc` for every time that
/// exists once in the zone.
pub(crate) fn utc_to_local_time(dt: DateTime<Utc>, tz: chrono_tz::Tz) -> String {
    dt.with_timezone(&tz).format("%H:%M").to_string()
}

/// MAPPS-754: the tenant's segment-correction policy, as this card needs it.
///
/// The names are the server's closed set (`SegmentEditPolicy`, PMS-1145) and
/// the default for an unset or unrecognised value is the server's too. The
/// page that WRITES this setting owns the list and its wording
/// (`settings_timesheet_editing`); this is only the read.
pub(crate) fn segment_edit_allowed(
    policy: &str,
    caller_id: uuid::Uuid,
    caller_is_admin: bool,
    caller_can_manage: bool,
    owner_id: uuid::Uuid,
) -> bool {
    match policy {
        "off" => false,
        "owner_or_manager" => caller_id == owner_id || caller_can_manage,
        // `owner_or_admin` and anything this build does not recognise, which
        // is what the server falls back to as well.
        _ => caller_id == owner_id || caller_is_admin,
    }
}

/// MAPPS-753: the line that names the day when it is not the reader's today.
///
/// `GET /workday` with no date returns the OPEN segment's date, not today, so
/// a clock left running overnight shows yesterday's card - correct, and
/// baffling if the card does not say so. `None` when the day being shown IS
/// today, because then there is nothing to explain.
pub(crate) fn other_day_note(
    shown: &str,
    today: chrono::NaiveDate,
    running: bool,
) -> Option<String> {
    let shown_date = chrono::NaiveDate::parse_from_str(shown, "%Y-%m-%d").ok()?;
    if shown_date == today {
        return None;
    }
    let pretty = shown_date.format("%A %-d %b");
    Some(if running {
        format!("Still clocked in from {pretty}")
    } else {
        format!("Showing {pretty}")
    })
}

/// The `date` query the strip sends: nothing for the server's own today
/// (the caller's zone, never the UTC day), or the picked day.
pub(crate) fn day_query(date: Option<NaiveDate>, user_id: Option<uuid::Uuid>) -> String {
    let mut parts = Vec::new();
    if let Some(d) = date {
        parts.push(format!("date={}", d.format("%Y-%m-%d")));
    }
    if let Some(u) = user_id {
        parts.push(format!("user_id={u}"));
    }
    if parts.is_empty() {
        "/workday".to_string()
    } else {
        format!("/workday?{}", parts.join("&"))
    }
}

/// MAPPS-746: what the Time page says in place of the clock when the
/// server answers 404 for the day, which is the timesheets module being
/// off for the tenant (PMS-943: a `personal` tenant starts with it off, an
/// `org` with it on; migration 120 backfilled the same rule). Named, so a
/// reader can ask for the right thing; there is no module page in this
/// client yet, so it does not promise a link it cannot make.
pub(crate) const MODULES_OFF_NOTICE: &str = "Clocking in and out needs the Timesheets module, which is off for this organisation. An administrator can turn it on; time entries are unaffected.";

/// The strip on the Time page: clock in and out, breaks when the tenant
/// tracks them, the day's segments, the per-item breakdown and the gap.
#[component]
pub fn WorkDayStrip() -> Element {
    let auth = crate::hooks::use_auth();
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .is_some_and(|u| u.role.can_manage_users());
    let mut picked_date = use_signal(|| None::<NaiveDate>);
    let mut picked_user = use_signal(|| None::<uuid::Uuid>);
    let mut busy = use_signal(|| false);
    let mut action_error = use_signal(String::new);
    let mut show_date_picker = use_signal(|| false);
    // Two ticks, on purpose. `refetch_tick` restarts the resource, which is
    // what picks up time logged elsewhere and re-anchors the elapsed figures
    // on a fresh server value. `clock_tick` only re-renders, so the displayed
    // minute rolls over promptly without a request behind it: before
    // MAPPS-753 the only tick was the refetch, so the timer cost a full
    // `GET /workday` per minute and still sat stale for up to sixty seconds.
    let mut refetch_tick = use_signal(|| 0u32);
    let mut clock_tick = use_signal(|| 0u32);
    // The last day this client actually received. Rendered while the next
    // load is in flight so a refetch does not blank the card; see the module
    // docs.
    let mut last_day = use_signal(|| None::<(Box<RemoteWorkDay>, DateTime<Utc>)>);
    // MAPPS-754: which segment's correction form is open, and what is typed
    // in it. One at a time: two open forms on a strip this size is a way to
    // save the wrong one.
    let mut correcting = use_signal(|| None::<uuid::Uuid>);
    // The segment a Remove is asking about. Removing a clock entry is a
    // destructive action on attendance data, so it confirms first
    // (docs/destructive-actions.md); the DELETE fires from the dialog.
    let mut removing = use_signal(|| None::<uuid::Uuid>);
    let mut edit_start = use_signal(String::new);
    let mut edit_end = use_signal(String::new);
    let can_mutate = crate::hooks::use_can_mutate();

    let date_for_resource = picked_date();
    let user_for_resource = picked_user();
    let mut day_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        let _tick = refetch_tick();
        let path = day_query(date_for_resource, user_for_resource);
        #[cfg(feature = "app")]
        {
            match crate::hooks::fetch::api::get_authed_typed::<RemoteWorkDay>(&path).await {
                // Stamped here rather than at render: this is the instant the
                // server's figures were true, and every elapsed value on the
                // card counts from it.
                Ok(day) => Some(DayLoad::Day(Box::new(day), Utc::now())),
                Err(e) if e.status_code() == Some(404) => Some(DayLoad::ModulesOff),
                Err(e) => {
                    tracing::warn!("work day load failed: {e}");
                    None
                }
            }
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = path;
            None::<DayLoad>
        }
    });
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep_ms(60_000).await;
            refetch_tick.with_mut(|t| *t = t.wrapping_add(1));
        }
    });
    // Ten seconds, not one: the card shows whole minutes (`fmt_duration`
    // follows the user's duration preference and neither shape has seconds),
    // so this only has to be short enough that the minute turns over without
    // a visible lag. A one-second tick would re-render sixty times for one
    // changed digit.
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep_ms(10_000).await;
            clock_tick.with_mut(|t| *t = t.wrapping_add(1));
        }
    });
    // MAPPS-754: the tenant's correction policy, fetched once. It decides
    // whether to DRAW the controls; the server decides whether to allow the
    // request. An unreadable settings list reads as the server's default,
    // which is what an older server without the setting also gives.
    let policy_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        #[cfg(feature = "app")]
        {
            crate::hooks::fetch::api::get_all_authed::<
                crate::pages::settings_timesheet_editing::TenantSetting,
            >("/settings")
            .await
            .map(|rows| crate::pages::settings_timesheet_editing::policy_in(&rows).to_string())
            .unwrap_or_else(|e| {
                tracing::warn!("segment editing policy load failed: {e}");
                crate::pages::settings_timesheet_editing::DEFAULT_POLICY.to_string()
            })
        }
        #[cfg(not(feature = "app"))]
        {
            crate::pages::settings_timesheet_editing::DEFAULT_POLICY.to_string()
        }
    });

    let users_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !is_admin {
            return Vec::<DayUser>::new();
        }
        crate::hooks::fetch::api::get_all_authed::<DayUser>("/auth/users")
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("work day user list load failed: {e}");
                Vec::new()
            })
    });

    let snap = day_resource.read_unchecked().clone();
    // MAPPS-753: hold the day the last load produced. A resource that has
    // been restarted reads `None` again, so without this the card vanished
    // and the page jumped every time the refetch tick fired.
    if let Some(Some(DayLoad::Day(fresh, at))) = &snap {
        let incoming = Some((fresh.clone(), *at));
        if *last_day.peek() != incoming {
            last_day.set(incoming);
        }
    }
    let (day, fetched_at) = match snap {
        // Loading. A placeholder here would move the page twice on every load
        // for a strip that is usually small, so the FIRST load stays silent
        // (MAPPS-746); a later one re-renders what it already had.
        None => match last_day() {
            Some((prev, at)) => (*prev, at),
            None => return rsx! {},
        },
        // MAPPS-746: the two states this used to hide. Both hid the clock
        // with nothing on the page to say why, and the nav's Timesheets
        // entries do not follow the module flag, so the reader had every
        // reason to expect a clock and no way to learn where it went.
        Some(None) => {
            return rsx! {
                div { class: "mb-6 flex items-center justify-between gap-3 rounded-lg border border-line bg-surface px-4 py-3",
                    p { class: "text-sm text-red-600 dark:text-red-300", "Could not load today's clock." }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| day_resource.restart(),
                        "Retry"
                    }
                }
            }
        }
        Some(Some(DayLoad::ModulesOff)) => {
            return rsx! {
                p { class: "mb-6 text-sm text-subtle", "{MODULES_OFF_NOTICE}" }
            }
        }
        Some(Some(DayLoad::Day(day, at))) => (*day, at),
    };

    let mut post = move |path: &'static str, body: serde_json::Value, verb: &'static str| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed_typed::<serde_json::Value, _>(
                    path, &body,
                )
                .await
                {
                    Ok(_) => day_resource.restart(),
                    Err(e) => action_error.set(format!("Could not {verb}: {}", e.user_message())),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (path, &body, verb);
            }
            busy.set(false);
        });
    };
    // MAPPS-754: the correction mutators. Same non-optimistic shape as
    // `post` above and for a stronger reason - this is attendance data, so a
    // failed correction must not leave a corrected time on the screen. The
    // server's own refusal is surfaced verbatim: each of the 400/403/409
    // messages names what is wrong, and re-wording them here would drift from
    // what the server actually enforces.
    let mut put_segment = move |id: uuid::Uuid, body: serde_json::Value| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/workday/segments/{id}");
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => day_resource.restart(),
                    Err(e) => action_error.set(e),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = (id, &body);
            }
            busy.set(false);
        });
    };
    let mut delete_segment = move |id: uuid::Uuid| {
        if *busy.read() {
            return;
        }
        busy.set(true);
        action_error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let path = format!("/workday/segments/{id}");
                match crate::hooks::fetch::api::delete_authed(&path).await {
                    Ok(()) => day_resource.restart(),
                    Err(e) => action_error.set(e),
                }
            }
            #[cfg(not(feature = "app"))]
            {
                let _ = id;
            }
            busy.set(false);
        });
    };
    let clock_in_body = match picked_date() {
        Some(d) => serde_json::json!({ "date": d.format("%Y-%m-%d").to_string() }),
        None => serde_json::json!({}),
    };
    let viewing_someone_else = picked_user().is_some();
    let state = ClockState::read(day.is_clocked_in, day.on_break);

    // Re-render on the clock tick, and read `now` once for every figure below
    // so they cannot disagree with each other by a tick.
    let _ = clock_tick();
    let now = Utc::now();
    let open_segment = day.segments.iter().find(|s| s.ended_at.is_none());
    // Only a running clock accrues. Nothing here counts up for a day that is
    // closed, or for another user's day being read.
    let accruing = state.is_running() && !viewing_someone_else;
    let session_minutes = open_segment.map(|s| {
        if accruing {
            live_minutes(s.minutes, fetched_at, now)
        } else {
            s.minutes
        }
    });
    let clocked_live = if accruing && state == ClockState::In {
        live_minutes(day.clocked_minutes, fetched_at, now)
    } else {
        day.clocked_minutes
    };
    let break_live = if accruing && state == ClockState::OnBreak {
        live_minutes(day.break_minutes, fetched_at, now)
    } else {
        day.break_minutes
    };
    // The gap follows the clocked figure, or it contradicts the number
    // printed beside it the moment the clock moves.
    let unlogged_live = clocked_live - day.logged_minutes;
    let started_label = open_segment
        .and_then(|s| s.started_at)
        .map(clock_time)
        .unwrap_or_default();
    let stale = accruing && session_minutes.is_some_and(|m| m >= STALE_AFTER_HOURS * 60);
    let day_note = other_day_note(
        &day.date,
        crate::utils::datetime::user_today(),
        state.is_running(),
    );

    let date_value = picked_date()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| day.date.clone());
    let date_pretty = NaiveDate::parse_from_str(&date_value, "%Y-%m-%d")
        .map(|d| d.format("%A, %-d %b").to_string())
        .unwrap_or_else(|_| date_value.clone());
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let viewing_name = picked_user()
        .and_then(|id| users.iter().find(|u| u.id == id).map(|u| u.display_name()))
        .unwrap_or_default();
    let disabled_title =
        (!can_mutate).then(|| "Can't change the day while the server is unreachable".to_string());

    // MAPPS-754: may this caller correct THIS day's segments? Every segment
    // on a day belongs to one person, so the answer is per day and not per
    // segment. A day whose owner the server did not name offers nothing
    // rather than guessing that it is the caller's own.
    let policy = policy_resource
        .read_unchecked()
        .clone()
        .unwrap_or_else(|| crate::pages::settings_timesheet_editing::DEFAULT_POLICY.to_string());
    let caller = auth.read().user.as_ref().map(|u| (u.id, u.role));
    let can_correct = match (caller, day.user_id) {
        (Some((caller_id, role)), Some(owner)) => segment_edit_allowed(
            &policy,
            caller_id,
            role.is_admin(),
            role.can_manage_users(),
            owner,
        ),
        _ => false,
    };
    let zone = crate::utils::datetime::user_timezone();
    let segment_day = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").ok();

    rsx! {
        Card { class: "mb-6",
            div { class: "space-y-4",
                // Header: the card's name, and (admin only) whose day is
                // being read. The picker is labelled and out of the action
                // row: MAPPS-751 read it as a mode selector for the clock,
                // which it never was.
                div { class: "flex flex-wrap items-center gap-3",
                    h2 { class: "text-lg font-semibold text-content", "Work day" }
                    div { class: "flex-1" }
                    if is_admin && !users.is_empty() {
                        label { class: "flex items-center gap-2 text-sm text-muted",
                            "Viewing"
                            select {
                                class: "rounded-md border border-line bg-surface px-2 py-1 text-sm text-content",
                                onchange: move |e: FormEvent| {
                                    picked_user.set(uuid::Uuid::parse_str(&e.value()).ok());
                                },
                                option { value: "", selected: picked_user().is_none(), "My day" }
                                for u in users.iter() {
                                    option {
                                        value: "{u.id}",
                                        selected: picked_user() == Some(u.id),
                                        "{u.display_name()}"
                                    }
                                }
                            }
                        }
                    }
                }

                if viewing_someone_else {
                    p { class: "text-sm text-muted",
                        "Read-only: this is {viewing_name}'s day. Clocking in and out acts on your own."
                    }
                }

                if let Some(note) = day_note.clone() {
                    p { class: "text-sm text-muted", "{note}" }
                }

                if !action_error.read().is_empty() {
                    ErrorBanner { "{action_error.read()}" }
                }

                // The card's actual job. Announced on state change only: the
                // elapsed value below updates silently, because a live region
                // that re-read the timer every minute would be unusable.
                div {
                    class: "rounded-lg p-4 {state.container_class()}",
                    div {
                        class: "sr-only",
                        role: "status",
                        "aria-live": "polite",
                        "{state.label()}"
                    }
                    if state.is_running() {
                        div { class: "flex flex-wrap items-center justify-between gap-4",
                            div { class: "flex items-start gap-3",
                                ClockIcon {
                                    size: IconSize::Medium,
                                    class: "mt-1 text-content".to_string(),
                                }
                                div {
                                    p { class: "text-sm font-medium text-content", "{state.label()}" }
                                    // The elapsed figure. Deliberately not in
                                    // a live region; see the div above.
                                    p {
                                        class: "text-3xl font-semibold tabular-nums text-content",
                                        "aria-live": "off",
                                        if let Some(m) = session_minutes {
                                            "{fmt_duration(m)}"
                                        }
                                    }
                                    if !started_label.is_empty() {
                                        p { class: "text-sm text-muted", "since {started_label}" }
                                    }
                                }
                            }
                            if !viewing_someone_else {
                                div { class: "flex flex-wrap gap-2",
                                    if day.track_breaks {
                                        if day.on_break {
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                loading: *busy.read(),
                                                disabled: !can_mutate,
                                                title: disabled_title.clone(),
                                                onclick: move |_| post("/workday/break/end", serde_json::json!({}), "end the break"),
                                                "End break"
                                            }
                                        } else {
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                loading: *busy.read(),
                                                disabled: !can_mutate,
                                                title: disabled_title.clone(),
                                                onclick: move |_| post("/workday/break/start", serde_json::json!({}), "start a break"),
                                                "Start break"
                                            }
                                        }
                                    }
                                    Button {
                                        variant: ButtonVariant::Danger,
                                        size: ButtonSize::Large,
                                        loading: *busy.read(),
                                        disabled: !can_mutate,
                                        title: disabled_title.clone(),
                                        onclick: move |_| post("/workday/clock-out", serde_json::json!({}), "clock out"),
                                        "Clock out"
                                    }
                                }
                            }
                        }
                        if stale {
                            div { class: "mt-3 flex items-start gap-2 rounded-md border border-line-strong bg-surface px-3 py-2",
                                ExclamationIcon {
                                    size: IconSize::Small,
                                    class: "mt-0.5 shrink-0 text-content".to_string(),
                                }
                                p { class: "text-sm text-content",
                                    "This clock has been running for over {STALE_AFTER_HOURS} hours. If you forgot to clock out, clocking out now records the time up to this moment."
                                }
                            }
                        }
                    } else {
                        div { class: "flex flex-wrap items-center justify-between gap-4",
                            div {
                                p { class: "text-sm font-medium text-content", "{state.label()}" }
                                p { class: "text-lg text-content", "{date_pretty}" }
                            }
                            if !viewing_someone_else {
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Large,
                                    loading: *busy.read(),
                                    disabled: !can_mutate,
                                    title: disabled_title.clone(),
                                    onclick: {
                                        let body = clock_in_body.clone();
                                        move |_| post("/workday/clock-in", body.clone(), "clock in")
                                    },
                                    "Clock in"
                                }
                            }
                        }
                        // Backdating is the rare case, so it asks rather than
                        // occupying the widest control on the card.
                        if !viewing_someone_else {
                            div { class: "mt-3",
                                if *show_date_picker.read() {
                                    label { class: "flex flex-wrap items-center gap-2 text-sm text-muted",
                                        "Clock in for"
                                        input {
                                            r#type: "date",
                                            class: "rounded-md border border-line bg-surface px-2 py-1 text-sm text-content",
                                            value: "{date_value}",
                                            oninput: move |e: FormEvent| {
                                                picked_date.set(NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d").ok());
                                            },
                                        }
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Small,
                                            onclick: move |_| {
                                                picked_date.set(None);
                                                show_date_picker.set(false);
                                            },
                                            "Back to today"
                                        }
                                    }
                                } else {
                                    Button {
                                        variant: ButtonVariant::Link,
                                        size: ButtonSize::Small,
                                        class: "px-0".to_string(),
                                        onclick: move |_| show_date_picker.set(true),
                                        "Log for another day"
                                    }
                                }
                            }
                        }
                    }
                }

                // The reconciliation: three facts, three slots. The gap is
                // the one that asks for something, so it is the one that
                // carries an action.
                div { class: "grid grid-cols-1 gap-3 sm:grid-cols-3",
                    div {
                        p { class: "text-xs uppercase tracking-wide text-subtle", "Clocked" }
                        p { class: "text-lg font-medium tabular-nums text-content", "{fmt_duration(clocked_live)}" }
                        if break_live > 0 {
                            p { class: "text-xs text-subtle", "plus {fmt_duration(break_live)} on breaks" }
                        }
                    }
                    div {
                        p { class: "text-xs uppercase tracking-wide text-subtle", "Logged to work" }
                        p { class: "text-lg font-medium tabular-nums text-content", "{fmt_duration(day.logged_minutes)}" }
                    }
                    div {
                        p { class: "text-xs uppercase tracking-wide text-subtle", "Unlogged" }
                        p {
                            class: if unlogged_live > 0 { "text-lg font-medium tabular-nums text-content" } else { "text-lg font-medium tabular-nums text-muted" },
                            "{unlogged_label(unlogged_live)}"
                        }
                        if unlogged_live > 0 && !viewing_someone_else {
                            Link {
                                to: Route::TimeEntryNew {},
                                class: "text-sm text-accent hover:opacity-90",
                                "Log this time"
                            }
                        }
                    }
                }

                if !day.segments.is_empty() {
                    ul { class: "flex flex-col gap-2 text-xs",
                        for seg in day.segments.iter() {
                            li { key: "{seg.id}",
                                class: if seg.kind == "break" { "rounded-md border border-line px-2 py-1 text-muted" } else { "rounded-md border border-line bg-surface-2 px-2 py-1" },
                                div { class: "flex flex-wrap items-center gap-2",
                                    span { class: "font-medium", if seg.kind == "break" { "Break" } else { "Work" } }
                                    " {segment_span(seg.started_at, seg.ended_at)} "
                                    span { class: "text-muted", "({fmt_duration(seg.minutes)})" }
                                    // MAPPS-754: offered only where the
                                    // tenant's policy and this caller's role
                                    // would let the request through. The
                                    // server still decides; this decides
                                    // whether to draw the control.
                                    if can_correct && correcting() != Some(seg.id) {
                                        Button {
                                            variant: ButtonVariant::Link,
                                            size: ButtonSize::Small,
                                            class: "px-0".to_string(),
                                            disabled: !can_mutate,
                                            title: disabled_title.clone(),
                                            onclick: {
                                                let id = seg.id;
                                                let started = seg.started_at;
                                                let ended = seg.ended_at;
                                                move |_| {
                                                    edit_start.set(started.map(|t| utc_to_local_time(t, zone)).unwrap_or_default());
                                                    edit_end.set(ended.map(|t| utc_to_local_time(t, zone)).unwrap_or_default());
                                                    action_error.set(String::new());
                                                    correcting.set(Some(id));
                                                }
                                            },
                                            "Correct"
                                        }
                                    }
                                }
                                if can_correct && correcting() == Some(seg.id) {
                                    div { class: "mt-2 flex flex-wrap items-end gap-2",
                                        label { class: "flex flex-col gap-1",
                                            span { class: "text-muted", "Started" }
                                            input {
                                                r#type: "time",
                                                class: "rounded-md border border-line bg-surface px-2 py-1 text-content",
                                                value: "{edit_start}",
                                                oninput: move |e: FormEvent| edit_start.set(e.value()),
                                            }
                                        }
                                        // An open segment has no end to type;
                                        // it is ended by clocking out, and
                                        // showing an empty box here would
                                        // read as one that can be filled.
                                        if seg.ended_at.is_some() {
                                            label { class: "flex flex-col gap-1",
                                                span { class: "text-muted", "Ended" }
                                                input {
                                                    r#type: "time",
                                                    class: "rounded-md border border-line bg-surface px-2 py-1 text-content",
                                                    value: "{edit_end}",
                                                    oninput: move |e: FormEvent| edit_end.set(e.value()),
                                                }
                                            }
                                        }
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            size: ButtonSize::Small,
                                            loading: *busy.read(),
                                            disabled: !can_mutate,
                                            onclick: {
                                                let id = seg.id;
                                                let closed = seg.ended_at.is_some();
                                                move |_| {
                                                    let Some(date) = segment_day else {
                                                        action_error.set("This day's date could not be read, so a correction cannot be timed.".to_string());
                                                        return;
                                                    };
                                                    let mut body = serde_json::Map::new();
                                                    match local_time_to_utc(date, &edit_start.read(), zone) {
                                                        Some(at) => { body.insert("started_at".to_string(), serde_json::json!(at)); }
                                                        None => {
                                                            action_error.set("That start time does not exist on this day in your time zone.".to_string());
                                                            return;
                                                        }
                                                    }
                                                    if closed {
                                                        match local_time_to_utc(date, &edit_end.read(), zone) {
                                                            Some(at) => { body.insert("ended_at".to_string(), serde_json::json!(at)); }
                                                            None => {
                                                                action_error.set("That end time does not exist on this day in your time zone.".to_string());
                                                                return;
                                                            }
                                                        }
                                                    }
                                                    correcting.set(None);
                                                    put_segment(id, serde_json::Value::Object(body));
                                                }
                                            },
                                            "Save"
                                        }
                                        // Reopen is its own button because on
                                        // the wire it is an explicit null,
                                        // and an empty field that means "undo
                                        // the clock-out" is not something
                                        // anyone discovers.
                                        if seg.ended_at.is_some() {
                                            Button {
                                                variant: ButtonVariant::Secondary,
                                                size: ButtonSize::Small,
                                                loading: *busy.read(),
                                                disabled: !can_mutate,
                                                onclick: {
                                                    let id = seg.id;
                                                    move |_| {
                                                        correcting.set(None);
                                                        put_segment(id, serde_json::json!({ "ended_at": serde_json::Value::Null }));
                                                    }
                                                },
                                                "Reopen"
                                            }
                                        }
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            size: ButtonSize::Small,
                                            loading: *busy.read(),
                                            disabled: !can_mutate,
                                            onclick: {
                                                let id = seg.id;
                                                move |_| removing.set(Some(id))
                                            },
                                            "Remove"
                                        }
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Small,
                                            onclick: move |_| correcting.set(None),
                                            "Cancel"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Named rather than generic: "Remove this clock entry?" beside
                // three chips does not say which. The span is what the person
                // is looking at on the chip they clicked.
                if let Some(id) = removing() {
                    {
                        let target = day.segments.iter().find(|s| s.id == id);
                        let label = target
                            .map(|s| {
                                format!(
                                    "{} {}",
                                    if s.kind == "break" { "break" } else { "work" },
                                    segment_span(s.started_at, s.ended_at)
                                )
                            })
                            .unwrap_or_else(|| "clock entry".to_string());
                        rsx! {
                            crate::components::ConfirmDialog {
                                open: true,
                                title: "Remove this clock entry".to_string(),
                                message: format!(
                                    "The {label} entry is removed from this day and the day's clocked total drops by its length.                                      The removal is recorded. This cannot be undone; clocking in again starts a new entry."
                                ),
                                confirm_text: "Remove".to_string(),
                                cancel_text: "Cancel".to_string(),
                                destructive: true,
                                error: action_error.read().clone(),
                                loading: *busy.read(),
                                onconfirm: move |_| {
                                    correcting.set(None);
                                    removing.set(None);
                                    delete_segment(id);
                                },
                                oncancel: move |_| {
                                    if !*busy.read() {
                                        removing.set(None);
                                    }
                                },
                            }
                        }
                    }
                }
                if day.logged_minutes > 0 {
                    div { class: "grid grid-cols-1 gap-3 sm:grid-cols-2 text-sm",
                        div {
                            h3 { class: "font-medium text-content mb-1", "Tickets" }
                            if day.breakdown.tickets.is_empty() {
                                p { class: "text-subtle", "None" }
                            } else {
                                ul { class: "space-y-1",
                                    for t in day.breakdown.tickets.iter() {
                                        li { key: "{t.ticket_id}", class: "flex justify-between gap-2",
                                            Link {
                                                to: Route::TicketDetail { id: t.ticket_id.to_string() },
                                                class: "truncate text-accent hover:opacity-90",
                                                "{ticket_label(t)}"
                                            }
                                            span { class: "shrink-0", "{fmt_duration(t.minutes)}" }
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            h3 { class: "font-medium text-content mb-1", "Projects and own time" }
                            ul { class: "space-y-1",
                                for p in day.breakdown.projects.iter() {
                                    li { key: "{p.project_id}", class: "flex justify-between gap-2",
                                        Link {
                                            to: Route::ProjectDetail { id: p.project_id.to_string() },
                                            class: "truncate text-accent hover:opacity-90",
                                            "{p.project_name.clone().unwrap_or_else(|| \"Project\".to_string())}"
                                        }
                                        span { class: "shrink-0", "{fmt_duration(p.minutes)}" }
                                    }
                                }
                                li { class: "flex justify-between gap-2",
                                    span { "Administrative" }
                                    span { "{fmt_duration(day.breakdown.administrative.minutes)}" }
                                }
                                if day.breakdown.unattached.minutes > 0 {
                                    li { class: "flex justify-between gap-2",
                                        span { "Unattached client work" }
                                        span { "{fmt_duration(day.breakdown.unattached.minutes)}" }
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

/// The clock face of an instant, in the viewer's own zone.
///
/// `format_user_datetime` renders against `users.timezone` (the same
/// preference the server dates a clock-in with), and only the time half is
/// wanted here: the day is named by the card itself.
fn clock_time(dt: DateTime<Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    let full = crate::utils::datetime::format_user_datetime(dt, pref.as_deref());
    full.rsplit(' ').next().unwrap_or(&full).to_string()
}

/// "09:02 to 12:30", or "09:02 onward" while open, in the viewer's zone.
fn segment_span(started: Option<DateTime<Utc>>, ended: Option<DateTime<Utc>>) -> String {
    let clock = clock_time;
    match (started, ended) {
        (Some(s), Some(e)) => format!("{} to {}", clock(s), clock(e)),
        (Some(s), None) => format!("{} onward", clock(s)),
        _ => String::new(),
    }
}

fn ticket_label(t: &RemoteTicketLine) -> String {
    match (t.ticket_number.as_deref(), t.ticket_title.as_deref()) {
        (Some(n), Some(title)) => format!("{n} {title}"),
        (Some(n), None) => n.to_string(),
        (None, Some(title)) => title.to_string(),
        (None, None) => "Ticket".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The signed gap reads back as the server reports it, and the wording
    /// no longer says "accounted for" (MAPPS-751 called it jargon, because it
    /// is - it is an accounting word in a technician's sentence).
    #[test]
    fn the_unlogged_slot_reads_the_servers_number() {
        assert_eq!(unlogged_label(0), "All clocked time is logged");
        assert!(unlogged_label(30).ends_with("not yet logged"));
        assert!(unlogged_label(-135).ends_with("logged beyond the clock"));
        for n in [-135, 0, 30] {
            assert!(
                !unlogged_label(n).contains("accounted for"),
                "the jargon stays gone"
            );
        }
    }

    /// The elapsed figure is the SERVER's number plus wall time since this
    /// client received it, so a client clock that is wrong in absolute terms
    /// still shows the right elapsed. Only its rate is trusted.
    #[test]
    fn elapsed_is_anchored_on_what_the_server_said() {
        let at = DateTime::parse_from_rfc3339("2026-06-15T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // Server said 40 minutes; 5 minutes of wall time have passed.
        assert_eq!(live_minutes(40, at, at + chrono::Duration::minutes(5)), 45);
        // Same instant as the fetch: exactly what the server said, never more.
        assert_eq!(live_minutes(40, at, at), 40);
        // A clock that jumped backwards must not shrink an elapsed figure.
        assert_eq!(live_minutes(40, at, at - chrono::Duration::hours(3)), 40);
    }

    /// The card names the day whenever it is not the reader's own today,
    /// which is the overnight case: `GET /workday` returns the OPEN segment's
    /// date, so a clock left running shows yesterday and must say so.
    #[test]
    fn a_day_that_is_not_today_says_which_day_it_is() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(other_day_note("2026-06-15", today, true), None);
        assert_eq!(other_day_note("2026-06-15", today, false), None);
        let running = other_day_note("2026-06-14", today, true).expect("a note");
        assert!(running.starts_with("Still clocked in from"), "{running}");
        let closed = other_day_note("2026-06-14", today, false).expect("a note");
        assert!(closed.starts_with("Showing"), "{closed}");
        // A date the server did not send in the expected shape is not worth
        // a wrong sentence.
        assert_eq!(other_day_note("not-a-date", today, true), None);
    }

    /// MAPPS-754: the policy decides whether the control is drawn, and the
    /// three names are the server's. `off` refuses the owner too, which is
    /// the half a reader is most likely to assume is exempt.
    #[test]
    fn the_policy_decides_who_sees_a_correction_control() {
        let me = uuid::Uuid::new_v4();
        let someone_else = uuid::Uuid::new_v4();

        // off: nobody, including the person whose day it is.
        assert!(!segment_edit_allowed("off", me, false, false, me));
        assert!(!segment_edit_allowed("off", me, true, true, me));

        // owner_or_admin, the default: my own always, another's only as admin.
        assert!(segment_edit_allowed("owner_or_admin", me, false, false, me));
        assert!(!segment_edit_allowed(
            "owner_or_admin",
            me,
            false,
            true,
            someone_else
        ));
        assert!(segment_edit_allowed(
            "owner_or_admin",
            me,
            true,
            true,
            someone_else
        ));

        // owner_or_manager widens it to anyone who manages users.
        assert!(segment_edit_allowed(
            "owner_or_manager",
            me,
            false,
            true,
            someone_else
        ));
        assert!(!segment_edit_allowed(
            "owner_or_manager",
            me,
            false,
            false,
            someone_else
        ));
    }

    /// A policy this build does not know reads as the default, which is what
    /// the server does with one - not as `off`, which would hide a control
    /// the request would have been allowed to make.
    #[test]
    fn an_unknown_policy_reads_as_the_default_not_as_off() {
        let me = uuid::Uuid::new_v4();
        assert!(segment_edit_allowed("something_new", me, false, false, me));
        assert!(segment_edit_allowed("", me, false, false, me));
    }

    /// A typed `HH:MM` is a wall clock and the stored value is an instant, so
    /// the day and the USER's zone are what join them. The browser's zone is
    /// deliberately not used: the times shown beside the input are rendered
    /// in the profile zone, and a correction typed as "09:14" has to land on
    /// the "09:14" the person is reading.
    #[test]
    fn a_typed_time_is_read_in_the_users_own_zone() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).expect("a date");
        let ny: chrono_tz::Tz = "America/New_York".parse().expect("a zone");
        let at = local_time_to_utc(date, "09:14", ny).expect("a real time");
        // 09:14 in New York in June is 13:14 UTC.
        assert_eq!(at.to_rfc3339(), "2026-06-15T13:14:00+00:00");
        // And it round-trips back to what the person typed.
        assert_eq!(utc_to_local_time(at, ny), "09:14");

        // A zone far from UTC lands on a different UTC day, which is the case
        // that would silently move a segment if the day were dropped.
        let auckland: chrono_tz::Tz = "Pacific/Auckland".parse().expect("a zone");
        let at = local_time_to_utc(date, "09:14", auckland).expect("a real time");
        assert_eq!(at.to_rfc3339(), "2026-06-14T21:14:00+00:00");
        assert_eq!(utc_to_local_time(at, auckland), "09:14");
    }

    /// A time that does not exist in the zone (the hour a spring-forward
    /// skips) is refused rather than silently resolved, so nobody stores a
    /// time they did not mean.
    #[test]
    fn a_time_that_does_not_exist_is_refused() {
        let ny: chrono_tz::Tz = "America/New_York".parse().expect("a zone");
        // 2026-03-08 02:30 does not happen in New York: the clock goes
        // 01:59:59 -> 03:00:00.
        let spring_forward = NaiveDate::from_ymd_opt(2026, 3, 8).expect("a date");
        assert_eq!(local_time_to_utc(spring_forward, "02:30", ny), None);
        // Garbage is refused the same way.
        let date = NaiveDate::from_ymd_opt(2026, 6, 15).expect("a date");
        assert_eq!(local_time_to_utc(date, "", ny), None);
        assert_eq!(local_time_to_utc(date, "25:00", ny), None);
        assert_eq!(local_time_to_utc(date, "9am", ny), None);
    }

    /// Sixteen hours is the threshold, and it is a whole number of hours in
    /// minutes: an off-by-sixty here either never fires or fires on every
    /// normal day.
    #[test]
    fn the_stale_threshold_is_sixteen_hours_of_minutes() {
        assert_eq!(STALE_AFTER_HOURS * 60, 960);
    }

    /// No date means the server's own today, never a date computed here.
    #[test]
    fn the_query_sends_only_what_was_picked() {
        assert_eq!(day_query(None, None), "/workday");
        let d = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(day_query(Some(d), None), "/workday?date=2026-06-15");
        let u = uuid::Uuid::nil();
        assert_eq!(
            day_query(Some(d), Some(u)),
            format!("/workday?date=2026-06-15&user_id={u}")
        );
    }

    /// A break wins over clocked in; out is out. `is_clocked_in` is any open
    /// segment on the wire, so a break satisfies it too.
    #[test]
    fn the_state_follows_the_flags() {
        assert_eq!(ClockState::read(true, true), ClockState::OnBreak);
        assert_eq!(ClockState::read(true, false), ClockState::In);
        assert_eq!(ClockState::read(false, false), ClockState::Out);
        assert!(ClockState::In.is_running() && ClockState::OnBreak.is_running());
        assert!(!ClockState::Out.is_running());
    }

    /// The two running states must differ from each other and from the third
    /// by something that survives greyscale, because the accent may itself be
    /// red on this tenant and `Danger` is a hardcoded red: colour alone
    /// cannot carry this distinction.
    #[test]
    fn every_state_is_distinguishable_without_colour() {
        let states = [ClockState::Out, ClockState::In, ClockState::OnBreak];
        for (i, a) in states.iter().enumerate() {
            for b in states.iter().skip(i + 1) {
                assert_ne!(a.label(), b.label(), "the words differ");
                assert_ne!(
                    a.container_class(),
                    b.container_class(),
                    "the container differs, not only its colour"
                );
            }
        }
        // Border weight, not hue, is what carries it.
        assert!(ClockState::In.container_class().contains("border-2"));
        assert!(ClockState::OnBreak.container_class().contains("dashed"));
        assert!(!ClockState::Out.container_class().contains("border-2"));
    }

    /// The wire shape decodes with its nested breakdown, and a 404 is not a
    /// day: the module gate answers it for both modules.
    #[test]
    fn the_day_decodes_from_the_wire() {
        let day: RemoteWorkDay = serde_json::from_value(serde_json::json!({
            "user_id": uuid::Uuid::nil(), "date": "2026-06-15",
            "is_clocked_in": true, "on_break": false, "track_breaks": true,
            "segments": [{ "id": uuid::Uuid::nil(), "kind": "work",
                           "started_at": "2026-06-15T09:02:00Z", "ended_at": null, "minutes": 40 }],
            "clocked_minutes": 40, "break_minutes": 0, "logged_minutes": 30, "unlogged_minutes": 10,
            "breakdown": { "tickets": [{ "ticket_id": uuid::Uuid::nil(), "ticket_number": "T000123",
                                         "ticket_title": "Printer", "minutes": 30, "entry_count": 1 }],
                           "projects": [], "administrative": { "minutes": 0, "entry_count": 0 },
                           "unattached": { "minutes": 0, "entry_count": 0 } }
        }))
        .expect("decodes");
        assert!(day.is_clocked_in && day.track_breaks);
        assert_eq!(day.segments[0].kind, "work");
        assert_eq!(ticket_label(&day.breakdown.tickets[0]), "T000123 Printer");
        assert_eq!(day.unlogged_minutes, 10);
    }
}

/// MAPPS-746: the strip says why the clock is absent.
#[cfg(test)]
mod mapps746_strip_says_why_tests {
    use super::MODULES_OFF_NOTICE;

    /// Only the loading state renders nothing; a failed load offers a retry
    /// and a 404 names the module, so "where is the clock in UI?" is
    /// answered on the page.
    #[test]
    fn a_failed_load_and_a_disabled_module_both_say_so() {
        let src = include_str!("work_day.rs");
        let head = &src[..src
            .find("mod mapps746_strip_says_why_tests")
            .expect("this module")];
        assert!(
            head.contains("None => return rsx! {},"),
            "the FIRST load stays silent"
        );
        // MAPPS-753: and only the first. A restarted resource reads `None`
        // again, so a bare `None => rsx! {}` made the whole card disappear
        // every time the refetch tick fired. The cached day is what keeps it
        // on the page; losing this arm brings the blink back and nothing
        // fails.
        assert!(
            head.contains("None => match last_day() {"),
            "a refetch re-renders the last day rather than blanking the card"
        );
        assert!(
            !head
                .contains("None | Some(None) | Some(Some(DayLoad::ModulesOff)) => return rsx! {},"),
            "the silent arm is gone"
        );
        assert!(head.contains("\"Could not load today's clock.\""));
        assert!(
            head.contains("onclick: move |_| day_resource.restart(),"),
            "a retry, not a reload"
        );
        assert!(head.contains("\"{MODULES_OFF_NOTICE}\""));
        assert!(
            MODULES_OFF_NOTICE.contains("Timesheets module"),
            "the module is named"
        );
        assert!(
            !MODULES_OFF_NOTICE.contains("Settings"),
            "no link is promised that the client cannot make"
        );
    }
}

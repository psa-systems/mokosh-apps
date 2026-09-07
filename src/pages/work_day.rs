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

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorBanner};
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
    Day(Box<RemoteWorkDay>),
    /// The 404 both module gates answer with. Not an error: the feature is
    /// off for this tenant and the strip stays out of the page.
    ModulesOff,
}

/// MAPPS-730: the sentence under the totals. The server's three numbers,
/// read back as it reports them; `unlogged` is signed, positive when time
/// was clocked and not logged, negative when more was logged than clocked.
pub(crate) fn gap_line(clocked: i64, logged: i64, unlogged: i64) -> String {
    let head = format!(
        "{} clocked, {} logged",
        fmt_duration(clocked),
        fmt_duration(logged)
    );
    match unlogged {
        0 => format!("{head}, all of it accounted for."),
        n if n > 0 => format!("{head}, {} not yet logged.", fmt_duration(n)),
        n => format!("{head}, {} more logged than clocked.", fmt_duration(-n)),
    }
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

/// The badge beside the buttons: what the day is doing right now.
pub(crate) fn state_label(is_clocked_in: bool, on_break: bool) -> (&'static str, BadgeVariant) {
    match (is_clocked_in, on_break) {
        (_, true) => ("On break", BadgeVariant::Yellow),
        (true, false) => ("Clocked in", BadgeVariant::Green),
        (false, false) => ("Clocked out", BadgeVariant::Gray),
    }
}

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
    // A minute tick so the elapsed figures move while the day is open.
    let mut tick = use_signal(|| 0u32);
    let can_mutate = crate::hooks::use_can_mutate();

    let date_for_resource = picked_date();
    let user_for_resource = picked_user();
    let mut day_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        let _tick = tick();
        let path = day_query(date_for_resource, user_for_resource);
        #[cfg(feature = "app")]
        {
            match crate::hooks::fetch::api::get_authed_typed::<RemoteWorkDay>(&path).await {
                Ok(day) => Some(DayLoad::Day(Box::new(day))),
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
            tick.with_mut(|t| *t = t.wrapping_add(1));
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
    let day = match snap {
        // Loading, a fetch failure, or the modules are off: nothing to show.
        // A failure is logged above; the Time page's own list reports the
        // outage state, so the strip does not repeat it.
        None | Some(None) | Some(Some(DayLoad::ModulesOff)) => return rsx! {},
        Some(Some(DayLoad::Day(day))) => *day,
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
    let clock_in_body = match picked_date() {
        Some(d) => serde_json::json!({ "date": d.format("%Y-%m-%d").to_string() }),
        None => serde_json::json!({}),
    };
    let viewing_someone_else = picked_user().is_some();
    let (state_text, state_variant) = state_label(day.is_clocked_in, day.on_break);
    let gap = gap_line(
        day.clocked_minutes,
        day.logged_minutes,
        day.unlogged_minutes,
    );
    let date_value = picked_date()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| day.date.clone());
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    let disabled_title =
        (!can_mutate).then(|| "Can't change the day while the server is unreachable".to_string());

    rsx! {
        Card { class: "mb-6",
            div { class: "space-y-4",
                div { class: "flex flex-wrap items-center gap-3",
                    h2 { class: "text-lg font-semibold text-content", "Work day" }
                    Badge { variant: state_variant, "{state_text}" }
                    div { class: "flex-1" }
                    input {
                        r#type: "date",
                        class: "rounded-md border border-line bg-surface px-2 py-1 text-sm",
                        value: "{date_value}",
                        oninput: move |e: FormEvent| {
                            picked_date.set(NaiveDate::parse_from_str(&e.value(), "%Y-%m-%d").ok());
                        },
                    }
                    if is_admin && !users.is_empty() {
                        select {
                            class: "rounded-md border border-line bg-surface px-2 py-1 text-sm",
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
                if !action_error.read().is_empty() {
                    ErrorBanner { "{action_error.read()}" }
                }
                // The actions act on the caller's own day only; an admin
                // reading someone else's day gets the view without them.
                if !viewing_someone_else {
                    div { class: "flex flex-wrap gap-2",
                        if !day.is_clocked_in {
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: *busy.read(),
                                disabled: !can_mutate,
                                title: disabled_title.clone(),
                                onclick: {
                                    let body = clock_in_body.clone();
                                    move |_| post("/workday/clock-in", body.clone(), "clock in")
                                },
                                "Clock in"
                            }
                        } else {
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
                                loading: *busy.read(),
                                disabled: !can_mutate,
                                title: disabled_title.clone(),
                                onclick: move |_| post("/workday/clock-out", serde_json::json!({}), "clock out"),
                                "Clock out"
                            }
                        }
                    }
                }
                // The totals and the gap, the server's numbers.
                p { class: "text-sm text-muted",
                    "{gap}"
                    if day.break_minutes > 0 {
                        " Breaks: {fmt_duration(day.break_minutes)}."
                    }
                    if day.unlogged_minutes > 0 {
                        " "
                        Link { to: Route::TimeEntryNew {}, class: "text-accent hover:opacity-90", "Log time" }
                    }
                }
                if !day.segments.is_empty() {
                    ul { class: "flex flex-wrap gap-2 text-xs",
                        for seg in day.segments.iter() {
                            li { key: "{seg.id}",
                                class: if seg.kind == "break" { "rounded-md border border-line px-2 py-1 text-muted" } else { "rounded-md border border-line bg-surface-2 px-2 py-1" },
                                span { class: "font-medium", if seg.kind == "break" { "Break" } else { "Work" } }
                                " {segment_span(seg.started_at, seg.ended_at)} "
                                span { class: "text-muted", "({fmt_duration(seg.minutes)})" }
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

/// "09:02 to 12:30", or "09:02 onward" while open, in the viewer's zone.
fn segment_span(started: Option<DateTime<Utc>>, ended: Option<DateTime<Utc>>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    let clock = |dt: DateTime<Utc>| {
        // The date part is the strip's own header; only the clock matters here.
        let full = crate::utils::datetime::format_user_datetime(dt, pref.as_deref());
        full.rsplit(' ').next().unwrap_or(&full).to_string()
    };
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

    /// The three numbers are the server's; the sentence only reads them.
    #[test]
    fn the_gap_line_reads_the_servers_numbers() {
        assert!(gap_line(220, 190, 30).ends_with("not yet logged."));
        assert!(gap_line(0, 135, -135).ends_with("more logged than clocked."));
        assert!(gap_line(60, 60, 0).ends_with("accounted for."));
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

    /// A break wins over clocked in; out is out.
    #[test]
    fn the_state_badge_follows_the_flags() {
        assert_eq!(state_label(true, true).0, "On break");
        assert_eq!(state_label(true, false).0, "Clocked in");
        assert_eq!(state_label(false, false).0, "Clocked out");
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

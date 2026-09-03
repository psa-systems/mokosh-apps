//! MAPPS-302: "Big View" presentation mode for ops-center wall screens.
//!
//! Three routes (`/big/tickets`, `/big/dispatch`, `/big/calendar`) that
//! render the underlying data without `AppLayout`'s sidebar / top bar /
//! feedback launcher, with larger typography for legibility from across
//! a room, and a periodic auto-refresh tick so the screen stays fresh
//! without operator interaction.
//!
//! URL query params let the operator pre-configure the view for a
//! specific kiosk:
//!   - `?refresh=<seconds>` overrides the default 30-second tick.
//!   - `?status=<name>` and `?priority=<name>` scope the Tickets view
//!     (case-insensitive match against the server's status / priority
//!     name field).
//!
//! Companion ticket: MAPPS-280 (Dispatch week / month views) will share
//! the auto-refresh + filter-from-URL plumbing this module exposes.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Badge, BadgeVariant, ClockIcon, IconSize};
use crate::utils::sort_keys::TICKETS_RECENT_SORT;
use crate::utils::Paginated;

const DEFAULT_REFRESH_SECS: u32 = 30;

/// Read `?refresh=<seconds>` from the URL, clamped to a sane range. The
/// default 30s keeps the screen in step with the server without
/// hammering it; operators tune via the query param.
fn read_refresh_seconds() -> u32 {
    #[cfg(feature = "app")]
    {
        if let Some(raw) = crate::utils::url::current_query_param("refresh") {
            if let Ok(n) = raw.parse::<u32>() {
                return n.clamp(5, 600);
            }
        }
    }
    DEFAULT_REFRESH_SECS
}

/// Returns a signal that monotonically ticks every `seconds`. Resources
/// that want to auto-refresh read this in their closure so Dioxus
/// re-runs the fetch on each tick.
fn use_periodic_tick(seconds: u32) -> Signal<u64> {
    let mut tick = use_signal(|| 0u64);
    use_effect(move || {
        #[cfg(feature = "app")]
        {
            // MAPPS-504: a spawned sleep loop rather than `setInterval`
            // plus a leaked wasm closure. Same cadence, no browser
            // binding, and the Big View routes are kiosk-mounted so the
            // loop runs for the lifetime of the session either way.
            dioxus::prelude::spawn(async move {
                loop {
                    crate::platform::timer::sleep_ms(seconds * 1000).await;
                    *tick.write() += 1;
                }
            });
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = seconds;
        }
    });
    tick
}

#[derive(Props, Clone, PartialEq)]
pub struct BigLayoutProps {
    children: Element,
    title: String,
}

/// Stripped-down layout for the Big View routes. No sidebar, no top
/// bar's worth of chrome. A minimal title strip + a refresh badge so
/// the operator can see when the screen last updated, and the children
/// rendered with a larger typographic scale via the `lc-big` class
/// (`assets/styles.css` could pick this up if you want global
/// adjustments later; for now the per-page `text-*` classes are
/// applied directly).
#[component]
pub fn BigLayout(props: BigLayoutProps) -> Element {
    let now_str = current_time_string();
    rsx! {
        div { class: "h-screen w-screen overflow-hidden bg-app text-content flex flex-col",
            // Title strip. Sized large enough to read from across a
            // room; no menu, no user chip, just the page name and a
            // live clock.
            header { class: "flex items-center justify-between px-8 py-4 border-b border-line bg-surface",
                h1 { class: "text-3xl font-bold tracking-tight", "{props.title}" }
                div { class: "flex items-center gap-3 text-sm text-muted",
                    ClockIcon { size: IconSize::Small }
                    span { "Last updated {now_str}" }
                }
            }
            // Children fill the rest of the viewport.
            main { class: "flex-1 overflow-auto px-8 py-6", {props.children} }
        }
    }
}

/// MAPPS-504: was `Date.prototype.toLocaleTimeString("en-US")`. The
/// locale was pinned to `en-US` anyway, so the same rendering comes out
/// of `chrono` on both targets ("3:45:12 PM"), in the machine's local
/// zone.
fn current_time_string() -> String {
    chrono::Local::now().format("%-I:%M:%S %p").to_string()
}

// ===========================================================================
// /big/tickets
// ===========================================================================

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct BigTicketRow {
    id: uuid::Uuid,
    ticket_number: String,
    title: String,
    #[serde(default)]
    company_name: String,
    status: BigStatusRef,
    priority: BigStatusRef,
    #[serde(default)]
    assigned_to_name: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Default, PartialEq)]
struct BigStatusRef {
    #[serde(default)]
    name: String,
}

#[component]
pub fn BigTicketsPage() -> Element {
    let refresh_secs = read_refresh_seconds();
    let tick = use_periodic_tick(refresh_secs);
    let status_filter = crate::utils::url::current_query_param("status").unwrap_or_default();
    let priority_filter = crate::utils::url::current_query_param("priority").unwrap_or_default();

    // MAPPS-357: the ticket list is this kiosk's PRIMARY resource - if it
    // fails the board has no content. Route it through `use_remote_resource`
    // so a failed fetch during an outage is preserved instead of swallowed to
    // an empty `Vec` that reads as "no open tickets" (dangerously reassuring
    // on a NOC wall). The hook already subscribes to reachability +
    // `active_tenant_generation`, so we drop the explicit `_gen` line but keep
    // the periodic-tick read (the hook does not know about it) and hand back
    // the raw `Result` (no `.ok()/.unwrap_or_default()`).
    let tickets_data = crate::hooks::use_remote_resource(move || async move {
        // Subscribe to the tick so Dioxus re-fetches on every interval
        // firing. The value is otherwise unused.
        let _ = *tick.read();
        crate::hooks::fetch::api::get_authed::<Paginated<BigTicketRow>>(&format!(
            "/tickets?per_page=50&{TICKETS_RECENT_SORT}"
        ))
        .await
        .map(|p| p.data)
    });

    // MAPPS-357: on outage, swap the ticket grid for an honest "server down"
    // panel. This is a chrome-less kiosk (BigLayout, not AppLayout), so the
    // shared `ContentUnavailable` - which mounts AppLayout's sidebar / top bar
    // - would be wrong here; render the notice inside the same BigLayout so the
    // title strip + clock stay put. `use_remote_resource` auto-refetches on
    // reconnect, so the live grid returns on its own with no operator action.
    if tickets_data.is_unavailable() {
        return rsx! {
            BigLayout { title: "Tickets - Live Queue".to_string(),
                div { class: "py-16 text-center",
                    p { class: "text-3xl font-semibold text-content", "Server unreachable" }
                    p { class: "mt-3 text-xl text-muted",
                        "This board will refresh on its own once the connection is back."
                    }
                }
            }
        };
    }
    // MAPPS-404: capture the loading state before value_or_default() consumes
    // the resource, so the board below shows a loading state on the first fetch
    // instead of the "No tickets match" empty board.
    let is_loading = tickets_data.is_loading();
    let tickets = tickets_data.value_or_default();
    let filtered: Vec<BigTicketRow> = tickets
        .into_iter()
        .filter(|t| {
            if !status_filter.is_empty() && !t.status.name.eq_ignore_ascii_case(&status_filter) {
                return false;
            }
            if !priority_filter.is_empty()
                && !t.priority.name.eq_ignore_ascii_case(&priority_filter)
            {
                return false;
            }
            true
        })
        .collect();
    let total = filtered.len();

    rsx! {
        BigLayout { title: "Tickets - Live Queue".to_string(),
            if is_loading {
                // MAPPS-404: shimmer card grid while the first fetch is in
                // flight, instead of the "No tickets match" empty board that
                // then pops to real cards once the queue resolves.
                crate::components::CardGridSkeleton {}
            } else if total == 0 {
                div { class: "py-16 text-center text-2xl text-muted",
                    "No tickets match the current filter."
                }
            } else {
                div { class: "grid grid-cols-1 lg:grid-cols-2 gap-4",
                    for t in filtered.into_iter() {
                        BigTicketCard { key: "{t.id}", ticket: t }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BigTicketCardProps {
    ticket: BigTicketRow,
}

#[component]
fn BigTicketCard(props: BigTicketCardProps) -> Element {
    let t = props.ticket;
    let prio_variant = match t.priority.name.to_lowercase().as_str() {
        "critical" => BadgeVariant::Red,
        "high" => BadgeVariant::Red,
        "medium" => BadgeVariant::Yellow,
        "low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };
    let assignee = t
        .assigned_to_name
        .clone()
        .unwrap_or_else(|| "Unassigned".to_string());
    rsx! {
        div { class: "rounded-lg border border-line bg-surface px-6 py-4 shadow-sm",
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-3",
                    span { class: "text-xl font-semibold text-accent", "{t.ticket_number}" }
                    Badge { variant: prio_variant, "{t.priority.name}" }
                }
                span { class: "text-sm text-muted", "{t.status.name}" }
            }
            p { class: "mt-2 text-2xl font-medium truncate", "{t.title}" }
            div { class: "mt-2 flex items-center justify-between text-sm text-muted",
                span { "{t.company_name}" }
                span { "Assigned: {assignee}" }
            }
        }
    }
}

// ===========================================================================
// /big/dispatch
// ===========================================================================

#[component]
pub fn BigDispatchPage() -> Element {
    let refresh_secs = read_refresh_seconds();
    let _tick = use_periodic_tick(refresh_secs);
    // MAPPS-357: N/A here. This page fetches nothing of its own; it embeds
    // `crate::pages::calendar::DispatchBoardPage`, which owns the data fetch
    // and therefore the outage state. The unavailable retrofit belongs in
    // calendar.rs, not in this wrapper (out of scope for this one-file task).
    // The existing DispatchBoardPage already owns the per-technician
    // swimlane render; embed it inside the BigLayout chrome with a
    // larger title strip. The board itself reads the day from the
    // signed-in user's timezone and renders per-tech rows, so the kiosk
    // gets the same data without a code-path divergence. MAPPS-280
    // (week / month views) will add a view-mode toggle the operator
    // can drive via `?view=<day|week|month>`.
    rsx! {
        BigLayout { title: "Dispatch Board - Today".to_string(),
            crate::pages::calendar::DispatchBoardPage {}
        }
    }
}

// ===========================================================================
// /big/calendar
// ===========================================================================

#[component]
pub fn BigCalendarPage() -> Element {
    let refresh_secs = read_refresh_seconds();
    let _tick = use_periodic_tick(refresh_secs);
    // MAPPS-357: N/A here. Like BigDispatchPage, this fetches nothing of its
    // own; it embeds `crate::pages::calendar::CalendarPage`, which owns the
    // data fetch and its own outage state. The unavailable retrofit belongs in
    // calendar.rs, not in this wrapper (out of scope for this one-file task).
    rsx! {
        BigLayout { title: "Calendar - This Week".to_string(),
            crate::pages::calendar::CalendarPage {}
        }
    }
}

//! PMS-472 + PMS-487: view + edit surfaces for saved dashboards.
//!
//! PMS-472 introduced the read-only render pipeline: layout JSONB ->
//! typed `DashboardLayout` -> CSS-grid placement of widget cards.
//!
//! PMS-487 layers the editor (toggle, per-widget grid-coord inputs,
//! add-from-catalog, save round-trip through `PATCH /dashboards/{id}`)
//! and replaces the placeholder bodies with real data fetches against
//! existing PSA endpoints. The editor uses number inputs rather than
//! drag-and-drop to avoid a heavy dnd dependency for the v1; PMS-487
//! AC explicitly accepts this shape.

use chrono::{Datelike, Duration, Utc, Weekday};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    use_page_title, AlertType, Button, ButtonVariant, Card, Input, PageHeader, StatCard, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::utils::Paginated;

/// Layout JSONB shape the SPA owns. The server stores `layout` as an
/// opaque `serde_json::Value`; this struct is the canonical schema the
/// view + editor surfaces deserialise from. Unknown widget keys render
/// as an "Unknown widget" placeholder so a forward-compatible server
/// row never blank-screens the page.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DashboardLayout {
    #[serde(default)]
    pub widgets: Vec<WidgetSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WidgetSpec {
    pub widget_key: String,
    #[serde(default = "default_grid_one")]
    pub grid_col: u32,
    #[serde(default = "default_grid_one")]
    pub grid_row: u32,
    #[serde(default = "default_grid_span")]
    pub grid_col_span: u32,
    #[serde(default = "default_grid_one")]
    pub grid_row_span: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_scope: Option<serde_json::Value>,
}

fn default_grid_one() -> u32 {
    1
}
fn default_grid_span() -> u32 {
    4
}

// MAPPS-357: `Default` is required so this row can back a
// `use_remote_resource::<SavedDashboardRow>` (the hook's `T: Default` bound);
// the default row is only ever produced for a non-outage failure the hook
// degrades to `Ready(default)`, never rendered as real content.
#[derive(Clone, Debug, Default, Deserialize)]
struct SavedDashboardRow {
    #[allow(dead_code)]
    id: Uuid,
    name: String,
    #[serde(default)]
    layout: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize)]
struct UpdateLayoutBody {
    layout: serde_json::Value,
}

/// Catalog of supported widget keys. Used by both the renderer and
/// the editor's add-widget panel.
pub struct WidgetCatalogEntry {
    pub key: &'static str,
    pub title: &'static str,
    pub placeholder: &'static str,
}

pub const WIDGET_CATALOG: &[WidgetCatalogEntry] = &[
    WidgetCatalogEntry {
        key: "tickets_by_status",
        title: "Tickets by status",
        placeholder: "Open tickets grouped by priority bucket.",
    },
    WidgetCatalogEntry {
        key: "time_this_week",
        title: "Time this week",
        placeholder: "Hours logged Monday through today.",
    },
    WidgetCatalogEntry {
        key: "sla_at_risk",
        title: "SLA at risk",
        placeholder: "Tickets approaching SLA breach.",
    },
    WidgetCatalogEntry {
        key: "open_invoices",
        title: "Open invoices",
        placeholder: "Invoices in sent / overdue state.",
    },
    WidgetCatalogEntry {
        key: "recent_audit_log",
        title: "Recent audit log",
        placeholder: "Last 5 audit events in the tenant.",
    },
];

pub fn catalog_lookup(key: &str) -> Option<&'static WidgetCatalogEntry> {
    WIDGET_CATALOG.iter().find(|w| w.key == key)
}

/// View one saved dashboard by id. Toggles into the editor inline.
#[component]
pub fn SavedDashboardViewPage(id: String) -> Element {
    let version = use_signal(|| 0u32);
    // MAPPS-357: computed once in the body (a plain reactive flag read) so the
    // editor's Save re-enables itself on reconnect. Read before any early
    // return, per the rules of hooks.
    let can_mutate = crate::hooks::use_can_mutate();
    let id_for_fetch = id.clone();
    // MAPPS-357: the saved dashboard row is this page's PRIMARY resource - if
    // it fails to load, there is nothing meaningful to render. Move to
    // `use_remote_resource` so an outage surfaces the honest unavailable state
    // instead of a perpetual "Loading…". The fetcher returns the raw
    // `Result` (no `.ok()`); the `version` subscription stays inside the
    // closure so a Save still refetches, and the hook adds the
    // reachability + tenant-generation deps itself.
    let row_resource = crate::hooks::use_remote_resource(move || {
        let id = id_for_fetch.clone();
        async move {
            let _ = version.read();
            crate::hooks::fetch::api::get_authed::<SavedDashboardRow>(&format!("/dashboards/{id}"))
                .await
        }
    });
    // MAPPS-366: set the tab title (the saved dashboard's name once loaded)
    // unconditionally, BEFORE the unavailable early return, per the rules of
    // hooks. The persistent AppShell reads it.
    let title = row_resource
        .ready()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Dashboard".to_string());
    use_page_title(&title);
    if row_resource.is_unavailable() {
        return rsx! {
            crate::components::ContentUnavailable { title: "Dashboard".to_string() }
        };
    }
    // `into_ready()` keeps the Loading (None) vs Ready (Some) distinction the
    // old `.flatten()` provided, so a still-loading page keeps its "Loading…"
    // header rather than flashing an empty editor.
    let row = row_resource.into_ready();

    rsx! {
        match row {
            Some(d) => render_with_editor(d, version, can_mutate),
            None => rsx! {
                PageHeader {
                    title: "Dashboard".to_string(),
                    subtitle: "Loading…".to_string(),
                }
            },
        }
    }
}

/// `/dashboard` entrypoint. Hits `/dashboards/default` first so a user
/// who pinned a saved layout lands on it; absence of a pinned row
/// renders the v1 hardcoded dashboard as the fallback.
/// MAPPS-604: a contact-plane session has no `/dashboards/*` surface
/// (those are staff-scoped saved layouts). Dispatch to the staff body
/// or short-circuit to the hardcoded `DashboardPage` (which itself
/// branches to the contact-summary path) BEFORE any hook fires, so
/// MAPPS-602 (hooks-before-return guard) stays green: the staff body
/// lives in its own component and runs its hooks unconditionally.
#[component]
pub fn DefaultDashboardPage() -> Element {
    #[cfg(feature = "web")]
    if crate::hooks::fetch::api::has_contact_session()
        && crate::hooks::fetch::api::current_access_token().is_none()
    {
        return rsx! { crate::pages::dashboard::DashboardPage {} };
    }
    rsx! { StaffDefaultDashboardBody {} }
}

#[component]
fn StaffDefaultDashboardBody() -> Element {
    // MAPPS-357: the pinned-default lookup is this page's PRIMARY resource.
    // On an outage the old `.ok().flatten()` collapsed a failed fetch to
    // `None`, which silently fell through to the hardcoded dashboard and hid
    // the outage as "no pin". `use_remote_resource` keeps the failure so we
    // render the honest unavailable state instead. The fetcher returns the raw
    // `Result<Option<_>, _>`; a real absence still resolves to `Ready(None)`.
    let default_resource = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Option<SavedDashboardRow>>("/dashboards/default")
            .await
    });
    // MAPPS-366: set the tab title unconditionally, BEFORE the unavailable
    // early return (rules of hooks). A pinned dashboard shows its own name; the
    // hardcoded fallback (None branch -> DashboardPage) sets "Dashboard" itself.
    let title = default_resource
        .ready()
        .and_then(|pinned| pinned.as_ref())
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Dashboard".to_string());
    use_page_title(&title);
    if default_resource.is_unavailable() {
        // This is the /dashboard entrypoint, so no self-referential link.
        return rsx! {
            crate::components::ContentUnavailable {
                title: "Dashboard".to_string(),
                show_dashboard_link: false,
            }
        };
    }
    let pinned = default_resource.value_or_default();

    match pinned {
        Some(d) => rsx! { {render_read_only(d)} },
        None => rsx! { crate::pages::dashboard::DashboardPage {} },
    }
}

fn render_read_only(d: SavedDashboardRow) -> Element {
    let layout: DashboardLayout = serde_json::from_value(d.layout.clone()).unwrap_or_default();
    rsx! {
        PageHeader {
            title: d.name,
            subtitle: "Saved dashboard".to_string(),
        }
        {render_grid(&layout)}
    }
}

fn render_with_editor(d: SavedDashboardRow, mut version: Signal<u32>, can_mutate: bool) -> Element {
    let initial: DashboardLayout = serde_json::from_value(d.layout.clone()).unwrap_or_default();
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| initial.clone());
    let mut dirty = use_signal(|| false);
    let dashboard_id = d.id;
    let name = d.name.clone();

    let on_save = move |_| {
        let layout = draft.read().clone();
        spawn(async move {
            let body = UpdateLayoutBody {
                layout: serde_json::to_value(&layout).unwrap_or(serde_json::json!({})),
            };
            match crate::hooks::fetch::api::patch_authed::<SavedDashboardRow, _>(
                &format!("/dashboards/{dashboard_id}"),
                &body,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Layout saved");
                    editing.set(false);
                    dirty.set(false);
                    version += 1;
                }
                Err(e) => {
                    crate::hooks::toast::push_toast(AlertType::Error, format!("Save failed: {e}"));
                }
            }
        });
    };

    let on_cancel = move |_| {
        draft.set(initial.clone());
        dirty.set(false);
        editing.set(false);
    };

    let on_edit = move |_| editing.set(true);

    let mut on_add_widget = move |key: &'static str| {
        let next_row = draft
            .read()
            .widgets
            .iter()
            .map(|w| w.grid_row)
            .max()
            .unwrap_or(0)
            + 1;
        draft.write().widgets.push(WidgetSpec {
            widget_key: key.to_string(),
            grid_col: 1,
            grid_row: next_row,
            grid_col_span: 6,
            grid_row_span: 1,
            filter_scope: None,
        });
        dirty.set(true);
    };

    let actions = if *editing.read() {
        rsx! {
            div { class: "inline-flex gap-2",
                // Cancel only resets the in-memory draft (no server call), so
                // it stays enabled while down; only Save writes to the server.
                Button { variant: ButtonVariant::Secondary, onclick: on_cancel, "Cancel" }
                // MAPPS-357: Save is the only control that writes to the server
                // (PATCH /dashboards/{id}); block it while the server is
                // unreachable so the click cannot silently fail.
                Button {
                    variant: ButtonVariant::Primary,
                    onclick: on_save,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't save the layout while the server is unreachable".to_string()),
                    "Save layout"
                }
            }
        }
    } else {
        rsx! {
            Button { variant: ButtonVariant::Secondary, onclick: on_edit, "Edit layout" }
        }
    };

    rsx! {
        PageHeader {
            title: name,
            subtitle: "Saved dashboard".to_string(),
            actions: actions,
        }
        if *editing.read() {
            // MAPPS-357: the add-widget buttons, the per-widget coord inputs,
            // and Remove all mutate the in-memory `draft` only (never the
            // server); they are persisted solely by the disabled Save, so they
            // stay enabled while down - mirroring invitations.rs, where the invite
            // form inputs stay editable and only the submit is blocked.
            Card { title: "Add widget".to_string(),
                div { class: "flex flex-wrap gap-2",
                    for entry in WIDGET_CATALOG.iter() {
                        {
                            let key = entry.key;
                            rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| on_add_widget(key),
                                    "+ {entry.title}"
                                }
                            }
                        }
                    }
                }
            }
            {render_editor_grid(draft, dirty)}
        } else {
            {render_grid(&draft.read())}
        }
    }
}

fn render_grid(layout: &DashboardLayout) -> Element {
    if layout.widgets.is_empty() {
        return rsx! {
            div { class: "p-6 text-center text-muted",
                "This dashboard has no widgets yet. Click 'Edit layout' to add some."
            }
        };
    }
    rsx! {
        div { class: "grid grid-cols-12 gap-4",
            for (idx, w) in layout.widgets.iter().enumerate() {
                {render_widget_cell(w, idx)}
            }
        }
    }
}

fn render_editor_grid(draft: Signal<DashboardLayout>, dirty: Signal<bool>) -> Element {
    let widgets = draft.read().widgets.clone();
    if widgets.is_empty() {
        return rsx! {
            div { class: "p-6 text-center text-muted mt-4",
                "Empty layout. Add a widget from the catalog above."
            }
        };
    }
    rsx! {
        div { class: "grid grid-cols-12 gap-4 mt-4",
            for (idx, w) in widgets.iter().enumerate() {
                {render_widget_editor_cell(w.clone(), idx, draft, dirty)}
            }
        }
    }
}

fn render_widget_cell(w: &WidgetSpec, idx: usize) -> Element {
    let entry = catalog_lookup(&w.widget_key);
    let title = entry.map(|e| e.title).unwrap_or("Unknown widget");
    let style = grid_style(w);
    let key = format!("w-{idx}-{}", w.widget_key);
    // `time_this_week` renders as a `StatCard`, which already carries its own
    // card chrome (border, shadow, padding) and label, so wrapping it in
    // another titled `Card` would double up both.
    if w.widget_key == "time_this_week" {
        return rsx! {
            div { key: "{key}", style: "{style}",
                {render_widget_body(&w.widget_key)}
            }
        };
    }
    rsx! {
        div { key: "{key}", style: "{style}",
            Card { title: title.to_string(),
                {render_widget_body(&w.widget_key)}
            }
        }
    }
}

fn render_widget_editor_cell(
    w: WidgetSpec,
    idx: usize,
    mut draft: Signal<DashboardLayout>,
    mut dirty: Signal<bool>,
) -> Element {
    let entry = catalog_lookup(&w.widget_key);
    let title = entry.map(|e| e.title).unwrap_or("Unknown widget");
    let style = grid_style(&w);
    let key = format!("e-{idx}-{}", w.widget_key);

    let on_remove = move |_| {
        draft.write().widgets.remove(idx);
        dirty.set(true);
    };

    rsx! {
        div { key: "{key}", style: "{style}",
            Card { title: title.to_string(),
                div { class: "grid grid-cols-2 gap-2 text-sm",
                    Input {
                        name: "{key}-col",
                        label: "Col",
                        r#type: "number",
                        min: "1",
                        max: "12",
                        value: "{w.grid_col}",
                        oninput: move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                if let Some(s) = draft.write().widgets.get_mut(idx) {
                                    s.grid_col = v.max(1);
                                }
                                dirty.set(true);
                            }
                        },
                    }
                    Input {
                        name: "{key}-row",
                        label: "Row",
                        r#type: "number",
                        min: "1",
                        value: "{w.grid_row}",
                        oninput: move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                if let Some(s) = draft.write().widgets.get_mut(idx) {
                                    s.grid_row = v.max(1);
                                }
                                dirty.set(true);
                            }
                        },
                    }
                    Input {
                        name: "{key}-w",
                        label: "Width",
                        r#type: "number",
                        min: "1",
                        max: "12",
                        value: "{w.grid_col_span}",
                        oninput: move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                if let Some(s) = draft.write().widgets.get_mut(idx) {
                                    s.grid_col_span = v.clamp(1, 12);
                                }
                                dirty.set(true);
                            }
                        },
                    }
                    Input {
                        name: "{key}-h",
                        label: "Height",
                        r#type: "number",
                        min: "1",
                        value: "{w.grid_row_span}",
                        oninput: move |e: FormEvent| {
                            if let Ok(v) = e.value().parse::<u32>() {
                                if let Some(s) = draft.write().widgets.get_mut(idx) {
                                    s.grid_row_span = v.max(1);
                                }
                                dirty.set(true);
                            }
                        },
                    }
                }
                div { class: "flex justify-end mt-3",
                    Button { variant: ButtonVariant::Danger, onclick: on_remove, "Remove" }
                }
            }
        }
    }
}

fn render_widget_body(key: &str) -> Element {
    match key {
        "tickets_by_status" => rsx! { WidgetTicketsByStatus {} },
        "time_this_week" => rsx! { WidgetTimeThisWeek {} },
        "sla_at_risk" => rsx! { WidgetSlaAtRisk {} },
        "open_invoices" => rsx! { WidgetOpenInvoices {} },
        "recent_audit_log" => rsx! { WidgetRecentAuditLog {} },
        _ => rsx! {
            p { class: "text-sm text-muted italic", "Unknown widget key." }
        },
    }
}

fn grid_style(w: &WidgetSpec) -> String {
    let col_start = w.grid_col.max(1);
    let row_start = w.grid_row.max(1);
    let col_span = w.grid_col_span.clamp(1, 12);
    let row_span = w.grid_row_span.max(1);
    format!("grid-column: {col_start} / span {col_span}; grid-row: {row_start} / span {row_span};")
}

// ---------------------------------------------------------------------------
// Per-widget components. Each hits one tiny existing endpoint through
// `use_remote_resource` (MAPPS-785) so a request in flight, a failed fetch,
// and a genuinely empty result render as three distinct states instead of
// the identical zero a bare `use_resource` + `.ok().unwrap_or_default()`
// produced for all three.
// ---------------------------------------------------------------------------

/// Shimmer placeholder shown while a widget's fetch is in flight. Sized to a
/// tile's body rather than a full page, unlike `CardGridSkeleton` /
/// `DetailSkeleton`.
fn widget_loading() -> Element {
    rsx! {
        div { class: "space-y-2",
            div { class: "h-4 w-2/3 bg-surface-2 rounded animate-pulse" }
            div { class: "h-4 w-1/2 bg-surface-2 rounded animate-pulse" }
        }
    }
}

/// Inline failure shown when a widget's fetch could not load because the
/// server is unreachable. Scoped to the one tile so a slow/down endpoint
/// does not blank the whole grid (see the issue's "one shared loading
/// state" alternative, rejected for the same reason).
fn widget_failed() -> Element {
    rsx! {
        p { class: "text-sm text-red-700 dark:text-red-400", "Couldn't load this widget." }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DashboardReportLite {
    #[serde(default)]
    open_by_priority: Vec<ReportBucket>,
    #[serde(default)]
    sla_warnings: i64,
    #[serde(default)]
    sla_breached: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct ReportBucket {
    #[serde(default)]
    label: String,
    #[serde(default)]
    count: i64,
}

#[component]
fn WidgetTicketsByStatus() -> Element {
    let report = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<DashboardReportLite>("/reports/dashboard").await
    });
    if report.is_loading() {
        return widget_loading();
    }
    if report.is_unavailable() {
        return widget_failed();
    }
    let r = report.value_or_default();
    if r.open_by_priority.is_empty() {
        return rsx! { p { class: "text-sm text-muted italic", "No open tickets." } };
    }
    rsx! {
        ul { class: "text-sm space-y-1",
            for b in r.open_by_priority.iter() {
                li { class: "flex justify-between",
                    span { class: "text-muted", "{b.label}" }
                    span { class: "font-medium", "{b.count}" }
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct TimeEntryLite {
    date: chrono::NaiveDate,
    #[serde(default)]
    duration_minutes: i64,
}

#[component]
fn WidgetTimeThisWeek() -> Element {
    // MAPPS-543: this widget sums a week, so it needs rows rather than a count -
    // but only this week's. The server takes `date_from`, so the fetch is
    // bounded by the window the tile reports instead of the tenant's whole time
    // history. It previously read the server's default 25 rows and summed them
    // as if they were the week, which reports a wrong number of hours rather
    // than no number at all.
    let entries = crate::hooks::use_remote_resource(|| async {
        let week_start = monday_of_week(Utc::now().date_naive());
        crate::hooks::fetch::api::get_all_authed::<TimeEntryLite>(&format!(
            "/time-entries?date_from={week_start}"
        ))
        .await
    });
    if entries.is_loading() {
        return widget_loading();
    }
    if entries.is_unavailable() {
        return widget_failed();
    }
    let rows = entries.value_or_default();
    let week_start = monday_of_week(Utc::now().date_naive());
    let minutes: i64 = rows
        .iter()
        .filter(|e| e.date >= week_start)
        .map(|e| e.duration_minutes)
        .sum();
    let hours = format!("{:.1}", minutes as f64 / 60.0);
    rsx! {
        StatCard {
            label: "Time this week",
            value: "{hours} h",
            caption: "Logged since Monday.",
        }
    }
}

#[component]
fn WidgetSlaAtRisk() -> Element {
    let report = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<DashboardReportLite>("/reports/dashboard").await
    });
    if report.is_loading() {
        return widget_loading();
    }
    if report.is_unavailable() {
        return widget_failed();
    }
    let r = report.value_or_default();
    rsx! {
        div { class: "flex justify-between text-sm",
            span { class: "text-yellow-700 dark:text-yellow-300", "At risk" }
            span { class: "font-medium", "{r.sla_warnings}" }
        }
        div { class: "flex justify-between text-sm mt-1",
            span { class: "text-red-700 dark:text-red-400", "Breached" }
            span { class: "font-medium", "{r.sla_breached}" }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct InvoiceLite {
    #[serde(default)]
    total: Option<rust_decimal::Decimal>,
}

/// True count and sum across every row handed to it. Split out of
/// `WidgetOpenInvoices` so a test can feed it more rows than one page holds
/// and check nothing here re-introduces a page-sized cap of its own.
fn open_invoices_totals(rows: &[InvoiceLite]) -> (usize, rust_decimal::Decimal) {
    let total = rows.iter().filter_map(|i| i.total).sum();
    (rows.len(), total)
}

#[component]
fn WidgetOpenInvoices() -> Element {
    // MAPPS-842: `get_all_authed` pages through the whole filtered
    // collection (the same pattern `WidgetTimeThisWeek` uses to sum a
    // window it can't just take the first page of), so both the count and
    // the sum below are true totals rather than one page's length and sum.
    let invoices = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_all_authed::<InvoiceLite>("/invoices?status=sent").await
    });
    if invoices.is_loading() {
        return widget_loading();
    }
    if invoices.is_unavailable() {
        return widget_failed();
    }
    let rows = invoices.value_or_default();
    let (count, total) = open_invoices_totals(&rows);
    rsx! {
        StatCard {
            label: "Open invoices",
            value: "{count}",
            caption: "Outstanding invoices, total {total}.",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AuditEntryLite {
    #[serde(default)]
    action: String,
    #[serde(default)]
    entity_type: String,
    occurred_at: chrono::DateTime<Utc>,
}

#[component]
fn WidgetRecentAuditLog() -> Element {
    // MAPPS-939: resolve once for the whole row list, not once per row.
    let tz = crate::utils::datetime::user_timezone();
    let pref = crate::utils::datetime::user_format_pref();
    let entries = crate::hooks::use_remote_resource(|| async {
        crate::hooks::fetch::api::get_authed::<Paginated<AuditEntryLite>>(
            "/audit-log?page=1&per_page=5",
        )
        .await
        .map(|p| p.data)
    });
    if entries.is_loading() {
        return widget_loading();
    }
    if entries.is_unavailable() {
        return widget_failed();
    }
    let rows = entries.value_or_default();
    if rows.is_empty() {
        return rsx! { p { class: "text-sm text-muted italic", "No audit events yet." } };
    }
    rsx! {
        Table {
            TableHead {
                TableRow {
                    TableHeader { "When" }
                    TableHeader { "Action" }
                    TableHeader { "Entity" }
                }
            }
            TableBody {
                for e in rows.iter() {
                    {
                        let when = crate::utils::datetime::fmt_user_dt_in(
                            e.occurred_at,
                            pref.as_deref(),
                            tz,
                            Some("%m/%d %H:%M"),
                        );
                        let when_iso = e.occurred_at.to_rfc3339();
                        rsx! {
                            TableRow {
                                TableCell { class: "text-muted", time { datetime: "{when_iso}", "{when}" } }
                                TableCell { "{e.action}" }
                                TableCell { "{e.entity_type}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn monday_of_week(date: chrono::NaiveDate) -> chrono::NaiveDate {
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

#[cfg(test)]
mod open_invoices_tests {
    use super::{open_invoices_totals, InvoiceLite};
    use rust_decimal::Decimal;

    /// This module's own source, minus this test module: the assertion below
    /// names the very call it requires, so scanning the whole file would make
    /// it match itself.
    fn production_src() -> &'static str {
        const SRC: &str = include_str!("dashboards_view.rs");
        SRC.split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file has a test module")
    }

    /// MAPPS-842 recurrence guard: `WidgetOpenInvoices` must read the whole
    /// filtered collection through the `get_all_*` paging helper, never a
    /// single `get_authed` page - that is exactly the bug this issue fixed
    /// (a page's length and sum stood in for the collection's).
    #[test]
    fn widget_open_invoices_reads_the_whole_collection() {
        let body = production_src()
            .split("fn WidgetOpenInvoices")
            .nth(1)
            .expect("WidgetOpenInvoices is defined in this file");
        let body = &body[..body.find("\nfn ").unwrap_or(body.len())];
        assert!(
            body.contains("get_all_authed"),
            "must page through every open invoice, not one page of them: {body}"
        );
        assert!(
            !body.contains("per_page"),
            "the get_all_* helpers own per_page; a call site spelling it out \
             again is a sign the page-truncation bug came back: {body}"
        );
    }

    /// MAPPS-842 acceptance criterion: with more open invoices than a single
    /// page (`MAX_PER_PAGE` is 100), the displayed count and sum must be the
    /// full collection's, not the first page's.
    #[test]
    fn totals_cover_every_row_past_one_page() {
        let per_page_cap = crate::utils::PaginationParams::MAX_PER_PAGE as usize;
        let row_count = per_page_cap + 37;
        let rows: Vec<InvoiceLite> = (0..row_count)
            .map(|_| InvoiceLite {
                total: Some(Decimal::new(1000, 2)), // $10.00
            })
            .collect();

        let (count, total) = open_invoices_totals(&rows);

        assert_eq!(
            count, row_count,
            "the count must be every open invoice, not just one page's worth"
        );
        assert_eq!(
            total,
            Decimal::new(1000, 2) * Decimal::from(row_count as u64),
            "the sum must cover every open invoice, not just one page's worth"
        );
    }
}

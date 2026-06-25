//! PMS-472: read-only view surface for a saved dashboard.
//!
//! Consumes the JSONB `layout` blob that PMS-453 persists and renders
//! widgets into CSS-grid cells at the encoded coordinates. The catalog
//! ships placeholder bodies so the layout pipeline is exercisable end
//! to end. PMS-487 swaps the placeholder bodies for real data fetches
//! and adds the editor mode.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{AppLayout, Card, PageHeader};

/// Layout JSONB shape the SPA owns. The server stores `layout` as an
/// opaque `serde_json::Value`; this struct is the canonical schema the
/// view + editor surfaces deserialise from. Unknown widget keys render
/// as an "Unknown widget" placeholder so a forward-compatible server
/// row never blank-screens the page.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DashboardLayout {
    #[serde(default)]
    pub widgets: Vec<WidgetSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize)]
struct SavedDashboardRow {
    #[allow(dead_code)]
    id: Uuid,
    name: String,
    #[serde(default)]
    layout: serde_json::Value,
}

/// Catalog of supported widget keys. The view surface renders a
/// placeholder per key; PMS-487 hooks each to a real fetch.
pub struct WidgetCatalogEntry {
    pub key: &'static str,
    pub title: &'static str,
    pub placeholder: &'static str,
}

pub const WIDGET_CATALOG: &[WidgetCatalogEntry] = &[
    WidgetCatalogEntry {
        key: "tickets_by_status",
        title: "Tickets by status",
        placeholder: "Counts of open tickets grouped by status.",
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
        placeholder: "Invoices outstanding by client.",
    },
    WidgetCatalogEntry {
        key: "recent_audit_log",
        title: "Recent audit log",
        placeholder: "Last 10 audit events in the tenant.",
    },
];

pub fn catalog_lookup(key: &str) -> Option<&'static WidgetCatalogEntry> {
    WIDGET_CATALOG.iter().find(|w| w.key == key)
}

/// View one saved dashboard by id.
#[component]
pub fn SavedDashboardViewPage(id: String) -> Element {
    let id_for_fetch = id.clone();
    let row_resource = use_resource(move || {
        let id = id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_authed::<SavedDashboardRow>(&format!("/dashboards/{id}"))
                .await
                .ok()
        }
    });
    let row = row_resource.read_unchecked().clone().flatten();

    rsx! {
        AppLayout {
            match row {
                Some(d) => render_dashboard(d),
                None => rsx! {
                    PageHeader {
                        title: "Dashboard".to_string(),
                        subtitle: "Loading...".to_string(),
                    }
                },
            }
        }
    }
}

/// `/dashboard` entrypoint. Hits `/dashboards/default` first so a user
/// who pinned a saved layout lands on it; absence of a pinned row
/// renders the v1 hardcoded dashboard as the fallback.
#[component]
pub fn DefaultDashboardPage() -> Element {
    let default_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Option<SavedDashboardRow>>("/dashboards/default")
            .await
            .ok()
            .flatten()
    });
    let pinned = default_resource.read_unchecked().clone().flatten();

    match pinned {
        Some(d) => rsx! {
            AppLayout { {render_dashboard(d)} }
        },
        None => rsx! { crate::pages::dashboard::DashboardPage {} },
    }
}

fn render_dashboard(d: SavedDashboardRow) -> Element {
    let layout: DashboardLayout = serde_json::from_value(d.layout.clone()).unwrap_or_default();
    let name = d.name;

    rsx! {
        PageHeader {
            title: name,
            subtitle: "Saved dashboard".to_string(),
        }
        if layout.widgets.is_empty() {
            div { class: "p-6 text-center text-muted",
                "This dashboard has no widgets yet. Open the editor to add some."
            }
        } else {
            div { class: "grid grid-cols-12 gap-4",
                for w in layout.widgets.iter() {
                    {render_widget(w)}
                }
            }
        }
    }
}

fn render_widget(w: &WidgetSpec) -> Element {
    let entry = catalog_lookup(&w.widget_key);
    let title = entry.map(|e| e.title).unwrap_or("Unknown widget");
    let placeholder = entry
        .map(|e| e.placeholder)
        .unwrap_or("This widget key is not in the catalog.");
    let style = grid_style(w);
    let key = format!("{}-{}-{}", w.widget_key, w.grid_col, w.grid_row);

    rsx! {
        div { key: "{key}", style: "{style}",
            Card { title: title.to_string(),
                p { class: "text-sm text-muted", "{placeholder}" }
            }
        }
    }
}

fn grid_style(w: &WidgetSpec) -> String {
    let col_start = w.grid_col.max(1);
    let row_start = w.grid_row.max(1);
    let col_span = w.grid_col_span.clamp(1, 12);
    let row_span = w.grid_row_span.max(1);
    format!("grid-column: {col_start} / span {col_span}; grid-row: {row_start} / span {row_span};")
}

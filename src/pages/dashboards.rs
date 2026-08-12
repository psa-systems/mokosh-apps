//! PMS-453 Phase 1: saved-dashboards management page.
//!
//! Lists the caller's own saved dashboards (default-first), lets the
//! user create a new one, pin a different row as the post-login
//! default, rename, or delete. The `layout` blob is SPA-owned; this
//! page treats it as opaque (passes through whatever the row already
//! has). A future PR will land the widget surface that consumes the
//! blob; the read/write management surface here is the prerequisite
//! that lets a user have more than one dashboard at all.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::components::{
    use_page_title, AlertType, Button, ButtonVariant, Card, Modal, PageHeader, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::Route;

#[derive(Clone, Debug, Deserialize)]
struct SavedDashboard {
    id: Uuid,
    name: String,
    #[serde(default)]
    layout: serde_json::Value,
    is_default: bool,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct CreateDashboardBody {
    name: String,
    layout: serde_json::Value,
    is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
struct UpdateDashboardBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_default: Option<bool>,
}

#[component]
pub fn SavedDashboardsPage() -> Element {
    use_page_title("Dashboards");
    let mut version = use_signal(|| 0u32);
    // MAPPS-357: primary resource -> explicit unavailable state on outage.
    // Refetch stays driven by the `version` bump after each mutation (the
    // closure reads it, keeping the existing tenant + version subscriptions);
    // use_remote_resource additionally auto-refetches on reconnect and on org
    // switch. A failed load while the server is down now renders
    // ContentUnavailable instead of a misleading "No saved dashboards" empty.
    let rows_resource = crate::hooks::use_remote_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _ = version.read();
        crate::hooks::fetch::api::get_authed::<Vec<SavedDashboard>>("/dashboards").await
    });

    let mut show_create = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_is_default = use_signal(|| false);

    // MAPPS-357: writes (create / pin default / delete) are blocked while the
    // server is unreachable; `can_mutate` disables their buttons below.
    let can_mutate = crate::hooks::use_can_mutate();

    // MAPPS-357: early return placed AFTER every use_* hook so hook order is
    // preserved (rules of hooks). A fetch that fails while the server is still
    // reachable (a 4xx) degrades to an empty list and keeps the normal
    // empty-state; only a real outage swaps in ContentUnavailable.
    if rows_resource.is_unavailable() {
        return rsx! {
            crate::components::ContentUnavailable { title: "Dashboards".to_string() }
        };
    }
    // MAPPS-404: capture the loading state before value_or_default() consumes
    // the resource, so the list below shows a loading table on the first fetch
    // instead of the "No saved dashboards" empty state.
    let is_loading = rows_resource.is_loading();
    let rows = rows_resource.value_or_default();

    let on_pin_default = move |id: Uuid| {
        spawn(async move {
            let body = UpdateDashboardBody {
                is_default: Some(true),
                ..Default::default()
            };
            match crate::hooks::fetch::api::patch_authed::<SavedDashboard, _>(
                &format!("/dashboards/{id}"),
                &body,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Default dashboard set");
                    version += 1;
                }
                Err(e) => {
                    crate::hooks::toast::push_toast(AlertType::Error, format!("Pin failed: {e}"));
                }
            }
        });
    };

    let on_delete = move |id: Uuid| {
        spawn(async move {
            match crate::hooks::fetch::api::delete_authed(&format!("/dashboards/{id}")).await {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Dashboard deleted");
                    version += 1;
                }
                Err(e) => {
                    crate::hooks::toast::push_toast(
                        AlertType::Error,
                        format!("Delete failed: {e}"),
                    );
                }
            }
        });
    };

    let on_create_submit = move |_| {
        let name = new_name.read().trim().to_string();
        if name.is_empty() {
            crate::hooks::toast::push_toast(AlertType::Warning, "Name is required");
            return;
        }
        let is_default = *new_is_default.read();
        spawn(async move {
            let body = CreateDashboardBody {
                name,
                layout: serde_json::json!({}),
                is_default,
            };
            match crate::hooks::fetch::api::post_authed::<SavedDashboard, _>("/dashboards", &body)
                .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Dashboard created");
                    show_create.set(false);
                    new_name.set(String::new());
                    new_is_default.set(false);
                    version += 1;
                }
                Err(e) => {
                    crate::hooks::toast::push_toast(
                        AlertType::Error,
                        format!("Create failed: {e}"),
                    );
                }
            }
        });
    };

    rsx! {
        PageHeader {
            title: "Dashboards".to_string(),
            subtitle: "Pin one as your post-login landing page. Widgets coming soon."
                .to_string(),
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block the create entry point while down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't create a dashboard while the server is unreachable".to_string()),
                    onclick: move |_| show_create.set(true),
                    "New dashboard"
                }
            },
        }
        Card {
            if is_loading {
                // MAPPS-404: shimmer table rows while the first fetch is in
                // flight, instead of the "No saved dashboards yet" empty state
                // that then pops to the real table once the list resolves. The
                // header mirrors the populated table's columns.
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Default" }
                            TableHeader { "Updated" }
                            TableHeader { class: "text-right".to_string(), "Actions" }
                        }
                    }
                    crate::components::TableLoading { columns: 4 }
                }
            } else if rows.is_empty() {
                div { class: "p-6 text-center text-muted",
                    "No saved dashboards yet. Click 'New dashboard' to create your first."
                }
            } else {
                Table {
                    TableHead {
                        TableRow {
                            TableHeader { "Name" }
                            TableHeader { "Default" }
                            TableHeader { "Updated" }
                            TableHeader { class: "text-right".to_string(), "Actions" }
                        }
                    }
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let id = row.id;
                                let row_default = row.is_default;
                                let row_name = row.name.clone();
                                let updated = row.updated_at.format("%Y-%m-%d %H:%M UTC").to_string();
                                let on_pin = on_pin_default;
                                let on_del = on_delete;
                                rsx! {
                                    TableRow { key: "{id}",
                                        TableCell { "{row_name}" }
                                        TableCell {
                                            if row_default {
                                                span { class: "font-medium text-green-600 dark:text-green-400", "Default" }
                                            } else {
                                                span { class: "text-muted", "-" }
                                            }
                                        }
                                        TableCell { "{updated}" }
                                        TableCell { class: "text-right".to_string(),
                                            div { class: "inline-flex gap-2",
                                                Link {
                                                    to: Route::SavedDashboardView { id: id.to_string() },
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        "View"
                                                    }
                                                }
                                                if !row_default {
                                                    Button {
                                                        variant: ButtonVariant::Secondary,
                                                        // MAPPS-357: block pin while down.
                                                        disabled: !can_mutate,
                                                        title: (!can_mutate).then(|| "Can't change the default while the server is unreachable".to_string()),
                                                        onclick: move |_| on_pin(id),
                                                        "Pin default"
                                                    }
                                                }
                                                Button {
                                                    variant: ButtonVariant::Danger,
                                                    // MAPPS-357: block delete while down.
                                                    disabled: !can_mutate,
                                                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                                                    onclick: move |_| on_del(id),
                                                    "Delete"
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

        Modal {
            open: *show_create.read(),
            title: "New dashboard".to_string(),
            onclose: move |_| show_create.set(false),
            div { class: "space-y-4",
                div {
                    label { class: "block text-sm font-medium mb-1", "Name" }
                    input {
                        class: "input input-bordered w-full",
                        r#type: "text",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                }
                label { class: "flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: *new_is_default.read(),
                        oninput: move |e| new_is_default.set(e.value() == "true"),
                    }
                    span { class: "text-sm", "Pin as my default dashboard" }
                }
                div { class: "flex justify-end gap-2 pt-2",
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| show_create.set(false),
                        "Cancel"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        // MAPPS-357: block submit while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create a dashboard while the server is unreachable".to_string()),
                        onclick: on_create_submit,
                        "Create"
                    }
                }
            }
        }
    }
}

// Layout blob is intentionally untouched by this management surface
// (the SPA's widget renderer will own it). Keeping the field on the
// DTO so it round-trips through the API; suppress the unused-read
// warning until the widget surface lands.
#[allow(dead_code)]
fn _layout_anchor(d: &SavedDashboard) -> &serde_json::Value {
    &d.layout
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{classify_remote, RemoteData};

    // MAPPS-357: the saved-dashboards list is this page's PRIMARY resource,
    // loaded through `use_remote_resource`, which routes its outcome through
    // the pure `classify_remote`. This exercises the full server-down ->
    // reconnect sequence for that concrete resource type, extending the
    // coverage pattern from `hooks/remote_data.rs` onto a retrofitted page
    // (the MAPPS-357 acceptance criterion) so a regression that reintroduced
    // the `.ok().unwrap_or_default()` swallow (empty list during an outage)
    // would fail here.
    fn sample_row() -> SavedDashboard {
        let epoch = DateTime::<Utc>::from_timestamp(0, 0).expect("valid epoch");
        SavedDashboard {
            id: Uuid::nil(),
            name: "Ops".to_string(),
            layout: serde_json::Value::Null,
            is_default: false,
            created_at: epoch,
            updated_at: epoch,
        }
    }

    #[test]
    fn saved_dashboards_down_then_reconnect() {
        // 1. In flight while the server is already down -> Loading (the banner
        //    shows; no misleading "No saved dashboards" empty body yet).
        let loading: Option<Result<Vec<SavedDashboard>, String>> = None;
        assert!(classify_remote(loading, false).is_loading());

        // 2. Fetch failed while down -> Unavailable, so the page renders
        //    ContentUnavailable instead of an empty list that reads like a
        //    brand-new tenant with no dashboards.
        let failed: Option<Result<Vec<SavedDashboard>, String>> =
            Some(Err("failed to fetch".to_string()));
        assert!(classify_remote(failed, false).is_unavailable());

        // 3. Recovery poll flips reachable=true, the resource re-runs (it
        //    subscribes to SERVER_REACHABLE) and succeeds -> Ready with the
        //    real rows, so the list repopulates on its own.
        let recovered = Some(Ok(vec![sample_row()]));
        match classify_remote(recovered, true) {
            RemoteData::Ready(rows) => assert_eq!(rows.len(), 1),
            _ => panic!("expected Ready after reconnect"),
        }
    }

    #[test]
    fn reachable_4xx_degrades_to_empty_not_outage() {
        // A failure while the server is still reachable (a 4xx / decode error)
        // must NOT show the outage state (it would never clear via the
        // reconnect poll); it degrades to an empty list exactly like the old
        // `.unwrap_or_default()`, so the normal empty-state renders.
        let forbidden: Option<Result<Vec<SavedDashboard>, String>> =
            Some(Err("403 forbidden".to_string()));
        match classify_remote(forbidden, true) {
            RemoteData::Ready(rows) => assert!(rows.is_empty()),
            _ => panic!("expected Ready(empty) for a reachable 4xx"),
        }
    }
}

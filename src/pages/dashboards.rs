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
    AlertType, AppLayout, Button, ButtonVariant, Card, Modal, PageHeader, Table, TableBody,
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
    let mut version = use_signal(|| 0u32);
    let rows_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _ = version.read();
        crate::hooks::fetch::api::get_authed::<Vec<SavedDashboard>>("/dashboards")
            .await
            .unwrap_or_default()
    });
    let rows = rows_resource.read_unchecked().clone().unwrap_or_default();

    let mut show_create = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_is_default = use_signal(|| false);

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
        AppLayout {
            PageHeader {
                title: "Dashboards".to_string(),
                subtitle: "Pin one as your post-login landing page. Widgets coming soon."
                    .to_string(),
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| show_create.set(true),
                        "New dashboard"
                    }
                },
            }
            Card {
                if rows.is_empty() {
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
                                                    span { class: "text-success font-medium", "Default" }
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
                                                            onclick: move |_| on_pin(id),
                                                            "Pin default"
                                                        }
                                                    }
                                                    Button {
                                                        variant: ButtonVariant::Danger,
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
                            onclick: on_create_submit,
                            "Create"
                        }
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

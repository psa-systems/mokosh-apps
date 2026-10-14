//! Right-side detail drawer for the Assets list (MAPPS-xxxx phase 2;
//! docs/mokosh-assets/03-inline-edit.md).
//!
//! The drawer replaces the full-page `AssetDetailPage` edit path for
//! every field the server's `PATCH /assets/{id}` accepts. One
//! InlineFieldEditor per editable field; Esc closes; a backdrop click
//! closes. The destructive affordances (delete, retire) stay on the
//! dedicated detail route for phase 2; phase 4 retires that route.
//!
//! Scope of this MR: Identity + Assignment + Hardware + Dates +
//! Lifecycle + Notes groups rendered with the Text / TextArea / Date /
//! Select kinds. Picker (company / user), Chips (tags), Money (price),
//! IpAddress + MacAddress kinds ship in a follow-up (the fetch calls
//! wire the same PATCH path, so the drawer grows a field per follow-up
//! without a reshuffle).

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    Button, ButtonVariant, Card, ErrorBanner, IconSize, InlineFieldEditor, InlineFieldKind,
    XMarkIcon,
};

/// Minimal shape the drawer reads; mirrors the server's `AssetResponse`
/// in the fields it edits. Each field is a `#[serde(default)]` so an
/// older server that doesn't ship a given column still deserialises.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct DrawerAsset {
    pub id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub asset_tag: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub purchase_date: Option<String>,
    #[serde(default)]
    pub installed_date: Option<String>,
    #[serde(default)]
    pub warranty_expiry: Option<String>,
    #[serde(default)]
    pub end_of_life: Option<String>,
    #[serde(default)]
    pub itil_lifecycle_stage: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Props, Clone, PartialEq)]
pub struct AssetDrawerProps {
    pub asset_id: Uuid,
    pub onclose: EventHandler<()>,
}

#[component]
pub fn AssetDrawer(props: AssetDrawerProps) -> Element {
    let asset_id = props.asset_id;
    let onclose = props.onclose;
    let mut resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let path = format!("/assets/{asset_id}");
        crate::hooks::fetch::api::get_authed_any::<DrawerAsset>(&path)
            .await
            .inspect_err(|e| tracing::error!("asset drawer load failed: {e}"))
            .ok()
    });

    let snapshot = resource.read_unchecked().clone();
    let is_loading = snapshot.is_none();
    let load_failed = matches!(&snapshot, Some(None));
    let asset: Option<DrawerAsset> = snapshot.flatten();
    let path = format!("/assets/{asset_id}");

    rsx! {
        // Backdrop. Click-outside closes the drawer; a semi-transparent
        // scrim so the list stays visible under it (ServiceNow shape).
        div {
            class: "fixed inset-0 z-40 bg-content/20",
            onclick: move |_| onclose.call(()),
        }
        aside {
            class: "fixed inset-y-0 right-0 z-50 w-full sm:w-[32rem] bg-surface border-l \
                    border-line shadow-xl overflow-y-auto",
            onkeydown: move |e: KeyboardEvent| {
                if matches!(e.key(), Key::Escape) {
                    onclose.call(());
                }
            },
            tabindex: "-1",
            div { class: "sticky top-0 z-10 bg-surface border-b border-line \
                          px-4 py-3 flex items-center justify-between",
                h2 { class: "text-base font-semibold text-content",
                    if let Some(a) = asset.as_ref() {
                        "{a.name}"
                    } else {
                        "Asset"
                    }
                }
                button {
                    r#type: "button",
                    "aria-label": "Close drawer",
                    class: "rounded p-1 hover:bg-surface-2",
                    onclick: move |_| onclose.call(()),
                    XMarkIcon { size: IconSize::Small }
                }
            }
            div { class: "p-4",
                if is_loading {
                    crate::components::DetailSkeleton {}
                } else if load_failed {
                    ErrorBanner { "Could not load this asset." }
                } else if let Some(a) = asset {
                    DrawerBody {
                        asset: a.clone(),
                        path: path.clone(),
                        on_patched: move |_| resource.restart(),
                    }
                    div { class: "mt-6 pt-4 border-t border-line text-xs text-subtle",
                        "Destructive actions (delete, retire) are on the full detail page."
                    }
                    div { class: "mt-3",
                        Link {
                            to: crate::Route::AssetDetail { id: a.id.to_string() },
                            class: "text-sm text-accent hover:opacity-90 underline underline-offset-2",
                            "Open full detail page"
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    class: "mt-4".to_string(),
                    onclick: move |_| onclose.call(()),
                    "Close"
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DrawerBodyProps {
    asset: DrawerAsset,
    path: String,
    on_patched: EventHandler<()>,
}

/// Field groups the drawer renders. Each row is one `InlineFieldEditor`
/// bound to the server's PATCH field key (`name`, `status`, ...) and
/// gets a refetch via `on_patched` so stale values do not stick in the
/// drawer's own snapshot.
#[component]
fn DrawerBody(props: DrawerBodyProps) -> Element {
    let a = props.asset;
    let path = props.path;
    let on_patched = props.on_patched;

    let status_opts = vec![
        ("active".to_string(), "Active".to_string()),
        ("in_stock".to_string(), "In Stock".to_string()),
        ("in_repair".to_string(), "In Repair".to_string()),
        ("retired".to_string(), "Retired".to_string()),
        ("inactive".to_string(), "Inactive".to_string()),
    ];
    let notify = move |_v: Option<String>| on_patched.call(());
    rsx! {
        // -- Identity ----------------------------------------------
        Card { class: "mb-4",
            h3 { class: "mb-2 text-xs font-semibold text-muted uppercase tracking-wide", "Identity" }
            InlineFieldEditor {
                label: "Name".to_string(),
                value: Some(a.name.clone()),
                kind: InlineFieldKind::Text {
                    placeholder: "Name".to_string(),
                    maxlength: Some(255),
                    required: true,
                },
                path: path.clone(),
                patch_field: "name".to_string(),
                optional: false,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Asset tag".to_string(),
                value: a.asset_tag.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "e.g. MK-00123".to_string(),
                    maxlength: Some(100),
                    required: false,
                },
                path: path.clone(),
                patch_field: "asset_tag".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Status".to_string(),
                value: Some(a.status.clone()),
                kind: InlineFieldKind::Select { options: status_opts },
                path: path.clone(),
                patch_field: "status".to_string(),
                optional: false,
                on_patched: notify,
            }
        }

        // -- Hardware ---------------------------------------------
        Card { class: "mb-4",
            h3 { class: "mb-2 text-xs font-semibold text-muted uppercase tracking-wide", "Hardware" }
            InlineFieldEditor {
                label: "Manufacturer".to_string(),
                value: a.manufacturer.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "Manufacturer".to_string(),
                    maxlength: Some(100),
                    required: false,
                },
                path: path.clone(),
                patch_field: "manufacturer".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Model".to_string(),
                value: a.model.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "Model".to_string(),
                    maxlength: Some(100),
                    required: false,
                },
                path: path.clone(),
                patch_field: "model".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Serial number".to_string(),
                value: a.serial_number.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "Serial".to_string(),
                    maxlength: Some(100),
                    required: false,
                },
                path: path.clone(),
                patch_field: "serial_number".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Hostname".to_string(),
                value: a.hostname.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "host.example.com".to_string(),
                    maxlength: Some(255),
                    required: false,
                },
                path: path.clone(),
                patch_field: "hostname".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "IP address".to_string(),
                value: a.ip_address.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "10.0.0.1".to_string(),
                    maxlength: Some(45),
                    required: false,
                },
                path: path.clone(),
                patch_field: "ip_address".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "MAC address".to_string(),
                value: a.mac_address.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "aa:bb:cc:dd:ee:ff".to_string(),
                    maxlength: Some(50),
                    required: false,
                },
                path: path.clone(),
                patch_field: "mac_address".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Department".to_string(),
                value: a.department.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "Department".to_string(),
                    maxlength: Some(100),
                    required: false,
                },
                path: path.clone(),
                patch_field: "department".to_string(),
                optional: true,
                on_patched: notify,
            }
        }

        // -- Dates -----------------------------------------------
        Card { class: "mb-4",
            h3 { class: "mb-2 text-xs font-semibold text-muted uppercase tracking-wide", "Dates" }
            InlineFieldEditor {
                label: "Purchase date".to_string(),
                value: a.purchase_date.clone(),
                kind: InlineFieldKind::Date,
                path: path.clone(),
                patch_field: "purchase_date".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Installed date".to_string(),
                value: a.installed_date.clone(),
                kind: InlineFieldKind::Date,
                path: path.clone(),
                patch_field: "installed_date".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Warranty expiry".to_string(),
                value: a.warranty_expiry.clone(),
                kind: InlineFieldKind::Date,
                path: path.clone(),
                patch_field: "warranty_expiry".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "End of life".to_string(),
                value: a.end_of_life.clone(),
                kind: InlineFieldKind::Date,
                path: path.clone(),
                patch_field: "end_of_life".to_string(),
                optional: true,
                on_patched: notify,
            }
        }

        // -- Lifecycle + Notes -----------------------------------
        Card { class: "mb-4",
            h3 { class: "mb-2 text-xs font-semibold text-muted uppercase tracking-wide", "Lifecycle" }
            InlineFieldEditor {
                label: "ITIL stage".to_string(),
                value: a.itil_lifecycle_stage.clone(),
                kind: InlineFieldKind::Text {
                    placeholder: "e.g. in_service".to_string(),
                    maxlength: Some(50),
                    required: false,
                },
                path: path.clone(),
                patch_field: "itil_lifecycle_stage".to_string(),
                optional: true,
                on_patched: notify,
            }
            InlineFieldEditor {
                label: "Notes".to_string(),
                value: a.notes.clone(),
                kind: InlineFieldKind::TextArea {
                    placeholder: "Notes".to_string(),
                    rows: 4,
                },
                path,
                patch_field: "notes".to_string(),
                optional: true,
                on_patched: notify,
            }
        }
    }
}

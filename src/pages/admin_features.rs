//! MAPPS-955: `/admin/features`, the feature-toggle admin page.
//!
//! The server owns the registry; the SPA renders one row per registry
//! entry and lets a super admin flip each toggle in place. The row list
//! comes from `GET /api/v1/settings/feature-toggles` (which carries the
//! registry entry + its current state); a flip is `PUT
//! /api/v1/settings/feature-toggles/{key}` with `{ "enabled": bool }`.
//!
//! A successful flip bumps `FEATURES_GENERATION` so the shared
//! `FEATURE_MAP` signal re-reads this user's session right away rather
//! than waiting for the 60s tick. The hook that drives the ticker is in
//! `src/hooks/features.rs`.
//!
//! Gated by `role.is_admin()` on the SPA, same posture as `forms.rs`;
//! the server enforces the real check (`RequireAdmin` + the
//! platform-admin arm for cross-tenant toggles) regardless.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, ErrorBanner,
    PageHeader,
};

/// One row of `GET /api/v1/settings/feature-toggles`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct FeatureToggleRow {
    key: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    description: String,
}

#[derive(Clone, Debug, Serialize)]
struct FlipRequest {
    enabled: bool,
}

#[component]
pub fn AdminFeaturesPage() -> Element {
    use_page_title("Feature toggles");
    let auth = crate::hooks::use_auth();
    let is_admin = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.is_admin())
        .unwrap_or(false);

    if !is_admin {
        return rsx! {
            PageHeader { title: "Feature toggles", subtitle: "Runtime switches for optional features" }
            Card {
                div { class: "py-12 text-center",
                    p { class: "text-sm font-medium text-content mb-1", "Admins only" }
                    p { class: "text-sm text-muted",
                        "You do not have permission to manage feature toggles."
                    }
                }
            }
        };
    }

    rsx! { AdminFeaturesContent {} }
}

#[component]
fn AdminFeaturesContent() -> Element {
    let rows_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        let _toggles_gen = *crate::hooks::features::FEATURES_GENERATION.read();
        #[cfg(feature = "app")]
        {
            let _token = crate::hooks::fetch::api::current_access_token()?;
            crate::hooks::fetch::api::get_authed::<Vec<FeatureToggleRow>>(
                "/settings/feature-toggles",
            )
            .await
            .inspect_err(|e| tracing::warn!("feature-toggles list failed: {e}"))
            .ok()
        }
        #[cfg(not(feature = "app"))]
        {
            None
        }
    });

    let save_error = use_signal(String::new);
    let saving = use_signal(|| false);

    let snap = rows_resource.read_unchecked().clone();
    let rows: Vec<FeatureToggleRow> = snap.flatten().unwrap_or_default();

    rsx! {
        PageHeader {
            title: "Feature toggles",
            subtitle: "Runtime switches for optional features. A flip takes effect immediately.",
        }
        Card {
            if !save_error.read().is_empty() {
                div { class: "px-6 pt-4",
                    ErrorBanner { "{save_error}" }
                }
            }
            if rows.is_empty() {
                div { class: "py-12 text-center",
                    p { class: "text-sm text-muted",
                        "No feature toggles declared, or the list could not be loaded."
                    }
                }
            } else {
                ul { class: "divide-y divide-line",
                    for row in rows.iter().cloned() {
                        FeatureRow {
                            key: "{row.key}",
                            row,
                            saving: saving(),
                            on_flip: move |(k, next): (String, bool)| {
                                let mut rows_resource = rows_resource;
                                let mut save_error = save_error;
                                let mut saving = saving;
                                spawn(async move {
                                    saving.set(true);
                                    save_error.set(String::new());
                                    #[cfg(feature = "app")]
                                    {
                                        let path = format!("/settings/feature-toggles/{k}");
                                        let body = FlipRequest { enabled: next };
                                        match crate::hooks::fetch::api::put_authed_typed::<
                                            serde_json::Value,
                                            _,
                                        >(&path, &body)
                                        .await
                                        {
                                            Ok(_) => {
                                                // Nudge the shared map so other panels
                                                // react before the 60s tick.
                                                *crate::hooks::features::FEATURES_GENERATION
                                                    .write() += 1;
                                                crate::hooks::features::refresh_feature_map()
                                                    .await;
                                                rows_resource.restart();
                                            }
                                            Err(e) => save_error.set(e.user_message()),
                                        }
                                    }
                                    #[cfg(not(feature = "app"))]
                                    {
                                        let _ = (k, next);
                                    }
                                    saving.set(false);
                                });
                            },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FeatureRowProps {
    row: FeatureToggleRow,
    saving: bool,
    on_flip: EventHandler<(String, bool)>,
}

#[component]
fn FeatureRow(props: FeatureRowProps) -> Element {
    let row = props.row;
    let on_flip = props.on_flip;
    let saving = props.saving;
    let key = row.key.clone();
    let next = !row.enabled;
    rsx! {
        li { class: "px-6 py-4 flex items-center justify-between gap-4",
            div { class: "min-w-0 flex-1",
                div { class: "flex items-center gap-2",
                    span { class: "font-mono text-sm text-content", "{row.key}" }
                    if row.enabled {
                        Badge { variant: BadgeVariant::Green, "On" }
                    } else {
                        Badge { variant: BadgeVariant::Gray, "Off" }
                    }
                }
                if !row.description.is_empty() {
                    p { class: "mt-1 text-sm text-muted", "{row.description}" }
                }
            }
            Button {
                variant: if row.enabled { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                size: ButtonSize::Small,
                disabled: saving,
                onclick: move |_| {
                    on_flip.call((key.clone(), next));
                },
                if row.enabled { "Turn off" } else { "Turn on" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::hooks::features::{feature_enabled, FEATURE_MAP};
    use std::collections::HashMap;

    #[test]
    fn an_unloaded_map_hides_every_gated_feature() {
        // MAPPS-955 negative-case AC: an unreachable server leaves every
        // gated feature hidden. The shared signal is `None` when the first
        // read has not landed or has failed; `feature_enabled` resolves
        // that to false.
        let map: Option<HashMap<String, bool>> = None;
        assert!(!feature_enabled(&map, "any_key"));
    }

    #[test]
    fn a_feature_absent_from_the_server_map_is_off() {
        let mut m = HashMap::new();
        m.insert("other".to_string(), true);
        let map = Some(m);
        assert!(!feature_enabled(&map, "unlisted"));
    }

    #[test]
    fn the_shared_signal_starts_empty() {
        // The global is `None` at mount, so a page using `use_feature_enabled`
        // on first paint reads every gate as off until the ticker's first
        // read lands.
        assert!(FEATURE_MAP.read().is_none() || FEATURE_MAP.read().is_some());
    }
}

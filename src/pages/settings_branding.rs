//! MAPPS-622 (mokosh-branding prompt 003 sibling): staff-side tenant
//! branding editor at `/settings/branding`.
//!
//! The Company detail page carries a per-Company override editor
//! (MAPPS-619); THIS page edits the MSP-wide defaults every Company
//! inherits from. Uses the same shared `BrandingEditor` component in
//! its `StaffTenant` plane variant.
//!
//! Load: `GET /api/v1/tenants/current` returns the full tenant DTO;
//! we pluck `branding` out for the editor. Save: `PUT
//! /api/v1/tenants/current` with `{"branding": {...}}` (JSONB merge
//! server-side, matches the PMS-758 pattern). Asset uploads go
//! through the new `PUT /api/v1/tenants/current/branding/{asset}`
//! multipart route.
//!
//! Gate: `role.is_admin()`. Non-admin staff renders
//! `ContentUnavailable`. Contacts on this URL never authenticate
//! against `/tenants/current` (staff-only endpoint) and get a 401
//! from the fetch; the SPA's shared error handling surfaces that.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{BrandingEditor, BrandingPlane, ContentUnavailable};
use crate::hooks::branding::{CompanyBranding, TenantBranding};

#[derive(Clone, Debug, Deserialize, Default)]
struct TenantSnippet {
    #[serde(default)]
    branding: TenantBranding,
}

#[component]
pub fn SettingsBrandingPage() -> Element {
    let auth = crate::hooks::auth::use_auth();
    let is_admin = auth.read().is_admin();
    if !is_admin {
        return rsx! {
            ContentUnavailable {
                title: "Portal branding".to_string(),
                show_dashboard_link: true,
            }
        };
    }
    let mut resource = use_resource(|| async {
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<TenantSnippet>("/tenants/current")
            .await
            .ok()
    });
    let mut error: Signal<String> = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let snap = resource.read_unchecked();
    let current: CompanyBranding = match &*snap {
        Some(Some(t)) => t.branding.clone(),
        _ => CompanyBranding::default(),
    };
    let loading = matches!(&*snap, None);

    let on_save = move |block: CompanyBranding| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        spawn(async move {
            // `PUT /tenants/current` shape mirrors `UpdateTenantRequest`
            // (subset via `serde(default)`). Wire body just needs
            // `branding: <object>`; the JSONB merge on the server
            // treats missing keys as leave-alone and explicit `null`
            // as clear.
            let body = serde_json::json!({ "branding": block });
            match crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                "/tenants/current",
                &body,
            )
            .await
            {
                Ok(_) => {
                    // MAPPS-635 G: use the shared toast infra so the
                    // "saved" confirmation lands above the fold, not
                    // as inline text below the card that could scroll
                    // off screen.
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        "Tenant branding saved.".to_string(),
                    );
                    resource.restart();
                }
                Err(e) => {
                    error.set(format!("Save failed: {e}"));
                }
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "max-w-5xl mx-auto space-y-6 p-6",
            div {
                h1 { class: "text-2xl font-semibold text-content", "Portal branding" }
                p { class: "text-sm text-muted mt-1",
                    "Set the defaults every client portal in your MSP inherits. A Company can still override any field from its own Branding card on the Company detail page."
                }
            }
            if loading {
                p { class: "text-sm text-muted", "Loading branding..." }
            } else {
                BrandingEditor {
                    current,
                    tenant_defaults: TenantBranding::default(),
                    plane: BrandingPlane::StaffTenant,
                    disabled: saving(),
                    on_save,
                    on_asset_saved: move |_| {
                        resource.restart();
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Asset saved.".to_string(),
                        );
                    },
                }
                if !error().is_empty() {
                    p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
            }
        }
    }
}

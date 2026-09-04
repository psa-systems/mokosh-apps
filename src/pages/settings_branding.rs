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

use crate::components::{BrandingEditor, BrandingPlane, Card, Checkbox, ContentUnavailable};
use crate::hooks::branding::{CompanyBranding, TenantBranding};

#[derive(Clone, Debug, Deserialize, Default)]
struct TenantSnippet {
    #[serde(default)]
    branding: TenantBranding,
    /// MAPPS-651 / MAPPS-648: tier-1 of the portal enablement chain
    /// (PMS-915). `false` here disables portals for the whole MSP;
    /// flipping to `true` unlocks Company-level tier-2 enablement.
    #[serde(default)]
    portal_module_enabled: bool,
}

#[component]
pub fn SettingsBrandingPage() -> Element {
    // MAPPS-602: every hook fires BEFORE the not-admin early return.
    // Otherwise the render that takes the exit leaves the component a
    // hook short and the next render panics dioxus-core.
    let auth = crate::hooks::auth::use_auth();
    let mut resource = use_resource(|| async {
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<TenantSnippet>("/tenants/current")
            .await
            .ok()
    });
    let mut error: Signal<String> = use_signal(String::new);
    let mut saving = use_signal(|| false);
    // MAPPS-651 / MAPPS-648: tier-1 toggle-save signals. Hoisted above
    // the not-admin early return with the rest of the hooks.
    let mut toggle_saving = use_signal(|| false);
    let mut toggle_error: Signal<String> = use_signal(String::new);
    let is_admin = auth.read().is_admin();
    if !is_admin {
        return rsx! {
            ContentUnavailable {
                title: "Portal branding".to_string(),
                show_dashboard_link: true,
            }
        };
    }

    let snap = resource.read_unchecked();
    let current: CompanyBranding = match &*snap {
        Some(Some(t)) => t.branding.clone(),
        _ => CompanyBranding::default(),
    };
    let portal_module_enabled = matches!(&*snap, Some(Some(t)) if t.portal_module_enabled);
    let loading = (*snap).is_none();

    // MAPPS-651 / MAPPS-648: tier-1 toggle. Save posts
    // `{portal_module_enabled: v}` alone so a flip does not disturb
    // any other tenant field; the server merges via the same JSONB
    // pattern the branding save uses. Refetches on success so the
    // toggle re-hydrates from server state (and the toggle-off case
    // that immediately evicts every portal session gets its state
    // reflected authoritatively). `toggle_saving` + `toggle_error`
    // hooks are declared above the not-admin early return; the closure
    // below just captures them.
    let mut on_toggle_module = move |next: bool| {
        if toggle_saving() {
            return;
        }
        toggle_saving.set(true);
        toggle_error.set(String::new());
        spawn(async move {
            let body = serde_json::json!({ "portal_module_enabled": next });
            match crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                "/tenants/current",
                &body,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        if next {
                            "Portal module enabled for this tenant.".to_string()
                        } else {
                            "Portal module disabled. Live portal sessions will end on their next request."
                                .to_string()
                        },
                    );
                    resource.restart();
                }
                Err(e) => {
                    toggle_error.set(format!("Save failed: {e}"));
                }
            }
            toggle_saving.set(false);
        });
    };

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
                p { class: "text-sm text-muted", "Loading branding…" }
            } else {
                // MAPPS-651 / MAPPS-648: tier-1 toggle sits above the
                // branding editor since it gates whether the whole
                // portal surface is reachable at all. Turning off
                // evicts every live portal session (mirrors the
                // MAPPS-557 tenant-suspend semantic one tier down);
                // the toast message names that side-effect so the
                // operator is not surprised.
                Card {
                    div { class: "p-6 space-y-2 max-w-2xl",
                        h2 { class: "text-lg font-semibold text-content", "Portal module" }
                        p { class: "text-sm text-muted",
                            "The portal module gates whether any client Company under this tenant can offer a portal. Off = the whole surface is unreachable; on = Company Admins can enable the portal for individual Companies from each Company's detail page."
                        }
                        Checkbox {
                            name: "portal_module_enabled",
                            label: "Portals enabled for this tenant",
                            checked: portal_module_enabled,
                            help: "Turning this off signs out every portal user across every Company on their next request.",
                            disabled: toggle_saving(),
                            onchange: move |e: FormEvent| on_toggle_module(e.checked()),
                        }
                        if !toggle_error().is_empty() {
                            p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{toggle_error}" }
                        }
                    }
                }
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

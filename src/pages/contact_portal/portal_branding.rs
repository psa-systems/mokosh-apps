//! MAPPS-620 (mokosh-branding prompt 004): contact-plane portal
//! branding editor.
//!
//! Mounted at `/settings/portal-branding` under the contact plane.
//! First-render gate: `use_capability("settings:manage_company_branding")`;
//! false renders `ContentUnavailable` (the sidebar entry only
//! surfaces the link when the capability is held, so a caller who
//! reaches this URL directly still sees a clean explanation rather
//! than a broken form).
//!
//! Load: `GET /api/v1/contact/companies/self/branding` returns the
//! raw tenant + Company blocks plus the resolved effective set. Save:
//! `PATCH /api/v1/contact/companies/self/branding` with a JSONB
//! subset the caller owns (an explicit `null` clears a key so the
//! tenant default flows back through the resolver on the next fetch).

use dioxus::prelude::*;

use crate::components::{BrandingEditor, ContentUnavailable};
use crate::hooks::branding::{CompanyBranding, ContactOwnCompanyBranding, TenantBranding};

const CAP: &str = "settings:manage_company_branding";

#[component]
pub fn ContactPortalBrandingPage() -> Element {
    if !crate::hooks::capabilities::use_capability(CAP) {
        return rsx! {
            ContentUnavailable {
                title: "Portal branding".to_string(),
                show_dashboard_link: true,
            }
        };
    }
    let mut resource = use_resource(|| async {
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_contact_authed::<ContactOwnCompanyBranding>(
            "/contact/companies/self/branding",
        )
        .await
        .ok()
    });
    let mut error: Signal<String> = use_signal(String::new);
    let mut toast: Signal<String> = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let snap = resource.read_unchecked();
    let (current, defaults, effective_for_paint): (
        CompanyBranding,
        TenantBranding,
        crate::hooks::branding::EffectiveBranding,
    ) = match &*snap {
        Some(Some(b)) => (b.company.clone(), b.tenant.clone(), b.effective.clone()),
        _ => (
            CompanyBranding::default(),
            TenantBranding::default(),
            crate::hooks::branding::EffectiveBranding::default(),
        ),
    };
    let loading = matches!(&*snap, None);

    let on_save = move |block: CompanyBranding| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        toast.set(String::new());
        spawn(async move {
            match crate::hooks::fetch::api::patch_contact_authed_typed::<
                ContactOwnCompanyBranding,
                _,
            >("/contact/companies/self/branding", &block)
            .await
            {
                Ok(fresh) => {
                    // Push the fresh effective block into the global
                    // signal so `AuthLayout` repaints the visible
                    // brand immediately, before the next `/refresh`
                    // tick would land it.
                    crate::hooks::branding::set_effective_branding(fresh.effective.clone());
                    toast.set("Branding saved.".to_string());
                    resource.restart();
                }
                Err(e) => {
                    error.set(format!("Save failed: {e}"));
                }
            }
            saving.set(false);
        });
    };

    let _ = effective_for_paint; // reserved for a "current preview" panel below.

    rsx! {
        div { class: "max-w-4xl mx-auto space-y-6 p-6",
            div {
                h1 { class: "text-2xl font-semibold text-content", "Portal branding" }
                p { class: "text-sm text-muted mt-1",
                    "Customize how your company's portal looks to your colleagues. Empty fields inherit from your MSP's defaults."
                }
            }
            if loading {
                p { class: "text-sm text-muted", "Loading branding..." }
            } else {
                BrandingEditor {
                    current,
                    tenant_defaults: defaults,
                    plane: crate::components::BrandingPlane::ContactSelf,
                    disabled: saving(),
                    on_save,
                    on_asset_saved: move |_| {
                        resource.restart();
                        toast.set("Asset saved.".to_string());
                    },
                }
                if !error().is_empty() {
                    p { role: "alert", class: "text-sm text-red-600 dark:text-red-400", "{error}" }
                }
                if !toast().is_empty() {
                    p { class: "text-sm text-green-700 dark:text-green-400", "{toast}" }
                }
            }
        }
    }
}

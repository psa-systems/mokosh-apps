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

use crate::components::{BrandingEditor, Card, PageHeader};
use crate::hooks::branding::{CompanyBranding, ContactOwnCompanyBranding, TenantBranding};

const CAP: &str = "settings:manage_company_branding";

#[component]
pub fn ContactPortalBrandingPage() -> Element {
    if !crate::hooks::capabilities::use_capability(CAP) {
        // The default `ContentUnavailable` widget's copy reads as
        // "the server is unreachable", which is the wrong signal for
        // a permission-denied state (the server is fine; the caller
        // just lacks a role). Render a bespoke panel that spells out
        // WHY the page is empty + how to get access. Falls back to
        // the brand's `support_email` / `support_phone` so the
        // contact has a direct handle to their MSP for the ask,
        // without needing to leave the portal.
        let brand = crate::hooks::branding::EFFECTIVE_BRANDING.read();
        let support_email = brand.support_email.clone().filter(|s| !s.is_empty());
        let support_phone = brand.support_phone.clone().filter(|s| !s.is_empty());
        let support_contact = brand
            .support_contact_name
            .clone()
            .filter(|s| !s.is_empty());
        return rsx! {
            PageHeader { title: "Portal branding".to_string() }
            div { class: "max-w-3xl mx-auto",
                Card {
                    div { class: "py-8 px-6 space-y-4",
                        h3 { class: "text-base font-semibold text-content",
                            "You need the Manage portal branding role to customize this page."
                        }
                        p { class: "text-sm text-muted",
                            "Ask your MSP administrator to grant your account the 'Manage portal branding' capability. Once they do, this page unlocks the full editor so you can set your Company's logo, colors, and support-contact block without pinging them for every change."
                        }
                        if support_email.is_some() || support_phone.is_some() {
                            div { class: "text-sm border-t border-line pt-4",
                                p { class: "text-content font-medium mb-1",
                                    if let Some(name) = support_contact {
                                        "Reach out to {name}"
                                    } else {
                                        "Reach out to your MSP"
                                    }
                                }
                                if let Some(email) = support_email {
                                    div {
                                        a {
                                            href: "mailto:{email}",
                                            class: "text-accent hover:underline",
                                            "{email}"
                                        }
                                    }
                                }
                                if let Some(phone) = support_phone {
                                    div { class: "text-muted", "{phone}" }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
    // Track the fetch outcome as `Result` (not the `.ok()`-flattened
    // `Option`) so we can distinguish "still loading" from "fetch
    // failed" from "fetch returned empty". A failed fetch surfaces
    // in the UI so a contact whose portal server hasn't shipped
    // MAPPS-618 yet sees the specific error instead of a spinner
    // that never resolves.
    let mut resource = use_resource(|| async {
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_contact_authed::<ContactOwnCompanyBranding>(
            "/contact/companies/self/branding",
        )
        .await
        .map_err(|e| e.to_string())
    });
    let mut error: Signal<String> = use_signal(String::new);
    let mut toast: Signal<String> = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let snap = resource.read_unchecked();
    let (current, defaults, effective_for_paint, fetch_error): (
        CompanyBranding,
        TenantBranding,
        crate::hooks::branding::EffectiveBranding,
        Option<String>,
    ) = match &*snap {
        Some(Ok(b)) => (
            b.company.clone(),
            b.tenant.clone(),
            b.effective.clone(),
            None,
        ),
        Some(Err(msg)) => (
            CompanyBranding::default(),
            TenantBranding::default(),
            crate::hooks::branding::EffectiveBranding::default(),
            Some(msg.clone()),
        ),
        None => (
            CompanyBranding::default(),
            TenantBranding::default(),
            crate::hooks::branding::EffectiveBranding::default(),
            None,
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
            } else if let Some(msg) = fetch_error.clone() {
                Card {
                    div { class: "py-6 px-6 space-y-3",
                        h3 { class: "text-base font-semibold text-content",
                            "Couldn't load your portal branding."
                        }
                        p { class: "text-sm text-muted",
                            "The server responded with an error. If your MSP just enabled portal branding, they may still be finishing the rollout - try again in a minute."
                        }
                        p { class: "text-xs text-muted italic", "Details: {msg}" }
                    }
                }
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

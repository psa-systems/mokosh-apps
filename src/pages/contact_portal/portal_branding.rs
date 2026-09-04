//! MAPPS-620 (mokosh-branding prompt 004): contact-plane portal
//! branding editor.
//!
//! Mounted at `/settings/portal-branding` under the contact plane.
//! First-render gate: `use_capability("settings:manage_company_branding")`.
//! A contact who lacks the permission (or hits a server that has not
//! shipped the feature yet, so the branding endpoint declines) sees
//! the shared [`NoAccessPanel`] below: no jargon, no technical
//! details, just an explanation that they need extra access and a
//! direct handle to their MSP for the ask.
//!
//! Load: `GET /api/v1/contact/companies/self/branding` returns the
//! raw tenant + Company blocks plus the resolved effective set. Save:
//! `PATCH /api/v1/contact/companies/self/branding` with a JSONB
//! subset the caller owns (an explicit `null` clears a key so the
//! tenant default flows back through the resolver on the next fetch).

use dioxus::prelude::*;

use crate::components::{BrandingEditor, Card, IconSize, PageHeader, ShieldCheckIcon};
use crate::hooks::branding::{
    CompanyBranding, ContactOwnCompanyBranding, EffectiveBranding, TenantBranding,
};

const CAP: &str = "settings:manage_company_branding";

#[component]
pub fn ContactPortalBrandingPage() -> Element {
    // MAPPS-602: every hook fires BEFORE the no-cap early return so
    // the render that takes the exit does not leave the component a
    // hook short.
    let has_cap = crate::hooks::capabilities::use_capability(CAP);
    // Track the fetch outcome as `Result` (not the `.ok()`-flattened
    // `Option`) so we can tell "still loading" from "load declined".
    // If the server rejects the request for any reason we render the
    // same friendly `NoAccessPanel` a permission-denied caller sees;
    // a portal visitor should not see raw error strings, status
    // codes, or hints about what changed on the server side.
    let mut resource = use_resource(|| async {
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_contact_authed::<ContactOwnCompanyBranding>(
            "/contact/companies/self/branding",
        )
        .await
        // The panel deliberately says nothing about the cause (see above), so
        // the log is the only place the reason survives.
        .inspect_err(|e| tracing::error!("contact company branding load failed: {e}"))
        .ok()
    });
    let mut error: Signal<String> = use_signal(String::new);
    let _toast: Signal<String> = use_signal(String::new);
    let mut saving = use_signal(|| false);
    if !has_cap {
        return rsx! { NoAccessPanel {} };
    }

    let snap = resource.read_unchecked();
    let (current, defaults, effective_for_paint): (
        CompanyBranding,
        TenantBranding,
        EffectiveBranding,
    ) = match &*snap {
        Some(Some(b)) => (b.company.clone(), b.tenant.clone(), b.effective.clone()),
        _ => (
            CompanyBranding::default(),
            TenantBranding::default(),
            EffectiveBranding::default(),
        ),
    };
    let loading = (*snap).is_none();
    let load_declined = matches!(&*snap, Some(None));
    if load_declined {
        return rsx! { NoAccessPanel {} };
    }

    let on_save = move |block: CompanyBranding| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
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
                    // MAPPS-635 G: shared toast infra.
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        "Branding saved.".to_string(),
                    );
                    resource.restart();
                }
                Err(_) => {
                    // Never surface the underlying `ApiError` to a
                    // portal user; the actionable read for them is
                    // "your change did not stick, try again", which
                    // covers both a network wobble and a 4xx from
                    // the server.
                    error.set(
                        "We couldn't save that change. Try again in a moment.".to_string(),
                    );
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
                p { class: "text-sm text-muted", "Loading branding…" }
            } else {
                BrandingEditor {
                    current,
                    tenant_defaults: defaults,
                    plane: crate::components::BrandingPlane::ContactSelf,
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

/// Friendly, jargon-free panel rendered whenever a caller cannot see
/// the branding editor: they lack the permission client-side, OR the
/// server-side gate declined the load. Copy deliberately avoids the
/// words "capability", "role", "cap", "permission code", status
/// codes, or anything a portal user would not recognize. Falls back
/// to the brand's support contact block so the reader has one place
/// to click for the ask.
#[component]
fn NoAccessPanel() -> Element {
    let brand = crate::hooks::branding::EFFECTIVE_BRANDING.read();
    let support_email = brand.support_email.clone().filter(|s| !s.is_empty());
    let support_phone = brand.support_phone.clone().filter(|s| !s.is_empty());
    let support_contact = brand.support_contact_name.clone().filter(|s| !s.is_empty());
    let has_any_contact = support_email.is_some() || support_phone.is_some();
    let contact_lead = support_contact
        .clone()
        .map(|name| format!("Reach out to {name}."))
        .unwrap_or_else(|| "Reach out to your administrator or support team.".to_string());
    rsx! {
        PageHeader { title: "Portal branding".to_string() }
        div { class: "max-w-2xl mx-auto",
            Card {
                div { class: "py-10 px-8 flex flex-col items-center text-center gap-4",
                    div { class: "flex h-14 w-14 items-center justify-center rounded-full bg-accent/10 text-accent",
                        ShieldCheckIcon { size: IconSize::Large }
                    }
                    h3 { class: "text-xl font-semibold text-content",
                        "You don't have access to this page."
                    }
                    p { class: "text-sm text-muted max-w-md",
                        "Customizing your portal's branding needs extra permissions your account doesn't have right now. Please contact your administrator or support team and ask them to enable this for you."
                    }
                    if has_any_contact {
                        div { class: "mt-2 w-full border-t border-line pt-4 text-sm space-y-1",
                            p { class: "text-content font-medium", "{contact_lead}" }
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
                                // MAPPS-635 D3: tel: link.
                                div {
                                    a {
                                        href: "tel:{phone}",
                                        class: "text-accent hover:underline",
                                        "{phone}"
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

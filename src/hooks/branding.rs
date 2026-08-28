//! MAPPS-619/620/621 (mokosh-branding prompts 003-005): client-side
//! branding wire types + fetch helpers + a global signal the
//! `AuthLayout` painter (prompt 005) reads on every render.
//!
//! Types are shadowed here rather than pulled from the `mokosh-types`
//! crate so the client compiles regardless of which server branch the
//! `Cargo.toml` pin currently tracks (MAPPS-617's new fields have not
//! landed on `mokosh-client-login` yet). Wire shape is the SAME
//! `EffectiveBranding` / `TenantBranding` / `CompanyBranding` the
//! server returns; `serde(default)` on every field lets a legacy
//! response deserialize cleanly.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Wire shape returned by every server endpoint that fills a
/// branding block: `GET /portal/{id}/host`, `GET /contact/auth/me`,
/// `POST /contact/auth/login`, `POST /contact/auth/refresh`, plus the
/// new `GET /contact/companies/self/branding`. Every field is
/// `Option<String>`; both sides of the merge falling through leaves
/// `None` and the SPA supplies the coded fallback.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct EffectiveBranding {
    // MAPPS-618 phase B: skip_serializing_if=Option::is_none on every
    // field. The BrandingEditor submits its FULL local state on
    // every save; the JSONB merge on the server treats a JSON null
    // as "clear this key" and an absent key as "leave untouched".
    // Serializing every None as null would clobber uploaded logo /
    // favicon / background URLs whenever the JSON editor saves the
    // color / support-text fields. skip_serializing_if lets the
    // caller opt into an explicit clear by emitting `"key": null`
    // via `serde_json::json!` directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon_mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_contact_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portal_domain: Option<String>,
}

/// Raw tenant defaults (staff-owned). Same shape as
/// [`EffectiveBranding`] on purpose so the merge is a keys union.
pub type TenantBranding = EffectiveBranding;

/// Raw Company overrides (Company-owned). Same shape as
/// [`EffectiveBranding`].
pub type CompanyBranding = EffectiveBranding;

/// Response from `GET /api/v1/contact/companies/self/branding`.
/// Powers the contact-plane editor's "Inherits from MSP default: X"
/// hints per field.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContactOwnCompanyBranding {
    #[serde(default)]
    pub tenant: TenantBranding,
    #[serde(default)]
    pub company: CompanyBranding,
    #[serde(default)]
    pub effective: EffectiveBranding,
}

/// Global signal painted by `AuthLayout` (prompt 005). Populated by
/// the login response, the refresh response, and the `/auth/me` cold
/// bootstrap; also by the pre-auth `/host` fetch on the step-2 login
/// page so branding paints BEFORE the visitor signs in. Missing
/// session leaves `EffectiveBranding::default()`; painters fall back
/// to coded defaults on every `None` field.
pub static EFFECTIVE_BRANDING: dioxus::prelude::GlobalSignal<EffectiveBranding> =
    dioxus::prelude::GlobalSignal::new(EffectiveBranding::default);

/// Push a fresh effective-branding block into the global signal.
/// Callers: contact-plane auth-response handlers, the `/host` fetch,
/// the branding editor's save action (to see the change immediately
/// without waiting for the next refresh tick).
pub fn set_effective_branding(next: EffectiveBranding) {
    *EFFECTIVE_BRANDING.write() = next;
}

/// Reset to the coded fallback. Called on sign-out so the next
/// unauthenticated `AuthLayout` render paints the neutral default.
pub fn clear_effective_branding() {
    *EFFECTIVE_BRANDING.write() = EffectiveBranding::default();
    #[cfg(feature = "web")]
    clear_brand_css_vars();
}

/// MAPPS-621 CSS pipeline: inline the brand colors as CSS custom
/// properties on `<html>`. Mirrors the `hooks::theme::apply_now`
/// pattern (accent picker uses the same lever). When
/// `primary_color` is set the whole `text-accent` / `bg-accent` /
/// `border-accent` / ring-accent family repaints because those
/// utilities resolve `color: var(--accent)` in `input.css`.
///
/// Only touches the accent + on-accent + a small set of brand-only
/// custom properties. Leaves the theme-picker's per-user accent
/// alone whenever the brand side is `None`, so a staff user's
/// personal accent stays intact on staff pages (the brand signal
/// stays default there because it is only populated from
/// contact-plane fetches + the `/host` pre-auth splash).
#[cfg(feature = "web")]
pub fn apply_brand_css_vars(brand: &EffectiveBranding) {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Some(root) = doc.document_element() else {
        return;
    };
    let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let style = html.style();
    if let Some(primary) = brand.primary_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--accent", primary);
        // A brand-only alias for downstream consumers that don't want
        // to override the theme-picker's `--accent`. Reserved for the
        // future per-brand accent that opts out of the picker
        // override; today the two are the same value.
        let _ = style.set_property("--brand-primary", primary);
    }
    if let Some(secondary) = brand.secondary_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--brand-secondary", secondary);
    }
    if let Some(bg) = brand.background_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--brand-bg", bg);
    }
}

#[cfg(not(feature = "web"))]
pub fn apply_brand_css_vars(_brand: &EffectiveBranding) {}

/// Undo `apply_brand_css_vars` on sign-out so the theme-picker's
/// stored accent is what the SPA reads again. Removes only the
/// brand-owned properties; the theme's own `--accent-N` ramp stays
/// intact (theme::apply_now re-applies on the next render).
#[cfg(feature = "web")]
pub fn clear_brand_css_vars() {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Some(root) = doc.document_element() else {
        return;
    };
    let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let style = html.style();
    let _ = style.remove_property("--brand-primary");
    let _ = style.remove_property("--brand-secondary");
    let _ = style.remove_property("--brand-bg");
    // Re-apply the theme picker so `--accent` returns to the
    // user-chosen accent rather than sticking on the brand color.
    crate::hooks::theme::apply_now();
}

#[cfg(not(feature = "web"))]
pub fn clear_brand_css_vars() {}

/// Root-mounted hook that repaints the brand's CSS custom
/// properties whenever the effective-branding signal changes. Mount
/// once at the App root next to `use_apply_theme`; each render reads
/// the signal and calls `apply_brand_css_vars`.
#[cfg(feature = "web")]
pub fn use_apply_brand() {
    use dioxus::prelude::*;
    use_effect(move || {
        let brand = EFFECTIVE_BRANDING.read().clone();
        apply_brand_css_vars(&brand);
    });
}

#[cfg(not(feature = "web"))]
pub fn use_apply_brand() {}

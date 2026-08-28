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
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub logo_mime: Option<String>,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub favicon_mime: Option<String>,
    #[serde(default)]
    pub primary_color: Option<String>,
    #[serde(default)]
    pub secondary_color: Option<String>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub background_url: Option<String>,
    #[serde(default)]
    pub background_mime: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub company_name: Option<String>,
    #[serde(default)]
    pub support_email: Option<String>,
    #[serde(default)]
    pub support_phone: Option<String>,
    #[serde(default)]
    pub support_contact_name: Option<String>,
    #[serde(default)]
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
}

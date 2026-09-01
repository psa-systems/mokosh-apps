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
    // PRIMARY: hijack `--accent` so every `text-accent` / `bg-accent`
    // / `border-accent` / `ring-accent` utility across the stylesheet
    // repaints. `--brand-primary` is a stable alias for consumers
    // that want the brand color independent of the theme picker.
    if let Some(primary) = brand.primary_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--accent", primary);
        let _ = style.set_property("--brand-primary", primary);
    } else {
        let _ = style.remove_property("--brand-primary");
    }
    // SECONDARY: paint as the `--secondary` design-token consumer
    // pattern. Also expose `--brand-secondary` for future utilities.
    if let Some(secondary) = brand.secondary_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--secondary", secondary);
        let _ = style.set_property("--brand-secondary", secondary);
    } else {
        let _ = style.remove_property("--secondary");
        let _ = style.remove_property("--brand-secondary");
    }
    // BACKGROUND COLOR: hijack `--bg` so every `bg-app` element
    // (the outer container of AuthLayout + the AppShell body) picks
    // up the brand background. Reverts to the theme value on
    // sign-out via `clear_brand_css_vars` -> `theme::apply_now`.
    if let Some(bg) = brand.background_color.as_deref().filter(|s| !s.is_empty()) {
        let _ = style.set_property("--bg", bg);
        let _ = style.set_property("--brand-bg", bg);
    } else {
        let _ = style.remove_property("--brand-bg");
        // Do NOT clear `--bg` here: the theme (`hooks::theme::apply_now`)
        // owns the base value and re-applying it here would race the
        // theme hook. Falling back to whatever the theme wrote last
        // is correct.
    }
    // BACKGROUND IMAGE: paint on <body> via inline style so the
    // uploaded image tiles the whole app. Bare data / http(s) urls
    // are wrapped in url(). Cleared when the brand has no
    // `background_url` so a subsequent brand save that omits the
    // image reverts to the flat background color / theme.
    if let Some(body) = doc.body() {
        let body_style = body.style();
        if let Some(url) = brand.background_url.as_deref().filter(|s| !s.is_empty()) {
            let escaped = url.replace('"', "%22");
            let _ = body_style.set_property("background-image", &format!("url(\"{escaped}\")"));
            let _ = body_style.set_property("background-size", "cover");
            let _ = body_style.set_property("background-position", "center");
            let _ = body_style.set_property("background-attachment", "fixed");
            let _ = body_style.set_property("background-repeat", "no-repeat");
        } else {
            let _ = body_style.remove_property("background-image");
            let _ = body_style.remove_property("background-size");
            let _ = body_style.remove_property("background-position");
            let _ = body_style.remove_property("background-attachment");
            let _ = body_style.remove_property("background-repeat");
        }
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
    let _ = style.remove_property("--secondary");
    let _ = style.remove_property("--brand-bg");
    let _ = style.remove_property("--bg");
    // Also drop the body background-image so a returning visitor
    // does not see the brand background flash on the pre-auth
    // page.
    if let Some(body) = doc.body() {
        let body_style = body.style();
        let _ = body_style.remove_property("background-image");
        let _ = body_style.remove_property("background-size");
        let _ = body_style.remove_property("background-position");
        let _ = body_style.remove_property("background-attachment");
        let _ = body_style.remove_property("background-repeat");
    }
    // Re-apply the theme picker so `--accent` returns to the
    // user-chosen accent rather than sticking on the brand color.
    crate::hooks::theme::apply_now();
}

#[cfg(not(feature = "web"))]
pub fn clear_brand_css_vars() {}

/// Root-mounted hook that repaints the brand's CSS custom
/// properties whenever the effective-branding signal changes AND
/// updates the browser-tab favicon to point at `branding.favicon_url`.
/// Mount once at the App root next to `use_apply_theme`; each render
/// reads the signal and applies both.
#[cfg(feature = "web")]
pub fn use_apply_brand() {
    use dioxus::prelude::*;
    use_effect(move || {
        let brand = EFFECTIVE_BRANDING.read().clone();
        apply_brand_css_vars(&brand);
        apply_favicon(&brand);
    });
}

#[cfg(not(feature = "web"))]
pub fn use_apply_brand() {}

/// MAPPS-621: point every `<link rel="icon">` at
/// `branding.favicon_url` so the browser tab icon reflects the
/// current brand. Falls back to the SPA's coded default (a
/// `/favicon.svg` + `/favicon.ico` pair from `index.html`) by
/// re-applying those hrefs when the brand has no favicon set,
/// so a sign-out returns the tab icon to the Mokosh default.
///
/// Selects every existing `link[rel~=icon]` (there are two in
/// index.html: an SVG primary + an ICO fallback), sets both to the
/// same brand URL, and clears the `type` attribute since the served
/// image will be whatever mime the upload stored (typically PNG or
/// WebP). Most browsers still recompute the icon from the first
/// success.
#[cfg(feature = "web")]
pub fn apply_favicon(brand: &EffectiveBranding) {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Ok(nodes) = doc.query_selector_all("link[rel~=\"icon\"]") else {
        return;
    };
    let target = brand.favicon_url.as_deref().filter(|s| !s.is_empty());
    // Default hrefs restored on clear. Kept in sync with index.html.
    const DEFAULT_ICONS: &[(&str, &str)] =
        &[("/favicon.svg", "image/svg+xml"), ("/favicon.ico", "image/x-icon")];
    for i in 0..nodes.length() {
        let Some(node) = nodes.item(i) else { continue };
        let Ok(el) = node.dyn_into::<web_sys::Element>() else {
            continue;
        };
        if let Some(url) = target {
            let _ = el.set_attribute("href", url);
            // Clear the type hint; whatever mime the server serves
            // wins. Chrome + Firefox recompute from the response.
            let _ = el.remove_attribute("type");
        } else {
            // Restore the coded default matching this <link>'s slot
            // by index; safe because the SPA ships exactly two icon
            // links in a fixed order.
            let idx = i as usize;
            if let Some((href, ty)) = DEFAULT_ICONS.get(idx) {
                let _ = el.set_attribute("href", href);
                let _ = el.set_attribute("type", ty);
            }
        }
    }
}

#[cfg(not(feature = "web"))]
pub fn apply_favicon(_brand: &EffectiveBranding) {}

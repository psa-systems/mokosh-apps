//! MAPPS-619/620/621 (mokosh-branding prompts 003-005): client-side
//! branding wire types + fetch helpers + a global signal the
//! `AuthLayout` painter (prompt 005) reads on every render.
//!
//! Types are shadowed here rather than pulled from the `mokosh-types`
//! crate because the client and server crates are still built and
//! released independently (the `Cargo.toml` pin followed the server's
//! `mokosh-contact-login` branch during MAPPS-617 through MAPPS-621's
//! rollout and now follows `main` again per MAPPS-726). Wire shape is
//! the SAME `EffectiveBranding` / `TenantBranding` / `CompanyBranding`
//! the server returns; `serde(default)` on every field lets a legacy
//! response deserialize cleanly.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Wire shape returned by every server endpoint that fills a
/// branding block: `GET /portal/{id}/host`, `GET /contact/auth/me`,
/// `POST /contact/auth/login`, `POST /contact/auth/refresh`, plus the
/// new `GET /contact/companies/self/branding`. Every field is
/// `Option<String>`; both sides of the merge falling through leaves
/// `None` and the SPA supplies the coded fallback.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
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
    // MAPPS-1024: the five identity keys that drive the invoice "From"
    // block (PMS-911 issuer::resolve: legal_name -> company_name ->
    // tenant display name) and the invoice template choice (PMS-1006).
    // Server already accepts them under `branding` on PUT
    // /tenants/current; invoice_template is tenant-only per
    // TENANT_ONLY_KEYS on the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_template: Option<String>,
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

/// MAPPS-767: [`versioned_asset_url`], absolute against the API origin.
///
/// `branding.logo_url` is a PATH by design - the server's write validator
/// pins it to `/api/v1/public/tenants/` - and the API is on a different host
/// from the SPA on every deployed environment. A page rendering the path
/// directly resolves it against its OWN origin and gets a 404, which is what
/// the portal sign-in page did: `https://msp.a8n.systems/api/v1/public/...`
/// where the logo lives on `api.msp.a8n.systems`.
///
/// PMS-758 settled the rule and `api_origin` is the helper for it (it strips
/// the `/api/v1` suffix, because the stored value carries its own). This
/// wraps the two steps together so a render site cannot do one and forget the
/// other, which is how the auth layout came to version a URL it had not yet
/// made absolute.
///
/// Empty in, empty out, so a caller with no logo renders nothing rather than
/// a bare origin.
pub fn absolute_versioned_asset_url(url: &str, brand: &EffectiveBranding) -> String {
    if url.trim().is_empty() {
        return String::new();
    }
    let versioned = versioned_asset_url(url, brand);
    if versioned.starts_with("http://") || versioned.starts_with("https://") {
        return versioned;
    }
    #[cfg(feature = "app")]
    {
        format!("{}{versioned}", crate::hooks::fetch::api::api_origin())
    }
    #[cfg(not(feature = "app"))]
    {
        versioned
    }
}

/// MAPPS-635 A: append a version query param to a brand-asset URL so
/// the browser cache misses whenever the underlying brand changes.
/// The server serves brand assets with `Cache-Control: public,
/// max-age=3600` + no ETag; without a version param an upload would
/// not repaint until the hour expires. Version is a stable
/// content-derived hash of the whole `EffectiveBranding` block, so
/// same block → same URL → cache hit, any field change → new URL →
/// cache miss.
///
/// URL is passed through unchanged when empty or already carrying a
/// query string with a `v` parameter (defensive: never double-tag).
/// MAPPS-1238: that means a `v` KEY in the query, not the substring
/// `v=` anywhere, which skipped `?rev=2` and any path containing `v=`. A
/// bare url `/api/v1/public/tenants/{id}/logo` becomes
/// `/api/v1/public/tenants/{id}/logo?v=<hash>`.
pub fn versioned_asset_url(url: &str, brand: &EffectiveBranding) -> String {
    if url.is_empty() {
        return String::new();
    }
    if has_version_param(url) {
        return url.to_string();
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    brand.hash(&mut hasher);
    let version = hasher.finish();
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}v={version:x}")
}

/// Does the query string of `url` already carry a `v` parameter?
fn has_version_param(url: &str) -> bool {
    let Some((_, query)) = url.split_once('?') else {
        return false;
    };
    let query = query.split('#').next().unwrap_or(query);
    query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("v"))
}

/// What a brand without a primary or background colour has to undo
/// (MAPPS-1238). `--accent` and `--bg` are only ever hijacked together with
/// their `--brand-*` alias, so a non-empty alias means the previous brand's
/// value is still painted and has to go, or a later brand that sets no colour
/// keeps showing the old one until sign-out.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn stale_brand_overrides(
    painted_primary: &str,
    painted_bg: &str,
    brand: &EffectiveBranding,
) -> (bool, bool) {
    let unset = |v: &Option<String>| v.as_deref().is_none_or(str::is_empty);
    (
        unset(&brand.primary_color) && !painted_primary.is_empty(),
        unset(&brand.background_color) && !painted_bg.is_empty(),
    )
}

/// Write one inline style property, logging a refusal (MAPPS-1238: every
/// result here used to be discarded).
#[cfg(target_arch = "wasm32")]
fn set_style(style: &web_sys::CssStyleDeclaration, name: &str, value: &str) {
    if let Err(e) = style.set_property(name, value) {
        tracing::error!("could not set the brand style {name}: {e:?}");
    }
}

/// Remove one inline style property, logging a refusal.
#[cfg(target_arch = "wasm32")]
fn remove_style(style: &web_sys::CssStyleDeclaration, name: &str) {
    if let Err(e) = style.remove_property(name) {
        tracing::error!("could not remove the brand style {name}: {e:?}");
    }
}

/// Reset to the coded fallback. Called on sign-out so the next
/// unauthenticated `AuthLayout` render paints the neutral default.
pub fn clear_effective_branding() {
    *EFFECTIVE_BRANDING.write() = EffectiveBranding::default();
    #[cfg(target_arch = "wasm32")]
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
#[cfg(target_arch = "wasm32")]
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
    // Read before anything below rewrites them: whether the previous brand
    // left its colours painted.
    let (restore_accent, drop_bg) = stale_brand_overrides(
        &style
            .get_property_value("--brand-primary")
            .unwrap_or_default(),
        &style.get_property_value("--brand-bg").unwrap_or_default(),
        brand,
    );
    // PRIMARY: hijack `--accent` so every `text-accent` / `bg-accent`
    // / `border-accent` / `ring-accent` utility across the stylesheet
    // repaints. `--brand-primary` is a stable alias for consumers
    // that want the brand color independent of the theme picker.
    if let Some(primary) = brand.primary_color.as_deref().filter(|s| !s.is_empty()) {
        set_style(&style, "--accent", primary);
        set_style(&style, "--brand-primary", primary);
    } else {
        remove_style(&style, "--brand-primary");
    }
    // SECONDARY: paint as the `--secondary` design-token consumer
    // pattern. Also expose `--brand-secondary` for future utilities.
    if let Some(secondary) = brand.secondary_color.as_deref().filter(|s| !s.is_empty()) {
        set_style(&style, "--secondary", secondary);
        set_style(&style, "--brand-secondary", secondary);
    } else {
        remove_style(&style, "--secondary");
        remove_style(&style, "--brand-secondary");
    }
    // BACKGROUND COLOR: hijack `--bg` so every `bg-app` element
    // (the outer container of AuthLayout + the AppShell body) picks
    // up the brand background. Reverts to the theme value on
    // sign-out via `clear_brand_css_vars` -> `theme::apply_now`.
    if let Some(bg) = brand.background_color.as_deref().filter(|s| !s.is_empty()) {
        set_style(&style, "--bg", bg);
        set_style(&style, "--brand-bg", bg);
    } else {
        remove_style(&style, "--brand-bg");
        // MAPPS-1238: the theme never writes `--bg` inline (it comes from
        // the stylesheet, driven by the `dark` class), so the only inline
        // `--bg` is a previous brand's. Removing it is what reverts to the
        // theme; leaving it kept the old brand's background.
        if drop_bg {
            remove_style(&style, "--bg");
        }
    }
    // BACKGROUND IMAGE: paint on <body> via inline style so the
    // uploaded image tiles the whole app. Bare data / http(s) urls
    // are wrapped in url(). Cleared when the brand has no
    // `background_url` so a subsequent brand save that omits the
    // image reverts to the flat background color / theme.
    if let Some(body) = doc.body() {
        let body_style = body.style();
        if let Some(url) = brand.background_url.as_deref().filter(|s| !s.is_empty()) {
            // MAPPS-767: absolute, for the same reason the logo is - a CSS
            // `url()` resolves against the document, not the API.
            let versioned = absolute_versioned_asset_url(url, brand);
            let escaped = versioned.replace('"', "%22");
            set_style(
                &body_style,
                "background-image",
                &format!("url(\"{escaped}\")"),
            );
            set_style(&body_style, "background-size", "cover");
            set_style(&body_style, "background-position", "center");
            set_style(&body_style, "background-attachment", "fixed");
            set_style(&body_style, "background-repeat", "no-repeat");
        } else {
            remove_body_background(&body_style);
        }
    }
    // MAPPS-1238: `--accent` is the theme's own inline value, so a previous
    // brand's primary is undone by re-applying the user's accent, as
    // `clear_brand_css_vars` does on sign-out.
    if restore_accent {
        crate::hooks::theme::apply_now();
    }
}

/// The five body properties a brand background image sets.
#[cfg(target_arch = "wasm32")]
fn remove_body_background(body_style: &web_sys::CssStyleDeclaration) {
    for name in [
        "background-image",
        "background-size",
        "background-position",
        "background-attachment",
        "background-repeat",
    ] {
        remove_style(body_style, name);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_brand_css_vars(_brand: &EffectiveBranding) {}

/// Undo `apply_brand_css_vars` on sign-out so the theme-picker's
/// stored accent is what the SPA reads again. Removes only the
/// brand-owned properties; the theme's own `--accent-N` ramp stays
/// intact (theme::apply_now re-applies on the next render).
#[cfg(target_arch = "wasm32")]
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
    for name in [
        "--brand-primary",
        "--brand-secondary",
        "--secondary",
        "--brand-bg",
        "--bg",
    ] {
        remove_style(&style, name);
    }
    // Also drop the body background-image so a returning visitor
    // does not see the brand background flash on the pre-auth
    // page.
    if let Some(body) = doc.body() {
        remove_body_background(&body.style());
    }
    // Re-apply the theme picker so `--accent` returns to the
    // user-chosen accent rather than sticking on the brand color.
    crate::hooks::theme::apply_now();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_brand_css_vars() {}

/// Root-mounted hook that repaints the brand's CSS custom
/// properties whenever the effective-branding signal changes,
/// updates the browser-tab favicon to point at
/// `branding.favicon_url`, AND sets `document.title` to the brand's
/// display name so the tab reads as the client-facing wordmark
/// instead of "Mokosh Platform". Mount once at the App root next to
/// `use_apply_theme`.
#[cfg(target_arch = "wasm32")]
pub fn use_apply_brand() {
    use dioxus::prelude::*;
    use_effect(move || {
        let brand = EFFECTIVE_BRANDING.read().clone();
        apply_brand_css_vars(&brand);
        apply_favicon(&brand);
        apply_document_title(&brand);
    });
}

/// MAPPS-635 D1: point the browser tab title at the brand's
/// `display_name` (or `company_name` as a fallback) whenever the
/// signal changes. On clear, restore the coded default "Mokosh
/// Platform" so a returning visitor on a signed-out page reads the
/// vendor default, not the last brand they saw.
///
/// Individual pages that already call `use_page_title(...)` still
/// override the title AFTER this hook runs on the same render tick;
/// this hook is the neutral shell fallback for pages that do not
/// set their own title (login shell, dashboards where the tab title
/// is not carrying page-specific context).
#[cfg(target_arch = "wasm32")]
pub fn apply_document_title(brand: &EffectiveBranding) {
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let title = brand
        .display_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(brand.company_name.as_deref().filter(|s| !s.is_empty()))
        .unwrap_or("Mokosh Platform");
    doc.set_title(title);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_document_title(_brand: &EffectiveBranding) {}

#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(target_arch = "wasm32")]
pub fn apply_favicon(brand: &EffectiveBranding) {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Ok(nodes) = doc.query_selector_all("link[rel~=\"icon\"]") else {
        return;
    };
    let target = brand.favicon_url.as_deref().filter(|s| !s.is_empty());
    // Default hrefs restored on clear. Kept in sync with index.html.
    const DEFAULT_ICONS: &[(&str, &str)] = &[
        ("/favicon.svg", "image/svg+xml"),
        ("/favicon.ico", "image/x-icon"),
    ];
    for i in 0..nodes.length() {
        let Some(node) = nodes.item(i) else { continue };
        let Ok(el) = node.dyn_into::<web_sys::Element>() else {
            continue;
        };
        if let Some(url) = target {
            // MAPPS-767: absolute, or the browser asks the SPA origin for a
            // favicon the API serves.
            let versioned = absolute_versioned_asset_url(url, brand);
            let _ = el.set_attribute("href", &versioned);
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

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_favicon(_brand: &EffectiveBranding) {}

/// MAPPS-767: a branding asset URL has to name the host that serves it.
/// MAPPS-1238: the version check, the stale-override decision and the style
/// writes that used to be discarded.
#[cfg(test)]
mod mapps_1238_tests {
    use super::*;

    fn brand(primary: Option<&str>, bg: Option<&str>) -> EffectiveBranding {
        EffectiveBranding {
            primary_color: primary.map(str::to_string),
            background_color: bg.map(str::to_string),
            ..EffectiveBranding::default()
        }
    }

    #[test]
    fn only_a_v_key_in_the_query_counts_as_already_versioned() {
        assert!(has_version_param("/logo?v=1"));
        assert!(has_version_param("/logo?a=1&v=2"));
        assert!(has_version_param("/logo?v"));
        for url in [
            "/logo?rev=2",
            "/dev=1/logo",
            "/logo?av=3",
            "/logo",
            "/logo#v=1",
        ] {
            assert!(!has_version_param(url), "{url}");
        }
    }

    #[test]
    fn a_url_with_a_lookalike_param_is_still_versioned() {
        let b = EffectiveBranding::default();
        let out = versioned_asset_url("/logo?rev=2", &b);
        assert!(out.starts_with("/logo?rev=2&v="), "{out}");
        assert_eq!(versioned_asset_url("/logo?v=9", &b), "/logo?v=9");
    }

    #[test]
    fn a_brand_without_colours_undoes_the_previous_brands() {
        assert_eq!(
            stale_brand_overrides("#111", "#222", &brand(None, None)),
            (true, true)
        );
        assert_eq!(
            stale_brand_overrides("#111", "#222", &brand(Some(""), Some(""))),
            (true, true)
        );
        assert_eq!(
            stale_brand_overrides("#111", "#222", &brand(Some("#333"), Some("#444"))),
            (false, false)
        );
        assert_eq!(
            stale_brand_overrides("", "", &brand(None, None)),
            (false, false),
            "nothing painted, nothing to undo"
        );
    }

    #[test]
    fn each_function_carries_its_own_doc() {
        let src = include_str!("branding.rs");
        let abs = src
            .find("pub fn absolute_versioned_asset_url(")
            .expect("absolute");
        let doc = &src[src[..abs].rfind("\n\n").expect("blank line before the doc")..abs];
        assert!(!doc.contains("MAPPS-635 A"), "{doc}");
        let ver = src.find("pub fn versioned_asset_url(").expect("versioned");
        assert!(src[..ver]
            .trim_end()
            .ends_with("`/api/v1/public/tenants/{id}/logo?v=<hash>`."));
    }

    #[test]
    fn brand_css_writes_are_never_discarded() {
        let src = include_str!("branding.rs");
        let code = &src[..src.find("mod mapps_1238_tests").expect("this module")];
        let start = code.find("pub fn apply_brand_css_vars(").expect("apply");
        let end = code
            .find("/// Root-mounted hook that repaints")
            .expect("end of clear");
        let region = &code[start..end];
        assert!(!region.contains("let _ = style."), "{region}");
        assert!(!region.contains("let _ = body_style."), "{region}");
        assert!(region.contains("if drop_bg {"), "{region}");
        assert!(region.contains("if restore_accent {"), "{region}");
    }
}

#[cfg(test)]
mod asset_url_tests {
    use super::{absolute_versioned_asset_url, versioned_asset_url, EffectiveBranding};

    fn brand() -> EffectiveBranding {
        EffectiveBranding::default()
    }

    /// The stored value is a PATH from the API origin, and every deployed
    /// environment serves the SPA from a different host. Rendering the path
    /// directly asked `msp.a8n.systems` for a logo that lives on
    /// `api.msp.a8n.systems`, which is the reported 404.
    ///
    /// Off-web there is no origin to join, so the path is returned as-is and
    /// this asserts the version tag survives rather than the host, which is
    /// the half that can be checked without a browser.
    #[test]
    fn a_path_keeps_its_version_tag() {
        let b = brand();
        let out = absolute_versioned_asset_url("/api/v1/public/tenants/abc/logo", &b);
        assert!(out.contains("/api/v1/public/tenants/abc/logo"), "{out}");
        assert!(out.contains("?v="), "the cache-buster must survive: {out}");
    }

    /// A value that is already absolute is never prefixed. An operator
    /// pointing branding at an externally hosted mark is not this function's
    /// business, and a second origin in front of it would break what worked.
    #[test]
    fn an_absolute_url_is_left_alone() {
        let b = brand();
        for url in [
            "https://cdn.example/logo.png",
            "http://cdn.example/logo.png",
        ] {
            let out = absolute_versioned_asset_url(url, &b);
            assert!(out.starts_with(url), "{out}");
            assert_eq!(out.matches("http").count(), 1, "prefixed twice: {out}");
        }
    }

    /// Empty in, empty out, so a brand with no logo renders nothing rather
    /// than a bare origin that would 404 on its own.
    #[test]
    fn no_logo_yields_no_url() {
        let b = brand();
        assert_eq!(absolute_versioned_asset_url("", &b), "");
        assert_eq!(absolute_versioned_asset_url("   ", &b), "");
    }

    /// The versioning half is unchanged: same brand, same URL, so the browser
    /// cache still hits between renders (MAPPS-635).
    #[test]
    fn the_version_is_stable_for_one_brand() {
        let b = brand();
        let first = absolute_versioned_asset_url("/api/v1/public/tenants/abc/logo", &b);
        let second = absolute_versioned_asset_url("/api/v1/public/tenants/abc/logo", &b);
        assert_eq!(first, second);
        assert!(!versioned_asset_url("/x", &b).is_empty());
    }

    /// A URL that already carries a version is not tagged twice, which the
    /// underlying helper guarantees and the absolute wrapper must not undo.
    #[test]
    fn an_already_versioned_url_is_not_tagged_again() {
        let b = brand();
        let out = absolute_versioned_asset_url("/api/v1/public/tenants/abc/logo?v=deadbeef", &b);
        assert_eq!(out.matches("v=").count(), 1, "{out}");
    }
}

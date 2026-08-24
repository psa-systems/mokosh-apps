//! PMS-729: shared client-portal branding hint.
//!
//! The `/portal/host` endpoint returns the MSP's display name plus the
//! full [`PortalBranding`] surface (logos, primary color, support
//! contact, welcome copy, footer text) for the tenant whose slug is
//! derived from the current host. Every portal-side page reads the same
//! hint through this hook, and the fetch itself is a one-shot latched
//! `GlobalSignal` so mounting the login page and the layout back-to-
//! back does not double-fire the request.
//!
//! Slice 2 (PMS-729 phase 2 §6): on a successful fetch the hint is
//! also applied to the DOM so the whole SPA recolors and re-brands
//! without a per-page hook. Specifically:
//!
//! - `--accent` (and, in dark mode, its dark variant) is overridden
//!   with the tenant's `primary_color` when that color passes WCAG AA
//!   against white text. Colors that fail the contrast gate are
//!   dropped on the floor: the accent picker default holds, and a
//!   `warn!` fires so a devtools-open developer sees the reason.
//! - The `<link rel="icon">` in `<head>` is created (or replaced) with
//!   the tenant's `favicon_url`, so a browser tab shows the MSP mark
//!   instead of Mokosh's.
//! - `<meta name="theme-color">` picks up the primary color so mobile
//!   chrome tints itself with the MSP color.
//!
//! Everything else the hint carries (welcome copy, support contact,
//! footer text, logo variants) is rendered directly by the page or
//! layout that wants it.

use dioxus::prelude::*;
use serde::Deserialize;

/// Every field the server's `PortalBranding` DTO carries. All optional:
/// a tenant that filled in nothing shows the generic Mokosh branding,
/// and a tenant that filled in only a subset gets partial personalisation
/// with the defaults filling the rest.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PortalBranding {
    #[serde(default)]
    pub logo_url: Option<String>,
    #[serde(default)]
    pub logo_url_dark: Option<String>,
    #[serde(default)]
    pub favicon_url: Option<String>,
    #[serde(default)]
    pub primary_color: Option<String>,
    #[serde(default)]
    pub primary_color_dark: Option<String>,
    #[serde(default)]
    pub support_email: Option<String>,
    #[serde(default)]
    pub support_phone: Option<String>,
    #[serde(default)]
    pub support_hours: Option<String>,
    #[serde(default)]
    pub footer_text: Option<String>,
    #[serde(default)]
    pub welcome_message: Option<String>,
}

impl PortalBranding {
    /// Pick the logo appropriate for the resolved theme. Dark-mode
    /// falls back to the light logo when the tenant did not supply a
    /// dark variant.
    pub fn logo_for(&self, is_dark: bool) -> Option<&str> {
        if is_dark {
            self.logo_url_dark.as_deref().or(self.logo_url.as_deref())
        } else {
            self.logo_url.as_deref()
        }
    }
}

/// The branding hint the SPA displays across every portal page. Matches
/// mokosh-server's `PortalHostHint`, which flattens the `PortalBranding`
/// surface onto the top level of the JSON body.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PortalHostHint {
    pub name: String,
    /// MAPPS-559: raw `tenants.status` value (`'active' | 'suspended'
    /// | 'cancelled'`). Portal pages gate on `is_active()` and render
    /// a "This client is suspended" splash instead of the sign-in
    /// form when the tenant is not accepting logins. Defaults to
    /// "active" so a pre-559 server response (no `status` field)
    /// keeps behaving like today.
    #[serde(default = "portal_host_hint_default_status")]
    pub status: String,
    #[serde(flatten)]
    pub branding: PortalBranding,
}

fn portal_host_hint_default_status() -> String {
    "active".to_string()
}

impl PortalHostHint {
    /// Back-compat helper for call sites that still read the logo off
    /// the hint directly (the SPA had a `hint.logo_url` field before
    /// slice 1 promoted branding into its own struct). Delegates to
    /// `branding.logo_url` so the shape change is transparent.
    pub fn logo_url(&self) -> Option<&str> {
        self.branding.logo_url.as_deref()
    }

    /// MAPPS-559: whether the tenant is currently accepting portal
    /// traffic. Anything other than `'active'` (suspended, cancelled,
    /// or a future unknown value) reads as inactive so the sign-in
    /// form gates closed by default.
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

/// The current portal-host branding hint, shared across pages. `None`
/// before the fetch completes, on a non-portal host (the SPA on the
/// agent origin, where /portal/host 404s), and on any transport
/// error. `Some(hint)` on a successful resolve.
#[cfg(feature = "web")]
pub static PORTAL_HOST_HINT: GlobalSignal<Option<PortalHostHint>> = Signal::global(|| None);

/// One-shot latch: `true` once the fetch has been kicked off in this
/// tab so components mounted later do not re-fire the same request.
#[cfg(feature = "web")]
pub static HAS_FETCHED_HINT: GlobalSignal<bool> = Signal::global(|| false);

/// Kick off the `/portal/host` fetch if the current host looks like a
/// portal host and the fetch has not already been started. Idempotent:
/// safe to call from the login page's `use_future` AND from
/// `PortalLayout::use_future` without producing duplicate requests.
///
/// Runs only inside `feature = "web"`; on non-web builds this is a no-op
/// so call sites do not need their own `cfg` gate.
#[cfg(feature = "web")]
pub fn ensure_portal_branding_fetch() {
    use crate::hooks::fetch::api::{get_typed, on_portal_host, ApiError};

    if !on_portal_host() {
        return;
    }
    if *HAS_FETCHED_HINT.peek() {
        return;
    }
    *HAS_FETCHED_HINT.write() = true;

    spawn(async move {
        match get_typed::<PortalHostHint>("/portal/host").await {
            Ok(hint) => {
                apply_portal_branding(&hint.branding);
                *PORTAL_HOST_HINT.write() = Some(hint);
            }
            // 404 is the fail-closed shape for "not a portal host". Any
            // other error also leaves the hint unset so the UI falls
            // back to the generic layout.
            Err(ApiError::Status { code: 404, .. }) => {}
            Err(_) => {}
        }
    });
}

#[cfg(not(feature = "web"))]
pub fn ensure_portal_branding_fetch() {}

/// Read a cloned snapshot of the current hint. Registers a reactive
/// dependency: components that call this re-render when the hint flips
/// from `None` to `Some`.
#[cfg(feature = "web")]
pub fn use_portal_host_hint() -> Option<PortalHostHint> {
    PORTAL_HOST_HINT.read().clone()
}

#[cfg(not(feature = "web"))]
pub fn use_portal_host_hint() -> Option<PortalHostHint> {
    None
}

/// Apply a resolved branding surface to the live document. Called once
/// on the successful `/portal/host` fetch; the effect is a bag of
/// side-effects on `<html>` + `<head>` (CSS custom properties, favicon,
/// theme-color meta), all of which persist until the tab closes.
///
/// The primary-color override is contrast-gated: colors that would
/// leave white button text below WCAG AA on the accent fill are dropped
/// silently (the built-in accent holds). Same policy on the dark
/// variant against a near-black on-accent foreground.
#[cfg(feature = "web")]
pub fn apply_portal_branding(branding: &PortalBranding) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };

    apply_primary_color(&doc, branding);
    apply_favicon(&doc, branding.favicon_url.as_deref());
    apply_theme_color(&doc, branding.primary_color.as_deref());
}

#[cfg(not(feature = "web"))]
pub fn apply_portal_branding(_branding: &PortalBranding) {}

#[cfg(feature = "web")]
fn apply_primary_color(doc: &web_sys::Document, branding: &PortalBranding) {
    use wasm_bindgen::JsCast;
    let Some(html) = doc
        .document_element()
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let style = html.style();

    if let Some(primary) = branding.primary_color.as_deref() {
        if accent_passes_contrast(primary, "#ffffff") {
            let _ = style.set_property("--accent", primary);
            let _ = style.set_property("--on-accent", "#ffffff");
        } else {
            tracing::warn!(
                "portal branding: primary_color {primary} fails WCAG AA vs white; \
                 falling back to built-in accent"
            );
        }
    }

    if let Some(dark) = branding.primary_color_dark.as_deref() {
        // Dark variant sits above near-black text in the Mokosh dark
        // palette; validate against #0a0a0a which is the darkest
        // on-accent shade the built-in accents ship (see
        // `crate::modules::theme::accents`).
        if accent_passes_contrast(dark, "#0a0a0a") {
            let _ = style.set_property("--accent-dark", dark);
        } else {
            tracing::warn!(
                "portal branding: primary_color_dark {dark} fails WCAG AA vs near-black; \
                 dropping override"
            );
        }
    }
}

#[cfg(feature = "web")]
fn apply_favicon(doc: &web_sys::Document, favicon_url: Option<&str>) {
    let Some(url) = favicon_url else {
        return;
    };
    let head = match doc.head() {
        Some(h) => h,
        None => return,
    };
    // Remove any pre-existing `<link rel="icon">` (there is exactly one
    // in Mokosh's `index.html` head; a tenant favicon replaces it).
    let existing = doc.query_selector_all("link[rel~='icon']").ok();
    if let Some(list) = existing {
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                let _ = head.remove_child(&node);
            }
        }
    }
    let link = match doc.create_element("link") {
        Ok(el) => el,
        Err(_) => return,
    };
    let _ = link.set_attribute("rel", "icon");
    let _ = link.set_attribute("href", url);
    let _ = head.append_child(&link);
}

#[cfg(feature = "web")]
fn apply_theme_color(doc: &web_sys::Document, primary_color: Option<&str>) {
    let Some(color) = primary_color else {
        return;
    };
    let head = match doc.head() {
        Some(h) => h,
        None => return,
    };
    let existing = doc
        .query_selector("meta[name='theme-color']")
        .ok()
        .flatten();
    let meta = match existing {
        Some(m) => m,
        None => match doc.create_element("meta") {
            Ok(el) => {
                let _ = el.set_attribute("name", "theme-color");
                let _ = head.append_child(&el);
                el
            }
            Err(_) => return,
        },
    };
    let _ = meta.set_attribute("content", color);
}

/// True when `fg` on `bg` clears WCAG AA (4.5:1). Wrapper around the
/// existing pure contrast helper so a mis-typed hex ("bright pink") is
/// treated as failing rather than silently applied.
pub fn accent_passes_contrast(fg: &str, bg: &str) -> bool {
    use crate::modules::theme::contrast;
    contrast::contrast_ratio(fg, bg).is_some_and(|r| r >= contrast::WCAG_AA_NORMAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_for_dark_falls_back_to_light() {
        let b = PortalBranding {
            logo_url: Some("/light.svg".to_string()),
            ..Default::default()
        };
        assert_eq!(b.logo_for(false), Some("/light.svg"));
        assert_eq!(b.logo_for(true), Some("/light.svg"));
    }

    #[test]
    fn logo_for_dark_prefers_dark_variant() {
        let b = PortalBranding {
            logo_url: Some("/light.svg".to_string()),
            logo_url_dark: Some("/dark.svg".to_string()),
            ..Default::default()
        };
        assert_eq!(b.logo_for(false), Some("/light.svg"));
        assert_eq!(b.logo_for(true), Some("/dark.svg"));
    }

    #[test]
    fn contrast_gate_rejects_light_yellow_on_white() {
        // WCAG contrast ratio ~1.07: unreadable.
        assert!(!accent_passes_contrast("#fef3c7", "#ffffff"));
    }

    #[test]
    fn contrast_gate_accepts_navy_on_white() {
        // ~15:1, well past AA.
        assert!(accent_passes_contrast("#0f172a", "#ffffff"));
    }

    #[test]
    fn contrast_gate_treats_garbage_as_failing() {
        assert!(!accent_passes_contrast("hot pink", "#ffffff"));
        assert!(!accent_passes_contrast("", "#ffffff"));
    }

    #[test]
    fn deserializes_flattened_shape_from_server() {
        // Double `#` delimiters so the `"#123456"` hex literal inside
        // the payload does not close the raw string early.
        let json = r##"{
            "name": "Acme MSP",
            "logo_url": "/l.svg",
            "primary_color": "#123456",
            "welcome_message": "Welcome!"
        }"##;
        let hint: PortalHostHint = serde_json::from_str(json).expect("parse");
        assert_eq!(hint.name, "Acme MSP");
        assert_eq!(hint.branding.logo_url.as_deref(), Some("/l.svg"));
        assert_eq!(hint.branding.primary_color.as_deref(), Some("#123456"));
        assert_eq!(hint.branding.welcome_message.as_deref(), Some("Welcome!"));
        // Back-compat shim still resolves.
        assert_eq!(hint.logo_url(), Some("/l.svg"));
    }

    #[test]
    fn deserializes_bare_hint_with_no_branding_fields() {
        let json = r#"{"name": "Beta MSP"}"#;
        let hint: PortalHostHint = serde_json::from_str(json).expect("parse");
        assert_eq!(hint.name, "Beta MSP");
        assert_eq!(hint.branding, PortalBranding::default());
    }
}

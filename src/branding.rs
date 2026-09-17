//! Deployment branding, resolved at runtime (MAPPS-509).
//!
//! An operator hosting this platform under their own brand sets
//! `MOKOSH_BRAND_NAME`, `MOKOSH_BRAND_LOGO_URL` and `MOKOSH_BRAND_HERO_URL`
//! on the container; `oci-build/entrypoint.sh` emits them into
//! `_mokosh_config.js` and [`crate::modules::runtime_config::get`] reads
//! them here. Nothing is baked in at build time, so branding a deployment
//! never needs a rebuild or a fork. See `docs/deployment-branding.md`.
//!
//! This module holds the ONLY user-visible spellings of the product name
//! and the ONLY references to the built-in logo/hero assets. Every render
//! site calls a helper here, which is what keeps a brand from drifting
//! back into the markup. Internal identifiers (log targets, storage keys,
//! env-var prefix, API paths, module docs) are unaffected and keep saying
//! Mokosh.
//!
//! The pure `*_for` / `default_*` halves exist so the fallback path is
//! unit-testable without a host to configure. Since MAPPS-504
//! `runtime_config::get` reads `window.__MOKOSH_CONFIG__` in the browser
//! and a `config.json` on the desktop, so the same three keys brand a
//! desktop install; on the host test build it simply finds nothing and
//! the fallbacks below are what render.

use dioxus::prelude::*;

use crate::modules::runtime_config;

/// Product name when the deployment sets no `MOKOSH_BRAND_NAME`.
const DEFAULT_PRODUCT_NAME: &str = "Mokosh Platform";

/// Alt text for the built-in hero artwork. Only correct while that
/// artwork is what renders, hence [`hero_alt`].
const DEFAULT_HERO_ALT: &str = "Mokosh, the weaver goddess, at her loom";

/// Intrinsic pixel dimensions of the built-in hero artwork
/// (`assets/mokosh-hero.png`), pinned here so [`hero_dimensions`] can
/// reserve the correct box before the image loads (MAPPS-861). Only correct
/// while that artwork is what renders, same caveat as [`DEFAULT_HERO_ALT`].
const DEFAULT_HERO_WIDTH: u32 = 642;
const DEFAULT_HERO_HEIGHT: u32 = 760;

/// Pick the operator's value, falling back when it is absent or blank.
fn or_default(configured: Option<String>, fallback: &str) -> String {
    configured
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// The built-in logo, content-hashed by `asset!()` at build time.
fn default_logo_src() -> String {
    asset!("/assets/icon-192.png").to_string()
}

/// The built-in marketing hero, content-hashed by `asset!()` at build time.
fn default_hero_src() -> String {
    asset!("/assets/mokosh-hero.png").to_string()
}

/// Smaller renditions of the built-in hero, generated at build time by
/// `asset!()`'s image pipeline (MAPPS-861) so the `srcset` below can serve a
/// mobile viewport fewer bytes than the 642px original.
fn default_hero_src_320w() -> String {
    asset!(
        "/assets/mokosh-hero.png",
        ImageAssetOptions::new().with_size(ImageSize::Manual {
            width: 320,
            height: 379
        })
    )
    .to_string()
}

fn default_hero_src_480w() -> String {
    asset!(
        "/assets/mokosh-hero.png",
        ImageAssetOptions::new().with_size(ImageSize::Manual {
            width: 480,
            height: 568
        })
    )
    .to_string()
}

/// `srcset` for the built-in hero: the two downscaled renditions plus the
/// original, each labeled with its width so the browser picks the smallest
/// one that still fills the rendered box.
fn default_hero_srcset() -> String {
    format!(
        "{} 320w, {} 480w, {} 642w",
        default_hero_src_320w(),
        default_hero_src_480w(),
        default_hero_src()
    )
}

/// Whether the hero renders the built-in artwork rather than an operator's
/// `brand_hero_url`. The dimensions, `srcset`, and alt text below are only
/// correct for the built-in artwork: an operator's image is a single URL of
/// unknown size, so none of those are derivable for it.
fn using_default_hero(configured_hero: Option<String>) -> bool {
    or_default(configured_hero, "").is_empty()
}

/// Alt text for whichever hero image is actually rendering.
fn hero_alt_for(configured_hero: Option<String>, brand: &str) -> String {
    if using_default_hero(configured_hero) {
        DEFAULT_HERO_ALT.to_string()
    } else {
        brand.to_string()
    }
}

/// Name shown in the tab title, the wordmarks and user-facing copy.
pub fn product_name() -> String {
    or_default(runtime_config::get("brand_name"), DEFAULT_PRODUCT_NAME)
}

/// Logo shown in the app top bar and on the auth screens.
///
/// The default is the content-hashed built-in asset, which is exactly why
/// this field exists: `asset!()` rewrites the filename at build time, so
/// an operator cannot override the logo by mounting `icon-192.png` over
/// the image's web root.
pub fn logo_src() -> String {
    or_default(runtime_config::get("brand_logo_url"), &default_logo_src())
}

/// Hero image on the marketing landing page.
pub fn hero_src() -> String {
    or_default(runtime_config::get("brand_hero_url"), &default_hero_src())
}

/// Alt text for the hero image.
///
/// The built-in alt describes the artwork, so it becomes a lie the moment
/// an operator points `brand_hero_url` at their own image. A branded hero
/// gets the brand name instead.
pub fn hero_alt() -> String {
    hero_alt_for(runtime_config::get("brand_hero_url"), &product_name())
}

/// Intrinsic width/height for the hero `img` tag, so the browser reserves
/// its box before the image loads instead of shifting layout in underneath
/// it (MAPPS-861). `None` for an operator-configured hero: its dimensions
/// aren't known here, and a wrong guess would reserve the wrong box for it.
pub fn hero_dimensions() -> Option<(u32, u32)> {
    using_default_hero(runtime_config::get("brand_hero_url"))
        .then_some((DEFAULT_HERO_WIDTH, DEFAULT_HERO_HEIGHT))
}

/// `srcset` for the hero image, so a small viewport downloads a smaller
/// rendition instead of the full 642px artwork (MAPPS-861). `None` for an
/// operator-configured hero: it is a single URL, so there is nothing to
/// build a `srcset` from.
pub fn hero_srcset() -> Option<String> {
    using_default_hero(runtime_config::get("brand_hero_url")).then(default_hero_srcset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unset (the default deployment: the entrypoint emits no field at
    /// all) resolves to today's built-in brand at every site.
    #[test]
    fn falls_back_to_the_built_in_brand() {
        assert_eq!(or_default(None, DEFAULT_PRODUCT_NAME), "Mokosh Platform");
        assert_eq!(
            hero_alt_for(None, "PSA Systems"),
            "Mokosh, the weaver goddess, at her loom"
        );
        assert!(
            default_logo_src().contains("icon-192"),
            "logo falls back to the built-in asset, got {}",
            default_logo_src()
        );
        assert!(
            default_hero_src().contains("mokosh-hero"),
            "hero falls back to the built-in asset, got {}",
            default_hero_src()
        );
    }

    /// The fallback also covers empty and whitespace-only values: an
    /// operator who declares `MOKOSH_BRAND_NAME=` gets the built-in name,
    /// not a blank wordmark.
    #[test]
    fn blank_configuration_is_the_same_as_unset() {
        assert_eq!(or_default(Some(String::new()), "built-in"), "built-in");
        assert_eq!(or_default(Some("   ".to_string()), "built-in"), "built-in");
        assert_eq!(
            hero_alt_for(Some("  ".to_string()), "PSA Systems"),
            DEFAULT_HERO_ALT
        );
    }

    /// A configured value wins, trimmed. A branded hero drops the alt text
    /// describing artwork that is no longer on the page.
    #[test]
    fn configured_value_overrides_the_fallback() {
        assert_eq!(
            or_default(Some("PSA Systems".into()), "built-in"),
            "PSA Systems"
        );
        assert_eq!(
            or_default(Some("  /branding/logo.svg  ".into()), "built-in"),
            "/branding/logo.svg"
        );
        assert_eq!(
            hero_alt_for(Some("/branding/hero.png".into()), "PSA Systems"),
            "PSA Systems"
        );
    }

    /// The built-in hero's `width`/`height` reserve its intrinsic box
    /// (MAPPS-861 CLS fix); an operator-configured hero is a single URL of
    /// unknown size, so guessing at a box for it would be wrong instead of
    /// merely absent.
    #[test]
    fn hero_dimensions_are_only_known_for_the_built_in_hero() {
        assert!(using_default_hero(None));
        assert!(!using_default_hero(Some("/branding/hero.png".into())));
    }

    /// `srcset` only makes sense for the built-in hero: it is the only one
    /// with pre-generated smaller renditions (MAPPS-861).
    #[test]
    fn hero_srcset_lists_the_built_in_renditions_smallest_first() {
        let srcset = default_hero_srcset();
        let widths: Vec<&str> = srcset
            .split(", ")
            .map(|part| {
                part.rsplit(' ')
                    .next()
                    .expect("each entry ends in a width descriptor")
            })
            .collect();
        assert_eq!(widths, ["320w", "480w", "642w"]);
    }

    /// The helpers read the runtime-config fields the container entrypoint
    /// emits. A rename on either side silently un-brands the deployment,
    /// so the field names are pinned from both ends.
    #[test]
    fn field_names_match_the_container_entrypoint() {
        const SRC: &str = include_str!("branding.rs");
        const ENTRYPOINT: &str = include_str!("../oci-build/entrypoint.sh");
        for (field, env) in [
            ("brand_name", "MOKOSH_BRAND_NAME"),
            ("brand_logo_url", "MOKOSH_BRAND_LOGO_URL"),
            ("brand_hero_url", "MOKOSH_BRAND_HERO_URL"),
        ] {
            assert!(
                SRC.contains(&format!("runtime_config::get(\"{field}\")")),
                "branding reads {field} from runtime config"
            );
            assert!(
                ENTRYPOINT.contains(&format!("printf '{field}\\t%s\\n' \"${{{env}:-}}\"")),
                "entrypoint.sh emits {field} from {env}"
            );
        }
    }

    /// Recurrence gate for MAPPS-509: the product name, the logo and the
    /// hero render from these helpers, never from a literal at the call
    /// site. Rendering a Dioxus component needs a browser, so this is a
    /// source scan, like the MAPPS-428 banner gates.
    #[test]
    fn every_render_site_reads_the_helper() {
        for (file, src) in [
            ("main.rs", include_str!("main.rs")),
            ("layout.rs", include_str!("components/layout.rs")),
            (
                "update_available_banner.rs",
                include_str!("components/update_available_banner.rs"),
            ),
            (
                "theme_picker.rs",
                include_str!("components/theme_picker.rs"),
            ),
            ("home.rs", include_str!("pages/home.rs")),
            ("login.rs", include_str!("pages/login.rs")),
            ("onboarding.rs", include_str!("pages/onboarding.rs")),
            ("profile.rs", include_str!("pages/profile.rs")),
            ("settings.rs", include_str!("pages/settings.rs")),
            ("system_status.rs", include_str!("pages/system_status.rs")),
        ] {
            assert!(
                src.contains("branding::product_name()"),
                "{file} renders the product name via branding::product_name()"
            );
            for literal in ["\"Mokosh Platform\"", "\"Sign in to Mokosh"] {
                assert!(
                    !src.contains(literal),
                    "{file} must not hold the literal {literal}"
                );
            }
        }
        const LAYOUT: &str = include_str!("components/layout.rs");
        const HOME: &str = include_str!("pages/home.rs");
        assert!(
            !LAYOUT.contains("asset!(\"/assets/icon-192.png\")"),
            "the top-bar and auth logos read branding::logo_src()"
        );
        assert!(
            !HOME.contains("asset!(\"/assets/mokosh-hero.png\")"),
            "the marketing hero reads branding::hero_src()"
        );
        assert_eq!(
            LAYOUT.matches("branding::logo_src()").count(),
            2,
            "both logo sites (TopBar and AuthLayout) read the helper"
        );
    }
}

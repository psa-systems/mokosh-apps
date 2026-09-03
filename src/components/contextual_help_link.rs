//! MAPPS-453: a reusable contextual help affordance.
//!
//! Drops beside a feature and deep-links to the matching article on the
//! documentation subdomain, opening in a new tab. Renders nothing when no docs
//! subdomain is configured (`OidcConfig::has_docs`), so an unconfigured deploy
//! shows no dead link rather than a link to a missing site.

use dioxus::prelude::*;

use super::icons::InformationIcon;

/// A small "open the docs for this" link. `article` is the path on the docs
/// subdomain (e.g. `/tickets/sla`), joined to the configured base by
/// `OidcConfig::docs_url`. Nothing renders when no docs base is configured.
#[component]
pub fn ContextualHelpLink(article: String) -> Element {
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    if !cfg.has_docs() {
        return rsx! {};
    }
    let href = cfg.docs_url(&article);
    rsx! {
        a {
            href: "{href}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center text-subtle hover:text-content",
            title: "Open documentation",
            aria_label: "Help: open documentation",
            InformationIcon {}
        }
    }
}

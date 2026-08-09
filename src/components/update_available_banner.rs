//! Full-width "a new version was deployed, reload this page" banner
//! (MAPPS-428).
//!
//! [`crate::hooks::update_check`] already detects that a new SPA build is
//! live, but did so silently: the only feedback a stale tab gave was
//! per-page "Could not load X. Refresh the page to retry." errors, which
//! present an app-wide condition as form-specific data-loading trouble
//! and name a remedy without offering a control for it. This banner is
//! that control.
//!
//! Renders nothing while the loaded bundle is current, following
//! [`crate::components::ServerStatusBanner`] rather than the admin
//! [`crate::components::UpdateBanner`]: no reserved height, no
//! reserve-then-collapse transition, and the Reload button stays out of
//! the tab order until it is actually actionable.
//!
//! Deliberately NOT dismissible. The loaded bundle really is stale and
//! every subsequent request may fail, so a dismiss control would only
//! let the user hide the one accurate explanation for the failures they
//! are about to hit.
//!
//! Distinct from the admin [`crate::components::UpdateBanner`], which
//! stays operator-facing (`docker compose pull` guidance, driven by
//! `GET /api/v1/version` skew). This one is shown to every signed-in
//! user regardless of role. Both can be up at once and say different
//! things, which is correct.

use dioxus::prelude::*;

use crate::hooks::use_update_pending;

/// App-wide banner shown once a newer SPA build has been deployed.
#[component]
pub fn UpdateAvailableBanner() -> Element {
    // Reading the flag subscribes this component, so the banner appears
    // the moment the update check confirms a mismatch. The flag is
    // one-way, so the banner never disappears short of a reload.
    if !use_update_pending() {
        return rsx! {};
    }

    rsx! {
        div {
            // `role=status` + `aria-live=polite` so assistive tech
            // announces the update when the bar appears, without
            // interrupting the user mid-action.
            role: "status",
            aria_live: "polite",
            class: "border-b border-amber-200 dark:border-amber-900 bg-amber-50 dark:bg-amber-950/30 text-amber-900 dark:text-amber-200",
            div {
                class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-2 flex items-center justify-center gap-3 text-sm text-center",
                // User-facing cause only: no build hashes, no version
                // numbers, no operator instructions.
                span { class: "font-medium",
                    "A new version of Mokosh has been deployed. Reload the page to get it."
                }
                button {
                    r#type: "button",
                    class: "shrink-0 rounded px-3 py-1 font-medium border border-amber-300 dark:border-amber-800 bg-amber-100 dark:bg-amber-900/50 hover:bg-amber-200 dark:hover:bg-amber-900",
                    onclick: move |_| crate::hooks::update_check::reload_now(),
                    "Reload page"
                }
            }
        }
    }
}

/// MAPPS-428 recurrence gates. Rendering a Dioxus component needs a
/// browser, so these are source scans over the banner, the shell that
/// mounts it, and the admin banner it must not disturb.
#[cfg(test)]
mod tests {
    const BANNER_SRC: &str = include_str!("update_available_banner.rs");
    const LAYOUT_SRC: &str = include_str!("layout.rs");
    const ADMIN_BANNER_SRC: &str = include_str!("update_banner.rs");

    /// The component body only: this file minus its test module and minus
    /// the module docs above it, which discuss (and therefore quote) the
    /// very strings these gates require the rendered markup not to carry.
    fn banner_src() -> &'static str {
        BANNER_SRC
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields a first segment")
            .split_once("pub fn UpdateAvailableBanner")
            .expect("the component is still declared here")
            .1
    }

    /// The banner is the topmost row of the shell, above the server-status
    /// and admin update banners.
    #[test]
    fn mounted_at_the_top_of_the_app_shell() {
        let update = LAYOUT_SRC
            .find("super::UpdateAvailableBanner {}")
            .expect("UpdateAvailableBanner is mounted in AppShell");
        let server = LAYOUT_SRC
            .find("super::ServerStatusBanner {}")
            .expect("ServerStatusBanner is mounted in AppShell");
        let admin = LAYOUT_SRC
            .find("super::UpdateBanner {}")
            .expect("admin UpdateBanner is mounted in AppShell");
        assert!(
            update < server,
            "update banner must sit above ServerStatusBanner"
        );
        assert!(
            update < admin,
            "update banner must sit above the admin UpdateBanner"
        );
    }

    /// Absent (zero height, no reserved row, no transition) while the
    /// loaded build is current: the healthy path returns an empty `rsx!`
    /// rather than the admin banner's reserve-then-collapse grid.
    #[test]
    fn renders_nothing_while_the_build_is_current() {
        let src = banner_src();
        assert!(src.contains("if !use_update_pending() {\n        return rsx! {};"));
        assert!(
            !src.contains("grid-rows-["),
            "no reserved/collapsing row: the healthy default is absent"
        );
        assert!(
            !src.contains("transition-"),
            "no height transition on the healthy path"
        );
    }

    /// Announced to assistive tech without interrupting the user.
    #[test]
    fn announces_politely() {
        let src = banner_src();
        assert!(src.contains("role: \"status\""));
        assert!(src.contains("aria_live: \"polite\""));
    }

    /// The copy names the cause in user terms. No build hashes, no version
    /// numbers, no operator instructions.
    #[test]
    fn copy_is_user_facing_only() {
        let src = banner_src();
        assert!(src
            .contains("\"A new version of Mokosh has been deployed. Reload the page to get it.\""));
        for forbidden in [
            "docker compose",
            "GIT_HASH",
            "build_sha",
            "{version",
            "{sha",
        ] {
            assert!(
                !src.contains(forbidden),
                "banner copy must not mention {forbidden}"
            );
        }
    }

    /// A real `<button>` (so it is keyboard reachable and focusable by
    /// default) whose text content is its accessible name, wired to the
    /// same reload the automatic path uses.
    #[test]
    fn offers_a_keyboard_reachable_reload_button() {
        let src = banner_src();
        let button = src
            .split_once("button {")
            .expect("the banner renders a button element")
            .1;
        assert!(
            button.contains("\"Reload page\""),
            "button carries its accessible name"
        );
        assert!(button.contains("crate::hooks::update_check::reload_now()"));
        assert!(
            !src.contains("tabindex"),
            "no tabindex override: the native button is already in the tab order"
        );
    }

    /// Not dismissible: the bundle really is stale, so hiding the banner
    /// would only hide the explanation for the failures that follow.
    #[test]
    fn has_no_dismiss_control() {
        let src = banner_src().to_lowercase();
        for forbidden in ["dismiss", "xmarkicon", "close"] {
            assert!(
                !src.contains(forbidden),
                "banner must not offer {forbidden}"
            );
        }
    }

    /// The admin banner keeps its own trigger, operator copy, and
    /// dismissal behaviour: this banner is additive, not a replacement.
    #[test]
    fn admin_update_banner_keeps_its_own_behaviour() {
        assert!(ADMIN_BANNER_SRC.contains("docker compose pull && docker compose up --detach"));
        assert!(ADMIN_BANNER_SRC.contains("aria_label: \"Dismiss update notification\""));
        assert!(ADMIN_BANNER_SRC.contains("get_version()"));
        assert!(
            !ADMIN_BANNER_SRC.contains("use_update_pending"),
            "the admin banner keeps its /api/v1/version trigger"
        );
    }
}

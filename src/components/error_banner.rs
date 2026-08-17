//! Shared inline status banner (MAPPS-418, MAPPS-439).
//!
//! One home for the compact banner recipe that was hand-rolled across ~13
//! pages (and diverged into a few tints). Callers pass the message as
//! `children` and, optionally, their own spacing via `class` (e.g. `mb-3`).
//! Unlike the heavier icon + title [`crate::components::Alert`], this is the
//! icon-less, single-line banner shown above a form or list.
//!
//! MAPPS-418 landed the red one only, so the success, warning and info states
//! stayed hand-rolled in six different recipes. [`StatusBanner`] generalizes it
//! over [`BannerTone`]: one shape, the hue swapped per tone (MAPPS-412 recipe:
//! `dark:bg-{hue}-950/30` + `dark:border-{hue}-900`). [`ErrorBanner`] is a thin
//! alias over `BannerTone::Error` so the existing call sites are untouched.

use dioxus::prelude::*;

/// Which state an inline banner reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BannerTone {
    #[default]
    Error,
    Success,
    Warning,
    Info,
}

impl BannerTone {
    /// The base recipe, hue swapped per tone. Spelled out in full because
    /// Tailwind scans the source for literal class names.
    fn class(self) -> &'static str {
        match self {
            Self::Error => "rounded-md border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 px-3 py-2 text-sm text-red-700 dark:text-red-300",
            Self::Success => "rounded-md border border-green-200 dark:border-green-900 bg-green-50 dark:bg-green-950/30 px-3 py-2 text-sm text-green-700 dark:text-green-300",
            Self::Warning => "rounded-md border border-amber-200 dark:border-amber-900 bg-amber-50 dark:bg-amber-950/30 px-3 py-2 text-sm text-amber-700 dark:text-amber-300",
            Self::Info => "rounded-md border border-blue-200 dark:border-blue-900 bg-blue-50 dark:bg-blue-950/30 px-3 py-2 text-sm text-blue-700 dark:text-blue-300",
        }
    }

    /// `alert` interrupts the screen reader, which only the error state earns.
    /// The other outcomes are announced politely as `status`, matching how
    /// `portal_set_password.rs` and `request_form.rs` already mark them.
    fn role(self) -> &'static str {
        match self {
            Self::Error => "alert",
            Self::Success | Self::Warning | Self::Info => "status",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct StatusBannerProps {
    /// Which state the banner reports. Defaults to [`BannerTone::Error`].
    #[props(default)]
    tone: BannerTone,
    children: Element,
    /// Extra classes appended after the base recipe (e.g. caller margin "mb-3"/"mb-4").
    #[props(default)]
    class: String,
}

/// Compact inline status banner (MAPPS-439). Renders the standardized recipe
/// for `tone`; the message is passed as `children`.
#[component]
pub fn StatusBanner(props: StatusBannerProps) -> Element {
    let base = props.tone.class();
    let role = props.tone.role();
    let class = props.class;
    rsx! {
        div { role, class: "{base} {class}", {props.children} }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ErrorBannerProps {
    children: Element,
    /// Extra classes appended after the base recipe (e.g. caller margin "mb-3"/"mb-4").
    #[props(default)]
    class: String,
}

/// Compact inline error banner (MAPPS-418): [`StatusBanner`] at
/// [`BannerTone::Error`], kept as its own name for the existing call sites.
#[component]
pub fn ErrorBanner(props: ErrorBannerProps) -> Element {
    rsx! {
        StatusBanner { tone: BannerTone::Error, class: props.class, {props.children} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tone renders the same shape with only the hue swapped, which is
    /// the whole reason the four banners collapsed into one component.
    #[test]
    fn every_tone_shares_the_recipe_and_differs_only_in_hue() {
        let shape = ["rounded-md", "border", "px-3", "py-2", "text-sm"];
        for (tone, hue) in [
            (BannerTone::Error, "red"),
            (BannerTone::Success, "green"),
            (BannerTone::Warning, "amber"),
            (BannerTone::Info, "blue"),
        ] {
            let class = tone.class();
            for part in shape {
                assert!(class.contains(part), "{tone:?} is missing `{part}`");
            }
            for part in [
                format!("border-{hue}-200"),
                format!("dark:border-{hue}-900"),
                format!("bg-{hue}-50"),
                format!("dark:bg-{hue}-950/30"),
                format!("text-{hue}-700"),
                format!("dark:text-{hue}-300"),
            ] {
                assert!(class.contains(&part), "{tone:?} is missing `{part}`");
            }
        }
    }

    /// Only the error state interrupts; the rest are announced politely.
    #[test]
    fn only_error_is_an_alert() {
        assert_eq!(BannerTone::Error.role(), "alert");
        for tone in [BannerTone::Success, BannerTone::Warning, BannerTone::Info] {
            assert_eq!(tone.role(), "status", "{tone:?} should not interrupt");
        }
    }
}

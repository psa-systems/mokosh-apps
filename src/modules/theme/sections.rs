//! Per-nav-area section colors (MAPPS-359).
//!
//! A small, fixed palette that ties each product area to one hue, applied
//! as an accent (icon / heading tint, left-border) rather than a flood.
//! First introduced privately in the Settings landing pages (MAPPS-257) to
//! color each group's cards; promoted here so the sidebar categories
//! (`src/components/layout.rs`) and the Settings landings share ONE source
//! of truth, which is what keeps a domain's color identical in the nav rail
//! and in its Settings cards (the "styling is consistent between settings
//! and the rest of the app" requirement).
//!
//! Pure data + Tailwind class fragments, no web_sys, so it is
//! native-testable alongside the rest of the theme module. Every variant
//! ships a lighter dark-mode shade so both base modes keep adequate
//! contrast; the tests below enforce that both a light and a `dark:` class
//! are always present.

/// One nav-area accent hue. The five original hues (Emerald, Amber, Blue,
/// Violet, Rose) come from the Settings taxonomy (MAPPS-257); the four
/// added for MAPPS-359 (Indigo, Cyan, Teal, Fuchsia) give every top-level
/// sidebar category its own distinct hue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectionColor {
    /// Service Desk (the existing active-nav accent hue).
    Blue,
    /// Projects.
    Indigo,
    /// CRM.
    Cyan,
    /// Operations / Service & Asset Types.
    Emerald,
    /// Assets.
    Teal,
    /// Contracts & Billing / SLA.
    Amber,
    /// Knowledge.
    Fuchsia,
    /// Analytics; also Personalization in Settings (per-user, not a nav
    /// domain, so it stands apart in its own hue).
    Rose,
    /// Admin / Integrations.
    Violet,
}

impl SectionColor {
    /// Heading / icon tint, with a lighter dark-mode variant for contrast.
    pub fn heading_class(self) -> &'static str {
        match self {
            SectionColor::Blue => "text-blue-600 dark:text-blue-400",
            SectionColor::Indigo => "text-indigo-600 dark:text-indigo-400",
            SectionColor::Cyan => "text-cyan-600 dark:text-cyan-400",
            SectionColor::Emerald => "text-emerald-600 dark:text-emerald-400",
            SectionColor::Teal => "text-teal-600 dark:text-teal-400",
            SectionColor::Amber => "text-amber-600 dark:text-amber-400",
            SectionColor::Fuchsia => "text-fuchsia-600 dark:text-fuchsia-400",
            SectionColor::Rose => "text-rose-600 dark:text-rose-400",
            SectionColor::Violet => "text-violet-600 dark:text-violet-400",
        }
    }

    /// Colored left-accent border: base + same-family hover, each with a
    /// lighter dark-mode variant. Used for a card's or an active nav row's
    /// left edge.
    pub fn card_border_class(self) -> &'static str {
        match self {
            SectionColor::Blue => "border-l-blue-500 hover:border-l-blue-400 dark:border-l-blue-400 dark:hover:border-l-blue-300",
            SectionColor::Indigo => "border-l-indigo-500 hover:border-l-indigo-400 dark:border-l-indigo-400 dark:hover:border-l-indigo-300",
            SectionColor::Cyan => "border-l-cyan-500 hover:border-l-cyan-400 dark:border-l-cyan-400 dark:hover:border-l-cyan-300",
            SectionColor::Emerald => "border-l-emerald-500 hover:border-l-emerald-400 dark:border-l-emerald-400 dark:hover:border-l-emerald-300",
            SectionColor::Teal => "border-l-teal-500 hover:border-l-teal-400 dark:border-l-teal-400 dark:hover:border-l-teal-300",
            SectionColor::Amber => "border-l-amber-500 hover:border-l-amber-400 dark:border-l-amber-400 dark:hover:border-l-amber-300",
            SectionColor::Fuchsia => "border-l-fuchsia-500 hover:border-l-fuchsia-400 dark:border-l-fuchsia-400 dark:hover:border-l-fuchsia-300",
            SectionColor::Rose => "border-l-rose-500 hover:border-l-rose-400 dark:border-l-rose-400 dark:hover:border-l-rose-300",
            SectionColor::Violet => "border-l-violet-500 hover:border-l-violet-400 dark:border-l-violet-400 dark:hover:border-l-violet-300",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant, so a new hue is automatically exercised by the
    /// class-shape tests below.
    const ALL: &[SectionColor] = &[
        SectionColor::Blue,
        SectionColor::Indigo,
        SectionColor::Cyan,
        SectionColor::Emerald,
        SectionColor::Teal,
        SectionColor::Amber,
        SectionColor::Fuchsia,
        SectionColor::Rose,
        SectionColor::Violet,
    ];

    #[test]
    fn heading_class_has_light_and_dark_variants() {
        // MAPPS-359 AC: the themed chrome must render in BOTH base modes, so
        // every hue carries a base (light) tint and a `dark:` override.
        for c in ALL {
            let cls = c.heading_class();
            assert!(
                cls.contains("text-") && !cls.starts_with("dark:"),
                "{c:?} heading_class is missing a light-mode tint: {cls}"
            );
            assert!(
                cls.contains("dark:text-"),
                "{c:?} heading_class is missing a dark-mode tint: {cls}"
            );
        }
    }

    #[test]
    fn card_border_class_has_light_and_dark_variants() {
        for c in ALL {
            let cls = c.card_border_class();
            assert!(
                cls.contains("border-l-") && !cls.starts_with("dark:"),
                "{c:?} card_border_class is missing a light-mode border: {cls}"
            );
            assert!(
                cls.contains("dark:border-l-"),
                "{c:?} card_border_class is missing a dark-mode border: {cls}"
            );
        }
    }

    #[test]
    fn hues_are_distinct() {
        // Distinct hue per variant, so two categories never collide.
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(
                    a.heading_class(),
                    b.heading_class(),
                    "{a:?} and {b:?} share a heading tint"
                );
            }
        }
    }
}

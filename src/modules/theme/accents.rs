//! Curated accent catalog (MAPPS-259).
//!
//! A wide-gamut but bounded set of accents spanning the color wheel.
//! Each accent ships a canonical 50..950 ramp (used for accent tints:
//! `bg-accent-100`, `text-accent-700`, ...) plus a per-base variant: the
//! `fill` (the accent surface, e.g. a primary button background) and the
//! `on_accent` foreground that sits on it. Light fills are dark shades
//! with white text; dark fills are bright shades with a near-black text,
//! so the accent reads correctly on either base.
//!
//! Every variant is required to meet WCAG AA (see the test below), which
//! is what makes the picker's "auto-fit shade per base" hold: selecting
//! an accent swaps in the base-appropriate fill/on_accent, and the
//! runtime contrast check (contrast.rs) only ever locks a hue if a
//! future addition or phase-2 palette regresses below AA.

/// Per-base accent surface + the foreground that sits on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    /// Accent fill (primary button background, active indicator, etc.).
    pub fill: &'static str,
    /// Foreground color placed on `fill` (button label, icon).
    pub on_accent: &'static str,
}

/// One curated accent: a stable id, a display name, the canonical ramp,
/// and the Light/Dark variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    pub id: &'static str,
    pub name: &'static str,
    /// 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950.
    pub ramp: [&'static str; 11],
    pub light: Variant,
    pub dark: Variant,
}

/// Default accent id (Teal). Matches the CSS default in `input.css`.
pub const DEFAULT_ACCENT_ID: &str = "teal";

/// The curated set, ordered roughly around the wheel.
pub const ACCENTS: &[Accent] = &[
    Accent {
        id: "red",
        name: "Red",
        ramp: [
            "#fef2f2", "#fee2e2", "#fecaca", "#fca5a5", "#f87171", "#ef4444", "#dc2626", "#b91c1c",
            "#991b1b", "#7f1d1d", "#450a0a",
        ],
        light: Variant {
            fill: "#b91c1c",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#f87171",
            on_accent: "#450a0a",
        },
    },
    Accent {
        id: "orange",
        name: "Orange",
        ramp: [
            "#fff7ed", "#ffedd5", "#fed7aa", "#fdba74", "#fb923c", "#f97316", "#ea580c", "#c2410c",
            "#9a3412", "#7c2d12", "#431407",
        ],
        light: Variant {
            fill: "#9a3412",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#fb923c",
            on_accent: "#431407",
        },
    },
    Accent {
        id: "amber",
        name: "Amber",
        ramp: [
            "#fffbeb", "#fef3c7", "#fde68a", "#fcd34d", "#fbbf24", "#f59e0b", "#d97706", "#b45309",
            "#92400e", "#78350f", "#451a03",
        ],
        light: Variant {
            fill: "#92400e",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#fbbf24",
            on_accent: "#451a03",
        },
    },
    Accent {
        id: "lime",
        name: "Lime",
        ramp: [
            "#f7fee7", "#ecfccb", "#d9f99d", "#bef264", "#a3e635", "#84cc16", "#65a30d", "#4d7c0f",
            "#3f6212", "#365314", "#1a2e05",
        ],
        light: Variant {
            fill: "#3f6212",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#a3e635",
            on_accent: "#1a2e05",
        },
    },
    Accent {
        id: "green",
        name: "Green",
        ramp: [
            "#f0fdf4", "#dcfce7", "#bbf7d0", "#86efac", "#4ade80", "#22c55e", "#16a34a", "#15803d",
            "#166534", "#14532d", "#052e16",
        ],
        light: Variant {
            fill: "#15803d",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#4ade80",
            on_accent: "#052e16",
        },
    },
    Accent {
        id: "emerald",
        name: "Emerald",
        ramp: [
            "#ecfdf5", "#d1fae5", "#a7f3d0", "#6ee7b7", "#34d399", "#10b981", "#059669", "#047857",
            "#065f46", "#064e3b", "#022c22",
        ],
        light: Variant {
            fill: "#047857",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#34d399",
            on_accent: "#022c22",
        },
    },
    Accent {
        id: "teal",
        name: "Teal",
        ramp: [
            "#f0fdfa", "#ccfbf1", "#99f6e4", "#5eead4", "#2dd4bf", "#14b8a6", "#0d9488", "#0f766e",
            "#115e59", "#134e4a", "#042f2e",
        ],
        light: Variant {
            fill: "#0f766e",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#2dd4bf",
            on_accent: "#042f2e",
        },
    },
    Accent {
        id: "cyan",
        name: "Cyan",
        ramp: [
            "#ecfeff", "#cffafe", "#a5f3fc", "#67e8f9", "#22d3ee", "#06b6d4", "#0891b2", "#0e7490",
            "#155e75", "#164e63", "#083344",
        ],
        light: Variant {
            fill: "#155e75",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#22d3ee",
            on_accent: "#083344",
        },
    },
    Accent {
        id: "blue",
        name: "Blue",
        ramp: [
            "#eff6ff", "#dbeafe", "#bfdbfe", "#93c5fd", "#60a5fa", "#3b82f6", "#2563eb", "#1d4ed8",
            "#1e40af", "#1e3a8a", "#172554",
        ],
        light: Variant {
            fill: "#1d4ed8",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#60a5fa",
            on_accent: "#172554",
        },
    },
    Accent {
        id: "indigo",
        name: "Indigo",
        ramp: [
            "#eef2ff", "#e0e7ff", "#c7d2fe", "#a5b4fc", "#818cf8", "#6366f1", "#4f46e5", "#4338ca",
            "#3730a3", "#312e81", "#1e1b4b",
        ],
        light: Variant {
            fill: "#4338ca",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#818cf8",
            on_accent: "#1e1b4b",
        },
    },
    Accent {
        id: "violet",
        name: "Violet",
        ramp: [
            "#f5f3ff", "#ede9fe", "#ddd6fe", "#c4b5fd", "#a78bfa", "#8b5cf6", "#7c3aed", "#6d28d9",
            "#5b21b6", "#4c1d95", "#2e1065",
        ],
        light: Variant {
            fill: "#6d28d9",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#a78bfa",
            on_accent: "#2e1065",
        },
    },
    Accent {
        id: "fuchsia",
        name: "Fuchsia",
        ramp: [
            "#fdf4ff", "#fae8ff", "#f5d0fe", "#f0abfc", "#e879f9", "#d946ef", "#c026d3", "#a21caf",
            "#86198f", "#701a75", "#4a044e",
        ],
        light: Variant {
            fill: "#a21caf",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#e879f9",
            on_accent: "#4a044e",
        },
    },
    Accent {
        id: "pink",
        name: "Pink",
        ramp: [
            "#fdf2f8", "#fce7f3", "#fbcfe8", "#f9a8d4", "#f472b6", "#ec4899", "#db2777", "#be185d",
            "#9d174d", "#831843", "#500724",
        ],
        light: Variant {
            fill: "#be185d",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#f472b6",
            on_accent: "#500724",
        },
    },
    Accent {
        id: "slate",
        name: "Slate",
        ramp: [
            "#f8fafc", "#f1f5f9", "#e2e8f0", "#cbd5e1", "#94a3b8", "#64748b", "#475569", "#334155",
            "#1e293b", "#0f172a", "#020617",
        ],
        light: Variant {
            fill: "#334155",
            on_accent: "#ffffff",
        },
        dark: Variant {
            fill: "#94a3b8",
            on_accent: "#020617",
        },
    },
];

/// Look up an accent by id.
pub fn by_id(id: &str) -> Option<&'static Accent> {
    ACCENTS.iter().find(|a| a.id == id)
}

/// The default accent (Teal). Falls back to the first entry if the
/// default id is ever removed, so this never panics in a release build.
pub fn default_accent() -> &'static Accent {
    by_id(DEFAULT_ACCENT_ID).unwrap_or(&ACCENTS[0])
}

/// Resolve an accent id to an accent, falling back to the default for an
/// unknown or removed id (matches the spec's error handling).
pub fn resolve(id: &str) -> &'static Accent {
    by_id(id).unwrap_or_else(default_accent)
}

#[cfg(test)]
mod tests {
    use super::super::contrast::passes_aa;
    use super::*;

    #[test]
    fn every_variant_meets_wcag_aa_on_its_base() {
        for a in ACCENTS {
            assert!(
                passes_aa(a.light.on_accent, a.light.fill),
                "{} light: on_accent {} on fill {} fails AA",
                a.id,
                a.light.on_accent,
                a.light.fill
            );
            assert!(
                passes_aa(a.dark.on_accent, a.dark.fill),
                "{} dark: on_accent {} on fill {} fails AA",
                a.id,
                a.dark.on_accent,
                a.dark.fill
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in ACCENTS.iter().enumerate() {
            for b in &ACCENTS[i + 1..] {
                assert_ne!(a.id, b.id, "duplicate accent id {}", a.id);
            }
        }
    }

    #[test]
    fn default_resolves_and_is_present() {
        assert_eq!(default_accent().id, DEFAULT_ACCENT_ID);
        assert!(by_id(DEFAULT_ACCENT_ID).is_some());
    }

    #[test]
    fn unknown_id_falls_back_to_default() {
        assert_eq!(resolve("no-such-accent").id, DEFAULT_ACCENT_ID);
        assert_eq!(resolve("blue").id, "blue");
    }

    #[test]
    fn ramps_are_well_formed_hex() {
        use super::super::contrast::parse_hex;
        for a in ACCENTS {
            for step in a.ramp {
                assert!(parse_hex(step).is_some(), "{} bad ramp hex {}", a.id, step);
            }
            assert!(parse_hex(a.light.fill).is_some());
            assert!(parse_hex(a.dark.fill).is_some());
        }
    }
}

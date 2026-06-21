//! Shared theme picker (MAPPS-259).
//!
//! Two layers: Base mode (Light/Dark/System now; named palettes are a
//! later phase, shown locked) and a curated Accent grid with a live
//! preview. The accent swatch for each hue shows the base-appropriate
//! fill, and a swatch locks only if it would fail WCAG AA on the active
//! base (curated accents never do; this protects future additions and
//! phase-2 palettes). One component, rendered in both the header modal
//! and the Settings > Appearance section.

use dioxus::prelude::*;

use super::icons::SwatchIcon;
use super::modal::{Modal, ModalSize};
use crate::hooks::theme;
use crate::modules::theme::{accents, contrast};

/// Top-bar trigger: a swatch icon that opens the picker in a centered
/// modal. Rendered next to the notification bell in the TopBar. The
/// button uses the nav-chrome palette directly for now (the chrome is
/// migrated to surface tokens in the same MAPPS-259 sweep).
#[component]
pub fn ThemePickerButton() -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        button {
            r#type: "button",
            class: "p-2 rounded-full text-gray-400 hover:text-white hover:bg-gray-700",
            aria_label: "Theme and appearance",
            title: "Appearance",
            onclick: move |_| open.set(true),
            SwatchIcon {}
        }
        Modal {
            open: open(),
            title: "Appearance".to_string(),
            size: ModalSize::Large,
            onclose: move |_| open.set(false),
            ThemePicker {}
        }
    }
}

#[component]
pub fn ThemePicker() -> Element {
    let mut base = use_signal(theme::current);
    let mut is_dark = use_signal(theme::current_is_dark);
    let mut accent_id = use_signal(|| theme::current_accent().id.to_string());

    rsx! {
        div { class: "space-y-6",
            // Layer 1: Base mode
            section { class: "space-y-2",
                h3 { class: "text-xs font-semibold uppercase tracking-wide text-subtle",
                    "Base mode"
                }
                div { class: "inline-flex gap-1 rounded-lg bg-surface-2 p-1",
                    for opt in [theme::Theme::Light, theme::Theme::Dark, theme::Theme::System] {
                        {
                            let cls = if base() == opt {
                                "px-4 py-1.5 rounded-md text-sm font-semibold bg-surface text-content shadow-sm"
                            } else {
                                "px-4 py-1.5 rounded-md text-sm font-medium text-muted hover:text-content"
                            };
                            let label = match opt {
                                theme::Theme::Light => "Light",
                                theme::Theme::Dark => "Dark",
                                theme::Theme::System => "System",
                            };
                            rsx! {
                                button {
                                    key: "{opt.as_str()}",
                                    r#type: "button",
                                    class: "{cls}",
                                    onclick: move |_| {
                                        theme::set(opt);
                                        base.set(opt);
                                        is_dark.set(theme::current_is_dark());
                                        crate::hooks::theme_sync::save_to_account();
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                // Phase-2 named palettes, locked.
                div { class: "flex gap-2 pt-1",
                    for name in ["Sepia", "Midnight", "Forest"] {
                        div {
                            key: "{name}",
                            class: "flex-1 rounded-lg border border-dashed border-line px-3 py-2 text-center text-xs font-medium text-subtle",
                            title: "Named palettes are coming in a later phase",
                            "{name} "
                            span { class: "opacity-60", "(later)" }
                        }
                    }
                }
            }

            // Layer 2: Accent
            section { class: "space-y-2",
                div { class: "flex items-center justify-between",
                    h3 { class: "text-xs font-semibold uppercase tracking-wide text-subtle",
                        "Accent"
                    }
                    // MAPPS-288: hint that dimmed dots are AA-locked. Hover
                    // any disabled accent for the actual contrast ratio.
                    span { class: "text-xs text-subtle", "Dimmed = below WCAG AA on this base" }
                }
                div { class: "grid grid-cols-7 gap-3",
                    for a in accents::ACCENTS {
                        {
                            let variant = if is_dark() { a.dark } else { a.light };
                            let locked = !contrast::passes_aa(variant.on_accent, variant.fill);
                            // MAPPS-288: surface the actual contrast ratio on
                            // hover so a curious user (or an accessibility
                            // reviewer) can see why an accent is locked instead
                            // of just observing it dimmed. The disabled state
                            // already prevents picking a failing combination;
                            // this adds the user-visible "what / why" the AC
                            // calls for.
                            let ratio = contrast::contrast_ratio(variant.on_accent, variant.fill)
                                .unwrap_or(0.0);
                            let tooltip = if locked {
                                format!(
                                    "{} — locked: contrast {:.1}:1 (needs 4.5:1 for WCAG AA)",
                                    a.name, ratio
                                )
                            } else {
                                format!("{} (contrast {:.1}:1)", a.name, ratio)
                            };
                            let selected = accent_id() == a.id;
                            let ring = if selected {
                                "ring-2 ring-offset-2 ring-offset-surface ring-content"
                            } else {
                                ""
                            };
                            let state = if locked {
                                "opacity-30 cursor-not-allowed"
                            } else {
                                "cursor-pointer hover:scale-110 transition-transform"
                            };
                            rsx! {
                                button {
                                    key: "{a.id}",
                                    r#type: "button",
                                    disabled: locked,
                                    title: "{tooltip}",
                                    "aria-label": "{tooltip}",
                                    "aria-pressed": "{selected}",
                                    class: "aspect-square w-full rounded-full {ring} {state}",
                                    style: "background-color: {variant.fill}",
                                    onclick: move |_| {
                                        if !locked {
                                            theme::set_accent(a.id);
                                            accent_id.set(a.id.to_string());
                                            crate::hooks::theme_sync::save_to_account();
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Live preview (reads the same tokens the whole app does).
            section { class: "space-y-2",
                h3 { class: "text-xs font-semibold uppercase tracking-wide text-subtle",
                    "Preview"
                }
                div { class: "flex flex-wrap items-center gap-3 rounded-lg border border-line bg-surface-2 p-4",
                    span { class: "px-3 py-1.5 rounded-md text-sm font-medium border-l-2 border-accent bg-accent-50 text-accent-700 dark:bg-accent-950 dark:text-accent-300",
                        "Tickets"
                    }
                    button { r#type: "button", class: "px-3 py-1.5 rounded-md text-sm font-semibold bg-accent text-on-accent",
                        "Save changes"
                    }
                    a { class: "text-sm font-semibold text-accent underline", "View all" }
                    span { class: "inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium bg-accent-100 text-accent-800 dark:bg-accent-900 dark:text-accent-300",
                        "Open"
                    }
                }
                p { class: "text-xs text-subtle",
                    "Saved to your account. Applies across Mokosh immediately."
                }
            }
        }
    }
}

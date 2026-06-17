//! Theme preference + DOM application.
//!
//! Three settings:
//!  - `Light` forces the light palette (no `dark` class on `<html>`).
//!  - `Dark` forces the dark palette (`<html class="dark">`).
//!  - `System` follows the OS / browser preference (matches
//!    `prefers-color-scheme: dark`) and re-evaluates on change.
//!
//! Stored as a string pref under `mokosh_theme`. The setter writes to
//! localStorage AND immediately re-applies the resolved class so the
//! UI updates without a reload; the root-level `use_apply_theme` hook
//! covers the boot path (re-applies once on every mount).

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::modules::theme::accents;
use crate::utils::prefs;

const THEME_KEY: &str = "mokosh_theme";
const ACCENT_KEY: &str = "mokosh_accent";
const HTML_DARK_CLASS: &str = "dark";

/// Persisted theme preference. The string form is what's in
/// localStorage; the enum is what the UI reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// Read the current preference. Defaults to `System` when unset.
pub fn current() -> Theme {
    Theme::parse(&prefs::get_str(THEME_KEY, Theme::default().as_str()))
}

/// Persist `theme` and immediately re-apply the resolved class to
/// `<html>` so the UI flips without a reload.
pub fn set(theme: Theme) {
    prefs::set_str(THEME_KEY, theme.as_str());
    apply_now();
}

/// Read the current accent preference. Falls back to the default accent
/// when unset, on a non-web build, or when the stored id is unknown.
pub fn current_accent() -> &'static accents::Accent {
    accents::resolve(&prefs::get_str(ACCENT_KEY, accents::DEFAULT_ACCENT_ID))
}

/// Persist the accent by id and immediately re-apply so the whole app
/// recolors without a reload. Unknown ids resolve to the default on read.
pub fn set_accent(id: &str) {
    prefs::set_str(ACCENT_KEY, id);
    apply_now();
}

/// Whether the resolved base mode is currently dark (explicit Dark, or
/// System following an OS dark preference). The picker uses this to show
/// base-appropriate accent swatches and the contrast guardrail.
pub fn current_is_dark() -> bool {
    resolved_is_dark(current())
}

/// Resolve which palette to actually show right now: explicit `Light`
/// / `Dark` pass through; `System` reads `prefers-color-scheme: dark`.
fn resolved_is_dark(theme: Theme) -> bool {
    match theme {
        Theme::Light => false,
        Theme::Dark => true,
        Theme::System => system_prefers_dark(),
    }
}

#[cfg(feature = "web")]
fn system_prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false)
}

#[cfg(not(feature = "web"))]
fn system_prefers_dark() -> bool {
    false
}

/// Apply the current theme to the `<html>` element. Toggles
/// `class="dark"` (the Tailwind variant the SPA already uses).
#[cfg(feature = "web")]
pub fn apply_now() {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };
    let Some(root) = doc.document_element() else {
        return;
    };
    let is_dark = resolved_is_dark(current());
    let class_list = root.class_list();
    if is_dark {
        let _ = class_list.add_1(HTML_DARK_CLASS);
    } else {
        let _ = class_list.remove_1(HTML_DARK_CLASS);
    }
    apply_accent(&root, is_dark);
}

/// Inject the current accent's ramp + per-base fill/on-accent as inline
/// CSS variables on `<html>`. Inline values override the stylesheet
/// defaults in `input.css`, so every `*-accent*` utility recolors live.
/// The base surface/text/line variables are left to the stylesheet
/// (driven by the `dark` class), so only the accent is dynamic here.
#[cfg(feature = "web")]
fn apply_accent(root: &web_sys::Element, is_dark: bool) {
    use wasm_bindgen::JsCast;
    let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let accent = current_accent();
    let variant = if is_dark { accent.dark } else { accent.light };
    let style = html.style();
    const RAMP_VARS: [&str; 11] = [
        "--accent-50",
        "--accent-100",
        "--accent-200",
        "--accent-300",
        "--accent-400",
        "--accent-500",
        "--accent-600",
        "--accent-700",
        "--accent-800",
        "--accent-900",
        "--accent-950",
    ];
    for (name, value) in RAMP_VARS.iter().zip(accent.ramp.iter()) {
        let _ = style.set_property(name, value);
    }
    let _ = style.set_property("--accent", variant.fill);
    let _ = style.set_property("--on-accent", variant.on_accent);
}

#[cfg(not(feature = "web"))]
pub fn apply_now() {}

/// Root-level hook. Mount once at `App`; applies the saved theme on
/// boot and subscribes to OS-level dark-mode changes so `Theme::System`
/// users follow the system in real time.
#[cfg(feature = "web")]
pub fn use_apply_theme() {
    use dioxus::prelude::*;
    use_effect(move || {
        apply_now();
        let Some(win) = web_sys::window() else {
            return;
        };
        let Some(media) = win
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten()
        else {
            return;
        };
        let cb = Closure::wrap(Box::new(move |_: web_sys::Event| {
            // Only the System branch reacts to OS changes; the explicit
            // settings already wrote the right class via `set`.
            if matches!(current(), Theme::System) {
                apply_now();
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = media.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
        // Listener lives for the app's lifetime; nothing else removes it.
        cb.forget();
    });
}

#[cfg(not(feature = "web"))]
pub fn use_apply_theme() {}

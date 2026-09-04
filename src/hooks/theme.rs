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

use crate::modules::theme::accents;
use crate::utils::prefs;

const THEME_KEY: &str = "mokosh_theme";
const ACCENT_KEY: &str = "mokosh_accent";

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
/// when unset, on a non-`app` build, or when the stored id is unknown.
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

/// MAPPS-504: `prefers-color-scheme` in the browser. The desktop reads
/// the same query out of its webview once MAPPS-511's listener has
/// reported one, and the window's tao theme until then.
fn system_prefers_dark() -> bool {
    crate::platform::dom::system_prefers_dark()
}

/// Apply the current theme to the `<html>` element. Toggles
/// `class="dark"` (the Tailwind variant the SPA already uses).
///
/// MAPPS-504: the two writes go through [`crate::platform::dom`], which
/// touches the DOM directly in the browser and evaluates the equivalent
/// script in the webview on the desktop.
pub fn apply_now() {
    let is_dark = resolved_is_dark(current());
    crate::platform::dom::set_root_dark(is_dark);
    apply_accent(is_dark);
}

/// Inject the current accent's ramp + per-base fill/on-accent as inline
/// CSS variables on `<html>`. Inline values override the stylesheet
/// defaults in `input.css`, so every `*-accent*` utility recolors live.
/// The base surface/text/line variables are left to the stylesheet
/// (driven by the `dark` class), so only the accent is dynamic here.
fn apply_accent(is_dark: bool) {
    let accent = current_accent();
    let variant = if is_dark { accent.dark } else { accent.light };
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
    let mut vars: Vec<(&str, &str)> = RAMP_VARS
        .iter()
        .copied()
        .zip(accent.ramp.iter().copied())
        .collect();
    vars.push(("--accent", variant.fill));
    vars.push(("--on-accent", variant.on_accent));
    crate::platform::dom::set_root_css_vars(&vars);
}

/// Root-level hook. Mount once at `App`; applies the saved theme on
/// boot and subscribes to OS-level dark-mode changes so `Theme::System`
/// users follow the system in real time.
///
/// MAPPS-504 / MAPPS-511: both hosts subscribe to the same
/// `prefers-color-scheme` media query. The browser attaches the listener
/// itself; the desktop has its webview attach one and post each change
/// back over the `eval` channel, which
/// [`crate::platform::dom::watch_system_theme`] owns.
pub fn use_apply_theme() {
    use dioxus::prelude::*;
    use_effect(move || {
        apply_now();
        #[cfg(target_arch = "wasm32")]
        subscribe_to_system_theme();
        #[cfg(not(target_arch = "wasm32"))]
        crate::platform::dom::watch_system_theme(|| {
            // Only the System branch reacts to OS changes; the explicit
            // settings already wrote the right class via `set`.
            if matches!(current(), Theme::System) {
                apply_now();
            }
        });
    });
}

#[cfg(target_arch = "wasm32")]
fn subscribe_to_system_theme() {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

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
    if media
        .add_event_listener_with_callback("change", cb.as_ref().unchecked_ref())
        .is_err()
    {
        // Silently losing this leaves `Theme::System` users pinned to
        // whatever the OS said at boot, with no way to tell why.
        tracing::error!("could not subscribe to OS dark-mode changes");
        return;
    }
    // Listener lives for the app's lifetime; nothing else removes it.
    cb.forget();
}

/// MAPPS-511: both hosts follow the OS light/dark preference.
///
/// A source scan: the subscription needs a document (or a webview) and no
/// host test has either. What is pinned is that the desktop has a branch at
/// all - it had none, and a `Theme::System` user there stayed on whatever the
/// OS said when the app started, which reads as "the setting does nothing".
#[cfg(test)]
mod system_theme_tests {
    const SRC: &str = include_str!("theme.rs");

    fn code_only() -> String {
        let end = SRC
            .find("mod system_theme_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn every_host_subscribes_to_the_os_preference() {
        let code = code_only();
        assert!(
            code.contains("#[cfg(target_arch = \"wasm32\")] subscribe_to_system_theme();"),
            "the browser attaches its own media-query listener"
        );
        assert!(
            code.contains("crate::platform::dom::watch_system_theme(|| {"),
            "and the desktop subscribes through the platform layer"
        );
    }
}

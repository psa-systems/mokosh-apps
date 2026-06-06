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

use crate::utils::prefs;

const THEME_KEY: &str = "mokosh_theme";
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
    let class_list = root.class_list();
    if resolved_is_dark(current()) {
        let _ = class_list.add_1(HTML_DARK_CLASS);
    } else {
        let _ = class_list.remove_1(HTML_DARK_CLASS);
    }
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

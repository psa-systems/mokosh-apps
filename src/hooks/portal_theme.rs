//! PMS-729 phase 2 §6 slice 3: portal-scoped theme preference.
//!
//! The agent-side theme state lives under `mokosh_theme` /
//! `mokosh_accent` (see [`crate::hooks::theme`]) and is bound to the
//! agent-side user account via [`crate::hooks::theme_sync`]. Portal
//! users are a different identity kind (a `contacts` row, not a
//! `users` row) and, critically, portal users on a per-tenant host
//! (`{slug}.client.<apex>`) live in a different browser origin than
//! the agent shell (`msp.<apex>`), so localStorage between the two is
//! already partitioned. The portal-side toggle therefore keeps its
//! own localStorage key so a shared-browser scenario (e.g. an MSP
//! employee opening a customer portal in the same window) does not
//! see the agent-side theme leak in, and vice versa.
//!
//! What this module owns:
//! - `mokosh_portal_theme`: Light / Dark / System string pref.
//! - `apply_now`: toggles `<html>.dark` per the resolved theme. It
//!   deliberately does NOT touch `--accent` / `--on-accent`; those
//!   are driven by [`crate::hooks::portal_branding`] (per-tenant
//!   colors override the built-in accent, or the tenant's default
//!   holds when unset). Keeping them independent means a
//!   customer flipping to dark mode does not clobber the MSP's
//!   primary color.
//! - `use_apply_portal_theme`: mount-once hook that applies on boot
//!   and subscribes to `prefers-color-scheme` for the System branch.
//!
//! What this module deliberately does NOT do: sync to the account,
//! push to the server, or interact with the accent picker. Portal
//! identity has no `PUT /portal/auth/me` yet (the phase 2 §5 read
//! endpoint is one-way today), and adding round-trip theme
//! persistence per-contact is out of scope for slice 3.

use crate::hooks::theme::Theme;
use crate::utils::prefs;

const PORTAL_THEME_KEY: &str = "mokosh_portal_theme";
const HTML_DARK_CLASS: &str = "dark";

/// Read the current portal theme preference. Defaults to `System`
/// when unset, on non-web builds, or when the stored string does not
/// parse as one of the three known values.
pub fn current() -> Theme {
    Theme::parse(&prefs::get_str(PORTAL_THEME_KEY, Theme::default().as_str()))
}

/// Persist `theme` under [`PORTAL_THEME_KEY`] and immediately re-apply
/// so the `<html>.dark` class flips without a reload.
pub fn set(theme: Theme) {
    prefs::set_str(PORTAL_THEME_KEY, theme.as_str());
    apply_now();
}

/// Whether the resolved theme currently paints dark. Handy for
/// picking the tenant's dark-mode logo variant at render time.
pub fn current_is_dark() -> bool {
    resolved_is_dark(current())
}

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

/// Apply the resolved portal theme to `<html>`. Toggles the `.dark`
/// class the Tailwind config keys off, matching the agent-side
/// behaviour so the same utility classes work in both shells. Does not
/// touch accent CSS variables (portal branding owns those).
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
}

#[cfg(not(feature = "web"))]
pub fn apply_now() {}

/// One-shot latch: `true` after the first PortalLayout mount registers
/// the `prefers-color-scheme` listener, so subsequent portal-page
/// navigations do not accumulate duplicate listeners. `apply_now`
/// still runs on every mount so a Route swap re-asserts the class.
#[cfg(feature = "web")]
pub static MEDIA_LISTENER_REGISTERED: dioxus::prelude::GlobalSignal<bool> =
    dioxus::prelude::Signal::global(|| false);

/// PortalLayout-mount hook: apply the saved portal theme on boot and,
/// for `Theme::System`, follow OS-level dark-mode changes in real time.
///
/// Mirrors [`crate::hooks::theme::use_apply_theme`] but reads the
/// portal-scoped key. The `prefers-color-scheme` listener is registered
/// exactly once per tab via [`MEDIA_LISTENER_REGISTERED`]; every mount
/// still re-asserts the class so a Route swap that re-runs this effect
/// picks up any preference changed since.
#[cfg(feature = "web")]
pub fn use_apply_portal_theme() {
    use dioxus::prelude::*;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    use_effect(move || {
        apply_now();
        if *MEDIA_LISTENER_REGISTERED.peek() {
            return;
        }
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
            if matches!(current(), Theme::System) {
                apply_now();
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = media.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
        // Same lifetime posture as `theme::use_apply_theme`: the listener
        // lives for the app's lifetime; nothing else removes it.
        cb.forget();
        *MEDIA_LISTENER_REGISTERED.write() = true;
    });
}

#[cfg(not(feature = "web"))]
pub fn use_apply_portal_theme() {}

/// Rotate through the three theme states in a stable order so a single
/// button can cycle a customer between Light -> Dark -> System without
/// popping a menu. Returns the next state to persist.
pub fn next_in_cycle(current: Theme) -> Theme {
    match current {
        Theme::Light => Theme::Dark,
        Theme::Dark => Theme::System,
        Theme::System => Theme::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_goes_light_dark_system_light() {
        assert!(matches!(next_in_cycle(Theme::Light), Theme::Dark));
        assert!(matches!(next_in_cycle(Theme::Dark), Theme::System));
        assert!(matches!(next_in_cycle(Theme::System), Theme::Light));
    }
}

//! Per-user theme sync (MAPPS-259 / PMS-410).
//!
//! localStorage is the boot cache (applied instantly, flash-free, by
//! `theme::use_apply_theme`); the account is the cross-device source of
//! truth. On the first authenticated mount we pull the account prefs and
//! let them win over localStorage (then re-apply); on every change in the
//! picker we push them back, fire-and-forget. A push failure is logged,
//! never surfaced or blocking, because localStorage already holds the
//! value. The base-mode + accent fields live on the mokosh-server user
//! row (PMS-410), reached through the same `/auth/me` endpoint the
//! profile screen uses.

use serde::{Deserialize, Serialize};

use crate::hooks::theme;

const ME_PATH: &str = "/auth/me";

/// Theme-only PUT body. The server's update handler only writes fields
/// that are present, so sending just these two leaves every other user
/// field untouched (mirrors how the profile screen sends a subset).
#[derive(Serialize)]
struct ThemeAccountUpdate {
    theme_base_mode: Option<String>,
    theme_accent_id: Option<String>,
}

/// The two theme fields off `GET /auth/me`; all other fields are ignored.
#[derive(Default, Deserialize)]
struct ThemePrefs {
    #[serde(default)]
    theme_base_mode: Option<String>,
    #[serde(default)]
    theme_accent_id: Option<String>,
}

/// Push the current base mode + accent to the account. Fire-and-forget:
/// failures are logged, never block the UI. No-op off web. Call this
/// after a local `theme::set` / `theme::set_accent` in the picker.
pub fn save_to_account() {
    #[cfg(feature = "web")]
    {
        use dioxus::prelude::spawn;
        let body = ThemeAccountUpdate {
            theme_base_mode: Some(theme::current().as_str().to_string()),
            theme_accent_id: Some(theme::current_accent().id.to_string()),
        };
        spawn(async move {
            if let Err(e) =
                crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(ME_PATH, &body)
                    .await
            {
                tracing::warn!("theme account sync failed: {}", e.user_message());
            }
        });
    }
}

/// Root hook: on the first authenticated mount, pull the account theme
/// prefs and reconcile them into localStorage (server wins), then
/// re-apply. Reconciling via `theme::set` / `theme::set_accent` writes
/// localStorage and re-applies WITHOUT echoing back to the server, so
/// there is no save loop. No-op off web.
#[cfg(feature = "web")]
pub fn use_theme_sync() {
    use dioxus::prelude::*;

    let auth = crate::hooks::use_auth();
    let mut synced = use_signal(|| false);

    use_effect(move || {
        let state = auth.read();
        if state.is_loading || !state.is_authenticated() || synced() {
            return;
        }
        synced.set(true);
        spawn(async move {
            let Ok(prefs) = crate::hooks::fetch::api::get_authed::<ThemePrefs>(ME_PATH).await
            else {
                return;
            };
            let mut changed = false;
            if let Some(mode) = prefs.theme_base_mode.as_deref() {
                theme::set(theme::Theme::parse(mode));
                changed = true;
            }
            if let Some(accent) = prefs.theme_accent_id.as_deref() {
                theme::set_accent(accent);
                changed = true;
            }
            if changed {
                theme::apply_now();
            }
        });
    });
}

#[cfg(not(feature = "web"))]
pub fn use_theme_sync() {}

//! Contact-plane cold-load bootstrap + background refresh (prompt 005).
//!
//! The contact access token lives in memory only; the refresh token is
//! mirrored to localStorage so a hard refresh / deep-link can re-mint
//! the access token before AuthGuard bounces the visitor away. This
//! module owns the two flows that surround that mirror:
//!
//! - `refresh_contact_session` posts to `/contact/auth/refresh` with the
//!   stored refresh token and, on success, writes both fresh tokens
//!   through the fetch helpers. On any failure it clears both so the
//!   visitor lands on `/portal/{slug}/login` rather than looping.
//! - `use_contact_auto_refresh` runs a periodic loop that calls
//!   `refresh_contact_session` while a session is held, so a long-lived
//!   tab keeps its access token fresh without a user round-trip. It
//!   also registers a `window` `focus` listener so the caps + roles on
//!   the SPA rehydrate the instant the visitor tabs back into the app
//!   (MAPPS-616).

use dioxus::prelude::*;

#[cfg(feature = "web")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "web")]
#[derive(Serialize)]
struct RefreshBody {
    refresh_token: String,
}

#[cfg(feature = "web")]
#[derive(Deserialize)]
struct RefreshResp {
    access_token: String,
    refresh_token: String,
    // Fresh contact snapshot the server returns on a successful
    // refresh (prompt 004 `ContactLoginResponse`). Prompt 006 pulls
    // `caps` off this into the capability hook so a role revoke
    // lands within one tick.
    #[serde(default)]
    contact: Option<RefreshContactSnippet>,
}

#[cfg(feature = "web")]
#[derive(Deserialize, Default)]
struct RefreshContactSnippet {
    #[serde(default)]
    caps: Vec<String>,
    /// MAPPS-589 (prompt 011): carried when PMS-928 has landed so the
    /// AuthGuard bootstrap can prefer the Portal-ID bounce path.
    /// Optional so the deserialise still round-trips a legacy
    /// response body that pre-dates the field.
    #[serde(default)]
    portal_id: Option<i64>,
    /// Legacy slug the session is scoped to (kept during the
    /// transition window that keeps `portal_slug` on the server).
    #[serde(default)]
    portal_slug: String,
    /// MAPPS-604 (prompt 013): the Company UUID this session is scoped
    /// to. `Option` because pre-PMS-935 servers omit the field; on a
    /// legacy response the SPA falls back to its old URL-derived path.
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    /// MAPPS-609: the UUID of the Contact behind this session. `Option`
    /// because pre-PMS-937 servers omit the field; on a legacy response
    /// the SPA leaves the store empty and ownership gates (e.g. the
    /// ticket-detail Edit button) fall closed.
    #[serde(default)]
    contact_id: Option<uuid::Uuid>,
}

/// Rotate the contact session using the stored refresh token.
///
/// Reads `current_contact_refresh_token()` (memory first, localStorage
/// fallback). On a 2xx writes both the new access + refresh tokens
/// through the setters and returns `Ok`. On ANY failure clears both
/// tokens so a stale refresh cannot loop the visitor through repeated
/// bootstrap attempts.
#[cfg(feature = "web")]
pub async fn refresh_contact_session() -> Result<(), String> {
    let Some(refresh) = crate::hooks::fetch::api::current_contact_refresh_token() else {
        crate::hooks::fetch::api::clear_contact_session();
        crate::hooks::capabilities::clear_contact_capabilities();
        return Err("no contact refresh token".to_string());
    };
    let body = RefreshBody {
        refresh_token: refresh,
    };
    match crate::hooks::fetch::api::post_typed::<RefreshResp, _>("/contact/auth/refresh", &body)
        .await
    {
        Ok(resp) => {
            let caps = resp
                .contact
                .as_ref()
                .map(|c| c.caps.clone())
                .unwrap_or_default();
            crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
            crate::hooks::fetch::api::set_contact_refresh_token(Some(resp.refresh_token));
            // MAPPS-589 (prompt 011): mirror last_slug + last_portal_id
            // from the refresh response, so a long-lived tab that has
            // never re-logged-in also picks up the Portal ID as soon as
            // PMS-928 starts returning it.
            if let Some(snippet) = resp.contact.as_ref() {
                if !snippet.portal_slug.is_empty() {
                    crate::hooks::fetch::api::set_contact_last_slug(&snippet.portal_slug);
                }
                if let Some(pid) = snippet.portal_id {
                    crate::hooks::fetch::api::set_contact_last_portal_id(&pid.to_string());
                }
                // MAPPS-604: pick up the session's Company scope from
                // the refresh response so pages can build Company-scoped
                // URLs without a second round-trip.
                crate::hooks::fetch::api::set_contact_company_id(snippet.company_id);
                // MAPPS-609: pick up the session's Contact UUID so the
                // ticket-detail Edit button can gate on ownership.
                crate::hooks::fetch::api::set_contact_id(snippet.contact_id);
            }
            crate::hooks::capabilities::set_contact_capabilities(Some(caps));
            Ok(())
        }
        Err(e) => {
            crate::hooks::fetch::api::clear_contact_session();
            crate::hooks::capabilities::clear_contact_capabilities();
            Err(e.to_string())
        }
    }
}

/// Non-web stub so callers under `cargo check` without the `web`
/// feature still compile.
#[cfg(not(feature = "web"))]
pub async fn refresh_contact_session() -> Result<(), String> {
    Err("contact refresh unavailable outside web".to_string())
}

/// Background auto-refresh + focus-driven refresh. Mount once near the
/// app root.
///
/// Two triggers, layered:
///
/// - Periodic tick every 2 minutes (was 12 min prior to MAPPS-616).
///   Server mints a 15-minute access token so 2 minutes still leaves
///   plenty of margin; the trade is 6x more `/refresh` calls per idle
///   session per hour, which is cheap. Cadence caps the WORST-CASE
///   latency for a staff-side role change to reflect in the contact
///   SPA at ~2 minutes.
/// - Window `focus` event. Fires the instant the visitor tabs back
///   into the app so caps + roles rehydrate BEFORE any click has a
///   chance to render stale UI. Server-side enforcement (prompt 008
///   DB-load per request) is already live; this closes the last
///   client-side gap where a role revoke could sit invisible on the
///   contact's own screen until the next tick.
///
/// Each trigger only fires the network call when a contact session is
/// actually held, so the hook no-ops on the staff-only browser
/// session.
pub fn use_contact_auto_refresh() {
    use_future(move || async move {
        loop {
            #[cfg(feature = "web")]
            {
                gloo_timers::future::TimeoutFuture::new(120_000).await;
                if crate::hooks::fetch::api::has_contact_session() {
                    let _ = refresh_contact_session().await;
                }
            }
            #[cfg(not(feature = "web"))]
            {
                // Non-web builds have no timer; break so the future
                // does not busy-loop under `cargo check`.
                break;
            }
        }
    });

    #[cfg(feature = "web")]
    use_effect(move || {
        install_focus_refresh_listener();
    });
}

/// MAPPS-616: register a single `window` `focus` listener that fires
/// `refresh_contact_session` on tab-focus regain, but only when a
/// contact session is held. Guarded by a thread-local `bool` so a
/// remount / hot-reload does not stack duplicate listeners; the
/// closure itself is `.forget()`-ed because the listener lives for
/// the wasm module's lifetime (the auth.rs `pageshow` hook uses the
/// same pattern).
#[cfg(feature = "web")]
fn install_focus_refresh_listener() {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    thread_local! {
        static FOCUS_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if FOCUS_INSTALLED.with(|f| f.get()) {
        return;
    }

    let Some(win) = web_sys::window() else {
        return;
    };
    let cb = Closure::wrap(Box::new(move |_evt: web_sys::Event| {
        if !crate::hooks::fetch::api::has_contact_session() {
            return;
        }
        dioxus::prelude::spawn(async move {
            let _ = refresh_contact_session().await;
        });
    }) as Box<dyn FnMut(web_sys::Event)>);
    if let Err(err) =
        win.add_event_listener_with_callback("focus", cb.as_ref().unchecked_ref())
    {
        tracing::warn!(error = ?err, "use_contact_auto_refresh: focus listener install failed");
        return;
    }
    cb.forget();
    FOCUS_INSTALLED.with(|f| f.set(true));
}

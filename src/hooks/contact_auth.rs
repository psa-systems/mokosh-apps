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
//! - `use_contact_auto_refresh` runs a ~12-minute loop that calls
//!   `refresh_contact_session` while a session is held, so a long-lived
//!   tab keeps its access token fresh without a user round-trip.

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
        return Err("no contact refresh token".to_string());
    };
    let body = RefreshBody {
        refresh_token: refresh,
    };
    match crate::hooks::fetch::api::post_typed::<RefreshResp, _>("/contact/auth/refresh", &body)
        .await
    {
        Ok(resp) => {
            crate::hooks::fetch::api::set_contact_access_token(Some(resp.access_token));
            crate::hooks::fetch::api::set_contact_refresh_token(Some(resp.refresh_token));
            Ok(())
        }
        Err(e) => {
            crate::hooks::fetch::api::clear_contact_session();
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

/// Background auto-refresh loop. Mount once near the app root. Sleeps
/// ~12 minutes between ticks (the server mints a 15-minute contact
/// access token, so a 12-minute cadence leaves a comfortable margin);
/// each tick calls `refresh_contact_session` only when a session is
/// held, so the loop no-ops on the staff-only browser session.
pub fn use_contact_auto_refresh() {
    use_future(move || async move {
        loop {
            #[cfg(feature = "web")]
            {
                gloo_timers::future::TimeoutFuture::new(720_000).await;
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
}

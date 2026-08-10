//! PMS-729 phase 2 H1+H2: portal auth lifecycle helpers.
//!
//! Two responsibilities:
//!
//! 1. **Auto-refresh.** [`use_portal_auto_refresh`] is a hook mounted at
//!    the `PortalLayout` level. It runs a background loop that rotates
//!    the access + refresh token pair every ~12 minutes (well before
//!    the server's 15-minute access-token TTL). Uses `gloo_timers`,
//!    matching the pattern in [`crate::hooks::auth`].
//!
//! 2. **Logout.** [`portal_logout`] revokes the refresh token
//!    server-side, then clears both in-memory holders, then navigates
//!    the browser to `/portal/login`. Idempotent: safe to call from
//!    both the user-menu button AND the terminal error path (e.g.
//!    refresh returned 401 because the session was killed elsewhere).

use serde::{Deserialize, Serialize};

#[cfg(feature = "web")]
use crate::hooks::fetch::api::{
    current_portal_refresh_token, has_portal_session, set_portal_access_token,
    set_portal_refresh_token,
};

/// Access-token TTL is 15 minutes on the server. Rotate before the
/// midpoint so a wall-clock skew of a few minutes still lands inside
/// the valid window. The 12-minute cadence also keeps the load on
/// `/portal/auth/refresh` low.
#[cfg(feature = "web")]
const REFRESH_INTERVAL_MS: u32 = 12 * 60 * 1_000;

/// PMS-729 phase 2 H2 request body for `POST /portal/auth/refresh`.
#[derive(Serialize)]
struct RefreshBody<'a> {
    refresh_token: &'a str,
}

/// PMS-729 phase 2 H2 response body from `POST /portal/auth/refresh`.
/// `expires_at` and `refresh_expires_at` are ignored: the SPA schedules
/// its next rotation on a fixed cadence, not on wall-clock math.
#[derive(Deserialize)]
struct RefreshResp {
    access_token: String,
    refresh_token: String,
}

/// PMS-729 phase 2 H1 request body for `POST /portal/auth/logout`.
#[derive(Serialize)]
struct LogoutBody<'a> {
    refresh_token: &'a str,
}

/// Rotate the current portal session: POST the refresh token, store
/// both new tokens on success, or clear the session (and return an
/// error) on 401. Returns `Ok(())` when the session is refreshed OR
/// when there is nothing to refresh (idle tab); an `Err` string is
/// surfaced only when the server actively rejected the token so the
/// caller (auto-refresh loop) can stop looping.
#[cfg(feature = "web")]
pub async fn refresh_portal_session() -> Result<(), String> {
    use crate::hooks::fetch::api::{post_typed, ApiError};

    let Some(refresh) = current_portal_refresh_token() else {
        return Ok(());
    };
    let body = RefreshBody {
        refresh_token: &refresh,
    };
    match post_typed::<RefreshResp, _>("/portal/auth/refresh", &body).await {
        Ok(resp) => {
            set_portal_access_token(Some(resp.access_token));
            set_portal_refresh_token(Some(resp.refresh_token));
            Ok(())
        }
        // 401 means the token is dead (expired, revoked, or replay
        // detected). Clear local state so the guard bounces the user
        // to /portal/login on the next fetch attempt.
        Err(ApiError::Status { code: 401, .. }) => {
            set_portal_access_token(None);
            set_portal_refresh_token(None);
            Err("portal session expired".to_string())
        }
        Err(e) => Err(e.user_message()),
    }
}

#[cfg(not(feature = "web"))]
pub async fn refresh_portal_session() -> Result<(), String> {
    Ok(())
}

/// Revoke the refresh token server-side, then clear both in-memory
/// holders. Idempotent: multiple calls or a network failure leave the
/// SPA in the signed-out state either way.
#[cfg(feature = "web")]
pub async fn portal_logout() {
    use crate::hooks::fetch::api::post_typed_no_content;

    if let Some(refresh) = current_portal_refresh_token() {
        // Best-effort server-side revocation. Ignore any error: the
        // local holders are the source of truth for the SPA, and a
        // network failure should not block the user from signing out.
        let body = LogoutBody {
            refresh_token: &refresh,
        };
        let _ = post_typed_no_content::<_>("/portal/auth/logout", &body).await;
    }
    set_portal_access_token(None);
    set_portal_refresh_token(None);
}

#[cfg(not(feature = "web"))]
pub async fn portal_logout() {}

/// Background rotation loop. Mount at `PortalLayout` so it runs for
/// every authenticated portal session, and stops when the user
/// navigates back to `/portal/login` (unmounts the layout).
///
/// Runs on the web feature only; the non-web stub is a no-op so the
/// call site does not need its own cfg gate.
#[cfg(feature = "web")]
pub fn use_portal_auto_refresh() {
    use dioxus::prelude::*;

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(REFRESH_INTERVAL_MS).await;
            if !has_portal_session() {
                // Session ended between ticks (explicit logout or a
                // failed prior refresh); nothing to do until the next
                // sign-in remounts this hook.
                continue;
            }
            if refresh_portal_session().await.is_err() {
                // Terminal failure. Break out of the loop; the guard
                // will pick up the missing token on the next fetch.
                break;
            }
        }
    });
}

#[cfg(not(feature = "web"))]
pub fn use_portal_auto_refresh() {}

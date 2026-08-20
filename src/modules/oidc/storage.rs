//! Tiny `sessionStorage` helpers used to bridge the redirect AND to
//! survive a full page reload.
//!
//! sessionStorage is cleared when the tab closes and is per-origin.
//! Distinct payloads:
//!
//!  * `STATE_KEY` (`PendingFlow`) - short-lived OIDC code-flow state
//!    (verifier + state + nonce). Written by `start_login`, removed by
//!    `complete_login`.
//!  * `AUTH_KEY` (`StoredTokens`) - the access/id/refresh-token bundle
//!    after a successful login, so URL-bar navigation and tab reload
//!    rehydrate `AuthContext` instead of dropping the user back on the
//!    login page. We deliberately use sessionStorage rather than
//!    localStorage: the bundle disappears when the tab closes, which
//!    matches user expectations and matches the lifetime of the OP
//!    session cookie. An XSS in the SPA can already read tokens out
//!    of memory, so writing them to sessionStorage adds little
//!    additional exposure compared to the alternative
//!    (localStorage cross-tab leak, or background-refresh complexity
//!    via `prompt=none`).
//!  * `CALLBACK_RETRY_KEY` (a counter) - MAPPS-432: how many login restarts
//!    `/auth/callback` has taken for a recoverable error, so the silent retry
//!    is bounded. Cleared on a successful exchange.
//!
//! The full accepted-risk decision (mitigations, the httpOnly-cookie BFF option,
//! and why the refresh token lives in the browser) is recorded in
//! `docs/oidc-token-storage.md` (MAPPS-362).

const STATE_KEY: &str = "mokosh_oidc_flow_v1";
const AUTH_KEY: &str = "mokosh_auth_bundle_v1";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub id_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub scope: String,
}

pub fn save_auth(t: &StoredTokens) {
    if let Ok(storage) = session_storage() {
        if let Ok(json) = serde_json::to_string(t) {
            let _ = storage.set_item(AUTH_KEY, &json);
        }
    }
}

pub fn load_auth() -> Option<StoredTokens> {
    let storage = session_storage().ok()?;
    let raw = storage.get_item(AUTH_KEY).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

pub fn clear_auth() {
    if let Ok(storage) = session_storage() {
        let _ = storage.remove_item(AUTH_KEY);
    }
    // MAPPS-368: also drop any standalone (non-OIDC) session so logout is
    // complete regardless of which path signed the user in; otherwise the
    // stored standalone session would rehydrate the user right after logout.
    clear_standalone();
}

/// MAPPS-368: standalone (non-OIDC) session key. Kept separate from `AUTH_KEY`
/// so the two rehydrate paths never collide.
const STANDALONE_KEY: &str = "mokosh_standalone_session_v1";

/// MAPPS-368: a persisted standalone username/password session. Unlike
/// [`StoredTokens`] there is no `id_token` to rebuild the user from, so the
/// `CurrentUser` view model is stored directly alongside the tokens. Rehydrated
/// at boot by `crate::hooks::auth`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StandaloneSession {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub user: crate::CurrentUser,
}

pub fn save_standalone(s: &StandaloneSession) {
    if let Ok(storage) = session_storage() {
        if let Ok(json) = serde_json::to_string(s) {
            let _ = storage.set_item(STANDALONE_KEY, &json);
        }
    }
}

pub fn load_standalone() -> Option<StandaloneSession> {
    let storage = session_storage().ok()?;
    let raw = storage.get_item(STANDALONE_KEY).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

pub fn clear_standalone() {
    if let Ok(storage) = session_storage() {
        let _ = storage.remove_item(STANDALONE_KEY);
    }
}

/// MAPPS-432: consecutive login restarts kicked off by a recoverable
/// `/auth/callback` failure. Tab-scoped like the flow state itself, so the
/// budget dies with the tab it was spent in.
const CALLBACK_RETRY_KEY: &str = "mokosh_oidc_callback_retry_v1";

/// MAPPS-432: restarts allowed before `/auth/callback` gives up and shows the
/// underlying error. The 3rd consecutive recoverable failure renders instead of
/// navigating, so a callback that keeps failing cannot loop the user forever.
pub const MAX_CALLBACK_RETRIES: u32 = 2;

/// Count this restart and return the number taken in this tab so far,
/// including this one. `Err` when the counter cannot be read or written: the
/// caller must not restart without a working guard, or the loop is unbounded.
pub fn bump_callback_retry() -> Result<u32, String> {
    let storage = session_storage()?;
    let previous = storage
        .get_item(CALLBACK_RETRY_KEY)
        .map_err(|_| "sessionStorage read failed".to_string())?
        .map(|raw| {
            raw.parse::<u32>()
                .map_err(|e| format!("corrupt callback retry counter {raw:?}: {e}"))
        })
        .transpose()?
        .unwrap_or(0);
    let count = previous.saturating_add(1);
    storage
        .set_item(CALLBACK_RETRY_KEY, &count.to_string())
        .map_err(|_| "sessionStorage write failed".to_string())?;
    Ok(count)
}

/// Reset the restart budget. Called on a successful token exchange so a later
/// legitimate reload starts with a full one.
pub fn clear_callback_retry() -> Result<(), String> {
    session_storage()?
        .remove_item(CALLBACK_RETRY_KEY)
        .map_err(|_| "sessionStorage delete failed".to_string())
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PendingFlow {
    pub code_verifier: String,
    pub state: String,
    pub nonce: String,
    pub return_to: String,
    /// MAPPS-338: epoch milliseconds when this flow was issued. Read on
    /// `take_pending` to reject flows older than `PENDING_FLOW_TTL_MS`;
    /// a stale flow from a tab opened a week ago should not satisfy the
    /// OIDC callback today. `#[serde(default)]` keeps deserialization
    /// compatible with rows minted before this field existed (they
    /// default to `0`, which `is_expired` reads as "expired" so the user
    /// re-authenticates cleanly).
    #[serde(default)]
    pub issued_at_ms: u64,
}

/// MAPPS-338: maximum age of a `PendingFlow` before `take_pending`
/// rejects it. 10 minutes mirrors the upstream OIDC code TTL window; a
/// flow older than that would fail the code-redemption side anyway.
pub const PENDING_FLOW_TTL_MS: u64 = 10 * 60 * 1000;

/// MAPPS-338: current epoch milliseconds, via
/// [`crate::platform::clock`] so the same reading works in a browser and
/// on the desktop.
fn now_ms() -> u64 {
    crate::platform::clock::now_ms()
}

pub fn save_pending(flow: &mut PendingFlow) -> Result<(), String> {
    // MAPPS-338: stamp issuance time at save so `take_pending` can age it.
    if flow.issued_at_ms == 0 {
        flow.issued_at_ms = now_ms();
    }
    let storage = session_storage()?;
    let json = serde_json::to_string(flow).map_err(|e| e.to_string())?;
    storage
        .set_item(STATE_KEY, &json)
        .map_err(|_| "sessionStorage write failed".to_string())
}

pub fn take_pending() -> Result<PendingFlow, String> {
    let storage = session_storage()?;
    let raw = storage
        .get_item(STATE_KEY)
        .map_err(|_| "sessionStorage read failed".to_string())?
        .ok_or_else(|| "no pending OIDC flow".to_string())?;
    let _ = storage.remove_item(STATE_KEY);
    let flow: PendingFlow =
        serde_json::from_str(&raw).map_err(|e| format!("corrupt flow state: {e}"))?;
    // MAPPS-338: reject stale flows. A week-old tab should not authenticate.
    let age = now_ms().saturating_sub(flow.issued_at_ms);
    if age > PENDING_FLOW_TTL_MS {
        return Err(format!(
            "pending OIDC flow expired (age {} ms > {} ms)",
            age, PENDING_FLOW_TTL_MS
        ));
    }
    Ok(flow)
}

/// MAPPS-504: `sessionStorage` in the browser, an in-process map on the
/// desktop, where the window IS the session. Same lifetime either way:
/// nothing here survives the app closing.
fn session_storage() -> Result<crate::platform::store::Store, String> {
    crate::platform::store::session()
}

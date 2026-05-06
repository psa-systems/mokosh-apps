//! Tiny `sessionStorage` helpers used to bridge the redirect.
//!
//! sessionStorage is cleared when the tab closes and is per-origin, which
//! is appropriate for short-lived flow state (verifier + state + nonce).
//! Tokens themselves are NEVER written here: see `tokens.rs`.

const STATE_KEY: &str = "mokosh_oidc_flow_v1";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PendingFlow {
    pub code_verifier: String,
    pub state: String,
    pub nonce: String,
    pub return_to: String,
}

pub fn save_pending(flow: &PendingFlow) -> Result<(), String> {
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
    serde_json::from_str(&raw).map_err(|e| format!("corrupt flow state: {e}"))
}

fn session_storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or_else(|| "no window".to_string())?
        .session_storage()
        .map_err(|_| "no sessionStorage handle".to_string())?
        .ok_or_else(|| "sessionStorage disabled".to_string())
}

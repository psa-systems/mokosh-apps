//! OIDC public-client (SPA) flow against mokosh-server.
//!
//! mokosh-apps is a pure WASM single-page app, so we use the
//! authorization-code flow with PKCE as a public client (no
//! `client_secret`). The flow:
//!
//!  1. `start_login()` generates a `code_verifier`, computes the S256
//!     `code_challenge`, generates `state` and `nonce`, persists those in
//!     `sessionStorage`, then redirects the browser to
//!     `<issuer>/oauth2/authorize`.
//!  2. mokosh-server walks the user through login and redirects back to
//!     `redirect_uri` with `?code=...&state=...`.
//!  3. The `/auth/callback` route page calls [`complete_login`], which
//!     verifies `state`, POSTs the code + verifier to `/oauth2/token`,
//!     and returns parsed [`Tokens`].
//!  4. A failed exchange is classified by [`classify_flow_error`]: the cases
//!     that only mean "no live authorization flow on this URL" (a bare
//!     `/auth/callback`, a missing or expired `PendingFlow`, an OP
//!     re-authentication signal) restart the login flow silently under a retry
//!     cap; CSRF / replay / config / network faults render an error screen.
//!  5. Tokens live in memory (in [`AuthContext`]) and are also persisted
//!     to `sessionStorage` via [`storage`] so a page reload rehydrates the
//!     session rather than redirecting to authorize again. They are NEVER
//!     written to `localStorage`: `sessionStorage` is tab-scoped and
//!     cleared on tab close, which narrows the XSS exposure window while
//!     still surviving an in-tab reload.

pub mod config;
pub mod flow;
pub mod pkce;
pub mod storage;
pub mod tokens;

pub use config::OidcConfig;

/// MAPPS-368: true when no OIDC issuer is configured for this deployment, so
/// the SPA presents standalone username/password login (against mokosh-server's
/// `/api/v1/auth/login`) instead of the bunyip OIDC redirect. Convenience
/// wrapper over [`OidcConfig::has_issuer`] for the login trigger sites.
pub fn is_standalone() -> bool {
    !OidcConfig::for_current_origin().has_issuer()
}

/// MAPPS-432: one line for an auth-flow failure the user is not shown.
/// The WASM build wires no `tracing` subscriber, so the browser console is the
/// only place an operator can read it there.
pub fn log_auth_error(msg: &str) {
    crate::platform::log::error(msg);
}

pub use flow::{
    classify_flow_error, classify_return_to, complete_login, current_return_to, issuer_get_authed,
    issuer_post_authed, refresh_tokens, revoke_refresh_token, snapshot_initial_search, start_login,
    CallbackRecovery, FlowError, ReturnTarget,
};
pub use tokens::{IdTokenClaims, Tokens};

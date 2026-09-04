//! OIDC public-client flow against the configured OP.
//!
//! mokosh-apps is a public client with no `client_secret` on either
//! target - a browser SPA and a desktop binary can both be read - so it
//! uses the authorization-code flow with PKCE. The flow:
//!
//!  1. `start_login()` generates a `code_verifier`, computes the S256
//!     `code_challenge`, generates `state` and `nonce`, persists those in
//!     the session store, then sends the user to
//!     `<issuer>/oauth2/authorize`. In a browser that is a redirect of
//!     the document. On the desktop (MAPPS-505) there is no origin to
//!     redirect to and back from, so the authorize URL opens in the
//!     user's own browser and the response comes back on an RFC 8252
//!     loopback listener bound for that one flow.
//!  2. The OP walks the user through login and redirects back to
//!     `redirect_uri` with `?code=...&state=...`.
//!  3. The `/auth/callback` route page calls [`complete_login`], which
//!     verifies `state`, POSTs the code + verifier to `/oauth2/token`,
//!     and returns parsed [`Tokens`].
//!  4. A failed exchange is classified by [`classify_flow_error`]: the cases
//!     that only mean "no live authorization flow on this URL" (a bare
//!     `/auth/callback`, a missing or expired `PendingFlow`, an OP
//!     re-authentication signal) restart the login flow silently under a retry
//!     cap; CSRF / replay / config / network faults render an error screen.
//!     A silent restart is a re-navigation to `/login`, which the desktop
//!     has no URL to perform, so there the recoverable cases render the
//!     error screen too rather than pretending to retry.
//!  5. Tokens live in memory (in [`AuthContext`]) and are also persisted
//!     to `sessionStorage` via [`storage`] so a page reload rehydrates the
//!     session rather than redirecting to authorize again. They are NEVER
//!     written to `localStorage`: `sessionStorage` is tab-scoped and
//!     cleared on tab close, which narrows the XSS exposure window while
//!     still surviving an in-tab reload.

pub mod config;
pub mod flow;
// MAPPS-505: the desktop's RFC 8252 hand-off. Private: `start_login` and
// `complete_login` stay the whole public surface of the flow on both
// targets.
#[cfg(not(target_arch = "wasm32"))]
mod native_flow;
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
    classify_flow_error, classify_return_to, complete_login, current_return_to, initial_search,
    issuer_get_authed, issuer_post_authed, refresh_tokens, revoke_refresh_token,
    snapshot_initial_search, start_login, CallbackRecovery, FlowError, ReturnTarget,
};
pub use tokens::{IdTokenClaims, Tokens};

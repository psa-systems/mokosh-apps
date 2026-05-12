//! OIDC public-client (SPA) flow against mokosh-server.
//!
//! mokosh-clients is a pure WASM single-page app, so we use the
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
//!  4. Tokens live in memory only (in [`AuthContext`]). They are NEVER
//!     written to localStorage: that would expose them to XSS. On page
//!     reload the user is redirected to authorize again (a silent prompt
//!     with `prompt=none` is a future iteration).

pub mod config;
pub mod flow;
pub mod pkce;
pub mod storage;
pub mod tokens;

pub use config::OidcConfig;
pub use flow::{
    complete_login, issuer_get, issuer_get_authed, issuer_post, issuer_post_authed,
    issuer_post_authed_empty, issuer_post_authed_no_body, mfa_verify, password_login,
    refresh_tokens, revoke_refresh_token, snapshot_initial_search, start_login, FlowError,
    LoginOutcome,
};
pub use tokens::{IdTokenClaims, Tokens};

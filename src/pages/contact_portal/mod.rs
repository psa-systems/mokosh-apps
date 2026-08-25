//! Contact-plane portal pages (mokosh-contact-login, prompt 005).
//!
//! Each submodule renders one leaf of the `/portal/{slug}/*` route
//! family: login, magic-link password setup, forgot-password,
//! reset-password. All are public (no `AuthGuard`); a successful login
//! seeds the contact-session tokens (see `hooks::fetch::api`) and
//! navigates the visitor into the workspace at `/dashboard`.

pub mod forgot_password;
pub mod login;
// MAPPS-572 (prompt 010): magic-link finder + Company picker land as
// two sibling routes at `/portal/login` and `/portal/pick?:token`.
pub mod magic_link_login;
pub mod picker;
pub mod reset_password;
pub mod set_password;

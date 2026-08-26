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
// two sibling routes. Post MAPPS-589 (prompt 011) the finder moved
// from `/portal/login` to `/portal/find?:email` so the shorter path
// can host the primary three-field password login page.
pub mod magic_link_login;
pub mod picker;
pub mod reset_password;
pub mod set_password;
// MAPPS-589 (prompt 011): Portal-ID login pages.
// - `generic_login` at `/portal/login` (three-field: Portal ID +
//   email + password).
// - `portal_id_login` at `/portal/{portal_id}/login` via the
//   `ContactHandleLogin` wrapper (Portal ID read-only, email +
//   password editable).
pub mod generic_login;
pub mod portal_id_login;

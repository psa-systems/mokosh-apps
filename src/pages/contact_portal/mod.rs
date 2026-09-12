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
/// MAPPS-761: where a contact lands after signing in, when the link that
/// brought them named a page.
pub mod next_target;
pub mod picker;
pub mod reset_password;
pub mod set_password;
// MAPPS-589 (prompt 011): Portal-ID login pages.
// - `generic_login` at `/portal/login` (three-field: Company ID +
//   email + password).
// - `portal_id_login` at `/portal/{portal_id}/login` via the
//   `ContactHandleLogin` wrapper (Company ID read-only, email +
//   password editable).
pub mod generic_login;
pub mod portal_branding;
pub mod portal_id_login;

/// MAPPS-763: a portal password is its own, and the sign-in page has to say so.
///
/// A customer who already has an account with this product - the MSP's own
/// staff testing the portal, or a contact who is also a user elsewhere -
/// reasonably types the password they already know, and is told only "Invalid
/// credentials". Reported twice in one sitting by the person building the
/// product: *"the ... already have account in msp, but i used the same
/// password, which doesnt work"*, and again on a second address, *"password
/// with bunyip doesnt work, ok i will use magiclink again"*.
///
/// The refusal is correct. A portal identity is a `contacts` row with its own
/// password and a platform identity is a `users` row; one address can hold
/// both, and pointing the portal at the other credential is the defect PMS-820
/// exists to have closed, where a customer resetting their portal password
/// reset a staff login. What was missing is anyone saying it.
///
/// Two rules these strings obey.
///
/// They speak as the MSP to their customer (the MAPPS-755 rule): the words
/// "platform", "tenant" and "plane" describe how this is built and mean
/// nothing to the person reading. "Any other password you use" is the same
/// fact in their terms.
///
/// And the failure message is IDENTICAL whether or not a contact with that
/// address exists. A message that softened for a known address would answer
/// "does this person have a portal account here", which is the enumeration
/// oracle the portal plane avoids everywhere else.
/// Help under the password field, shown before anything is typed.
pub const PORTAL_PASSWORD_HELP: &str =
    "Set from your invitation email. It is not any other password you use with us.";

/// What a refused sign-in says. Names no account, and points at the way through
/// for the customer who never set a password in the first place.
pub const PORTAL_SIGN_IN_FAILED: &str =
    "That email and password do not match. If you have not set a portal password yet, sign in without one below.";

#[cfg(test)]
mod sign_in_copy_tests {
    use super::{PORTAL_PASSWORD_HELP, PORTAL_SIGN_IN_FAILED};

    /// MAPPS-763: the fact the reporter needed, said in their terms.
    ///
    /// The person who gets this wrong is the one who ALREADY has an account
    /// with this product and types that password, so the help has to rule that
    /// out by name rather than describe what a portal password is.
    #[test]
    fn the_help_says_the_password_is_not_another_one() {
        let lowered = PORTAL_PASSWORD_HELP.to_lowercase();
        assert!(lowered.contains("invitation"), "{PORTAL_PASSWORD_HELP}");
        assert!(
            lowered.contains("not any other password"),
            "{PORTAL_PASSWORD_HELP}"
        );
    }

    /// A refused sign-in names the way through, because the customer who hits
    /// it most often is the one who never set a portal password at all.
    #[test]
    fn the_failure_points_at_the_passwordless_route() {
        let lowered = PORTAL_SIGN_IN_FAILED.to_lowercase();
        assert!(lowered.contains("without one"), "{PORTAL_SIGN_IN_FAILED}");
    }

    /// The failure must read identically whether or not a contact with that
    /// address exists here. Anything that names the account, or hedges about
    /// whether it was found, answers "is this person registered" - the
    /// enumeration oracle the rest of this plane is careful to avoid.
    #[test]
    fn the_failure_is_not_an_account_oracle() {
        let lowered = PORTAL_SIGN_IN_FAILED.to_lowercase();
        for leak in [
            "no account",
            "not found",
            "unknown",
            "no such",
            "does not exist",
            "unrecognised",
            "unrecognized",
            "register",
        ] {
            assert!(!lowered.contains(leak), "{PORTAL_SIGN_IN_FAILED}");
        }
    }

    /// Both strings speak as the MSP to their customer (MAPPS-755). The words
    /// below describe how this is built and mean nothing to the reader.
    #[test]
    fn the_copy_uses_no_implementation_words() {
        for copy in [PORTAL_PASSWORD_HELP, PORTAL_SIGN_IN_FAILED] {
            let lowered = copy.to_lowercase();
            for jargon in ["platform", "tenant", "plane", "credential", "identity"] {
                assert!(!lowered.contains(jargon), "{copy}");
            }
        }
    }
}

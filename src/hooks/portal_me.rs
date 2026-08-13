//! PMS-729 follow-up: the current portal contact.
//!
//! `GET /portal/auth/me` returns the authenticated contact's id + tenant + email +
//! first / last name. Cached in a `GlobalSignal` and gated behind a one-shot
//! latch so every page that reads it goes through the same request; a
//! logout or token failure clears both so a fresh session re-fetches.
//!
//! Mirrors [`crate::hooks::portal_branding`] exactly. Kept in its own module
//! because the two hooks are consumed by different components (the branding
//! hook renders the header wordmark + logo; the me hook renders the user
//! menu name / initials / email).

use dioxus::prelude::*;
use serde::Deserialize;

/// Snapshot of the authenticated portal contact. Matches mokosh-server's
/// `CurrentContactMe` DTO (`src/modules/portal/models.rs`): the JWT
/// claims plus account-state fields (currently `mfa_enabled`) the
/// Settings page needs without a second round-trip.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PortalMe {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub company_id: uuid::Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    /// Server's `contacts.portal_mfa_enabled`. Drives the "Set up
    /// two-factor auth" vs "Two-factor auth is on" affordance on
    /// `/portal/settings`.
    #[serde(default)]
    pub mfa_enabled: bool,
}

impl PortalMe {
    /// Full "First Last" for menu display. Falls back to email when both
    /// name halves are blank so the button never renders empty.
    pub fn display_name(&self) -> String {
        let first = self.first_name.trim();
        let last = self.last_name.trim();
        match (first.is_empty(), last.is_empty()) {
            (false, false) => format!("{first} {last}"),
            (false, true) => first.to_string(),
            (true, false) => last.to_string(),
            (true, true) => self.email.clone(),
        }
    }

    /// One-or-two-letter monogram for the avatar circle. First initial +
    /// last initial when both are present; single letter otherwise;
    /// email's first char as the last-resort fallback.
    pub fn initials(&self) -> String {
        let first = self.first_name.trim().chars().next();
        let last = self.last_name.trim().chars().next();
        match (first, last) {
            (Some(a), Some(b)) => format!("{a}{b}").to_ascii_uppercase(),
            (Some(a), None) | (None, Some(a)) => a.to_string().to_ascii_uppercase(),
            (None, None) => self
                .email
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string()),
        }
    }
}

/// The cached snapshot, shared across every portal page. `None` before
/// the fetch lands and any time the session is cleared.
#[cfg(feature = "web")]
pub static PORTAL_ME: GlobalSignal<Option<PortalMe>> = Signal::global(|| None);

/// One-shot latch: `true` once the fetch has been kicked off in this
/// tab so re-mounted components do not re-fire the request.
#[cfg(feature = "web")]
pub static HAS_FETCHED_ME: GlobalSignal<bool> = Signal::global(|| false);

/// Fire `GET /portal/auth/me` once per session when the SPA has a portal
/// token. Idempotent: safe to call from every portal-layout mount. A 401
/// clears both signals so the logout / token-invalidated path re-fetches
/// on the next session.
#[cfg(feature = "web")]
pub fn ensure_portal_me_fetch() {
    use crate::hooks::fetch::api::{get_portal_authed, has_portal_session};

    if !has_portal_session() {
        return;
    }
    if *HAS_FETCHED_ME.peek() {
        return;
    }
    *HAS_FETCHED_ME.write() = true;

    spawn(async move {
        match get_portal_authed::<PortalMe>("/portal/auth/me").await {
            Ok(me) => {
                *PORTAL_ME.write() = Some(me);
            }
            Err(_) => {
                // Session may have died between the has_portal_session
                // check and the fetch (server-side revoke, 401), or a
                // transport error. Reset both signals so the auth loop's
                // own 401 handler can bounce, and so the next render can
                // retry cleanly without needing a full logout.
                *PORTAL_ME.write() = None;
                *HAS_FETCHED_ME.write() = false;
            }
        }
    });
}

#[cfg(not(feature = "web"))]
pub fn ensure_portal_me_fetch() {}

/// Read a cloned snapshot of the current portal-contact identity.
/// Reactive: components that call this re-render when the value flips
/// from `None` to `Some`.
#[cfg(feature = "web")]
pub fn use_portal_me() -> Option<PortalMe> {
    PORTAL_ME.read().clone()
}

#[cfg(not(feature = "web"))]
pub fn use_portal_me() -> Option<PortalMe> {
    None
}

/// Clear the cached me signal (logout, revoke, session invalidation).
#[cfg(feature = "web")]
pub fn clear_portal_me() {
    *PORTAL_ME.write() = None;
    *HAS_FETCHED_ME.write() = false;
}

#[cfg(not(feature = "web"))]
pub fn clear_portal_me() {}

/// Force `GET /portal/auth/me` to run again on the next
/// `ensure_portal_me_fetch()` call. Used by write flows that mutate
/// server-side identity state (MFA enable/disable, later profile
/// updates) so the cached snapshot reflects the change without a full
/// logout.
#[cfg(feature = "web")]
pub fn invalidate_portal_me() {
    *HAS_FETCHED_ME.write() = false;
    ensure_portal_me_fetch();
}

#[cfg(not(feature = "web"))]
pub fn invalidate_portal_me() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn me(first: &str, last: &str, email: &str) -> PortalMe {
        PortalMe {
            id: uuid::Uuid::nil(),
            tenant_id: uuid::Uuid::nil(),
            company_id: uuid::Uuid::nil(),
            email: email.to_string(),
            first_name: first.to_string(),
            last_name: last.to_string(),
            mfa_enabled: false,
        }
    }

    #[test]
    fn display_name_both_halves_present() {
        assert_eq!(
            me("Ada", "Lovelace", "ada@example").display_name(),
            "Ada Lovelace"
        );
    }

    #[test]
    fn display_name_falls_back_when_one_half_blank() {
        assert_eq!(me("Ada", "", "ada@example").display_name(), "Ada");
        assert_eq!(me("", "Lovelace", "ada@example").display_name(), "Lovelace");
    }

    #[test]
    fn display_name_falls_back_to_email_when_both_blank() {
        assert_eq!(me("", "", "ada@example").display_name(), "ada@example");
    }

    #[test]
    fn initials_uppercase_first_and_last() {
        assert_eq!(me("ada", "lovelace", "").initials(), "AL");
    }

    #[test]
    fn initials_single_when_only_one_half() {
        assert_eq!(me("Ada", "", "").initials(), "A");
    }

    #[test]
    fn initials_email_fallback_when_both_blank() {
        assert_eq!(me("", "", "ada@example").initials(), "A");
    }
}

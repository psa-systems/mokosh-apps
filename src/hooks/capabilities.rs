//! mokosh-contact-login prompt 006: contact-portal capability gating.
//!
//! `use_capability("cap")` answers three questions in one call:
//!   1. Is a staff session active (`current_access_token().is_some()`)?
//!      Staff bypass capability gating and always return true.
//!   2. Is a platform-admin bearer stashed in `sessionStorage`? Same
//!      unconditional bypass so the super-admin persona sees the whole
//!      UI regardless of contact caps.
//!   3. Is a contact session active and does its `caps` snapshot
//!      include the requested cap? True only then.
//!   Otherwise false.
//!
//! `caps` are refreshed on every `/contact/auth/login` and
//! `/contact/auth/refresh` reply, so a role revoke lands on the SPA
//! within one refresh tick. The snapshot lives in a thread-local
//! (WASM is single-threaded, so a `RefCell` is safe); we never mirror
//! caps to `localStorage` because they come back for free on every
//! login and refresh.
//!
//! This module is UX-only. The security boundary is server-side
//! (prompt 008): each `/api/v1/contact/*` mutation extractor re-checks
//! the cap on the fresh JWT. A misconfigured client that fails to
//! hide a button still fails closed at the API.

#[cfg(feature = "web")]
use std::cell::RefCell;

/// MAPPS-518: sessionStorage key the `/platform/login` page writes the
/// platform-admin bearer under. Kept in sync with the definition in
/// `pages::platform_login` and `hooks::fetch::api`.
#[cfg(feature = "web")]
const PLATFORM_TOKEN_KEY: &str = "mokosh:platform_token";

#[cfg(feature = "web")]
thread_local! {
    /// Union of every assigned contact role's capabilities, as returned
    /// by the login / refresh response. `None` before the first contact
    /// session lands and after `clear_contact_capabilities`.
    static CONTACT_CAPABILITIES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// True when the caller holds `cap`. Precedence:
///
/// 1. If a contact session is active, the caller is a portal
///    contact - check the contact caps snapshot ONLY. The staff
///    bypass below is deliberately skipped so a stale staff bearer
///    left over in the same browser tab (QA testing both planes,
///    a returning staff user who then signs into the portal, etc.)
///    can NEVER paint staff-only UI while the visible session is
///    the contact plane. Fixes the report of a Read-Only contact
///    still seeing the CRM Companies/Contacts nav items because
///    the browser also happened to hold a staff bearer (MAPPS-625).
/// 2. Otherwise, an active staff or platform-admin bearer bypasses
///    the cap check unconditionally (the SPA does not know a
///    staff user's per-role caps; the server is authoritative).
/// 3. Otherwise, false.
pub fn use_capability(cap: &str) -> bool {
    #[cfg(feature = "web")]
    {
        if crate::hooks::fetch::api::has_contact_session() {
            return current_contact_capabilities()
                .map(|caps| caps.iter().any(|c| c == cap))
                .unwrap_or(false);
        }
        if crate::hooks::fetch::api::current_access_token().is_some() {
            return true;
        }
        if platform_bearer_present() {
            return true;
        }
        false
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = cap;
        false
    }
}

/// Convenience for a nav item that a caller may access under any of
/// several caps (e.g. the Dashboard is visible whenever the contact
/// can see tickets OR invoices OR quotes). Same plane-precedence
/// rules as [`use_capability`] - a live contact session shadows any
/// stale staff bearer.
pub fn use_any_capability(caps: &[&str]) -> bool {
    #[cfg(feature = "web")]
    {
        if crate::hooks::fetch::api::has_contact_session() {
            return current_contact_capabilities()
                .map(|held| caps.iter().any(|want| held.iter().any(|c| c == want)))
                .unwrap_or(false);
        }
        if crate::hooks::fetch::api::current_access_token().is_some() {
            return true;
        }
        if platform_bearer_present() {
            return true;
        }
        false
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = caps;
        false
    }
}

/// Overwrite the caps snapshot. Called from the contact-login page on
/// a successful `POST /contact/auth/login` and from
/// `refresh_contact_session` after a successful
/// `POST /contact/auth/refresh`. `None` clears the snapshot without
/// distinguishing "not signed in" from "signed in with zero caps" -
/// callers should always pass `Some(caps)` on success and only pass
/// `None` on session clear.
pub fn set_contact_capabilities(caps: Option<Vec<String>>) {
    #[cfg(feature = "web")]
    {
        CONTACT_CAPABILITIES.with(|slot| *slot.borrow_mut() = caps);
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = caps;
    }
}

/// Read the current caps snapshot. `None` before any contact login.
pub fn current_contact_capabilities() -> Option<Vec<String>> {
    #[cfg(feature = "web")]
    {
        CONTACT_CAPABILITIES.with(|slot| slot.borrow().clone())
    }
    #[cfg(not(feature = "web"))]
    {
        None
    }
}

/// Drop the caps snapshot. Called from every path that also clears
/// the contact refresh token (logout, refresh failure).
pub fn clear_contact_capabilities() {
    set_contact_capabilities(None);
}

/// Is the platform-admin bearer stashed in `sessionStorage`? Same
/// shape as `components::layout::platform_bearer_present`, inlined
/// here so this module has no dependency on the layout component.
///
/// `web_sys::window()` panics on non-wasm targets even under
/// `feature = "web"`, so the sessionStorage read is gated on
/// `target_arch = "wasm32"`. On the native `cargo test --lib` target
/// this returns false, which is the right answer: no browser, no
/// platform bearer.
#[cfg(feature = "web")]
fn platform_bearer_present() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(Some(store)) = win.session_storage() {
                if let Ok(Some(token)) = store.get_item(PLATFORM_TOKEN_KEY) {
                    return !token.trim().is_empty();
                }
            }
        }
    }
    false
}

/// The `__staff_only__` sentinel is a client-side signal for a control
/// no contact ever sees. It is deliberately absent from the server's
/// `ALL_CAPABILITIES` set so a `PortalRoleService::create_role` write
/// with this string fails validation - no contact JWT can ever carry
/// it. Staff and platform-admin sessions bypass the cap check in
/// `use_capability` unconditionally, so this constant renders as true
/// for them and false for every contact.
pub const STAFF_ONLY: &str = "__staff_only__";

#[cfg(all(test, feature = "web"))]
mod tests {
    use super::*;
    use crate::hooks::fetch::api::{set_access_token_for_test, set_contact_access_token};

    /// Reset every session slot this module can inspect. Called at
    /// the top of each test so leftover state from a previous test
    /// on the same thread cannot leak into the assertion.
    fn reset_session_state() {
        set_access_token_for_test(None);
        set_contact_access_token(None);
        clear_contact_capabilities();
    }

    #[test]
    fn staff_session_bypasses_capability_check() {
        reset_session_state();
        set_access_token_for_test(Some("staff-token".to_string()));
        assert!(use_capability("tickets:read"));
        assert!(use_capability("__staff_only__"));
        assert!(use_capability("does:not:exist"));
        set_access_token_for_test(None);
    }

    #[test]
    fn contact_with_cap_returns_true() {
        reset_session_state();
        set_contact_access_token(Some("contact-token".to_string()));
        set_contact_capabilities(Some(vec![
            "tickets:read".to_string(),
            "invoices:pay".to_string(),
        ]));
        assert!(use_capability("tickets:read"));
        assert!(use_capability("invoices:pay"));
        set_contact_access_token(None);
        clear_contact_capabilities();
    }

    #[test]
    fn contact_without_cap_returns_false() {
        reset_session_state();
        set_contact_access_token(Some("contact-token".to_string()));
        set_contact_capabilities(Some(vec!["tickets:read".to_string()]));
        assert!(!use_capability("invoices:pay"));
        assert!(!use_capability("__staff_only__"));
        set_contact_access_token(None);
        clear_contact_capabilities();
    }

    #[test]
    fn no_session_returns_false() {
        reset_session_state();
        assert!(!use_capability("tickets:read"));
        assert!(!use_capability("__staff_only__"));
    }

    #[test]
    fn use_any_capability_matches_when_any_present() {
        reset_session_state();
        set_contact_access_token(Some("contact-token".to_string()));
        set_contact_capabilities(Some(vec!["invoices:read".to_string()]));
        assert!(use_any_capability(&[
            "tickets:read",
            "invoices:read",
            "quotes:read"
        ]));
        set_contact_access_token(None);
        clear_contact_capabilities();
    }

    #[test]
    fn use_any_capability_returns_false_when_none_match() {
        reset_session_state();
        set_contact_access_token(Some("contact-token".to_string()));
        set_contact_capabilities(Some(vec!["kb:read".to_string()]));
        assert!(!use_any_capability(&[
            "tickets:read",
            "invoices:read",
            "quotes:read"
        ]));
        set_contact_access_token(None);
        clear_contact_capabilities();
    }

    /// MAPPS-625: a stale staff bearer left in memory while the
    /// visitor is signed in on the contact plane must NOT paint
    /// staff-only UI. The precedence rule in `use_capability` treats
    /// the contact session as authoritative when it is present, so
    /// `__staff_only__` returns false and the sidebar hides
    /// Companies / Contacts / Calendar / Reports / etc.
    #[test]
    fn contact_session_shadows_stale_staff_bearer() {
        reset_session_state();
        // Both tokens present at the same time.
        set_access_token_for_test(Some("stale-staff-token".to_string()));
        set_contact_access_token(Some("live-contact-token".to_string()));
        // The seeded "Read-Only" role only holds *:read caps; no
        // company / contact caps exist in the portal catalog at all.
        set_contact_capabilities(Some(vec![
            "tickets:read".to_string(),
            "invoices:read".to_string(),
            "quotes:read".to_string(),
            "contracts:read".to_string(),
            "assets:read".to_string(),
            "projects:read".to_string(),
            "kb:read".to_string(),
            "notifications:read".to_string(),
        ]));
        // Contact-scoped read caps pass.
        assert!(use_capability("tickets:read"));
        assert!(use_capability("invoices:read"));
        // Staff-only sidebar sentinel MUST fail even with the stale
        // staff bearer present.
        assert!(
            !use_capability(STAFF_ONLY),
            "__staff_only__ must return false while a contact session is live"
        );
        // Any-cap match still respects the contact snapshot.
        assert!(use_any_capability(&["tickets:read", "invoices:pay"]));
        assert!(!use_any_capability(&["invoices:pay", STAFF_ONLY]));
        set_access_token_for_test(None);
        set_contact_access_token(None);
        clear_contact_capabilities();
    }
}

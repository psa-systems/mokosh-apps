//! MAPPS-522: the one sign-out sequence, shared by every path that ends a
//! session.
//!
//! Signing out used to call only the identity provider's logout endpoint, so
//! the mokosh session that `POST /api/v1/auth/login` created on a standalone
//! deployment (MAPPS-368) was never revoked and outlived the sign-out meant to
//! end it. Every step now runs BEFORE the redirect and is awaited: the
//! provider does not return control to the SPA, so anything still unsent when
//! `location.replace` fires is never sent.
//!
//! Kept here rather than in the components so a fourth sign-out path cannot be
//! added without it; `sign_out_paths_go_through_the_shared_helper` below fails
//! the build if one is.

use crate::modules::oidc::{storage, OidcConfig};

/// Where the browser lands once it is signed out: this SPA's own origin root,
/// so the user sees mokosh's public landing page, signed out. Falls back to
/// the hub root when the origin is unavailable (desktop / SSR path).
fn post_logout_target(cfg: &OidcConfig) -> String {
    crate::platform::location::origin()
        .map(|origin| format!("{}/", origin.trim_end_matches('/')))
        .unwrap_or_else(|| cfg.hub_url("/"))
}

/// The URL sign-out ends on.
///
/// With an issuer configured that is bunyip's RP-initiated
/// `GET /v1/auth/logout?url=<absolute>`, which clears the `.a8n.systems`-scoped
/// cookies via Set-Cookie and then 302s straight to `url`.
pub fn logout_redirect_url() -> String {
    let cfg = OidcConfig::for_current_origin();
    let target = post_logout_target(&cfg);
    // MAPPS-368: a standalone deployment has no OP. Interpolating its empty
    // issuer produced the origin-relative `/v1/auth/logout?url=...`, which
    // resolves against this SPA and 404s, so go to the landing page directly.
    if !cfg.has_issuer() {
        return target;
    }
    let issuer = cfg.issuer.trim_end_matches('/');
    format!(
        "{issuer}/v1/auth/logout?url={}",
        crate::utils::url::encode_uri_component(&target)
    )
}

/// Revoke the mokosh session server-side with the bearer the SPA still holds.
///
/// Nothing to revoke when no agent bearer is held, so that is a quiet return
/// rather than an error. A portal contact holds a different token entirely and
/// ends their session through [`sign_out_portal`].
async fn revoke_mokosh_session() {
    #[cfg(feature = "app")]
    {
        use crate::hooks::fetch::api;

        if api::current_access_token().is_none() {
            return;
        }
        if let Err(e) = api::post_authed_no_content("/auth/logout").await {
            crate::platform::log::error(&format!(
                "sign-out: revoking the mokosh session failed, signing out anyway: {e}"
            ));
        }
    }
}

/// Revoke the OP's refresh-token family (MAPPS-336) so a leaked refresh token
/// does not survive the user clicking "Log out".
async fn revoke_provider_refresh_token() {
    let refresh = match storage::load_auth().and_then(|t| t.refresh_token) {
        Some(r) => r,
        None => return,
    };
    let cfg = OidcConfig::for_current_origin();
    if let Err(e) = crate::modules::oidc::flow::revoke_refresh_token(&cfg, &refresh).await {
        crate::platform::log::error(&format!(
            "sign-out: revoking the refresh token at the OP failed, signing out anyway: {e:?}"
        ));
    }
}

/// End every session this SPA holds, then leave for [`logout_redirect_url`].
///
/// A failed revoke is logged and does NOT block the redirect: leaving the user
/// signed in to the SPA because revocation failed is worse than a stale
/// session elsewhere.
///
/// The local clear happens last, after both revokes have read what they need,
/// and the hard navigation (rather than a router push) is what resets the
/// in-memory auth state: writing to the auth signal first would re-render into
/// the route guard, which pushes `/login` and races the redirect.
pub async fn sign_out() {
    revoke_mokosh_session().await;
    revoke_provider_refresh_token().await;
    storage::clear_auth();
    crate::platform::location::replace(&logout_redirect_url());
}

/// Where a portal sign-out lands: this SPA's own `/portal/login`.
///
/// Never the OP's logout endpoint. A portal identity is a `contacts` row that
/// has no bunyip account at all, so sending it to bunyip's sign-in page was
/// sending a customer somewhere they cannot sign in (MAPPS-532).
///
/// The account name the login form wants is not carried over: `tenant_slug` is
/// part of the portal credential and this SPA never stored the one that was
/// used, so the customer retypes it. Better than the alternative, which was
/// landing them on the wrong product entirely.
pub fn portal_login_url() -> String {
    crate::platform::location::origin()
        .map(|origin| format!("{}/portal/login", origin.trim_end_matches('/')))
        .unwrap_or_else(|| "/portal/login".to_string())
}

/// Revoke the portal session server-side (MAPPS-532).
///
/// `POST /portal/auth/logout` stamps a cutoff on the contact, so every portal
/// token they hold stops being accepted; without this the token stayed valid
/// for the rest of its 8-hour life after the customer clicked Logout.
///
/// Runs before the holder is cleared, because it needs the token it is
/// revoking.
async fn revoke_portal_session() {
    #[cfg(feature = "app")]
    {
        use crate::hooks::fetch::api;

        if !api::has_portal_session() {
            return;
        }
        if let Err(e) = api::post_portal_authed_no_content("/portal/auth/logout").await {
            crate::platform::log::error(&format!(
                "sign-out: revoking the portal session failed, signing out anyway: {e}"
            ));
        }
    }
}

/// End the portal session and leave for [`portal_login_url`].
///
/// The portal's own sequence, not a branch inside [`sign_out`], because both
/// token holders can be populated in one tab: an agent who opens
/// `/portal/login` and signs in as a contact holds an agent bearer AND a
/// portal token, so a global `has_portal_session()` test would route that
/// agent's own sign-out down this path. Which menu was clicked is the fact
/// that decides, and only the caller knows it.
///
/// Same failure posture as [`sign_out`]: a failed revoke is logged and does
/// not block the exit.
pub async fn sign_out_portal() {
    revoke_portal_session().await;
    #[cfg(feature = "app")]
    crate::hooks::fetch::api::set_portal_access_token(None);
    crate::platform::location::replace(&portal_login_url());
}

#[cfg(test)]
mod tests {
    /// Every component that ends a session. A new one added here without a
    /// `sign_out` call fails the scan below.
    const SIGN_OUT_SITES: &[(&str, &str)] = &[
        (
            "components/layout.rs",
            include_str!("../../components/layout.rs"),
        ),
        (
            "components/account_deleted_overlay.rs",
            include_str!("../../components/account_deleted_overlay.rs"),
        ),
    ];

    /// MAPPS-522 recurrence gate. A sign-out path that builds the provider
    /// logout URL for itself is a path that skipped the mokosh revoke, which
    /// is the whole defect. The URL is built in exactly one place.
    #[test]
    fn sign_out_paths_go_through_the_shared_helper() {
        for (name, src) in SIGN_OUT_SITES {
            assert!(
                !src.contains("v1/auth/logout"),
                "{name} builds the provider logout URL itself; it must call \
                 `modules::auth::sign_out::sign_out`, which revokes the mokosh \
                 session first"
            );
            assert!(
                src.contains("sign_out::sign_out()"),
                "{name} is listed as a sign-out site but never calls the shared helper"
            );
        }
    }

    /// This module's own source, minus its tests (which name the same symbols
    /// in their assertion messages).
    fn production_src() -> &'static str {
        include_str!("sign_out.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields a first segment")
    }

    /// The body of a `fn` declared at module level here: from its signature to
    /// the next one.
    fn body_of(name: &str) -> String {
        let needle = format!("fn {name}(");
        let start = production_src()
            .find(&needle)
            .unwrap_or_else(|| panic!("{name} is defined in this file"));
        let rest = &production_src()[start + needle.len()..];
        let end = rest
            .find("\nfn ")
            .or_else(|| rest.find("\npub "))
            .unwrap_or(rest.len());
        rest[..end].to_string()
    }

    /// MAPPS-532: the portal identity has no bunyip account, so its sign-out
    /// must never hand the browser to the OP. The two exits are separate
    /// functions precisely so this is checkable.
    #[test]
    fn the_portal_sign_out_never_leaves_for_the_provider() {
        let portal = body_of("sign_out_portal");
        assert!(
            !portal.contains("logout_redirect_url"),
            "sign_out_portal sends a `contacts` identity to the OP's logout \
             endpoint, which is a sign-in page for an account they do not have"
        );
        assert!(
            portal.contains("portal_login_url"),
            "sign_out_portal must land on this SPA's own /portal/login"
        );
    }

    /// MAPPS-532: the revoke is the point of the change. Without the call the
    /// portal token stays valid for the rest of its 8-hour TTL, which is the
    /// defect this module already exists to prevent on the agent side.
    #[test]
    fn the_portal_sign_out_revokes_before_it_clears_and_leaves() {
        let revoke = body_of("revoke_portal_session");
        assert!(
            revoke.contains("/portal/auth/logout"),
            "nothing calls the portal revoke endpoint"
        );

        let portal = body_of("sign_out_portal");
        let revoked = portal
            .find("revoke_portal_session")
            .expect("sign_out_portal revokes the session");
        let cleared = portal
            .find("set_portal_access_token")
            .expect("sign_out_portal clears the portal token holder");
        let left = portal
            .find("location::replace")
            .expect("sign_out_portal leaves the page");
        assert!(
            revoked < cleared && cleared < left,
            "the revoke needs the token it is revoking, and the navigation never \
             returns control to this SPA, so the order is revoke, clear, leave"
        );
    }

    // mokosh-contact-login (prompt 001): the MAPPS-532 test that pinned
    // `PortalUserMenu` to `sign_out_portal()` retired with the menu itself
    // when the customer-portal `/portal/*` route family was dropped. See
    // `src/components/layout.rs` above line 1483: "PortalLayout +
    // PortalUserMenu retired with the customer-portal /portal/* route
    // family (prompt 001)". Contact-plane pages under
    // `src/pages/contact_portal/` render their own layout and their own
    // sign-out wiring; a new pin over that surface belongs there.

    /// The fallback matters as much as the happy path: a host with no
    /// `window` (the desktop build) must still get an origin-relative portal
    /// route, never the hub URL the agent path falls back to.
    #[test]
    fn the_portal_login_url_is_this_origins_own_route() {
        let url = super::portal_login_url();
        assert!(
            url.ends_with("/portal/login"),
            "portal sign-out must land on /portal/login, got {url}"
        );
        assert!(
            !url.contains("v1/auth/logout"),
            "portal sign-out must not land on the OP, got {url}"
        );
    }
}

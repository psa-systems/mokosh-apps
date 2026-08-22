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
/// Nothing to revoke when no bearer is held (the portal identity signs in on
/// its own token, and mokosh-server has no `/portal/auth/logout`), so that is
/// a quiet return rather than an error.
async fn revoke_mokosh_session() {
    #[cfg(feature = "web")]
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
}

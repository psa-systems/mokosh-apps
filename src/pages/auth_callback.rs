//! `/auth/callback`: lands here after the user signs in at mokosh-server.
//!
//! Reads `code` + `state` from the query string, exchanges them at the
//! token endpoint via `oidc::complete_login`, stashes the tokens in
//! [`AuthContext`], and routes back to wherever the original
//! `start_login` requested.

use dioxus::prelude::*;

use crate::hooks::use_auth;
use crate::modules::auth::{CurrentUser, UserRole};
use crate::modules::oidc::{complete_login, OidcConfig};
use crate::Route;

#[component]
pub fn AuthCallbackPage() -> Element {
    let mut auth = use_auth();
    let navigator = use_navigator();
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    use_future(move || async move {
        let cfg = OidcConfig::for_current_origin();
        match complete_login(&cfg).await {
            Ok((tokens, return_to)) => {
                let claims = match tokens.id_claims() {
                    Ok(c) => c,
                    Err(e) => {
                        error_msg.set(Some(format!("invalid id_token: {e}")));
                        return;
                    }
                };
                let user_id = match claims.sub.parse::<uuid::Uuid>() {
                    Ok(id) => id,
                    Err(e) => {
                        // Reject an unparseable subject id rather than
                        // substituting `Uuid::nil()`: a nil id can collide
                        // or corrupt tenant-scoped writes silently.
                        error_msg.set(Some(format!("invalid subject id: {e}")));
                        return;
                    }
                };
                let tenant_id = claims
                    .tenant_id
                    .as_deref()
                    .and_then(|s| s.parse::<uuid::Uuid>().ok())
                    .unwrap_or_else(uuid::Uuid::nil);
                // Map mokosh-server's role string onto the client enum via
                // the single canonical `UserRole::parse_role` parser (PMS-158,
                // same path used at auth.rs:206 / :424). The inline match it
                // replaces only handled admin/manager/finance and silently
                // downgraded super_admin/technician/dispatcher/sales to the
                // default role on the OIDC-callback path. Unknown / unset
                // values fall back to Technician (the default service role)
                // so a fresh user still gets a usable session.
                let role = claims
                    .role
                    .as_deref()
                    .map(UserRole::parse_role)
                    .unwrap_or_default();
                // Make the access token available to api::*_authed
                // helpers across the app. Stored in the same in-memory
                // holder used by every authed fetch call; no localStorage.
                crate::hooks::fetch::api::set_access_token(Some(tokens.access_token.clone()));
                // Persist the token bundle to sessionStorage BEFORE any
                // navigation. The post-login route can be reached two
                // ways: an internal `navigator.push` (no reload, the
                // in-memory `auth.write` above carries) OR a hard
                // `location.set_href` for absolute return_to URLs. The
                // hard reload tears the in-memory `AuthContext` down,
                // and without a saved bundle `rehydrate_from_storage`
                // finds nothing on the next boot: `AuthGuard` sees
                // `is_authenticated = false` and immediately fires
                // another `start_login`, looping the user back through
                // OIDC instead of landing them on the dashboard.
                // `use_token_refresh` writes to sessionStorage on every
                // rotation but that only fires 60s+ later. Save here
                // so the very next render of any route is authed.
                crate::modules::oidc::storage::save_auth(
                    &crate::modules::oidc::storage::StoredTokens {
                        access_token: tokens.access_token.clone(),
                        id_token: tokens.id_token.clone(),
                        refresh_token: tokens.refresh_token.clone(),
                        expires_at: tokens.expires_at,
                        scope: tokens.scope.clone(),
                    },
                );
                {
                    let mut a = auth.write();
                    a.user = Some(CurrentUser {
                        id: user_id,
                        tenant_id,
                        email: claims.email.clone().unwrap_or_default(),
                        first_name: String::new(),
                        last_name: String::new(),
                        role,
                        timezone: "UTC".to_string(),
                        avatar_url: None,
                        // Optimistic default: the post-login /me fetch
                        // (use_memberships_loader / use_token_refresh)
                        // overwrites this with the authoritative value
                        // within a tick. New Bunyip-JIT users land with
                        // `profile_completed: false` from /me and the
                        // onboarding gate redirects them.
                        profile_completed: true,
                        date_format_string: None,
                        // The id_token carries no own-company claim; the
                        // post-login /me fetch fills it within a tick.
                        own_company_id: None,
                    });
                    a.is_loading = false;
                    a.error = None;
                    a.tokens = Some(tokens);
                }
                // Land back on the originally requested page (MAPPS-323).
                //
                // No specific target (`""` / `"/"`): go to the dashboard via
                // a router push so the in-memory `AuthContext` written above
                // survives - a hard reload would tear the signal down between
                // the write and the next render.
                //
                // A concrete same-origin deep link (e.g. `/tickets/new`):
                // the Dioxus router only knows `Route` variants and cannot
                // navigate to an arbitrary path string, so restore it with a
                // hard nav. The token bundle was saved to sessionStorage
                // above, so `rehydrate_from_storage` re-authes the rebooted
                // tree on the next boot - no re-login loop. A protocol-
                // relative `//host` is rejected (off-origin redirect guard).
                let safe_internal = return_to.starts_with('/') && !return_to.starts_with("//");
                if return_to.is_empty() || return_to == "/" || !safe_internal {
                    navigator.push(Route::Dashboard {});
                } else if let Some(win) = web_sys::window() {
                    let _ = win.location().set_href(&return_to);
                } else {
                    navigator.push(Route::Dashboard {});
                }
            }
            Err(e) => error_msg.set(Some(e.to_string())),
        }
    });

    rsx! {
        div { class: "min-h-screen flex items-center justify-center",
            div { class: "text-center space-y-4",
                if let Some(err) = error_msg.read().as_ref() {
                    h1 { class: "text-xl font-semibold text-red-600", "Sign-in failed" }
                    p { class: "text-content", "{err}" }
                    a { href: "/login", class: "text-accent underline", "Try again" }
                } else {
                    h1 { class: "text-xl", "Signing you in..." }
                }
            }
        }
    }
}

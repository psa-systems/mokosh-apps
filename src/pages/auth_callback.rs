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
        let exchange_result = complete_login(&cfg).await;
        // MAPPS-336: strip `?code=` + `?state=` from window.location BEFORE
        // any further navigation runs. Relying on the Dioxus router's
        // history.replaceState side-effect was racy: the code lingered in
        // browser history and in Referer headers on the next request when
        // the router skipped or deferred the rewrite. Replace happens on
        // both success and error so a failed exchange does not leave the
        // sensitive query string sitting in the URL bar either.
        #[cfg(feature = "web")]
        if let Some(win) = web_sys::window() {
            if let Ok(history) = win.history() {
                let _ = history.replace_state_with_url(
                    &wasm_bindgen::JsValue::NULL,
                    "",
                    Some("/auth/callback"),
                );
            }
        }
        match exchange_result {
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
                // Role comes from `/api/v1/auth/me` (PMS-158); the id_token
                // carries no usable role claim post-cutover, so seed the
                // Technician default and let the post-login /me fetch reconcile
                // it within a tick.
                let role = UserRole::default();
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
                        // Optimistic None; the post-login /me fetch reconciles
                        // the authoritative theme prefs within a tick.
                        theme_base_mode: None,
                        theme_accent_id: None,
                        // The id_token carries no own-company claim; the
                        // post-login /me fetch fills it within a tick.
                        own_company_id: None,
                    });
                    a.is_loading = false;
                    a.error = None;
                    a.tokens = Some(tokens);
                }
                // Land back on the originally requested page (MAPPS-323).
                // `classify_return_to` (unit-tested) decides:
                //
                // - Dashboard: no specific target (`""` / `/` / `/dashboard`,
                //   the interactive-login default), an off-origin / scheme
                //   target, or an auth-plumbing route. Soft router push so the
                //   in-memory `AuthContext` written above survives - a hard
                //   reload would tear the signal down between the write and the
                //   next render, and `/dashboard` does not warrant a redundant
                //   full-page reload (the callback is already a fresh boot).
                //
                // - Restore: a concrete same-origin deep link (e.g.
                //   `/tickets/new`). The Dioxus router cannot navigate to an
                //   arbitrary path string, so restore it with a hard nav. The
                //   token bundle was saved to sessionStorage above, so
                //   `rehydrate_from_storage` re-authes the rebooted tree - no
                //   re-login loop.
                match crate::modules::oidc::classify_return_to(&return_to) {
                    crate::modules::oidc::ReturnTarget::Restore => {
                        if let Some(win) = web_sys::window() {
                            let _ = win.location().set_href(&return_to);
                        } else {
                            navigator.push(Route::Dashboard {});
                        }
                    }
                    crate::modules::oidc::ReturnTarget::Dashboard => {
                        navigator.push(Route::Dashboard {});
                    }
                }
            }
            Err(e) => error_msg.set(Some(e.to_string())),
        }
    });

    // MAPPS-355: `storage:` errors (missing pending flow, expired flow) are the
    // common "opened /auth/callback in a new tab / refreshed after MAPPS-336
    // stripped the query" cases. Nothing an operator needs to look at: just
    // start a fresh login. Auto-navigate to `/login` (which kicks
    // `start_login`) so the user does not sit on a red "Sign-in failed" wall.
    // CSRF / replay / config errors keep the manual screen because they can
    // indicate a real problem that a silent retry would loop on.
    let is_retriable = error_msg
        .read()
        .as_ref()
        .is_some_and(|e| e.starts_with("storage:"));
    #[cfg(feature = "web")]
    use_effect(move || {
        if is_retriable {
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/login");
            }
        }
    });

    rsx! {
        div { class: "min-h-screen flex items-center justify-center",
            div { class: "text-center space-y-4",
                if is_retriable {
                    h1 { class: "text-xl", "Signing you in…" }
                } else if let Some(err) = error_msg.read().as_ref() {
                    h1 { class: "text-xl font-semibold text-red-600", "Sign-in failed" }
                    p { class: "text-content", "{err}" }
                    a { href: "/login", class: "text-accent underline", "Try again" }
                } else {
                    h1 { class: "text-xl", "Signing you in…" }
                }
            }
        }
    }
}

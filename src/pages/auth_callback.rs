//! `/auth/callback`: lands here after the user signs in at mokosh-server.
//!
//! Reads `code` + `state` from the query string, exchanges them at the
//! token endpoint via `oidc::complete_login`, stashes the tokens in
//! [`AuthContext`], and routes back to wherever the original
//! `start_login` requested.
//!
//! MAPPS-432: a failed exchange is routed by `classify_flow_error`, which
//! matches on the `FlowError` variant (never on its message text). Errors that
//! only mean "no live authorization flow on this URL" restart the login flow
//! silently via [`restart_login`]; everything else renders the error screen.

use dioxus::prelude::*;

use crate::hooks::use_auth;
use crate::modules::auth::{CurrentUser, UserRole};
use crate::modules::oidc::storage::{
    bump_callback_retry, clear_callback_retry, MAX_CALLBACK_RETRIES,
};
use crate::modules::oidc::{
    classify_flow_error, complete_login, log_auth_error, CallbackRecovery, OidcConfig,
};
use crate::Route;

/// MAPPS-432: silently re-kick the login flow after a recoverable callback
/// failure. `/login` (not a reload of `/auth/callback`, which MAPPS-336 strips
/// of `code`/`state`) is the only URL that re-runs `start_login`, and a hard
/// `replace` both re-runs `snapshot_initial_search` and keeps the dead callback
/// URL out of history.
///
/// `Err` means the caller must fall through to the visible error screen rather
/// than leave the user on `Signing you in…`: either the restart budget is
/// spent, the counter is unusable (an unbounded loop otherwise), or the
/// navigation itself failed.
fn restart_login(underlying: &str) -> Result<(), String> {
    let attempt = bump_callback_retry()?;
    if attempt > MAX_CALLBACK_RETRIES {
        return Err(format!(
            "restart budget spent ({MAX_CALLBACK_RETRIES} allowed, this is attempt {attempt})"
        ));
    }
    log_auth_error(&format!(
        "auth callback: restarting the login flow (attempt {attempt} of {MAX_CALLBACK_RETRIES}): {underlying}"
    ));
    // MAPPS-518 URL swap: tenant login lives at /client/login now.
    crate::platform::location::set_href("/client/login")
}

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
        // MAPPS-504: browser-only, and it has nothing to strip anywhere
        // else - the desktop build never reaches this route, because it
        // has no redirect to come back from (MAPPS-505).
        #[cfg(all(feature = "app", target_arch = "wasm32"))]
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
                // MAPPS-630: cross-plane isolation. Clear any
                // contact session (in-memory + localStorage refresh
                // + caps + brand) BEFORE we write the fresh staff
                // bearer, so the tab reads as staff from this point
                // on and the AuthGuard cold-load bootstrap can not
                // resurrect a stale portal session.
                crate::hooks::fetch::api::on_staff_signin_clear_contact_side();
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
                // MAPPS-432: a completed exchange ends the recoverable-failure
                // streak, so a reload later in this tab gets a full budget.
                if let Err(e) = clear_callback_retry() {
                    log_auth_error(&format!(
                        "auth callback: could not clear the restart counter: {e}"
                    ));
                }
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
                        // (use_active_org_loader / use_token_refresh)
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
                        // PMS-791 / MAPPS-462: no tenant_kind claim on
                        // the id_token; /me reconciles within a tick.
                        // Empty default reads as org via
                        // AuthState::is_org_tenant (fail-open UI).
                        tenant_kind: String::new(),
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
                        if let Err(e) = crate::platform::location::set_href(&return_to) {
                            tracing::warn!("could not restore {return_to}: {e}");
                            navigator.push(Route::Dashboard {});
                        }
                    }
                    crate::modules::oidc::ReturnTarget::Dashboard => {
                        navigator.push(Route::Dashboard {});
                    }
                }
            }
            // MAPPS-355/MAPPS-432: a missing or expired `PendingFlow`, a bare
            // `/auth/callback` (reload, restored tab, bookmark), and the OP's
            // re-authentication signals all mean "no live flow here". Nothing an
            // operator needs to look at: start a fresh login rather than parking
            // the user on a red "Sign-in failed" wall. CSRF / replay / config
            // errors keep the visible screen because they can indicate a real
            // problem that a silent retry would loop on, and a restart that
            // cannot be bounded or cannot navigate falls through to it too.
            Err(e) => {
                let msg = e.to_string();
                match classify_flow_error(&e) {
                    CallbackRecovery::Show => error_msg.set(Some(msg)),
                    CallbackRecovery::Restart => {
                        if let Err(reason) = restart_login(&msg) {
                            log_auth_error(&format!(
                                "auth callback: showing the error instead of restarting ({reason}): {msg}"
                            ));
                            error_msg.set(Some(msg));
                        }
                    }
                }
            }
        }
    });

    rsx! {
        div { class: "min-h-screen flex items-center justify-center",
            div { class: "text-center space-y-4",
                if let Some(err) = error_msg.read().as_ref() {
                    h1 { class: "text-2xl font-semibold text-content", "Sign-in failed" }
                    p { class: "text-content", "{err}" }
                    // MAPPS-632: routed `Link`, not a raw `<a href>` - the desktop
                    // webview refuses an internal navigation, so the only way out
                    // of a failed sign-in would silently do nothing.
                    Link { to: Route::Login {}, class: "text-accent underline", "Try again" }
                } else {
                    h1 { class: "text-2xl font-semibold text-content", "Signing you in…" }
                }
            }
        }
    }
}

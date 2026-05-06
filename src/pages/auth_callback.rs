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
        let cfg = OidcConfig::from_env();
        match complete_login(&cfg).await {
            Ok((tokens, return_to)) => {
                let claims = match tokens.id_claims() {
                    Ok(c) => c,
                    Err(e) => {
                        error_msg.set(Some(format!("invalid id_token: {e}")));
                        return;
                    }
                };
                let user_id = claims
                    .sub
                    .parse::<uuid::Uuid>()
                    .unwrap_or_else(|_| uuid::Uuid::nil());
                let tenant_id = claims
                    .tenant_id
                    .as_deref()
                    .and_then(|s| s.parse::<uuid::Uuid>().ok())
                    .unwrap_or_else(uuid::Uuid::nil);
                // Map mokosh-server's role enum onto the client's
                // (the client has different "Technician/Dispatcher/Sales"
                // axes that the server-side IdP doesn't model). Unknown
                // values fall back to Technician (the default service
                // role) so a fresh user still gets a usable session.
                let role = claims
                    .role
                    .as_deref()
                    .and_then(|s| match s {
                        "admin" => Some(UserRole::Admin),
                        "manager" => Some(UserRole::Manager),
                        "finance" => Some(UserRole::Finance),
                        _ => None,
                    })
                    .unwrap_or_default();
                // Make the access token available to api::*_authed
                // helpers across the app. Stored in the same in-memory
                // holder used by every authed fetch call; no localStorage.
                crate::hooks::fetch::api::set_access_token(Some(tokens.access_token.clone()));
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
                    });
                    a.is_loading = false;
                    a.error = None;
                    a.tokens = Some(tokens);
                }
                // Land back on the originally requested page, or dashboard.
                if return_to.is_empty() || return_to == "/" {
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
                    p { class: "text-gray-600", "{err}" }
                    a { href: "/login", class: "text-blue-600 underline", "Try again" }
                } else {
                    h1 { class: "text-xl", "Signing you in..." }
                }
            }
        }
    }
}

//! PMS-1208: the grant-invitation accept surface.
//!
//! Reached by the invitation email link
//! (`{spa_base_url}/accept-grant?token={plaintext}`) sent by
//! mokosh-server's `GrantInvitationsService::create` when the owner
//! invites another Bunyip user to see their Mokosh account.
//!
//! The token is the ONE credential the visitor has here: they may
//! not yet be signed in, and in standalone mode they may not have
//! a Bunyip identity at all. The page therefore renders the
//! metadata read (`GET /grants/invitations/by-token/{token}`, which
//! is unauthenticated at the middleware level) first and only
//! then sends the accept POST, which does need a signed-in caller
//! and bounces through OIDC if the visitor is not yet signed in.
//!
//! Two happy paths - Accept and Decline - and four refusal shapes
//! from the server (404 NotFound, 410 Gone for canceled/expired,
//! 409 Conflict for already-accepted/declined, 403 Forbidden for
//! wrong-caller). Each maps to a specific message so a customer
//! landing here from a stale email link reads exactly what
//! happened.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{AuthLayout, Button, ButtonVariant};

#[derive(Deserialize, Clone, Debug)]
struct InvitationMetadata {
    mokosh_account_name: String,
    role: String,
    #[serde(default)]
    status: String,
}

/// Empty request bodies for the two POST verbs. The token is in
/// the path; the server does not read a body.
#[derive(Serialize)]
struct Empty {}

#[component]
pub fn AcceptGrantPage(token: String) -> Element {
    let mut error = use_signal(String::new);
    let mut info = use_signal(String::new);
    let mut acting = use_signal(|| false);
    let mut done = use_signal(|| false);

    // PMS-1208 SPA: the page is public (the token IS the credential
    // for the metadata read), but the Accept + Decline POSTs need a
    // signed-in caller so mokosh-server can bind the invitation to
    // their identity. A visitor who clicked the email link in a
    // fresh tab has no in-memory bearer; without the check here the
    // fetch layer would attach nothing, the server would 401, and
    // the SPA would render "Your session has ended" - the exact
    // shape the tester hit. Reading `use_auth()` and bouncing
    // through OIDC with the current URL as `return_to` when the
    // caller is not authenticated fixes it: the callback restores
    // this same `/accept-grant?token=<t>` page after sign-in and
    // the buttons work.
    let auth = crate::hooks::use_auth();
    let is_signed_in = auth.read().is_authenticated();

    let token_for_meta = token.clone();
    let metadata: Resource<Option<InvitationMetadata>> = use_resource(move || {
        let token = token_for_meta.clone();
        async move {
            #[cfg(feature = "web")]
            {
                let path = format!("/grants/invitations/by-token/{token}");
                crate::hooks::fetch::api::get_typed::<InvitationMetadata>(&path)
                    .await
                    .inspect_err(|e| {
                        tracing::warn!("grant invitation metadata load failed: {e}");
                    })
                    .ok()
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = token;
                None
            }
        }
    });

    let metadata_snap = metadata.read_unchecked();
    let metadata_ready = metadata_snap.is_some();
    let metadata_body: Option<InvitationMetadata> = match &*metadata_snap {
        Some(inner) => inner.clone(),
        None => None,
    };

    let token_for_accept = token.clone();
    let accept = move |_| {
        if acting() {
            return;
        }
        let tok = token_for_accept.clone();
        acting.set(true);
        error.set(String::new());
        info.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let path = format!("/grants/invitations/by-token/{tok}/accept");
                match crate::hooks::fetch::api::post_authed_typed::<serde_json::Value, _>(
                    &path,
                    &Empty {},
                )
                .await
                {
                    Ok(_) => {
                        info.set("Invitation accepted. Redirecting to the account.".to_string());
                        done.set(true);
                        // A hard nav is safer than a router push here:
                        // the OIDC session should refetch memberships so
                        // the new tenant appears in the switcher, and the
                        // simplest way to make that reliable is a full
                        // page load.
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.location().replace("/");
                        }
                    }
                    Err(err) => {
                        error.set(match err {
                            ApiError::Status { code: 401, .. } => {
                                // Bearer missing or stale. Send them
                                // through OIDC again with the current
                                // URL as return_to so the callback
                                // brings them back here signed in.
                                #[cfg(feature = "web")]
                                {
                                    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
                                    let return_to = crate::modules::oidc::current_return_to();
                                    if let Err(e) = crate::modules::oidc::start_login(&cfg, return_to) {
                                        crate::modules::oidc::log_auth_error(&format!(
                                            "accept-grant: login kickoff failed on 401: {e}"
                                        ));
                                    }
                                }
                                "Your sign-in expired. Sending you back to sign in again...".to_string()
                            }
                            ApiError::Status { code: 404, .. } => {
                                "This invitation link is not valid.".to_string()
                            }
                            ApiError::Status { code: 410, .. } => {
                                "This invitation has expired or been canceled. Ask the sender for a new one.".to_string()
                            }
                            ApiError::Status { code: 409, .. } => {
                                "This invitation has already been used.".to_string()
                            }
                            ApiError::Status { code: 403, .. } => {
                                "This invitation was sent to a different account. Sign in as the invited user.".to_string()
                            }
                            other => other.user_message(),
                        });
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = tok;
            }
            acting.set(false);
        });
    };

    let token_for_decline = token.clone();
    let decline = move |_| {
        if acting() {
            return;
        }
        let tok = token_for_decline.clone();
        acting.set(true);
        error.set(String::new());
        info.set(String::new());
        spawn(async move {
            #[cfg(feature = "web")]
            {
                use crate::hooks::fetch::api::ApiError;
                let path = format!("/grants/invitations/by-token/{tok}/decline");
                match crate::hooks::fetch::api::post_authed_typed::<serde_json::Value, _>(
                    &path,
                    &Empty {},
                )
                .await
                {
                    Ok(_) => {
                        info.set("Invitation declined.".to_string());
                        done.set(true);
                    }
                    Err(err) => {
                        error.set(match err {
                            ApiError::Status { code: 401, .. } => {
                                #[cfg(feature = "web")]
                                {
                                    let cfg =
                                        crate::modules::oidc::OidcConfig::for_current_origin();
                                    let return_to = crate::modules::oidc::current_return_to();
                                    if let Err(e) =
                                        crate::modules::oidc::start_login(&cfg, return_to)
                                    {
                                        crate::modules::oidc::log_auth_error(&format!(
                                            "accept-grant: login kickoff failed on 401: {e}"
                                        ));
                                    }
                                }
                                "Your sign-in expired. Sending you back to sign in again..."
                                    .to_string()
                            }
                            ApiError::Status { code: 404, .. } => {
                                "This invitation link is not valid.".to_string()
                            }
                            ApiError::Status { code: 410, .. } => {
                                "This invitation has expired or been canceled.".to_string()
                            }
                            ApiError::Status { code: 409, .. } => {
                                "This invitation has already been used.".to_string()
                            }
                            ApiError::Status { code: 403, .. } => {
                                "This invitation was sent to a different account.".to_string()
                            }
                            other => other.user_message(),
                        });
                    }
                }
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = tok;
            }
            acting.set(false);
        });
    };

    rsx! {
        AuthLayout {
            h1 { class: "text-xl font-semibold mb-4", "You have been invited" }
            if !metadata_ready {
                p { class: "text-subtle", "Loading invitation..." }
            } else if let Some(meta) = metadata_body.as_ref() {
                {
                    let status = meta.status.as_str();
                    let terminal = matches!(status, "accepted" | "declined" | "canceled" | "expired");
                    let role_display = humanise_role(&meta.role);
                    let account = meta.mokosh_account_name.clone();
                    let account_for_body = account.clone();
                    rsx! {
                        if terminal {
                            p { class: "text-content",
                                "This invitation to " strong { "{account_for_body}" } " is "
                                span { class: "font-medium", "{terminal_label(status)}" } "."
                            }
                            p { class: "text-subtle mt-2",
                                "Ask the sender for a new invitation if you still need access."
                            }
                        } else {
                            p { class: "text-content",
                                "You have been invited to access " strong { "{account_for_body}" }
                                " as " strong { "{role_display}" } "."
                            }
                            p { class: "text-subtle mt-2",
                                "If you accept, this account will appear in your workspace switcher and you can leave at any time."
                            }
                            if !is_signed_in {
                                // Not signed in: the accept POST needs a
                                // bearer, so send them through OIDC first
                                // with the current URL as `return_to`.
                                // After sign-in the callback restores this
                                // exact `/accept-grant?token=<t>` and the
                                // Accept button below becomes clickable.
                                p { class: "text-subtle mt-2",
                                    "Sign in first, then accept the invitation."
                                }
                                div { class: "flex gap-3 mt-6",
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        onclick: move |_| {
                                            #[cfg(feature = "web")]
                                            {
                                                let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
                                                let return_to = crate::modules::oidc::current_return_to();
                                                if let Err(e) = crate::modules::oidc::start_login(&cfg, return_to) {
                                                    crate::modules::oidc::log_auth_error(&format!(
                                                        "accept-grant: login kickoff failed: {e}"
                                                    ));
                                                    error.set(
                                                        "Sign-in failed to start. Reload the page and try again."
                                                            .to_string(),
                                                    );
                                                }
                                            }
                                        },
                                        "Sign in to accept"
                                    }
                                }
                            } else {
                                div { class: "flex gap-3 mt-6",
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        disabled: acting() || done(),
                                        onclick: accept,
                                        "Accept invitation"
                                    }
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        disabled: acting() || done(),
                                        onclick: decline,
                                        "Decline"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "text-content",
                    "This invitation link is not valid or has already been used."
                }
                p { class: "text-subtle mt-2",
                    "Ask the sender for a fresh invitation if you still need access."
                }
            }
            if !error().is_empty() {
                p { role: "alert", class: "mt-4 text-sm text-red-600 dark:text-red-400", "{error}" }
            }
            if !info().is_empty() {
                p { class: "mt-4 text-sm text-content", "{info}" }
            }
        }
    }
}

/// Server sends the PMS-1162 vocab as-is; the SPA titles it for the
/// heading so `manager` reads as `Manager` and `read_only` as
/// `Read only`. Matches the mokosh-server `role_display` helper
/// exactly so the wire and the UI never disagree.
fn humanise_role(role: &str) -> String {
    match role {
        "admin" => "Admin".to_string(),
        "manager" => "Manager".to_string(),
        "technician" => "Technician".to_string(),
        "finance" => "Finance".to_string(),
        "read_only" => "Read only".to_string(),
        other => other.to_string(),
    }
}

/// Terminal-state label. Kept as a small function so the four
/// arms and the fall-through are visible together.
fn terminal_label(status: &str) -> &'static str {
    match status {
        "accepted" => "already accepted",
        "declined" => "declined",
        "canceled" => "canceled",
        "expired" => "expired",
        _ => "closed",
    }
}

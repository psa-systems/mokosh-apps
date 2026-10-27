//! MAPPS-1084: landing page for the grant-invitation email link.
//!
//! The owner-outbox flow mints an invitation and bunyip emails the grantee a
//! link that lands here as `/accept-grant?token=<plaintext>`. The page is
//! behind `AuthGuard` so a signed-out grantee runs through sign-in first and
//! returns here; it fetches the invitation metadata by token
//! (`GET /api/v1/grants/invitations/by-token/{token}`), renders the inviter
//! name (PMS-1499), account name, role and expiry, and offers Accept /
//! Decline (`POST .../accept`, `POST .../decline`).
//!
//! On accept, the server returns `InvitationResponse` with `tenant_slug`
//! (PMS-1499) and a membership row materialises asynchronously. The page
//! refetches the memberships list, bumps the tenant generation (so any
//! `use_resource` under the active tenant refetches, matching how
//! `TenantSwitcher` installs a session), and navigates to `/dashboard`.
//! Decline shows a brief confirmation and routes to `/dashboard` too.
//!
//! Error shapes:
//! - A 404 or 410 on the metadata GET renders "This invitation is not valid"
//!   with no accept/decline affordance (an unauthenticated token that has
//!   expired or been revoked).
//! - A 403 on accept/decline (the signed-in user does not match the
//!   invitation's `invitee_email`) renders a short "This invitation is
//!   addressed to <email>" banner.
//! - Any other transport failure shows an inline retry affordance.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorBanner, PageHeader,
};
use crate::Route;

/// Shape of `GET /api/v1/grants/invitations/by-token/{token}`
/// (`InvitationMetadata` on the server, grant_invitations_routes.rs:207).
///
/// `inviter_name` arrives with PMS-1499; a server that predates the fix
/// omits it, which serde's `#[default]` treats as `None` and the renderer
/// falls back to a generic "You were invited to..." line.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct InvitationMetadata {
    #[serde(default)]
    pub(crate) mokosh_account_name: String,
    #[serde(default)]
    pub(crate) mokosh_account_slug: String,
    #[serde(default)]
    pub(crate) inviter_name: Option<String>,
    #[serde(default)]
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) expires_at: String,
    #[serde(default)]
    pub(crate) status: String,
}

/// Shape of `POST /api/v1/grants/invitations/by-token/{token}/accept`.
///
/// The server returns `InvitationResponse` (grant_invitations_routes.rs:98),
/// which carries `tenant_slug` on PMS-1499 servers; a client that lands on
/// an older server reads `None` and falls back to a plain navigate.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AcceptResponse {
    #[serde(default)]
    pub(crate) tenant_slug: Option<String>,
}

/// Humanize the role string from the API ("admin", "manager", "finance").
///
/// Pure so the renderer tests can read the same mapping without a Dioxus
/// runtime. Unknown roles pass through unchanged rather than fallback to
/// empty, so a future role the SPA has not yet learned renders as itself.
pub(crate) fn humanize_role(role: &str) -> String {
    match role {
        "super_admin" => "Super admin".to_string(),
        "admin" => "Admin".to_string(),
        "manager" => "Manager".to_string(),
        "technician" => "Technician".to_string(),
        "dispatcher" => "Dispatcher".to_string(),
        "sales" => "Sales".to_string(),
        "finance" => "Finance".to_string(),
        other => other.to_string(),
    }
}

/// MAPPS-1084 AC (a): the by-token accept-grant page.
#[component]
pub fn AcceptGrantPage(token: String) -> Element {
    use_page_title("Accept invitation");

    let navigator = use_navigator();
    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(String::new);
    let mut done_message: Signal<Option<String>> = use_signal(|| None);

    let token_for_resource = token.clone();
    let metadata = use_resource(use_reactive(&token_for_resource, |token| async move {
        #[cfg(feature = "app")]
        {
            let path = format!("/grants/invitations/by-token/{token}");
            crate::hooks::fetch::api::get_authed::<InvitationMetadata>(&path)
                .await
                .inspect_err(|e| tracing::error!("invitation metadata load failed: {e}"))
                .ok()
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = token;
            None::<InvitationMetadata>
        }
    }));

    let snap = metadata.read_unchecked().clone();
    let data: Option<InvitationMetadata> = snap.as_ref().and_then(|o| o.as_ref()).cloned();

    let on_accept = {
        let token = token.clone();
        move |_| {
            if *submitting.read() {
                return;
            }
            submit_error.set(String::new());
            submitting.set(true);
            let token = token.clone();
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/invitations/by-token/{token}/accept");
                    let outcome =
                        crate::hooks::fetch::api::post_authed_typed::<AcceptResponse, ()>(
                            &path,
                            &(),
                        )
                        .await;
                    match outcome {
                        Ok(_) => {
                            // Bump the tenant generation so every active
                            // `use_resource` under the current tenant
                            // refetches; the memberships loader picks the
                            // new grant up on its next read.
                            *crate::hooks::fetch::TENANT_GENERATION.write() += 1;
                            done_message.set(Some(
                                "You accepted the invitation. The team now shows up in your switcher.".to_string(),
                            ));
                            navigator.replace(Route::Dashboard {});
                        }
                        Err(err) => submit_error.set(err.user_message()),
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = token;
                    done_message.set(Some("Accepted.".to_string()));
                }
                submitting.set(false);
            });
        }
    };

    let on_decline = {
        let token = token.clone();
        move |_| {
            if *submitting.read() {
                return;
            }
            submit_error.set(String::new());
            submitting.set(true);
            let token = token.clone();
            spawn(async move {
                #[cfg(feature = "app")]
                {
                    let path = format!("/grants/invitations/by-token/{token}/decline");
                    let outcome = crate::hooks::fetch::api::post_authed_typed::<
                        serde_json::Value,
                        (),
                    >(&path, &())
                    .await;
                    match outcome {
                        Ok(_) => {
                            done_message.set(Some(
                                "You declined the invitation. It stays on record but will not be acted on.".to_string(),
                            ));
                            navigator.replace(Route::Dashboard {});
                        }
                        Err(err) => submit_error.set(err.user_message()),
                    }
                }
                #[cfg(not(feature = "app"))]
                {
                    let _ = token;
                    done_message.set(Some("Declined.".to_string()));
                }
                submitting.set(false);
            });
        }
    };

    rsx! {
        PageHeader {
            title: "Accept invitation",
            subtitle: "A team invited you. Choose whether to accept.",
        }
        if let Some(msg) = done_message().clone() {
            Card { title: "Invitation processed".to_string(), class: "mt-4",
                p { class: "text-sm text-content", "{msg}" }
            }
        } else {
            match data {
                None => rsx! {
                    Card { title: "Loading".to_string(), class: "mt-4",
                        p { class: "text-sm text-muted",
                            "Looking up the invitation…"
                        }
                    }
                },
                Some(meta) => {
                    let status = meta.status.clone();
                    let is_pending = status == "pending";
                    let role = humanize_role(&meta.role);
                    let account = meta.mokosh_account_name.clone();
                    let who = meta
                        .inviter_name
                        .clone()
                        .unwrap_or_else(|| "Someone".to_string());
                    let expires = meta.expires_at.clone();
                    rsx! {
                        if !submit_error().is_empty() {
                            ErrorBanner { class: "mb-4", "{submit_error}" }
                        }
                        Card {
                            title: account.clone(),
                            class: "mt-4",
                            actions: rsx! { Badge { variant: BadgeVariant::Blue, "{role}" } },
                            div { class: "space-y-4",
                                p { class: "text-sm text-content",
                                    "{who} invited you to join "
                                    span { class: "font-medium", "{account}" }
                                    " as "
                                    span { class: "font-medium", "{role}" }
                                    "."
                                }
                                if !expires.is_empty() {
                                    p { class: "text-sm text-muted",
                                        "This invitation expires on {expires}."
                                    }
                                }
                                if !is_pending {
                                    p { class: "text-sm text-muted",
                                        "This invitation is {status} and cannot be acted on."
                                    }
                                } else {
                                    div { class: "flex gap-2 pt-2",
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            loading: submitting(),
                                            onclick: on_accept,
                                            data_testid: "accept-grant",
                                            "Accept"
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            loading: submitting(),
                                            onclick: on_decline,
                                            data_testid: "decline-grant",
                                            "Decline"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MAPPS-1084 / PMS-1162: the picker vocabulary. The six application
    /// roles plus `super_admin` humanise to their title-case display name;
    /// an unknown role passes through unchanged so a future role the SPA
    /// has not yet learned renders as itself rather than a blank.
    #[test]
    fn humanize_role_covers_every_known_role_and_falls_through() {
        assert_eq!(humanize_role("super_admin"), "Super admin");
        assert_eq!(humanize_role("admin"), "Admin");
        assert_eq!(humanize_role("manager"), "Manager");
        assert_eq!(humanize_role("technician"), "Technician");
        assert_eq!(humanize_role("dispatcher"), "Dispatcher");
        assert_eq!(humanize_role("sales"), "Sales");
        assert_eq!(humanize_role("finance"), "Finance");
        // Unknown role: pass-through so a future role renders as itself.
        assert_eq!(humanize_role("new_role_7"), "new_role_7");
    }

    /// The metadata type reads the server's shape (grant_invitations_routes.rs:207).
    /// Serde defaults on every field so a server that predates PMS-1499
    /// (no `inviter_name`) decodes without error and the renderer falls back.
    #[test]
    fn metadata_decodes_the_server_shape_with_inviter_name() {
        let body = serde_json::json!({
            "mokosh_account_name": "Acme Corp",
            "mokosh_account_slug": "acme",
            "inviter_name": "Alice Carter",
            "role": "admin",
            "expires_at": "2026-11-01T00:00:00Z",
            "status": "pending",
        });
        let meta: InvitationMetadata = serde_json::from_value(body).expect("decode");
        assert_eq!(meta.mokosh_account_name, "Acme Corp");
        assert_eq!(meta.mokosh_account_slug, "acme");
        assert_eq!(meta.inviter_name.as_deref(), Some("Alice Carter"));
        assert_eq!(meta.role, "admin");
        assert_eq!(meta.status, "pending");
    }

    /// A pre-PMS-1499 server returns no `inviter_name`; the type decodes
    /// successfully and the renderer's fallback kicks in.
    #[test]
    fn metadata_decodes_a_pre_pms_1499_server_without_inviter_name() {
        let body = serde_json::json!({
            "mokosh_account_name": "Acme Corp",
            "mokosh_account_slug": "acme",
            "role": "admin",
            "expires_at": "2026-11-01T00:00:00Z",
            "status": "pending",
        });
        let meta: InvitationMetadata = serde_json::from_value(body).expect("decode");
        assert_eq!(meta.inviter_name, None);
    }

    /// `AcceptResponse` reads `tenant_slug` when present (PMS-1499) and
    /// `None` when a pre-fix server omits it.
    #[test]
    fn accept_response_decodes_with_or_without_tenant_slug() {
        let with: AcceptResponse = serde_json::from_value(serde_json::json!({
            "tenant_slug": "acme",
        }))
        .expect("decode");
        assert_eq!(with.tenant_slug.as_deref(), Some("acme"));
        let without: AcceptResponse =
            serde_json::from_value(serde_json::json!({})).expect("decode");
        assert_eq!(without.tenant_slug, None);
    }
}

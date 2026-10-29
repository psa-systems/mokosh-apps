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

/// MAPPS-1170: classification of a failed `by-token` metadata fetch. Three
/// branches so the renderer can distinguish "the invitation is gone" from
/// "the signed-in grantee does not match the invitation's email" from "the
/// network/server blew up".
///
/// Pure (no Dioxus runtime, no fetch) so the test module can match on each
/// branch without a browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetadataError {
    /// 404 or 410 from `GET /api/v1/grants/invitations/by-token/{token}`.
    /// The invitation does not exist (never did, or was revoked), so the
    /// renderer shows a "This invitation is not valid" card with no
    /// Accept/Decline affordance.
    Invalid,
    /// 403 from the metadata endpoint. The signed-in grantee's email does
    /// not match the invitation's `invitee_email`, so the renderer shows a
    /// short email-mismatch banner.
    EmailMismatch,
    /// Any other transport failure (network, 5xx, decode). The renderer
    /// shows an inline retry affordance (restart the resource) rather than
    /// a dead-end "invalid" message, because this one is recoverable.
    Transient,
}

/// Map an API fetch result into the three documented UI branches. Pure
/// over `code`: a `Status { code: 404|410, .. }` reads as `Invalid`, a
/// `Status { code: 403, .. }` as `EmailMismatch`, anything else as
/// `Transient`. `Network` and `Decode` always read as `Transient`.
#[cfg(feature = "app")]
pub(crate) fn classify_metadata_error(err: &crate::hooks::fetch::api::ApiError) -> MetadataError {
    match err {
        crate::hooks::fetch::api::ApiError::Status { code, .. } => match *code {
            404 | 410 => MetadataError::Invalid,
            403 => MetadataError::EmailMismatch,
            _ => MetadataError::Transient,
        },
        crate::hooks::fetch::api::ApiError::Network(_)
        | crate::hooks::fetch::api::ApiError::Decode(_) => MetadataError::Transient,
    }
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
    // MAPPS-1170: keep the Err arm so the renderer can tell "invitation
    // gone" (404/410) from "wrong signed-in email" (403) from "please
    // retry" (anything else). The previous `.inspect_err(...).ok()`
    // collapsed every failure into the same `None` the resource shows
    // while a request is still in flight, so a dead link rendered as an
    // infinite skeleton with no feedback.
    let mut metadata = use_resource(use_reactive(&token_for_resource, |token| async move {
        #[cfg(feature = "app")]
        {
            let path = format!("/grants/invitations/by-token/{token}");
            crate::hooks::fetch::api::get_authed_typed::<InvitationMetadata>(&path)
                .await
                .map_err(|e| {
                    let kind = classify_metadata_error(&e);
                    tracing::error!("invitation metadata load failed: {e}");
                    kind
                })
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = token;
            Err::<InvitationMetadata, MetadataError>(MetadataError::Transient)
        }
    }));

    let snap = metadata.read_unchecked().clone();

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
            match snap {
                // Still in flight: skeleton (unchanged from MAPPS-1084).
                None => rsx! {
                    div { class: "mt-4",
                        crate::components::DetailSkeleton { rows: 3 }
                    }
                },
                // MAPPS-1170: the three documented error branches get
                // distinct UI so a dead link stops being mistaken for a
                // still-loading one.
                Some(Err(MetadataError::Invalid)) => rsx! {
                    Card { title: "This invitation is not valid".to_string(), class: "mt-4",
                        p { class: "text-sm text-content",
                            "The invitation link has expired or was revoked. Ask whoever sent it for a fresh one."
                        }
                    }
                },
                Some(Err(MetadataError::EmailMismatch)) => rsx! {
                    Card { title: "This invitation is addressed to someone else".to_string(), class: "mt-4",
                        p { class: "text-sm text-content",
                            "The invitation was sent to a different email than the one you are signed in with. Sign out and sign in with the invited address, or ask the team to resend the invitation."
                        }
                    }
                },
                Some(Err(MetadataError::Transient)) => rsx! {
                    Card { title: "Could not reach the server".to_string(), class: "mt-4",
                        div { class: "space-y-4",
                            p { class: "text-sm text-content",
                                "The invitation could not be loaded. Check your connection and try again."
                            }
                            div { class: "flex gap-2 pt-2",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    data_testid: "retry-accept-grant",
                                    onclick: move |_| metadata.restart(),
                                    "Retry"
                                }
                            }
                        }
                    }
                },
                Some(Ok(meta)) => {
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

    /// MAPPS-1170: the three error branches the page renders distinctly.
    /// `classify_metadata_error` is the pure function the renderer uses to
    /// decide which card goes up; pin every HTTP status + the two non-HTTP
    /// error shapes so a regression that bucketed, say, 500 as `Invalid`
    /// (dead-end) or 404 as `Transient` (retry) fails loud.
    #[cfg(feature = "app")]
    #[test]
    fn classify_metadata_error_maps_each_status_to_the_right_branch() {
        use crate::hooks::fetch::api::ApiError;

        let status = |code: u16| ApiError::Status {
            code,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        };

        // 404 / 410 render the "This invitation is not valid" dead-end
        // card: the token is gone and no amount of retrying brings it
        // back, so the renderer offers no Accept/Decline/Retry button.
        assert_eq!(
            classify_metadata_error(&status(404)),
            MetadataError::Invalid,
            "a 404 means the token is unknown; renders the Invalid card"
        );
        assert_eq!(
            classify_metadata_error(&status(410)),
            MetadataError::Invalid,
            "a 410 means the token was revoked; renders the Invalid card"
        );

        // 403: the signed-in grantee's email does not match the invite's.
        // The renderer tells them to sign in with the invited address.
        assert_eq!(
            classify_metadata_error(&status(403)),
            MetadataError::EmailMismatch,
            "a 403 is the email-mismatch banner, not a dead-end",
        );

        // Everything else routes to the Transient card with a Retry
        // button: network blips, 5xx, decode mismatches, and the handful
        // of other 4xx that are not "gone" or "wrong email".
        assert_eq!(
            classify_metadata_error(&status(500)),
            MetadataError::Transient,
            "a 500 is retryable; renders the Retry card"
        );
        assert_eq!(
            classify_metadata_error(&status(502)),
            MetadataError::Transient,
        );
        assert_eq!(
            classify_metadata_error(&status(400)),
            MetadataError::Transient,
            "an unexpected 4xx (not 403/404/410) falls to Transient so a \
             fresh network attempt is at least offered"
        );
        assert_eq!(
            classify_metadata_error(&ApiError::Network("timeout".to_string())),
            MetadataError::Transient,
        );
        assert_eq!(
            classify_metadata_error(&ApiError::Decode("missing field `role`".to_string())),
            MetadataError::Transient,
        );
    }

    /// MAPPS-1170 regression guard: the renderer must branch on the three
    /// error shapes AND still carry the loading-skeleton branch, so a
    /// future rewrite that forgets one of them is caught at build.
    /// A source-grep is cheaper than wiring the Dioxus renderer into a
    /// unit test; both the `starts_with_SELECT` test on PMS-1517 and the
    /// tenant_switcher wiring tests follow the same shape.
    #[test]
    fn the_page_renders_all_three_error_branches_plus_the_skeleton() {
        let src = include_str!("accept_grant.rs");
        let body = &src[..src.find("mod tests").expect("this module")];
        assert!(
            body.contains("Some(Err(MetadataError::Invalid))"),
            "the renderer must branch on the Invalid error (404/410)"
        );
        assert!(
            body.contains("Some(Err(MetadataError::EmailMismatch))"),
            "the renderer must branch on the EmailMismatch error (403)"
        );
        assert!(
            body.contains("Some(Err(MetadataError::Transient))"),
            "the renderer must branch on the Transient error (5xx, network, decode)"
        );
        assert!(
            body.contains("crate::components::DetailSkeleton"),
            "the loading-skeleton branch must still render on None (not-yet-resolved)"
        );
        assert!(
            body.contains("metadata.restart()"),
            "the Transient branch must wire a Retry button to the resource's restart"
        );
    }
}

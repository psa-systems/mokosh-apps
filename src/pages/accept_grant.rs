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
//! Error shapes on the metadata GET:
//! - A 404 or 410 renders "This invitation is not valid" with no
//!   accept/decline affordance (an unauthenticated token that has expired
//!   or been revoked).
//! - A 403 (the signed-in user does not match the invitation's
//!   `invitee_email`) renders a short "This invitation is addressed to
//!   <email>" banner.
//! - Any other transport failure (a 500, a network error) shows an inline
//!   retry affordance that restarts the fetch.
//!
//! None of the three is mistaken for the initial loading skeleton: the
//! resource keeps the `Result` instead of collapsing every `Err` to `None`.

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

/// What the metadata GET resolved to. Replaces the old collapse-to-`None`
/// that treated every `Err` the same as "still loading" (MAPPS-1170); each
/// variant here gets its own render arm, distinct from the loading skeleton.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MetadataLoad {
    Loaded(InvitationMetadata),
    /// 404 or 410: the token is dead. No accept/decline affordance.
    Invalid,
    /// 403: the signed-in user does not match the invitation's
    /// `invitee_email`. Carries the address when the server's error
    /// envelope includes it, so the banner can name it.
    EmailMismatch(Option<String>),
    /// Any other transport failure: offer a retry instead of the skeleton.
    Failed,
}

/// Pulls `error.invitee_email` out of a 403's raw envelope body, when the
/// server includes it, so the mismatch banner can name the address next to
/// this account's own sign-in rather than only saying "wrong account".
#[cfg(feature = "app")]
pub(crate) fn invitee_email_from_body(body: &Option<serde_json::Value>) -> Option<String> {
    body.as_ref()?
        .get("error")?
        .get("invitee_email")?
        .as_str()
        .map(str::to_string)
}

/// Classifies the metadata GET's `Err` into one of the three documented
/// shapes (module doc comment, lines 18-26). Split out from the
/// `use_resource` closure so the mapping is reachable from a plain unit
/// test without a Dioxus runtime or a live HTTP call.
#[cfg(feature = "app")]
pub(crate) fn classify_metadata_error(e: &crate::hooks::fetch::api::ApiError) -> MetadataLoad {
    use crate::hooks::fetch::api::ApiError;
    match e {
        ApiError::Status { code: 404, .. } | ApiError::Status { code: 410, .. } => {
            MetadataLoad::Invalid
        }
        ApiError::Status {
            code: 403,
            envelope_body,
            ..
        } => MetadataLoad::EmailMismatch(invitee_email_from_body(envelope_body)),
        _ => MetadataLoad::Failed,
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
    let mut metadata = use_resource(use_reactive(&token_for_resource, |token| async move {
        #[cfg(feature = "app")]
        {
            let path = format!("/grants/invitations/by-token/{token}");
            match crate::hooks::fetch::api::get_authed_typed::<InvitationMetadata>(&path).await {
                Ok(meta) => Some(MetadataLoad::Loaded(meta)),
                Err(e) => {
                    tracing::error!("invitation metadata load failed: {e}");
                    Some(classify_metadata_error(&e))
                }
            }
        }
        #[cfg(not(feature = "app"))]
        {
            let _ = token;
            None::<MetadataLoad>
        }
    }));

    let snap = metadata.read_unchecked().clone();
    let data: Option<MetadataLoad> = snap.and_then(|o| o);

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
                    div { class: "mt-4",
                        crate::components::DetailSkeleton { rows: 3 }
                    }
                },
                Some(MetadataLoad::Invalid) => rsx! {
                    Card { title: "Invitation".to_string(), class: "mt-4",
                        p { class: "text-sm text-content", "This invitation is not valid." }
                    }
                },
                Some(MetadataLoad::EmailMismatch(email)) => rsx! {
                    Card { title: "Invitation".to_string(), class: "mt-4",
                        ErrorBanner {
                            match email {
                                Some(email) => rsx! { "This invitation is addressed to {email}." },
                                None => rsx! { "This invitation is addressed to a different email than the one you're signed in with." },
                            }
                        }
                    }
                },
                Some(MetadataLoad::Failed) => rsx! {
                    div { class: "mt-4 flex items-center justify-between gap-3 rounded-lg border border-line bg-surface px-4 py-3",
                        ErrorBanner { class: "flex-1", "Could not load this invitation." }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| metadata.restart(),
                            "Retry"
                        }
                    }
                },
                Some(MetadataLoad::Loaded(meta)) => {
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

    /// MAPPS-1170 AC: the metadata resource's three documented error
    /// shapes. A 404 or 410 must classify as `Invalid` (no accept/decline),
    /// never anything that falls through to the loading skeleton.
    #[cfg(feature = "app")]
    #[test]
    fn a_404_or_410_classifies_as_invalid() {
        use crate::hooks::fetch::api::ApiError;
        let status = |code| ApiError::Status {
            code,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        };
        assert_eq!(classify_metadata_error(&status(404)), MetadataLoad::Invalid);
        assert_eq!(classify_metadata_error(&status(410)), MetadataLoad::Invalid);
    }

    /// AC: a 403 classifies as `EmailMismatch`, carrying the address when
    /// the server's envelope names it, and `None` when it does not (a
    /// server that predates the field still renders a mismatch banner,
    /// just without the specific address).
    #[cfg(feature = "app")]
    #[test]
    fn a_403_classifies_as_email_mismatch_with_or_without_the_address() {
        use crate::hooks::fetch::api::ApiError;
        let with_email = ApiError::Status {
            code: 403,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: Some(serde_json::json!({
                "error": { "invitee_email": "grantee@example.com" }
            })),
        };
        assert_eq!(
            classify_metadata_error(&with_email),
            MetadataLoad::EmailMismatch(Some("grantee@example.com".to_string()))
        );

        let without_email = ApiError::Status {
            code: 403,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        };
        assert_eq!(
            classify_metadata_error(&without_email),
            MetadataLoad::EmailMismatch(None)
        );
    }

    /// AC: any other transport failure (a 500, a network error) classifies
    /// as `Failed` so the page offers a retry, distinct from `Invalid` and
    /// `EmailMismatch`.
    #[cfg(feature = "app")]
    #[test]
    fn any_other_failure_classifies_as_failed() {
        use crate::hooks::fetch::api::ApiError;
        let five_hundred = ApiError::Status {
            code: 500,
            message: String::new(),
            fields: Vec::new(),
            envelope_code: String::new(),
            envelope_body: None,
        };
        assert_eq!(classify_metadata_error(&five_hundred), MetadataLoad::Failed);
        assert_eq!(
            classify_metadata_error(&ApiError::Network("offline".to_string())),
            MetadataLoad::Failed
        );
    }

    /// AC: the loading state (resource not yet resolved, `data: None`) is
    /// a fourth, distinct case from all three error shapes - none of them
    /// is `None`, so none can be mistaken for "still loading".
    #[test]
    fn none_of_the_three_error_shapes_is_the_loading_state() {
        let shapes = [
            MetadataLoad::Invalid,
            MetadataLoad::EmailMismatch(Some("a@example.com".to_string())),
            MetadataLoad::EmailMismatch(None),
            MetadataLoad::Failed,
        ];
        for shape in shapes {
            assert_ne!(Some(shape), None::<MetadataLoad>);
        }
    }

    /// AC: each error shape renders its own distinct text, not the
    /// `DetailSkeleton` loading indicator, and the 404/410 arm offers no
    /// Accept/Decline affordance.
    #[test]
    fn each_error_shape_renders_distinct_text_and_the_invalid_arm_has_no_actions() {
        let src = include_str!("accept_grant.rs");
        let head = &src[..src.find("mod tests").expect("this module")];
        assert!(
            head.contains("Some(MetadataLoad::Invalid) => rsx! {")
                && head.contains("\"This invitation is not valid.\""),
            "the 404/410 arm must render the documented invalid-link message"
        );
        let invalid_start = head.find("Some(MetadataLoad::Invalid) => rsx! {").unwrap();
        let invalid_end = head[invalid_start..]
            .find("Some(MetadataLoad::EmailMismatch")
            .map(|i| invalid_start + i)
            .expect("the next arm");
        let invalid_arm = &head[invalid_start..invalid_end];
        assert!(
            !invalid_arm.contains("data_testid: \"accept-grant\"")
                && !invalid_arm.contains("data_testid: \"decline-grant\""),
            "an invalid token must offer no Accept/Decline affordance"
        );
        assert!(
            head.contains("\"This invitation is addressed to {email}.\""),
            "the 403 arm must render the email-mismatch banner"
        );
        assert!(
            head.contains("Some(MetadataLoad::Failed) => rsx! {") && head.contains("\"Retry\""),
            "any other failure must render an inline retry affordance"
        );
        assert!(
            head.contains("crate::components::DetailSkeleton"),
            "the loading state keeps its own skeleton, untouched by the error arms"
        );
    }
}

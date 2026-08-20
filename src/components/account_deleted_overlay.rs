//! MAPPS-348: full-screen terminal overlay shown when the mokosh SPA
//! discovers that the current user's Bunyip account has been deleted.
//!
//! Trigger: the shared fetch layer (`hooks::fetch::note_account_deleted`)
//! flips the `ACCOUNT_DELETED` `GlobalSignal` when any request comes back
//! as `410 Gone` with `error.code == "ACCOUNT_DELETED"`. The server-side
//! contract lives on the mokosh-server MAPPS-348 branch: every auth
//! extractor (`RequireAuth`, `TenantScope`, `RequireRole`, `RequireModule
//! Enabled`) now returns `AccountDeleted` (410) instead of the generic
//! `Unauthorized` (401) once `users.deleted_at` is stamped.
//!
//! Behaviour once the signal flips:
//! - The overlay renders on top of every route (`AppLayout` mounts it
//!   below the top bar). It is non-dismissible: the deletion is one-way,
//!   so there is no state to recover to.
//! - `use_effect` on mount clears the local OIDC token holder (via
//!   `oidc::storage::clear_auth`) so the SPA cannot re-authenticate with
//!   the tombstoned bearer. Belt-and-braces even if the redirect below
//!   fails or is intercepted by a network hiccup.
//! - A 5s countdown, then `window.location.replace(hub_logout)` sends
//!   the browser to bunyip's `/v1/auth/logout?url=<msp origin root>`.
//!   That URL is the same pattern the existing `UserMenu` uses for a
//!   user-initiated logout - bunyip clears the shared `.a8n.systems`
//!   cookies via Set-Cookie, then 302s straight back to the SPA origin
//!   root, signed out. See `layout.rs::UserMenu::hub_logout` for the
//!   original.
//! - A "Sign out now" button jumps to the same target immediately, for
//!   users who don't want to wait out the countdown.

use dioxus::prelude::*;

#[cfg(feature = "web")]
const COUNTDOWN_SECS: u32 = 5;

/// Read the sticky "account was deleted" flag. Reactive: the wrapping
/// AppLayout re-renders when the flag transitions from false to true.
#[cfg(feature = "web")]
pub fn use_account_deleted() -> bool {
    *crate::hooks::fetch::ACCOUNT_DELETED.read()
}

/// Non-web stub so components that consume the flag still compile under
/// `cargo check` without the `web` feature.
#[cfg(not(feature = "web"))]
pub fn use_account_deleted() -> bool {
    false
}

/// Build the hub logout URL, mirroring `UserMenu::hub_logout` in
/// `layout.rs`. Extracted here so the overlay does not depend on the
/// menu component (they are peers under `AppLayout`).
#[cfg(feature = "web")]
fn build_hub_logout_url() -> String {
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    let issuer = cfg.issuer.trim_end_matches('/');
    let post_logout_target = crate::platform::location::origin()
        .map(|origin| format!("{}/", origin.trim_end_matches('/')))
        .unwrap_or_else(|| cfg.hub_url("/"));
    format!(
        "{issuer}/v1/auth/logout?url={}",
        crate::utils::url::encode_uri_component(&post_logout_target)
    )
}

/// Renders the terminal "your account has been deleted" overlay when
/// the `ACCOUNT_DELETED` signal has flipped. Otherwise renders nothing,
/// so the overlay adds zero DOM to a healthy session.
#[component]
pub fn AccountDeletedOverlay() -> Element {
    #[cfg(feature = "web")]
    {
        if !use_account_deleted() {
            return rsx! {};
        }
        // MAPPS-377: mount the terminal body as a child so its countdown and
        // token-clearing hooks run unconditionally within it. Hoisting them
        // above the `!is_deleted` return would instead fire the logout redirect
        // and clear the OIDC tokens on every healthy render.
        return rsx! { AccountDeletedTerminal {} };
    }
    #[cfg(not(feature = "web"))]
    {
        rsx! {}
    }
}

/// The terminal "your account has been deleted" body, mounted only once the
/// `ACCOUNT_DELETED` signal has flipped (MAPPS-377). Split out of
/// `AccountDeletedOverlay` so its countdown / redirect hooks run
/// unconditionally within it rather than after the parent's early return.
#[cfg(feature = "web")]
#[component]
fn AccountDeletedTerminal() -> Element {
    // Countdown state: decrements once a second while the overlay is
    // mounted. When it hits 0, `use_effect` triggers the redirect.
    let mut remaining = use_signal(|| COUNTDOWN_SECS);

    // Clear local OIDC tokens the moment the overlay mounts so a
    // subsequent request cannot re-arm the tombstoned bearer. Runs
    // exactly once (per mount) because it reads no signal.
    use_effect(move || {
        crate::modules::oidc::storage::clear_auth();
    });

    // Countdown driver: schedule the next tick via a spawned task
    // with a 1-second timeout. When the remaining seconds reach 0,
    // hard-navigate to bunyip's logout endpoint.
    use_effect(move || {
        let secs = *remaining.read();
        if secs == 0 {
            crate::platform::location::replace(&build_hub_logout_url());
            return;
        }
        spawn(async move {
            crate::platform::timer::sleep_ms(1000).await;
            let now = *remaining.peek();
            if now > 0 {
                remaining.set(now - 1);
            }
        });
    });

    let sign_out_now = move |_| {
        crate::platform::location::replace(&build_hub_logout_url());
    };

    let secs = *remaining.read();
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-app/95 backdrop-blur",
            role: "alertdialog",
            aria_modal: "true",
            aria_labelledby: "account-deleted-title",
            div { class: "max-w-md w-full mx-4 rounded-lg bg-raised shadow-xl ring-1 ring-line p-6 text-center space-y-4",
                h2 {
                    id: "account-deleted-title",
                    class: "text-lg font-semibold text-content",
                    "Your account has been deleted."
                }
                p { class: "text-sm text-muted",
                    "Signing you out in "
                    span { class: "font-semibold text-content", "{secs}" }
                    " seconds…"
                }
                button {
                    r#type: "button",
                    class: "inline-flex items-center justify-center px-4 py-2 rounded-md bg-accent text-white text-sm font-medium hover:opacity-90 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2",
                    onclick: sign_out_now,
                    "Sign out now"
                }
            }
        }
    }
}

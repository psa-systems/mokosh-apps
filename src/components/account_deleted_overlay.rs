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
//!   fails or is intercepted by a network hiccup. It clears
//!   `sessionStorage` only; the in-memory bearer survives, so the revoke
//!   below still has a credential to present.
//! - A 5s countdown, then `modules::auth::sign_out::sign_out()`, the
//!   shared sequence the `UserMenu` entries run (MAPPS-522): revoke the
//!   mokosh session, revoke the OP refresh-token family, clear local
//!   storage, then redirect off this origin, signed out.
//! - A "Sign out now" button runs the same sequence immediately, for
//!   users who don't want to wait out the countdown.

use dioxus::prelude::*;

#[cfg(feature = "app")]
const COUNTDOWN_SECS: u32 = 5;

/// Read the sticky "account was deleted" flag. Reactive: the wrapping
/// AppLayout re-renders when the flag transitions from false to true.
#[cfg(feature = "app")]
pub fn use_account_deleted() -> bool {
    *crate::hooks::fetch::ACCOUNT_DELETED.read()
}

/// Non-web stub so components that consume the flag still compile under
/// `cargo check` without the `app` feature.
#[cfg(not(feature = "app"))]
pub fn use_account_deleted() -> bool {
    false
}

/// Renders the terminal "your account has been deleted" overlay when
/// the `ACCOUNT_DELETED` signal has flipped. Otherwise renders nothing,
/// so the overlay adds zero DOM to a healthy session.
#[component]
pub fn AccountDeletedOverlay() -> Element {
    #[cfg(feature = "app")]
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
    #[cfg(not(feature = "app"))]
    {
        rsx! {}
    }
}

/// The terminal "your account has been deleted" body, mounted only once the
/// `ACCOUNT_DELETED` signal has flipped (MAPPS-377). Split out of
/// `AccountDeletedOverlay` so its countdown / redirect hooks run
/// unconditionally within it rather than after the parent's early return.
#[cfg(feature = "app")]
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
    // run the shared sign-out sequence.
    use_effect(move || {
        let secs = *remaining.read();
        if secs == 0 {
            spawn(async move {
                crate::modules::auth::sign_out::sign_out().await;
            });
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
        spawn(async move {
            crate::modules::auth::sign_out::sign_out().await;
        });
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

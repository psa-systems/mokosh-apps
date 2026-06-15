//! Update-available banner shown to admins when the server reports
//! that a newer image is available in the registry.
//!
//! The version skew is read from mokosh-server's public
//! `GET /api/v1/version` endpoint; on any fetch error the banner
//! renders nothing. Non-admin users never see it (the check is in
//! this component, not the layout, so adding it does not change
//! non-admin renders).
//!
//! Dismissal is keyed on the latest version string so a *new* update
//! resurfaces after the user clicked dismiss on the previous one.

use dioxus::prelude::*;

use crate::components::{IconSize, XMarkIcon};
use crate::hooks::use_auth;
use crate::modules::system::{get_version, SystemVersion};

const DISMISS_KEY_PREFIX: &str = "mokosh-update-dismissed-";

/// True when localStorage holds a dismissal record for this exact
/// pair of latest versions. Failing closed (returning false) on any
/// access error means the banner still shows; an admin who can't
/// dismiss is better than an admin who never sees the notification.
fn is_dismissed(key: &str) -> bool {
    let Some(win) = web_sys::window() else {
        return false;
    };
    let Ok(Some(storage)) = win.local_storage() else {
        return false;
    };
    matches!(storage.get_item(key), Ok(Some(_)))
}

fn mark_dismissed(key: &str) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = win.local_storage() else {
        return;
    };
    let _ = storage.set_item(key, "1");
}

/// Build the dismissal key from both the server and client latest
/// versions. Either bump re-shows the banner because the key changes.
fn dismissal_key(v: &SystemVersion) -> String {
    let s = v.server.latest.as_deref().unwrap_or("-");
    let c = v.client.latest.as_deref().unwrap_or("-");
    format!("{DISMISS_KEY_PREFIX}s{s}-c{c}")
}

#[component]
pub fn UpdateBanner() -> Element {
    // Hooks first, gating render afterwards. Dioxus requires the set
    // of hooks called per render to stay stable across that component
    // instance's lifetime; bailing on `!is_admin` before
    // `use_resource` / `use_signal` would skip them on the non-admin
    // render and then add them on the next render if the user becomes
    // admin (tenant switch, late hydration), violating that invariant.
    //
    // The resource's outer closure reads `auth` so the reactivity
    // tracker re-runs the fetch when auth flips, AND skips the
    // network call entirely while the user is not an admin - that
    // keeps the hook order stable without burning a
    // `GET /api/v1/version` round-trip on every non-admin
    // layout mount.
    let auth = use_auth();
    let mut dismissed_local = use_signal(|| false);
    let version_resource = use_resource(move || {
        let admin = auth.read().user.as_ref().is_some_and(|u| u.role.is_admin());
        async move {
            if !admin {
                return Err("not admin".to_string());
            }
            get_version().await
        }
    });

    let is_admin = auth.read().user.as_ref().is_some_and(|u| u.role.is_admin());
    if !is_admin {
        return rsx! {};
    }

    // PMS-313: zero layout shift. The async version check used to make
    // this component render nothing until a result arrived, then pop a
    // ~40px banner in and shove the TopBar / sidebar / content down
    // (non-zero CLS on every admin login on a stale client).
    //
    // Fix: for admins the outer container ALWAYS renders, so the
    // banner's height is reserved from first paint. While the check is
    // in flight we reserve that height invisibly (`Reserving`); once it
    // resolves we either fade the banner into the already-reserved space
    // (`Show`, zero shift) or collapse the reserved height away
    // (`Collapsed`) with a deliberate 200ms transition. The collapse
    // uses the `grid-template-rows: 1fr -> 0fr` pattern so it animates to
    // the content's natural height with no hard-coded pixel value and no
    // clipping of a wrapped multi-line banner on narrow viewports.
    //
    // `read()` (not `read_unchecked()`) so the component re-renders when
    // the resource transitions Loading -> Ready.
    enum BannerState {
        /// Version check in flight: reserve height, paint nothing.
        Reserving,
        /// Update available and not dismissed: show the banner.
        Show(SystemVersion),
        /// Resolved with no update, fetch error, or dismissed: collapse.
        Collapsed,
    }

    let state = match &*version_resource.read() {
        None => BannerState::Reserving,
        Some(Ok(v)) => {
            let has_update = v.server.update_available() || v.client.update_available();
            let dismissed = *dismissed_local.read() || is_dismissed(&dismissal_key(v));
            if has_update && !dismissed {
                BannerState::Show(v.clone())
            } else {
                BannerState::Collapsed
            }
        }
        Some(Err(_)) => BannerState::Collapsed,
    };

    // `1fr` reserves/keeps the row's natural height; `0fr` collapses it.
    let rows = match &state {
        BannerState::Collapsed => "grid-rows-[0fr]",
        _ => "grid-rows-[1fr]",
    };
    // Only the shown banner is visible; the reserved placeholder and the
    // collapsing row stay transparent (AC: invisible on first paint).
    let opacity = match &state {
        BannerState::Show(_) => "opacity-100",
        _ => "opacity-0",
    };

    rsx! {
        div {
            class: "grid overflow-hidden transition-all duration-200 ease-in-out {rows} {opacity}",
            // Inner wrapper is the grid row whose height the transition
            // animates; `overflow-hidden` clips its content as it collapses.
            div { class: "overflow-hidden",
                {match state {
                    // `VersionPair::update_available()` only returns true
                    // when `latest` is `Some`, so the unwraps here are
                    // unreachable; they avoid carrying owned `String`s
                    // into the rsx! closures just to please the borrow
                    // checker.
                    BannerState::Show(v) => {
                        let server_update = v.server.update_available();
                        let client_update = v.client.update_available();
                        let client_running = v.client.running.clone();
                        let client_latest =
                            v.client.latest.clone().unwrap_or_else(|| "unknown".to_string());
                        let server_running = v.server.running.clone();
                        let server_latest =
                            v.server.latest.clone().unwrap_or_else(|| "unknown".to_string());
                        let key = dismissal_key(&v);
                        rsx! {
                            div {
                                class: "border-b border-amber-200 dark:border-amber-900 bg-amber-50 dark:bg-amber-950/30 text-amber-900 dark:text-amber-200",
                                div {
                                    class: "max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-2 flex items-center justify-between gap-4 text-sm",
                                    div { class: "flex-1",
                                        span { class: "font-medium", "Update available." }
                                        " "
                                        if client_update {
                                            span { "Client: {client_running} → {client_latest}. " }
                                        }
                                        if server_update {
                                            span { "Server: {server_running} → {server_latest}. " }
                                        }
                                        span { class: "text-amber-700 dark:text-amber-300",
                                            "Bump the tag(s) in your compose.yml and run "
                                            code { class: "px-1 py-0.5 bg-amber-100 dark:bg-amber-900/50 rounded text-xs",
                                                "docker compose pull && docker compose up --detach"
                                            }
                                            " on the host to apply."
                                        }
                                    }
                                    button {
                                        class: "shrink-0 p-1 rounded hover:bg-amber-100 dark:hover:bg-amber-900/40",
                                        title: "Dismiss until next update",
                                        aria_label: "Dismiss update notification",
                                        onclick: {
                                            let key = key.clone();
                                            move |_| {
                                                mark_dismissed(&key);
                                                dismissed_local.set(true);
                                            }
                                        },
                                        XMarkIcon { size: IconSize::Small, class: "text-amber-700 dark:text-amber-300".to_string() }
                                    }
                                }
                            }
                        }
                    }
                    // One banner-line tall, transparent: reserves the
                    // height so the real banner fades in without shifting
                    // anything below it.
                    BannerState::Reserving => rsx! {
                        div { class: "border-b border-transparent py-2 text-sm", "\u{00a0}" }
                    },
                    BannerState::Collapsed => rsx! {},
                }}
            }
        }
    }
}

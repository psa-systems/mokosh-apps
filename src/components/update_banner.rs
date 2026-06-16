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
use crate::hooks::{use_auth, use_version_cache};
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
    // The resource keys on a memoized admin flag (not the raw `auth`
    // context): it re-runs the fetch when admin-ness flips and skips the
    // network call entirely while the user is not an admin, keeping the
    // hook order stable without burning a `GET /api/v1/version` on every
    // non-admin mount. MAPPS-187: reading `auth` directly re-fired this
    // resource because the context is written more than once during startup
    // (OIDC callback, then rehydration) with the same user; the memo only
    // notifies when the bool actually changes, so /version is fetched once.
    let auth = use_auth();
    let is_admin = use_memo(move || auth.read().user.as_ref().is_some_and(|u| u.role.is_admin()));
    let mut dismissed_local = use_signal(|| false);
    // MAPPS-203: read the cached App-root version signal first. If it
    // already carries a result, the banner skips its `Reserving` state
    // entirely on this mount - no reserve-then-collapse animation, no
    // page jump. Only the very first admin mount of the App's lifetime
    // sees `None` here and falls through to the local `use_resource`.
    let mut cached_version = use_version_cache();
    // Keep the local fetch so the banner refreshes on each admin
    // navigation; the cache is updated when it resolves so subsequent
    // mounts benefit. The non-admin early-out below means we still
    // skip the network call entirely for non-admins (MAPPS-187).
    let version_resource = use_resource(move || {
        let admin = is_admin();
        async move {
            if !admin {
                return Err("not admin".to_string());
            }
            get_version().await
        }
    });
    // Each time the resource resolves, write through to the App-root
    // cache so the next AppLayout re-mount (any nav click) starts at a
    // resolved state instead of `Reserving`.
    use_effect(move || {
        if let Some(result) = version_resource.read().as_ref() {
            let snapshot: Result<SystemVersion, String> = match result {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(e.clone()),
            };
            cached_version.set(Some(snapshot));
        }
    });

    if !is_admin() {
        return rsx! {};
    }

    // PMS-313 + MAPPS-203: zero layout shift across both first paint
    // AND every subsequent navigation.
    //
    // PMS-313 fixed the first-paint case: the async version check used
    // to make this component render nothing until a result arrived,
    // then pop a ~40px banner in and shove the TopBar / sidebar /
    // content down on every admin login.
    //
    // MAPPS-203 fixes the subsequent-nav case: because every page
    // re-mounts AppLayout (and therefore this component), the local
    // `use_resource` started at `None` every time, dropping us into
    // `Reserving` for one tick before transitioning to `Collapsed`,
    // which animated the row from 1fr to 0fr over 200ms - visibly
    // shifting the layout below on every nav click. The App-root
    // `cached_version` signal short-circuits that: on any mount where
    // the App has already seen a result, we derive the final state
    // (`Show` or `Collapsed`) directly with no intermediate
    // `Reserving` row, so the transition never runs.
    //
    // Resolution order: cached -> live resource -> Reserving.
    // `read()` (not `read_unchecked()`) so the component re-renders when
    // either signal transitions.
    enum BannerState {
        /// Version check in flight: reserve height, paint nothing.
        Reserving,
        /// Update available and not dismissed: show the banner.
        Show(SystemVersion),
        /// Resolved with no update, fetch error, or dismissed: collapse.
        Collapsed,
    }

    let derive_state = |v: &SystemVersion| -> BannerState {
        let has_update = v.server.update_available() || v.client.update_available();
        let dismissed = *dismissed_local.read() || is_dismissed(&dismissal_key(v));
        if has_update && !dismissed {
            BannerState::Show(v.clone())
        } else {
            BannerState::Collapsed
        }
    };

    let state = match (
        cached_version.read().as_ref(),
        version_resource.read().as_ref(),
    ) {
        (Some(Ok(v)), _) => derive_state(v),
        (Some(Err(_)), _) => BannerState::Collapsed,
        (None, Some(Ok(v))) => derive_state(v),
        (None, Some(Err(_))) => BannerState::Collapsed,
        (None, None) => BannerState::Reserving,
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

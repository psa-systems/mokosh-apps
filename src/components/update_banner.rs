//! Update-available banner shown to admins when the server reports
//! that a newer image is available in the registry.
//!
//! Progressive enablement: while mokosh-server's
//! `GET /api/v1/system/version` endpoint is not deployed, the fetch
//! errors and the banner renders nothing. Non-admin users never see
//! it (the check is in this component, not the layout, so adding
//! it does not change non-admin renders).
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
    // `GET /api/v1/system/version` round-trip on every non-admin
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

    // `read()` rather than `read_unchecked()` so this component
    // re-renders when the resource resolves. The unchecked variant
    // would silently miss the transition from Loading -> Ready.
    let v: SystemVersion = match &*version_resource.read() {
        Some(Ok(v)) => v.clone(),
        _ => return rsx! {},
    };

    let server_update = v.server.update_available();
    let client_update = v.client.update_available();
    if !server_update && !client_update {
        return rsx! {};
    }

    let key = dismissal_key(&v);
    if *dismissed_local.read() || is_dismissed(&key) {
        return rsx! {};
    }

    // `VersionPair::update_available()` only returns true when
    // `latest` is `Some`, so the unwraps here are unreachable. They
    // also avoid carrying owned `String`s into the rsx! closures
    // just to please the lifetime checker.
    let client_running = v.client.running.clone();
    let client_latest = v
        .client
        .latest
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let server_running = v.server.running.clone();
    let server_latest = v
        .server
        .latest
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

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

//! Auto-reload when a new SPA build is deployed.
//!
//! The Mokosh SPA is a static WASM bundle served by Caddy. Once an open
//! tab has loaded the bundle, the user is pinned to that build forever
//! unless they hard-reload (Ctrl+Shift+R). When a fresh deploy lands,
//! they keep running the old code, miss bug fixes, and silently 401
//! against a renamed endpoint. This hook is the auto-update path.
//!
//! ## How it works
//!
//! 1. **Boot snapshot.** At mount, we capture the build hash baked
//!    into THIS bundle (`crate::utils::version::GIT_HASH`, baked by
//!    `build.rs` from the `APP_GIT_HASH` env). That is the build the
//!    user is currently running.
//!
//! 2. **Runtime probe.** `_mokosh_config.js` is generated per-container
//!    by `oci-build/entrypoint.sh` and now carries a `build_sha` field
//!    (the `GIT_SHA` env baked into the image at build time). The
//!    Caddyfile already serves it with `Cache-Control: no-cache`, so a
//!    fresh fetch always returns the deployed value.
//!
//! 3. **Detection loop.** Every `POLL_INTERVAL_SECS` (and immediately
//!    on `visibilitychange` to `visible`, so users coming back to a
//!    backgrounded tab probe right away), we re-evaluate
//!    `runtime_config::get("build_sha")`. The browser fetches the
//!    no-cache `_mokosh_config.js` on the natural reload cycle; for an
//!    instantaneous check we explicitly re-fetch via fetch API and
//!    re-evaluate the script.
//!
//! 4. **Reload at a safe boundary.** When the live build differs from
//!    the bundle's own hash, we trigger `location.reload()` on the
//!    NEXT `visibilitychange` to `hidden` (the user is switching away
//!    from the tab; they will not notice the reload). If the tab stays
//!    foregrounded for too long, we fall back to an immediate reload
//!    after `MAX_DEFERRED_SECS`, otherwise users on a single-tab setup
//!    would never auto-update.
//!
//! Skips the whole machinery when the compile-time `GIT_HASH` is empty
//! or `"unknown"` (dev builds), so `cargo run --features web` does not
//! reload itself out from under a developer iterating on the SPA.

use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

/// How long to wait between background polls.
const POLL_INTERVAL_SECS: u64 = 5 * 60;

/// Hard cap on how long we defer a reload after detecting a new build.
/// The intended path is "user switches tabs, we reload while hidden, no
/// disruption." This cap covers the always-on dashboard case so users
/// who never leave the tab still eventually pick up the new build.
const MAX_DEFERRED_SECS: u64 = 30 * 60;

const BUILD_SHA_FIELD: &str = "build_sha";
const CONFIG_JS_PATH: &str = "/_mokosh_config.js";

/// Compile-time build hash. When empty or `"unknown"` we treat the
/// build as a dev build and disable the auto-reload behaviour.
#[cfg(feature = "web")]
fn baseline_sha() -> Option<String> {
    let sha = crate::utils::version::GIT_HASH.trim();
    if sha.is_empty() || sha == "unknown" {
        None
    } else {
        Some(sha.to_string())
    }
}

/// Re-fetch `_mokosh_config.js` and re-evaluate it so
/// `window.__MOKOSH_CONFIG__` reflects the live deploy. Returns the
/// freshly-read `build_sha` field. The Caddyfile serves the file with
/// `Cache-Control: no-cache`, so the network request actually round-
/// trips; the SPA's compiled-in fetch never returns a stale value.
///
/// Why re-eval instead of just fetching JSON? Operators may also use
/// this file as a JS shim that mutates other globals; the SPA does not
/// own that contract. Sticking to the existing format (a single
/// `window.__MOKOSH_CONFIG__ = { ... }` assignment) means we do not
/// add a second source of truth for the build hash.
#[cfg(feature = "web")]
async fn fetch_live_build_sha() -> Option<String> {
    use gloo_net::http::Request;
    use wasm_bindgen::JsValue;

    let resp = Request::get(CONFIG_JS_PATH).send().await.ok()?;
    if !resp.ok() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let win = web_sys::window()?;
    // `js_sys::eval` is invoked in the SPA's own origin against a
    // resource the SPA itself controls. The body is fetched no-cache
    // from the same host that served the SPA bundle, so we are not
    // crossing a trust boundary. Failure modes (parse error, CSP
    // block) surface as `None` and the SPA just stays on the current
    // build.
    let _ = js_sys::eval(&body).ok()?;
    let cfg = js_sys::Reflect::get(&win, &JsValue::from_str("__MOKOSH_CONFIG__")).ok()?;
    let val = js_sys::Reflect::get(&cfg, &JsValue::from_str(BUILD_SHA_FIELD)).ok()?;
    val.as_string().filter(|s| !s.is_empty())
}

#[cfg(not(feature = "web"))]
async fn fetch_live_build_sha() -> Option<String> {
    None
}

#[cfg(feature = "web")]
fn reload_now() {
    if let Some(win) = web_sys::window() {
        // `location.reload()` issues a normal reload (re-validates
        // index.html against the no-cache directive, picks up the new
        // bundle filenames). No need for the deprecated reload(true)
        // hard-reload argument.
        let _ = win.location().reload();
    }
}

#[cfg(not(feature = "web"))]
fn reload_now() {}

/// Root-level update-check hook. Mount once at `App`. No-op when the
/// build hash is unknown (dev builds) or `web_sys` is unavailable.
#[cfg(feature = "web")]
pub fn use_update_check() {
    let Some(my_sha) = baseline_sha() else {
        return;
    };
    let mut pending_reload = use_signal(|| false);
    let mut detected_at_secs: Signal<Option<f64>> = use_signal(|| None);

    // Background polling loop.
    use_future(move || {
        let baseline = my_sha.clone();
        async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_SECS as u32 * 1000).await;
                if pending_reload() {
                    // Already flagged; the visibilitychange listener
                    // will fire the reload. Keep polling silently in
                    // case the deploy gets rolled back, but never
                    // un-flag (a confirmed-update signal stays
                    // confirmed).
                    continue;
                }
                if let Some(live) = fetch_live_build_sha().await {
                    if live != baseline {
                        tracing::info!(
                            baseline = %baseline,
                            live = %live,
                            "auto-update: new SPA build detected, scheduling reload at next safe boundary"
                        );
                        pending_reload.set(true);
                        detected_at_secs.set(Some(performance_now_secs().unwrap_or(0.0)));
                    }
                }
            }
        }
    });

    // visibilitychange handler: reload when the tab goes hidden after
    // a pending update, OR when it comes back visible and we have not
    // yet caught up (covers the "tab was backgrounded across a deploy"
    // case so users probe immediately on return).
    use_effect(move || {
        let Some(win) = web_sys::window() else {
            return;
        };
        let Some(doc) = win.document() else {
            return;
        };
        let cb = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let hidden = doc.hidden();
            if pending_reload() {
                if hidden {
                    // User switched away. Reload while they are not
                    // looking; when they come back the new bundle is
                    // already loaded.
                    reload_now();
                } else if let Some(detected_at) = detected_at_secs() {
                    // Foregrounded with a pending reload. Stay polite
                    // unless we have been holding the reload for too
                    // long; then bite the bullet and reload anyway so
                    // users who never leave the tab still update.
                    let now = performance_now_secs().unwrap_or(0.0);
                    if now - detected_at >= MAX_DEFERRED_SECS as f64 {
                        reload_now();
                    }
                }
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ =
            doc.add_event_listener_with_callback("visibilitychange", cb.as_ref().unchecked_ref());
        // Lives for the lifetime of the app; nothing else removes it.
        cb.forget();
    });
}

#[cfg(not(feature = "web"))]
pub fn use_update_check() {}

#[cfg(feature = "web")]
fn performance_now_secs() -> Option<f64> {
    let perf = web_sys::window()?.performance()?;
    Some(perf.now() / 1000.0)
}

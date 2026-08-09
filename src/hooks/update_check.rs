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
//! 5. **Tell the user (MAPPS-428).** Detection also flips the
//!    [`UPDATE_PENDING`] global, which drives the app-wide
//!    [`crate::components::UpdateAvailableBanner`]. Until MAPPS-428 the
//!    detection was completely silent, so a stale tab's only feedback
//!    was per-page "Could not load X. Refresh the page to retry."
//!    errors that blamed the form for an app-wide condition.
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

/// MAPPS-428: app-wide "the bundle this tab is running is out of date"
/// flag. Set once a `build_sha` mismatch is confirmed, never cleared (a
/// confirmed-update signal stays confirmed; only a reload resolves it).
///
/// A `GlobalSignal` for the same reason
/// [`crate::hooks::fetch::SERVER_REACHABLE`] is one: it is written from
/// plain async fns that cannot reach a context-provided signal, and read
/// by [`crate::components::UpdateAvailableBanner`] mounted in `AppShell`,
/// so no props are threaded through the layout.
#[cfg(feature = "web")]
pub static UPDATE_PENDING: GlobalSignal<bool> = Signal::global(|| false);

/// MAPPS-428: "the fetch layer just saw a failure that looks like a
/// version skew; probe `_mokosh_config.js` now". Set by
/// [`note_possible_version_skew`], consumed by the probe resource in
/// [`use_update_check`], which clears it when the probe finishes.
///
/// Being a flag rather than a counter is what debounces the probe: while
/// one check is in flight the flag is already `true`, so a burst of
/// failing requests writes nothing and fans out into exactly one
/// `_mokosh_config.js` fetch.
#[cfg(feature = "web")]
static SKEW_PROBE_REQUESTED: GlobalSignal<bool> = Signal::global(|| false);

// When the current `UPDATE_PENDING` was first observed, as a
// `performance.now()` second count. Drives the `MAX_DEFERRED_SECS` cap.
// A `thread_local` rather than a signal because nothing re-renders on
// it; WASM is single-threaded so a `Cell` is sufficient.
#[cfg(feature = "web")]
thread_local! {
    static DETECTED_AT_SECS: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

/// Whether an HTTP status is consistent with this tab running a bundle
/// older than the deployed one: a `404` (the new build renamed or
/// removed the endpoint the old bundle calls) or a `5xx` (the server
/// rejecting a request shaped by the old bundle).
///
/// Transport failures are deliberately NOT in this set: they mean the
/// server is unreachable, which is [`crate::hooks::fetch::SERVER_REACHABLE`]'s
/// job and already has its own banner. Probing a host we cannot reach
/// would only fail again.
pub(crate) fn is_version_skew_status(status: u16) -> bool {
    status == 404 || (500..600).contains(&status)
}

/// Whether a build hash marks a dev build: empty or `"unknown"`. Such a
/// build has nothing meaningful to compare against, so the whole
/// machinery (auto-reload AND the MAPPS-428 banner) stays off.
fn is_dev_sha(sha: &str) -> bool {
    let sha = sha.trim();
    sha.is_empty() || sha == "unknown"
}

/// Compile-time build hash. When empty or `"unknown"` we treat the
/// build as a dev build and disable the auto-reload behaviour.
#[cfg(feature = "web")]
fn baseline_sha() -> Option<String> {
    let sha = crate::utils::version::GIT_HASH.trim();
    if is_dev_sha(sha) {
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

/// Reload the tab now. `pub(crate)` since MAPPS-428 so the
/// [`crate::components::UpdateAvailableBanner`] "Reload page" button
/// performs exactly the same reload the automatic path does.
#[cfg(feature = "web")]
pub(crate) fn reload_now() {
    if let Some(win) = web_sys::window() {
        // `location.reload()` issues a normal reload (re-validates
        // index.html against the no-cache directive, picks up the new
        // bundle filenames). No need for the deprecated reload(true)
        // hard-reload argument.
        let _ = win.location().reload();
    }
}

#[cfg(not(feature = "web"))]
pub(crate) fn reload_now() {}

/// Record a confirmed `build_sha` mismatch. Idempotent: the flag is
/// one-way, so a second detection (poll and skew probe racing) neither
/// re-logs nor moves the deferred-reload deadline.
#[cfg(feature = "web")]
fn note_new_build_detected(baseline: &str, live: &str) {
    if *UPDATE_PENDING.peek() {
        return;
    }
    tracing::info!(
        baseline = %baseline,
        live = %live,
        "auto-update: new SPA build detected, scheduling reload at next safe boundary"
    );
    DETECTED_AT_SECS.with(|cell| cell.set(Some(performance_now_secs().unwrap_or(0.0))));
    *UPDATE_PENDING.write() = true;
}

/// Whether a pending reload has been deferred past `MAX_DEFERRED_SECS`.
#[cfg(feature = "web")]
fn deferred_cap_elapsed() -> bool {
    let Some(detected_at) = DETECTED_AT_SECS.with(|cell| cell.get()) else {
        return false;
    };
    performance_now_secs().unwrap_or(0.0) - detected_at >= MAX_DEFERRED_SECS as f64
}

/// MAPPS-428: called by the fetch layer when a request fails in a way
/// consistent with a version skew. Asks the probe resource in
/// [`use_update_check`] to check `build_sha` now, so the banner appears
/// in the same interaction that produced the error the user is looking
/// at instead of up to `POLL_INTERVAL_SECS` later.
///
/// No-op on a dev build, once the update is already known, and while a
/// probe is in flight.
#[cfg(feature = "web")]
pub(crate) fn note_possible_version_skew() {
    if *UPDATE_PENDING.peek() || *SKEW_PROBE_REQUESTED.peek() || baseline_sha().is_none() {
        return;
    }
    *SKEW_PROBE_REQUESTED.write() = true;
}

#[cfg(not(feature = "web"))]
pub(crate) fn note_possible_version_skew() {}

/// Root-level update-check hook. Mount once at `App`. No-op when the
/// build hash is unknown (dev builds) or `web_sys` is unavailable.
#[cfg(feature = "web")]
pub fn use_update_check() {
    // MAPPS-377: `baseline_sha()` is a compile-time constant (None only on dev
    // builds), so its value never varies across renders; still, call every hook
    // unconditionally and no-op inside when there is no baseline instead of
    // returning before the hooks, so the hook set stays stable.
    let baseline = baseline_sha();

    // Background polling loop.
    let baseline_for_future = baseline.clone();
    use_future(move || {
        let baseline = baseline_for_future.clone();
        async move {
            // Dev build (no baseline): nothing to poll for.
            let Some(baseline) = baseline else {
                return;
            };
            loop {
                gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_SECS as u32 * 1000).await;
                if *UPDATE_PENDING.peek() {
                    // Already flagged; the visibilitychange listener
                    // fires the reload at the next hidden boundary.
                    //
                    // MAPPS-428: the `MAX_DEFERRED_SECS` cap is evaluated
                    // HERE, not only in that listener. A tab that is never
                    // hidden nor re-shown fires no `visibilitychange` at
                    // all, so the listener-only check meant the fallback
                    // reload never happened and the user stayed on the
                    // stale bundle indefinitely.
                    if deferred_cap_elapsed() {
                        reload_now();
                    }
                    // Keep polling silently in case the deploy gets rolled
                    // back, but never un-flag (a confirmed-update signal
                    // stays confirmed).
                    continue;
                }
                if let Some(live) = fetch_live_build_sha().await {
                    if live != baseline {
                        note_new_build_detected(&baseline, &live);
                    }
                }
            }
        }
    });

    // MAPPS-428: on-demand probe. The fetch layer flips
    // `SKEW_PROBE_REQUESTED` when a request fails in a way consistent with
    // a version skew; this resource re-runs on that transition and checks
    // `build_sha` immediately instead of waiting for the next 5-minute
    // poll. Clearing the flag at the end re-runs the closure once more,
    // which lands on the early return below - so exactly one probe runs
    // per request, and none while one is in flight.
    let baseline_for_probe = baseline.clone();
    let _skew_probe = use_resource(move || {
        let baseline = baseline_for_probe.clone();
        async move {
            // Subscribe: a flip to `true` re-runs this closure.
            if !*SKEW_PROBE_REQUESTED.read() {
                return;
            }
            // Dev build (no baseline): nothing to compare against.
            // `note_possible_version_skew` already refuses to set the flag
            // there, so this is belt-and-braces.
            if let Some(baseline) = baseline {
                if let Some(live) = fetch_live_build_sha().await {
                    if live != baseline {
                        note_new_build_detected(&baseline, &live);
                    }
                }
            }
            *SKEW_PROBE_REQUESTED.write() = false;
        }
    });

    // visibilitychange handler: reload when the tab goes hidden after
    // a pending update, OR when it comes back visible and we have not
    // yet caught up (covers the "tab was backgrounded across a deploy"
    // case so users probe immediately on return).
    use_effect(move || {
        // Dev build (no baseline): register no visibilitychange listener.
        if baseline.is_none() {
            return;
        }
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
            if *UPDATE_PENDING.peek() {
                if hidden {
                    // User switched away. Reload while they are not
                    // looking; when they come back the new bundle is
                    // already loaded.
                    reload_now();
                } else if deferred_cap_elapsed() {
                    // Foregrounded with a pending reload. Stay polite
                    // unless we have been holding the reload for too
                    // long; then bite the bullet and reload anyway so
                    // users who never leave the tab still update.
                    reload_now();
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

/// MAPPS-428 recurrence gates. The hook itself needs a browser to run, so
/// the loop-placement gate is a source scan over this file; the
/// classification and dev-build gates are ordinary unit tests over the
/// pure helpers the hook calls.
#[cfg(test)]
mod tests {
    use super::{is_dev_sha, is_version_skew_status};

    const SRC: &str = include_str!("update_check.rs");

    /// This file minus its test module, which names the same symbols.
    fn production_src() -> &'static str {
        SRC.split("#[cfg(test)]")
            .next()
            .expect("split always yields a first segment")
    }

    /// A dev build (`GIT_HASH` empty or `"unknown"`) has no baseline, so
    /// nothing polls, nothing probes, and the banner never shows.
    #[test]
    fn dev_builds_have_no_baseline() {
        assert!(is_dev_sha(""));
        assert!(is_dev_sha("   "));
        assert!(is_dev_sha("unknown"));
        assert!(!is_dev_sha("f98fb4a"));
    }

    /// Only statuses that can mean "this bundle is older than the deploy"
    /// kick a probe. A 401/403/409 is ordinary app behaviour and must not
    /// re-fetch `_mokosh_config.js` on every occurrence.
    #[test]
    fn only_skew_shaped_statuses_probe() {
        for status in [404, 500, 502, 503, 599] {
            assert!(is_version_skew_status(status), "{status} should probe");
        }
        for status in [200, 201, 304, 400, 401, 403, 409, 410, 422, 600] {
            assert!(!is_version_skew_status(status), "{status} should not probe");
        }
    }

    /// The `MAX_DEFERRED_SECS` fallback must be evaluated in the polling
    /// loop, not only in the `visibilitychange` listener: a tab that is
    /// never hidden nor re-shown fires no visibility event at all, so a
    /// listener-only check never fires and the tab stays stale forever.
    #[test]
    fn deferred_cap_is_checked_from_the_polling_loop() {
        let src = production_src();
        let after_sleep = src
            .split_once("TimeoutFuture::new(POLL_INTERVAL_SECS")
            .expect("polling loop still sleeps on POLL_INTERVAL_SECS")
            .1;
        let loop_body = after_sleep
            .split_once("fetch_live_build_sha()")
            .expect("polling loop still probes for the live build sha")
            .0;
        assert!(
            loop_body.contains("deferred_cap_elapsed()"),
            "the MAX_DEFERRED_SECS cap must be evaluated in the polling loop"
        );
    }

    /// The stale-build flag is a global signal, so the banner reads it
    /// directly and no props are threaded through the layout.
    #[test]
    fn stale_flag_is_a_global_signal() {
        assert!(production_src().contains("pub static UPDATE_PENDING: GlobalSignal<bool>"));
    }
}

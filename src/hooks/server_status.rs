//! App-wide server-reachability monitoring (MAPPS-333).
//!
//! When mokosh-server is unreachable, the app reflects it as a single
//! clear app-wide state - the [`crate::components::ServerStatusBanner`] -
//! instead of scattered per-page errors or CORS-looking console noise,
//! and clears that state automatically once the server recovers.
//!
//! ## Detection vs recovery
//!
//! Detection is reactive and free: the central request helpers in
//! [`crate::hooks::fetch`] flip the [`fetch::SERVER_REACHABLE`] flag to
//! `false` the moment a request fails with a "down" condition (transport
//! failure, timeout, or `5xx`). No health poll runs while the app is
//! healthy, so there is zero added traffic in the normal case.
//!
//! Recovery is an explicit poll, but only while already down: this hook
//! hits the existing `/ready` probe on an interval and stops the instant
//! the server answers (the probe's own classification flips the flag back
//! to `true`). A reactive-only design (clear on the next successful
//! request) would leave the banner stuck on an idle screen until the user
//! happened to trigger another request; the recovery poll makes the
//! banner disappear on its own.
//!
//! [`fetch::SERVER_REACHABLE`]: crate::hooks::fetch::SERVER_REACHABLE

use dioxus::prelude::*;

/// How often to re-probe `/ready` while the server is marked unreachable.
/// Assumed 10s per MAPPS-333; revise if it proves too chatty or too slow.
#[cfg(feature = "web")]
const RECOVERY_POLL_SECS: u32 = 10;

/// Read the live server-reachable flag reactively. Call from a component
/// (e.g. the banner) so it re-renders when the flag transitions.
#[cfg(feature = "web")]
pub fn use_server_reachable() -> bool {
    *crate::hooks::fetch::SERVER_REACHABLE.read()
}

/// Non-web stub: native / SSR builds have no fetch layer to drive the
/// flag, so the app is always considered reachable.
#[cfg(not(feature = "web"))]
pub fn use_server_reachable() -> bool {
    true
}

/// Whether the app may currently send a *mutating* request (create /
/// update / delete). The inverse of "server down" (MAPPS-351): while
/// mokosh-server is unreachable we disable Save/submit at the button so
/// the user gets immediate, honest "can't save right now" feedback
/// instead of a write that silently fails and reads as data loss.
///
/// Reading it subscribes the caller, so a disabled Save button re-enables
/// on its own the instant the MAPPS-333 recovery poll flips the flag back.
/// When the local-first epic lands, the guard flips from "block" to
/// "enqueue" (see [`crate::hooks::edit_queue`]).
#[cfg(feature = "web")]
pub fn use_can_mutate() -> bool {
    use_server_reachable()
}

/// Non-web stub: no fetch layer, so writes are always permitted.
#[cfg(not(feature = "web"))]
pub fn use_can_mutate() -> bool {
    true
}

/// Mount once at the `App` root. Drives the recovery poll: idle (no
/// network) while reachable, polling `/ready` on an interval while down.
///
/// Implemented as a `use_resource` that reads `SERVER_REACHABLE`. While
/// reachable it returns immediately - no polling. When the flag flips to
/// `false`, the resource re-runs and enters the probe loop; the first
/// successful `/ready` (or any other request that succeeds in the
/// meantime) flips the flag back to `true`, which re-runs the resource
/// and drops it back to the idle branch.
#[cfg(feature = "web")]
pub fn use_server_status_monitor() {
    let _server_monitor = use_resource(move || async move {
        // Subscribe to the flag: a transition to `false` re-runs this
        // closure and starts the loop; a transition back to `true` re-runs
        // it and lands here, ending the poll.
        if *crate::hooks::fetch::SERVER_REACHABLE.read() {
            return;
        }
        loop {
            gloo_timers::future::TimeoutFuture::new(RECOVERY_POLL_SECS * 1000).await;
            // `probe` classifies its own result, so a recovered server
            // flips `SERVER_REACHABLE` back to `true` here.
            let _ = crate::hooks::fetch::api::probe("/ready").await;
            // `peek` (non-subscribing): the subscription above already
            // schedules the re-run; this just exits the loop promptly once
            // the server is back.
            if *crate::hooks::fetch::SERVER_REACHABLE.peek() {
                // RECONNECT TRANSITION (MAPPS-351). The server just came
                // back. Pages auto-refetch on their own because their
                // `use_remote_resource` subscribes to SERVER_REACHABLE.
                // When the local-first epic lands, drain any edits the
                // user made while offline here - the queue and drain are
                // already scaffolded in `crate::hooks::edit_queue`:
                //
                //   crate::hooks::edit_queue::replay_pending_edits().await;
                return;
            }
        }
    });
}

/// Non-web stub so `App` compiles under `cargo check`/`cargo test`
/// without the `web` feature.
#[cfg(not(feature = "web"))]
pub fn use_server_status_monitor() {}

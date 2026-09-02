//! MAPPS-351 (scaffold only): offline edit queue for future hold-and-replay.
//!
//! ## Status: infrastructure, intentionally inert
//!
//! MAPPS-351 ships options 1 + 2 (explicit unavailable content state,
//! auto-refetch on reconnect, and blocking writes at the button while the
//! server is down). Option 3 - holding a user's edits while offline and
//! replaying them when the connection returns - is the first slice of the
//! larger local-first epic and is deliberately NOT implemented here.
//!
//! This module lays down the seams so that work is a small, well-located
//! change rather than a new subsystem:
//!   - [`PendingEdit`] - the unit of held work.
//!   - [`EDIT_QUEUE`] - where held edits would live (a GlobalSignal, same
//!     primitive rationale as `SERVER_REACHABLE`: WASM is single-threaded
//!     and the enqueue sites are plain async fns, not components).
//!   - [`enqueue_pending_edit`] - the call the mutation guard would make
//!     instead of surfacing "can't save" (today writes are simply
//!     blocked; see [`crate::hooks::use_can_mutate`]).
//!   - [`replay_pending_edits`] - the reconnect drain, whose backoff /
//!     conflict-resolution body is commented out below. It is already
//!     wired (commented) into the recovery transition in
//!     [`crate::hooks::server_status::use_server_status_monitor`].
//!
//! Everything here compiles and is exported, but nothing calls the replay
//! path yet, so the runtime behavior is unchanged. When the local-first
//! epic starts: fill in [`replay_pending_edits`], flip the mutation guard
//! to enqueue instead of block, and uncomment the one call in the monitor.

#![allow(dead_code)]

#[cfg(feature = "app")]
use dioxus::prelude::*;

/// HTTP verb of a held mutation. Kept minimal on purpose; expand when the
/// replay path is actually built.
#[derive(Clone, Debug, PartialEq)]
pub enum PendingMethod {
    Post,
    Put,
    Patch,
    Delete,
}

/// One mutation captured while the server was unreachable, to be replayed
/// on reconnect. `body` is the already-serialized JSON payload (empty for
/// DELETE) so the queue does not need to be generic over every request
/// type. A monotonic `seq` preserves submit order across replay.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingEdit {
    /// Submit order; replay drains ascending so edits apply as issued.
    pub seq: u64,
    /// API path relative to `api_base()`, e.g. `/time-entries/123`.
    pub path: String,
    pub method: PendingMethod,
    /// Serialized JSON body; empty string for bodiless verbs.
    pub body: String,
}

/// The offline edit queue. Empty in the shipped build (nothing enqueues
/// yet). A `GlobalSignal` so the (future) enqueue sites in the fetch layer
/// (plain async fns that cannot reach a context signal) can push to it,
/// exactly like `SERVER_REACHABLE`.
#[cfg(feature = "app")]
pub static EDIT_QUEUE: GlobalSignal<Vec<PendingEdit>> = Signal::global(Vec::new);

/// Append an edit to the offline queue. The mutation guard would call this
/// instead of blocking once hold-and-replay lands; today it is unused
/// (writes are blocked at the button while down).
#[cfg(feature = "app")]
pub fn enqueue_pending_edit(edit: PendingEdit) {
    EDIT_QUEUE.write().push(edit);
}

/// Non-web stub.
#[cfg(not(feature = "app"))]
pub fn enqueue_pending_edit(_edit: PendingEdit) {}

/// Drain and replay the offline queue after the server comes back.
///
/// SCAFFOLD: today this is a no-op beyond clearing the (always-empty)
/// queue. The real implementation is sketched, commented out, below so the
/// shape is fixed: drain in `seq` order, re-issue each request through the
/// normal authed fetch helpers, retry transient failures with exponential
/// backoff, and surface conflicts (e.g. a `409`) to the user rather than
/// silently dropping their edit.
#[cfg(feature = "app")]
pub async fn replay_pending_edits() {
    // Nothing is ever enqueued in the shipped build, so this just resets
    // the (empty) queue and returns. Keeping the drain here means the
    // reconnect call site is already correct.
    if EDIT_QUEUE.peek().is_empty() {
        return;
    }

    // ---- local-first epic: replace the early-return above with this ----
    //
    // use crate::platform::timer::sleep_ms;
    //
    // // Snapshot + clear so new edits made during replay queue behind it.
    // let mut pending: Vec<PendingEdit> = EDIT_QUEUE.write().drain(..).collect();
    // pending.sort_by_key(|e| e.seq);
    //
    // for edit in pending {
    //     let mut backoff_ms = 250u32;
    //     loop {
    //         let result = match edit.method {
    //             PendingMethod::Delete => {
    //                 crate::hooks::fetch::api::delete_authed(&edit.path).await.map(|_| ())
    //             }
    //             PendingMethod::Post | PendingMethod::Put | PendingMethod::Patch => {
    //                 // Re-issue with the raw serialized body; a
    //                 // body-passthrough variant of the authed helpers is
    //                 // needed here (the typed ones serialize their input).
    //                 crate::hooks::fetch::api::replay_authed(
    //                     &edit.path, &edit.method, &edit.body,
    //                 ).await
    //             }
    //         };
    //         match result {
    //             Ok(()) => break,
    //             // Server up but rejected the write (validation / conflict):
    //             // do NOT retry, surface it so the edit is not lost silently.
    //             Err(e) if is_conflict(&e) => {
    //                 crate::hooks::toast::error(format!(
    //                     "An edit made while offline could not be applied: {e}"
    //                 ));
    //                 break;
    //             }
    //             // Transport failure again: the server dropped back off.
    //             // Re-queue the remaining edits and let the next reconnect
    //             // drain them.
    //             Err(_) if !*crate::hooks::fetch::SERVER_REACHABLE.peek() => {
    //                 EDIT_QUEUE.write().push(edit);
    //                 return;
    //             }
    //             Err(_) => {
    //                 TimeoutFuture::new(backoff_ms).await;
    //                 backoff_ms = (backoff_ms * 2).min(8_000);
    //             }
    //         }
    //     }
    // }
    // --------------------------------------------------------------------

    EDIT_QUEUE.write().clear();
}

/// Non-web stub.
#[cfg(not(feature = "app"))]
pub async fn replay_pending_edits() {}

/// Convenience hook so a component (rather than the monitor) could trigger
/// a replay; unused today but part of the seam. Spawns the drain without
/// blocking render.
#[cfg(feature = "app")]
pub fn use_replay_pending_edits() {
    // Placeholder for the local-first epic. Intentionally does not spawn
    // anything yet so the shipped build has no behavior change.
}

/// Non-web stub.
#[cfg(not(feature = "app"))]
pub fn use_replay_pending_edits() {}

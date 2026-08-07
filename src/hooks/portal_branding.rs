//! PMS-729: shared client-portal branding hint.
//!
//! The `/portal/host` endpoint returns the MSP name + optional logo URL
//! for the tenant whose slug is derived from the current host. Both the
//! login page (pre-auth) and the post-login layout paint the same
//! branding, so the hint is stored in a `GlobalSignal` that either
//! component can read.
//!
//! WASM is single-threaded so a `GlobalSignal` is the right primitive
//! here (same rationale as `SERVER_REACHABLE` in `hooks/fetch.rs`). The
//! signal starts `None` (nothing known yet) and flips to `Some(hint)` on
//! the first successful `/portal/host` response. A 404 (this is not a
//! portal host) or transport error keeps it `None`, so the login page
//! and layout both fall back to the generic "Client Portal" title.
//!
//! A separate `HAS_FETCHED_HINT` flag prevents the login page + the
//! layout from firing two concurrent GETs when a user opens the SPA
//! directly on `/portal/tickets` and the layout mounts before the login
//! page ever renders.

use dioxus::prelude::*;
use serde::Deserialize;

/// The branding hint the SPA displays above the credential fields and
/// in the post-login header. Matches mokosh-server `PortalHostHint`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PortalHostHint {
    pub name: String,
    #[serde(default)]
    pub logo_url: Option<String>,
}

/// The current portal-host branding hint, shared across pages. `None`
/// before the fetch completes, on a legacy host (the endpoint 404s),
/// and on any transport error. `Some(hint)` on a successful resolve.
#[cfg(feature = "web")]
pub static PORTAL_HOST_HINT: GlobalSignal<Option<PortalHostHint>> = Signal::global(|| None);

/// One-shot latch: `true` once the fetch has been kicked off in this
/// tab so components mounted later do not re-fire the same request.
#[cfg(feature = "web")]
pub static HAS_FETCHED_HINT: GlobalSignal<bool> = Signal::global(|| false);

/// Kick off the `/portal/host` fetch if the current host looks like a
/// portal host and the fetch has not already been started. Idempotent:
/// safe to call from the login page's `use_future` AND from
/// `PortalLayout::use_future` without producing duplicate requests.
///
/// Runs only inside `feature = "web"`; on non-web builds this is a no-op
/// so call sites do not need their own `cfg` gate.
#[cfg(feature = "web")]
pub fn ensure_portal_branding_fetch() {
    use crate::hooks::fetch::api::{get_typed, on_portal_host, ApiError};

    if !on_portal_host() {
        return;
    }
    if *HAS_FETCHED_HINT.peek() {
        return;
    }
    *HAS_FETCHED_HINT.write() = true;

    spawn(async move {
        match get_typed::<PortalHostHint>("/portal/host").await {
            Ok(hint) => *PORTAL_HOST_HINT.write() = Some(hint),
            // 404 is the fail-closed shape for "not a portal host". Any
            // other error also leaves the hint unset so the UI falls
            // back to the generic layout.
            Err(ApiError::Status { code: 404, .. }) => {}
            Err(_) => {}
        }
    });
}

#[cfg(not(feature = "web"))]
pub fn ensure_portal_branding_fetch() {}

/// Read a cloned snapshot of the current hint. Registers a reactive
/// dependency: components that call this re-render when the hint flips
/// from `None` to `Some`.
#[cfg(feature = "web")]
pub fn use_portal_host_hint() -> Option<PortalHostHint> {
    PORTAL_HOST_HINT.read().clone()
}

#[cfg(not(feature = "web"))]
pub fn use_portal_host_hint() -> Option<PortalHostHint> {
    None
}

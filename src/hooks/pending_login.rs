//! MAPPS-497 item 6: cross-page state carrier for the identity-first
//! login intermediate steps. `/login` populates it and navigates to
//! `/pick-tenant` (needs_selection) or `/create-org` (needs_setup);
//! those pages read it, run their own submit, then clear it. If either
//! page mounts with the signal empty (browser refresh mid-flow, direct
//! deep-link), the page redirects the operator back to `/login`.
//!
//! A GlobalSignal is the simplest cross-page carrier that survives
//! Dioxus navigation without touching sessionStorage; the state is
//! short-lived (matches the 5-minute identity_token TTL) and never
//! needs to survive a page reload (a full reload wipes the in-memory
//! session anyway).

use dioxus::prelude::*;
use serde::Deserialize;

/// One entry in the picker list returned from `/auth/login`
/// (`needs_selection` branch) and rendered on `/pick-tenant`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PickerMembership {
    pub tenant_id: String,
    pub tenant_name: String,
    #[serde(default)]
    pub tenant_slug: String,
    #[serde(default)]
    pub tenant_kind: String,
    pub role: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub is_active: bool,
}

/// Pending intermediate state between `/login` and one of its
/// downstream routes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingLogin {
    /// The short-lived JWT (typ="identity") returned in the login
    /// response's `identity_token` field. `None` when no pending state
    /// is in flight.
    pub identity_token: Option<String>,
    /// The membership list from the `needs_selection` branch. Empty
    /// when no picker step is in flight.
    pub memberships: Vec<PickerMembership>,
}

/// Root-mount signal shared by `/login`, `/pick-tenant`, and
/// `/create-org`. `/login` writes to it; the two downstream pages
/// read it and clear it when they're done.
pub static PENDING_LOGIN: GlobalSignal<PendingLogin> = Signal::global(PendingLogin::default);

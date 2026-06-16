//! Cached system-version state for the admin update banner (MAPPS-203).
//!
//! ## Why
//!
//! [`crate::components::UpdateBanner`] is mounted inside `AppLayout`,
//! and every page in the SPA wraps its content in a fresh `AppLayout`.
//! Each navigation therefore tears the banner down and re-mounts it,
//! which re-runs its `use_resource(get_version)` from scratch. The
//! resource's initial `None` state drives `BannerState::Reserving`,
//! which reserves a full banner-row's worth of height invisibly. As
//! soon as the fetch returns "no update" the row collapses with a
//! 200ms `grid-template-rows: 1fr -> 0fr` animation, and every page
//! visit visibly nudges the rest of the layout for the duration of
//! that collapse. MAPPS-203 is the user-reported symptom: the
//! sidebar scrolls to the top and the page jumps on every nav click.
//!
//! ## The fix
//!
//! Lift the version-pair signal to the App root and let the banner
//! consume it via context. The first AppLayout mount still goes
//! through `Reserving` (we genuinely do not know the version yet on
//! cold start, so reserving height is honest), but on every
//! subsequent re-mount the banner reads the cached `Some(result)` and
//! starts at the resolved state directly - no `Reserving` row, no
//! collapse animation, no shift. Foreground refreshes still happen on
//! a long polling cadence (driven from elsewhere), and the cache is
//! updated whenever a fresh poll lands.
//!
//! Non-admins never write to the cache (no need - the banner returns
//! `rsx! {}` for them anyway), so the signal stays `None` for the
//! whole non-admin session.

use dioxus::prelude::*;

use crate::modules::system::SystemVersion;

/// Cached result of the most recent `GET /api/v1/version`. `None` means
/// the App has not yet seen a result (first cold mount, or non-admin
/// session). `Some(Ok(...))` is the live version pair; `Some(Err(...))`
/// means the most recent fetch errored - the banner collapses on both
/// `Err` and `Ok(no-update)` so the wire-shape difference does not
/// matter to the consumer.
pub type CachedVersion = Option<Result<SystemVersion, String>>;

/// Provide the cache signal at the App root. Mirrors
/// [`crate::hooks::use_sidebar_provider`] and the auth provider.
pub fn use_version_cache_provider() -> Signal<CachedVersion> {
    let s = use_signal::<CachedVersion>(|| None);
    use_context_provider(|| s);
    s
}

/// Consume the cache signal from inside the UpdateBanner (or anywhere
/// else that needs to read the live version pair without re-firing
/// `get_version` itself).
pub fn use_version_cache() -> Signal<CachedVersion> {
    use_context::<Signal<CachedVersion>>()
}

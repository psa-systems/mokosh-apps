//! Remote-data loading with an explicit server-unreachable state (MAPPS-351).
//!
//! ## Why this exists
//!
//! Every data-driven page used to load with the pattern
//!
//! ```ignore
//! let x = use_resource(|| async {
//!     api::get_authed::<T>("/path").await.ok().unwrap_or_default()
//! });
//! let x = x.read_unchecked().clone().unwrap_or_default();
//! ```
//!
//! The `.ok().unwrap_or_default()` swallows a failed request into a
//! *default* value, so when mokosh-server is unreachable the page renders
//! an empty table / zero counts and looks exactly like a brand-new tenant
//! with no data. The MAPPS-333 banner is the only signal; the content
//! itself lies.
//!
//! [`use_remote_resource`] keeps the same ergonomics but preserves the
//! failure so a page can tell "no data" apart from "server down", and it
//! subscribes to the MAPPS-333 [`SERVER_REACHABLE`] flag so the resource
//! **auto-refetches the moment the server comes back** (paired with the
//! recovery poll in [`crate::hooks::server_status`]).
//!
//! The `(outcome, reachable) -> RemoteData` decision is factored into the
//! pure [`classify_remote`] so it is unit-testable without a renderer.
//!
//! [`SERVER_REACHABLE`]: crate::hooks::fetch::SERVER_REACHABLE

#[cfg(feature = "web")]
use dioxus::prelude::*;

/// The loading state of a single remote resource.
///
/// `Unavailable` is reserved for "the request failed *and* the server is
/// currently marked unreachable" - i.e. an outage the recovery poll will
/// clear. A request that fails while the server is reachable (a 4xx, a
/// one-off decode error) is NOT an outage: it collapses to
/// `Ready(T::default())`, preserving the previous `unwrap_or_default()`
/// behavior so those call sites do not regress.
#[derive(Clone, Debug)]
pub enum RemoteData<T> {
    /// Request still in flight (resource not yet resolved once).
    Loading,
    /// A value to render (a real payload, or the type default for a
    /// non-outage failure).
    Ready(T),
    /// The server is unreachable and this resource could not load. The
    /// page should render [`crate::components::ContentUnavailable`]
    /// instead of an empty body.
    Unavailable,
}

impl<T> RemoteData<T> {
    /// `true` while the first request is still in flight.
    pub fn is_loading(&self) -> bool {
        matches!(self, RemoteData::Loading)
    }

    /// `true` when the server is down and this resource could not load.
    /// Pages gate [`crate::components::ContentUnavailable`] on this.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, RemoteData::Unavailable)
    }

    /// Borrow the ready value, if any.
    pub fn ready(&self) -> Option<&T> {
        match self {
            RemoteData::Ready(t) => Some(t),
            _ => None,
        }
    }

    /// Take the ready value, if any.
    pub fn into_ready(self) -> Option<T> {
        match self {
            RemoteData::Ready(t) => Some(t),
            _ => None,
        }
    }
}

impl<T: Default> RemoteData<T> {
    /// The ready value, or the type default while `Loading`/`Unavailable`.
    ///
    /// A drop-in for the old `.unwrap_or_default()` call sites: a page can
    /// render its normal body from this while still consulting
    /// [`is_unavailable`](RemoteData::is_unavailable) on its *primary*
    /// resource to decide whether to swap in the outage state.
    pub fn value_or_default(self) -> T {
        self.into_ready().unwrap_or_default()
    }
}

/// Pure classifier: map a resource outcome plus the current reachability
/// flag onto a [`RemoteData`]. Factored out so the mapping is unit-tested
/// without spinning up a Dioxus runtime.
///
/// - `None` (resource unresolved) -> `Loading`.
/// - `Some(Ok(v))` -> `Ready(v)`.
/// - `Some(Err(_))` while unreachable -> `Unavailable` (recovery poll
///   will clear it).
/// - `Some(Err(_))` while reachable -> `Ready(default)`, matching the old
///   `unwrap_or_default()` so non-outage failures do not regress.
pub fn classify_remote<T: Default>(
    outcome: Option<Result<T, String>>,
    reachable: bool,
) -> RemoteData<T> {
    match outcome {
        None => RemoteData::Loading,
        Some(Ok(v)) => RemoteData::Ready(v),
        Some(Err(_)) if !reachable => RemoteData::Unavailable,
        Some(Err(_)) => RemoteData::Ready(T::default()),
    }
}

/// Load a remote resource with an explicit unreachable state and
/// auto-refetch on reconnect (MAPPS-351).
///
/// Pass an async fetcher that returns the *raw* `Result` (do NOT
/// `.ok().unwrap_or_default()` inside it - that is exactly the swallow
/// this replaces). Example:
///
/// ```ignore
/// let report = use_remote_resource(|| async {
///     crate::hooks::fetch::api::get_authed::<DashboardReport>("/reports/dashboard").await
/// });
/// if report.is_unavailable() {
///     return rsx! { AppLayout { title: "Dashboard", ContentUnavailable {} } };
/// }
/// let report = report.value_or_default();
/// ```
///
/// The resource re-runs when either the reachability flag flips (so it
/// refetches the instant the server returns) or the active tenant changes
/// (org switch / token refresh, Phase-4 F1), mirroring the subscription
/// the hand-rolled resources used.
#[cfg(feature = "web")]
pub fn use_remote_resource<T, F, Fut>(mut fetcher: F) -> RemoteData<T>
where
    T: Default + Clone + 'static,
    F: FnMut() -> Fut + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + 'static,
{
    let resource = use_resource(move || {
        let fut = fetcher();
        async move {
            // Read both signals BEFORE the first await so Dioxus records
            // them as dependencies (same placement the hand-rolled
            // resources used for `active_tenant_generation`). A flip of
            // SERVER_REACHABLE back to `true` re-runs this and refetches.
            let _reachable = *crate::hooks::fetch::SERVER_REACHABLE.read();
            let _gen = crate::hooks::fetch::active_tenant_generation();
            fut.await
        }
    });

    let reachable = *crate::hooks::fetch::SERVER_REACHABLE.read();
    classify_remote(resource.read_unchecked().clone(), reachable)
}

/// Non-web stub: native / SSR builds have no fetch layer, so a resource is
/// always considered `Ready` with the type default (there is no outage to
/// surface). Keeps `cargo check` green without the `web` feature.
#[cfg(not(feature = "web"))]
pub fn use_remote_resource<T, F, Fut>(_fetcher: F) -> RemoteData<T>
where
    T: Default + Clone + 'static,
    F: FnMut() -> Fut + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + 'static,
{
    RemoteData::Ready(T::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_is_loading() {
        let out: Option<Result<Vec<i32>, String>> = None;
        assert!(classify_remote(out, true).is_loading());
        let out: Option<Result<Vec<i32>, String>> = None;
        assert!(classify_remote(out, false).is_loading());
    }

    #[test]
    fn ok_is_ready_with_value_regardless_of_reachability() {
        let out = Some(Ok(vec![1, 2, 3]));
        match classify_remote(out, true) {
            RemoteData::Ready(v) => assert_eq!(v, vec![1, 2, 3]),
            _ => panic!("expected Ready"),
        }
        // A late success that arrives while still flagged down is still a
        // real payload, and rendering it also clears the outage view.
        let out = Some(Ok(vec![4]));
        match classify_remote(out, false) {
            RemoteData::Ready(v) => assert_eq!(v, vec![4]),
            _ => panic!("expected Ready"),
        }
    }

    #[test]
    fn err_while_unreachable_is_unavailable() {
        let out: Option<Result<Vec<i32>, String>> = Some(Err("failed to fetch".into()));
        assert!(classify_remote(out, false).is_unavailable());
    }

    #[test]
    fn err_while_reachable_falls_back_to_default_not_outage() {
        // A 4xx / decode error while the server is up must NOT show the
        // "server unreachable, retrying" state (it would never recover via
        // the reconnect poll). It degrades to the type default, exactly
        // like the old `unwrap_or_default()`.
        let out: Option<Result<Vec<i32>, String>> = Some(Err("403 forbidden".into()));
        match classify_remote(out, true) {
            RemoteData::Ready(v) => assert!(v.is_empty()),
            _ => panic!("expected Ready(default)"),
        }
    }

    #[test]
    fn down_then_reconnect_transition() {
        // Models the full server-down -> reconnect sequence a page sees as
        // its resource resolves and the reachability flag flips back
        // (MAPPS-351 AC: cover the server-down and reconnect transitions).
        // 1. In flight, server already down -> Loading (banner shows, no
        //    misleading empty body yet).
        let loading: Option<Result<Vec<i32>, String>> = None;
        assert!(classify_remote(loading, false).is_loading());
        // 2. Fetch failed while down -> Unavailable (ContentUnavailable).
        let failed: Option<Result<Vec<i32>, String>> = Some(Err("failed to fetch".into()));
        assert!(classify_remote(failed, false).is_unavailable());
        // 3. Recovery poll flips reachable=true, the resource re-runs and
        //    succeeds -> Ready with the real payload (page repopulates).
        let recovered = Some(Ok(vec![1, 2]));
        match classify_remote(recovered, true) {
            RemoteData::Ready(v) => assert_eq!(v, vec![1, 2]),
            _ => panic!("expected Ready after reconnect"),
        }
    }

    #[test]
    fn value_or_default_bridges_old_call_sites() {
        let rd: RemoteData<Vec<i32>> = RemoteData::Unavailable;
        assert!(rd.value_or_default().is_empty());
        let rd: RemoteData<Vec<i32>> = RemoteData::Ready(vec![9]);
        assert_eq!(rd.value_or_default(), vec![9]);
    }
}

//! Shared, session-scoped cache for the dashboard report
//! (`GET /reports/dashboard`), applying the pattern established by
//! [`crate::hooks::user_roster`] to the saved-dashboard widget surface
//! (`src/pages/dashboards_view.rs`).
//!
//! `WidgetTicketsByStatus` and `WidgetSlaAtRisk` each ran their own
//! `use_remote_resource` against the same endpoint, reading different
//! fields of the same response. A layout placing both on one mount fired
//! two identical requests instead of one.
//!
//! Follows the same shape as [`crate::hooks::user_roster`] and
//! [`crate::hooks::work_types`], except the shared [`Resource`] holds the
//! raw fetch outcome (`Option<Result<DashboardReportLite, String>>`)
//! rather than a plain `Vec`, so [`use_dashboard_report`] can run it
//! through [`crate::hooks::classify_remote`] and keep the
//! `RemoteData::Unavailable` state both widgets already render on a
//! server outage. Both widgets want the report unconditionally, so
//! unlike the `enabled`-gated precedents, [`use_dashboard_report`] takes
//! no argument: calling it is itself the "wanted" signal.

use dioxus::prelude::*;

use crate::hooks::remote_data::{classify_remote, RemoteData};

/// The subset of `GET /reports/dashboard` the two widgets read.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct DashboardReportLite {
    #[serde(default)]
    pub open_by_priority: Vec<ReportBucket>,
    #[serde(default)]
    pub sla_warnings: i64,
    #[serde(default)]
    pub sla_breached: i64,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ReportBucket {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub count: i64,
}

/// Shared "has any consumer asked for the report yet" flag, mirroring
/// [`crate::hooks::user_roster`]'s `RosterWanted`.
type DashboardReportWanted = Signal<bool>;

type DashboardReportOutcome = Option<Result<DashboardReportLite, String>>;

/// Provide the shared dashboard-report resource and its `wanted` flag at
/// the App root. Call once, alongside the other `use_*_provider` calls.
pub fn provide_dashboard_report() {
    let wanted = use_signal(|| false);
    use_context_provider::<DashboardReportWanted>(|| wanted);

    let resource = use_resource(move || async move {
        // Read both signals BEFORE the first await, matching
        // `use_remote_resource`, so a flip of either re-runs this.
        let _reachable = *crate::hooks::fetch::SERVER_REACHABLE.read();
        let _gen = crate::hooks::fetch::active_tenant_generation();
        if !*wanted.read() {
            return None;
        }
        #[cfg(feature = "app")]
        {
            Some(
                crate::hooks::fetch::api::get_authed::<DashboardReportLite>("/reports/dashboard")
                    .await,
            )
        }
        #[cfg(not(feature = "app"))]
        {
            Some(Ok(DashboardReportLite::default()))
        }
    });
    use_context_provider::<Resource<DashboardReportOutcome>>(|| resource);
}

/// Ask for the cached dashboard report. Calling this from any mount is
/// enough to trigger (and thereafter share, via [`provide_dashboard_report`])
/// the one underlying fetch; every consumer, including a later mount of the
/// same or a different widget, reads the same cached outcome.
pub fn use_dashboard_report() -> RemoteData<DashboardReportLite> {
    let mut wanted = use_context::<DashboardReportWanted>();
    if !*wanted.read() {
        wanted.set(true);
    }
    let resource = use_context::<Resource<DashboardReportOutcome>>();
    let reachable = *crate::hooks::fetch::SERVER_REACHABLE.read();
    classify_remote(resource.read_unchecked().clone().flatten(), reachable)
}

#[cfg(test)]
mod tests {
    // MAPPS-982: pins the endpoint to this one module the way
    // `mentions::tests::the_directory_is_the_source_not_user_management`
    // pins `/auth/directory` to `mentions.rs`, so a future widget that
    // grows its own fetch of the same endpoint is caught here instead of
    // reintroducing the duplicate request this issue removed.
    const HOOK_SRC: &str = include_str!("dashboard_report.rs");
    const WIDGETS_SRC: &str = include_str!("../pages/dashboards_view.rs");

    #[test]
    fn the_endpoint_lives_only_in_the_shared_hook() {
        let hook_code = &HOOK_SRC[..HOOK_SRC.find("mod tests").expect("tests are in this file")];
        assert_eq!(hook_code.matches("\"/reports/dashboard\"").count(), 1);
        assert!(!WIDGETS_SRC.contains("/reports/dashboard"));
    }

    #[test]
    fn both_widgets_call_the_shared_accessor() {
        assert_eq!(
            WIDGETS_SRC
                .matches("crate::hooks::use_dashboard_report()")
                .count(),
            2,
            "WidgetTicketsByStatus and WidgetSlaAtRisk should both read the shared hook"
        );
        assert!(!WIDGETS_SRC.contains("DashboardReportLite"));
    }
}

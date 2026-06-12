//! System Status page (PMS-237).
//!
//! Elevates the build-info that used to live only in the cramped
//! bottom-left footer (`VersionFooter`) into a dedicated, linkable
//! diagnostics page reachable from the user menu. Three cards:
//!
//! 1. **Client build** - the WASM bundle's version / commit / build
//!    date, baked in by `build.rs` (the same values the footer shows).
//! 2. **API server** - live reachability of mokosh-server plus the
//!    server's own build, from the public `GET /api/v1/version`
//!    endpoint. A transport failure (server down, DNS, CORS) flips the
//!    badge to "Unreachable".
//! 3. **Dependencies** - the readiness breakdown from `GET
//!    /api/v1/ready`, which reports the database and Infisical probes.
//!    That endpoint answers `503` with a JSON body when a dependency is
//!    down, so the page reads it through the tolerant `api::probe`
//!    helper rather than the 2xx-only `get`.
//!
//! A fourth card surfaces the resolved runtime endpoints (API base,
//! OIDC issuer, hub URL) so support can confirm which backend a given
//! browser session is actually wired to without opening devtools.

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{AppLayout, Badge, BadgeVariant, Button, ButtonVariant, Card, PageHeader};

/// Server build info (`GET /api/v1/version`). Mirrors mokosh-server's
/// `VersionInfo`; fields not rendered are dropped at deserialise time.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct ServerVersion {
    #[serde(default)]
    version: String,
    #[serde(default)]
    git_describe: String,
    #[serde(default)]
    git_hash: String,
    #[serde(default)]
    build_date: String,
}

/// Readiness breakdown (`GET /api/v1/ready`). The endpoint returns this
/// shape on both `200` (ready) and `503` (a dependency is down), with
/// the failing dependency's value replaced by a short error string.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct Readiness {
    #[serde(default)]
    status: String,
    #[serde(default)]
    checks: ReadinessChecks,
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
struct ReadinessChecks {
    #[serde(default)]
    db: String,
    #[serde(default)]
    infisical: String,
}

/// Aggregated result of one status sweep. Each server probe is an
/// independent `Result` so a reachable server with a degraded
/// dependency still renders the parts that did resolve.
#[derive(Clone, Debug, PartialEq)]
struct StatusReport {
    server: Result<ServerVersion, String>,
    readiness: Result<Readiness, String>,
}

/// Probe `GET /api/v1/version`. A non-2xx or unparseable body is an
/// error; a clean parse is the server's build info.
async fn probe_server_version() -> Result<ServerVersion, String> {
    #[cfg(feature = "web")]
    {
        let (status, body) = crate::hooks::fetch::api::probe("/version").await?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }
        serde_json::from_str::<ServerVersion>(&body).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "web"))]
    {
        Err("unavailable in non-web build".to_string())
    }
}

/// Probe `GET /api/v1/ready`. Parses the JSON body on both `200` and
/// `503`; only a transport failure or an unparseable body is an error.
async fn probe_readiness() -> Result<Readiness, String> {
    #[cfg(feature = "web")]
    {
        let (_status, body) = crate::hooks::fetch::api::probe("/ready").await?;
        serde_json::from_str::<Readiness>(&body).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "web"))]
    {
        Err("unavailable in non-web build".to_string())
    }
}

/// Map a dependency check string ("ok" / "skipped" / "error: ...") to a
/// labelled badge.
fn check_badge(value: &str) -> Element {
    let (variant, label) = if value == "ok" {
        (BadgeVariant::Green, "OK".to_string())
    } else if value == "skipped" {
        (BadgeVariant::Gray, "Skipped".to_string())
    } else if value.is_empty() {
        (BadgeVariant::Gray, "Unknown".to_string())
    } else {
        (BadgeVariant::Red, value.to_string())
    };
    rsx! { Badge { variant, "{label}" } }
}

/// One label/value row inside a status card.
#[component]
fn StatusRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex items-center justify-between py-2 border-b border-gray-100 dark:border-gray-700 last:border-0",
            span { class: "text-sm text-gray-500 dark:text-gray-400", "{label}" }
            span { class: "text-sm font-mono text-gray-900 dark:text-white break-all text-right ml-4", "{value}" }
        }
    }
}

/// System Status page.
#[component]
pub fn SystemStatusPage() -> Element {
    let mut report = use_resource(|| async move {
        let server = probe_server_version().await;
        let readiness = probe_readiness().await;
        StatusReport { server, readiness }
    });

    let snapshot = report.read_unchecked().clone();
    let loading = snapshot.is_none();

    // Client build metadata, baked in by `build.rs`.
    use crate::utils::version::{BUILD_DATE, GIT_HASH, VERSION};

    // Resolved runtime endpoints for the connection card.
    let cfg = crate::modules::oidc::OidcConfig::for_current_origin();
    let api_base = {
        #[cfg(feature = "web")]
        {
            crate::hooks::fetch::api::api_base()
        }
        #[cfg(not(feature = "web"))]
        {
            "n/a".to_string()
        }
    };
    let issuer = cfg.issuer.to_string();
    let hub_url = cfg.hub_url("/");

    rsx! {
        AppLayout { title: "System Status",
            PageHeader {
                title: "System Status",
                subtitle: "Build versions and live health of the Mokosh client and API.",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| report.restart(),
                        "Refresh"
                    }
                },
            }

            div { class: "grid grid-cols-1 gap-5 lg:grid-cols-2",

                // Client build (always available, no network).
                Card {
                    div { class: "flex items-center justify-between mb-3",
                        h3 { class: "text-base font-semibold text-gray-900 dark:text-white", "Client build" }
                        Badge { variant: BadgeVariant::Green, "Running" }
                    }
                    StatusRow { label: "Version".to_string(), value: VERSION.to_string() }
                    StatusRow { label: "Commit".to_string(), value: GIT_HASH.to_string() }
                    StatusRow { label: "Built".to_string(), value: BUILD_DATE.to_string() }
                }

                // API server reachability + server build.
                Card {
                    div { class: "flex items-center justify-between mb-3",
                        h3 { class: "text-base font-semibold text-gray-900 dark:text-white", "API server" }
                        if loading {
                            Badge { variant: BadgeVariant::Gray, "Checking..." }
                        } else if matches!(&snapshot, Some(r) if r.server.is_ok()) {
                            Badge { variant: BadgeVariant::Green, "Operational" }
                        } else {
                            Badge { variant: BadgeVariant::Red, "Unreachable" }
                        }
                    }
                    match snapshot.as_ref().map(|r| &r.server) {
                        None => rsx! {
                            p { class: "text-sm text-gray-500 dark:text-gray-400", "Checking the API server..." }
                        },
                        Some(Ok(v)) => rsx! {
                            StatusRow { label: "Version".to_string(), value: v.version.clone() }
                            StatusRow { label: "Release".to_string(), value: v.git_describe.clone() }
                            StatusRow { label: "Commit".to_string(), value: v.git_hash.clone() }
                            StatusRow { label: "Built".to_string(), value: v.build_date.clone() }
                        },
                        Some(Err(e)) => rsx! {
                            p { class: "text-sm text-red-600 dark:text-red-400",
                                "Could not reach the API server: {e}"
                            }
                        },
                    }
                }

                // Dependency readiness breakdown.
                Card {
                    div { class: "flex items-center justify-between mb-3",
                        h3 { class: "text-base font-semibold text-gray-900 dark:text-white", "Dependencies" }
                        match snapshot.as_ref().map(|r| &r.readiness) {
                            None => rsx! { Badge { variant: BadgeVariant::Gray, "Checking..." } },
                            Some(Ok(r)) if r.status == "ready" => rsx! { Badge { variant: BadgeVariant::Green, "Ready" } },
                            Some(Ok(_)) => rsx! { Badge { variant: BadgeVariant::Yellow, "Degraded" } },
                            Some(Err(_)) => rsx! { Badge { variant: BadgeVariant::Red, "Unknown" } },
                        }
                    }
                    match snapshot.as_ref().map(|r| &r.readiness) {
                        None => rsx! {
                            p { class: "text-sm text-gray-500 dark:text-gray-400", "Checking dependencies..." }
                        },
                        Some(Ok(r)) => rsx! {
                            div { class: "flex items-center justify-between py-2 border-b border-gray-100 dark:border-gray-700",
                                span { class: "text-sm text-gray-500 dark:text-gray-400", "Database" }
                                {check_badge(&r.checks.db)}
                            }
                            div { class: "flex items-center justify-between py-2",
                                span { class: "text-sm text-gray-500 dark:text-gray-400", "Infisical" }
                                {check_badge(&r.checks.infisical)}
                            }
                        },
                        Some(Err(e)) => rsx! {
                            p { class: "text-sm text-red-600 dark:text-red-400",
                                "Could not read readiness: {e}"
                            }
                        },
                    }
                }

                // Resolved runtime endpoints.
                Card {
                    h3 { class: "text-base font-semibold text-gray-900 dark:text-white mb-3", "Connection" }
                    StatusRow { label: "API base".to_string(), value: api_base }
                    StatusRow { label: "OIDC issuer".to_string(), value: issuer }
                    StatusRow { label: "Hub".to_string(), value: hub_url }
                }
            }
        }
    }
}

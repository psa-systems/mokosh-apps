//! System-level introspection: client-vs-server version skew.
//!
//! Reads the server's running build from the public
//! `GET /api/v1/version` endpoint mokosh-server actually serves, then
//! pairs that with the SPA's own compile-time `CARGO_PKG_VERSION`.
//! mokosh-server and mokosh-www are released together, so the version
//! the server reports is the version the matching client bundle should
//! also be on; when the loaded bundle is older (the user is on a stale
//! cached copy, or the client image has not been pulled) the update
//! banner surfaces a prompt to pull the new images.
//!
//! `GET /api/v1/version` reports only the server's own build, with no
//! registry "latest available" poll, so server-side update detection
//! cannot be driven from this endpoint: `server.latest` is always
//! `None` and the banner's server line stays hidden until such an
//! endpoint ships. On any fetch error [`get_version`] returns `Err`
//! and callers (the update banner) render nothing.

use serde::{Deserialize, Deserializer};

/// API path for the server version endpoint. Resolved against the API
/// base URL by `crate::hooks::fetch::api`. This is the public build-info
/// endpoint mokosh-server serves (the same one the System Status page
/// probes); the previously-used `/system/version` was never deployed
/// and 404'd on every poll (MAPPS-151).
pub const VERSION_PATH: &str = "/version";

/// Server-side response shape for `GET /api/v1/version`. Mirrors
/// mokosh-server's `VersionInfo`; only `version` is consumed here (the
/// remaining build-info fields are rendered by the System Status page
/// through its own struct), and unlisted fields are dropped at
/// deserialise time.
#[derive(Clone, Debug, Deserialize)]
pub struct ServerVersionResponse {
    /// The semver mokosh-server is currently running, normalised to
    /// plain semver without the leading `v`.
    #[serde(deserialize_with = "deserialize_semver")]
    pub version: String,
}

/// One running-vs-latest pair. Both fields are normalised to plain
/// semver without the leading `v`. The custom deserialiser handles
/// the case where the server (or its registry source) emits
/// `"v0.2.0"` so a pure string-equality check still matches the
/// SPA's `CARGO_PKG_VERSION`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct VersionPair {
    #[serde(deserialize_with = "deserialize_semver")]
    pub running: String,
    /// `None` until the registry poll has resolved.
    #[serde(default, deserialize_with = "deserialize_optional_semver")]
    pub latest: Option<String>,
}

impl VersionPair {
    /// True only when `latest` is a newer **release line** than `running`
    /// - a strictly greater `(major, minor)`, patch ignored.
    ///
    /// Skew is judged at minor granularity because mokosh-www and
    /// mokosh-server ship patch hotfixes independently (a www-only 0.7.2
    /// against a 0.7.0 API is expected and fine), so only a differing
    /// major/minor is a real mismatch. Returns false when the two share a
    /// release line (any patch delta), when `latest` is older, absent, or
    /// when either side fails to parse. MAPPS-370, MAPPS-372.
    pub fn update_available(&self) -> bool {
        match (
            parse_semver(&self.running),
            self.latest.as_deref().and_then(parse_semver),
        ) {
            (Some(running), Some(latest)) => major_minor(latest) > major_minor(running),
            _ => false,
        }
    }

    /// True only when `running` is a newer release line than `latest`
    /// (strictly greater `(major, minor)`, patch ignored) - the inverse of
    /// [`update_available`]. For the client pair this means the loaded
    /// bundle is a minor ahead of the server it talks to: the server image
    /// lags and should be upgraded. Same minor-granularity, fail-closed
    /// rules, so the two are mutually exclusive. MAPPS-370, MAPPS-372.
    pub fn running_ahead(&self) -> bool {
        match (
            parse_semver(&self.running),
            self.latest.as_deref().and_then(parse_semver),
        ) {
            (Some(running), Some(latest)) => major_minor(running) > major_minor(latest),
            _ => false,
        }
    }
}

/// Full "what's running vs what's available" picture, assembled by
/// pairing the server response with the SPA's own compile-time
/// `CARGO_PKG_VERSION`.
#[derive(Clone, Debug)]
pub struct SystemVersion {
    pub server: VersionPair,
    pub client: VersionPair,
}

/// Fetch the server's running version and pair it with the SPA's own
/// compile-time version to detect a stale client bundle.
#[cfg(feature = "web")]
pub async fn get_version() -> Result<SystemVersion, String> {
    let response =
        crate::hooks::fetch::api::get_authed::<ServerVersionResponse>(VERSION_PATH).await?;
    let server_running = response.version;
    Ok(SystemVersion {
        // This endpoint reports only what the server is running, not a
        // registry "latest available" tag, so there is no server-side
        // update signal to surface.
        server: VersionPair {
            running: server_running.clone(),
            latest: None,
        },
        // The matching client bundle tracks the server's minor release
        // line (the two share a major.minor; patch hotfixes ship
        // independently), so treat the server's version as the client's
        // "latest": a bundle on an older minor is stale and flags an
        // update. MAPPS-372.
        client: VersionPair {
            running: env!("CARGO_PKG_VERSION").to_string(),
            latest: Some(server_running),
        },
    })
}

/// Strip a single leading `v` so OCI tag names (`v0.2.0`) and Cargo
/// semver (`0.2.0`) compare equal.
fn normalise_semver(s: String) -> String {
    if let Some(rest) = s.strip_prefix('v') {
        rest.to_string()
    } else {
        s
    }
}

/// Parse a normalised `MAJOR.MINOR.PATCH` release string into a
/// comparable tuple. Returns `None` unless it is exactly three
/// dot-separated non-negative integers: our release tags are plain
/// `X.Y.Z`, so anything else (two components, a git-describe suffix, a
/// non-numeric field) is treated as unparseable, which
/// [`VersionPair::update_available`] reads as "no update" rather than
/// risking a wrong prompt. Input is already `v`-stripped by the
/// deserialiser.
fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// The `(major, minor)` release line of a parsed semver. Skew between the API
/// and the SPA is judged at this granularity: the two images ship patch
/// hotfixes independently, so a differing patch is not a mismatch - only a
/// differing major/minor is. MAPPS-372.
fn major_minor((major, minor, _patch): (u64, u64, u64)) -> (u64, u64) {
    (major, minor)
}

fn deserialize_semver<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(normalise_semver(s))
}

fn deserialize_optional_semver<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.map(normalise_semver))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_strips_v_prefix() {
        assert_eq!(normalise_semver("v0.2.0".to_string()), "0.2.0");
        assert_eq!(normalise_semver("0.2.0".to_string()), "0.2.0");
        // Single leading `v` only - "vv1.0" stays "v1.0" so a
        // malformed value still flags as different from the SPA's
        // clean CARGO_PKG_VERSION instead of silently equalling it.
        assert_eq!(normalise_semver("vv1.0".to_string()), "v1.0");
    }

    #[test]
    fn update_available_uses_minor_ordering() {
        let pair = |running: &str, latest: Option<&str>| VersionPair {
            running: running.to_string(),
            latest: latest.map(str::to_string),
        };

        // Same release line, any patch delta in either direction: no update.
        assert!(!pair("0.7.0", Some("0.7.0")).update_available());
        assert!(!pair("0.7.0", Some("0.7.9")).update_available());
        // MAPPS-372: a patch-newer server is not an update (a 0.7.2 client on a
        // 0.7.0 API is the same 0.7 line).
        assert!(!pair("0.7.2", Some("0.7.0")).update_available());
        // Newer minor on the server: client behind -> update.
        assert!(pair("0.7.5", Some("0.8.0")).update_available());
        // Older minor on the server: not an update (that is server-behind).
        assert!(!pair("0.8.0", Some("0.7.9")).update_available());
        // Multi-digit minor orders numerically: 0.10 > 0.7.
        assert!(pair("0.7.0", Some("0.10.0")).update_available());
        // Unknown latest / unparseable either side: fail closed, no update.
        assert!(!pair("0.2.0", None).update_available());
        assert!(!pair("0.2.0", Some("not-a-version")).update_available());
        assert!(!pair("0.2", Some("0.3.0")).update_available());
    }

    #[test]
    fn running_ahead_detects_minor_ahead_of_server() {
        let pair = |running: &str, latest: Option<&str>| VersionPair {
            running: running.to_string(),
            latest: latest.map(str::to_string),
        };

        // Bundle a minor ahead of the server (server behind): true.
        assert!(pair("0.8.0", Some("0.7.0")).running_ahead());
        // Multi-digit minor orders numerically (0.10 > 0.7).
        assert!(pair("0.10.0", Some("0.7.9")).running_ahead());
        // MAPPS-372: a patch ahead within the same minor is NOT server-behind
        // (a 0.7.2 client on a 0.7.0 API is fine).
        assert!(!pair("0.7.2", Some("0.7.0")).running_ahead());
        assert!(!pair("0.7.10", Some("0.7.9")).running_ahead());
        // Bundle on an older or equal minor: false.
        assert!(!pair("0.7.0", Some("0.8.0")).running_ahead());
        assert!(!pair("0.7.0", Some("0.7.0")).running_ahead());
        // Unknown / malformed latest: fail closed.
        assert!(!pair("0.8.0", None).running_ahead());
        assert!(!pair("0.8.0", Some("not-a-version")).running_ahead());

        // `running_ahead` and `update_available` are mutually exclusive, and a
        // same-minor patch delta triggers neither.
        let server_behind = pair("0.8.0", Some("0.7.0"));
        assert!(server_behind.running_ahead() && !server_behind.update_available());
        let client_behind = pair("0.7.0", Some("0.8.0"));
        assert!(client_behind.update_available() && !client_behind.running_ahead());
        let same_line = pair("0.7.2", Some("0.7.0"));
        assert!(!same_line.running_ahead() && !same_line.update_available());
    }

    #[test]
    fn parse_semver_accepts_plain_xyz_only() {
        assert_eq!(parse_semver("0.7.10"), Some((0, 7, 10)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.7"), None); // too few components
        assert_eq!(parse_semver("0.7.1.2"), None); // too many components
        assert_eq!(parse_semver("0.7.x"), None); // non-numeric field
        assert_eq!(parse_semver("v0.7.1"), None); // not yet v-stripped
        assert_eq!(parse_semver(""), None);
    }

    #[test]
    fn deserialise_normalises_v_prefix() {
        let pair: VersionPair =
            serde_json::from_str(r#"{"running": "v0.2.0", "latest": "v0.3.0"}"#).unwrap();
        assert_eq!(pair.running, "0.2.0");
        assert_eq!(pair.latest, Some("0.3.0".to_string()));
    }

    #[test]
    fn server_version_response_parses_real_endpoint_shape() {
        // The live `GET /api/v1/version` body carries extra build-info
        // fields the banner does not need; they must be dropped, and a
        // `v`-prefixed tag normalised, so the result compares equal to
        // the SPA's clean `CARGO_PKG_VERSION`.
        let resp: ServerVersionResponse = serde_json::from_str(
            r#"{"name": "mokosh-server", "version": "v0.2.0", "git_describe": "v0.2.0-3-gabc1234", "git_hash": "abc1234", "build_date": "2026-06-14"}"#,
        )
        .unwrap();
        assert_eq!(resp.version, "0.2.0");
    }
}

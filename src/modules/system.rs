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
    /// True when `latest` is known and differs from `running`. Both
    /// sides have already been normalised by the deserialiser so a
    /// plain string compare is correct.
    pub fn update_available(&self) -> bool {
        self.latest
            .as_ref()
            .is_some_and(|latest| latest != &self.running)
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
        // The matching client bundle should be on the same version the
        // server runs (the two images are released together), so treat
        // the server's version as the client's "latest": a bundle that
        // differs is stale and flags an update.
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
    fn update_available_pure_equality() {
        let pair = VersionPair {
            running: "0.2.0".to_string(),
            latest: Some("0.2.0".to_string()),
        };
        assert!(!pair.update_available());

        let pair = VersionPair {
            running: "0.2.0".to_string(),
            latest: Some("0.3.0".to_string()),
        };
        assert!(pair.update_available());

        let pair = VersionPair {
            running: "0.2.0".to_string(),
            latest: None,
        };
        assert!(!pair.update_available());
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

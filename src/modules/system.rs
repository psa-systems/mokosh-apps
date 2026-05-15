//! System-level introspection: version + update-available.
//!
//! Mirrors the planned `GET /api/v1/system/version` endpoint on
//! mokosh-server. The server polls the OCI registry on a timer and
//! caches the latest tag for both the server image and the SPA
//! image; this client deserialises the response and pairs the
//! `client.latest` with the SPA's own compile-time version (the
//! server cannot know what bundle a given browser actually has
//! loaded - the user might be on a stale cached copy).
//!
//! Progressive enablement: until the endpoint ships on mokosh-server,
//! [`get_version`] returns `Err` and callers (the update banner)
//! render nothing. There is no hard dependency in this direction.

use serde::{Deserialize, Deserializer};

/// API path for the version endpoint. Resolved against the API base
/// URL by `crate::hooks::fetch::api`.
pub const VERSION_PATH: &str = "/system/version";

/// Server-side response shape for `GET /api/v1/system/version`.
/// `server.running` is the version mokosh-server is itself executing;
/// `server.latest` and `client_latest` come from the registry poll.
#[derive(Clone, Debug, Deserialize)]
pub struct SystemVersionResponse {
    pub server: VersionPair,
    /// Latest published mokosh-www tag. `None` until the registry
    /// poll succeeds at least once (cold cache, registry unreachable).
    #[serde(default, deserialize_with = "deserialize_optional_semver")]
    pub client_latest: Option<String>,
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

/// Fetch the server's view of running + latest versions, then combine
/// with the SPA's own running version (compile-time).
#[cfg(feature = "web")]
pub async fn get_version() -> Result<SystemVersion, String> {
    let response =
        crate::hooks::fetch::api::get_authed::<SystemVersionResponse>(VERSION_PATH).await?;
    Ok(SystemVersion {
        server: response.server,
        client: VersionPair {
            running: env!("CARGO_PKG_VERSION").to_string(),
            latest: response.client_latest,
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
}

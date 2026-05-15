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

use serde::Deserialize;

/// Server-side response shape for `GET /api/v1/system/version`.
/// `server.running` is the version mokosh-server is itself executing;
/// `server.latest` and `client_latest` come from the registry poll.
#[derive(Clone, Debug, Deserialize)]
pub struct SystemVersionResponse {
    pub server: VersionPair,
    /// Latest published mokosh-www tag. `None` until the registry
    /// poll succeeds at least once (cold cache, registry unreachable).
    pub client_latest: Option<String>,
}

/// One running-vs-latest pair.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct VersionPair {
    pub running: String,
    /// `None` until the registry poll has resolved.
    pub latest: Option<String>,
}

impl VersionPair {
    /// True when `latest` is known and differs from `running`. Pure
    /// string equality - the server emits clean semver from
    /// `git describe`, so a semver-aware compare adds nothing.
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
    let response = crate::hooks::fetch::api::get_authed::<SystemVersionResponse>(
        "/system/version",
    )
    .await?;
    Ok(SystemVersion {
        server: response.server,
        client: VersionPair {
            running: env!("CARGO_PKG_VERSION").to_string(),
            latest: response.client_latest,
        },
    })
}

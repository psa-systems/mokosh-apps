//! Build-time version metadata. Populated by `build.rs` via
//! `cargo:rustc-env`. Surfaced in the UI for build troubleshooting.
//!
//! See `docs/versioning.md` for the full version-sourcing and
//! update-target story (staging vs production).

/// Canonical released semver, from Cargo.toml's `version` (via
/// `CARGO_PKG_VERSION`). Bumped as the first step of `just create-release`,
/// so it is correct the moment a release commit exists - it does NOT wait
/// for the `vX.Y.Z` git tag, which is applied post-merge (MAPPS-200).
pub const VERSION: &str = env!("APP_GIT_TAG");
/// Exact build commit (CI `GIT_SHA`, or `git rev-parse` locally). Carries
/// the build provenance the version string intentionally does not.
pub const GIT_HASH: &str = env!("APP_GIT_HASH");
pub const BUILD_DATE: &str = env!("APP_BUILD_DATE");

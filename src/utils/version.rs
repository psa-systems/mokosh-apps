//! Build-time version metadata. Populated by `build.rs` via
//! `cargo:rustc-env`. Surfaced in the UI for build troubleshooting.

pub const VERSION: &str = env!("APP_GIT_TAG");
pub const GIT_HASH: &str = env!("APP_GIT_HASH");
pub const BUILD_DATE: &str = env!("APP_BUILD_DATE");

pub fn version_string() -> String {
    format!("{VERSION} ({GIT_HASH}) built {BUILD_DATE}")
}

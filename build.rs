use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ADMIN_EMAIL");
    println!("cargo:rerun-if-env-changed=ADMIN_PASSWORD");
    // The OCI image build has no `.git` in its context, so `git rev-parse`
    // below fails and the footer would show `unknown`. CI injects the
    // commit via `GIT_SHA` (the builder stage exports it as an env), so
    // prefer that; fall back to git for local/dev builds.
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    // Re-run when HEAD or any ref moves so the embedded commit hash tracks
    // the current commit on local builds. Without these, cargo caches the
    // build script result and the displayed hash drifts from the actual
    // build. (The displayed version comes from CARGO_PKG_VERSION below, not
    // from git, so it does not depend on these.)
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");

    let git_hash = std::env::var("GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(12).collect::<String>())
        .or_else(|| run("git", &["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    // Displayed release version. Sourced from `CARGO_PKG_VERSION` (the
    // `version` field in Cargo.toml, kept in lockstep with package.json),
    // which `just create-release` bumps as the FIRST step of a release.
    //
    // We deliberately do NOT use `git describe --tags` here. The release
    // tag (`vX.Y.Z`) is only created AFTER the release PR merges, by
    // `.forgejo/workflows/create-release.yml`, so `git describe` lags one
    // release behind the actual `version`: a build cut right after the
    // 0.3.0 bump still resolves to `v0.2.0-N-gHASH` and the footer shows
    // "0.2" (MAPPS-200). The OCI build also strips `.git` (see
    // .dockerignore), so `git describe` cannot run there at all. The
    // commit hash above carries the exact build provenance; the tag here
    // carries the canonical released semver. This also keeps the footer
    // consistent with the update banner, which already compares against
    // `CARGO_PKG_VERSION` (see src/modules/system.rs).
    let git_tag = std::env::var("CARGO_PKG_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let build_date =
        run("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=APP_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=APP_GIT_TAG={git_tag}");
    println!("cargo:rustc-env=APP_BUILD_DATE={build_date}");
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

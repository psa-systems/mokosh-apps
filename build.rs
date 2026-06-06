use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ADMIN_EMAIL");
    println!("cargo:rerun-if-env-changed=ADMIN_PASSWORD");
    // The OCI image build has no `.git` in its context, so `git rev-parse`
    // below fails and the footer would show `unknown`. CI injects the
    // commit via `GIT_SHA` (the builder stage exports it as an env), so
    // prefer that; fall back to git for local/dev builds.
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    // Re-run when HEAD or any ref moves so the embedded hash/tag track the
    // current commit. Without these, cargo caches the build script result
    // and the displayed version drifts from the actual build.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");

    let git_hash = std::env::var("GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(12).collect::<String>())
        .or_else(|| run("git", &["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    let git_tag = run("git", &["describe", "--tags", "--always", "--dirty"])
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
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

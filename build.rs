use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ADMIN_EMAIL");
    println!("cargo:rerun-if-env-changed=ADMIN_PASSWORD");
    // Re-run when HEAD or any ref moves so the embedded hash/tag track the
    // current commit. Without these, cargo caches the build script result
    // and the displayed version drifts from the actual build.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/index");

    let git_hash =
        run("git", &["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".into());
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

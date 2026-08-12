# General Task Runner

# Image used by the pre-commit hook. Matches ci-build/Dockerfile so `just pre-commit` and the Forgejo `check.yml` job run a toolchain compatible with the rust-builder-glibc image the client is built against.
dev_image := "ghcr.io/niceguyit/rust-builder-glibc:v1.0.1-rust1.94-trixie"

# List available recipes
default:
    @just --list

# -- Hooks ------------------------------------------------------------------

# Install the git pre-commit hook (run once per fresh clone). Writes a stub at .git/hooks/pre-commit that execs `just pre-commit`. Bypass with `git commit --no-verify`.
[group: 'hooks']
install-hooks:
    #!/usr/bin/env nu
    let hook = ".git/hooks/pre-commit"
    # Remove first so a leftover symlink from an older install does not get
    # written through to its target file. `try` swallows the not-found case.
    try { rm $hook }
    "#!/usr/bin/env sh\nexec just pre-commit\n" | save $hook
    ^chmod +x $hook
    print $"Wrote ($hook) -> just pre-commit"

# Run the same checks as .forgejo/workflows/check.yml inside the rust-builder-glibc image so the toolchain matches CI.
# Depends on css-build because src/main.rs uses asset!("/assets/styles.css"), which requires the Tailwind output to exist at compile time. assets/styles.css is gitignored, so a clean clone must build it before clippy/check run.
[group: 'hooks']
pre-commit: css-build
    #!/usr/bin/env nu
    let img = "{{ dev_image }}"
    # Share the cargo target cache with `just dev`/compose. compose.yml
    # names this volume `dev-mokosh-apps-target-${USER}`; matching it here
    # (per-user, not the old shared `dev-mokosh-apps-cargo-target`) means
    # the pre-commit build reuses the dev container's compiled artifacts.
    let user_name = (^whoami | str trim)
    let target_vol = $"dev-mokosh-apps-target-($user_name)"
    print "\n[pre-commit] cargo fmt --all --check"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume $"($target_vol):/cargo-target" --env CARGO_TARGET_DIR=/cargo-target --volume dev-mokosh-apps-cargo-registry:/usr/local/cargo/registry $img cargo fmt --all --check
    print "\n[pre-commit] cargo clippy --all-targets -- -D warnings"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume $"($target_vol):/cargo-target" --env CARGO_TARGET_DIR=/cargo-target --volume dev-mokosh-apps-cargo-registry:/usr/local/cargo/registry $img cargo clippy --all-targets -- -D warnings
    print "\n[pre-commit] cargo check --target wasm32-unknown-unknown"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume $"($target_vol):/cargo-target" --env CARGO_TARGET_DIR=/cargo-target --volume dev-mokosh-apps-cargo-registry:/usr/local/cargo/registry $img cargo check --target wasm32-unknown-unknown
    print "\n[pre-commit] cargo test --lib"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume $"($target_vol):/cargo-target" --env CARGO_TARGET_DIR=/cargo-target --volume dev-mokosh-apps-cargo-registry:/usr/local/cargo/registry $img cargo test --lib
    print "\n[pre-commit] all checks passed"

# -- Checks ----------------------------------------------------------------------

# Umbrella check: build + clippy + fmt + docker builder stage.
[group: 'check']
check: check-web check-clippy check-fmt check-theme-tokens check-defined-colors check-runner-labels check-cancel-routes check-auth-error-prose

# Check web/WASM compilation
[group: 'check']
check-web:
    cargo check --target wasm32-unknown-unknown

# MAPPS-259: fail on hardcoded neutral/brand color classes (use tokens)
[group: 'check']
check-theme-tokens:
    bash scripts/check-theme-tokens.sh

# MAPPS-433: fail on colour classes input.css does not define (they render as nothing)
[group: 'check']
check-defined-colors:
    bash scripts/check-defined-colors.sh

# MAPPS-398: keep check.yml on the dev runner label and free of run-time package installs
[group: 'check']
check-runner-labels:
    bash scripts/check-runner-labels.sh

# MAPPS-423: keep shared create/edit forms cancelling to the record, and keep the pointer-cursor base rule in input.css
[group: 'check']
check-cancel-routes:
    bash scripts/check-cancel-routes.sh

# MAPPS-432: keep /auth/callback classifying on the FlowError variant, not on error prose
[group: 'check']
check-auth-error-prose:
    bash scripts/check-auth-error-prose.sh

# Run clippy lints
[group: 'check']
check-clippy:
    cargo clippy --all-targets -- -D warnings

# Check formatting
[group: 'check']
check-fmt:
    cargo fmt --all --check

# Install JS dependencies
[private]
[group: 'hooks']
ensure-npm:
    @test -d node_modules || bun install

# Build Tailwind CSS once
[group: 'css']
css-build: ensure-npm
    bun x @tailwindcss/cli --input input.css --output assets/styles.css

# Watch and rebuild Tailwind CSS on changes
[group: 'css']
css-watch: ensure-npm
    bun x @tailwindcss/cli --input input.css --output assets/styles.css --watch

# Start the dx dev server in Docker, bound to the host LAN IP
[group: 'dev']
dev:
    #!/usr/bin/env nu
    # Pick the first private (RFC1918) IPv4 on a physical/bridge interface.
    # Match common modern names too (ens3/enp* predictable names, wlan*)
    # instead of only eth0/br0. Bind only to a private LAN address, never a
    # public interface: br0 on this host is public, and binding dev services
    # to it exposed them to the internet (a sibling stack's postgres was
    # compromised by the PG_MEM botnet that way). Fall back to loopback with a
    # warning when no private address is present, so dx serve can never be
    # published on a public interface.
    let candidates = (sys net | where name =~ '^(en|eth|br|wlan)' | get ip | flatten | where protocol == 'ipv4' and loop == false)
    let private = ($candidates | where (($it.address | str starts-with '10.') or ($it.address =~ '^172\.(1[6-9]|2[0-9]|3[01])\.') or ($it.address | str starts-with '192.168.')))
    let host_ip = (if ($private | is-empty) { '127.0.0.1' } else { $private | get 0.address })
    if $host_ip == '127.0.0.1' {
        print 'WARNING: no private (RFC1918) LAN IPv4 found on an en*/eth*/br*/wlan* interface; binding dx serve to loopback (127.0.0.1) only.'
    }
    let uid = (^id --user | str trim)
    let gid = (^id --group | str trim)
    let user_name = (^whoami | str trim)
    print $"Binding dx serve to ($host_ip):4301 as ($user_name) \(uid ($uid):($gid)\)"
    with-env { HOST_IP: $host_ip, HOST_UID: $uid, HOST_GID: $gid, USER: $user_name } { docker compose up --build }

# Per-developer Traefik-routed instance for SSO testing.
#   App: https://{USER}-mokosh.a8n.run
# Run `just dev-sso` here AND in mokosh-server. The overlay requires
# MOKOSH_OIDC_CLIENT_ID set in .env (or the shell), which comes from
# `just register-client` in mokosh-server. The compose file fails loud
# if it's missing.
[doc("Start the SSO dev stack (Traefik-routed at *.a8n.run)")]
[group: 'dev']
dev-sso:
    #!/usr/bin/env nu
    let uid = (^id --user | str trim)
    let gid = (^id --group | str trim)
    let user_name = (^whoami | str trim)
    # The base compose.yml declares the per-developer private network
    # `dev-mokosh-private-${USER}` as `external: true`, so compose will
    # NOT create it. Ensure it exists (idempotent: docker network
    # inspect returns 0 when present, otherwise create).
    let net = $"dev-mokosh-private-($user_name)"
    if (do { ^docker network inspect $net } | complete | get exit_code) != 0 {
        ^docker network create $net out> /dev/null
    }
    # HOST_IP is referenced by the base compose.yml's port mapping; the
    # overlay !resets it but the variable still has to substitute, so
    # we set a harmless placeholder. --detach so the URL print runs.
    with-env { HOST_IP: "127.0.0.1", HOST_UID: $uid, HOST_GID: $gid, USER: $user_name } {
        docker compose --file compose.yml --file compose.dev-sso.yml up --build --detach
    }
    print ""
    print $"Mokosh client \(SPA\): https://($user_name)-mokosh.a8n.run"

# Stop everything this repo runs (both LAN-IP and SSO modes),
# regardless of which `just dev*` you started with. Volumes preserved.
# `--remove-orphans` cleans up the dx server container from either file
# layout. HOST_IP is set defensively so the base compose.yml's port
# substitution does not warn during teardown.
[doc("Stop the dev stack (LAN-IP and SSO modes). Volumes preserved.")]
[group: 'dev']
down:
    #!/usr/bin/env nu
    # Same external-network defensiveness as `dev-sso`: compose refuses
    # to even validate the file if the declared external network is
    # missing.
    let user_name = (^whoami | str trim)
    let net = $"dev-mokosh-private-($user_name)"
    if (do { ^docker network inspect $net } | complete | get exit_code) != 0 {
        ^docker network create $net out> /dev/null
    }
    # MOKOSH_OIDC_CLIENT_ID is a `${...:?}` required var in compose.yml; supply a
    # harmless placeholder so teardown interpolates even before it is set in .env.
    with-env { HOST_IP: "127.0.0.1", USER: $user_name, MOKOSH_OIDC_CLIENT_ID: "teardown-placeholder" } {
        docker compose --file compose.yml --file compose.dev-sso.yml down --remove-orphans
    }

# Bring the SSO dev stack down and back up. Useful after pulling a
# code change or editing compose env vars: `down` waits for containers
# to fully terminate before `dev-sso` starts the fresh ones, so the
# rebuild picks up the new state. `down` is synchronous (docker
# compose down blocks until removal completes) and `dev-sso` uses
# `--detach`, so this returns once the new stack is up.
[doc("Stop the dev stack and start dev-sso fresh.")]
[group: 'dev']
restart: down dev-sso

# Format code
[group: 'format']
fmt:
    cargo fmt --all

# Run tests
[group: 'test']
test:
    cargo test

# Build release WASM bundle
[group: 'build']
build: css-build
    dx build --release

# Build OCI image for validation
[group: 'check']
check-docker:
    docker buildx build --tag mokosh-apps:check --file oci-build/Dockerfile .

# Build OCI image
[group: 'build']
build-docker:
    docker buildx build --tag mokosh-apps:local --file oci-build/Dockerfile .

# -- Cleanup ------------------------------------------------------------------

# -- Release ---------------------------------------------------------------------

# Create a release: bump major (vx.0.0), minor (v0.x.0), or hotfix (v0.0.x), push the branch, and open the PR via fj.
# After the PR merges, the create-release workflow creates the tag and release automatically.
[group: 'release']
create-release bump:
    #!/usr/bin/env nu
    let bump = "{{ bump }}"

    # Abort if there are uncommitted changes
    let status = git status --porcelain | str trim
    if ($status | is-not-empty) {
        print $"(ansi red)Working tree is dirty. Please stash or commit your changes first.(ansi reset)"
        exit 1
    }

    # Switch to main if not already there
    let branch = git branch --show-current | str trim
    if $branch != "main" {
        print $"Switching from ($branch) to main..."
        git checkout main
    }

    # Pull latest changes
    git pull --rebase origin main

    let cargo_version = (open Cargo.toml | get package.version)
    let pkg_version = (open package.json | get version)
    if $cargo_version != $pkg_version {
        print $"(ansi red)Error: Cargo.toml v($cargo_version) does not match package.json v($pkg_version)(ansi reset)"
        exit 1
    }

    let current = ($cargo_version | split row "." | each { into int })
    let next = match $bump {
        "major" => [$"($current.0 + 1)" "0" "0"],
        "minor" => [$"($current.0)" $"($current.1 + 1)" "0"],
        "hotfix" => [$"($current.0)" $"($current.1)" $"($current.2 + 1)"],
        _ => { print $"(ansi red)Usage: just create-release <major|minor|hotfix>(ansi reset)"; exit 1 }
    }
    let bare = ($next | str join ".")
    let tag = $"v($bare)"
    let release_branch = $"release/($tag)"

    git checkout -b $release_branch
    # Bump only the `[package]` version line as text. `open Cargo.toml | update
    # package.version | to toml` round-trips through nu's TOML serializer, which
    # drops every comment, so it silently deleted the feature-flag / dependency
    # rationale on the v0.6.0 release. `str replace` (first match) targets the
    # package `version` line, which is the first `version = "..."` in the file.
    let old_version_line = $"version = \"($cargo_version)\""
    let new_version_line = $"version = \"($bare)\""
    open --raw Cargo.toml | str replace $old_version_line $new_version_line | save --force Cargo.toml
    open package.json | update version $bare | save --force package.json
    # MAPPS-371 (ports PMS-642): sync Cargo.lock to the bumped version so the
    # lock never drifts from Cargo.toml. Without this a `--locked` build fails,
    # every subsequent build re-dirties the lock (masking real lock changes in
    # diffs), and the dirty tree aborts the next `create-release`. Dev boxes
    # have no host cargo, so run the one cargo step in the rust-builder image
    # (the same invocation the `pre-commit` recipe uses). `--workspace` limits
    # the change to the workspace members' own versions - no transitive churn.
    let img = "{{ dev_image }}"
    let user_name = (^whoami | str trim)
    let target_vol = $"dev-mokosh-apps-target-($user_name)"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume $"($target_vol):/cargo-target" --env CARGO_TARGET_DIR=/cargo-target --volume dev-mokosh-apps-cargo-registry:/usr/local/cargo/registry $img cargo update --workspace
    git add Cargo.toml package.json Cargo.lock
    git commit --signoff --message $"Release ($tag)"

    # Push release branch
    git push --set-upstream origin $release_branch

    # Open the release PR via fj. Body lives in a tempfile so the
    # changelog can grow later without inline escaping pain.
    let body_file = (mktemp --tmpdir --suffix .md)
    [
        $"Automated release PR for ($tag)."
        ""
        $"After merge, `.forgejo/workflows/create-release.yml` tags and publishes ($tag) to the Generic Packages registry."
    ] | str join "\n" | save --force $body_file
    let fj_result = (^fj --host dev.a8n.run pr create $"Release ($tag)" --body-file $body_file | complete)
    rm $body_file
    if $fj_result.exit_code != 0 {
        print $"(ansi red)fj pr create failed(ansi reset)"
        print $fj_result.stderr
        exit 1
    }

    # `fj pr create` prints `created pull request #N: <title>` on success.
    # Parse the number out and build the PR URL from `origin` so the user
    # gets a clickable link instead of just the fj line.
    let pr_num = (
        $fj_result.stdout
        | str trim
        | parse --regex 'created pull request #(?P<num>\d+)'
        | get num.0?
    )
    let remote = (git remote get-url origin | str trim)
    let base_url = if ($remote | str starts-with "ssh://") {
        $remote | str replace "ssh://git@" "https://" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    } else {
        $remote | str replace --regex "git@([^:]+):" "https://$1/" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    }
    print $"(ansi green)Pushed ($release_branch)(ansi reset)"
    if ($pr_num | is-not-empty) {
        print $"PR: ($base_url)/pulls/($pr_num)"
    } else {
        # fj output format drifted; fall back to whatever it said.
        print $"fj output: ($fj_result.stdout | str trim)"
    }
    print $"After merging, the create-release workflow will tag and release ($tag) automatically."


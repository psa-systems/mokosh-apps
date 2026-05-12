# Mokosh Platform - Dioxus Client - Task Runner

# Image used by the pre-commit hook. Matches ci-build/Dockerfile so `just pre-commit` and the Forgejo `check.yml` job run a toolchain compatible with the rust-builder-glibc image the client is built against.
dev_image := "ghcr.io/niceguyit/rust-builder-glibc:v1.0.0-rust1.94-trixie"

# List available recipes
default:
    @just --list

# Install the git pre-commit hook (run once per fresh clone). Writes a stub at .git/hooks/pre-commit that execs `just pre-commit`. Bypass with `git commit --no-verify`.
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
pre-commit:
    #!/usr/bin/env nu
    let img = "{{ dev_image }}"
    print "\n[pre-commit] cargo fmt --all --check"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume dev-mokosh-clients-cargo-target:/build/target --volume dev-mokosh-clients-cargo-registry:/usr/local/cargo/registry $img cargo fmt --all --check
    print "\n[pre-commit] cargo clippy --all-targets -- -D warnings"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume dev-mokosh-clients-cargo-target:/build/target --volume dev-mokosh-clients-cargo-registry:/usr/local/cargo/registry $img cargo clippy --all-targets -- -D warnings
    print "\n[pre-commit] cargo check --target wasm32-unknown-unknown"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume dev-mokosh-clients-cargo-target:/build/target --volume dev-mokosh-clients-cargo-registry:/usr/local/cargo/registry $img cargo check --target wasm32-unknown-unknown
    print "\n[pre-commit] cargo test --lib"
    ^docker run --rm --volume $"($env.PWD):/build" --workdir /build --volume dev-mokosh-clients-cargo-target:/build/target --volume dev-mokosh-clients-cargo-registry:/usr/local/cargo/registry $img cargo test --lib
    print "\n[pre-commit] all checks passed"

# Install JS dependencies
[private]
ensure-npm:
    @test -d node_modules || bun install

# Build Tailwind CSS once
css-build: ensure-npm
    bun x @tailwindcss/cli --input input.css --output assets/styles.css

# Watch and rebuild Tailwind CSS on changes
css-watch: ensure-npm
    bun x @tailwindcss/cli --input input.css --output assets/styles.css --watch

# Start the dx dev server in Docker, bound to the host LAN IP
dev:
    #!/usr/bin/env nu
    let host_ip = (sys net | where name =~ 'eth0|br0' | get ip | flatten | where protocol == 'ipv4' and loop == false | get 0.address)
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
    with-env { HOST_IP: "127.0.0.1", USER: $user_name } {
        docker compose --file compose.yml --file compose.dev-sso.yml down --remove-orphans
    }

# Stop the SSO dev stack.
[doc("Stop the SSO dev stack")]
dev-sso-down:
    docker compose --file compose.yml --file compose.dev-sso.yml down

# Run all checks (web, clippy, fmt)
check: check-web check-clippy check-fmt

# Check web/WASM compilation
check-web:
    cargo check --target wasm32-unknown-unknown

# Run clippy lints
check-clippy:
    cargo clippy --all-targets

# Check formatting
check-fmt:
    cargo fmt --all --check

# Format code
fmt:
    cargo fmt --all

# Run tests
test:
    cargo test

# Build release WASM bundle
build: css-build
    dx build --release

# Build OCI image for validation
check-docker:
    docker buildx build --tag mokosh-client:check --file oci-build/Dockerfile .

# Build OCI image
build-docker:
    docker buildx build --tag mokosh-client:local --file oci-build/Dockerfile .

# Create a release: bump version, push branch, print PR link
create-release bump:
    #!/usr/bin/env nu
    let bump = "{{ bump }}"

    let status = git status --porcelain | str trim
    if ($status | is-not-empty) {
        print $"(ansi red)Working tree is dirty. Please stash or commit your changes first.(ansi reset)"
        exit 1
    }

    let branch = git branch --show-current | str trim
    if $branch != "main" {
        print $"Switching from ($branch) to main..."
        git checkout main
    }

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
    open Cargo.toml | update package.version $bare | to toml | collect | save --force Cargo.toml
    open package.json | update version $bare | save --force package.json
    git add Cargo.toml package.json
    git commit --signoff --message $"Release ($tag)"

    git push --set-upstream origin $release_branch

    let remote = git remote get-url origin
    let base_url = if ($remote | str starts-with "ssh://") {
        $remote | str replace "ssh://git@" "https://" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    } else {
        $remote | str replace --regex "git@([^:]+):" "https://$1/" | str replace "git.a8n.run" "dev.a8n.run" | str replace ".git" ""
    }
    print $"(ansi green)Pushed ($release_branch)(ansi reset)"
    print $"Create PR: ($base_url)/compare/main...($release_branch)"
    print $"After merging, the create-release workflow will tag and release ($tag) automatically."

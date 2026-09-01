# General Task Runner
#
# The hook, release and cleanup recipes come from the `common` submodule
# (MAPPS-452). Configure them with the variables below rather than by
# redefining a recipe; `just check-justfile` fails on a shadowed one. Run
# `git submodule update --init` in a fresh clone or the import will not resolve.

# Required: this file defines its own `default`, which collides with the imported one.
set allow-duplicate-recipes := true

# Names the cargo cache volumes the shared pre-commit uses (dev-mokosh-apps-cargo-*-$USER).
app := "mokosh-apps"

# No compose.dev.yml here, so the shared pre-commit runs the checks in a bare
# `docker run`. The image matches ci-build/Dockerfile so `just pre-commit` and
# the Forgejo `check.yml` job run a toolchain compatible with the
# rust-builder-glibc image the client is built against.
pre_commit_mode := "docker"
dev_image := "ghcr.io/niceguyit/rust-builder-glibc:v1.0.1-rust1.94-trixie"

# src/main.rs embeds assets/styles.css via asset!(), and that file is gitignored,
# so Tailwind has to run on the host before any cargo step in the container.
pre_commit_prepare := "css-build"

# Mirrors check-clippy and check.yml. The shared default is --all-features,
# which would turn on `desktop` (linking the system webview) and `single-tenant`
# alongside `multi-tenant`, neither of which this repo compiles anywhere else.
clippy_args := "--all-targets -- -D warnings"

# The clippy pass above is this repo's host-target typecheck, so the container
# compile step would only repeat it; the wasm pass is the one CI adds on top.
pre_commit_compile := "false"
wasm_check := "true"

# Mirrors check.yml's `cargo test --lib`: the tests live on the library target.
test_args := "--lib"

# package.json carries the same semver as the crate, so the release bumps both.
release_version_files := "package.json"

# compose.yml names the dev server's cargo target volume, which is outside the
# `dev-{{app}}-cargo-*` set dev-clean removes on its own.
dev_extra_volumes := "dev-mokosh-apps-target"

import 'common/common.just'

# List available recipes. Keep FIRST: just picks the default recipe by source order.
default:
    @just --list

# -- Checks ----------------------------------------------------------------------

# Umbrella check: build + clippy + fmt + docker builder stage.
[group: 'check']
check: check-ci-parity check-doc-links check-web check-desktop check-clippy check-fmt check-theme-tokens check-defined-colors check-runner-labels check-cancel-routes check-auth-error-prose check-confirm-destructive check-delete-result check-class-omissions check-kit-adoption check-ellipsis-glyph check-empty-state check-status-banner check-no-demo-rows check-email-affordance check-dev-sso-scheme check-sort-keys check-per-page-cap check-types-pin check-prose-layer check-field-value-binding check-hooks-before-return check-page-width

# Check web/WASM compilation
[group: 'check']
check-web:
    cargo check --target wasm32-unknown-unknown

# MAPPS-504: check the native desktop build. `--no-default-features` drops
# `web-renderer` so dioxus links the desktop renderer alone; `web` stays on
# because it is the app-runtime gate, not a browser gate (see Cargo.toml).
[group: 'check']
check-desktop:
    cargo check --no-default-features --features web,multi-tenant,desktop

# MAPPS-259: fail on hardcoded neutral/brand color classes (use tokens). MAPPS-444: and on a red/green text class with no dark: pair. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-theme-tokens:
    bash scripts/check-theme-tokens.sh --self-test
    bash scripts/check-theme-tokens.sh

# MAPPS-585: keep a shared form field's value on the `value:` attribute. As a textarea CHILD it is only the default value, so every toolbar transform died on the first keystroke. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-field-value-binding:
    bash scripts/check-field-value-binding.sh --self-test
    bash scripts/check-field-value-binding.sh

# MAPPS-624: the max-w-7xl cap lives on each AppShell route component in src/lib.rs, never back on the shell, so width stays a per-page choice. --self-test first, so a guard that stopped guarding fails loudly.
[doc("Fail if AppShell caps every page again, or a route declares no width (MAPPS-624).")]
[group: 'check']
check-page-width:
    bash scripts/check-page-width.sh --self-test
    bash scripts/check-page-width.sh

# MAPPS-602: a hook after an early return poisons the Dioxus runtime.
[doc("Fail if a component calls a hook after an early return (MAPPS-602).")]
[group: 'check']
check-hooks-before-return:
    bash scripts/check-hooks-before-return.sh --self-test
    bash scripts/check-hooks-before-return.sh

# MAPPS-584: keep the Markdown corrections in a cascade layer that outranks @tailwindcss/typography. In `@layer components` they lost to the plugin and shipped inert. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-prose-layer:
    bash scripts/check-prose-layer.sh --self-test
    bash scripts/check-prose-layer.sh

# MAPPS-433: fail on colour classes input.css does not define (they render as nothing). MAPPS-437: --self-test first, so a guard that stopped guarding fails loudly instead of reporting clean.
[group: 'check']
check-defined-colors:
    bash scripts/check-defined-colors.sh --self-test
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

# MAPPS-436: keep every destructive mutation behind ConfirmDialog, never straight from a button onclick
[group: 'check']
check-confirm-destructive:
    bash scripts/check-confirm-destructive.sh --self-test
    bash scripts/check-confirm-destructive.sh

# MAPPS-574: keep a destructive delete reporting the server's refusal, never reducing it to .is_ok()
[group: 'check']
check-delete-result:
    bash scripts/check-delete-result.sh --self-test
    bash scripts/check-delete-result.sh

# MAPPS-446: keep headings declaring a weight, two-up form grids on sm:, and table name cells naming their colour token
[group: 'check']
check-class-omissions:
    bash scripts/check-class-omissions.sh

# MAPPS-440: keep the DaisyUI classes out, the auth shells on AuthLayout, and the file-input recipe in FileField. MAPPS-483: and every floating dropdown on the shared .dropdown-panel surface. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-kit-adoption:
    bash scripts/check-kit-adoption.sh --self-test
    bash scripts/check-kit-adoption.sh

# MAPPS-445: keep rendered text on the single ellipsis character (U+2026), never three ASCII periods. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-ellipsis-glyph:
    bash scripts/check-ellipsis-glyph.sh --self-test
    bash scripts/check-ellipsis-glyph.sh

# MAPPS-442: keep every settings list page on the rich three-part EmptyState (title, description, "New <thing>" button), never the bare-message mode. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-empty-state:
    bash scripts/check-empty-state.sh --self-test
    bash scripts/check-empty-state.sh

# MAPPS-439: keep every inline status banner on StatusBanner, and keep all four BannerTone recipes in components/error_banner.rs. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-status-banner:
    bash scripts/check-status-banner.sh --self-test
    bash scripts/check-status-banner.sh

# MAPPS-438: keep every list page rendering only rows the backend returned, never a seeded demo fallback. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-no-demo-rows:
    bash scripts/check-no-demo-rows.sh --self-test
    bash scripts/check-no-demo-rows.sh

# MAPPS-482: keep every action that makes the server email someone marked with MailIcon and offering EmailPreview. The path list is maintained by hand (see docs/email-actions.md). --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-email-affordance:
    bash scripts/check-email-affordance.sh --self-test
    bash scripts/check-email-affordance.sh

# MAPPS-530: keep every absolute URL in the TLS-routed dev-SSO overlay on https, so the SPA never fetches mixed content. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-dev-sso-scheme:
    bash scripts/check-dev-sso-scheme.sh --self-test
    bash scripts/check-dev-sso-scheme.sh

# MAPPS-527: no page hardcodes a `?sort=` value; every fragment is a const in src/utils/sort_keys.rs that a test checks against the server's allow-list. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-sort-keys:
    bash scripts/check-sort-keys.sh --self-test
    bash scripts/check-sort-keys.sh

# MAPPS-528: no call site asks for a page at or above the server's per_page cap; whole-collection reads go through the paging helpers in src/hooks/fetch.rs. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-per-page-cap:
    bash scripts/check-per-page-cap.sh --self-test
    bash scripts/check-per-page-cap.sh

# MAPPS-545: every relative Markdown link resolves to a path that exists. MAPPS-540 took docs/ from 49 broken links to zero; all 49 came from a file move that left the links inside it one directory short, and a broken link fails silently - the reader lands on nothing and concludes the docs are abandoned. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-doc-links:
    bash scripts/check-doc-links.sh --self-test
    bash scripts/check-doc-links.sh

# MAPPS-534: `.forgejo/workflows/check.yml` says it mirrors this recipe, and it had drifted in four places with nothing able to notice. Fails if a command any recipe in the `check` list runs has no `run:` line in the workflow. Compares command lines, not recipe names, because two of those four drifts were steps present but invoked without their --self-test. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-ci-parity:
    bash scripts/check-ci-parity.sh --self-test
    bash scripts/check-ci-parity.sh

# MAPPS-525: fail when a shared-DTO change on mokosh-server main would otherwise sit unnoticed behind a pin nobody advances. MAPPS-537 narrowed it to moves where crates/mokosh-types actually differs, so another repository merging anything at all no longer turns this red. Needs network. --self-test first, so a guard that stopped guarding fails loudly.
[group: 'check']
check-types-pin:
    bash scripts/check-types-pin.sh --self-test
    bash scripts/check-types-pin.sh

# MAPPS-537: the pre-MAPPS-537 rule, where any lock move is a finding. What `types-pin-drift.yml` runs weekly, kept here so the catch-up distance can be read on demand. Not part of `just check`: it is deliberately allowed to be red.
[group: 'check']
check-types-pin-strict:
    bash scripts/check-types-pin.sh --self-test
    bash scripts/check-types-pin.sh --strict

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

# Build release WASM bundle. PMS-884: `--features web-renderer` because dx
# substitutes its own feature list for this crate's defaults and the feature it
# picks (`web`) enables no renderer; without it the bundle builds and then
# panics on launch.
[group: 'build']
build: css-build
    dx build --release --features web-renderer

# MAPPS-504: run the desktop app against a local build.
[group: 'dev']
desktop-run: css-build
    dx serve --platform desktop --no-default-features --features web,multi-tenant,desktop

# MAPPS-504: build the desktop binary without launching it.
[group: 'build']
desktop-build: css-build
    dx build --platform desktop --no-default-features --features web,multi-tenant,desktop

# MAPPS-504: produce an installable desktop bundle for this platform.
[group: 'build']
desktop-bundle: css-build
    dx bundle --release --platform desktop --no-default-features --features web,multi-tenant,desktop

# MAPPS-477: prove a no-JS link-preview crawler receives the og:/twitter: tags. Runs the real entrypoint + Caddyfile in a container and fetches it with curl, so it needs docker and is not part of `just check` (which compiles on a runner with no docker), like check-docker below.
[group: 'check']
check-link-preview:
    bash scripts/check-link-preview.sh --self-test
    bash scripts/check-link-preview.sh

# Build OCI image for validation
[group: 'check']
check-docker:
    docker buildx build --tag mokosh-apps:check --file oci-build/Dockerfile .

# Build OCI image
[group: 'build']
build-docker:
    docker buildx build --tag mokosh-apps:local --file oci-build/Dockerfile .


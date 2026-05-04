# Mokosh Client

Cross-platform Dioxus client for the Mokosh Platform. Compiles to WebAssembly and runs in the browser.

## Tech stack

- **Dioxus 0.7** (Rust UI framework, `web` + `router` features)
- **wasm32-unknown-unknown** target
- **Tailwind CSS v4** via Bun (`bun x @tailwindcss/cli`)
- **just** task runner
- **Docker Compose** for the dev server
- **Caddy** to serve the built bundle in production (see `oci-build/`)

## Prerequisites

Install on the host:

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Bun](https://bun.sh/)
- [just](https://github.com/casey/just)
- [Docker](https://docs.docker.com/engine/install/) with the Compose plugin
- [Nushell 0.112.2](https://www.nushell.sh/) (used by the `dev` and `create-release` recipes)

The dev server itself runs inside Docker, so the host does not need `dioxus-cli` installed.

## Quick start

```nu
just dev
```

This recipe:

1. Detects the host LAN IP from `br0` or `eth0` via `sys net`.
2. Exports it as `HOST_IP`.
3. Runs `docker compose up --build`, which starts `dev-mokosh-client` and binds port `4301` to `${HOST_IP}:4301`.

Open the printed URL (e.g. `http://172.16.100.120:4301`). Hot reload is enabled.

Binding to the LAN IP rather than `0.0.0.0` keeps the dev server off the public internet when the host is a VPS.

### Reaching the dev server from another container

Either:

- Use the host LAN IP: `http://${HOST_IP}:4301`.
- Or join the `dev-mokosh-private` Docker network and use `http://dev-mokosh-client:4301`.

## Justfile recipes

```nu
just                  # list recipes
just dev              # run the dev server in Docker (see above)
just css-build        # one-shot Tailwind build
just css-watch        # Tailwind watch mode
just check            # check-web, check-clippy, check-fmt
just check-web        # cargo check --target wasm32-unknown-unknown
just check-clippy     # cargo clippy --all-targets
just check-fmt        # cargo fmt --all --check
just fmt              # cargo fmt --all
just test             # cargo test
just build            # release WASM bundle (dx build --release)
just check-docker     # build the production OCI image as :check
just build-docker     # build the production OCI image as :local
just create-release   # cut a release branch and bump versions (see below)
```

## Project layout

```
src/
  main.rs           # WASM entry point
  lib.rs
  components/       # button, card, form, icons, layout, modal, table
  hooks/
  modules/          # auth, contacts, tenants, tickets
  pages/            # admin, billing, calendar, contracts, dashboard, ...
  utils/

assets/             # built CSS and static assets (styles.css is generated)
input.css           # Tailwind entry; compiled into assets/styles.css
index.html          # HTML template for the WASM bundle

Cargo.toml          # Rust workspace + crate config
Dioxus.toml         # dx serve / dx build config (port 4300, name, bundle)
package.json        # Bun deps (Tailwind v4)
justfile            # task runner
compose.yml         # dev server stack
Dockerfile          # dev image (dx serve with hot reload)
oci-build/          # production image (Caddy serving the built bundle)
```

## Ports

| Port | Where         | What                                                       |
|------|---------------|------------------------------------------------------------|
| 4300 | `Dioxus.toml` | `[server]` port used by `dx` for the runtime server config |
| 4301 | `compose.yml` | `dx serve` dev port, published as `${HOST_IP}:4301:4301`   |

## Login bypass (dev only)

Copy `.env.example` to `.env` and set both `ADMIN_EMAIL` and `ADMIN_PASSWORD`. When both are set and non-empty at compile time, the WASM client starts pre-authenticated as that admin user and the `/login` route redirects straight to `/dashboard`.

The values are baked into the bundle by `option_env!` at build time, so changing them requires a rebuild. `build.rs` declares `cargo:rerun-if-env-changed` for both vars, so editing `.env` (followed by re-running `just dev`) invalidates the cache and rebuilds.

This is dev-only and the boundary is enforced by Docker:

- `compose.yml` loads `.env` via `env_file` and forwards `ADMIN_EMAIL` / `ADMIN_PASSWORD` into the dev container, where `dx serve` bakes them into the WASM bundle.
- The bypass branch is gated behind `#[cfg(debug_assertions)]` and is compiled out of release builds entirely. `dx build --release` (and the `oci-build/Dockerfile` image) emits a WASM bundle that has no bypass code at all.
- `.dockerignore` excludes `.env` from every Docker build context, so the file never reaches the production builder even if it exists in the working tree.

Leave both vars unset to use the normal login screen.

## Cargo features

- `web` (default) - WASM/web build.
- `multi-tenant` (default) - multi-tenant build.
- `single-tenant` - single-tenant build (mutually exclusive with `multi-tenant`).

## Production build

Two options:

**Local WASM bundle:**

```nu
just build
```

Output lands under `target/dx/`.

**Production OCI image (Caddy + WASM bundle):**

```nu
just build-docker        # tags as mokosh-client:local
just check-docker        # tags as mokosh-client:check (smoke build)
```

The image is built from `oci-build/Dockerfile` and serves the bundle with `oci-build/Caddyfile`.

## Releases

`create-release` bumps the version in both `Cargo.toml` and `package.json`, commits to a `release/vX.Y.Z` branch, pushes, and prints the PR URL:

```nu
just create-release major     # X.0.0
just create-release minor     # 0.X.0
just create-release hotfix    # 0.0.X
```

The recipe refuses to run on a dirty tree, switches to `main`, rebases against `origin/main`, and aborts if the two version fields disagree. After the PR is merged, the `create-release` workflow tags and releases the version automatically.

## Troubleshooting

**`Address already in use (os error 98)` when starting `just dev`:**
Something else is bound to `${HOST_IP}:4301`. Find it with `ss --tcp --listening --numeric --processes 'sport = :4301'` (use `sudo` to see the owning process) or `docker ps --format 'table {{.Names}}\t{{.Ports}}' | grep 4301`. Stop the conflicting process or change the port in `compose.yml` and `Dockerfile`.

**`HOST_IP` is empty:**
The `dev` recipe reads `sys net | where name =~ 'eth0|br0'`. If neither interface has an IPv4 address, the recipe fails. Check `ip --brief address show` and edit the regex to match the right interface.

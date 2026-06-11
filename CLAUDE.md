# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

Pure WASM single-page app: the Mokosh Platform client, written in Rust against Dioxus 0.7 (web + router features), compiled to `wasm32-unknown-unknown`, served in dev by `dx serve` and in prod by Caddy. There is **no native binary, no server-side code, and no backend** in this repo. The backend lives in a separate `mokosh-server` repo and is reached via OIDC + REST.

A `server` Cargo feature is declared but never enabled here: several modules under `src/modules/*` share their shape with the server-side equivalents, and the unused feature lets the `#[cfg(feature = "server")]` gates compile silently rather than emitting unknown-cfg warnings.

## Commands

All day-to-day work goes through `just` (see `justfile`):

| Task | Command |
|---|---|
| List recipes | `just` |
| Dev server (LAN IP bind, hot reload, in Docker) | `just dev` |
| Dev server with Traefik SSO routing | `just dev-sso` |
| Tear down both dev modes | `just down` |
| Restart SSO stack fresh | `just restart` |
| Tailwind one-shot | `just css-build` |
| Tailwind watch | `just css-watch` |
| Local lint sweep | `just check` (= `check-web` + `check-clippy` + `check-fmt`) |
| WASM compile only | `just check-web` |
| Clippy | `just check-clippy` |
| Format check / fix | `just check-fmt` / `just fmt` |
| Tests | `just test` (or `cargo test --lib` for the CI-equivalent) |
| Release WASM bundle | `just build` |
| Prod OCI image (Caddy + bundle) | `just build-docker` / `just check-docker` |
| Cut a release branch | `just create-release {major,minor,hotfix}` |
| Install pre-commit hook (once) | `just install-hooks` |
| Run CI-equivalent checks in the CI image | `just pre-commit` |

The pre-commit hook runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo check --target wasm32-unknown-unknown`, and `cargo test --lib` inside `ghcr.io/niceguyit/rust-builder-glibc:v1.0.0-rust1.94-trixie` so the toolchain matches `.forgejo/workflows/check.yml`.

**Single-test invocation:** `cargo test --lib <pattern>` (e.g. `cargo test --lib oidc::pkce`). CI runs `cargo test --lib` only; there are no integration tests.

**CI quirk:** `assets/styles.css` is gitignored (built by Tailwind), but `asset!("/assets/styles.css")` in `src/main.rs` is checked at compile time. The Forgejo `check` job `touch`es an empty stub before `cargo check`; replicate that when running `cargo` directly on a clean checkout.

## Architecture

### Entry, router, and route guarding

`src/main.rs` does **one thing before `dioxus::launch`**: calls `mokosh_client::modules::oidc::snapshot_initial_search()`. Dioxus 0.7's router will `history.replaceState` to strip query params that aren't part of the declared route shape, so capturing `?code=&state=` from the OAuth redirect must happen pre-mount or the OIDC code exchange will see an empty URL.

`src/lib.rs` defines the full `Route` enum. The authenticated section sits under a single `#[layout(AuthGuard)]`. `AuthGuard` (also in `src/lib.rs`) renders nothing and synchronously kicks off the OIDC flow when unauthenticated. This **render-time gate** is what stops the back-button from flashing protected UI after logout. `use_require_auth` inside individual pages is a redundant safety net, not the primary defense.

Legacy account-management routes (`/login`, `/forgot-password`, `/reset-password/:token`, `/invite/:token`, `/signup`) are kept as `HubRedirect` stubs that bounce to the Bunyip hub so old bookmarks don't 404.

### OIDC (public client, PKCE)

`src/modules/oidc/` is a hand-rolled SPA OIDC client. Tokens live **in memory only** in `AuthContext` (re-exported from `modules::auth`). Persisting them to localStorage was rejected for XSS reasons; on full page reload the user is re-redirected through authorize. Refresh is handled by the `use_token_refresh` background loop mounted at the app root.

### Runtime config (three-tier lookup)

Anywhere the SPA needs a runtime value (API base, OIDC issuer, redirect URI, scopes, hub URL), the lookup order is:

1. `window.__MOKOSH_CONFIG__.<field>` populated by `oci-build/entrypoint.sh` writing `/_mokosh_config.js` from container env vars at start. See `src/modules/runtime_config.rs`.
2. Host-prefix derivation for `msp.<tld>` style deploys (in `hooks::fetch::api` and `OidcConfig::for_current_origin`).
3. Compile-time `option_env!()` defaults baked into the binary.

This means a **single OCI image** serves staging, prod, and self-host: operators set env vars on the container, no rebuild required. Restart the container to pick up changed values.

### Dev modes

Two `compose` files, two purposes:

- `compose.yml` (used by `just dev`): binds `dx serve` to `${HOST_IP}:4301` (detected from `br0`/`eth0`) plus `127.0.0.1:4301`. The 127.0.0.1 bind is there because Google OAuth's redirect URI rejects plain HTTP on non-loopback hosts. Joins the per-developer external network `dev-mokosh-private-${USER}` (shared with `mokosh-server`'s stack).
- `compose.dev-sso.yml` (overlay used by `just dev-sso`): drops the port binds, joins `network-traefik-public`, and routes `https://${USER}-mokosh.a8n.run` to the dx server with TLS via cloudflare cert resolver. Bakes `MOKOSH_OIDC_*` env vars that target `https://${USER}-mokosh-api.a8n.run`. **Fails loud** if `MOKOSH_OIDC_CLIENT_ID` is unset; run `just register-client` in `mokosh-server` first and set it in `.env`.

The base file declares `dev-mokosh-private-${USER}` as `external: true`, so the `dev-sso` and `down` recipes pre-create it (idempotent) before invoking compose; otherwise compose refuses to even validate the file.

`dx serve` runs inside the dev container as a non-root user matching host UID/GID (see `Dockerfile` ARGs `UID`/`GID`); bind-mounted files stay host-owned. Hot reload works through the bind mount.

### Login bypass (debug-only)

Setting both `ADMIN_EMAIL` and `ADMIN_PASSWORD` in `.env` makes the WASM bundle start pre-authenticated as that admin. The bypass branch is gated behind `#[cfg(debug_assertions)]`, so `dx build --release` and `oci-build/Dockerfile` strip it entirely. `.dockerignore` excludes `.env` from production build contexts as defense-in-depth. `build.rs` declares `cargo:rerun-if-env-changed` for both vars so `.env` edits invalidate the cache.

### Cargo features

`web` (default) and `multi-tenant` (default) are the live ones. `single-tenant` is the multi-tenant alternative (mutually exclusive). `server` is declared-but-unused (see "What this repo is" above). Some routes (e.g. `TenantManagement`) are `#[cfg(feature = "multi-tenant")]` gated in `Route`.

### Build artifacts and ports

- `target/dx/mokosh-client/release/web/public/` is the static bundle Caddy serves in prod.
- `assets/styles.css` is the Tailwind build output (gitignored).
- `Dioxus.toml` sets `[server] port = 4300` (used by dx internals only); the dev container actually serves on `4301` (set in `Dockerfile` CMD + `compose.yml` port map). Don't confuse the two.
- `Dioxus.toml` proxies `/api/*` to `http://server:4301/api/` (the `server` DNS name resolves on the shared `dev-mokosh-private-${USER}` network). This single-origin setup is required by Google OAuth's popup + postMessage flow.

### Versioning

`build.rs` injects `APP_GIT_HASH`, `APP_GIT_TAG`, `APP_BUILD_DATE` via `cargo:rustc-env=` and depends on `.git/HEAD`, `.git/refs`, `.git/index` so the embedded version tracks the actual commit.

`just create-release` keeps `Cargo.toml` and `package.json` versions locked in step and refuses to run if they drift. After the release PR merges, `.forgejo/workflows/create-release.yml` tags and publishes.

## Source layout (notable points only)

- `src/modules/oidc/` - PKCE flow, token storage, issuer HTTP helpers.
- `src/modules/runtime_config.rs` - the `window.__MOKOSH_CONFIG__` reader (only relevant if you're touching the runtime-config chain).
- `src/hooks/auth.rs` - `use_auth`, `use_auth_provider`, `use_memberships_loader`, `use_token_refresh`, `use_bfcache_invalidator`. All mounted in `App` in `src/main.rs`.
- `src/hooks/fetch.rs` - `FetchState<T>`, `use_fetch`, the `api` submodule holding the global access-token holder set/cleared from the OIDC callback. `pub mod` (not `mod`) deliberately because other modules outside `hooks/` reach into `api`.
- `dev-docs/codebase-state.md` and `dev-docs/client-server-integration.md` - audit-derived snapshot of UI/UX issues (F1..F19, P0..P3) and the client-server wiring gap table. Keep these in sync with your changes: closing an issue means removing/striking its entry; wiring a UI surface means flipping `decorative`/`mocked` to `wired`.

## Repo hosting

Forgejo (`dev.a8n.run`), not GitHub. Use `fj pr create` for PRs (see global instructions). `gh` is not installed.

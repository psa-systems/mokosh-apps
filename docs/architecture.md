# Architecture

How `mokosh-apps` is put together: the stack it is built on, the shape of the
source tree, the cargo features that select a build, and the two artifacts a
build produces.

## Tech stack

- **[Dioxus](https://dioxuslabs.com/) 0.7** (`0.7.7`), with the `router`
  feature and exactly one renderer feature, `web` or `desktop`.
- **`wasm32-unknown-unknown`** for the SPA; the host target for the desktop app.
- **[Tailwind CSS](https://tailwindcss.com/) v4** via [Bun](https://bun.sh/)
  (`bun x @tailwindcss/cli`), with the `forms` and `typography` plugins.
- **[just](https://github.com/casey/just)** as the task runner, with the shared
  recipes vendored as the `common` submodule.
- **Docker Compose** for the dev server.
- **[Caddy](https://caddyserver.com/)** to serve the built bundle in production
  (see [`oci-build/`](../oci-build/Caddyfile)).

Wire-format types come from `mokosh-types`, a crate in the mokosh-server
repository consumed as a git dependency with no `rev`, `tag` or `branch`: the
resolved commit lives in `Cargo.lock` and `just check-types-pin` fails the build
when the server's head has moved the shared DTOs. That contract, and what the
compiler still cannot see across it, is
[`client-server-integration.md`](client-server-integration.md).

The browser bindings (`gloo-net`, `wasm-bindgen`, `js-sys`) are declared under
`[target.'cfg(target_arch = "wasm32")'.dependencies]`, so a `web_sys` call that
escapes `src/platform/` fails to resolve on the native build rather than
compiling into a binding that panics at run time.

## Repository layout

```
src/
  main.rs           # entry point for both targets; picks the renderer
  lib.rs
  branding.rs       # the operator brand values the SPA reads at run time
  components/       # the shared UI kit: button, card, form, table, modal, icons, layout, ...
  hooks/            # auth, fetch, theme, sidebar, update checks, toasts, ...
  modules/          # per-domain API and state: audit, auth, billing, calendar,
                    # contacts, contracts, forms, kb, oidc, quotes, sla, tenants,
                    # theme, tickets, time_tracking, plus runtime_config and system
  pages/            # one module per route, plus contact_portal/ for the client portal
  platform/         # the host boundary: HTTP, storage, DOM, clock, timers, downloads, ...
  utils/            # formatting, validation, markdown, pagination, sort keys, ...

assets/             # static assets and the built CSS (styles.css is generated)
input.css           # Tailwind entry; compiled into assets/styles.css
index.html          # HTML template for the WASM bundle

Cargo.toml          # crate config, features and build profiles
Dioxus.toml         # dx serve / dx build config: ports, dev proxy, desktop bundle
package.json        # Bun deps (Tailwind v4) and the semver the release keeps in step
justfile            # task runner
common/             # psa-systems/common submodule (shared hook, release and cleanup recipes)
scripts/            # the repository's own guards, one per invariant, run by `just check`
compose.yml         # dev server stack
compose.dev-sso.yml # Traefik-routed overlay for testing the OIDC redirect
Dockerfile          # dev image (dx serve with hot reload)
oci-build/          # production image: Caddy, the entrypoint config shim, the reference compose file
docs/               # this documentation set
```

`src/platform/` is the whole reason the desktop target exists: every call that
reaches the host is behind it, split on `target_arch`, so the same components
render in a browser and in a webview. [`ROADMAP.md`](ROADMAP.md) has the
sequencing that produced it and [`desktop.md`](desktop.md) has the build.

## Cargo features

- `app` is the application-runtime gate: the API module, the app-wide signals
  and the page logic. It is not a platform gate. Every build that produces the
  actual application turns it on, and both renderer features pull it in.
- `web` (default) adds the browser renderer, `dioxus/web`.
- `desktop` adds the native renderer, `dioxus/desktop`. See
  [`desktop.md`](desktop.md); the desktop recipes pass
  `--no-default-features --features desktop,multi-tenant`, because dropping the
  defaults is what turns the web renderer off.
- `multi-tenant` (default) and `single-tenant` select the tenancy build. They
  are mutually exclusive.
- `server` is never enabled by any build here. It gates the mirrored server
  halves of `src/modules/*` and `src/utils/error.rs`, which are byte-identical
  copies of mokosh-server code kept for diffing, so the gate excludes them from
  every build this repository actually runs (MAPPS-141).
- `dev_admin_bypass` is the opt-in dev shortcut that auto-signs the client in
  from `ADMIN_EMAIL` and `ADMIN_PASSWORD`. Off by default in every profile,
  including debug; see the login bypass section of
  [`quickstart.md`](quickstart.md#login-bypass-dev-only).

Enabling both renderers does resolve (Dioxus documents that `desktop` wins),
but it also links `dioxus-web` and its `wasm-bindgen` stubs into a native binary
for nothing, so no recipe does it.

## What a build produces

**The WASM bundle.**

```nu
just build
```

That is `dx build --release --features web` after a one-shot Tailwind build.
The renderer is named explicitly because `dx` substitutes its own feature list
for the crate's defaults rather than reading them. Output lands under
`target/dx/`.

**The production OCI image.**

```nu
just build-docker        # tags mokosh-apps:local
just check-docker        # tags mokosh-apps:check, the smoke build
```

Both build [`oci-build/Dockerfile`](../oci-build/Dockerfile), which compiles the
bundle and serves it with Caddy under
[`oci-build/Caddyfile`](../oci-build/Caddyfile). The published image is
`dev.a8n.run/psa-systems-public/mokosh-www`, multi-arch for `linux/amd64` and
`linux/arm64`. Running it is [`self-hosting.md`](self-hosting.md).

The image is configured at container start rather than at build time.
[`oci-build/entrypoint.sh`](../oci-build/entrypoint.sh) writes a small
`_mokosh_config.js` from the `MOKOSH_*` environment on the container, and the
SPA reads it before falling through to its compile-time defaults. That is what
lets one image serve staging, production and every self-host deployment, and it
is also how the SPA notices it has been redeployed: the shim carries the build
revision, described in [`versioning.md`](versioning.md).

## Where the rest is documented

The conventions a change has to keep are their own pages, each backed by a guard
in `scripts/` that fails `just check` when the convention is broken:
[`form-conventions.md`](form-conventions.md),
[`button-variants.md`](button-variants.md),
[`destructive-actions.md`](destructive-actions.md) and
[`email-actions.md`](email-actions.md). Token storage is
[`oidc-token-storage.md`](oidc-token-storage.md).

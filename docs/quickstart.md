# Quickstart

Get a fresh clone of `mokosh-apps` running as a browser SPA on a Linux host.
The desktop target has its own prerequisites and its own page:
[`desktop.md`](desktop.md).

The dev server runs inside Docker, so the host does not need `dioxus-cli`
installed. It does need a Rust toolchain for the host-side release recipe and
for running `cargo` directly.

## Prerequisites

Install on the host:

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- The `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Bun](https://bun.sh/)
- [just](https://github.com/casey/just)
- [Docker](https://docs.docker.com/engine/install/) with the Compose plugin
- [Nushell](https://www.nushell.sh/) 0.112.2, used by the `dev`, `dev-release`,
  `dev-sso`, `down` and `create-release` recipes

## The shared task runner

The hook, release and cleanup recipes come from
<https://dev.a8n.run/psa-systems/common>, vendored as the `common` git
submodule and imported by the root [`justfile`](../justfile). Run this once in a
fresh clone, or every `just` invocation fails on the unresolved import:

```nu
git submodule update --init
```

Configure those recipes through the variables at the top of the `justfile`;
never redefine one, which `just check-justfile` rejects. `just install-hooks`
writes the `.git/hooks/pre-commit` stub, and `just pre-commit` runs fmt, clippy,
the wasm check and the library tests in the builder image. Recipe by recipe,
grouped the way `just --list` groups them, is [`recipes.md`](recipes.md).

## Start the dev server

```nu
just dev
```

The recipe:

1. Picks the first private (RFC1918) IPv4 address on an `en*`/`eth*`/`br*`/`wlan*`
   interface via `sys net`, falling back to `127.0.0.1` with a warning when it
   finds none.
2. Exports it as `HOST_IP`, along with the host UID, GID and user name.
3. Runs `docker compose up --build`, which starts the `dev-mokosh-apps-${USER}`
   container and publishes port `4301` twice: on `${HOST_IP}` and on loopback.

It prints the address it bound, for example
`Binding dx serve to 172.16.100.120:4301`. A browser on the dev host itself can
also use <http://localhost:4301>. Hot reload is on.

Binding to a private LAN address rather than `0.0.0.0` keeps the dev server off
the public internet when the host is a VPS. That is the load-bearing reason for
the RFC1918 filter, not a preference: `br0` on the shared dev host is a public
interface, and binding dev services to it has been exploited before.

`just dev-release` is the same thing with `dx serve --release`. Rebuilds are
slower, but it is the way through on hosts where the debug-mode post-processing
step chokes on the unstripped WASM (`Failed to write executable: No such file or
directory`).

### Reaching the dev server from another container

Either:

- Use the host LAN address: `http://${HOST_IP}:4301`.
- Or join the `dev-mokosh-private-${USER}` Docker network and use
  `http://dev-mokosh-apps-${USER}:4301`.

### The SSO profile

`just dev-sso` layers [`compose.dev-sso.yml`](../compose.dev-sso.yml) on top of
`compose.yml`: the container drops both host port publishes, joins the shared
`traefik-public` network, and Traefik routes
`https://${USER}-mokosh.a8n.run` to it. Use it when the flow under test is the
OIDC redirect, which needs a real hostname and real TLS. `just down` stops
either profile, and `just restart` is `down` followed by `dev-sso`.

## Ports

| Port | Where | What |
| --- | --- | --- |
| 4300 | [`Dioxus.toml`](../Dioxus.toml) | The `[server]` port in the `dx` runtime server config. |
| 4301 | [`compose.yml`](../compose.yml) | The `dx serve` dev port, published as `${HOST_IP}:4301` and `127.0.0.1:4301`. |

`Dioxus.toml` also proxies `/api/*` to `http://server:8080/api/`, the
mokosh-server compose service on the shared `dev-mokosh-private-${USER}`
network. That proxy is load-bearing rather than a convenience: `api_base()` in
[`src/hooks/fetch.rs`](../src/hooks/fetch.rs) resolves to a same-origin
`/api/v1` on any host that does not start with `msp.`, so in dev every request
the SPA makes is addressed to the dx server's own origin and reaches the API
only through the proxy. One origin is also what keeps the server's
`CLIENT_ORIGIN` and its CORS list a single entry.

## Login bypass (dev only)

Copy [`.env.example`](../.env.example) to `.env` and set both `ADMIN_EMAIL` and
`ADMIN_PASSWORD`. Build with the `dev_admin_bypass` cargo feature and the client
starts pre-authenticated as that admin user, with the `/login` route redirecting
straight to `/dashboard`.

Three conditions all have to hold, and any one of them missing gives the normal
login screen:

- `debug_assertions` is on, so a release build has no bypass code in it at all.
- The `dev_admin_bypass` feature is enabled. It is off by default in every
  profile, including debug, so a debug bundle that reaches staging by accident
  does not auto-grant Admin (MAPPS-338). The gate is
  `#[cfg(all(debug_assertions, feature = "dev_admin_bypass"))]` in
  [`src/hooks/auth.rs`](../src/hooks/auth.rs).
- Both env vars are set and non-empty at compile time.

The values are baked into the bundle by `option_env!` at build time, so changing
one requires a rebuild. [`build.rs`](../build.rs) declares
`cargo:rerun-if-env-changed` for both, so editing `.env` and re-running the
build invalidates the cache.

The `just dev` container does not pass the feature: its `CMD` runs
`dx serve --features web`, so enable the bypass by running the build yourself
with `--features dev_admin_bypass` and the two env vars set.

Two further boundaries hold regardless: `compose.yml` reads `.env` through
`env_file` with `required: false`, so the vars reach the dev container only, and
[`.dockerignore`](../.dockerignore) excludes `.env` from every Docker build
context, so the file never reaches the production builder even when it exists in
the working tree.

Do not reuse a real password in either variable. The bundle ships to every
browser that loads the page, and anyone who downloads it can read both fields.

## Troubleshooting

**`Address already in use (os error 98)` when starting `just dev`.** Something
else is bound to port 4301. Find it with
`ss --tcp --listening --numeric --processes 'sport = :4301'` (use `sudo` to see
the owning process) or
`docker ps --format 'table {{.Names}}\t{{.Ports}}' | grep 4301`. Stop the
conflicting process, or change the port in `compose.yml` and `Dockerfile`.

**`just dev` warns that it found no private LAN IPv4 and binds loopback.** The
host has no RFC1918 address on an `en*`/`eth*`/`br*`/`wlan*` interface. Check
`ip --brief address show`. The dev server still works from a browser on the host
itself at <http://localhost:4301>; it is only unreachable from other machines and
from sibling containers that are not on `dev-mokosh-private-${USER}`.

**`just` fails to parse before running anything.** The `common` submodule is not
checked out. Run `git submodule update --init`.

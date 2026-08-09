# Version sourcing and update targets

How `mokosh-apps` decides what version string to display, and how it
detects that a newer build/release is available. Written for MAPPS-200
("footer showed 0.2 after 0.3 was released").

## TL;DR

- The **displayed release version** is the canonical semver in
  [`Cargo.toml`](../Cargo.toml) (`version`), kept in lockstep with
  [`package.json`](../package.json). It is bumped as the **first** step
  of a release (`just create-release`), so it is correct the instant a
  release commit exists.
- The **git tag** `vX.Y.Z` is created **after** the release PR merges,
  by `.forgejo/workflows/create-release.yml`. It therefore always lags
  the `version` bump by one release and must **not** be the source of
  the displayed version.
- The **commit hash** shown next to the version is build provenance
  only (which exact commit produced this bundle), not the release
  identity.

## Where the version comes from (build-time)

The build-time metadata is baked in by [`build.rs`](../build.rs) and
exposed through [`src/utils/version.rs`](../src/utils/version.rs):

| Constant | Source | Meaning |
| --- | --- | --- |
| `VERSION` | `CARGO_PKG_VERSION` (Cargo.toml `version`) | Canonical released semver. |
| `GIT_HASH` | CI `GIT_SHA` build-arg, else `git rev-parse --short=12 HEAD` | Exact build commit. |
| `BUILD_DATE` | `date -u` at build time | When the bundle was built. |

`build.rs` emits these as `APP_GIT_TAG` / `APP_GIT_HASH` /
`APP_BUILD_DATE` via `cargo:rustc-env`; `version.rs` reads them with
`env!`.

### Why not `git describe --tags`?

The previous implementation derived `VERSION` from
`git describe --tags --always --dirty`. Two failure modes made it show
the wrong version:

1. **Tag lag.** The release tag is applied *after* the release PR
   merges, so a build cut right after the `0.3.0` version bump still
   resolves to `v0.2.0-N-gHASH` - the footer reads "0.2".
2. **No `.git` in the OCI build.** [`.dockerignore`](../.dockerignore)
   strips `.git/`, so inside [`oci-build/Dockerfile`](../oci-build/Dockerfile)
   `git describe` cannot run at all.

Sourcing `VERSION` from `CARGO_PKG_VERSION` removes both: the value is
correct from the release commit onward and needs no git history in the
build context. It also matches the update banner, which already
compares against `CARGO_PKG_VERSION` (see below).

## Where the version is shown (runtime)

- **Footer** - `VersionFooter` in
  [`src/components/layout.rs`](../src/components/layout.rs) renders
  `VERSION`, `GIT_HASH`, `BUILD_DATE` in every layout.
- **System Status page** -
  [`src/pages/system_status.rs`](../src/pages/system_status.rs) shows
  the same three values plus the server's reported build.
- **Update banner** -
  [`src/components/update_banner.rs`](../src/components/update_banner.rs)
  (admins only) surfaces a prompt when an update is available.
- **New-version banner** -
  [`src/components/update_available_banner.rs`](../src/components/update_available_banner.rs)
  (all users) surfaces a "reload this page" prompt when the loaded
  bundle is older than the deployed one. It shows no version or hash;
  it is listed here because it is the other user-visible consequence of
  the build provenance.

## Update targets (staging vs production)

There is no separate "staging channel" vs "production channel" baked
into the SPA. The SPA always measures itself against **whatever server
and deploy it is pointed at**, and that target is already configurable
per environment - no extra version knob is needed.

Two independent mechanisms run:

### 1. Cross-version update (the banner)

[`src/modules/system.rs`](../src/modules/system.rs) `get_version()`
fetches the server's running version from `GET /api/v1/version` and
pairs it with the SPA's own `CARGO_PKG_VERSION`:

- `client.running` = the SPA bundle's `CARGO_PKG_VERSION`.
- `client.latest` = the server's running version. The two images share a
  `major.minor` release line (patch hotfixes ship independently), so the
  server's minor is the minor the matching client bundle should be on.
- The banner compares the `(major, minor)` release line of `client.running`
  (bundle) against `client.latest` (server); patch is ignored, because
  mokosh-www and mokosh-server ship patch hotfixes independently (a www-only
  0.7.2 against a 0.7.0 API is expected and fine, not a mismatch). It shows
  one of two messages:
  - **Client behind** (server on a newer minor, `update_available()`): the
    loaded bundle is a minor behind. "Update available" prompts the admin to
    bump the image tag(s) in `compose.yml` and re-pull.
  - **Server behind** (client on a newer minor, `running_ahead()`): the
    client bundle is a minor *ahead* of the server (e.g. mokosh-www 0.8.x
    while mokosh-server stays 0.7.x). "Server needs updating" prompts the
    admin to upgrade the **server** image; it clears once the server catches
    up.
  A shared release line (any patch delta), an unknown, or an unparseable
  version shows nothing. Each side parses to `(major, minor, patch)` and the
  compare uses `(major, minor)`, so multi-digit fields order numerically
  (`0.10` > `0.7`) and a patch-older API never triggers a prompt. See
  MAPPS-370, MAPPS-372.

The **target** of this comparison is the server named by
`MOKOSH_API_BASE` (set per-container via
[`oci-build/entrypoint.sh`](../oci-build/entrypoint.sh) ->
`_mokosh_config.js`). Point the SPA at the staging API and it measures
against staging; point it at production and it measures against
production. That is the configurable update target.

The server's `/version` endpoint reports only its **own** running
build, not a registry "latest available" tag, so `server.latest` is
always `None` and the banner's server line stays hidden until such an
endpoint ships.

### 2. Within-version auto-reload (stale bundle)

[`src/hooks/update_check.rs`](../src/hooks/update_check.rs) keeps an
open tab from getting pinned to an old WASM bundle across a redeploy of
the **same** version. It compares the bundle's baked-in `GIT_HASH`
against the live `build_sha` field in `_mokosh_config.js` (served
`Cache-Control: no-cache`, baked from the image's `GIT_SHA`). On a
mismatch it reloads at the next safe boundary. The target here is again
"whatever container is currently serving this origin" - inherently the
environment the user is on.

On that same mismatch it raises the app-wide
[`src/components/update_available_banner.rs`](../src/components/update_available_banner.rs)
banner (MAPPS-428), shown to every signed-in user regardless of role,
with a "Reload page" button so the user does not have to wait for the
automatic boundary. That is distinct from the admin update banner
above: this one carries no version numbers or hashes and no operator
instructions, and it is not dismissible.

## Staging vs production summary

| | Version shown | Hash shown | Matches a git tag? |
| --- | --- | --- | --- |
| **Production** | `CARGO_PKG_VERSION` (= released semver) | release commit | Yes, the released `vX.Y.Z`. |
| **Staging** | `CARGO_PKG_VERSION` (may be ahead, the next in-dev version) | the staging build commit | Not necessarily; the hash distinguishes the exact build. |

Both environments always show a version **and** a hash. Production's
version matches the released tag because production is built from a
released commit; staging may be a version ahead, and its hash pins the
exact build.

## Releasing (for reference)

`just create-release <major|minor|hotfix>` (see
[`justfile`](../justfile)) bumps `Cargo.toml` + `package.json`, opens a
release PR, and after merge `.forgejo/workflows/create-release.yml`
tags `vX.Y.Z` and publishes the image. Because the displayed `VERSION`
follows the bump (not the tag), the footer is correct from the release
commit onward.

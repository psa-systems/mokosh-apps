# The task runner, recipe by recipe

`just` with no arguments lists everything. This page groups the recipes the way
`just --list` groups them and says what each is for; `just --list` stays the
authoritative list, because recipes arrive faster than prose does.

Recipes marked *(common)* come from the
[`psa-systems/common`](https://dev.a8n.run/psa-systems/common) submodule and are
configured through the variables at the top of the [`justfile`](../justfile),
never by redefining the recipe. `just check-justfile` fails on a shadowed one.

## Dev

| Recipe | What it does |
| --- | --- |
| `just dev` | Start `dx serve` in Docker, bound to the host's private LAN address. See [`quickstart.md`](quickstart.md#start-the-dev-server). |
| `just dev-release` | The same, in `--release` mode. Slower rebuilds, but the way through when the debug post-processing step chokes on the unstripped WASM. |
| `just dev-sso` | Start the Traefik-routed SSO stack at `https://${USER}-mokosh.a8n.run`, for testing the OIDC redirect. |
| `just down` | Stop the dev stack in either mode. Volumes are preserved. |
| `just restart` | `down` then `dev-sso`. |
| `just desktop-run` | Build and run the desktop app with hot reload. See [`desktop.md`](desktop.md). |

## CSS

| Recipe | What it does |
| --- | --- |
| `just css-build` | One-shot Tailwind build of `input.css` into `assets/styles.css`. |
| `just css-watch` | The same in watch mode. |

`assets/styles.css` is generated and gitignored, which is why every build and
desktop recipe depends on `css-build`: `src/main.rs` embeds the file through
`asset!()`, so it has to exist before any cargo step runs.

## Build

| Recipe | What it does |
| --- | --- |
| `just build` | `dx build --release --features web`. Output under `target/dx/`. |
| `just build-docker` | Build the production OCI image as `mokosh-apps:local`. |
| `just desktop-build` | Build the desktop binary without launching it. |
| `just desktop-bundle` | Build an installable desktop bundle for the current platform. |

## Check

`just check` is the umbrella recipe, and
[`.forgejo/workflows/check.yml`](../.forgejo/workflows/check.yml) mirrors it.
`just check-ci-parity` is what keeps the two honest: it compares the command
lines, not the recipe names, and fails when a command a `check` recipe runs has
no matching `run:` step in the workflow.

The broad ones:

| Recipe | What it does |
| --- | --- |
| `just check` | Everything below except `check-docker`, `check-link-preview` and `check-types-pin-strict`. |
| `just check-web` | `cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings`. |
| `just check-desktop` | Type-check the native desktop build. |
| `just check-clippy` | `cargo clippy --all-targets -- -D warnings` on the host target. |
| `just check-fmt` | `cargo fmt --all --check`. |
| `just test` | `cargo test`. The library target is where the tests live. |
| `just fmt` | `cargo fmt --all`. |

The rest of `scripts/` is one guard per invariant, each with a `--self-test`
that runs first so a guard that has stopped guarding fails loudly instead of
reporting clean. `just check-doc-links` is the one that covers this
documentation set: every relative Markdown link has to resolve to a path that
exists. It enumerates tracked files, so a new page has to be staged before the
guard can see it.

Three are deliberately outside `just check`:

- `just check-docker` builds the production image, and the check runner has no
  Docker.
- `just check-link-preview` runs the real entrypoint and Caddyfile in a
  container and fetches the result with `curl`, for the same reason.
- `just check-types-pin-strict` treats any lock move on mokosh-server as a
  finding, where `check-types-pin` narrows that to moves that actually change
  `crates/mokosh-types`. It is allowed to be red; the weekly
  `types-pin-drift.yml` workflow is what runs it.

## Hooks

| Recipe | What it does |
| --- | --- |
| `just install-hooks` *(common)* | Write the `.git/hooks/pre-commit` stub. Run once per fresh clone. |
| `just pre-commit` *(common)* | Run the same checks CI runs, in the builder image. |
| `just check-justfile` *(common)* | Fail if this justfile redefines a recipe that must come from `common`. |
| `just check-tree-ownership` *(common)* | Fail if the working tree holds a path the host user does not own. |

There is no `compose.dev.yml` here, so `pre_commit_mode` is `docker` and the
checks run in a bare `docker run` against the same builder image the client is
built against.

## Release

`just create-release <major|minor|hotfix>` *(common)* cuts the release branch and
opens its PR. The full sequence, and why the displayed version follows the
version bump rather than the git tag, is in
[`versioning.md`](versioning.md#releasing).

## Cleanup

| Recipe | What it does |
| --- | --- |
| `just dev-clean` *(common)* | Tear down this repository's dev footprint: the dev stack, this user's cargo and target volumes, and the local `target/` and `dist/` artifacts. Scoped to this repository, so it is safe on a shared host. |
| `just dev-clean-all` *(common)* | Everything `dev-clean` does, plus the images this repository builds and its buildx cache. |

## Recipes that do not apply here

`common` also carries a `[dev-local]` group (`dev-local`, `dev-local-detach`,
`dev-local-stop`, `dev-logs`) and a `[native]` group (`run`, `lint`,
`typecheck`) for consumers with a different layout. They are imported because
the import is unconditional, not because this repository uses them.

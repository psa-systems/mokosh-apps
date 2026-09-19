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

Every guard, named and one-line described (source: each script's own header
comment):

| Script | What it guards |
| --- | --- |
| `check-auth-error-prose.sh` | The `/auth/callback` failure classifies on the `FlowError` variant, never by matching the rendered error string. |
| `check-cancel-routes.sh` | A shared create/edit form's Cancel routes back to the record being edited, plus the global pointer-cursor rule. |
| `check-ci-parity.sh` | Every command a `check` recipe runs, including its `--self-test`, has a matching step in `.forgejo/workflows/check.yml`. |
| `check-class-omissions.sh` | Three specific Tailwind class omissions (auth heading, form-grid breakpoint, table name-cell colour) stay fixed. |
| `check-company-id-copy.sh` | No "Portal ID" copy under `src/pages/contact_portal/`; the user-facing name is "Company ID". |
| `check-confirm-destructive.sh` | A destructive mutation never fires straight from a button `onclick`. |
| `check-csp-host-derived-origin.sh` | The served Content-Security-Policy header names the same host-derived origin the app derives itself. |
| `check-defined-colors.sh` | No class references a Tailwind colour token nobody defined. |
| `check-delete-result.sh` | A delete's `Result` is never discarded; a server refusal reaches the user. |
| `check-dev-sso-scheme.sh` | The dev-SSO overlay bakes in `https://` URLs only, never `http://`. |
| `check-doc-links.sh` | Every relative Markdown link under `docs/` resolves to a path that exists. |
| `check-ellipsis-glyph.sh` | Rendered text uses the single ellipsis glyph (`…`), never three ASCII periods. |
| `check-email-affordance.sh` | Every action that emails someone renders the mail icon and an `EmailPreview`. |
| `check-empty-state.sh` | Settings type-editor lists render the full `EmptyState`, never `TableEmpty`'s bare-message mode. |
| `check-fetch-error-logging.sh` | An awaited fetch that fails logs why before the error is discarded. |
| `check-field-value-binding.sh` | A form field's value is set as an attribute, never as a text child. |
| `check-hooks-before-return.sh` | Every hook in a component runs before any early return. |
| `check-kit-adoption.sh` | No DaisyUI classes, and the shared layout, file field, and dropdown-panel recipes stay in use. |
| `check-link-preview.sh` | The served page carries its link-preview tags, verified against a real Caddy and entrypoint stack. |
| `check-loading-recipe.sh` | A busy surface renders the shared `TableLoading`/`DetailSkeleton`, never a hand-rolled "Loading…" string. |
| `check-no-demo-rows.sh` | A page renders only backend rows; a failed fetch never falls back to seeded demo rows. |
| `check-nu-interpolation.sh` | An unescaped `(` inside a Nushell interpolated string is caught before it runs as a subexpression. |
| `check-page-width.sh` | The page-width cap lives on the page component, never back on `AppShell`. |
| `check-per-page-cap.sh` | No call site requests a page size at or above the server's `per_page` cap. |
| `check-prose-layer.sh` | The Markdown prose corrections stay in a cascade layer that outranks the typography plugin. |
| `check-runner-labels.sh` | CI's Rust build runs on the dev runner label, not the base image. |
| `check-sort-keys.sh` | No page hardcodes a `?sort=` value outside the shared `sort_keys` module. |
| `check-status-banner.sh` | Every inline status banner uses `components::StatusBanner`, never a hand-rolled recipe. |
| `check-theme-storage-key.sh` | The first-paint theme script and the app agree on the same `localStorage` key. |
| `check-theme-tokens.sh` | Components use the semantic theme token utilities, never hardcoded neutral or brand colours. |
| `check-types-pin.sh` | The `mokosh-types` git pin does not silently drift behind the server's default branch. |

Three are deliberately outside `just check`:

- `just check-docker` builds the production image, and the check runner has no
  Docker.
- `just check-link-preview` runs the real entrypoint and Caddyfile in a
  container and fetches the result with `curl`, for the same reason.
- `just check-types-pin-strict` treats any lock move on mokosh-server as a
  finding, where `check-types-pin` narrows that to moves that actually change
  `crates/mokosh-types`. It is allowed to be red; the weekly
  `types-pin-drift.yml` workflow is what runs it.

## Claude auto-fix

[`.forgejo/workflows/claude-fix.yml`](../.forgejo/workflows/claude-fix.yml) is
manually dispatched from the Actions UI (or, later, by a comment-trigger
workflow) with a PR number and, optionally, a failed run ID and extra
instructions. It checks out that PR's branch, gathers the failing CI context,
runs Claude Code against it in headless mode, and, if Claude makes a change,
commits and pushes it with a bot PAT so CI re-runs, then comments on the PR
with the outcome either way. It is not a recipe, so it has no `just` entry.

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

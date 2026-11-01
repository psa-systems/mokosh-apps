# Mokosh Platform client specification

This document is normative. Each numbered statement below is a requirement,
identified by `SPEC-<section>.<n>` (for example `SPEC-9.2.4`), meaning:
section 9, subsection 2, requirement 4. A requirement uses "must" or "must
not" for behavior the implementation is required to have; "should" marks a
recommendation that is not load-bearing for correctness. Every requirement
describes one behavior: a trigger, the required outcome, and, where it
matters, the required failure behavior. Numeric section order is fixed
across the full specification; this revision defines Section 1 only. Later
sections are reserved for other parts of the client and are added in later
revisions without renumbering Section 1.

A requirement cites the file and function, script step, or configuration
block that implements or enforces it. Implementing code carries a matching
`SPEC-<id>` comment, in the file's own comment syntax, at each site that
enforces the requirement. The comment cites the id; it does not restate the
requirement text.

## Purpose and non-goals

This document specifies the observable behavior of the Mokosh Platform's
Dioxus-based client application: the Rust/WASM web and desktop client, its
build and packaging, its container runtime image, and the CI automation that
builds, checks and publishes it. It is the source of truth for what this
client must do; where the client's behavior and this document disagree, that
is a defect in one of the two, not a judgment call for whichever reader finds
it first.

It is not a specification of:

- The Mokosh API server, the OIDC provider, or any other service the client
  calls. Those are out of scope entirely; this document treats them as
  external systems reached over HTTP.
- The business logic of any individual workspace area (tickets, companies,
  invoicing, scheduling, and so on), or the client-side data hooks and
  storage keys those areas use. Those belong to later sections of this
  specification, covering files outside this revision's scope.
- Operational runbooks, incident procedures, or anything with a mutable
  status (in progress, planned, deprecated). This document describes
  required behavior, not current project state.

## Architecture and process model

The client is a single Rust crate (`mokosh-apps`) compiled either to
`wasm32-unknown-unknown` for the browser or natively for a desktop webview,
selected by Cargo features rather than by source-level target forks wherever
practical. A `Route` enum, derived via Dioxus's `Routable`, declares every
client-side route and nests most of them under an `AuthGuard` layout that
performs render-time authentication and authorization gating before handing
off to the matched page component. Configuration is resolved at runtime from
a small JSON/JS shim the container entrypoint renders from environment
variables, not baked into the compiled bundle, so one built image serves
every deployment. The container runtime is a static-file web server (Caddy)
that also derives security headers and reverse-proxy origins from that same
configuration. A separate CI pipeline builds, checks, and publishes the
container image from the same source tree.

## Cross-cutting required areas

Every numbered section states, for the files in its scope, what it does in
each of the following areas, or says "none for this section" with a reason
when an area does not apply:

- **Interfaces**: the routes, endpoints, and process entry points the files
  in scope expose.
- **Data model**: any schema, structured file format, or persisted state the
  files in scope define or own.
- **Configuration**: environment variables, build-time features, and config
  files the files in scope read, write, or define.
- **Security**: authentication, authorization, secrets handling, and
  hardening the files in scope perform or require.
- **Error handling and observability**: required failure behavior, and what
  is logged and where.
- **Concurrency and ordering**: sequencing constraints, race conditions
  being guarded against, and anything that must happen before or after
  something else.
- **Integrations**: other systems, services, or repositories the files in
  scope talk to or depend on.
- **Build and deployment**: how the files in scope are built, packaged, and
  shipped.
- **Retention and cleanup**: what is kept, what is discarded, and when.

## 1. Foundation, build and deployment

Scope: the application's entry points and router layout
(`src/main.rs`, `src/lib.rs`, `src/branding.rs`), the build scripts and
manifests (`build.rs`, `Cargo.toml`, `Dioxus.toml`, `package.json`,
`index.html`, `input.css`, `renovate.json`, `.env.example`, `.gitmodules`,
`.dockerignore`, `.gitignore`, `justfile`), the container and compose
definitions (`Dockerfile`, `compose.yml`, `compose.dev-sso.yml`,
`config/variables.toml`, and every file under `oci-build/`), the CI
workflows under `.forgejo/workflows/`, and the dev container definition under
`.devcontainer/`.

### 1.1 Interfaces

- **SPEC-1.1.1**: The client must expose exactly the routes declared in the
  `Route` enum in `src/lib.rs`, in three tiers: public routes reachable with
  no session (sign-in, password reset, invite acceptance, the contact-portal
  login family, and similar); routes nested under the `AuthGuard` layout,
  themselves split between a chromeless group (onboarding, the TV dashboard,
  the "big" board views) and the group nested further under the `AppShell`
  layout (the staff workspace: tickets, companies, scheduling, billing,
  knowledge base, reports, settings, and related areas); and a catch-all
  route matched when no other route does.
- **SPEC-1.1.2**: A request for a route retired in favor of a renamed one
  (the legacy client/platform sign-in paths) must still resolve, by
  redirecting to the current route, rather than falling through to the
  catch-all.
- **SPEC-1.1.3**: A request for a path matching none of the declared routes
  must render the catch-all route's page rather than an unhandled
  router error.
- **SPEC-1.1.4**: The container's HTTP server must serve exactly these
  endpoints at the root of the origin: the runtime configuration shim at
  `/_mokosh_config.js` and `/_mokosh_config.json`, the static bundle and its
  content-hashed assets, the HTML entry point (patched at container start),
  and, for any other path, that same patched HTML entry point, so client-side
  routing resolves a deep link on first load.
- **SPEC-1.1.5**: During local development (`dx serve`), requests under
  `/api/` must be proxied to the API container rather than served by the
  dev server itself.
- **SPEC-1.1.6**: The HTML entry point must load its theme-applier and
  scrollbar scripts as same-origin, non-inline `<script>` tags, consistent
  with the container's Content-Security-Policy, which refuses inline script
  execution.

### 1.2 Data model

None of the files in this section's scope define a schema for persisted
application data; the SPA has no database and no server-side storage of its
own (that is owned by the API server, out of scope here). The one structured
format owned by this scope is the operator-facing configuration catalog:

- **SPEC-1.2.1**: `config/variables.toml` is the single source of truth for
  every operator-facing runtime variable: for each variable, its name,
  the version it was added in, whether it is required, a one-line summary,
  and which file reads it; and, separately, which variables have been
  removed or changed, and when. Generated documentation and its drift check
  must derive from this file, not restate its contents independently.

### 1.3 Configuration

- **SPEC-1.3.1**: The crate's buildable feature combinations must be exactly
  those declared in `Cargo.toml`: `app` (shared code with no renderer),
  `web` and `desktop` (each implying `app` plus the matching Dioxus
  renderer), `multi-tenant` (on by default), `server` (reserved, never
  enabled by any build in this scope), and `dev_admin_bypass` (off by
  default).
- **SPEC-1.3.2**: `Cargo.toml`'s `release` and `wasm-release` profiles must
  each be configured for minimum shipped size (optimize for size, abort on
  panic, strip symbols, and, for the WASM target, thin LTO), independently
  of the `dev`/`wasm-dev` profiles used for local iteration.
- **SPEC-1.3.3**: `Dioxus.toml` must be the single source of the application
  identity, asset directory, WASM optimization level, dev server bind
  address and port, dev-only API proxy target, and desktop bundle metadata
  (identifier and icon set) used by every build and packaging step in this
  scope.
- **SPEC-1.3.4**: `package.json`'s scripts must be the only place the
  Tailwind CSS build (minified for production, watched for local
  development) is invoked from, so every build path (CI, container image,
  `just` recipes) runs the same command.
- **SPEC-1.3.5**: `input.css` must be the single Tailwind entry point: it
  declares the plugins in use, the brand and semantic design tokens
  (resolved through CSS custom properties so a runtime theme change recolors
  the app without a rebuild), and every structural and component-level style
  correction the client depends on.
- **SPEC-1.3.6**: `.env.example` must document every variable a local
  `dx serve` run can set, including that the `dev_admin_bypass` credentials
  it describes are compiled into the bundle and must never be set in a
  production build.
- **SPEC-1.3.7**: The container runtime configuration fields written to
  `_mokosh_config.js`/`.json` (API base URL, OIDC issuer/client id/redirect
  URI/scopes, hub base URL, portal host, docs base URL with a deprecated
  fallback name, organizations-enabled flag, KB attachment size limit, the
  three brand fields, and a build SHA truncated to 12 characters) must be
  produced by one function in `oci-build/entrypoint.sh`, and the build SHA
  truncation there must match the truncation `build.rs` applies when it
  bakes the same value into the compiled bundle's compile-time default.
- **SPEC-1.3.8**: Container-only variables not forwarded to the browser
  (listen `PORT`, the writable `MOKOSH_RUNTIME_DIR`, the link-preview-only
  `MOKOSH_BRAND_DESCRIPTION`, the absolute-URL base `MOKOSH_PUBLIC_URL`, and
  the ownership/identity variables `PUID`/`PGID`/`APP_USER` read by the
  startup wrapper) must each resolve to a documented default when unset.
- **SPEC-1.3.9**: `compose.yml` (the local development stack) must expose a
  documented override for the `dx serve` arguments, cap the Cargo build job
  count to bound memory use, and bind the dev server to a configurable host
  address.
- **SPEC-1.3.10**: `compose.dev-sso.yml` must be an overlay that supplies the
  OIDC issuer, redirect URI, scopes and client id needed to exercise a real
  sign-in flow against a local stack, without altering the base
  `compose.yml` service definition.
- **SPEC-1.3.11**: `oci-build/compose.example.yml` must document every
  operator-facing variable inline, including its fallback behavior, as the
  self-hosting reference compose file.
- **SPEC-1.3.12**: `renovate.json` must declare which dependency managers are
  enabled and how their updates are grouped, and must pin `getrandom` below
  `0.5` until the ecosystem's rand-core transition lands.
- **SPEC-1.3.13**: `.devcontainer/devcontainer.json` must pin an exact base
  image and declare the ports the in-container dev server needs forwarded.
- **SPEC-1.3.14**: `src/branding.rs` must be the only reader of the
  `brand_name`, `brand_logo_url` and `brand_hero_url` runtime configuration
  fields, trimming a configured value and falling back to the built-in
  default whenever the configured value is absent or blank after trimming.

### 1.4 Security

- **SPEC-1.4.1**: `AuthGuard` must decide and act on the unauthenticated case
  during the initial render of the guarded subtree, not in an effect that
  runs after it, so a protected page is never rendered even momentarily
  before a redirect takes effect.
- **SPEC-1.4.2**: `AuthGuard` must not treat a visitor as unauthenticated
  while a contact-session rehydrate from an HttpOnly cookie may still
  succeed; it must hold rendering in that window rather than deciding early.
- **SPEC-1.4.3**: `pathname_is_contact_forbidden` must deny a contact session
  access to every staff-only path prefix it enumerates (companies, contacts,
  calendar, dispatch, scheduling templates, rate cards, payments, tax rates,
  payment gateways, reports, time, timesheets, dashboards, the TV dashboard,
  the "big" board views, admin, tenant switching, org creation, invites, and
  dev-only tooling), with `/settings/*` denied except the hub and the
  branding and appearance leaf pages; `AuthGuard` must redirect a contact
  session matching a denied path to the dashboard rather than render it.
- **SPEC-1.4.4**: A platform-administrator bearer token must let `AuthGuard`
  render the guarded subtree without itself granting any additional
  authorization; per-action authorization remains the API server's
  responsibility.
- **SPEC-1.4.5**: When the deployment has no OIDC issuer configured
  (standalone mode), `AuthGuard` must redirect an unauthenticated visitor to
  the local sign-in page instead of attempting an OIDC redirect.
- **SPEC-1.4.6**: The container's HTTP server must set a Content-Security-Policy
  and the accompanying hardening headers (frame denial, content-type
  sniffing denial, referrer policy, and no `Server` header) on every
  response.
- **SPEC-1.4.7**: The container must not run the application process as root
  once it has started; the startup wrapper must remap to a non-root user (or
  verify that the configured start is already non-root) before executing the
  real entrypoint.
- **SPEC-1.4.8**: The startup wrapper must reject a non-numeric or zero
  `PUID`/`PGID` rather than silently falling back to another identity.
- **SPEC-1.4.9**: The `dev_admin_bypass` feature must default off, and
  `.env.example` must document that credentials it describes are readable by
  anyone who downloads a bundle built with it enabled.
- **SPEC-1.4.10**: Files that can carry secrets (`.env` and its variants,
  excluding the committed `.env.example`) must be excluded from both version
  control and the Docker build context.
- **SPEC-1.4.11**: Every CI check run must include a dependency vulnerability
  audit with its ignored advisories listed explicitly, and the dependency
  update automation must alert on OSV vulnerabilities.
- **SPEC-1.4.12**: The CI workflow that invokes an external coding agent
  against a pull request must pass untrusted PR-derived strings (PR number,
  title, branch name) through environment variables, never interpolated
  directly into a shell script.
- **SPEC-1.4.13**: Every push and pull request must be scanned for
  accidentally committed secrets before any other check is considered
  passing.

### 1.5 Error handling and observability

- **SPEC-1.5.1**: A `wasm32` build that does not enable the `web` feature
  must fail to compile, rather than compile and panic at launch with no
  actionable message.
- **SPEC-1.5.2**: On the web target, the panic hook must be installed before
  any other startup code runs, so a panic during boot still produces a
  readable browser console trace.
- **SPEC-1.5.3**: An error raised while rendering a routed page must be
  caught by an error boundary and shown with recovery actions (return to the
  dashboard, or reload), rather than leaving the page frozen with no
  indication anything failed.
- **SPEC-1.5.4**: The caught route error must be logged, and the error
  boundary's documented limitation (it does not catch WASM panics, only
  propagated errors) must hold.
- **SPEC-1.5.5**: If the container cannot prepare its writable runtime
  directory at startup, it must fail fatally rather than start serving stale
  or absent configuration.
- **SPEC-1.5.6**: If a startup hook run by the privilege-dropping wrapper
  fails, the container must abort startup with a message rather than
  continue with a partially applied environment.
- **SPEC-1.5.7**: If the git hash or build date cannot be resolved at build
  time, the build must fall back to a literal placeholder rather than fail.
- **SPEC-1.5.8**: On desktop, a failed window-icon load must be logged and
  must not prevent the window from opening.
- **SPEC-1.5.9**: CI job-level metrics must be reported even when an earlier
  step in the same job failed.
- **SPEC-1.5.10**: An unresolvable or non-absolute logo URL for the
  link-preview tags must result in those tags being omitted, with the reason
  logged at container start, rather than emitting a URL no crawler can
  fetch.

### 1.6 Concurrency and ordering

- **SPEC-1.6.1**: The initial page URL's query string must be captured before
  the client-side router mounts, so a redirect response appended to the URL
  (an OIDC authorization response) is not lost to a router-driven URL
  rewrite.
- **SPEC-1.6.2**: Whether `AuthGuard` must hold rendering for a contact
  session rehydrate is decided in a signal initializer evaluated during the
  first render, not in an effect evaluated after it, because an effect
  cannot prevent the render it is meant to gate.
- **SPEC-1.6.3**: The container image publish workflow must derive its tag
  set from the triggering event (a push to the default branch, a release
  tag, or a manual dispatch), not from a separately computed value, so that
  two runs triggered by the same release never compute conflicting tags for
  the same artifact.
- **SPEC-1.6.4**: The privilege-dropping startup wrapper must change
  ownership only of files whose ownership does not already match the
  resolved user and group, not the whole tree unconditionally.
- **SPEC-1.6.5**: The container image build must compile dependencies
  against a placeholder source tree before the real source is copied in, so
  an application source change does not invalidate the cached dependency
  build.

### 1.7 Integrations

- **SPEC-1.7.1**: The shared wire-format crate must be consumed as a git
  dependency pinned only through the lockfile (no `rev`, `tag`, or `branch`
  in the manifest), with drift between the lockfile and the dependency's
  default branch detected by a dedicated check.
- **SPEC-1.7.2**: The dependency update automation must group updates to the
  pinned shared crate separately from the general dependency update stream.
- **SPEC-1.7.3**: The local development server must proxy API requests to
  the API container so the client and the API share one origin in
  development, matching the single-origin assumption the production
  container's reverse proxy also makes.
- **SPEC-1.7.4**: The container's reverse proxy must derive the API and OIDC
  origins used in both the Content-Security-Policy and the runtime
  configuration shim from the request's own host whenever an explicit origin
  override is not set.
- **SPEC-1.7.5**: The container image publish workflow must install
  multi-architecture emulation before attempting a multi-platform image
  build, and must mirror a published image to a second registry only by
  copying its manifest list, never by rebuilding it.
- **SPEC-1.7.6**: The manual CI-fix workflow must invoke the external coding
  agent against the failing pull request and push a commit only when the
  agent's run produced an actual diff.
- **SPEC-1.7.7**: The release workflow must trigger only on a merged pull
  request whose source branch follows the release-branch naming convention,
  and must delegate the actual release steps to the shared, repository-external
  reusable release workflow.

### 1.8 Build and deployment

- **SPEC-1.8.1**: Building the web bundle must regenerate the Tailwind
  stylesheet before compiling, and must pass the `web` feature explicitly to
  the build tool, which otherwise substitutes its own feature list for the
  crate's declared defaults.
- **SPEC-1.8.2**: The production container image must be built in two
  stages: a builder stage that produces the platform-independent WASM
  bundle, and a minimal runtime stage that serves it with no build toolchain
  present.
- **SPEC-1.8.3**: The builder stage must verify that the WASM optimizer
  version it installs matches the version the build tool itself resolves,
  and must fail the build on a mismatch rather than risk a build that
  succeeds with a mismatched optimizer.
- **SPEC-1.8.4**: The runtime stage must include the built static bundle,
  the root-level icon and manifest files, and the two startup scripts in the
  served web root.
- **SPEC-1.8.5**: The development container image must pre-build
  dependencies against a placeholder source tree (see SPEC-1.6.5) and must
  run the dev server with the `web` feature on container start.
- **SPEC-1.8.6**: The container image checks and builds must run from the
  repository root using the build tool's multi-platform builder.
- **SPEC-1.8.7**: The CI check job must run, at minimum, a formatting check,
  a lint check with warnings treated as errors on both the host and the
  WASM target, a desktop-feature compilation check, the library test suite,
  and the dependency audit, in addition to the repository's own guard
  scripts.
- **SPEC-1.8.8**: The CI check job must produce a placeholder stylesheet
  before any guard or compile step runs, because the generated stylesheet is
  excluded from version control and this job does not render it for real.
- **SPEC-1.8.9**: The container image publish workflow must resolve exactly
  one of its publish modes (a tagged release, a rolling "latest" update, a
  branch build, or a dry run) from the triggering event, and must tag the
  published image only within that mode's naming convention.
- **SPEC-1.8.10**: A non-dry-run manual run of the publish workflow must fail
  immediately if it targets neither the default branch nor a release tag.
- **SPEC-1.8.11**: The scheduled dependency-pin drift check must run the
  strict variant of the shared-crate pin check, and its failure must not
  block any pull request.
- **SPEC-1.8.12**: The dependency update workflow must run on its schedule or
  on manual dispatch and must delegate to the shared, repository-external
  reusable update workflow.
- **SPEC-1.8.13**: Any CI job that needs the repository's shared recipes must
  check out the git submodule that provides them.
- **SPEC-1.8.14**: Build artifacts, dependency caches, and editor or OS
  files must be excluded from version control; the generated stylesheet must
  additionally be excluded from the Docker build context.
- **SPEC-1.8.15**: The Docker build context must exclude development-only
  compose files, Dockerfiles, and environment files, so they cannot be
  copied into an image layer.
- **SPEC-1.8.16**: The dev container's provisioning steps must run
  identically regardless of which of the dev-container lifecycle hooks
  invokes them, because not every client runs every hook as the configured
  user.

### 1.9 Retention and cleanup

- **SPEC-1.9.1**: The container entrypoint must restore a pristine copy of
  the bundled HTML entry point into the writable runtime directory on every
  container start, so a previous run's in-place patch never persists across
  a restart.
- **SPEC-1.9.2**: The container must require no persistent volume beyond a
  writable scratch directory for runtime-generated files; that directory's
  default location must not be expected to survive container recreation.
- **SPEC-1.9.3**: Generated build artifacts, dependency caches, and editor or
  OS files must never be committed, so a fresh clone carries no stale
  generated state (see SPEC-1.8.14).

## 2. Repository check scripts

Scope: every file under `scripts/`: the 35 `check-*.sh` lint/invariant scripts
and `config-docs.nu`, the configuration-documentation generator. Each
`check-*.sh` script is a standalone, repeatable guard against one named
regression (most carry an issue id in their header comment); each is invoked
from `justfile` and, for the ones `just check` depends on, from
`.forgejo/workflows/check.yml`. This section does not cover the files these
scripts read as fixtures or targets (`src/**`, `input.css`, `justfile`,
`.forgejo/workflows/*.yml`, `config/variables.toml`, `docs/configuration.md`)
except to state what the scripts in this section's scope require of them;
those files' own behavior belongs to the sections that own them.

### 2.1 Interfaces

- **SPEC-2.1.1**: Every `check-*.sh` script must be invocable with no
  arguments and, unless stated otherwise below, must then scan a fixed
  default root (`src`, or the one file the guard is named for) and exit 0
  with a `<name> guard: clean` (or `: OK`) line on stdout if the invariant
  holds, or a nonzero exit with a `<name> guard: FAIL (<reason>)` line
  naming the reason if it does not.
- **SPEC-2.1.2**: A `check-*.sh` script whose default target can be
  overridden (by ROOT, DIR, or FILE) must accept that override as its first
  positional argument, scanning the given path instead of the default.
- **SPEC-2.1.3**: A `check-*.sh` script that accepts a `--self-test` first
  argument must, when given it, run its own detection logic against
  generated fixtures proving it still rejects every violation shape it
  exists to catch and still accepts the corrected/migrated shape, print one
  line per fixture case stating whether that case behaved as expected, and
  exit 0 only if every case behaved as expected, exit nonzero otherwise; the
  script must not scan the real tree in this mode.
  `check-company-id-copy.sh`, `check-auth-error-prose.sh`,
  `check-runner-labels.sh`, `check-cancel-routes.sh`, and
  `check-csp-host-derived-origin.sh` carry no `--self-test` mode (the first
  three have no synthetic fixture to construct; `check-cancel-routes.sh`'s
  pinned-list diff and `check-csp-host-derived-origin.sh`'s three live HTTP
  scenarios run against real source on every invocation, which is their own
  test of the guard).
- **SPEC-2.1.4**: `check-types-pin.sh` must additionally accept a `--strict`
  flag before `--self-test` or the real run, selecting the unnarrowed
  pre-MAPPS-537 rule (every lock move not just a `mokosh-types`-touching
  one); `check-types-pin.sh` and `check-types-pin.sh --compare OLD NEW
  [STATE]` must accept that explicit two/three-argument form for driving the
  comparison directly off two lock-file snapshots instead of running
  `cargo update` itself.
- **SPEC-2.1.5**: `check-theme-storage-key.sh` must additionally accept
  `--compare RUST_FILE JS_FILE` to run its key-equality and stray-read checks
  against an explicit file pair instead of the repo's own
  `src/hooks/theme.rs` / `assets/theme-init.js`, and must reject any other
  unrecognized first argument with exit 2.
- **SPEC-2.1.6**: `check-page-width.sh` must additionally accept `--files
  LIB LAYOUT` to run its AppShell/route-cap checks against an explicit file
  pair instead of the repo's own `src/lib.rs` / `src/components/layout.rs`.
- **SPEC-2.1.7**: `config-docs.nu` must expose exactly five subcommands:
  `generate` (rewrite the generated parts of `docs/configuration.md`),
  `check` (fail if that file disagrees with the facts), `since <version>`
  (list every configuration change after a given release, optionally as
  `--json`), `release-notes <version> [--ref REF]` (emit release-note prose
  for one release), and `self-test` (prove its own fact-checking logic
  against an in-memory fixture, touching neither `config/variables.toml` nor
  `docs/configuration.md`); invoked with no subcommand it must print a one
  line usage summary and exit 0.

### 2.2 Data model

- **SPEC-2.2.1**: `config-docs.nu` must read three kinds of fact records
  from `config/variables.toml`: `[[variable]]` (`name`, `since`, `required`
  defaulting to false, optional `required_since`, optional `deprecated =
  { release, replacement, issue }`), `[[removed]]` (`name`, `since`,
  `removed`, `issue`, optional `action` defaulting to empty, optional
  `breaking` defaulting to empty), and `[[change]]` (`release`, `group` one
  of Added/Changed/Deprecated/Removed, `variables`, optional `issue`
  defaulting to empty, optional `action` defaulting to empty, optional
  `breaking` defaulting to empty, optional `feature` defaulting to empty),
  and must derive from them a flat list of changelog entries, one per
  (release, group, variable) combination, each carrying whether the
  variable was required as of that release and a breaking level of `""`,
  `"wont-start"`, or `"feature-off"`.
- **SPEC-2.2.2**: `config-docs.nu generate` must maintain exactly two
  generated regions of `docs/configuration.md`: the block between the
  literal marker lines `<!-- BEGIN GENERATED: recently-added (just
  config-docs) -->` and `<!-- END GENERATED: recently-added -->` under the
  "Recently added" heading, and the Required/Added-in columns of every row
  under the `## All variables` heading matching `| <a
  id="..."></a>\`VAR\` | Required | Added in | ...`; it must leave every
  other column and every other part of the file untouched.
- **SPEC-2.2.3**: `check-ci-parity.sh` must treat `justfile`'s `check:`
  recipe's `check-*`-named dependencies as the authoritative set of checks
  that must run in `.forgejo/workflows/check.yml`, resolving each
  dependency's command lines from the root justfile or, failing that, from
  one level of its `import '...'` files, and falling back to a synthetic
  `just <recipe>` line when a recipe's body is a shebang script or contains
  `{{` interpolation it cannot statically resolve.
- **SPEC-2.2.4**: `check-loading-recipe.sh` must treat its hardcoded
  per-file baseline table as the ceiling on pre-existing `"Loading` string
  literals for the files it lists, and zero for every other file, and
  `check-email-affordance.sh` must treat its hardcoded `EMAIL_PATHS` table
  (pattern plus human label) as the complete set of server paths whose
  callers are required to render both a mail icon and an email preview;
  neither table is read from any file other than the script itself.

### 2.3 Configuration

- **SPEC-2.3.1**: A `check-*.sh` script that recognizes a per-line opt-out
  marker (`theme-guard-allow`, `card-heading-allow:`, `dev-sso-scheme-guard-
  allow`, `sort-key-guard-allow`, `ellipsis-guard-allow`, `loading-guard-
  allow`, `per-page-guard-allow`, `nu-interp-guard-allow`, `fetch-error-
  logging-allow:`) must skip the violation it would otherwise report on any
  line carrying that marker, and must not treat the marker's presence
  elsewhere in the file as an opt-out for a different line.
- **SPEC-2.3.2**: `check-fetch-error-logging.sh`'s `fetch-error-logging-
  allow:` marker must require a non-whitespace reason after the colon; a bare
  marker with nothing following it must not suppress the violation.
- **SPEC-2.3.3**: `check-csp-host-derived-origin.sh` must read
  `MOKOSH_API_BASE` and `MOKOSH_OIDC_ISSUER` only as environment variables
  passed into the container it starts under test, never from the host
  environment the script itself runs in, so the guard's three scenarios
  (zero-config, zero-config on a non-derived host, explicit origin) are
  reproducible regardless of the invoking shell's environment.
- **SPEC-2.3.4**: `check-oci-runtime.sh` must configure its read-only-
  runtime-directory scenario via `MOKOSH_RUNTIME_DIR` pointed at a mount the
  test container cannot write under any uid, and must configure its uid
  scenarios via the container's `--user` flag, not via a script-level
  environment variable the entrypoint reads.

### 2.4 Security

- **SPEC-2.4.1**: `check-refresh-token-storage.sh` must fail if
  `src/hooks/fetch.rs` writes a portal or contact refresh token
  (`PORTAL_REFRESH_STORAGE_KEY` or `CONTACT_REFRESH_STORAGE_KEY`) via
  `local_storage()` within 6 lines of the key reference, rather than
  `sessionStorage`, because a refresh token in `localStorage` outlives the
  tab and survives an XSS payload's exfiltration window; this is a
  proximity heuristic bound to +/-6 lines; a discard spread wider is out of
  the guard's reach.
- **SPEC-2.4.2**: `check-dev-sso-scheme.sh` must fail if
  `compose.dev-sso.yml` (or an explicit target file) contains a bare
  `http://` URL outside a `#` comment and not marked
  `dev-sso-scheme-guard-allow`, because the dev-SSO overlay is TLS-routed
  and a plain-http absolute URL baked into it bypasses that routing.
- **SPEC-2.4.3**: `check-csp-host-derived-origin.sh` must fail if, serving
  the real container stack with no explicit origin configured on an
  `msp.<tld>` request host, the served `Content-Security-Policy`'s
  `connect-src` and `img-src` do not name the API/OIDC origins the client
  itself derives from that host; must fail if, on a non-`msp.<tld>` host
  with no explicit origin, the served policy adds any derived API origin at
  all; and must fail if, with `MOKOSH_API_BASE`/`MOKOSH_OIDC_ISSUER`
  explicitly set, the served policy either drops the operator's origin or
  still names the host-derived one.
- **SPEC-2.4.4**: `check-confirm-destructive.sh` must fail if any
  `onclick:` handler issues a delete (`delete_authed`, `delete_authed_typed`,
  or `delete_lookup`, directly or through a traced local alias) without the
  action first passing through a confirmation step, and must fail loudly,
  not silently pass, when its own brace/string parse of a file cannot
  close (ends at nonzero depth, or ends inside an unterminated string or raw
  string), rather than reporting that file clean because it could not be
  parsed.
- **SPEC-2.4.5**: `check-delete-result.sh` must fail if a `delete_authed*`
  call's result is reduced to `.is_ok()` or dropped via `let _ =` within 3
  lines of the call, because the discarded `Result`'s `Err` string is the
  server's own refusal reason and the UI's only way to show it; collecting
  results across a bulk delete (counting failures rather than discarding
  them) must not be flagged.
- **SPEC-2.4.6**: `check-oci-runtime.sh` must fail if the container's
  entrypoint starts successfully (exit 0) when its writable runtime
  directory is mounted read-only to every uid, and must fail if its startup
  log contains a case-insensitive "permission denied" under either the
  default uid or an explicit `--user` override; it must require that a
  refused boot on an unwritable runtime directory logs `FATAL` and the
  directory path to stderr.
- **SPEC-2.4.7**: `check-types-pin.sh` must fail if `cargo update --package
  mokosh-types` moves `Cargo.lock` while the git revision it reports for
  `crates/mokosh-types` on `mokosh-server`'s default branch is unchanged
  from the currently pinned revision (meaning something else in the lock
  moved, not the dependency this guard exists to catch), and must fail,
  never silently pass, when it cannot determine whether the crate itself
  changed (no local git mirror, either revision absent from it, or `git
  diff` erroring) rather than assuming either outcome; under the default
  (non-`--strict`) mode it must treat a pin move as clean when the crate's
  own source is provably unchanged (a catch-up-only move), and under
  `--strict` that same catch-up-only move must still fail.

### 2.5 Error handling and observability

- **SPEC-2.5.1**: A `check-*.sh` script must exit 2, not 0 or 1, when it
  cannot evaluate the invariant at all: a required target file or directory
  does not exist, a `find`/`awk` sub-process it depends on exits nonzero, or
  (except `check-nu-interpolation.sh`, which exits 1 for this case) the scan
  matches zero files to check; "found nothing to check" must never be
  reported as a pass.
- **SPEC-2.5.2**: `check-class-omissions.sh`'s heading-weight and
  table-name-cell passes must each fail if the number of elements they
  matched across the whole scan falls under a fixed floor (50 headings, 15
  table-cell spans respectively), so a refactor that silently moves the
  shape out from under the detection pattern is loud rather than passing
  with zero matches.
- **SPEC-2.5.3**: `check-auth-error-prose.sh` must fail independently on
  each of two conditions in `src/pages/auth_callback.rs` (a `.starts_with`/
  `.ends_with`/`.contains("` string match on an error message; the absence
  of a call to `classify_flow_error`), printing both failures when both
  occur rather than stopping at the first.
- **SPEC-2.5.4**: `check-runner-labels.sh` must independently check, and
  report every failing one of, three conditions on `.forgejo/workflows/*.yml`:
  the Rust-build job in `check.yml` requesting the heavy runner label, no
  workflow referencing a retired `*_LATEST` runner label, and `check.yml`
  not installing OS packages at run time.
- **SPEC-2.5.5**: `check-cancel-routes.sh` must fail if the sorted set of
  `Link`-rendered Cancel destinations across `src/pages/*.rs` differs at all
  from its pinned expected set, printing a diff of expected versus actual so
  a new, removed, or retargeted Cancel site is visible, and must separately
  and unconditionally check `src/pages/knowledge_base.rs`'s non-Link
  confirm-before-cancel control (routes to the record being edited, asks
  only when the form is dirty) and `input.css`'s base-layer `cursor:
  pointer` rule on enabled buttons, reporting every one of the checks that
  fails rather than stopping at the first.
- **SPEC-2.5.6**: `check-no-demo-rows.sh` must fail independently for each
  of two patterns found anywhere under `src/pages`: a bare `Demo` token
  naming a data-source variant, and rendered copy admitting (case-
  insensitively) that displayed rows are "demo rows"; on a failed fetch a
  page must render the failure, never backend-absent invented rows.
- **SPEC-2.5.7**: `check-empty-state.sh` must fail a `TableEmpty { .. }`
  block in the checked file (`src/pages/settings.rs` by default) that uses
  the bare `message:` prop, that lacks `title:`, `description:`, or
  `actions:`, whose `actions:` carries no `Button` with a "New <thing>"
  label, or whose copy instructs the user to click a named button in prose,
  reporting every one of those conditions that holds rather than only the
  first.
- **SPEC-2.5.8**: `check-status-banner.sh` must fail if any page under
  `src/pages` hand-rolls a state-hue banner fill (`bg-{red,green,amber,
  blue}-50`) instead of rendering `StatusBanner`, and must independently
  fail if `src/components/error_banner.rs` is missing, if any of the four
  `BannerTone` recipes it must carry (red, green, amber, blue, each an exact
  class string) is not present in it exactly once, or if any of those
  recipes is copied verbatim into another component file.
- **SPEC-2.5.9**: `check-defined-colors.sh` must fail on any colour-shaped
  Tailwind utility class in `src/**/*.rs` whose bare colour name is neither
  one of `input.css`'s own `--color-*` tokens nor a recognized non-colour
  Tailwind keyword, because Tailwind v4 emits no CSS at all (no error, no
  fallback) for an unresolvable name.
- **SPEC-2.5.10**: `check-field-value-binding.sh` must fail a `textarea {
  .. }` in `src/components/form.rs` that has no `value:` attribute, and must
  independently fail one whose value is passed as a trailing string child,
  because a `<textarea>`'s text child is only its default value and stops
  reflecting the signal the moment the user types.
- **SPEC-2.5.11**: `check-hooks-before-return.sh` must fail any `use_*`
  hook call that appears, at component-body or single-level-`if`-body
  indentation, after an early `return` within the same `#[component]`
  function or custom `use_*` hook function, because Dioxus identifies a hook
  by call order and a hook skipped on one render desyncs every later
  render.
- **SPEC-2.5.12**: `check-per-page-cap.sh` must fail a literal `per_page=N`
  with N at or above 100, or a `per_page={CONST}` interpolation resolving to
  such a constant, anywhere outside `src/hooks/fetch.rs`, because the server
  enforces that cap itself.
- **SPEC-2.5.13**: `check-nu-interpolation.sh` must fail a literal,
  unescaped `(` inside a Nushell `$"..."` interpolated string (outside a
  `($...)` subexpression) in any `*.nu` file, `justfile`, or the `run:` body
  of a `shell: nu` step in `.forgejo/workflows/*.yml`, because Nushell
  parses it as a subexpression open rather than a literal character; a
  `shell:` and `run:` key may appear in either order within the same CI
  step.
- **SPEC-2.5.14**: `check-page-width.sh` must fail if
  `src/components/layout.rs`'s `AppShell` re-applies a `max-w-` class or no
  longer supplies `px-4 sm:px-6 lg:px-8` padding on its `main`, if any route
  nested under `#[layout(AppShell)]` in `src/lib.rs` is not found as a named
  component, if a route named in the full-width allowlist still carries the
  `max-w-7xl mx-auto` cap, or if any other such route does not carry it.
- **SPEC-2.5.15**: `check-card-headings.sh` must fail a `Card { .. }`
  block (including through any number of wrapper `div`s) whose first
  rendered element, other than a prop line or a line marked
  `card-heading-allow:`, is an `h2` or `h3`, because the card's heading must
  come from its `title` prop; a heading rendered after other content in the
  card is out of this guard's narrow scope by design.
- **SPEC-2.5.16**: `check-theme-tokens.sh` must fail a hardcoded neutral
  gray or `bg-white` utility outside `src/modules/theme/` and
  `src/components/theme_picker.rs`, and must independently fail a
  state-hue `text-*` utility (red, green, yellow, amber, orange, blue) or a
  `ring-blue-*` utility with no `dark:`-prefixed sibling pair in the same
  variant chain, because an unpaired light-mode state colour fails contrast
  on the dark surface.
- **SPEC-2.5.17**: `check-theme-storage-key.sh` must fail if
  `src/hooks/theme.rs` and `assets/theme-init.js` do not each declare
  exactly one `THEME_KEY` constant, if the two declared values differ, or if
  the JS applier reads or writes `localStorage` by any path other than
  `THEME_KEY` (a bare property, an `in` check, or a literal-string
  `getItem`/`setItem`/`removeItem` call).
- **SPEC-2.5.18**: `check-email-affordance.sh` must fail if no file under
  `src` calls one of its listed email-sending patterns at all (the pattern
  list has drifted), and independently must fail any calling file that
  renders no `MailIcon { ` or no `EmailPreview { ` alongside that call.
- **SPEC-2.5.19**: `check-doc-links.sh` must fail a Markdown link, image,
  reference definition, or raw HTML `href`/`src` in a tracked `.md` file
  whose target, resolved relative to that file's own directory (not the
  repository root) and stripped of any fragment or query, does not exist on
  disk; it must not evaluate a target inside a fenced code block or an HTML
  comment, and must not evaluate an anchor-only, protocol-relative, or
  scheme-qualified (`http:`, `mailto:`, etc.) target.
- **SPEC-2.5.20**: `check-kit-adoption.sh` must fail a DaisyUI
  `input-bordered` class anywhere under `src` (the build has no plugin for
  it, so it renders unstyled); must fail if the set of pages using
  `min-h-screen` is not exactly `{home.rs, not_found.rs, auth_callback.rs}`;
  must fail if `AuthLayout` no longer renders the product-name helper or
  `VersionFooter`, or if `login.rs`, `onboarding.rs`, or `request_form.rs`
  no longer render `AuthLayout`; must fail a hand-copied file-input class
  recipe (`file:mr-3`) outside `components/form.rs`, or that recipe present
  there other than exactly once, or a raw `r#type: "file"` element outside
  `components/form.rs`'s own two; must fail `input.css`'s `.dropdown-panel`
  rule declared other than exactly once; must fail a hand-copied floating-
  panel recipe (`bg-raised` with `shadow-lg` and no `dropdown-panel` class),
  any untokenized `ring-black`, or a hand-copied accent-button recipe
  (`bg-accent` with `text-on-accent`) outside `components/button.rs` and the
  one documented `components/table.rs` exception.
- **SPEC-2.5.21**: `check-link-preview.sh` must fail if, serving the real
  container stack, a crawler request to `/` or to an unrouted deep path
  (served via the SPA fallback) does not carry every expected `og:`/
  `twitter:` tag for each of: branded configuration, no configuration at all
  (built-in defaults, `twitter:card` of `summary`, no image tags), and a
  relative logo URL with no public base URL configured (image tags omitted
  entirely, with the reason logged); must fail if a second entrypoint run
  against the same container filesystem, with the branding env vars changed,
  still shows the old values or leaves more than one copy of the metadata
  marker comment in the served HTML.
- **SPEC-2.5.22**: `check-ci-parity.sh` must fail if any command line
  belonging to a `check:`-recipe dependency in `justfile` does not appear
  verbatim anywhere in `.forgejo/workflows/check.yml`, naming the missing
  recipe and command; it must fail with exit 2, not 1, if it cannot even
  parse the justfile's `check:` dependency line or a named recipe's
  commands, since that is the guard's own precondition failing, not the
  invariant it is checking.
- **SPEC-2.5.23**: `check-scrollbars.sh` must fail `input.css` (and, when
  it is non-empty, the built `assets/styles.css`) if it sets
  `scrollbar-width: none` or hides a `::-webkit-scrollbar*` rule anywhere,
  declares `scrollbar-width`/`scrollbar-color` outside the Firefox
  `@supports not selector(::-webkit-scrollbar)` block, references
  `--sb-alpha` inside that block, paints a background other than
  transparent on anything but the webkit thumb, or sets
  `scrollbar-gutter: stable`; it must independently fail if any of the
  required declarations (a 14px-wide and -tall webkit scrollbar, a
  transparent webkit track, an alpha-mixed webkit thumb colour, the
  `@property --sb-alpha` declaration, the Firefox block itself and its
  `thin`/idle/active `scrollbar-color` rules, and `html`/`body` both set to
  `height: 100%; overflow: hidden`) is missing; it must fail
  `assets/theme-init.js` (or the configured JS file) if `SCROLLBAR_IDLE_MS`
  is declared other than exactly once or its value falls outside
  5000-7000ms; an empty or absent built `assets/styles.css` must not itself
  be treated as a failure, since CI stubs it without building it.
- **SPEC-2.5.24**: `check-fetch-error-logging.sh` must fail an `.ok()`,
  `.unwrap_or_default()`, `.unwrap_or_else(`, or `.unwrap_or(` call chained
  (through no more than 200 characters and no statement boundary) directly
  onto an `.await` on a `Result`, with no `tracing::` call between the
  `.await` and the collapse, because that drops the awaited error instead of
  logging it.
- **SPEC-2.5.25**: `check-prose-layer.sh` must fail `input.css` if no
  `@layer` block containing the Markdown-correction selector
  `.prose :where(code)` exists, if that layer is one of Tailwind's own four
  built-in layers (`theme`, `base`, `components`, `utilities`), if the layer
  has no bare `@layer <name>;` declaration positioned after the Tailwind
  import, or if the file no longer contains the `content: none;` rule that
  neutralizes the typography plugin's backtick glyphs; when a real built
  `assets/styles.css` exists (one whose combined `@layer` statement names
  `utilities`), it must additionally fail if that file's combined `@layer`
  statement never names the corrections' layer, or names it before
  `utilities`.
- **SPEC-2.5.26**: `check-oci-runtime.sh`, `check-link-preview.sh`, and
  `check-csp-host-derived-origin.sh` must each require Docker to be present,
  failing with exit 2 and a message naming Docker as the requirement if it
  is not on PATH, rather than letting the absence surface as an
  unexplained failure further down the script.
- **SPEC-2.5.27**: `config-docs.nu check` must fail (stderr, exit 1) if
  `docs/configuration.md` disagrees with `config/variables.toml`'s facts, or
  with the actual `${MOKOSH_...}` reads in `oci-build/entrypoint.sh`,
  listing every disagreement found: a catalog name declared more than once;
  a release field not shaped `vX.Y.Z`; a `required_since` set without
  `required` or not later than `since`; a `[[change]]` record naming an
  unknown group, an unrecognized `breaking` level, a variable not in the
  catalog, or (for a non-`Changed` group) one with no matching derived
  fact; a breaking entry missing its required action text, issue id, or (for
  `feature-off`) its feature name; a name read by the entrypoint but absent
  from the catalog, or a catalog name the entrypoint no longer reads; a
  malformed, missing, duplicated, or extra "All variables" table row, or one
  whose anchor, Required, or Added-in column disagrees with the facts; and a
  malformed "Recently added" entry, one naming a change the facts do not
  record, one whose Required/Breaking-mark/issue/feature/anchor text
  disagrees with the facts, or a fact inside the current release window with
  no matching entry in that block.
- **SPEC-2.5.28**: `config-docs.nu generate` must fail (stderr, exit 1)
  before writing anything if the underlying facts in
  `config/variables.toml` are inconsistent by the same catalog/change-record
  rules `check` applies to the facts (duplicate names, malformed releases,
  invalid `required_since`, invalid `[[change]]` records); it must leave
  `docs/configuration.md` unmodified and print `docs/configuration.md is
  current` if regenerating the two generated regions produces byte-identical
  output, and otherwise overwrite the file and print that it rewrote the
  generated parts.
- **SPEC-2.5.29**: `config-docs.nu since <version>` must reject a
  `<version>` argument not shaped `vX.Y.Z`; `config-docs.nu release-notes
  <version>` performs no such validation and instead reports an unmatched
  version the same way it reports a real release with zero changes
  ("No configuration changes in `<version>`"), which this specification
  records as the stated behavior of `release-notes`, not a defect.

### 2.6 Concurrency and ordering

- **SPEC-2.6.1**: `check-hooks-before-return.sh` encodes the ordering
  invariant that every Dioxus hook call in a component or custom hook
  function must execute on every render in the same relative order,
  specifically before any `return` that could skip it on some renders and
  not others (see SPEC-2.5.11).
- **SPEC-2.6.2**: `check-field-value-binding.sh` encodes the ordering
  invariant that a `textarea`'s displayed value must be driven by the
  `value:` attribute on every render after the first, because a text child
  is copied into the DOM value only while the element is still in its
  initial "clean" state, before the user's first keystroke (see
  SPEC-2.5.10).
- **SPEC-2.6.3**: `check-prose-layer.sh` encodes the ordering invariant
  that CSS cascade-layer precedence is resolved by first mention, not by
  source position of the full block, so the Markdown corrections' bare
  `@layer` declaration must be the first mention of that layer and must
  itself appear after the Tailwind import (see SPEC-2.5.25).
- **SPEC-2.6.4**: None of the check scripts in this section's scope impose
  an ordering requirement between each other; `justfile`'s `check:` recipe
  order and `check-ci-parity.sh`'s command-line comparison (SPEC-2.5.22) are
  the only place script invocation order is required to match anything, and
  that match is against `.forgejo/workflows/check.yml`'s command text, not
  against a required run order within either file.

### 2.7 Integrations

- **SPEC-2.7.1**: `check-csp-host-derived-origin.sh`, `check-link-
  preview.sh`, and `check-oci-runtime.sh` must each start the real
  `caddy:2-alpine` serving stack (`oci-build/entrypoint.sh` plus
  `oci-build/Caddyfile`) in a Docker container to observe behavior that only
  exists once Caddy's `map` directives evaluate a real request, and must
  stop and remove every container they started, regardless of outcome.
- **SPEC-2.7.2**: `check-link-preview.sh` must additionally depend on the
  `curlimages/curl:8.16.0` image to issue its no-JS crawler requests, and
  must attempt a local pull of any required image it cannot find cached
  before failing.
- **SPEC-2.7.3**: `check-types-pin.sh` must integrate with the local Cargo
  git mirror of `mokosh-server` (under `~/.cargo/git/db/`) to decide whether
  `crates/mokosh-types` itself changed between two pinned revisions, and
  with the network (`cargo update --package mokosh-types`) to produce the
  candidate new lock in its default (non-`--compare`) mode; either
  dependency being unavailable must surface as `unknown` (see SPEC-2.4.7),
  not as a crash.
- **SPEC-2.7.4**: `check-ci-parity.sh` integrates `justfile` with
  `.forgejo/workflows/check.yml`, requiring the latter to run, verbatim,
  every command line the former's `check:` recipe depends on (see
  SPEC-2.5.22); this is the one script in this section whose subject is the
  relationship between two other files in this repository rather than
  between source code and a rendered or served artifact.
- **SPEC-2.7.5**: `config-docs.nu` integrates `config/variables.toml`,
  `oci-build/entrypoint.sh`, and `docs/configuration.md` with each other
  (see SPEC-2.2.1, SPEC-2.5.27) and with this repository's git tags (`vX.Y.Z`
  reachable from the current history) to decide which releases are
  "unreleased" versus already tagged when rendering or checking the
  "Recently added" window (SPEC-2.5.27) and when forming a
  `release-notes` link to `<repo>/src/tag/<ref>/...`.

### 2.8 Build and deployment

- **SPEC-2.8.1**: `justfile` must run every `check-*.sh` script that has a
  `--self-test` mode with `--self-test` before running it for real in the
  same recipe, so a guard whose own detection logic has been neutered fails
  the recipe before the real scan would have passed falsely.
- **SPEC-2.8.2**: `.forgejo/workflows/check.yml` must run every command
  that `justfile`'s `check:` recipe's dependencies run, enforced
  mechanically by `check-ci-parity.sh` (SPEC-2.5.22, SPEC-2.7.4).
- **SPEC-2.8.3**: `check-types-pin.sh`'s plain invocation (no `--compare`)
  must run `cargo update --package mokosh-types` only inside a temporary
  copy of the repository's `Cargo.lock`, and must restore the working
  tree's original `Cargo.lock` on every exit path (success, failure, or a
  parse/strict failure), so the guard never leaves a bumped lock file behind
  for the caller to accidentally commit; a failure to restore it must itself
  be reported as a guard failure (exit 2) naming the risk.
- **SPEC-2.8.4**: `config-docs.nu generate` is the only member of this
  section's scope that writes a file under version control
  (`docs/configuration.md`); every other script in this section is read-only
  with respect to the repository, writing only to a temporary directory it
  creates and removes.

### 2.9 Retention and cleanup

- **SPEC-2.9.1**: A `check-*.sh` script's `--self-test` mode, and
  `check-csp-host-derived-origin.sh`, `check-link-preview.sh`, and
  `check-oci-runtime.sh`'s real runs, must remove every temporary directory
  and Docker container they created on exit, via a `trap ... EXIT`, so a
  failing run leaves no fixture files or running containers behind.
- **SPEC-2.9.2**: `check-loading-recipe.sh`'s per-file baseline table
  (SPEC-2.2.4) is a ceiling, not a target: existing `"Loading` debt in a
  listed file must never be allowed to grow past its recorded count, and
  must trend toward zero as files are migrated to the shared skeleton
  components, but the guard does not itself require that migration.
- **SPEC-2.9.3**: `check-types-pin.sh` must discard the candidate
  `Cargo.lock` it produced for comparison once a run completes (see
  SPEC-2.8.3); no retained artifact of a `check-types-pin.sh` run may
  persist outside the restored `Cargo.lock` itself.

## 6. Identity, portal and org pages

Scope: `src/pages/accept_grant.rs`, `src/pages/admin_features.rs`,
`src/pages/auth_callback.rs`, `src/pages/button_showcase.rs`,
`src/pages/contact_portal/` (all files: `generic_login.rs`, `login.rs`,
`magic_link_login.rs`, `mod.rs`, `next_target.rs`, `picker.rs`,
`portal_branding.rs`, `portal_id_login.rs`, `set_password.rs`),
`src/pages/create_org.rs`, `src/pages/forgot_password.rs`,
`src/pages/home.rs`, `src/pages/invitations.rs`, `src/pages/login.rs`,
`src/pages/members.rs`, `src/pages/members_teams_tab.rs`,
`src/pages/mod.rs`, `src/pages/not_found.rs`, `src/pages/onboarding.rs`,
`src/pages/pick_tenant.rs`, `src/pages/platform_account.rs`,
`src/pages/platform_login.rs`, `src/pages/profile.rs`,
`src/pages/reset_password.rs`, `src/pages/set_password.rs`,
`src/pages/system_status.rs`, `src/pages/teams.rs`.

`src/pages/mod.rs` is a flat list of `pub mod` declarations with no
behavior of its own; it is covered by this section but owns no
`SPEC-6.` id. `src/pages/button_showcase.rs` is a static, dev-only
style reference with no form, no network call and no state beyond its
one route; it is covered by SPEC-6.1.19 only.

### 6.1 Interfaces

- **SPEC-6.1.1**: `StandaloneLogin` (`src/pages/login.rs`) must render the
  staff sign-in form only when the deployment has no OIDC issuer
  configured, with fields email, password, and, once the server reports
  it is required, an MFA code field and an approval code field, each
  shown only while its corresponding flag is set.
- **SPEC-6.1.2**: `AuthCallbackPage` (`src/pages/auth_callback.rs`) must
  render a transient "Signing you in..." state while the OIDC code
  exchange is in flight, and on failure a "Sign-in failed" state with
  the underlying error text and a "Try again" link back to
  `Route::Login`.
- **SPEC-6.1.3**: `PlatformLoginPage` (`src/pages/platform_login.rs`)
  must render a platform-administrator sign-in form (email, password)
  independent of the tenant sign-in form, and must show the most recent
  success banner, if any, above the form.
- **SPEC-6.1.4**: `PlatformAccountPage` (`src/pages/platform_account.rs`)
  must redirect to `Route::Login` instead of rendering when no platform
  bearer token is present, and otherwise must render a password-change
  card and an MFA card showing the platform admin's current MFA state.
- **SPEC-6.1.5**: `StandaloneForgotPassword` (`src/pages/forgot_password.rs`)
  must render an email field and, after submission, replace the form
  with a fixed confirmation screen, never a per-attempt result.
- **SPEC-6.1.6**: `StandaloneResetPassword` (`src/pages/reset_password.rs`)
  must render a new-password and confirm-password pair bound to a reset
  token taken from the route, and on success replace the form with a
  confirmation screen and a link back to sign-in.
- **SPEC-6.1.7**: `SetPasswordPage` (`src/pages/set_password.rs`) must
  resolve the owning tenant's name from its token before rendering its
  heading, falling back to a generic heading when that lookup fails,
  and must render the same new-password/confirm-password pair as
  SPEC-6.1.6.
- **SPEC-6.1.8**: `CreateOrgPage` (`src/pages/create_org.rs`) must
  redirect to `Route::Login` instead of rendering when no pending
  identity token is held from a prior sign-in, and otherwise must render
  a team-name field (required) and an optional portal-slug field.
- **SPEC-6.1.9**: `PickTenantPage` (`src/pages/pick_tenant.rs`) must
  redirect to `Route::Login` instead of rendering when no pending
  identity token and membership list are held, and otherwise must list
  every pending membership with its tenant name and role, each
  selectable.
- **SPEC-6.1.10**: `AdminFeaturesPage` (`src/pages/admin_features.rs`)
  must render a permission notice instead of the toggle list for a
  non-admin user, and for an admin must list every feature-toggle row
  with its current on/off state and a button that flips it.
- **SPEC-6.1.11**: `ContactGenericLoginPage` (`src/pages/contact_portal/generic_login.rs`)
  must render a single required Company ID field, exactly nine digits,
  with no link to the staff sign-in page.
- **SPEC-6.1.12**: `ContactLoginPage` and `ContactLoginByPortalIdPage`
  (`src/pages/contact_portal/login.rs`, `src/pages/contact_portal/portal_id_login.rs`)
  must render email, password and, once required, an MFA-code field for
  a slug- or portal-ID-scoped contact sign-in, and must replace the form
  with an unavailable-portal notice whenever the resolved tenant's
  status is not `"active"`.
- **SPEC-6.1.13**: `ContactMagicLinkLoginPage` (`src/pages/contact_portal/magic_link_login.rs`)
  must render a required Company ID field and a required, pre-fillable
  email field, and after submission must always show the same
  "Check your inbox" confirmation regardless of whether the address is
  registered.
- **SPEC-6.1.14**: `ContactPickerPage` (`src/pages/contact_portal/picker.rs`)
  must classify a redeemed login-link outcome, in order, as a forced
  password-setup redirect, an MFA prompt, a direct session install, or
  an invalid-link notice; it must never render the multi-company
  selection list the server's `candidates` payload could otherwise
  drive, showing instead a redirect to the Company ID entry page.
- **SPEC-6.1.15**: `ContactPortalBrandingPage` (`src/pages/contact_portal/portal_branding.rs`)
  must render the same no-access panel both when the signed-in contact
  lacks the `settings:manage_company_branding` capability and when the
  server declines to return the branding payload, so the two cases are
  visually indistinguishable to the contact.
- **SPEC-6.1.16**: `ContactSetPasswordPage` (`src/pages/contact_portal/set_password.rs`)
  must take its slug and token from route props, never from the URL
  query string, and on success must show a confirmation screen with an
  explicit "Go to sign in" action rather than redirecting automatically.
- **SPEC-6.1.17**: `MembersPage` (`src/pages/members.rs`) must expose
  three tabs, People, Teams and Invitations, selected by a `tab` query
  parameter defaulting to People for any unrecognized value, and must
  gate its People/Invitations mutation controls (invite, change role,
  remove, revoke) on an admin role distinct from the page's own
  manager-and-up view gate.
- **SPEC-6.1.18**: `TeamsTab` and its modals (`src/pages/members_teams_tab.rs`)
  must render a paginated team roster with create and edit affordances
  gated on an admin role, and the edit modal must render that team's
  member roster with add and remove affordances.
- **SPEC-6.1.19**: `ButtonShowcasePage` (`src/pages/button_showcase.rs`)
  is a static, dev-only button style reference behind the app's normal
  auth guard; it performs no network call and renders no business data.
- **SPEC-6.1.20**: `TeamsPage` (`src/pages/teams.rs`) must redirect an
  admin to `Route::MembersPage` with the Teams tab selected, and must
  show a permission notice, with no redirect, to a non-admin.
- **SPEC-6.1.21**: `InvitationsPage` (`src/pages/invitations.rs`) must
  render a permission notice for a non-admin user and, for an admin, an
  invite form (email, role, and, on an organization tenant only, a team
  picker) and a paginated table of pending invitations with a per-row
  revoke action.
- **SPEC-6.1.22**: `AcceptGrantPage` (`src/pages/accept_grant.rs`) must
  render exactly one of: a loading skeleton, an invalid-link notice, an
  email-mismatch notice, a retryable transient-error notice, or the
  invitation's accept/decline card, and must omit the accept/decline
  actions whenever the invitation's own status is not `"pending"`.
- **SPEC-6.1.23**: `HomePage` (`src/pages/home.rs`) must render a public
  landing page whose "Learn More" action scrolls the current page to
  its features section via script, never via an anchor `href`, because
  the desktop build's webview refuses navigation to any
  `dioxus://...#fragment` target after the first load.
- **SPEC-6.1.24**: `SystemStatusPage` (`src/pages/system_status.rs`) must
  report, independently, the client build's own version, the API
  server's reachability and version, and the dependency readiness
  checks it returns, and must never collapse a probe's success/failure
  state before rendering it.
- **SPEC-6.1.25**: `NotFoundPage` (`src/pages/not_found.rs`) must render
  the catch-all 404 content for any unmatched path, with links back to
  the dashboard, the home page and the knowledge base.

### 6.2 Data model

This scope owns no persisted application schema; it defines the wire
DTOs these pages exchange with the API server, and the client-side
storage keys listed under Security and Retention and cleanup below.

- **SPEC-6.2.1**: `members.rs`'s `MemberRow` must decode a row as either
  a native `User` or an `UnplacedGuest` by its `kind` tag, and every
  field but `user_id`/`grant_id` must default on absence so an older or
  newer server payload still decodes.
- **SPEC-6.2.2**: `members_teams_tab.rs`'s `UpdateTeamBody` must always
  serialize its `manager_id` field, including as `null`, because the
  server's update endpoint treats every present field as the value to
  set; omitting it would be read by the server as "leave unchanged,"
  not "clear."
- **SPEC-6.2.3**: `members.rs`'s role-change and remove actions must
  address a native user through `/users/{id}` and a guest, placed or
  not yet placed, through `/grants/{grant_id}`, because a guest's role
  and membership are owned by the grant record, not a users row.
- **SPEC-6.2.4**: `contact_portal/login.rs`'s and
  `contact_portal/portal_id_login.rs`'s `ContactSnippet` must default
  their `company_id` and `contact_id` fields on absence, so a contact
  plane response from a server older than the identifiers it carries
  still decodes.

### 6.3 Configuration

- **SPEC-6.3.1**: `login.rs` must persist a generated device identifier
  under the `mokosh_device_id` preference key and send it as `device_id`
  on every `/auth/login` call, generating and storing a new one only
  when none is already stored.
- **SPEC-6.3.2**: `login.rs` and `platform_login.rs` must store the
  platform-administrator bearer token under the sessionStorage key
  `mokosh:platform_token`, distinct from and never overwritten by a
  tenant sign-in.
- **SPEC-6.3.3**: `members.rs` and `invitations.rs` must each page their
  server list at 25 rows per page.
- **SPEC-6.3.4**: `contact_portal/next_target.rs` must store at most one
  pending post-sign-in target per tab, under the sessionStorage key
  `mokosh.portal.next`, capped at 512 characters.
- **SPEC-6.3.5**: `profile.rs`'s `PreferencesCard` must persist the time
  format, first-day-of-week and duration-format preferences to local
  storage under the stable keys `mokosh_time_format`,
  `mokosh_first_day_of_week` and the shared duration-format preference
  key, each with its own documented default, and must make no server
  call to do so.
- **SPEC-6.3.6**: `onboarding.rs`'s submit path, and every contact- and
  staff-plane page in this scope that performs a network call, must
  compile to a no-op on a build that does not enable the `app` or `web`
  feature it depends on, rather than attempting the call.

### 6.4 Security

- **SPEC-6.4.1**: `login.rs`'s `install_session`, `auth_callback.rs`, and
  every contact-plane sign-in page in this scope that installs a
  session (`contact_portal/login.rs`, `contact_portal/picker.rs`,
  `contact_portal/portal_id_login.rs`) must clear any session of the
  other plane (staff clears a contact session, contact clears a staff
  session) before writing the new plane's token, so a browser never
  holds a live staff and a live contact session together. `create_org.rs`
  and `pick_tenant.rs` write a staff token without this clear; that gap
  is tracked as a separate defect (see the PR body) and is not itself a
  requirement of this section.
- **SPEC-6.4.2**: `login.rs`'s platform-login-first attempt must be
  skipped whenever an MFA or approval code is already present, or
  whenever the current host resolves to a tenant subdomain, and its
  failure must fall through silently to the ordinary tenant login
  rather than surfacing a platform-specific error.
- **SPEC-6.4.3**: `auth_callback.rs` must strip the `code` and `state`
  query parameters from the browser URL before any further navigation,
  on both the success and the error path, so an OIDC authorization code
  never persists in browser history or is sent in a later `Referer`.
- **SPEC-6.4.4**: `auth_callback.rs`'s `classify_return_to` must restore
  a same-origin, same-scheme deep link via a hard navigation, and must
  collapse every other case (no target, off-origin, scheme mismatch, or
  an auth-plumbing route) to a soft navigation to the dashboard, never
  following an attacker-controlled off-origin target.
- **SPEC-6.4.5**: `auth_callback.rs`'s login restart must stop retrying
  once the attempt count exceeds its configured budget, returning a
  budget-spent error instead of restarting the flow again.
- **SPEC-6.4.6**: `forgot_password.rs` and `contact_portal/magic_link_login.rs`
  must discard the result of their password-reset/login-link request
  and always show the same confirmation text regardless of the
  outcome, so neither page discloses whether an address is registered.
- **SPEC-6.4.7**: `contact_portal/mod.rs`'s `PORTAL_SIGN_IN_FAILED`
  message, used on a failed contact sign-in across
  `contact_portal/login.rs` and `contact_portal/portal_id_login.rs`,
  must read identically whether or not the attempted email has an
  account, and must never contain an enumeration word ("no account",
  "not found", "unknown", "no such", "does not exist", "unrecognised",
  "unrecognized", "register") or an internal term ("platform",
  "tenant", "plane", "credential", "identity").
- **SPEC-6.4.8**: `contact_portal/next_target.rs`'s `sanitize` must
  accept only a same-origin path: non-empty, at most 512 characters, no
  control character, starting with exactly one `/` (never `//` or a
  backslash), carrying no URI scheme, and resolving to a real,
  non-catch-all route; it must reject the value without percent-decoding
  it first, so a percent-encoded bypass (for example `%2F%2Fevil.example`)
  is rejected as literal text rather than decoded into an open redirect.
- **SPEC-6.4.9**: `contact_portal/next_target.rs`'s stored next-target
  must be sanitized again on read as well as on write, and must be
  removed from storage on every read regardless of validity, so a
  rejected or stale value can never redirect a later, unrelated sign-in
  in the same tab.
- **SPEC-6.4.10**: Every contact-plane sign-in page in this scope
  (`contact_portal/login.rs`, `picker.rs`, `portal_id_login.rs`,
  `generic_login.rs`, `magic_link_login.rs`, `set_password.rs`) must
  navigate a signed-in contact through `next_target::landing`, never
  directly to the dashboard route, so the sanitized next-target is
  always honored.
- **SPEC-6.4.11**: `contact_portal/portal_id_login.rs` and
  `contact_portal/generic_login.rs` must carry no link to the staff
  sign-in page in their shipped rendering, so a contact cannot be
  routed from the portal into the staff sign-in surface.
- **SPEC-6.4.12**: `contact_portal/picker.rs`'s `classify_redeem` must
  treat a non-empty `password_setup_url` in the server's response as
  taking precedence over an MFA prompt or a direct session install,
  because a session cannot be installed from a login link whose token
  was never actually minted.
- **SPEC-6.4.13**: `contact_portal/picker.rs` must perform the
  server-supplied `password_setup_url` redirect as a hard, full-page
  navigation, never through the client-side router, because the router
  resolves only its own typed routes, not an arbitrary server-supplied
  path.
- **SPEC-6.4.14**: `admin_features.rs`'s client-side admin-role gate on
  the feature-toggle list is cosmetic only; the client must not treat
  it as sufficient authorization, and every mutating call it makes must
  rely on the server's own admin check.
- **SPEC-6.4.15**: `members.rs`'s page-level gate (manager and up) and
  its row-mutation gate (admin only) must remain distinct: a manager
  who is not an admin must be able to view the People, Teams and
  Invitations tabs but must see no invite, role-change, remove or
  revoke control.
- **SPEC-6.4.16**: `contact_portal/portal_branding.rs`'s error text for
  a denied or failed load must never name a capability, role, status
  code, or other implementation detail to the contact.
- **SPEC-6.4.17**: `onboarding.rs` must gate its admin-only organization
  form on the server-confirmed role (`server_loaded`), not on the
  optimistic default role a fresh session starts with, so an admin
  whose role has not yet been reconciled from `/auth/me` is shown the
  loading state rather than the non-admin screen.
- **SPEC-6.4.18**: `onboarding.rs` must not call `POST /auth/me/complete-onboarding`
  when the preceding organization save for an admin failed; the submit
  sequence must stop at the first failed call in its plan.
- **SPEC-6.4.19**: `profile.rs`'s `UpdateMeRequest` must omit
  `first_name`, `last_name` and `phone`, since those fields are owned
  by the identity provider once one is configured, and must include
  `mobile`.
- **SPEC-6.4.20**: `profile.rs`'s MFA setup, enable and disable calls
  must each be gated on the deployment having no OIDC issuer configured,
  and the disable action must require the current password together
  with a live authenticator or recovery code.
- **SPEC-6.4.21**: `platform_account.rs`'s password-change and MFA
  setup/enable/disable actions must each require the current platform
  admin password to be resubmitted, and must be disabled, with an
  explanatory tooltip, whenever the server is unreachable.
- **SPEC-6.4.22**: `accept_grant.rs` must treat a `403` from the
  invitation-metadata lookup as the signed-in user not matching the
  invitation's invited address, and must render that case as a distinct,
  non-actionable notice rather than retrying or silently failing.

### 6.5 Error handling and observability

- **SPEC-6.5.1**: `login.rs` must show "Enter your email and password."
  without sending a request when either field is empty, and must map a
  401 while an approval code is pending to
  "That code is not valid, check the email or try signing in again.",
  a 401 otherwise to "Invalid email or password.", and a successful
  response carrying no user to
  "Sign-in succeeded but no account was returned. Try again."
- **SPEC-6.5.2**: `platform_login.rs` must show
  "Enter your platform admin email and password." on an empty field,
  and must map a 401 to "Invalid email or password.", clearing any
  prior success banner on every new submit attempt.
- **SPEC-6.5.3**: `reset_password.rs`, `set_password.rs` and
  `contact_portal/set_password.rs` must reject a password shorter than
  12 characters with "Password must be at least 12 characters." and a
  mismatched confirmation with "Passwords do not match.", and must map
  a 400 with an empty server message to a page-specific
  invalid-or-expired-link message, otherwise showing the server's
  message verbatim; `contact_portal/set_password.rs` must additionally
  map a 410 with an empty server message to
  "This setup link has already been used.".
- **SPEC-6.5.4**: `create_org.rs` must map a 401 on team creation to
  clearing the pending identity token and returning to sign-in, a 409
  to the server's conflict message verbatim, and a 400 to the server's
  validation message verbatim.
- **SPEC-6.5.5**: `pick_tenant.rs` must map a 401 on tenant selection to
  clearing the pending identity token and returning to sign-in, a 404
  to "You do not have access to that organization. Pick another.", and
  a successful response carrying no user to
  "Sign-in succeeded but no account was returned."; `create_org.rs`
  must map that same no-user case on team creation to
  "Team created but no session was returned."
- **SPEC-6.5.6**: `admin_features.rs` must log a warning and render
  "Could not load feature toggles. Refresh the page to retry." when the
  toggle list fails to load, and must render
  "No feature toggles declared." when it loads with zero rows.
- **SPEC-6.5.7**: `contact_portal/generic_login.rs`'s `classify_verify`
  must map a 404 Company ID lookup to
  "Company ID not found. Check the number your MSP sent you." and any
  transport failure or other status to
  "Could not reach the portal service. Check your connection and try again."
- **SPEC-6.5.8**: `contact_portal/login.rs` and
  `contact_portal/portal_id_login.rs` must log a warning and continue
  rendering the sign-in form when their tenant host-hint lookup fails,
  rather than blocking the form on that failure.
- **SPEC-6.5.9**: `contact_portal/picker.rs` must collapse any redeem or
  select failure, of any kind, to the single invalid-or-expired-link
  notice, matching the server's intentionally opaque response for an
  invalid, expired, replayed or revoked login link.
- **SPEC-6.5.10**: `members.rs` must log an error and render
  "Could not load the member list." when the member list fails to
  load, "Nobody has access to this workspace yet." when it loads with
  zero rows, and a warning banner
  "Guest list unavailable: the identity service could not be reached. Native users are shown; guests may be missing."
  whenever the server reports its guest-identity lookup as unreachable.
- **SPEC-6.5.11**: `members.rs` must report a failed role change as
  "Could not change role: {error}" and a failed remove as
  "Could not remove: {error}", each scoped to its own control rather
  than the page's other actions; its Invitations tab must report a
  failed invitation list load as
  "Couldn't load pending invitations. Refresh to try again." and a
  failed cancel as "Could not cancel: {error}".
- **SPEC-6.5.12**: `members_teams_tab.rs` must report an empty team name
  as "Team name is required." on both create and edit, and a save
  failure with the server's own message.
- **SPEC-6.5.13**: `invitations.rs` must distinguish a true outage (the
  list failed to load and the server is unreachable) from an ordinary
  empty or failed list: only the former replaces the page with a
  full-page unavailable notice.
- **SPEC-6.5.14**: `accept_grant.rs` must report an accept failure and a
  decline failure each through its own error banner using the server's
  message, and on either success must show its own confirmation text
  ("You accepted the invitation..." or "You declined the invitation...")
  before navigating to the dashboard.
- **SPEC-6.5.15**: `onboarding.rs` must report a failed organization
  save either against the `name` field, if the server scopes its error
  to that field, or as a page-level "Could not save: {error}" otherwise,
  and must report a failed logo upload as a non-blocking warning toast
  while still completing onboarding.
- **SPEC-6.5.16**: `system_status.rs` must render each of the client
  build, API server and dependency-readiness cards independently: a
  "Checking..." state while its own probe is pending, and on failure a
  card-specific message naming the error, without affecting the other
  cards' state.
- **SPEC-6.5.17**: `not_found.rs` must render the unmatched path
  segments whenever the joined path is non-empty, in every build; its
  own comment's claim that this is development-only does not reflect
  the code and is tracked as a separate defect (see the PR body).

### 6.6 Concurrency and ordering

- **SPEC-6.6.1**: `login.rs`, `platform_login.rs`, `forgot_password.rs`,
  `reset_password.rs`, `set_password.rs`, `create_org.rs`,
  `pick_tenant.rs`, and every contact-plane sign-in page in this scope
  must ignore a submit while a previous submit from the same form is
  still in flight, rather than issuing a second concurrent request.
- **SPEC-6.6.2**: `members.rs`'s member search must debounce 300
  milliseconds after the last keystroke before refetching, and any
  change to the search text, role filter, kind filter or team filter
  must reset the current page to 1.
- **SPEC-6.6.3**: `members.rs` must reset the current page to 1 after a
  successful remove, so removing the last row of a page beyond page 1
  never strands the view on a now-empty page.
- **SPEC-6.6.4**: `onboarding.rs`'s submit must perform its planned
  calls (organization save, then logo upload if one was chosen, then
  complete-onboarding) strictly in that order, and must abort the
  remaining calls as soon as the organization save fails.
- **SPEC-6.6.5**: `contact_portal/login.rs`'s on-mount redirect to the
  portal-ID-scoped route (when the slug resolves to one) and its
  separate host-hint fetch for branding both fire on mount; only the
  redirect navigates away, and it must take effect whenever it
  succeeds regardless of whether the host-hint fetch has yet resolved.
- **SPEC-6.6.6**: `onboarding.rs`'s redirect for an already-completed
  profile and `teams.rs`'s redirect for an admin visiting the retired
  teams route must run on every render as an effect that no-ops rather
  than being skipped, so the number of hooks the component runs stays
  constant across renders.

### 6.7 Integrations

- **SPEC-6.7.1**: `login.rs` must call, as applicable, `POST /platform/login`,
  `POST /auth/login`, and `POST /auth/select-tenant`, in that order for
  the chained platform-then-tenant attempt.
- **SPEC-6.7.2**: `auth_callback.rs` must exchange the OIDC
  authorization code for tokens against the configured issuer for the
  current origin before installing any session.
- **SPEC-6.7.3**: `platform_login.rs` must call `POST /platform/login`;
  `platform_account.rs` must call `PUT /platform/me/password`,
  `POST /platform/me/mfa/setup`, `POST /platform/me/mfa/enable` and
  `POST /platform/me/mfa/disable`.
- **SPEC-6.7.4**: `forgot_password.rs` must call `POST /auth/forgot-password`;
  `reset_password.rs` and `set_password.rs` must call
  `POST /auth/reset-password`, and `set_password.rs` must first call
  `GET /auth/set-password/context/{token}` to resolve the owning
  tenant's name.
- **SPEC-6.7.5**: `create_org.rs` must call `POST /tenants/self-serve`;
  `pick_tenant.rs` must call `POST /auth/select-tenant`.
- **SPEC-6.7.6**: `admin_features.rs` must call `GET /settings/feature-toggles`
  and `PUT /settings/feature-toggles/{key}`.
- **SPEC-6.7.7**: `contact_portal/generic_login.rs` must call
  `GET /contact/portal/{portal_id}/host`;
  `contact_portal/login.rs` and `contact_portal/portal_id_login.rs`
  must call `GET /contact/portal/{slug|portal_id}/host` and
  `POST /contact/auth/login`; `contact_portal/login.rs` must also call
  `GET /contact/portal/{slug}/resolve-to-portal-id`.
- **SPEC-6.7.8**: `contact_portal/magic_link_login.rs` must call
  `POST /contact/auth/login-link`; `contact_portal/picker.rs` must call
  `POST /contact/auth/login-link/redeem` and
  `POST /contact/auth/login-link/select`, reusing each endpoint for its
  MFA follow-up request.
- **SPEC-6.7.9**: `contact_portal/portal_branding.rs` must call
  `GET /contact/companies/self/branding` and
  `PATCH /contact/companies/self/branding`;
  `contact_portal/set_password.rs` must call
  `GET /contact/portal/{slug}/host` and `POST /contact/auth/set-password`.
- **SPEC-6.7.10**: `members.rs` must call `GET /members`,
  `GET /teams` (for its team filter), `POST /invitations`,
  `PUT /users/{id}`, `PATCH /grants/{id}`, `DELETE /grants/{id}`,
  `GET /invitations` and `DELETE /invitations/{id}`.
- **SPEC-6.7.11**: `members_teams_tab.rs` must call `GET /teams`,
  `POST /teams`, `PUT /teams/{id}`, `GET /teams/{id}/members`,
  `POST /teams/{id}/members` and `DELETE /teams/{id}/members/{uid}`.
- **SPEC-6.7.12**: `invitations.rs` must call `GET /invitations`,
  `POST /invitations`, `DELETE /invitations/{id}` and, on an
  organization tenant, `GET /teams`.
- **SPEC-6.7.13**: `accept_grant.rs` must call
  `GET /grants/invitations/by-token/{token}`,
  `POST /grants/invitations/by-token/{token}/accept` and
  `POST /grants/invitations/by-token/{token}/decline`.
- **SPEC-6.7.14**: `profile.rs` must call `GET /auth/me`,
  `PUT /auth/me`, `POST /me/mfa/setup`, `POST /me/mfa/enable`,
  `POST /me/mfa/disable`, and, for a contact session,
  `GET /contact/auth/me` and `PUT /contact/auth/me`.
- **SPEC-6.7.15**: `onboarding.rs` must call `PUT /tenants/current`,
  `PUT /tenants/current/logo` and `POST /auth/me/complete-onboarding`.
- **SPEC-6.7.16**: `system_status.rs` must call `GET /version` and
  `GET /ready` to populate its API-server and dependency cards.

### 6.8 Build and deployment

None of the files in this section's scope add a build or packaging
step of their own; they compile under the same `app`/`web`/`desktop`
feature gating `src/main.rs` and `Cargo.toml` define (see SPEC-1.3.1).
A page or helper in this scope that performs a network call must be
gated on the feature its target requires (`app` for logic shared by web
and desktop, `web` for a wasm-only call), consistent with the pattern
cited at SPEC-6.3.6.

### 6.9 Retention and cleanup

- **SPEC-6.9.1**: `create_org.rs` and `pick_tenant.rs` must clear the
  pending-login signal (identity token and candidate memberships) on a
  successful sign-in, on a 401 indicating the identity token expired,
  and on an explicit "Back to sign in" action; no other path may leave
  it populated.
- **SPEC-6.9.2**: `members.rs` must implement "remove" differently by
  row kind: a native user's removal deactivates the user record
  (`status = inactive`, reversible by reactivation), while a guest's
  removal, placed or not, revokes the grant outright (not reversible;
  re-invitation is required).
- **SPEC-6.9.3**: `invitations.rs`'s revoke action must delete the
  invitation outright with no undo, and its confirmation text must say
  so before the request is sent.
- **SPEC-6.9.4**: `contact_portal/next_target.rs` must retain its
  pending next-target for at most the current tab's session and must
  consume it (remove it from storage) the first time it is read,
  regardless of whether that read's value was valid.

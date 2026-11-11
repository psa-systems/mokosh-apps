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
`config/variables.toml`, and every file under `oci-build/`), and the CI
workflows under `.forgejo/workflows/`.

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
  `check-cancel-routes.sh`, and `check-csp-host-derived-origin.sh` carry
  no `--self-test` mode (the first two have no synthetic fixture to
  construct; `check-cancel-routes.sh`'s
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
  the Rust-build job in `check.yml` (the job named `cargo`) requesting the
  heavy runner label in its own `runs-on`, not merely the label appearing
  somewhere in the file; no workflow referencing a retired `*_LATEST`
  runner label; and `check.yml` not installing OS packages at run time. A
  missing `check.yml`, or no `cargo` job with a `runs-on`, must exit 2
  (SPEC-2.5.1).
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
  holds a live staff and a live contact session together.
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
  segments only in `debug_assertions` builds, and only when the joined
  path is non-empty. Release builds show the generic "Page not found"
  copy with no path echo.

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

## 7. Contacts and companies

Scope: `src/pages/contacts.rs`, `src/pages/contact_provenance.rs`,
`src/pages/company_documentation.rs`, `src/pages/company_role_edit.rs`.

### 7.1 Interfaces

- **SPEC-7.1.1**: `CompanyListPage` must render a paginated, searchable,
  filterable (type, status) table of companies, defaulting the status
  filter to `active`, and must distinguish, with a "Clear filters"
  recovery action, a filtered-to-nothing result from a truly empty
  account (`contacts.rs:551-813`).
- **SPEC-7.1.2**: `CompanyNewPage` and `CompanyEditPage` must render the
  shared `CompanyForm` in Create or Edit mode respectively, `CompanyEditPage`
  loading its initial values from `GET /contacts/companies/{id}` and
  showing breadcrumbs that fall back to a generic "Company" label while
  the record name is not yet loaded (`contacts.rs:875-1054`).
- **SPEC-7.1.3**: `CompanyDetailPage` must render the company record plus
  independent cards for its contacts, sites, tickets, contracts,
  projects, invoices, assets, opportunities, knowledge-base articles,
  backup status, portal access, roles, and branding, each card
  resolving from its own resource (`contacts.rs:2176-6193`).
- **SPEC-7.1.4**: `CompanyRoleEditPage` (`company_role_edit.rs:107-251`)
  must render, at `/companies/{company_id}/roles/{id}`, a create form
  when `id == "new"` and otherwise fetch and edit the existing
  Company-scoped portal role, rendering a distinct "This role doesn't
  exist." state on a 404 and a distinct "Could not load this role."
  state on any other fetch failure.
- **SPEC-7.1.5**: `ContactListPage` must render a paginated, searchable
  table of contacts with the same filtered-vs-empty distinction as
  SPEC-7.1.1 (`contacts.rs:6194-6567`).
- **SPEC-7.1.6**: `ContactNewPage` and `ContactEditPage` must render the
  shared `ContactForm` in Create or Edit mode respectively;
  `ContactNewPage` must prefill the linked company, in order, from the
  session-storage stash described at SPEC-7.3.2, then the live URL
  query string, then the OIDC-flow's captured initial query string,
  falling back to no prefill only when all three carry no company id
  (`contacts.rs:6568-6762`).
- **SPEC-7.1.7**: `ContactDetailPage` must render the contact record plus
  independent cards for its tickets, ticket-comment notes, portal access
  and access requests, provenance (via `ImportCard`, SPEC-7.1.9), and the
  billing-contact relationship of each company it is linked to
  (`contacts.rs:7803-11200` and surrounding card components).
- **SPEC-7.1.8**: `CompanyDocumentationPage` (`company_documentation.rs:90-141`)
  must render one screen composing a company's assets, credential
  references, and knowledge-base articles from a single
  `GET /companies/{company_id}/documentation` call, rendering a single
  "Nothing is documented for this client yet." card only when all three
  lists are empty, and otherwise one card per list.
- **SPEC-7.1.9**: `ImportCard` (`contact_provenance.rs`) must render
  nothing when a contact carries no provenance link, and otherwise must
  render the current link's origin, account or file source, and any
  field locks, with an "Unlink" action shown only while the link is
  still live and a "Remove imported data" action shown only to an admin
  (`contact_provenance.rs:216-607`, exact range per the component body).
- **SPEC-7.1.10**: The company list's `Company` column and the contact
  list's `Name` column are the only sortable columns either list exposes
  (`CompanySortKey`, `ContactSortKey`, `contacts.rs:111-131`), because no
  other column's key is in the server's sort allow-list.

### 7.2 Data model

This scope owns no persisted application schema; it defines the wire
DTOs these pages exchange with the API server.

- **SPEC-7.2.1**: `RemoteCompany` and `RemoteContact` (`contacts.rs:256-382`)
  must default every field but `id` on absence, so a server response
  missing a field (an older server, or a field this page does not use)
  still decodes instead of failing the whole list.
- **SPEC-7.2.2**: `RemoteContact.phones`/`companies` must stay the
  authoritative list for a contact's phone numbers and company links;
  the mirrored `phone`/`company_id`/`company_name` fields on the same
  struct are read only as a fallback when the list is empty, for a
  server that predates PMS-806 (`contacts.rs:342-382`).
- **SPEC-7.2.3**: `CompanyFormBody`, `ContactFormBody`, `UpdateCompanyBody`,
  `UpdateContactBody`, `SiteFormBody`, `GrantPortalAccessBody`,
  `ResolveAccessRequestBody`, and `CreateCompanyFromNameBody`
  (`contacts.rs:401-547`) must each mirror the corresponding
  `mokosh-types` request DTO field for field; a `#[cfg(test)]` module
  in `contacts.rs` must destructure every such DTO against the matching
  body struct so that a server-side field add, remove, rename, or
  retype fails this crate's build rather than silently 422ing at
  runtime (MAPPS-687, `contacts.rs:11381-12132`).
- **SPEC-7.2.4**: `UpdateCompanyBody` and `UpdateContactBody` must omit a
  field the firing control did not set (via `skip_serializing_if`)
  rather than sending it as JSON `null`, except `CompanyFormBody.fax`,
  which carries no such attribute and so sends an explicit `null` when
  the field is cleared, matching the server's distinction between "not
  present" (leave unchanged) and "present as null" (clear) on that one
  field (`contacts.rs:407-442`, `contacts.rs:1708-1710`).
- **SPEC-7.2.5**: `PortalRoleDetailWire` (`company_role_edit.rs:53-64`)
  must decode `company_id` as present on a Company-scoped role; the
  page performs no server-side scope check of its own and relies on the
  nested endpoint path to exclude a tenant-wide row.
- **SPEC-7.2.6**: `Provenance` (`contact_provenance.rs`) must treat
  `links.first()` as the contact's current import link, relying on the
  server to return the live link first (or, absent one, the most recent)
  rather than re-deriving "current" client-side.
- **SPEC-7.2.7**: `CompanyDocumentation`, `DocAsset`, `DocCredential`,
  and `DocArticle` (`company_documentation.rs:30-82`) must decode every
  field but each type's `id`/`name`/`title` on absence; `DocCredential`
  must declare no field capable of carrying an encrypted secret
  (password, secret data, or username), so the aggregate endpoint's
  payload cannot carry one at the type level.

### 7.3 Configuration

- **SPEC-7.3.1**: The company and contact list pages must page their
  server list at `PER_PAGE = 25` rows (`contacts.rs:23`), and the
  company-detail page's child-list previews (contacts, tickets,
  contracts, projects, invoices, assets) must fetch 5 rows but render at
  most 3, each with a "View all" link to the full filtered list
  (`contacts.rs` card components, e.g. `contacts.rs:3336-3511` and the
  5213-6193 range).
- **SPEC-7.3.2**: `ContactNewPage`'s company prefill must be written to
  session storage by `stash_contact_new_prefill` before navigation and
  read exactly once by `take_stashed_contact_new_prefill` on mount,
  because the Dioxus 0.7 router strips a `?company_id=...` query string
  via `history.replaceState()` on mount for a route with no declared
  query parameters (`contacts.rs:6568-6762` and the stash helpers it
  calls).
- **SPEC-7.3.3**: Every network call in this scope must compile to a
  no-op (or, for a `use_resource` body, resolve to its failure/empty
  variant) on a build that does not enable the `web` feature it
  depends on, consistent with SPEC-6.3.6; `company_role_edit.rs` gates
  each of its fetch and submit paths individually behind
  `#[cfg(feature = "web")]` (`company_role_edit.rs:128-165`, `390-471`).
- **SPEC-7.3.4**: The company form's Address State field must offer
  exactly the 55 US states, territories, and the District of Columbia
  listed in `US_STATES` (`contacts.rs:29-82`), keyed by USPS code.
- **SPEC-7.3.5**: The contact form's phone-type select must offer
  exactly the five values in `PHONE_TYPES` (mobile, work, home, fax,
  other), matching PMS-806's `contact_phones.phone_type` CHECK
  constraint (`contacts.rs:1846-1852`).

### 7.4 Security

- **SPEC-7.4.1**: `validate_website_field` (`contacts.rs:1999-2045`) must
  reject any value containing whitespace or a control character, must
  reject any explicit scheme other than `http`/`https` (rejecting
  `javascript:`, `data:`, `vbscript:`, `mailto:`, and any other scheme),
  must require a non-empty host when a `http`/`https` scheme is given,
  and must normalize a scheme-less, host-shaped value (at least two
  non-empty, dot-separated, host-charset labels, `is_host_like`,
  `contacts.rs:2056-2065`) to `https://<value>`; every other case must
  return the error "Website must be a domain or http(s) URL (e.g.
  example.com or https://example.com)." before any request is sent.
- **SPEC-7.4.2**: `safe_href` must gate every company website link
  rendered on `CompanyDetailPage` before it is used as an anchor `href`
  (`contacts.rs`, the company sidebar website link), so a value that
  slipped past SPEC-7.4.1 (an older stored record) cannot run as script
  at render time.
- **SPEC-7.4.3**: `contact_provenance.rs`'s `release_lock_disabled(busy,
  can_mutate, can_manage)` must be `true` whenever the field release is
  already in flight, the server is unreachable, or the signed-in user
  lacks a manager-or-above role, mirroring the server's `RequireManager`
  guard on `DELETE /contacts/contacts/{id}/sync/locks/{field}`; a
  technician session must not be able to release a field lock even when
  `can_mutate` is true (`contact_provenance.rs:592-594`).
- **SPEC-7.4.4**: `ImportCard`'s "Remove imported data" action must be
  rendered only for an admin session, and its confirm dialog must
  require a non-empty reason before the removal request fires; neither
  gate may be bypassed by a direct call to the handler
  (`contact_provenance.rs`, `ImportCard`'s `remove` handler and its
  confirm dialog).
- **SPEC-7.4.5**: No request body in this scope may be constructed as a
  `serde_json::json!` literal; every `POST`/`PUT`/`PATCH` body must be a
  typed struct destructured against its DTO per SPEC-7.2.3. A
  `#[cfg(test)]` source scan (`contacts.rs:12171-12262`) enforces this
  for the shipping portion of `contacts.rs`.
- **SPEC-7.4.6**: A Company-scoped portal role's `company_id` must never
  be sent on `PUT /contacts/companies/{company_id}/portal-roles/{id}`
  (`UpdateCompanyRoleBody`, `company_role_edit.rs:81-85`); a role's
  scope is immutable once created and the server rejects a scope change
  with 400.
- **SPEC-7.4.7**: `CompanyPortalAccessCard`'s "Invite" action must be
  disabled for a contact with no email address, and every portal-access
  mutation (grant, resend invite, revoke, change roles) on this scope's
  pages must be disabled whenever `!can_mutate` (the server is
  unreachable), consistent with SPEC-1's reachability gating.
- **SPEC-7.4.8**: `PortalGrantOutcomeWire` (`portal_slug`, `portal_id`,
  `password_email_queued: bool`) must carry no setup link or other
  credential-bearing field; `password_email_queued` tells the caller
  whether a setup email was queued without ever carrying the token
  itself, and no response rendered by the portal-access cards may
  surface a setup link to the screen.
- **SPEC-7.4.9**: `DocCredential` (`company_documentation.rs:58-68`)
  must carry no password, secret, or username field; a credential's
  secret value is reachable only through the separate, independently
  audited credential-vault reveal endpoint, never through the
  documentation aggregate.

### 7.5 Error handling and observability

- **SPEC-7.5.1**: Every list and detail resource in this scope that
  fails to load while the server remains reachable must render
  "Could not load {noun}." (companies, contacts, sites, tickets,
  contracts, projects, invoices, assets, roles, opportunities, articles,
  backup status) scoped to its own card, never collapsing the whole
  page; a failure while the server is unreachable must instead render
  `ContentUnavailable` for the page's primary resource, consistent with
  the MAPPS-357 pattern already stated at SPEC-6's error-handling
  sections.
- **SPEC-7.5.2**: `CompanyForm` and `ContactForm`'s `handle_submit` must
  collect every field's validation failure via `FormGuard` in one pass
  rather than stopping at the first invalid field, and must move focus
  to the first invalid field in on-screen order (`contacts.rs:1298-1500`,
  `contacts.rs:7220-7420`).
- **SPEC-7.5.3**: A 409 on a company or contact save must be mapped to
  a name-field conflict message ("Could not save company: ..." /
  "Could not save contact: ...", carrying the server's message); a 422
  whose `field_errors()` names a field on the form must route to that
  field's inline error, and any remaining, unmapped error must route to
  the form's top error banner rather than being dropped
  (`contacts.rs:1484-1510`, `contacts.rs:7389-7420`).
- **SPEC-7.5.4**: `SiteFormModal.handle_save` must validate every field
  via `FormGuard` and report every failure at once; a `company_id` or
  Site `name` parse failure, which has no dedicated inline slot, must
  route to the modal's banner ("...could not be read, so nothing was
  saved."), while phone, timezone, postal, and country failures use
  their own per-field inline slots (`contacts.rs:4938-5032`).
- **SPEC-7.5.5**: `ContactDetailPage`'s notes resource (`contacts.rs:7864`)
  must keep the fetch's `Result`, not discard it via `.ok()`; `ContactNotesCard`
  (`contacts.rs:8577`) must render "Could not load comments for this
  contact: {err}" on a failed load and the distinct text "This contact
  has not commented on any tickets yet." on a genuinely empty feed.
- **SPEC-7.5.6**: No awaited fetch result in the shipping portion of
  `contacts.rs` may be discarded via `.ok()` without first being logged
  through `.inspect_err(...)`; a `#[cfg(test)]` source scan
  (`contacts.rs:11300`, module `mapps692_fetch_error_logging_tests`)
  enforces this over the whole file.
- **SPEC-7.5.7**: `assign_portal_roles_no_roles_configured`
  (`contacts.rs:8712`) must return `false` while the role list is still
  loading (pending) or has failed to load, and `true` only once the
  list has resolved successfully and is empty, so a role-assignment
  submit button is never disabled on the basis of a fetch that has not
  yet told the page there are no roles.
- **SPEC-7.5.8**: `CompanyRoleEditPage` must distinguish a 404 on the
  role fetch (rendered as "This role doesn't exist." with a link back
  to the Company page) from any other fetch failure (rendered as "Could
  not load this role. Refresh the page to retry.", or, while the server
  is unreachable, `ContentUnavailable`) (`company_role_edit.rs:184-231`).
- **SPEC-7.5.9**: `CompanyRoleEditForm`'s submit must reject an empty
  name with "Name is required.", a name over 64 characters with "Name
  must be 64 characters or fewer.", and, on create only, an empty
  capability selection with "Pick at least one capability.", each
  checked before any request is sent (`company_role_edit.rs:364-381`).
- **SPEC-7.5.10**: `CompanyDocumentationPage` must render a
  `DetailSkeleton` while its fetch is pending, the server's own message
  via `ErrorBanner` ("Could not load documentation: {e.user_message()}")
  on failure, and the distinct empty-state copy described at
  SPEC-7.1.8 only when all three lists resolve empty
  (`company_documentation.rs:109-140`).

### 7.6 Concurrency and ordering

- **SPEC-7.6.1**: `CompanyDetailPage` and `ContactDetailPage` must run
  each card's data as an independent `use_resource`, so one card's
  failure or slow load must not block or delay another card's render
  (`contacts.rs:2176-3089`, `contacts.rs:7803-8842`).
- **SPEC-7.6.2**: `CompanyDetailPage`'s deletion-preview resource must
  fetch only while the delete-confirmation dialog is open, never
  eagerly on page load (`contacts.rs`, the `deletion_preview` resource
  near `contacts.rs:2448-2470`).
- **SPEC-7.6.3**: The company and contact list pages must debounce
  their search input 300 milliseconds after the last keystroke before
  issuing a request, and any change to the search text, type filter, or
  status filter must reset the current page to 1 (`contacts.rs:594-601`,
  mirroring SPEC-6.6.2).
- **SPEC-7.6.4**: `CompanyPortalAccessCard` must isolate its per-contact
  "toggling" loading state to a `HashSet<Uuid>` keyed by contact id, so
  toggling one row's portal state must not show every row as loading.
- **SPEC-7.6.5**: `CompanySitesCard`'s save and delete handlers must
  restart both the sites resource and the company resource on success,
  so the company detail page's site count updates without a manual
  reload (`contacts.rs:4810-4835`).
- **SPEC-7.6.6**: `CompanyRoleEditForm`'s and `CompanyBrandingCard`'s
  submit handlers must ignore a second submit while a previous submit
  from the same form is still in flight (`saving`/`can_mutate` guard),
  consistent with SPEC-6.6.1.
### 7.7 Integrations

- **SPEC-7.7.1**: The company pages in this scope must call
  `GET /contacts/companies`, `POST /contacts/companies`,
  `GET /contacts/companies/{id}`, `PUT /contacts/companies/{id}`,
  `GET /contacts/companies/{id}/deletion-preview`,
  `GET /contacts/companies/{id}/contacts`,
  `GET /contacts/companies/{id}/sites`, `POST /contacts/sites`,
  `PUT /contacts/sites/{id}`, `DELETE /contacts/sites/{id}`,
  `GET /contacts/company-industries`, and `GET /companies/website-probe`.
- **SPEC-7.7.2**: The contact pages in this scope must call
  `GET /contacts/contacts`, `POST /contacts/contacts`,
  `GET /contacts/contacts/{id}`, `PUT /contacts/contacts/{id}`,
  `GET /contacts/{id}/notes`, `GET /contacts/contacts/{id}/sync`,
  `POST /contacts/contacts/{id}/grant-portal-access`,
  `POST /contacts/contacts/{id}/resend-portal-invite`,
  `POST /contacts/contacts/{id}/revoke-portal-access`,
  `GET /contacts/contacts/{id}/access-requests`, and
  `POST /contacts/contacts/access-requests/{id}/resolve`.
- **SPEC-7.7.3**: The portal-roles surfaces in this scope must call
  `GET /contacts/portal-roles`, `GET /contacts/companies/{id}/portal-roles`,
  `GET /contacts/companies/{company_id}/portal-roles/{id}`,
  `POST /contacts/companies/{company_id}/portal-roles`, and
  `PUT /contacts/companies/{company_id}/portal-roles/{id}`
  (`company_role_edit.rs:9-12`), plus the shared
  `GET /portal-roles/capabilities`.
- **SPEC-7.7.4**: `CompanyDetailPage`'s child-relationship cards must
  call `GET /tickets?company_id={id}`, `GET /contracts?company_id={id}`,
  `GET /projects?company_id={id}`, `GET /invoices?company_id={id}`,
  `GET /assets?company_id={id}`, `GET /crm/opportunities?company_id={id}&closed=false`,
  `GET /kb/companies/{company_id}`, `GET /status/companies/{id}/backup`,
  and `GET /tenants/current` (for branding defaults).
- **SPEC-7.7.5**: `contact_provenance.rs`'s `ImportCard` must call
  `DELETE /contacts/contacts/{id}/sync/locks/{field}`,
  `POST /contacts/contacts/{id}/sync/unlink`, and
  `POST /contacts/contacts/{id}/sync/remove-imported-data`.
- **SPEC-7.7.6**: `CompanyDocumentationPage` must call
  `GET /companies/{company_id}/documentation`.

### 7.8 Build and deployment

None of the files in this section's scope add a build or packaging step
of their own; they compile under the same `app`/`web`/`desktop` feature
gating `src/main.rs` and `Cargo.toml` define (see SPEC-1.3.1), per the
per-call gating described at SPEC-7.3.3.

### 7.9 Retention and cleanup

- **SPEC-7.9.1**: A company's own tenant record is never offered for
  deletion; the delete-confirmation dialog on `CompanyDetailPage` must
  instead offer "Archive instead" (`PUT /contacts/companies/{id}` with
  `status = inactive`) whenever the deletion-preview response reports
  the record as blocked from deletion, and must offer no archive bypass
  at all for the tenant's own company record.
- **SPEC-7.9.2**: `ImportCard.unlink()` must leave the contact record
  itself untouched (a local record going forward, no longer updated by
  a later sync); `ImportCard.remove()` must delete the contact outright
  only when the server reports `contact_deleted == true` (the import
  created the record and no other data was added since), and otherwise
  must keep the contact and remove only what the import attached,
  reporting each outcome with its own distinct success toast.
- **SPEC-7.9.3**: `ImportCard.remove()`'s confirmation text and the
  server's own removal behavior must agree that the link and
  review-queue data are deleted, that the record will not be
  re-imported by a later sync, that the audit log retains a record of
  the removal event (not the removed details), and that the contact's
  tickets and invoices are never touched by the removal.
- **SPEC-7.9.4**: `ContactNewPage`'s stashed company prefill
  (SPEC-7.3.2) must be removed from session storage the first time it
  is read (`take_stashed_contact_new_prefill`), so a stale prefill
  cannot apply to an unrelated later visit to the New Contact page in
  the same tab.


## 8. Contact sync and import

Scope: `src/pages/contact_sync_review.rs`, `src/pages/settings_contact_roles.rs`,
`src/pages/settings_contact_sync.rs`,
`src/pages/settings_contact_sync_group_import.rs`,
`src/pages/settings_contact_sync_icloud.rs`,
`src/pages/settings_contact_sync_import.rs`,
`src/pages/settings_contact_sync_microsoft.rs`,
`src/pages/settings_contact_sync_vcard.rs`.

`settings_contact_sync.rs` is the Google Contacts settings page, but it also
defines the `Overview`/`Connection`/`Run`/`RunFailure` wire types,
`card_state_of`, `copy_for`'s shared shape, `SyncCadence`, `RunProgress`,
`FailureList`, `ConnectionFacts` and `disconnect_message_from`, which
`settings_contact_sync_icloud.rs` and `settings_contact_sync_microsoft.rs`
reuse for their own provider; a requirement stated once in this section
against `settings_contact_sync.rs` applies to the iCloud and Microsoft pages
wherever they call the same function, and is not restated per provider.
`settings_contact_sync_group_import.rs` is likewise shared between the
`ICloudContactsImportPage` and `MicrosoftContactsImportPage` routes through
one `Vendor`-parameterized `GroupImportBody` component.

### 8.1 Interfaces

- **SPEC-8.1.1**: `ContactImportReviewPage` (`src/pages/contact_sync_review.rs`)
  must render a loading skeleton while the review queue loads, an error
  banner "Could not load the review queue: {e}" on failure, an empty state
  titled "Nothing to review" when the queue is empty, and otherwise a count
  line and one `ReviewCard` per queued record.
- **SPEC-8.1.2**: `ReviewCard` must render a card heading
  "{kind} {index+1} of {total}: {name}", a native radio-button fieldset of
  candidates with an `aria-keyshortcuts`-labelled Link/Create new/Skip
  action set, and, whenever more than one candidate is present, a hint
  "{n} Mokosh contacts could be this person. Choose one to link, or create a
  new contact."; the Link action must be disabled unless a candidate is
  chosen from a non-empty candidate list.
- **SPEC-8.1.3**: `ReviewCard`'s keyboard shortcut handler must map a plain
  (no Ctrl/Alt/Meta) `l`/`L` to Link, `c`/`C` to Create, and `s`/`S` to Skip,
  case-insensitively, and must ignore the key entirely whenever any
  modifier is held.
- **SPEC-8.1.4**: `ReviewCard` must require confirmation ("Skip this
  contact?", confirm label "Skip it") before sending a Skip answer, and must
  send a Link or Create answer immediately with no confirmation.
- **SPEC-8.1.5**: After an answer resolves, `ReviewCard` must remove that
  record from the local queue without reloading it from the server, move
  focus to the next remaining record's heading (or to the page title if the
  queue is now empty), and announce the outcome through a polite live
  region using one of the fixed templates "Linked {source} to {target}.",
  "Linked {source}.", "Created a new contact for {source}.", or "Skipped
  {source}; it will not be imported.", followed by "Nothing left to
  review.", "1 left to review.", or "{n} left to review."
- **SPEC-8.1.6**: `ImportReviewLink` must render nothing when the contact
  sync overview reports zero open reviews, and otherwise a link to
  `Route::ContactImportReview` reading "Review imports ({waiting})".
- **SPEC-8.1.7**: `ContactRolesListPage` (`src/pages/settings_contact_roles.rs`)
  must render a 3-row loading table while the role list loads, an inline
  error banner "Could not load portal roles. Refresh the page to retry." on
  a reachable-server failure (or a full unavailable notice when the server
  is unreachable), an empty-table message "No portal roles yet. Click New
  role to add one." when the list is empty, and otherwise one row per role
  with Name, Capabilities, Contacts and a per-row Delete action.
- **SPEC-8.1.8**: `ContactRoleRow`'s Delete action must be disabled, in this
  priority order, for a built-in role ("Built-in roles cannot be deleted."),
  then for a role with at least one assigned contact ("{n} contact(s) hold
  this role; remove those assignments first.", singular wording for
  exactly one), then whenever the server is unreachable ("Can't delete
  while the server is unreachable."), and must otherwise require
  confirmation (`Delete portal role "{name}"`, "This cannot be undone.")
  before deleting.
- **SPEC-8.1.9**: `ContactRoleRow` must truncate a role's capability list to
  60 characters with a trailing "..." in the table cell, while exposing the
  untruncated list through the cell's tooltip text, and must render "-" for
  an empty capability list and for a contact count the server did not
  return.
- **SPEC-8.1.10**: `ContactRoleEditPage` must skip the role fetch and start
  from a blank form when its route id is the literal string `"new"`,
  otherwise fetching `GET /portal-roles/{id}` and rendering a loading state
  until both that fetch and the capability-descriptor fetch resolve, an
  unavailable notice when the server is unreachable, or "Could not load
  this role. Refresh the page to retry." on any other role-fetch failure.
- **SPEC-8.1.11**: `ContactRoleEditForm` must render a required name field
  (max 64 characters) and a capability checkbox per descriptor grouped by
  the descriptor's `group` in first-appearance order, must render every
  checkbox disabled for a built-in role while keeping the name field
  editable, and must show "Built-in role. You can rename it, but its
  capability set is fixed." in place of "Name the role and pick the
  capabilities it grants." when the role is built-in.
- **SPEC-8.1.12**: `GoogleContactsSettingsPage` (`src/pages/settings_contact_sync.rs`)
  must render `AdminOnlyNotice` for a non-admin and otherwise
  `GoogleContactsSettingsBody`, which must show a loading skeleton while the
  overview loads and "Could not load the Google Contacts connection." if
  the fetch fails or no card state can be derived from it.
- **SPEC-8.1.13**: `card_state_of` must classify the overview into exactly
  one `CardState`, checked in this order: the organization switch is off;
  no deployment client is configured and never connected; never connected
  but configured; an import run is queued or running; the connection needs
  reconnecting; the connection is throttled; three or more consecutive
  failures; the latest finished run failed with at least one failed record;
  the connection's own `sync_status` is `"failed"`; no groups/labels/folders
  are selected; otherwise healthy.
- **SPEC-8.1.14**: `copy_for` must map every `CardState` from SPEC-8.1.13 to
  a fixed badge text and tone, headline and next-step sentence, and
  `actions_for` must map the same state to the exact subset of
  {connect, reconnect, sync_now, disconnect, choose_labels} enabled for it;
  both functions are defined once in `settings_contact_sync.rs` and reused
  verbatim by the iCloud and Microsoft pages for their own copy and action
  sets.
- **SPEC-8.1.15**: `SyncCadence` must offer the fixed interval list 10, 15,
  30, 60, 120, 240, 480 and 1440 minutes, injecting the connection's current
  interval into the list when it is not already one of them, and must make
  no network call when the chosen value equals the current one.
- **SPEC-8.1.16**: `RunProgress` must render "{processed} of {total}
  contacts read ({p}%)" once a run reports a total, or "Reading contacts
  from {source}..." before one is known; `FailureList` must list failure
  reasons most-frequent first (ties broken alphabetically) and append
  "{unlisted} more not listed" whenever the run's failed-record count
  exceeds the number of failure reasons returned; `ConnectionFacts` must
  show the account identifier, the last sync time or "Never", a label/group
  count ("None chosen"/"1 label"/"{n} labels"), and, only when non-zero, a
  "Deleted in {source}" count suffixed ", kept in Mokosh".
- **SPEC-8.1.17**: Connecting (on any of the Google, iCloud or Microsoft
  pages) must show the provider's fixed consent-point list before the
  connect call fires; turning the organization switch off must show a
  confirmation naming what is kept before writing it, while turning it on
  must write the setting immediately with no confirmation. The Google
  (`settings_contact_sync.rs`) and iCloud pages additionally re-sync the
  checkbox from the server when the turn-off confirmation is cancelled,
  rather than leaving it in its optimistic unticked state; the Microsoft
  page's cancel path is the one that still does not do this.
- **SPEC-8.1.18**: `settings_contact_sync.rs`'s `return_banner` must map a
  `contact_sync` query parameter of `"connected"`, `"reconnected"`, or
  `"failed"` to one fixed success or error banner each, and any other value
  or its absence to no banner.
- **SPEC-8.1.19**: `GroupImportBody` (`src/pages/settings_contact_sync_group_import.rs`)
  is one component parameterized by a `Vendor` constant (`ICLOUD` or
  `MICROSOFT`) that `ICloudContactsImportPage` and
  `MicrosoftContactsImportPage` each render behind their own admin-only
  gate, with every vendor-specific string (display name, group noun, hint
  text) coming from that constant.
- **SPEC-8.1.20**: `GroupImportBody`'s group list must rename the server's
  ungrouped bucket to "Contacts in no group" regardless of the name the
  server sent, and must sort it after every named group, which are
  themselves sorted case-insensitively by name.
- **SPEC-8.1.21**: `GroupImportBody` must present a two-step stepper
  ("1. Choose {groups}", "2. Review and import"); the Review step must show
  "Working out exactly what this selection would do..." until the
  exact-selection preview resolves, and its Start action must stay disabled
  until that preview has resolved successfully.
- **SPEC-8.1.22**: `ICloudContactsSettingsPage` (`src/pages/settings_contact_sync_icloud.rs`)
  must render, behind its admin-only gate, an Apple ID field (type email)
  and an app-specific-password field (type password, with the help text
  "Not your Apple ID password. Mokosh stores it encrypted and never shows
  it again.") whenever the card state allows connecting or reconnecting,
  labelling the submit action "Connect iCloud account" or "Save password"
  depending on which.
- **SPEC-8.1.23**: `GoogleContactsImportPage`/`GoogleContactsImportBody`
  (`src/pages/settings_contact_sync_import.rs`) must list the account's own
  labels alphabetically (case-insensitively), then the fixed `STARRED` label,
  then the fixed `EVERYTHING` label last, with `EVERYTHING`'s row name
  forced to "Every contact in the account" regardless of the server's name
  for it; its Review step must show exact totals from a server preview and
  keep Start disabled until that preview succeeds.
- **SPEC-8.1.24**: `MicrosoftContactsSettingsPage`/`Body` (`src/pages/settings_contact_sync_microsoft.rs`)
  must reuse `card_state_of`/`actions_for`/`StateIcon` from
  `settings_contact_sync.rs` (SPEC-8.1.13/8.1.14) and supply its own
  Microsoft-specific badge/headline/next-step text and its own four-item
  `CONSENT_POINTS` list, shown only while the state allows connecting or
  reconnecting.
- **SPEC-8.1.25**: `VcardImportPage`/`VcardImportBody` (`src/pages/settings_contact_sync_vcard.rs`)
  must drive a four-step flow (Upload, Choose, Review, Importing) behind an
  admin-only gate; the Upload step must accept only `.vcf`/`.vcard`
  (`text/vcard`/`text/x-vcard`) files and must list the account's recent
  uploads with their own loading, empty and unavailable states; its
  `upload_status` must report, in order, a finished/queued/running run's
  own status text, then "Expired" once the upload record is discarded with
  no run, then "Not imported yet".

### 8.2 Data model

- **SPEC-8.2.1**: `contact_sync_review.rs`'s `ReviewItem`, `SourceSnapshot`,
  `SnapshotPhone` and `Candidate` must default every field but
  `ReviewItem.external_id` and `Candidate.contact_id`/`match_reason` on
  absence, so an older or newer server payload still decodes; its
  `ResolveBody` must tag the action as `link`, `create` or `skip`
  (snake_case) and must omit `connection_id` from the request body when it
  is `None` rather than serializing it as `null`.
- **SPEC-8.2.2**: `settings_contact_roles.rs`'s `PortalRoleWire` must
  default `capabilities` and `is_builtin` on absence and must leave
  `contacts_count` as `None` (rendered "-") rather than `0` when the server
  omits it, since the server does not currently return it; its
  `CreateRoleBody` must always send both `name` and `capabilities`, while
  `UpdateRoleBody` must send each of `name`/`capabilities` only when it
  changed, and must never send a changed `capabilities` for a built-in
  role.
- **SPEC-8.2.3**: `settings_contact_sync.rs`'s `Overview` must default
  `enabled`/`icloud_enabled`/`microsoft_enabled` to `true` and
  `configured`/`microsoft_configured` to `false` on absence, and must
  resolve a named provider's connection through `connection_for(provider)`
  (`"icloud"`/`"microsoft"` to their own field, anything else including
  `"google"` to the Google field); its `Connection` must default
  `sync_interval_minutes` to 60 and every count field
  (`consecutive_failures`, `open_reviews`, `deleted_in_source`) to 0 on
  absence; its `Run` must default every count field to 0, `failures` to
  empty (capped at 50 by the server), and must treat `status` in
  {`"queued"`, `"running"`} as active.
- **SPEC-8.2.4**: `settings_contact_sync_group_import.rs`'s `PreviewBody`
  must omit `group_ids` from the request body for the initial, unfiltered
  preview rather than sending it as `null`, while its `SelectionBody`
  always sends a `group_ids` list; its `UNGROUPED` sentinel
  (`"mokosh:ungrouped"`) identifies the server's placeholder group for
  records in no group, shared with the vCard wizard's identical sentinel.
- **SPEC-8.2.5**: `settings_contact_sync_import.rs`'s `Preview`,
  `PreviewGroup`, `PreviewRecord` and `Totals` must default every field on
  absence; its fixed label constants `EVERYTHING`
  (`"contactGroups/myContacts"`) and `STARRED`
  (`"contactGroups/starred"`) identify Google's built-in labels among the
  account's own.
- **SPEC-8.2.6**: `settings_contact_sync_vcard.rs`'s `CardProblem`,
  `UploadedFile`, `Uploaded` and `RecentUpload` must default their optional
  fields on absence; unlike the group-import and Google wizards, its
  `PreviewBody`/`ImportBody` always send a non-optional `group_ids` list.
- **SPEC-8.2.7**: `settings_contact_sync_microsoft.rs`'s `AuthorizeResponse`
  must default `authorize_url` to an empty string on absence; its
  `SettingWrite` (shared shape with the Google and iCloud pages' own
  setting-write body) carries `category`, `key` and the boolean `value`
  sent to `PUT /settings`.

### 8.3 Configuration

- **SPEC-8.3.1**: The organization-level enable switch for each provider is
  a tenant setting under category `"integrations"`, keys
  `google_contacts_enabled`, `icloud_contacts_enabled` and
  `microsoft_contacts_enabled`, written through `PUT /settings` and read
  back from the shared `Overview`.
- **SPEC-8.3.2**: The Google, iCloud, Microsoft and vCard pages must each
  poll `GET /integrations/contact-sync` (or, for vCard, the run endpoint)
  every 3000 milliseconds while their own connection shows an active
  import, and must stop polling as soon as it is no longer active.
- **SPEC-8.3.3**: `settings_contact_sync.rs`'s `cadence_minutes` offers the
  fixed list 10, 15, 30, 60, 120, 240, 480, 1440 minutes (`cadence_label`
  rendering 1440 as "Once a day", 60 as "Every hour", another multiple of
  60 as "Every {n} hours", and anything else as "Every {m} minutes"); the
  server accepts any value from 5 to 1440 inclusive.
- **SPEC-8.3.4**: `settings_contact_sync.rs`'s `FAILING_AFTER` constant (3)
  is the consecutive-failure count at which `card_state_of` reports
  `FailingRepeatedly` (SPEC-8.1.13); per the comment at
  `settings_contact_sync.rs:61`, the server sends its own
  failing-repeatedly notification at the same count, a claim this client
  file states but does not itself enforce.
- **SPEC-8.3.5**: `settings_contact_sync_vcard.rs`'s `MAX_BYTES` constant
  (10 MiB) is a client-side mirror of the server's own upload size limit,
  checked only to skip a doomed upload before sending it; the server's
  limit, not this constant, is authoritative.
- **SPEC-8.3.6**: Every network call on every page in this scope must
  compile to a no-op under a build with neither the `app` nor the `web`
  feature enabled, consistent with the pattern cited at SPEC-6.3.6.

### 8.4 Security

- **SPEC-8.4.1**: Every settings and import page in this scope
  (`GoogleContactsSettingsPage`, `ICloudContactsSettingsPage`,
  `MicrosoftContactsSettingsPage`, `GoogleContactsImportPage`,
  `ICloudContactsImportPage`, `MicrosoftContactsImportPage`,
  `VcardImportPage`, `ContactRolesListPage`, `ContactRoleEditPage`) must
  render `AdminOnlyNotice` instead of its body for a non-admin; this
  client-side gate is not itself sufficient authorization, and every
  mutating call these pages make must rely on the server's own admin
  check, consistent with SPEC-6.4.14.
- **SPEC-8.4.2**: `ContactRoleEditForm`'s capability checkboxes must be
  rendered disabled for a built-in role and its submit must never send a
  changed `capabilities` value for one (SPEC-8.2.2), as a client-side
  mirror of the server's own read-only enforcement of a built-in role's
  capability set.
- **SPEC-8.4.3**: `ContactRoleRow`'s delete action must only fire from the
  confirm dialog's own confirm action, never from the row's bare click
  handler, and the dialog itself must be reachable only once the three
  disable conditions in SPEC-8.1.8 all pass.
- **SPEC-8.4.4**: Connecting a Google, iCloud or Microsoft account must
  show that provider's fixed consent-point list (naming what is read, that
  nothing is written back, where it is stored, and how to revoke it)
  before the authorize or connect call fires; the user cannot bypass this
  dialog to connect directly.
- **SPEC-8.4.5**: `settings_contact_sync_icloud.rs` must clear its
  `app_password` form field to empty immediately after a successful
  connect call, so the submitted password is never redisplayed.
- **SPEC-8.4.6**: `settings_contact_sync_group_import.rs` must append
  `?provider={vendor.provider}` to every call it makes on the shared
  preview/selection/runs endpoints, so a Microsoft group import can never
  be mistaken by the server for a Google one through an unqualified path.
- **SPEC-8.4.7**: `settings_contact_sync_vcard.rs`'s client-side `precheck`
  (empty file, oversized file) is a UX shortcut only, not the security or
  validity boundary; it must refuse a file only when the server certainly
  would, and the server's own message is what is shown when the server
  itself refuses an upload `precheck` did not catch.

### 8.5 Error handling and observability

- **SPEC-8.5.1**: `contact_sync_review.rs` must log an error and show
  "Could not load the review queue: {e}" when the queue fails to load, and
  must show "Could not answer {name}: {e}" scoped to that record when an
  answer fails.
- **SPEC-8.5.2**: `settings_contact_roles.rs` must log an error and show
  "Could not load portal roles. Refresh the page to retry." on a list-load
  failure, and must log a separate error and render the edit form with an
  empty capability list ("No capabilities available. The server did not
  return any capability descriptors.", rather than blocking the form)
  when the capability-descriptor fetch fails.
- **SPEC-8.5.3**: `ContactRoleEditForm` must reject an empty (after
  trimming) name with "Name is required.", a name over 64 characters with
  "Name must be 64 characters or fewer.", and, only when creating a new
  role, zero selected capabilities with "Pick at least one capability.";
  a save failure must show the server's own message when the server
  returned a non-empty one, otherwise a generic fallback.
- **SPEC-8.5.4**: `settings_contact_sync.rs` must log an error and show
  "Could not load the Google Contacts connection." on an overview-load
  failure; a failed connect attempt must show "Could not start the
  connection: {e}" (or, on a non-wasm build whose authorize call
  succeeded, an informational "Open this address in a browser to connect:
  {url}" reusing the same banner); a failed sync-now or disconnect must
  show "{action}: {e}" ("Could not start an import"/"Could not
  disconnect"); a failed cadence change must show "Could not change how
  often this syncs: {e}"; a failed enable/disable write must show "Could
  not change the setting: {e}".
- **SPEC-8.5.5**: `settings_contact_sync_icloud.rs` and
  `settings_contact_sync_microsoft.rs` must apply the same error-message
  shapes as SPEC-8.5.4 for their own sync-now, disconnect and
  enable/disable actions, each naming its own provider. For a failed
  connect attempt, `settings_contact_sync_microsoft.rs` must additionally
  match SPEC-8.5.4's connect shape ("Could not start the connection: {e}",
  or on a non-wasm build "Open this to continue: {url}" once the
  authorize call itself succeeded, or "Could not open Microsoft's
  sign-in page: {e}" when the authorize call itself fails), while
  `settings_contact_sync_icloud.rs` must instead show the connect
  endpoint's own error text verbatim with no added wrapper, since its
  connect flow is a direct credential submission rather than an
  authorize-URL redirect.
- **SPEC-8.5.6**: `settings_contact_sync_group_import.rs` must show "Could
  not read the {vendor.display} account: {e}" when the initial group
  preview fails, "Could not save the groups: {e}" when the selection save
  fails (stopping before starting the run), and "The groups are saved, but
  the import did not start: {e}" when the run-start call fails after the
  selection was saved.
- **SPEC-8.5.7**: `settings_contact_sync_import.rs` must show "Could not
  read the Google account: {e}" on a load failure, "Could not save the
  labels: {e}" on a selection-save failure, and "The labels are saved, but
  the import did not start: {e}" on a run-start failure after a saved
  selection; its `summary` must render "No contacts carry the chosen
  labels, so this import would change nothing." when the selection
  resolves to zero counted contacts and no excluded ones.
- **SPEC-8.5.8**: `settings_contact_sync_vcard.rs`'s `precheck` must reject
  an empty file with "{file_name} is empty." and a file over 10 MiB with
  "{file_name} is larger than 10 MB, the most one import accepts. Export
  the contacts in smaller groups and import each file."; an upload the
  server rejects must show "The file was not accepted: {e}"; a failed
  import start must show "The import did not start: {e}"; its
  `run_progress` must render, by run status, "Waiting to start. The import
  runs on the server; you can leave this page." (queued), "Importing:
  {processed} of {total} contacts read." or "Importing: reading the
  file." (running), a fixed created/linked/updated/queued-for-review
  summary (completed), "The import was cancelled. What it had already
  written is kept." (cancelled), or the run's own error text (any other
  failure).
- **SPEC-8.5.9**: `settings_contact_sync_group_import.rs`'s preview-load
  failure must be logged with a message naming the vendor whose preview
  failed, not a vendor fixed at compile time; the log message names the
  vendor via `vendor.provider`.

### 8.6 Concurrency and ordering

- **SPEC-8.6.1**: Every mutating action on every page in this scope must
  disable its own control (and, where one page exposes several concurrent
  actions, every other mutating control on the page) for the duration of
  that action, so a second submit cannot be issued while the first is still
  in flight.
- **SPEC-8.6.2**: `ContactRoleRow`'s delete handler must additionally check
  its own in-flight flag and return immediately if a delete for that row
  is already running, rather than relying on the disabled button alone.
- **SPEC-8.6.3**: The Google, iCloud, Microsoft and vCard pages' poll loops
  (SPEC-8.3.2) must each guard against stacking more than one in-flight
  poll: a loop must not schedule its next tick until the previous one has
  both slept and refreshed its resource.
- **SPEC-8.6.4**: `settings_contact_sync_group_import.rs` and
  `settings_contact_sync_import.rs` must each perform the selection write
  before the run-start call, and must abort before starting the run
  whenever the selection write fails.
- **SPEC-8.6.5**: `settings_contact_sync_group_import.rs` and
  `settings_contact_sync_import.rs` must each seed their local selection
  state from the saved overview connection at most once, on first load,
  never overwriting an in-progress local choice on a later overview
  refresh.
- **SPEC-8.6.6**: `settings_contact_sync.rs`'s `confirm_off` cancel path
  must restart the overview resource to resync the enable checkbox's
  visual state, since the checkbox optimistically unticks before the
  confirmation is answered.

### 8.7 Integrations

- **SPEC-8.7.1**: `contact_sync_review.rs` must call
  `GET /integrations/contact-sync/review-queue` and
  `POST /integrations/contact-sync/review-queue/resolve`.
- **SPEC-8.7.2**: `settings_contact_roles.rs` must call `GET /portal-roles`,
  `GET /portal-roles/{id}`, `POST /portal-roles`, `PUT /portal-roles/{id}`,
  `DELETE /portal-roles/{id}` and `GET /portal-roles/capabilities`.
- **SPEC-8.7.3**: `settings_contact_sync.rs` must call
  `GET /integrations/contact-sync`,
  `POST /integrations/contact-sync/google/authorize`,
  `POST /integrations/contact-sync/google/disconnect`,
  `POST /integrations/contact-sync/runs`,
  `PUT /integrations/contact-sync/schedule?provider={provider}` and
  `PUT /settings`.
- **SPEC-8.7.4**: `settings_contact_sync_icloud.rs` must call
  `GET /integrations/contact-sync`,
  `POST /integrations/contact-sync/icloud/connect`,
  `POST /integrations/contact-sync/runs?provider=icloud`,
  `POST /integrations/contact-sync/icloud/disconnect` and `PUT /settings`.
- **SPEC-8.7.5**: `settings_contact_sync_microsoft.rs` must call
  `GET /integrations/contact-sync`,
  `POST /integrations/contact-sync/microsoft/authorize`,
  `POST /integrations/contact-sync/runs?provider=microsoft`,
  `POST /integrations/contact-sync/microsoft/disconnect` and
  `PUT /settings`.
- **SPEC-8.7.6**: `settings_contact_sync_group_import.rs` must call
  `POST /integrations/contact-sync/preview?provider={provider}`,
  `PUT /integrations/contact-sync/selection?provider={provider}`,
  `POST /integrations/contact-sync/runs?provider={provider}` and
  `GET /integrations/contact-sync`.
- **SPEC-8.7.7**: `settings_contact_sync_import.rs` must call
  `POST /integrations/contact-sync/preview`,
  `PUT /integrations/contact-sync/selection` and
  `POST /integrations/contact-sync/runs` for the Google account, with no
  `provider` parameter (the server's default provider for this family of
  calls is Google).
- **SPEC-8.7.8**: `settings_contact_sync_vcard.rs` must call
  `POST /integrations/contact-sync/vcard/uploads`,
  `GET /integrations/contact-sync/vcard/uploads`,
  `POST /integrations/contact-sync/vcard/uploads/{file_id}/preview`,
  `POST /integrations/contact-sync/vcard/uploads/{file_id}/import` and
  `GET /integrations/contact-sync/runs/{run_id}`.

### 8.8 Build and deployment

None of the files in this section's scope add a build or packaging step of
their own; they compile under the same `app`/`web` feature gating
`src/main.rs` and `Cargo.toml` define (see SPEC-1.3.1), consistent with the
pattern cited at SPEC-6.3.6 and restated as SPEC-8.3.6 for this scope.

### 8.9 Retention and cleanup

- **SPEC-8.9.1**: A Skip answer in `contact_sync_review.rs` is permanent:
  the record is never imported by any later sync or file import and is
  never offered to a reviewer again; Link and Create likewise never undo.
- **SPEC-8.9.2**: `settings_contact_roles.rs`'s role delete is a hard,
  non-reversible delete with no soft-delete path, and is reachable only
  once none of SPEC-8.1.8's three disable conditions apply.
- **SPEC-8.9.3**: Disconnecting a Google, iCloud or Microsoft account must
  remove only the stored provider credential; every contact already
  imported stays as a local record, and an in-progress import run stops.
- **SPEC-8.9.4**: Turning a provider's organization switch off must keep
  its connection, every already-imported contact and its review queue
  intact, resuming from where it stopped when the switch is turned back
  on.
- **SPEC-8.9.5**: A contact deleted at the source (Google, iCloud or
  Microsoft) must be kept as a local record in Mokosh; `ConnectionFacts`
  (SPEC-8.1.16) must report that count rather than deleting the record.
- **SPEC-8.9.6**: This scope implements no retention or expiry of an
  uploaded vCard file; `settings_contact_sync_vcard.rs` only reads and
  displays a file's server-reported expiry (`discarded_at`) as its
  "Expired" status (SPEC-8.1.25). The file's retention period itself is a
  server contract this page cites in its own help text, not a requirement
  this client enforces.

## 12. Knowledge base

Scope: `src/pages/knowledge_base.rs`, `src/pages/kb_activity.rs`,
`src/pages/kb_inline.rs`.

### 12.1 Interfaces

- **SPEC-12.1.1**: `KBHomePage` must render a search box, a "Browse by
  company" picker, a category grid (loading skeleton, empty state, or
  populated), a "Top ticket-driving articles" widget, and a "Recent
  Articles" list capped at `RECENT_LIMIT = 5` articles
  (`knowledge_base.rs:47,55,540-782`); submitting the search box must
  navigate to `KBArticleListPage` with `?q=` set to the trimmed text
  and no tag/category filter (`knowledge_base.rs:631-639`).
- **SPEC-12.1.2**: `KBArticleListPage` must render a server-paginated
  (`PER_PAGE = 25`), searchable, category-filterable article table
  with a left `KbTreeNav` rail; its `initial_q`/`initial_tag`/
  `initial_category` route props seed the search/tag/category state
  and are kept in sync on prop change via `use_reactive!` effects, so
  navigating from one tag or category link to another re-filters
  without remounting (`knowledge_base.rs:981-1017`).
- **SPEC-12.1.3**: With a tag filter active, the list page must fetch
  the entire article corpus (`get_all_with_auth`, the server cannot
  narrow on tags) and filter and paginate client-side; with no tag
  filter, it must use the server's paginated `GET /kb/articles`
  (`knowledge_base.rs:1083-1155`).
- **SPEC-12.1.4**: `KBCompanyPage` (`/kb/companies/{id}`) must render
  the company's name, falling back to "Company" with an inline error
  banner when the name lookup fails or returns a blank name, and that
  company's articles via `GET /kb/articles?company_id={id}`
  (`knowledge_base.rs:1355-1505`).
- **SPEC-12.1.5**: `KBArticleDetailPage` must render the article body
  (Markdown, with interactive task-list toggles gated on
  `can_mutate`), breadcrumbs, a parent-version back-link for a company
  version, tags, attribution, a rating bar (shown only once the tenant
  has at least 2 users), inline comments (staff sessions only), and a
  right rail with a template-variable picker, actions, measured
  duration, linked tickets, company versions, a table of contents, and
  version history (`knowledge_base.rs:1591-2335`).
- **SPEC-12.1.6**: `KBArticleNewPage`/`KBArticleEditPage` must render
  the shared `ArticleForm` in Create/Edit mode; `KBArticleNewPage`
  accepts `parent_article_id` and `visibility` query-string prefills
  from the "Create a company version" link, defaulting visibility to
  `internal` when the query string leaves it blank
  (`knowledge_base.rs:3022-3047`); `KBArticleEditPage` must resolve the
  article's scoped company names in the same fetch pass that loads the
  article (`knowledge_base.rs:3060-3083`).
- **SPEC-12.1.7**: `ArticleActions` must render exactly three controls
  in a fixed order: Edit, "Open ticket about this article" (carrying
  `from_kb_article`/`from_kb_title`/`from_kb_url` query params), and
  Delete (behind a `ConfirmDialog`); `ArticleActionsMenu` must expose
  the same controls through a header overflow popover, shown whenever
  the right rail is collapsed and always below the `sm` breakpoint
  (`knowledge_base.rs:4940-5052`).
- **SPEC-12.1.8**: `RatingBar` must POST to
  `/kb/articles/{id}/helpful` or `/kb/articles/{id}/not_helpful` and
  update its counts and `my_vote` from the response; both buttons must
  be disabled while a vote is in flight or `can_mutate` is false
  (`knowledge_base.rs:5054-5150`). A failed vote POST must leave the
  counts and `my_vote` unchanged, log at error with the article id, and
  toast "Your vote was not recorded: {message}" (`vote_failed`).
- **SPEC-12.1.9**: `ArticleToc` must render nothing for an article
  with no headings, and otherwise a nested "On this page" list whose
  links intercept the click (no router hash navigation), scroll the
  target heading into view, and mark the active section via
  `aria-current` plus a border/weight change, never color alone
  (`knowledge_base.rs:2431-2493`).
- **SPEC-12.1.10**: `VersionHistoryCard` must list every prior version
  with per-row "Compare" (diff against the live article) and "Restore"
  (confirm dialog showing the diff, taking an optional `change_note`)
  actions, both disabled while `!can_mutate`; the live version's row
  is marked "Current" and carries neither action
  (`knowledge_base.rs:2749-2976`).
- **SPEC-12.1.11**: `ArticleActivity` must render a three-tab
  (`All activity`/`Comments`/`History`) `tablist` whose selected tab
  is persisted under the `kb_activity_tab` preference key and restored
  on mount, defaulting to `All activity` for an unrecognized stored
  value; `ArrowRight`/`ArrowLeft` on the tablist must step to the
  next/previous tab, wrapping at both ends, and move focus to the
  newly selected tab button (`kb_activity.rs:57-76,130,147,191-203`).
- **SPEC-12.1.12**: `build_stream` must merge the article's comments
  and version events into one time-ordered (oldest first) list, must
  include comments only when the active tab is not `History` and
  versions only when it is not `Comments`, and must omit a resolved
  comment whenever the `kb_activity_show_resolved` preference is off
  (`kb_activity.rs:103-128,131,148,246-251`).
- **SPEC-12.1.13**: `CommentThread` must fold a resolved root comment
  to one line (author, struck-through body, resolver name, reply
  count, an "Unresolve" action) and must otherwise render the full
  comment, its replies, a "Reply"/"Resolve" action pair, and, once
  "Reply" is clicked, a nested composer; editing a comment swaps its
  body for a `MarkdownEditor` whose Save is disabled while the trimmed
  draft is empty; deleting a comment goes through a `ConfirmDialog`
  before any request fires, and a deleted comment renders only
  "Comment deleted" in place of its body and author actions
  (`kb_activity.rs:392-655`).
- **SPEC-12.1.14**: `InlineComments` must render one left-gutter
  marker per resolved passage ("1 inline comment" / "{n} inline
  comments"); activating a marker or its mark must open a dialog
  listing every comment anchored there (body truncated to 120
  characters) with an "Open thread" action that focuses the matching
  row (`kb-comment-{id}`) in the foot-of-article stream
  (`kb_inline.rs:128,143-243`).
- **SPEC-12.1.15**: "Comment on selection" must be disabled while
  `!can_mutate`; with no live text selection, or a selection that
  cannot be anchored, it must set the hint "Select some of the article
  text first, then press this." and must not open the composer; a
  successfully captured selection must open a composer showing the
  captured quote and posting it with `parent_id: None` and the
  captured anchor (`kb_inline.rs:172-199,247-270`).

### 12.2 Data model

- **SPEC-12.2.1**: `CompanyNameRow` and `CompanyNameOnly`
  (`knowledge_base.rs:399-404,1364-1368`) must default `name` on
  absence, so a server response missing the field still decodes.
- **SPEC-12.2.2**: `TreeNode`/`build_kb_tree` must bucket any article
  whose `category_id` is absent or references an unknown category id
  into a separate "uncategorized" list rather than dropping it
  (`knowledge_base.rs:221-278`); `resolve_category_path` must walk
  `parent_id` from a category to the root, returning `[]` for no
  category or a dangling id, and must guard against a cycle with a
  visited-id set rather than looping forever
  (`knowledge_base.rs:199-216`).
- **SPEC-12.2.3**: `Entry` (`kb_activity.rs:81-98`) is a client-only
  union of an already-fetched `KbComment` or a `KbArticleVersion`
  paired with its predecessor; this scope persists no server schema
  of its own, only wire DTOs already owned by `crate::modules::kb`.
- **SPEC-12.2.4**: `place` (`kb_inline.rs:28-50`) must consider only
  root, non-deleted comments as anchor owners, must skip a root
  comment with no anchor or an unparsable anchor (neither marked nor
  an orphan), and must
  classify an anchor that fails to resolve against the current
  rendered text as an orphan (its comment id collected) distinct from
  one that resolves; resolved ranges must be sorted by start offset
  before overlaps are split, and the resulting fragments converted
  1:1 to `MarkSpec { start, end, ids }` in document order. An
  unparsable anchor must not be silent: `InlineComments` logs each
  live root `unreadable_anchors` names at warn with its comment id,
  once per change of body or anchors, and `CommentThread` logs a
  thread whose anchor `anchor_unreadable` rejects once per mount, since
  it shows no quote.
- **SPEC-12.2.5**: `orphaned_by_edit` (`kb_inline.rs:56-68`) must
  report exactly the quoted text of a root, non-deleted, anchored
  comment whose anchor resolved against the rendered text before an
  edit but does not resolve against the rendered text after it; a
  comment already unresolvable before the edit must never appear in
  the result, since the edit did not cause that loss.

### 12.3 Configuration

- **SPEC-12.3.1**: The article list must page the server list at
  `PER_PAGE = 25` rows; the home page's recent-articles list is capped
  at `RECENT_LIMIT = 5` (`knowledge_base.rs:47,55`). Client-side
  `maxlength` caps mirror server column limits: `TITLE_MAX = 255`,
  `SLUG_MAX = 255`, `SUMMARY_MAX = 255`, `BODY_MAX = 100_000`, and
  `TAG_MAX = 50` characters per tag (`knowledge_base.rs:63-71`).
- **SPEC-12.3.2**: `sanitize_tags` must split a tag string on `,`,
  strip markup (`<`/`>`) and control characters via `clean_strict`,
  trim, cap each tag at `TAG_MAX`, and drop empties
  (`knowledge_base.rs:82-99`). `slugify` must lowercase, collapse every
  run of non-alphanumerics to a single hyphen, trim leading/trailing
  hyphens, run `clean_strict` first so an invisible character cannot
  produce an unaccounted-for hyphen, and fall back to `"article"` for
  empty input (`knowledge_base.rs:109-128`).
- **SPEC-12.3.3**: `next_slug` must derive the slug from the title via
  `slugify` until the slug field is manually touched, after which it
  must leave the slug alone; the category form's slug follows the
  same rule against the category name
  (`knowledge_base.rs:133-139,4551-4569,4757-4760`).
- **SPEC-12.3.4**: The article-list search input must debounce 300ms
  before refetching (`knowledge_base.rs:1061`); the template-variable
  render picker debounces its edits the same 300ms
  (`knowledge_base.rs:1827`).
- **SPEC-12.3.5**: `meta_must_be_open` must report the article
  metadata panel as forced open whenever Title or Slug is empty; once
  both are non-empty, the panel's open/closed state becomes a
  user-toggled signal that is never auto-closed again
  (`knowledge_base.rs:181-190,3386-3395`).
- **SPEC-12.3.6**: The active activity tab and the resolved-thread
  visibility are the only pieces of this scope's state persisted
  across sessions, under the `kb_activity_tab` and
  `kb_activity_show_resolved` preference keys respectively
  (`kb_activity.rs:130-131`); every other control (edit drafts,
  reply-open state, settings-menu open state) is render-local state
  that resets on remount.

### 12.4 Security

- **SPEC-12.4.1**: Every mutating control in this scope (category
  create/edit/delete, article create/edit/delete, vote, task-list
  toggle, version restore, comment resolve/edit/delete/post, comment
  on selection) must read `crate::hooks::use_can_mutate()` and disable
  itself while `!can_mutate`, with a `title` attribute naming the
  server as unreachable (`knowledge_base.rs:621,652,833-848,907-910`;
  `kb_activity.rs:428,506,525,532,672,720,733,770,835`;
  `kb_inline.rs:172,176-177`).
- **SPEC-12.4.2**: `company_scope_error` must block submitting a
  `client_specific` article with an empty company scope client-side,
  mirroring the server's 422 (`knowledge_base.rs:370-376,3621-3634`);
  the save handler must reject any scoped-company id that fails
  `uuid::Uuid` parsing rather than silently dropping it, since a
  dropped id would revoke that client's portal access without telling
  the author (`knowledge_base.rs:3636-3657`).
- **SPEC-12.4.3**: `KBArticleDetailPage`'s inline comments and
  `LinkedTicketsCard` must not render, and their resources must not
  fetch, for a contact (customer) session, consistent with the server
  refusing a contact bearer on those routes
  (`knowledge_base.rs:1627-1645,2218-2227,2249-2257,2294-2328`).
- **SPEC-12.4.4**: `ArticleToc`'s click handler must call
  `e.prevent_default()` before scrolling, so a heading jump never
  becomes a router navigation that could be hijacked
  (`knowledge_base.rs:2476-2481`).
- **SPEC-12.4.5**: Edit and Delete on a comment must render only when
  the signed-in user is the comment's author or an admin; the
  resolve/unresolve action carries no such author restriction, since
  resolving a thread requires only `can_mutate`, not authorship
  (`kb_activity.rs:503-509,522-535,604,716-740`).
- **SPEC-12.4.6**: The span wrapping "Comment on selection" must call
  `prevent_default` on `mousedown`, so the button click does not first
  collapse the text selection it is about to capture
  (`kb_inline.rs:172`).

### 12.5 Error handling and observability

- **SPEC-12.5.1**: `KBHomePage`'s category grid is the primary
  resource; a failed load while the server is unreachable must render
  `ContentUnavailable`, while the recent-articles and
  top-ticket-driving widgets are secondary and degrade to their own
  inline "Could not load…" text (`knowledge_base.rs:612-626,755-757`).
  `KBArticleListPage`, `KBCompanyPage`, `KBArticleDetailPage`, and
  `KBArticleEditPage` must render `ContentUnavailable` only when their
  primary fetch failed AND the server is unreachable; a 4xx while
  reachable keeps an inline `ErrorBanner` plus the ordinary empty or
  "Could not load" state
  (`knowledge_base.rs:1129-1134,1403-1411,1904-1913,1983-1994,3092-3098,3138-3149`).
- **SPEC-12.5.2**: The task-list toggle PUT must, on failure, toast
  "Could not save that change: {message}" and re-fetch the article to
  put the checkbox back, because the DOM has already flipped the box
  before the request resolves (`knowledge_base.rs:2196-2209`). A toggle
  whose index `toggle_task` cannot find must not PUT; it must log at
  warn with the index and article id, toast "Could not apply that
  change: the checklist has changed. Try again.", and re-fetch the
  article the same way.
- **SPEC-12.5.3**: A 422 on article save whose field name matches
  `template_variables[N]`/`template_variables.N` (`parse_tplvar_index`,
  `knowledge_base.rs:507-519`) must route to that row's inline error
  slot; any remaining `template_variables` field error must route to
  the template-variables banner, and any other error to the form's top
  banner (`knowledge_base.rs:3822-3853`). `ArticleForm`'s submit guard
  must validate Title and Body via `FormGuard` in one pass, not
  stopping at the first invalid field (`knowledge_base.rs:3609-3661`).
- **SPEC-12.5.4**: `MeasuredDurationCard` must distinguish "Loading…",
  "Could not load the measured duration." (fetch failed), "No time
  measured yet." (fetch succeeded, no sample), and the populated
  average/total/period display; a failed fetch must never be presented
  as "no data" (`knowledge_base.rs:2367-2411`).
- **SPEC-12.5.5**: Canceling the article form after an image upload
  has auto-created the draft article must attempt to `DELETE` that
  article; a failed delete must not block navigation away but must
  toast a warning naming the article so the author knows an unintended
  draft remains (`knowledge_base.rs:4394-4431`).
- **SPEC-12.5.6**: A failed comments fetch must render "Could not load
  the comments." alongside, not instead of, the version-history half
  of the stream, which continues to render from whatever versions did
  load (`kb_activity.rs:176,263-265`). Likewise a failed versions
  fetch must render "Could not load the history." on the History and
  All tabs while the comments keep rendering. An empty stream must not
  show its empty-state text ("No history to show." and the others)
  when a slice that tab shows failed to load (`empty_state`). A version
  with no `created_at` sorts first and shows "-"; its id must be
  logged at warn once per load (`undated_versions`). Resolve/unresolve failure must
  render "Could not {verb} the thread: {message}"; comment edit
  failure must render "Could not save the comment: {e}"; comment
  delete failure must render inside the confirm dialog itself, not the
  page-level error slot; a new comment or reply post failure must
  render "Could not post: {message}"
  (`kb_activity.rs:445,626,631,685,792`).
- **SPEC-12.5.7**: If the inline-comment dialog's `open_at` ids no
  longer match any loaded comment, it must render "These comments are
  no longer in the list ({n})." instead of an empty list
  (`kb_inline.rs:214-216`). With no selection the "Comment on
  selection" handler shows "Select some of the article text first,
  then press this."; with a selection `Anchor::capture` cannot anchor
  (only whitespace, or past the end of the text) it must log the range
  and the text length at warn and show "That selection cannot be
  attached to the article. Select a few words of the text instead."
  (`capture_selection`). If `anchor_dom::text_of` resolves to `None`
  (the body element is not yet in the DOM), the placement effect must
  return without marking, logging, or showing an error, while the
  handler, which answers a click, must log at warn and show "The
  article text is not ready yet. Try again in a moment."
  (`kb_inline.rs:117-119,184-186`).
- **SPEC-12.5.8**: When the article list feeding `KbTreeNav` fails to
  load, on `KBArticleListPage` or `KBArticleDetailPage`, the rail must
  render "Could not load the article list." (`role="alert"`) instead of
  the category tree, never an empty rail that reads as a knowledge base
  with no articles (`KbTreeNav`'s `load_failed`).
- **SPEC-12.5.9**: A failed `GET /kb/articles/{id}/vote` seed must keep
  the counts from the article payload, leave `my_vote` unset, and log
  at warn with the article id.
- **SPEC-12.5.10**: `KBArticleDetailPage` must render a templated
  article with no picks when the template picks it serialized itself
  fail to parse, and must log that parse failure at error
  (`parse_template_picks`).
- **SPEC-12.5.11**: Before an edit save, `ArticleForm` must warn about
  inline comments the edit orphans (`orphan_check`). When the comments
  could not be read (the fetch failed or has not finished), it must not
  save as if none were anchored: it must open a "Could not check for
  inline comments" confirmation naming the reason, whose "Save anyway"
  saves and whose "Go back" returns to the form. A create has nothing
  anchored and is never asked.
- **SPEC-12.5.12**: The category form's sort order accepts any `i32`,
  negatives included, and blank as `0`; any other value must be refused
  on the field with "Enter a whole number between -2147483648 and
  2147483647." (`parse_sort_order`).

### 12.6 Concurrency and ordering

- **SPEC-12.6.1**: Every list/detail page's resources must read
  `active_tenant_generation()` inside the `use_resource` closure so an
  org switch or token swap re-fetches (`knowledge_base.rs:564,580,
  1038,1068,1371,1385,1601,1612,1635,1658,1732,2354,2519,3063`),
  including `KBArticleDetailPage`'s vote seed and scoped company names.
  `use_reactive!` must bind the route `id` prop into
  `KBArticleDetailPage`'s article, versions, and comments resources,
  so an in-app article-to-article navigation restarts the fetch rather
  than keeping the previous article's data
  (`knowledge_base.rs:1600,1611,1634`).
- **SPEC-12.6.2**: The template-render resource must bail out with no
  request when `tpl_variables` is empty, so a non-templated article
  issues no render call (`knowledge_base.rs:1833-1836`).
  `VersionHistoryCard.on_restored` must restart both the article and
  the versions resources on a successful restore, so the live article
  and the history list both reflect the restore without a manual
  reload (`knowledge_base.rs:2320-2328`).
- **SPEC-12.6.3**: The category-delete, article-delete, vote, resolve,
  unresolve, comment edit, and comment delete handlers must each
  ignore a second concurrent invocation of the same control via a
  `busy`/`delete_busy` guard before spawning a request
  (`knowledge_base.rs:827-830,1950-1951,5091,5122`;
  `kb_activity.rs:428-430,619-621,672`).
- **SPEC-12.6.4**: `build_stream` must pair each version with the
  version immediately after it in the newest-first input slice as its
  "previous" diff baseline, so the oldest version in the list is
  paired with `None` and renders no diff toggle
  (`kb_activity.rs:119-124`). A successful resolve, unresolve, edit,
  delete, or post must restart the shared comments resource rather
  than mutating local state, so every consumer (this component and the
  inline-mark layer) observes the change (`kb_activity.rs:178`).
- **SPEC-12.6.5**: The inline-comment placement effect must re-run
  exactly when the hash of the body content plus each comment's id,
  `deleted` flag, and anchor changes (`placement_key`,
  `kb_inline.rs:72-82`); a change to a comment's own body text alone
  must not retrigger it. On each run it must clear prior highlights
  before computing and applying new ones, then re-measure gutter
  markers from the now-current DOM, in that order
  (`kb_inline.rs:109-129`).

### 12.7 Integrations

- **SPEC-12.7.1**: The pages in this scope must call
  `GET/POST /kb/categories`, `PUT/DELETE /kb/categories/{id}`,
  `GET/POST /kb/articles` (filters `page`, `per_page`, `q`,
  `category_id`, `company_id`), `GET/PUT/DELETE /kb/articles/{id}`,
  `GET /kb/articles/{id}/versions`,
  `POST /kb/articles/{id}/versions/{n}/restore`,
  `GET /kb/articles/{id}/vote`, `POST /kb/articles/{id}/helpful`,
  `POST /kb/articles/{id}/not_helpful`, `POST /kb/articles/{id}/render`,
  `GET /kb/articles/{id}/measured-duration`,
  `GET /kb/articles/{id}/tickets`, `GET /kb/articles/{id}/comments`,
  `POST /kb/articles/{id}/attachments`,
  `GET /kb/top-ticket-driving-articles?limit=10`,
  `GET /contacts/companies`, `GET /contacts/companies/{id}`, and
  `GET /auth/users?page=1&per_page=1`.
- **SPEC-12.7.2**: `kb_activity.rs` must call
  `POST /kb/comments/{id}/resolve`, `POST /kb/comments/{id}/unresolve`,
  `PUT /kb/comments/{id}`, `DELETE /kb/comments/{id}`, and
  `POST /kb/articles/{id}/comments`; it issues no `GET` of its own,
  since the comments and versions it renders are fetched by the
  calling page and passed in.
- **SPEC-12.7.3**: `kb_inline.rs` calls no HTTP endpoint directly; it
  drives the DOM layer (`platform::anchor_dom`,
  `platform::dom::scroll_into_view`) and composes
  `kb_activity::CommentComposer`, which owns the actual comment POST.

### 12.8 Build and deployment

None of the files in this section's scope add a build or packaging
step of their own; they compile under the same `app`/`web` feature
gating `src/main.rs` and `Cargo.toml` define (SPEC-1.3.1), with every
network call individually gated behind `#[cfg(feature = "app")]`
(`knowledge_base.rs:833,1831,1868,1955,2185,2211,3475,3583,3757,4673,
4716`; `kb_activity.rs:433,624,677,778`; `kb_inline.rs:109-129`).

### 12.9 Retention and cleanup

- **SPEC-12.9.1**: Deleting a category must not delete the articles
  filed under it; they become uncategorized (consistent with
  SPEC-12.2.2's uncategorized bucket, `knowledge_base.rs:807-810`).
  Deleting an article is permanent ("This cannot be undone.",
  `knowledge_base.rs:1930-1937`).
- **SPEC-12.9.2**: Restoring a version must create a new version
  (`current_version + 1`), never truncate or delete history
  (`knowledge_base.rs:2910-2911`).
- **SPEC-12.9.3**: Canceling a create flow after an image upload
  auto-created the article must `DELETE` that article, cascading its
  attachments (`knowledge_base.rs:4394-4431`).
- **SPEC-12.9.4**: Deleting a comment must not remove its position in
  the thread: the body is replaced with "Comment deleted" and the row
  stays in place so sibling replies keep their on-screen order
  (`kb_activity.rs:613,654-655`). Resolving a thread must not delete or
  hide it from the data model, only fold its on-screen presentation
  until the `kb_activity_show_resolved` preference is toggled back on.
- **SPEC-12.9.5**: "Comment on selection" cancel leaves no server-side
  trace of an uncommitted comment; only the local pending-composer
  signal is cleared (`kb_inline.rs:266`).

## 13. Assets, contracts, quotes and projects

Scope: `src/pages/assets.rs`, `src/pages/asset_catalog_ui.rs`,
`src/pages/asset_drawer.rs`, `src/pages/contracts.rs`,
`src/pages/quotes.rs`, `src/pages/projects.rs`.

All six files implement behavior and carry at least one `SPEC-13.`
citation.

### 13.1 Interfaces

- **SPEC-13.1.1**: `AssetListPage` (`src/pages/assets.rs`) must list
  assets via a structured `POST /assets/search` query (free text plus
  parsed filter tokens), falling back automatically to the legacy
  `GET /assets` query-string form on a 404 from the structured
  endpoint, and must render a facet sidebar from `POST /assets/search/facets`
  that fails silently (no banner) on any error.
- **SPEC-13.1.2**: `AssetListPage` must render a free-text search box
  that accepts the DSL described in SPEC-13.2.2, a company/status/type
  filter bar, a column-visibility control persisted per SPEC-13.3.3,
  row selection with bulk edit and bulk delete, and, when the
  `assets:catalog` capability is on, a "Bulk create" action opening
  `AssetBulkCreateModal` and a dismissible `SignatureBanner` above the
  list.
- **SPEC-13.1.3**: `AssetListPage` must open `AssetDrawer`
  (`src/pages/asset_drawer.rs`) for a row click when the
  `assets:inline_edit` capability is on, and must navigate to
  `Route::AssetDetail` for that same click otherwise.
- **SPEC-13.1.4**: `AssetDetailPage` (`src/pages/assets.rs`) must render
  an editable field panel, a Relationships panel (list, add, remove), a
  Credentials panel (list, add, audited reveal, delete), a Configuration
  Items panel (list, audited reveal, delete), a Change History panel, a
  Related Tickets panel from `GET /tickets?asset_id={id}`, and, when the
  `assets:report_issue` capability is on, a "Report an issue" action
  that creates a linked ticket via `POST /assets/{id}/report-issue` and
  navigates to it.
- **SPEC-13.1.5**: `AssetDrawer` must render a right-side panel with one
  `InlineFieldEditor` per field in the Identity, Hardware, Dates,
  Lifecycle and Notes groups, must close on an Escape keypress, on a
  backdrop click, and on its own Close button, and must link to the
  full `Route::AssetDetail` page for the destructive actions (delete,
  retire) it does not itself expose.
- **SPEC-13.1.6**: `SignatureBanner` (`src/pages/asset_catalog_ui.rs`)
  must render a "Save as template" prompt above the asset list whenever
  `GET /asset-models/signature-detector` returns a candidate with a
  recent-count of at least 1, and must render nothing while dismissed,
  while loading, or when the lookup fails or 404s.
- **SPEC-13.1.7**: `AssetBulkCreateModal` (`src/pages/asset_catalog_ui.rs`)
  must render a model typeahead against `GET /asset-models?q=`, a
  company select, and a serials textarea, and must submit the batch via
  a single `POST /assets/bulk-create` once a model, a company and at
  least one serial to create are present.
- **SPEC-13.1.8**: `ContractListPage` (`src/pages/contracts.rs`) must
  list contracts from `GET /contracts` with company, status and type
  filters, and `ContractNewPage`/`ContractEditPage` must render the same
  create/edit form (name, company, type, status, value, dates, contract
  number, notes, and, on create only, initial line items).
- **SPEC-13.1.9**: `ContractDetailPage` (`src/pages/contracts.rs`) must
  render a Summary card, an editable hour-balance allotment, a Line
  Items card (list, add, edit, delete) backed by `GET /contracts/{id}/items`,
  an hour-balance card backed by `GET /contracts/{id}/hour-balance`, and
  a Change History panel.
- **SPEC-13.1.10**: `RateCardListPage` and `RateCardDetailPage`
  (`src/pages/contracts.rs`) must list, create, edit and delete rate
  cards and, within a card's detail, list, add, edit and delete its
  per-work-type rates.
- **SPEC-13.1.11**: `QuoteListPage` (`src/pages/quotes.rs`) must list
  quotes from `GET /quotes` with company, status and free-text filters,
  and `QuoteDetailPage` must render either the staff lifecycle view or,
  for a contact session, `CustomerQuoteView`, chosen per SPEC-13.4.2.
- **SPEC-13.1.12**: The staff quote lifecycle view (`QuoteDetailPage`,
  `src/pages/quotes.rs`) must render an Actions card offering only the
  subset of Submit, Approve, Reject, Send, Convert and Cancel that
  `crate::modules::quotes::status` currently allows, and must render a
  send dialog, a convert-to-project dialog, and an embedded
  `ApprovalsSection` for internal sign-off.
- **SPEC-13.1.13**: `CustomerQuoteView` (`src/pages/quotes.rs`) must
  render Accept and Decline actions only while the quote's status is
  one `crate::modules::quotes::status::awaiting_client` accepts and the
  `quotes:accept` capability is on, and must render a PDF preview or
  download action gated on the `quotes:download_pdf` capability.
- **SPEC-13.1.14**: The quote create/edit form (`src/pages/quotes.rs`)
  must render company (locked once created), title, summary,
  description, valid-until date, tax amount, and a repeating line-item
  editor (type, description, quantity, unit price), and must disable
  content editing entirely once the quote's status leaves the set
  `crate::modules::quotes::status::allows_content_edit` accepts.
- **SPEC-13.1.15**: `ProjectListPage` (`src/pages/projects.rs`) must
  list projects from `GET /projects` with company, status and free-text
  filters and must render status-count and total-budget stat cards from
  `GET /projects/summary`.
- **SPEC-13.1.16**: `ProjectDetailPage` (`src/pages/projects.rs`) must
  render an Overview card (description, budget, hours, progress), a
  checklist-item toggle inside a rendered Markdown description that
  calls back to the server, Edit and Delete actions, and a Change
  History panel.
- **SPEC-13.1.17**: `ProjectTasksPage` (`src/pages/projects.rs`) must
  render a task table with create, edit and delete affordances, each
  task's own change-history modal, and must disable "Add Task" whenever
  the owning project's status is `"cancelled"`.

### 13.2 Data model

This scope owns no persisted application schema; it defines the wire
DTOs these pages exchange with the API server and the client-side
status/type vocabularies below.

- **SPEC-13.2.1**: `assets.rs`'s `RemoteAsset` must decode every
  optional CMDB field (company, asset type, assigned user, contact,
  network identity, lifecycle dates) with `#[serde(default)]`, so a
  server response omitting any of them still decodes, and must render
  the asset-status options `active`, `in_stock`, `in_repair`, `retired`
  and `inactive` on both the list and the edit form.
- **SPEC-13.2.2**: `AssetListPage`'s search box must parse the tokens
  `status:`, `tag:`, `warranty:<days>`, `eol:expired`, `dept:`/`department:`,
  `manuf:`/`make:`/`manufacturer:`, `model:` and `company:"<name>"`
  (case-insensitive, double-quoted for a multi-word company name), and
  must treat an unrecognized key or an unparseable value as free text
  rather than rejecting the query.
- **SPEC-13.2.3**: `contracts.rs` must render the contract-type options
  `managed_services`, `block_hours`, `time_and_materials`, `fixed_price`
  and `warranty`, the contract-status options `draft`, `active`,
  `expired` and `cancelled` with no client-side transition restriction
  among them, the billing-cycle options `monthly`, `quarterly`,
  `annually`, `one_time`, `weekly` and `bi_weekly`, and the line-item-type
  options `recurring_service`, `block_hours`, `retainer`, `product` and
  `one_time`, each matching the server's corresponding CHECK constraint.
- **SPEC-13.2.4**: `contracts.rs`'s `ContractItemFormState` must retain
  the original `ContractItemResponse` alongside the edited fields and
  must send it back merged into the `PUT /contract-items/{id}` body, so
  a field this form does not expose (description, billing frequency,
  work type, included hours, overage rate, rollover settings, sort
  order, product id) is never wiped by an edit.
- **SPEC-13.2.5**: `quotes.rs` must render the quote-status vocabulary
  `draft`, `submitted`, `approved`, `rejected`, `sent`, `accepted`,
  `declined`, `expired`, `converted` and `cancelled`, and the line-type
  options `service`, `product`, `labour`, `expense`, `adjustment` and
  `discount`, and must derive every status-gated affordance from
  `crate::modules::quotes::status`, never from a locally re-derived
  rule.
- **SPEC-13.2.6**: `projects.rs`'s `RemoteProject` and `RemoteTask` must
  decode every money and hours field with a number-or-string tolerant
  reader (`de_flex_f64`), and must render the project-status options
  `planning` (default), `active`, `on_hold`, `completed` and
  `cancelled`, and the task-priority options `low`, `medium` (default),
  `high` and `critical`.
- **SPEC-13.2.7**: `projects.rs`'s `RemoteTask` must distinguish
  `actual_hours` (approved logged time only) from `logged_hours` (all
  non-rejected logged time) as two separate fields, and must not use
  either in place of the other.

### 13.3 Configuration

- **SPEC-13.3.1**: `AssetListPage`, `QuoteListPage`'s and
  `ProjectListPage`'s list fetch must each request 25 rows per page.
- **SPEC-13.3.2**: `AssetBulkCreateModal` must cap a single batch at
  500 serials, applied after both exact-match deduplication and the
  already-exists trim, rejecting a submit over that count with
  "Up to 500 serials per batch; split into two." rather than truncating
  the batch silently.
- **SPEC-13.3.3**: `AssetListPage`'s optional-column visibility must
  persist under the preference key `mokosh:assets:columns:v1`,
  independent of the active tenant, defaulting to every optional column
  visible.
- **SPEC-13.3.4**: `AssetBulkCreateModal`'s model typeahead and
  `SignatureBanner`'s detector call must both treat a 404 from their
  respective endpoint as the `assets_catalog` server toggle being off,
  and `AssetBulkCreateModal` must render
  "Asset catalog is not enabled on this deployment. Ask an admin to flip the `assets_catalog` toggle."
  in that case rather than an error.
- **SPEC-13.3.5**: `SignatureBanner`'s dismiss action must persist under
  the preference key `mokosh:assets:signature_dismissed:v1` for 30
  days from the click, independent of the active tenant.
- **SPEC-13.3.6**: Every network-call site in this scope that depends
  on the `app` feature (every create/edit/delete/send/convert/accept/decline
  mutation, and `projects.rs`'s task creation) must compile to a no-op,
  not a compile error, on a build that does not enable that feature,
  consistent with the gating pattern cited at SPEC-1.3.1.

### 13.4 Security

- **SPEC-13.4.1**: `ContractListPage` and `QuoteListPage` must render
  the lifecycle view only for a session with `use_can_manage_billing()`
  true or, for a contact session, the `contracts:read`/`quotes:read`
  capability; a contact session lacking that capability must see
  `PortalAccessRequired`, and a non-contact session lacking it must see
  `PermissionRequired` with the text
  "Contracts are restricted to administrator and finance roles. Ask an administrator to grant you access."
  (quotes: the same sentence with "Quotes").
- **SPEC-13.4.2**: `QuoteDetailPage` must render `CustomerQuoteView` for
  a contact session carrying the `quotes:read` capability,
  `PortalAccessRequired` for a contact session lacking it, the staff
  lifecycle view for a non-contact session with `use_can_manage_billing()`
  true or the `quotes:read` capability, and `PermissionRequired`
  otherwise.
- **SPEC-13.4.3**: `RateCardListPage` must render its list only for a
  session with `use_can_manage_billing()` true, denying every other
  session with `PermissionRequired` and the text
  "Rate cards are restricted to administrator and finance roles. Ask an administrator to grant you access."
- **SPEC-13.4.4**: `RateCardDetailPage`'s Edit Card, Add Rate and
  per-rate edit affordances must each be gated on `use_can_manage_billing()`,
  independent of the page's own (unrestricted) read access.
- **SPEC-13.4.5**: `AssetListPage`'s "Bulk create" action and
  `AssetDrawer`'s inline-edit opening must each be gated on their own
  capability (`assets:catalog`, `assets:inline_edit`); neither capability
  gates read access to the asset list or detail page itself.
- **SPEC-13.4.6**: `AssetDetailPage`'s "Report an issue" action must be
  gated on the `assets:report_issue` capability, bypassed unconditionally
  for a staff or platform session.
- **SPEC-13.4.7**: `AssetDetailPage`'s credential and configuration-item
  reveal actions must each fetch the decrypted value only on an explicit
  reveal click, through a dedicated, audited endpoint
  (`GET /credentials/{id}`, `GET /configuration-items/{id}`), never
  alongside the secret-free summary list fetch.
- **SPEC-13.4.8**: `quotes.rs`'s customer-decision actions (Accept,
  Decline) must be gated on the `quotes:accept` capability in addition
  to the status check in SPEC-13.1.13; a quote's PDF preview/download
  must be gated on the `quotes:download_pdf` capability, bypassed
  unconditionally for a staff or platform session.
- **SPEC-13.4.9**: Every mutating control in this scope (create, edit,
  delete, bulk edit, bulk delete, send, convert, accept, decline,
  checklist toggle) must be disabled whenever `crate::hooks::use_can_mutate()`
  reports the server unreachable, so a write is never silently queued
  or lost; `CustomerQuoteView`'s Accept/Decline controls and the staff
  lifecycle action buttons must additionally show the tooltip
  "Can't {verb} while the server is unreachable" while disabled for
  that reason.
- **SPEC-13.4.10**: `projects.rs`'s project and task list/detail fetches
  must select a contact bearer ahead of a staff bearer when both are
  present, so a signed-in contact's request is scoped server-side to
  their own company's projects.

### 13.5 Error handling and observability

- **SPEC-13.5.1**: `AssetListPage`'s list, facets and company-filter
  fetches must each log their own failure
  (`tracing::error!("asset list load failed: {e}")`,
  `tracing::warn!("asset facets load failed: {e}")`), and the list
  fetch failure must render an inline `ErrorBanner` while the server is
  reachable and a full-page unavailable state otherwise; the facets and
  company-filter failures must render no banner at all.
- **SPEC-13.5.2**: `AssetDetailPage` must log
  `tracing::error!("asset detail load failed for {id}: {e}")` on load
  failure and must distinguish "still loading" from "failed to load" in
  its rendered state; its audit-log panel must independently track
  Loading, Ready and Failed states rather than collapsing a failure
  into an empty history.
- **SPEC-13.5.3**: The asset new/edit form must require a name (capped
  at 255 characters, message "Please enter an asset name." when blank
  and "Name must be {max} characters or fewer." when over), must cap
  manufacturer, model and serial number at 255 characters and a
  credential name at 100 characters with the same over-length message
  pattern, and must require a type ("Please pick an asset type.") and a
  company ("Please pick a company.") on create.
- **SPEC-13.5.4**: `AssetListPage`'s bulk edit and bulk delete must each
  report a full-success count ("Updated {n} asset(s)." /
  "Deleted {n} asset(s).") or a partial-failure count
  ("Updated {done} of {total}; {failures} failed." /
  "Deleted {done} of {total}; {failures} failed.") rather than failing
  the whole batch on one row's error.
- **SPEC-13.5.5**: `AssetDetailPage`'s delete, credential-delete and
  relationship-delete confirmations must surface the server's own
  refusal reason when the delete is rejected, rather than closing the
  dialog silently.
- **SPEC-13.5.6**: `AssetBulkCreateModal` must report, distinctly, the
  count of duplicate serials dropped (with a five-item preview and an
  "and N more" suffix), the count already belonging to an asset at the
  selected company, and the final count to be created, updating all
  three as the operator types.
- **SPEC-13.5.7**: `ContractListPage`'s list fetch must log
  `tracing::error!("contract list load failed: {e}")` and render
  "Could not load contracts. Refresh the page to retry." while
  reachable, or a full-page unavailable state otherwise; its company
  filter must log its own failure separately and degrade to an empty
  dropdown with no banner.
- **SPEC-13.5.8**: The contract create/edit form must collect every
  field error before blocking submit (not fail on the first): name
  required and capped at 200 characters, value optional but non-negative,
  at most two decimal places and at most 10,000,000,000, start date
  required and valid, end date optional but, when set, not before the
  start date ("End date must be on or after the start date."), contract
  number optional and capped at 100 characters, notes optional and
  capped at 2000 characters; on a failed submit it must focus the first
  invalid field in the fixed order name, company, value, start date,
  end date, contract number, notes.
- **SPEC-13.5.9**: If a contract's creation succeeds but an initial line
  item's creation fails, `ContractNewPage` must not roll back the
  contract; it must show
  "Contract saved, but an item failed: {err}" and still navigate to the
  new contract's detail page.
- **SPEC-13.5.10**: `contracts.rs`'s line item, allotment, rate card and
  rate add/edit/delete actions must each report their own failure with
  a verb-specific message ("Could not save line item: {err}", "Could
  not delete line item: {err}", "Could not save allotment: {err}",
  "Could not save rate card: {err}", "Could not delete rate card: {err}",
  "Could not save rate: {err}", "Could not delete rate: {err}") inside
  the acting modal, without navigating away.
- **SPEC-13.5.11**: The rate-card item form must require a work type,
  require the hourly rate as a number, accept the after-hours and
  emergency rates as optional numbers, and must reject a negative value
  on any of the three explicitly, because the input's `min="0"` is
  advisory only and does not block a pasted or programmatic negative
  value.
- **SPEC-13.5.12**: `contracts.rs`'s per-line-item parse step at submit
  time (quantity, unit price, included hours) must log a warning and
  report a row-numbered error ("Item {n} quantity is not a number", and
  the equivalent for unit price and included hours) if it disagrees
  with the form's own prior validation of that row, since the two are
  expected to always agree.
- **SPEC-13.5.13**: `QuoteListPage`'s list fetch must log
  `tracing::error!("quote list load failed: {e}")` and render
  "Could not load quotes. Refresh the page to retry." while reachable,
  or a full-page unavailable state otherwise.
- **SPEC-13.5.14**: `CustomerQuoteView` must log
  `tracing::warn!("customer quote load failed for {qid}: {e}")` on load
  failure and must render
  "This quote could not be loaded. It may not be available to your account."
  for every failure cause (not found, not yours, network error) alike,
  and must report a failed decision as
  "Your decision was not sent: {err}" without changing the quote's
  rendered status.
- **SPEC-13.5.15**: The staff quote lifecycle actions must each report
  their own failure ("Could not update the quote: {e}",
  "Could not send the quote: {e}", "Could not cancel the quote: {e}",
  "Could not convert the quote: {e}") and their own success toast
  ("Quote updated", a send-method-specific sent message, "Quote
  cancelled", "Quote converted to a project").
- **SPEC-13.5.16**: The quote create/edit form must require a company
  and a title ("Company is required.", "Title is required."), must
  require a description on any line that has a quantity or unit price
  filled in ("Line {n} description is required."), must silently drop
  any line left entirely blank from the submitted payload, and must
  reject a convert-to-project end date before its start date with
  "Target end date cannot be before the start date."
- **SPEC-13.5.17**: `ProjectListPage`'s list and summary fetches must
  each log their own failure
  (`tracing::error!("project list load failed: {e}")`,
  `tracing::error!("project summary load failed: {e}")`), and the list
  fetch failure must drive a full-page unavailable state when the
  server is unreachable. A list failure while the server is reachable
  must render "Could not load projects." instead of either empty state,
  and a summary failure must show "-" on all four stat cards rather than
  0 and $0.00.
- **SPEC-13.5.18**: The project create/edit form must collect every
  field error before blocking submit: name required and capped at 80
  characters, budget amount optional but non-negative, at most two
  decimal places and at most 999,999,999,999.99, budget hours either
  blank or a positive number/`H:MM` duration (a negative value reported
  distinctly as "must be greater than 0" rather than "must be a
  number").
- **SPEC-13.5.19**: `ProjectDetailPage`'s delete action must surface the
  server's own refusal reason on failure rather than a generic message,
  and its checklist-item toggle must show
  "Could not update checklist: {e}" on failure while still restarting
  the project resource to reconcile with the server's actual state.
- **SPEC-13.5.20**: The Add Task and task-edit forms (`ProjectTasksPage`)
  must each require a title, must validate an optional due date against
  the range 2000-01-01 to 2100-12-31 with
  "{label} must be a valid date." / "{label} must be between 2000 and 2100.",
  and must validate an optional estimated-hours value as a number or an
  `H:MM` duration with "Estimated hours must be a number (2.5) or H:MM (1:30)."
- **SPEC-13.5.21**: `projects.rs`'s project-manager fallback at
  `ProjectDetailPage` (`src/pages/projects.rs:1538-1548`) must resolve
  an id-only project manager against the loaded user roster, the same
  way the task-assignee fallback at `src/pages/projects.rs:2242-2247`
  does; the fallback resolves through `user_name_with_tooltip_prefer`
  with the loaded roster, matching the task-assignee fallback.

- **SPEC-13.5.22**: An empty `ProjectListPage` must say "No projects
  yet" with New Project only when no search text and no status filter
  is set; with either set it must say "No projects match the current
  filters" with a Clear filters action that also returns to page 1
  (`filters_active`).
- **SPEC-13.5.23**: A task status the server names but the task-status
  cache has no row for must render under that name as a Blue
  (in-flight) badge, Green only when the cached row is completed, and
  must not be logged as an unresolved reference; only a status with no
  server-sent name and no cached row is the Gray "Unknown"
  (`task_status_badge_prefer`).
- **SPEC-13.5.24**: The project Time card's Remaining must be "-" when
  the project has no budget hours, never a negative figure computed
  from a budget of zero (`remaining_hours`).
- **SPEC-13.5.25**: A failed load of the project's tasks, the project's
  change history, or a task's change history must be logged and
  rendered as "Could not load tasks." / "Could not load the change
  history.", never as "No tasks yet." or "No edits yet."

### 13.6 Concurrency and ordering

- **SPEC-13.6.1**: Every list page in this scope (`AssetListPage`,
  `ContractListPage`, `RateCardListPage`, `QuoteListPage`,
  `ProjectListPage`) must debounce its free-text search 300 milliseconds
  after the last keystroke before refetching, and must reset the
  current page to 1 on any change to a filter (company, status, type)
  or to the search text.
- **SPEC-13.6.2**: `AssetBulkCreateModal`'s pasted-serials collision
  lookup (`POST /assets/exists`) must debounce 400 milliseconds after
  the last keystroke or company change before firing.
- **SPEC-13.6.3**: Every data-fetching resource in this scope must read
  `active_tenant_generation()` inside its own async closure (not
  outside it), so a tenant switch invalidates and reruns every
  in-flight fetch rather than returning stale-tenant data.
- **SPEC-13.6.4**: `AssetListPage`'s, `ContractListPage`'s,
  `QuoteListPage`'s and `ProjectListPage`'s primary list/detail
  resources must also track `use_server_reachable()`, so each refetches
  automatically on reconnect rather than requiring a manual refresh.
- **SPEC-13.6.5**: Every submit handler in this scope (asset, contract,
  rate card, rate, quote, project, task forms) must ignore a second
  submit while its own prior submit is still in flight, guarded by a
  dedicated `is_submitting`/`saving`/`deleting`-style signal checked
  before the request is sent.
- **SPEC-13.6.6**: After a successful contract line-item or allotment
  save, both the items resource and the hour-balance resource (and, on
  the detail page, the contract resource) must be restarted so the
  dependent totals are not left stale.
- **SPEC-13.6.7**: After a successful project edit or checklist toggle,
  both the project resource and the project-history resource must be
  restarted; after a successful task create, edit or delete, the tasks
  resource must be restarted.
- **SPEC-13.6.8**: `AssetBulkCreateModal`'s serial deduplication must be
  exact-match (case- and whitespace-sensitive), must preserve first
  occurrence order, and must report each duplicated value exactly once
  regardless of how many times it repeats.
- **SPEC-13.6.9**: The quote editor's `lines` and `line_errors` arrays
  must be kept the same length on every add, remove and submit-time
  resize, and the editor must seed its fields from the server exactly
  once per mount (guarded by a `loaded` flag) so a background refetch
  of the quote being edited never overwrites in-progress edits.
- **SPEC-13.6.10**: `AssetDrawer`'s field editors must share one
  `open_field` registry so opening one field's editor closes any other
  field's editor already open in the same drawer.

### 13.7 Integrations

- **SPEC-13.7.1**: `AssetListPage` must call
  `POST /assets/search`/`GET /assets` (with legacy fallback),
  `POST /assets/search/facets` and `GET /contacts/companies`;
  `AssetDetailPage` must call `GET /assets/{id}`, `PUT /assets/{id}`,
  `DELETE /assets/{id}`, `GET /assets/{id}/relationships`,
  `POST /assets/{id}/relationships`, `DELETE /asset-relationships/{id}`,
  `GET /assets/{id}/credentials`, `POST /assets/{id}/credentials`,
  `DELETE /credentials/{id}`, `GET /credentials/{id}`,
  `GET /assets/{id}/configuration-items`, `GET /configuration-items/{id}`,
  `GET /assets/{id}/audit-log`, `GET /tickets?asset_id={id}`,
  `POST /assets/{id}/report-issue`, `GET /auth/users` and
  `GET /asset-types`.
- **SPEC-13.7.2**: `AssetBulkCreateModal` must call
  `GET /asset-models?q=`, `POST /assets/exists` and
  `POST /assets/bulk-create`; `SignatureBanner` must call
  `GET /asset-models/signature-detector`.
- **SPEC-13.7.3**: `AssetDrawer` must call `GET /assets/{id}` to load
  its snapshot, and each field save goes through `InlineFieldEditor`'s
  own `PATCH /assets/{id}` keyed by the field's `patch_field` name.
- **SPEC-13.7.4**: `contracts.rs` must call `GET /contracts`,
  `POST /contracts`, `GET /contracts/{id}`, `PUT /contracts/{id}`,
  `DELETE /contracts/{id}`, `GET /contracts/{id}/items`,
  `POST /contracts/{id}/items`, `PUT /contract-items/{id}`,
  `DELETE /contract-items/{id}`, `GET /contracts/{id}/hour-balance`,
  `GET /contacts/companies` and `GET /rate-cards` (used both paginated
  for the list and unpaginated as the only way to look up one rate card
  by id, since no `GET /rate-cards/{id}` exists).
- **SPEC-13.7.5**: `contracts.rs` must call `GET /rate-cards/{id}/items`,
  `POST /rate-cards`, `PUT /rate-cards/{id}`, `DELETE /rate-cards/{id}`,
  `POST /rate-cards/{id}/items` (upsert, keyed on `work_type_id`) and
  `DELETE /rate-card-items/{id}`.
- **SPEC-13.7.6**: `quotes.rs` must call `GET /quotes`, `GET /quotes/{id}`,
  `POST /quotes`, `PUT /quotes/{id}`, `DELETE /quotes/{id}`,
  `POST /quotes/{id}/accept`, `POST /quotes/{id}/decline`,
  `POST /quotes/{id}/send`, `POST /quotes/{id}/convert`,
  `GET /quotes/{id}/delivery-options`, `GET /quotes/{id}/pdf` and
  `GET /contacts/companies`.
- **SPEC-13.7.7**: `quotes.rs` must reuse `crate::pages::billing`'s sent-row
  rendering, delivery-option lookup and sent-date formatting rather than
  reimplementing them, and must embed `crate::pages::tickets::ApprovalsSection`
  (scoped to the `"quotes"` entity segment) for internal sign-off.
  A successful quote conversion must link to `Route::ProjectDetail` for
  the resulting project.
- **SPEC-13.7.8**: `projects.rs` must call `GET /projects`,
  `GET /projects/summary`, `POST /projects`, `GET /projects/{id}`,
  `PUT /projects/{id}`, `DELETE /projects/{id}`,
  `GET /projects/{id}/tasks`, `POST /projects/{id}/tasks`,
  `PUT /tasks/{id}`, `DELETE /tasks/{id}`,
  `GET /audit-log/entity/projects/{id}` and
  `GET /audit-log/entity/tasks/{id}`.
- **SPEC-13.7.9**: `projects.rs` must resolve its company, project
  manager, task assignee and task status options through the shared
  `CompanyPicker`, `use_user_roster` and `use_task_statuses` hooks, and
  must prefer each row's server-sent display name over a client-side
  roster join wherever the server sends one (see SPEC-13.5.21 for the
  project-manager fallback).

### 13.8 Build and deployment

- **SPEC-13.8.1**: None of the files in this section's scope add a
  build or packaging step of their own; they compile under the same
  `app`/`web`/`desktop` feature gating `src/main.rs` and `Cargo.toml`
  define (see SPEC-1.3.1).
- **SPEC-13.8.2**: `assets.rs`'s, `quotes.rs`'s and `projects.rs`'s
  create-mutation bodies are wrapped in `#[cfg(feature = "app")]`;
  `projects.rs`'s task-creation body is too, but its project-edit,
  project-delete, checklist-toggle, task-edit and task-delete bodies are
  not. Both facts are as the code stands: this section states the
  asymmetry as observed behavior, not as a defect to fix.

### 13.9 Retention and cleanup

- **SPEC-13.9.1**: `AssetDetailPage`'s bulk-delete and single-delete
  confirmations must state that the asset's credentials, configuration
  items and relationships are also removed and that the action "cannot
  be undone"; credential and relationship removal confirmations must
  carry the same "cannot be undone" wording. None of the three has an
  undo path; a retired or deleted asset is recreated, not restored.
- **SPEC-13.9.2**: `ContractDetailPage`'s delete, line-item delete,
  `RateCardDetailPage`'s delete, and rate delete confirmations must each
  state "This cannot be undone." before the request is sent; none has
  an undo path.
- **SPEC-13.9.3**: `CustomerQuoteView`'s cancel confirmation must state
  that the customer can no longer accept the quote and that the action
  "cannot be undone"; a cancelled quote is terminal (no undo, no
  reopen).
- **SPEC-13.9.4**: The quote editor must never allow removing a line
  down to zero: the Remove-line control must stay disabled once exactly
  one line remains.
- **SPEC-13.9.5**: `ProjectDetailPage`'s delete and `ProjectTasksPage`'s
  task-delete confirmations must each state "This cannot be undone.";
  neither has an undo path.
- **SPEC-13.9.6**: `AssetBulkCreateModal` must reset its model, company
  and serials fields after a successful bulk-create, so the next open
  starts from a clean state.
- **SPEC-13.9.7**: `AssetListPage` must restart its list resource after
  a drawer close, a bulk edit, a bulk delete, or a catalog bulk-create,
  so the list reflects the just-completed change without a manual
  refresh.

## 14. Time, calendar, forms, dashboards, reports and audit

Scope: `src/pages/time.rs`, `src/pages/calendar.rs`, `src/pages/forms.rs`,
`src/pages/dashboard.rs`, `src/pages/dashboards.rs`,
`src/pages/dashboards_view.rs`, `src/pages/reports.rs`,
`src/pages/audit_log.rs`, `src/pages/big_view.rs`.

### 14.1 Time

Scope: `src/pages/time.rs`.

#### 14.1.1 Interfaces

- **SPEC-14.1.9**: The time-entries list must fetch via `GET /time-entries`;
  a fetch that fails while the server stays reachable degrades to an empty
  list, while a fetch that fails because the server is unreachable renders
  the shared outage state instead.
- **SPEC-14.1.11**: The Log Time route's `?ticket_id=` and `?project_id=`
  query parameters must pre-select the corresponding work item on the entry
  form.
- **SPEC-14.1.14**: An entry's Work item, Work type, and Description are
  each required; a blank value blocks submit with a field-specific message.
- **SPEC-14.1.15**: Hours accepts either a decimal value ("2.5") or an
  H:MM value ("2:30"); any other shape is rejected.
- **SPEC-14.1.16**: A client-side pre-flight blocks a submit that would
  exceed the per-day hour cap before it reaches the server.
- **SPEC-14.1.17**: A "General" work-item selection attributes the entry to
  the MSP itself rather than to a ticket or project.
- **SPEC-14.1.18**: Only a project with a matching billing configuration
  offers the billable toggle on the entry form.
- **SPEC-14.1.19**: The weekly timesheet grid is pivoted client-side from
  the user's own fetched entries; it issues no week-specific fetch of its
  own.
- **SPEC-14.1.20**: A pending (submitted) week can only be withdrawn back to
  draft; a draft or rejected week can only be (re)submitted; an approved
  week can be neither withdrawn nor resubmitted.
- **SPEC-14.1.21**: Submitting a week requires the certification checkbox
  to be checked; submit is blocked until it is.
- **SPEC-14.1.24**: The approvals queue, and its approve, reject, and
  history actions, are available only to a manager; a non-manager sees a
  fixed notice in place of the queue.
- **SPEC-14.1.25**: When the approvals queue's status filter is "pending",
  a week that was never submitted is excluded, since "pending" means
  awaiting approval, not simply unsubmitted.
- **SPEC-14.1.26**: Rejecting a submitted week requires a non-empty reason;
  the reject action is blocked until one is entered.
- **SPEC-14.1.28**: The approval history modal is read-only; it stays
  enabled regardless of the server's reachability state.
- **SPEC-14.1.31**: The entry edit modal cannot change an entry's underlying
  work item (ticket or project); that association is fixed at creation.
- **SPEC-14.1.32**: On the edit form, Work type and Date are both required.
- **SPEC-14.1.34**: An entry's `ticket_id`/`project_id` are re-sent
  unchanged on every edit submit, since the edit form never offers to
  change them.
- **SPEC-14.1.35**: Deleting a time entry requires confirmation through the
  styled confirm dialog; the delete request fires only once confirmed.

#### 14.1.2 Data model

- **SPEC-14.1.2**: The Monday-Sunday work week is closed at both ends
  (inclusive of both boundary days) when grouping entries into a week.
- **SPEC-14.1.3**: The weekly timesheet's day-by-day pivot groups entries by
  work-item id, never by the work item's display label, so two work items
  that happen to share a label are never merged.
- **SPEC-14.1.5**: An entry's displayed label follows a fixed precedence:
  ticket first, then project (with its client name), then the general
  fallback.
- **SPEC-14.1.6**: An entry counts as the MSP's own (employee) time only
  when the server's own classification says so, never inferred client-side
  from the work item alone.
- **SPEC-14.1.7**: "Non-Billable" excludes the MSP's own (employee) time;
  the two categories are disjoint.
- **SPEC-14.1.8**: A "general" work-item selection is the only one the
  server never bills; both the Billable checkbox and the submit body key
  off this same classification.
- **SPEC-14.1.10**: Every dashboard stat tile (Today, This week, Billable,
  Non-billable, Own time) is scoped to the same user-local week.
- **SPEC-14.1.22**: A week's history is derived client-side from its
  entries and submission state, never fetched from a dedicated history
  endpoint; a "Logged" event is synthesized from an entry's own timestamps.
- **SPEC-14.1.33**: The edit form's Hours parse enforces the same decimal/
  H:MM bound as the create form (SPEC-14.1.15).

#### 14.1.3 Configuration

- **SPEC-14.1.1**: The per-entry hard upper bound on logged hours is also
  the fallback per-day cap when no explicit per-day setting is configured.
- **SPEC-14.1.12**: The per-day cap setting must hold a whole positive
  number; any other stored value is treated as unset.
- **SPEC-14.1.29**: The per-day cap line is shown only for a setting that
  is actually configured; an unset cap shows no line rather than a zero.

#### 14.1.4 Security

None: this page performs no authorization check of its own beyond the
manager-only approvals-queue gate already covered under Interfaces
(SPEC-14.1.24), which is a client-side convenience; the server is the
actual enforcement boundary for every write this page makes.

#### 14.1.5 Error handling and observability

- **SPEC-14.1.4**: An entry whose linked ticket or project cannot be
  resolved shows a placeholder label and is logged through the shared
  unresolved-reference helper rather than failing the whole list render.
- **SPEC-14.1.23**: A blank reviewer name on an approval/rejection event is
  reported as an unresolved reference rather than rendered as empty.
- **SPEC-14.1.30**: One undecodable rounding rule is skipped and logged; it
  does not fail the whole rounding-rules list.
- **SPEC-14.1.36**: Every time-tracking write (create, edit, delete,
  submit, withdraw, approve, reject) is disabled while the server is
  flagged unreachable.

#### 14.1.6 Concurrency and ordering

- **SPEC-14.1.13**: Today's running total is scoped to the user's own
  day, recomputed client-side from the already-fetched entries rather than
  a separate fetch.
- **SPEC-14.1.27**: In range mode, the approvals queue's `from`/`to` query
  is capped to a bounded span rather than allowed to request an unbounded
  range.

#### 14.1.7 Integrations

None: every endpoint this page calls (`/time-entries` and its approval
actions) is already named under Interfaces and Error handling above.

#### 14.1.8 Build and deployment

None: this file is a page component with no build-time or deployment
behavior of its own beyond the crate-wide build already covered in
Section 1.

#### 14.1.9 Retention and cleanup

None: time-entry and approval retention is owned by the server; this page
keeps no client-side cache beyond the currently fetched entries and
derived week/history views.

### 14.2 Calendar

Scope: `src/pages/calendar.rs`.

#### 14.2.1 Interfaces

- **SPEC-14.2.1**: The calendar's appointment fetch must request
  `GET /calendar/appointments?from=<rfc3339>&to=<rfc3339>` for the active
  view's visible range, with the UTC offset emitted as a literal `Z` so the
  query string survives URL decoding.
- **SPEC-14.2.2**: Creating an appointment must `POST /appointments`;
  updating a one-off appointment must `PUT /appointments/{id}`; deleting
  must `DELETE /appointments/{id}`. An expanded recurring instance cannot be
  edited or deleted individually, since its id is the series master's.
- **SPEC-14.2.3**: The calendar must offer Month, Week, and Day views,
  defaulting to Month; Month view pads to whole grid weeks (Sunday first),
  Week view is the 7 days starting Sunday, and Day view is the single
  active day.
- **SPEC-14.2.4**: The dispatch board must fetch the aggregated
  `GET /dispatch?from&to` payload (appointments, availability, time off,
  on-call) for the active Day/Week/Month range.
- **SPEC-14.2.5**: The scheduling-templates management page must list
  templates via `GET /scheduling-templates`, create via
  `POST /scheduling-templates`, update via `PUT /scheduling-templates/{id}`,
  and delete via `DELETE /scheduling-templates/{id}`.
- **SPEC-14.2.6**: Selecting a template in the appointment form must
  pre-fill type, title, location, and duration from the template's
  defaults, never clobbering an existing field with a blank, and must
  stash the template's linked ticket id for the create request.
- **SPEC-14.2.7**: An empty-area click on a month cell must open the
  create-appointment modal pre-filled with that day; an appointment chip's
  own click must open it for edit instead and stop propagation.
- **SPEC-14.2.8**: Every element that announces itself as a button
  (`role="button"`) must be activatable by Enter or Space.
- **SPEC-14.2.9**: Every date/time shown or queried by the calendar must be
  interpreted in the user's profile timezone and converted to UTC at the
  local midnight boundary, so the visible range matches what the user sees.
- **SPEC-14.2.10**: An all-day appointment's persisted span must run from
  local 00:00 of the start date to local 00:00 of the day after the
  (inclusive) end date, so a single-day all-day event is a 24-hour span the
  grids render.
- **SPEC-14.2.11**: For a timed appointment, End must follow Start plus the
  chosen preset duration and is never stored independently; the Custom path
  stores an explicit End instead.
- **SPEC-14.2.12**: An appointment whose end time has passed must render
  muted (not hidden) across every view; back-dating is intentional for
  tracking, so past appointments are shown, just dimmed.
- **SPEC-14.2.13**: The day/week time grid spans the full 24-hour day and is
  never clamped to the working-hours window; the working-hours window is a
  display hint only (initial scroll position and out-of-hours shading),
  never a scheduling bound.
- **SPEC-14.2.14**: `appointment_type` must be one of ticket, project,
  meeting, other, and `status` must be one of scheduled, in_progress,
  completed, cancelled, matching the server's CHECK constraints.
- **SPEC-14.2.15**: A scheduling template's `kind` must be one of dispatch,
  calendar, matching the server's CHECK constraint; travel buffers are
  meaningful only for a dispatch template, and a calendar template's
  buffers are always zero and never surfaced.
- **SPEC-14.2.17**: The user roster fetch is skipped for a signed-in user
  without manage-users permission, since the server-side endpoint is gated
  to Admin/Manager and would otherwise return a 403.
- **SPEC-14.2.18**: An appointment's Title is required; an empty title
  blocks submit with "Title is required."
- **SPEC-14.2.19**: An appointment must have a technician assigned (a valid
  UUID); a missing or invalid assignee blocks submit with "Pick a
  technician to assign."
- **SPEC-14.2.20**: An all-day appointment requires both a start and end
  date, and the end date must be on or after the start date, else a
  field-specific error blocks submit.
- **SPEC-14.2.21**: A timed appointment requires a valid start time; End
  must be on or after Start.
- **SPEC-14.2.22**: A recurrence value is rejected before submit unless it
  is a well-formed RFC 5545 RRULE: a known FREQ is present, every part is a
  recognised RRULE key, COUNT and UNTIL are not both set, and numeric/
  BYDAY/UNTIL/WKST values are well-formed; each failure returns a specific
  inline message.
- **SPEC-14.2.23**: A scheduling template's Name is required, Duration must
  be a positive whole number of minutes, and (dispatch kind only) the
  travel buffers must be non-negative whole numbers, each with its own
  inline error message.
- **SPEC-14.2.24**: Delete, for both an appointment and a scheduling
  template, opens a confirmation dialog; the DELETE request fires only once
  the user confirms.
- **SPEC-14.2.27**: Overlapping appointments are split into side-by-side
  lanes so no two overlapping blocks visually collide; every event in a
  connected overlap cluster shares that cluster's column count.
- **SPEC-14.2.29**: Deleting a scheduling template never deletes or
  modifies any appointment previously created from it.

#### 14.2.2 Data model

None: this page renders the server-owned `AppointmentResponse`,
`DispatchResponse`, and `SchedulingTemplateResponse` shapes; its own
`PaginatedAppointments` type is a thin decode wrapper with no independent
schema.

#### 14.2.3 Configuration

None: this file reads no environment variable or build feature of its own;
the working-hours display window and grid bounds are fixed internal
constants, not operator-facing settings.

#### 14.2.4 Security

- **SPEC-14.2.16**: Write actions (create, update, delete, for both
  appointments and scheduling templates) are disabled while the server is
  flagged unreachable.

#### 14.2.5 Error handling and observability

- **SPEC-14.2.25**: A failed fetch while the server is flagged unreachable
  renders `ContentUnavailable` instead of an empty grid, timeline, or
  list, for the calendar, the dispatch board, and the scheduling-templates
  management page alike; a failed fetch while the server is still
  reachable keeps the inline error banner and treats the result as an
  empty list.
- **SPEC-14.2.26**: A re-entrant submit is blocked until the in-flight
  request resolves; a save failure shows "Could not save appointment: 
  <server message>" and a delete failure shows "Could not delete
  appointment: <server message>" (the scheduling-template delete failure
  mirrors the same pattern without an inline `saving` guard message).

#### 14.2.6 Concurrency and ordering

- **SPEC-14.2.28**: A successful save or delete refetches the active
  resource (appointments or templates) so the view reflects server state.

#### 14.2.7 Integrations

None: every endpoint this page calls is already named under Interfaces
above.

#### 14.2.8 Build and deployment

None: this file is a page component with no build-time or deployment
behavior of its own beyond the crate-wide build already covered in
Section 1.

#### 14.2.9 Retention and cleanup

None: appointment, dispatch, and scheduling-template retention is owned by
the server; this page keeps no client-side cache beyond the current view's
fetched resources.

### 14.3 Forms

Scope: `src/pages/forms.rs`.

#### 14.3.1 Interfaces

- **SPEC-14.3.6**: Only an active form definition offers Send; a retired
  definition refuses submissions server-side.
- **SPEC-14.3.7**: A form's slug, derived from its name, must mirror
  mokosh-server's `validate_slug` shape (lowercase alphanumerics, single
  hyphens, no leading or trailing hyphen) rather than approximating it.
- **SPEC-14.3.12**: A field's reference name must mirror mokosh-server's
  `validate_field_name` shape.
- **SPEC-14.3.17**: A draft must be persisted server-side via
  `PUT /forms/drafts`.
- **SPEC-14.3.22**: Fields are previewed and sent to a client in
  `sort_order`, never arrival order.
- **SPEC-14.3.23**: The editor's own preview must render through the same
  component the public client-facing form page uses.

#### 14.3.2 Data model

- **SPEC-14.3.9**: The local draft's storage key format is distinct for a
  new definition versus an existing one, so editing two definitions cannot
  cross-contaminate.
- **SPEC-14.3.10**: A draft's persisted wire shape (`DraftForm`) is its own
  type, distinct from the editor's in-memory `EditorState`, so a draft
  written by an older build still parses; an unrecognised stored
  `field_type` falls back to Text rather than failing to deserialize.

#### 14.3.3 Configuration

- **SPEC-14.3.8**: The server-draft autosave debounce is a fixed 500ms
  internal constant, not an operator-facing setting.

#### 14.3.4 Security

- **SPEC-14.3.1**: The builder page's admin-only gate is a client-side
  convenience only; the server enforces admin on every write.

#### 14.3.5 Error handling and observability

- **SPEC-14.3.2**: A draft-list fetch failure is best-effort: logged and
  treated as no drafts, never surfaced as a page-level error.
- **SPEC-14.3.3**: An unreachable server shows `ContentUnavailable` instead
  of the list; a reachable server that still failed the fetch instead shows
  a retry banner and treats the result as an empty list.
- **SPEC-14.3.4**: A destructive draft discard, whether of a drafts-list row
  or of a just-restored draft, requires explicit confirmation before it
  fires.
- **SPEC-14.3.5**: A failed draft discard, whether client-local or
  server-held, is reported by toast rather than failing silently.
- **SPEC-14.3.13**: A field's max-length input treats blank as no limit; a
  non-integer value is rejected with an inline message and logged, never
  silently dropped.
- **SPEC-14.3.14**: A field's linked-article picker treats blank as no
  article; a non-UUID value falls back to no article and is logged.
- **SPEC-14.3.18**: A failed server-draft autosave is silent to the
  operator, logged at debug level only.
- **SPEC-14.3.21**: A save failure surfaces the server's own error message
  in the form banner.

#### 14.3.6 Concurrency and ordering

- **SPEC-14.3.11**: A server timestamp that fails to parse reads as
  arbitrarily old, never as "now"; between a local and a server-held draft
  tier, the newer-timestamped draft wins.
- **SPEC-14.3.15**: The local draft is cleared once editor state returns to
  saved.
- **SPEC-14.3.16**: A superseded debounced autosave, identified by `Rc`
  identity, drops instead of sending a stale request.
- **SPEC-14.3.19**: Discarding a restored draft clears both the local copy
  and the server-held copy; otherwise the next open would restore what was
  just discarded.
- **SPEC-14.3.20**: A successful save clears only the local draft; the
  server retires its own draft on create and update.

#### 14.3.7 Integrations

None: the drafts and definition endpoints this page calls are already
named under Interfaces, Error handling, and Concurrency above.

#### 14.3.8 Build and deployment

None: this file is a page component with no build-time or deployment
behavior of its own beyond the crate-wide build already covered in
Section 1.

#### 14.3.9 Retention and cleanup

None: a form definition's and server-side draft's retention is owned by
the server; this page's own local draft is cleared on save or discard as
described under Concurrency and ordering above, and is otherwise left to
the browser's own storage lifetime.

### 14.4 Dashboards

Scope: `src/pages/dashboard.rs`, `src/pages/dashboards.rs`,
`src/pages/dashboards_view.rs`, `src/pages/big_view.rs`.

#### 14.4.1 Interfaces

- **SPEC-14.4.1**: An unrecognized contact-dashboard activity kind must route
  to `/dashboard` rather than a dead link.
- **SPEC-14.4.3**: The contact-plane dashboard must render one capability
  check per stat tile (tickets, invoices, quotes, contracts); a tile whose
  read capability the caller lacks must not render, and the body as a whole
  renders a capability-gated stat grid plus a recent-activity list sourced
  from one summary endpoint.
- **SPEC-14.4.4**: A contact caller with no granted capability must see a
  fixed explanatory empty state instead of an all-hidden tile grid.
- **SPEC-14.4.14**: `/dashboards` must list, create (`POST /dashboards`),
  pin-default (`PATCH /dashboards/{id}`), and delete
  (`DELETE /dashboards/{id}`) the caller's own saved dashboards.
- **SPEC-14.4.17**: A saved-dashboard delete must fire only from the confirm
  dialog's confirm handler, and a second confirm while one delete is already
  in flight must be a no-op.
- **SPEC-14.4.18**: A saved-dashboard create with an empty (post-trim) name
  must be rejected client-side with no request sent.
- **SPEC-14.4.20**: The saved dashboard's `layout` blob is passed through
  opaquely by the management list page; this page reads and writes it but
  interprets none of its contents.
- **SPEC-14.4.21**: `/dashboards/:id/view` must show one saved dashboard with
  an inline editor toggle, and `/dashboard` must resolve to the caller's
  pinned saved dashboard if one exists, or the hardcoded dashboard body
  otherwise.
- **SPEC-14.4.22**: The saved dashboard's `layout` JSONB must deserialize
  into a typed schema whose `grid_col`/`grid_row` default to 1 and
  `grid_col_span` defaults to 4; a widget entry whose `widget_key` is not in
  the fixed catalog must still deserialize and render as a placeholder
  rather than blank-screening the page.
- **SPEC-14.4.23**: The widget catalog is a fixed set of five keys
  (`tickets_by_status`, `time_this_week`, `sla_at_risk`, `open_invoices`,
  `recent_audit_log`), shared by the renderer and the editor's add-widget
  panel.
- **SPEC-14.4.24**: A widget's grid placement must floor `grid_col`/
  `grid_row` at 1, clamp `grid_col_span` to the 1..=12 range of the 12-column
  grid, and leave `grid_row_span` floored at 1 with no upper bound.
- **SPEC-14.4.27**: Only the editor's Save control is blocked while the
  server is unreachable; Cancel and the draft-editing controls (add widget,
  per-widget coordinate inputs, remove) stay enabled because they mutate only
  the in-memory draft and never reach the server.
- **SPEC-14.4.28**: Save must round-trip the draft layout through
  `PATCH /dashboards/{id}` and bump the page's refetch version on success.
- **SPEC-14.4.31**: `/big/tickets`, `/big/dispatch`, and `/big/calendar` must
  each render full-screen and chromeless (no `AppLayout` sidebar, top bar, or
  feedback launcher).
- **SPEC-14.4.32**: The Big View `?refresh=<seconds>` query parameter must
  override the default 30-second auto-refresh tick, clamped to the 5..=600
  second range.
- **SPEC-14.4.35**: The Big Tickets view's `?status=` and `?priority=` query
  parameters must filter the fetched tickets client-side by a
  case-insensitive match against the ticket's status/priority name field.
- **SPEC-14.4.37**: `BigDispatchPage` and `BigCalendarPage` must fetch
  nothing of their own; each embeds `calendar.rs`'s `DispatchBoardPage` or
  `CalendarPage`, which owns the data fetch and its own outage state.

#### 14.4.2 Data model

None: these pages render server-owned report, saved-dashboard, and ticket/
time/invoice/audit summary payloads; the only client-owned schema is the
saved-dashboard `layout` JSONB shape covered under Interfaces (SPEC-14.4.22).

#### 14.4.3 Configuration

None: these files read no environment variable or build feature of their
own; the Big View routes' refresh interval is a per-request URL parameter,
not a build-time or operator config setting.

#### 14.4.4 Security

- **SPEC-14.4.16**: Saved-dashboard create, pin-default, and delete must all
  be blocked client-side (their controls disabled) while the server is
  flagged unreachable, so a write cannot be attempted and silently fail.

#### 14.4.5 Error handling and observability

- **SPEC-14.4.2**: The contact-vs-staff dashboard body choice must happen
  before any hook fires, so each body's own component owns an unconditional
  hook order regardless of which body renders.
- **SPEC-14.4.5**: On the staff dashboard, the primary report resource's
  failure must render the shared `ContentUnavailable` state instead of a
  zeroed or empty dashboard.
- **SPEC-14.4.6**: A dashboard's loading state must be captured before its
  resource value is consumed, so the body can show a loading skeleton on the
  first fetch instead of transiently rendering zeroed stats or empty tables.
- **SPEC-14.4.7**: Each dashboard tile's resource fetch must be scoped to
  exactly what that tile needs (a top-5 ticket fetch, a time-entry sum
  bounded to the current server-filtered week) rather than reading the
  server's default page and treating it as the whole table.
- **SPEC-14.4.8**: The active-projects tile must read only `meta.total` off
  a one-row `?status=active&per_page=1` request; no field of a project row
  is ever rendered.
- **SPEC-14.4.9**: The TV board's team-scoped resources (tickets, KPI
  aggregate, technician roster) must scope to the single pinned team when
  one is set, and otherwise to the server-resolved union of the caller's own
  teams; a non-privileged viewer's roster lookup must come back as an empty
  cached roster rather than erroring.
- **SPEC-14.4.11**: A failed dispatch fetch on the TV board must log the
  failure and fall back to an empty board rather than surfacing an error
  state.
- **SPEC-14.4.12**: A roster user with no server-side name, and an
  appointment's technician id with no matching roster entry, must each log
  an unresolved reference through the shared helper and fall back to
  "Unknown"/a short id prefix.
- **SPEC-14.4.13**: An unavailable SLA fetch on the TV board must render an
  explicit "SLA data unavailable" notice instead of a zero, while an
  independently-loaded tile (open tickets) still renders normally.
- **SPEC-14.4.15**: An outage on the saved-dashboards list renders
  `ContentUnavailable`; a fetch failure while the server stays reachable
  (a 4xx) degrades to an empty list and the normal empty-state instead.
- **SPEC-14.4.25**: A load failure on a single saved dashboard's primary
  resource renders `ContentUnavailable` rather than a perpetual loading
  state.
- **SPEC-14.4.26**: On `/dashboard`, an outage on the pinned-default lookup
  renders `ContentUnavailable`; a genuine absence of a pinned dashboard
  (a successful fetch returning none) falls through to the hardcoded
  dashboard body instead.
- **SPEC-14.4.29**: Each dashboard widget must render loading, unavailable,
  and empty as three distinct states rather than collapsing a failed fetch
  and a genuinely empty result to the same zero.
- **SPEC-14.4.30**: Each dashboard widget's fetch must be backed by a
  server-side summary/aggregate endpoint (dashboard report, time-entries
  summary, invoices summary, a 5-row audit-log page) rather than a
  client-side paging loop over the full collection.
- **SPEC-14.4.36**: A Big Tickets card's assignee id with no server-sent name
  must log an unresolved reference and fall back to "Unassigned".

#### 14.4.6 Concurrency and ordering

- **SPEC-14.4.10**: The TV board's periodic refresh loop, which drives every
  resource's auto-refetch, must run only under the "app" feature.
- **SPEC-14.4.19**: Bumping the saved-dashboards list page's refetch version
  after a create, pin, or delete must re-run the list fetch in the same
  update cycle.
- **SPEC-14.4.33**: The Big View periodic-tick loop, which drives each
  route's auto-refresh, must run only under the "app" feature.
- **SPEC-14.4.34**: The Big Tickets fetch requests up to 50 tickets in the
  shared recent-first sort order; an outage renders an inline notice inside
  this page's own `BigLayout`, never the `AppLayout`-mounting
  `ContentUnavailable`.

#### 14.4.7 Integrations

None: every integration point here (the dashboard report, saved-dashboards,
widget summary, and Big View ticket/dispatch endpoints) is already named
under Interfaces and Error handling above.

#### 14.4.8 Build and deployment

None: these files are page components with no build-time or deployment
behavior of their own beyond the crate-wide build already covered in
Section 1.

#### 14.4.9 Retention and cleanup

None: these pages hold no state beyond the currently fetched report, saved
dashboard, or widget data; retention of saved dashboards and their layout is
owned by the server.

### 14.5 Reports

Scope: `src/pages/reports.rs`.

#### 14.5.1 Interfaces

- **SPEC-14.5.1**: The reports landing page must render a static, hard-coded
  catalog of report categories and report links; it must issue no fetch and
  expose no write control.
- **SPEC-14.5.2**: The report detail route's `report_type` path parameter
  must select both the page title and, through the report-kind mapping (see
  SPEC-14.5.6), which backend report the page reads.
- **SPEC-14.5.3**: A `report_type` of `report-builder` must render the
  interactive custom report builder instead of a fixed report view.
- **SPEC-14.5.4**: A supported report must show "Download CSV" and
  "Download PDF" actions whose target is `/reports/<key>/export` with
  `format=csv` or `format=pdf`, where `<key>` is the single backend report
  the page's kind reads (see SPEC-14.5.16); an unsupported kind must show
  neither action.
- **SPEC-14.5.5**: The custom report builder must fetch its source/dimension/
  measure catalog from `GET /reports/custom/schema`, and must enable running
  a report only once a source and at least one measure are chosen; running a
  report must `POST /reports/custom` a spec containing the chosen source,
  dimensions, measures, an optional `from`/`to` date range, and a fixed row
  limit of 500.
- **SPEC-14.5.6**: `report_type` must map to exactly one of five backend
  report kinds (Tickets, Time, Billing, Projects, Clients) by a fixed table
  of sixteen known `report_type` values; any `report_type` outside that
  table must map to the Unsupported kind.
- **SPEC-14.5.13**: An Unsupported report kind must render exactly this
  message: "This report isn't available yet. The reports service powers the
  ticket, time, billing, project, and client reports. The custom report
  builder is planned but not implemented."
- **SPEC-14.5.15**: Changing the custom report builder's data source must
  clear the chosen dimensions, the chosen measures, and any prior run
  result, in the same update as the source change.
- **SPEC-14.5.17**: A breakdown or summary value must be parsed for charting
  as a plain integer, a decimal, a currency string (for example `$1,234.56`
  or `1,234.56 USD`), or a percentage (for example `87%`); a value that does
  not parse as one of these must be dropped from the chart while remaining
  in the table.

#### 14.5.2 Data model

- **SPEC-14.5.7**: Every field of every backend report payload struct
  (`Bucket`, `IdCount`, `TicketsReport`, `TimeReport`, `AgingBucket`,
  `BillingReport`, `DashReport`, `ProjectsReport`, `ClientsReport`, and the
  custom builder's `CustomResult` columns/rows/totals) must default rather
  than fail deserialization when the field is missing or does not parse.

#### 14.5.3 Configuration

None: this file reads no environment variable, build feature, or config
file of its own.

#### 14.5.4 Security

- **SPEC-14.5.8**: The billing report's role requirement is enforced only by
  the server; a session lacking the required role must still receive a
  supported (but empty) billing view from a `403`, never a client-side role
  check that would suppress the fetch.

#### 14.5.5 Error handling and observability

- **SPEC-14.5.9**: A primary report fetch that fails while the server is
  still reachable (a role-gated `403`, a one-off decode error) must degrade
  to that report type's default value; a primary report fetch that fails
  because the server is unreachable must propagate as an error so the page
  renders the outage state instead of a report full of zeros.
- **SPEC-14.5.10**: A failure of the Tickets kind's secondary KPI aggregate
  fetch (`/reports/dashboard`) must log an error and degrade the SLA
  warning/breach figures to zero rather than failing the whole report.
- **SPEC-14.5.11**: A failed schema fetch in the custom report builder must
  render exactly this message: "Could not load the report catalog. The
  reports service may be unavailable."
- **SPEC-14.5.12**: A failed custom report run must render "Report failed:
  {error}" using the run's own error message.

#### 14.5.6 Concurrency and ordering

- **SPEC-14.5.14**: The report-view resource hook must be called
  unconditionally, before the report-builder's early return, so the
  component's hook order stays stable when the route's `report_type`
  parameter flips between the builder and a fixed report.

#### 14.5.7 Integrations

- **SPEC-14.5.16**: The five backend report endpoints this page reads, one
  per kind, are: `/reports/tickets` (plus `/reports/dashboard` for the SLA
  KPI aggregate), `/reports/time`, `/reports/billing`, `/reports/projects`,
  and `/reports/clients`; the custom builder additionally reads
  `/reports/custom/schema` and writes `/reports/custom`.

#### 14.5.8 Build and deployment

None: this file is a page component with no build-time or deployment
behavior of its own beyond the crate-wide build already covered in Section 1.

#### 14.5.9 Retention and cleanup

None: this page holds no state beyond the current report view and the
in-progress custom report run; it persists nothing of its own.

### 14.6 Audit log

Scope: `src/pages/audit_log.rs`.

#### 14.6.1 Interfaces

- **SPEC-14.6.1**: The audit log route must render the full filterable table
  only for a user whose cached role satisfies the admin check; a non-admin
  user must see a fixed "Admins only" notice instead, with the data-fetching
  hooks never mounted for that render (they live in a separate inner
  component so the outer gate's hook order never changes with auth state).
- **SPEC-14.6.2**: The audit list fetch must request
  `GET /audit-log?page=<n>&per_page=25`, with `entity_type`, `action`,
  `user_id`, `from`, and `to` query parameters appended only when their
  filter value is non-empty.
- **SPEC-14.6.3**: A non-empty `from`/`to` date filter must be widened from
  its `YYYY-MM-DD` input value to a full UTC day before being sent: `from`
  as `<date>T00:00:00Z`, `to` as `<date>T23:59:59Z`.
- **SPEC-14.6.4**: The "Download CSV" action must link to
  `<api base>/audit-log/export.csv` with the same filter query parameters as
  the list fetch, and must set the anchor's `download` attribute to
  `audit-log-<YYYY-MM-DD>.csv` using the current UTC date computed at render
  time, overriding whatever filename the server's response would otherwise
  suggest.
- **SPEC-14.6.5**: The audit table must render exactly seven columns, in
  order: Timestamp, Action, Entity, Record, User, IP, Details.
- **SPEC-14.6.6**: The timestamp cell must use the viewer's saved date-format
  preference and profile timezone, falling back to the fixed shape
  `%b %-d, %Y %H:%M:%S %Z` when no preference is set; the cell's `<time>`
  element must always carry the full RFC3339 instant in its `datetime`
  attribute regardless of the display format.
- **SPEC-14.6.7**: The action cell must render a badge whose color is fixed
  per verb: `create` green, `update` blue, `delete` red, `login` purple,
  `logout` gray, and any other verb gray.
- **SPEC-14.6.8**: The entity and user cells must show the entry's resolved
  name when it is non-empty; otherwise the entity cell shows the first 8
  characters of the entity id (or "-" when absent) and the user cell shows
  "System".
- **SPEC-14.6.9**: The Details toggle control must render only when the
  entry carries `old_values`, `new_values`, or a user agent; otherwise the
  cell shows a plain "-" placeholder. Each row's expanded/collapsed state is
  independent of every other row.
- **SPEC-14.6.10**: The expanded detail panel must render the raw
  pretty-printed JSON of `old_values` and `new_values` (an absent side as the
  literal string "null") whenever neither side is a JSON object; whenever at
  least one side is a JSON object, it must instead render one row per key
  whose value differs between the two sides, keys from the new payload first
  (alphabetical, since the underlying map is a BTreeMap) followed by any
  key present only in the old payload, and must render "No field changes
  recorded" instead of an empty row list when every key is unchanged.
- **SPEC-14.6.11**: Within the per-field diff, a key must be rendered with
  its trailing `_id` suffix stripped and the remainder sentence-cased (for
  example `project_manager_id` becomes "Project manager"); a value for one of
  the fixed set of user-reference keys (`project_manager_id`, `manager_id`,
  `assigned_to_id`, `assigned_to`, `assigned_technician_id`,
  `technician_id`, `user_id`, `owner_id`, `created_by`, `updated_by`) must
  resolve to the matching cached user's full name when known, or else the
  first 8 characters of the id; every other value renders as its raw scalar;
  an absent, null, or blank/whitespace-only string value renders as
  "(empty)".
- **SPEC-14.6.12**: The IP cell must render a clickable control for a
  non-empty actor address, and must render inert text (never a control) when
  the entry has no address.
- **SPEC-14.6.13**: The on-demand IP enrichment result must render exactly
  one of four states: idle/loading shows a fixed "looking up" message;
  `Loaded(None)` shows a fixed explanation that nothing is available for the
  address; `Loaded(Some(..))` shows every returned (label, value) pair plus a
  fixed advisory-only disclaimer; `Failed` shows an error banner with the
  failure message and a "Try again" control (no other state offers a retry
  control).
- **SPEC-14.6.14**: The filter row must offer a free-text entity-type field,
  single-select Action and User dropdowns (Action options fixed to
  All/Create/Update/Delete/Login/Logout; User options built from the cached
  user roster by name, plus a synthetic short-id option when the active
  filter value is not in that roster), and From/To date fields; changing any
  filter's value must reset the page number to 1 in the same update.

#### 14.6.2 Data model

None: the page owns no schema of its own. It renders the server-owned
`AuditLogEntry` and `IpEnrichment` records and keeps no client-side state
beyond per-component UI signals and the current page's fetched rows.

#### 14.6.3 Configuration

None: the page reads no environment variable, build feature, or config file
of its own; its page-size constant is a fixed internal value, not an
operator-facing setting.

#### 14.6.4 Security

- **SPEC-14.6.15**: The admin-only render gate described in SPEC-14.6.1 is a
  client-side convenience only; the server's own authorization check on the
  underlying endpoints is the actual enforcement boundary.
- **SPEC-14.6.16**: The list fetch, the enrichment lookup, and the CSV
  export must all ride the viewer's existing authenticated session: the list
  and enrichment requests through the shared authenticated-fetch helper, and
  the export through a same-origin anchor navigation that relies on the
  session cookie rather than attaching its own Authorization header.

#### 14.6.5 Error handling and observability

- **SPEC-14.6.17**: A failed list fetch while the server is flagged
  unreachable must render the shared unavailable/outage state in place of
  the table; a failed list fetch while the server is still reachable must
  keep the normal table and filters and show an inline error banner with the
  failure's user-facing message.
- **SPEC-14.6.18**: A failed fetch of the secondary user roster (used for
  name resolution and the user filter) must degrade to an empty roster
  rather than block or error the page.
- **SPEC-14.6.19**: A user-reference value or actor user id that has no
  matching entry in the cached roster must still render its fallback label
  (short id prefix, or "System") and must also be logged once through the
  shared unresolved-reference helper, tagged by call site, so the
  substitution is traceable.
- **SPEC-14.6.20**: A transport failure or non-2xx response from the IP
  enrichment lookup must never render as "nothing available" (`Loaded(None)`),
  and a successful response carrying no data must never render as a failure;
  only a malformed address is expected to produce a 400 from that endpoint.

#### 14.6.6 Concurrency and ordering

- **SPEC-14.6.21**: The audit list resource must re-run whenever any filter
  value, the current page, or the active tenant generation changes, and
  must also depend on server-reachability so it refetches automatically once
  the server becomes reachable again.
- **SPEC-14.6.22**: Each row's IP enrichment lookup is independent per row
  and must fetch at most once per opened address: the click handler issues a
  request only while that row's enrichment state is idle; reopening a panel
  already in a loaded or failed state must not re-fetch.

#### 14.6.7 Integrations

- **SPEC-14.6.23**: The IP enrichment lookup calls the server's
  `/ip-enrichment` endpoint, whose contract is: a malformed address answers
  `400`; a private or reserved address, an address unknown to the dataset, or
  a deployment with no enrichment dataset configured all answer `200` with a
  JSON `null`; any other address answers `200` with a populated object; any
  transport failure or a `403` surfaces as an error to the caller.

#### 14.6.8 Build and deployment

None: this file is a page component with no build-time or deployment
behavior of its own beyond the crate-wide build already covered in Section 1.

#### 14.6.9 Retention and cleanup

None: audit-entry retention is owned by the server; this page keeps no
client-side cache of entries beyond the in-memory resource for the currently
displayed page.

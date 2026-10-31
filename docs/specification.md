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

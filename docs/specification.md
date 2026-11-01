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

## 9. Tickets, SLA and requests

Scope: `src/pages/tickets.rs`, `src/pages/approvals.rs`, `src/pages/sla.rs`,
`src/pages/request_form.rs`, `src/pages/request_links.rs`,
`src/pages/work_day.rs`.

All six files are page or sub-page components mounted by routes declared
outside this scope (`src/lib.rs`); none of the six declares a `Route`
variant of its own. `work_day.rs` exports one component, `WorkDayStrip`,
embedded by the Time page rather than being a page itself; it is in scope
because its behavior (clock in/out, break tracking, segment correction) is
part of this section's subject matter.

### 9.1 Interfaces

- **SPEC-9.1.1**: `TicketListPage` (`tickets.rs`) must render the ticket
  list body for a session holding the `tickets:read` capability and must
  render nothing usable otherwise.
- **SPEC-9.1.2**: The ticket list must fetch `/tickets/statuses`,
  `/tickets/priorities`, and `/tickets/queues` to populate the Status,
  Priority, and Queue filter selects, degrading each independently to an
  empty list on its own fetch failure.
- **SPEC-9.1.3**: The ticket list's search box must debounce input by
  300ms before the value is included in the list request.
- **SPEC-9.1.4**: Changing the search text, status filter, priority
  filter, queue filter, or sort column must reset the list to page 1.
- **SPEC-9.1.5**: The "Clear filters" control shown on a filtered empty
  state must clear the status, priority, and queue filters together, not a
  subset.
- **SPEC-9.1.6**: Bulk ticket deletion must snapshot the selected ids at
  the moment the confirmation dialog is opened, issue one `DELETE` per
  snapshotted id in parallel, then show a single combined
  success/partial-failure count, clear the selection, and refresh the list.
- **SPEC-9.1.7**: `TicketNewPage` must seed the Description field from a
  knowledge-base-article prefill and the Company field from a company
  prefill, both read from the URL query string, and must auto-select the
  tenant's default priority (or the first available priority if none is
  marked default) only while the Priority field is still empty, so it
  never overwrites an in-progress user selection.
- **SPEC-9.1.8**: The ticket create form must hide the Team field
  entirely, not merely disable it, for a personal tenant.
- **SPEC-9.1.9**: `TicketDetailPage` must render the detail body for a
  `tickets:read`-capable session and nothing usable otherwise.
- **SPEC-9.1.10**: The ticket title and description editor must edit the
  page content in place; it must never open as a modal. The detail page
  must render exactly three modals: the approval-request dialog, the
  save-view dialog, and the rename-view dialog.
- **SPEC-9.1.11**: Cancelling an untouched (non-dirty) title/description
  edit must close it immediately with no confirmation; cancelling a dirty
  edit must show a confirmation dialog first.
- **SPEC-9.1.12**: Every Markdown-bearing field on the ticket pages (the
  create-form description, the in-page description edit, the note
  composer, and inline note editing) must use the shared Markdown editor
  component, never a bare text area, and a rendered note must display
  through the shared Markdown renderer rather than as preformatted raw
  text.
- **SPEC-9.1.13**: A ticket "family" card must render only when the
  ticket is a child (showing a link to its parent) or a parent with at
  least one child (listing the children); a parent with zero children, or
  a ticket with no family relation, must render no family card.
- **SPEC-9.1.14**: The "Request approval" action on the ticket sidebar
  must be disabled whenever the session cannot mutate the ticket, and its
  dialog must require exactly one of a chosen approver user or a chosen
  approver role before it can be submitted.
- **SPEC-9.1.15**: `ApprovalsPage` (`approvals.rs`) must render exactly
  one of: a loading state, a fetch-failed state, a filtered-to-nothing
  state, an empty-queue state, or the row list, never more than one at a
  time.
- **SPEC-9.1.16**: Each pending-approval row must render a target-kind
  badge, an entity reference (a link to the ticket detail page when the
  target is a ticket and an id is present, otherwise plain unlinked text),
  a requester line, an approver line, and Approve/Reject actions.
- **SPEC-9.1.17**: `SlaManagementPage` (`sla.rs`) must render a permission
  notice instead of any tab content for a non-admin session, and otherwise
  must render exactly one of its three tabs (Policies, Business Hours,
  Holiday Calendars) at a time.
- **SPEC-9.1.18**: The Policies tab's policy form must require a
  non-empty name, and must offer a Business Hours selector seeded with a
  "No profile (runs 24/7)" option plus every loaded business-hours row.
- **SPEC-9.1.19**: The Targets modal must render one row per tenant
  ticket priority, pre-filled from any existing SLA target for that
  (policy, priority) pair, each row offering first-response hours,
  resolution hours, and an operational-hours mode of `24x7` or
  `business_hours`.
- **SPEC-9.1.20**: The Business Hours form must require a non-empty name
  and must offer a schedule as a JSON text area; the Holiday Calendars
  form must require a non-empty name and must offer a holidays list as a
  JSON text area.
- **SPEC-9.1.21**: `RequestFormPage` (`request_form.rs`) must render
  exactly one of a terminal state (submitted, already submitted, retired,
  unusable), a loading state, a load-failed state, or the live form body,
  never more than one at a time.
- **SPEC-9.1.22**: The request-form body must render every field the
  server returns, in server-given order, using the field's declared type
  to choose its widget (text, email, date, select, checkbox, or
  multi-line text), and must render an unrecognized field type as a plain
  text input rather than dropping it.
- **SPEC-9.1.23**: A multi-person request form that still has submissions
  remaining after a successful submit must offer an "add the next person"
  action that re-fetches the form definition rather than only clearing the
  local answers.
- **SPEC-9.1.24**: `CompanyRequestFormsCard` (`request_links.rs`) must
  render a per-company table of sent request-form links with a "Send a
  form" action, disabled whenever the session cannot mutate.
  `SentRequestLinksPanel` must render the same table shape across every
  company in the tenant, truncated to the 8 most recent rows with a count
  of how many older rows are hidden.
- **SPEC-9.1.25**: `SendRequestLinkModal` must require a chosen form, a
  chosen company, a people count between 1 and 50 inclusive, and at least
  one of a chosen contact or a typed email address, before its "Send
  link" action is enabled; the action must additionally be disabled, with
  a distinct reason, whenever no active form definition exists or the
  session cannot mutate.
- **SPEC-9.1.26**: `WorkDayStrip` (`work_day.rs`) must render nothing on
  the first, still-loading fetch of the day and must keep showing the
  last successfully loaded day while a later periodic refetch is in
  flight, rather than blanking the card every 60 seconds.
- **SPEC-9.1.27**: The clock card must show a "Clock in" action when the
  viewed user is clocked out; "Clock out" and, when break tracking is on
  for the tenant, "Start break"/"End break" when clocked in or on break;
  none of the three actions may render while viewing another user's day.
- **SPEC-9.1.28**: An admin viewing another user's day must be offered a
  "Viewing" selector defaulting to "My day"; the selector must render only
  when the admin's user roster is non-empty.
- **SPEC-9.1.29**: Each clocked segment must offer a "Correct" action when
  correction is allowed and no other segment's correction form is
  already open; the open correction form must offer Save, Reopen (only
  when the segment is already closed), Remove, and Cancel.

### 9.2 Data model

- **SPEC-9.2.1**: A ticket's SLA status must default to "not applicable"
  when the server omits it or sends an unrecognized value, and the local
  SLA-status enum's match against the server's own SLA-status type must
  be exhaustive, so a new server-side status value fails to compile
  rather than silently defaulting.
- **SPEC-9.2.2**: A ticket update request body's assignee and asset
  fields must each distinguish three states on the wire: omitted (leave
  unchanged), explicit null (clear/unassign), and a value (set); omitting
  the field from the request and sending it as null must never be
  conflated.
- **SPEC-9.2.3**: The ticket create request must send exactly: title,
  description, company, contact, priority, asset, type, category,
  assignee, scheduled end, source knowledge-base article, and team. All
  other fields the server's create-ticket contract accepts are
  deliberately not offered by this form.
- **SPEC-9.2.4**: The ticket update request must send exactly: title,
  description, status, priority, assignee, and asset. All other fields
  the server's update-ticket contract accepts are deliberately not
  editable from this page.
- **SPEC-9.2.5**: A ticket's displayed "Time Logged" total must be
  computed by summing the page's own fetched time entries, never read
  directly from the ticket record's own total field, so the two cannot
  disagree.
- **SPEC-9.2.6**: A note's structured follow-up flag must render as "Not
  needed" when false, as "Needed - {description} (target {date})" when
  true, and as "Invalid follow-up data" when the key is missing or is not
  a boolean (including the string `"true"`).
- **SPEC-9.2.7**: A note's type must decode as a plain string, not a
  closed set of known values, so an unrecognized note type still decodes
  and renders as a plain note rather than failing the entire journal.
- **SPEC-9.2.8**: A pending approval record must decode with every field
  except its id defaulted, so a server payload missing any of the
  polymorphic subject fields still decodes; its target kind must default
  to `"ticket"` when absent.
- **SPEC-9.2.9**: A pending approval's displayed subject must prefer a
  resolved reference or label; when both are empty it must fall back to
  an 8-character prefix of the entity id, or to a plain dash when no
  entity id exists either.
- **SPEC-9.2.10**: An SLA target's hours fields accept a plain decimal
  number or an `H:MM` duration; an empty value (after trimming) must mean
  "no commitment for this dimension," not zero.
- **SPEC-9.2.11**: An SLA target row's operational-hours mode must default
  to `24x7` whenever the stored value is empty or absent, both when the
  row is first loaded and when it is saved.
- **SPEC-9.2.12**: A request-form submission's answers must be stored as
  a single string-keyed map, with a boolean field's answer represented as
  the literal string `"true"` or `"false"`, never a native boolean.
- **SPEC-9.2.13**: A request-form submission payload must omit a blank or
  whitespace-only text answer, but must always send an explicit boolean
  for every boolean field (defaulting to false when unanswered), never
  omitting a boolean field's key.
- **SPEC-9.2.14**: A request link's status must rank, in order, submitted
  (has a used-at timestamp), expired (past its expiry), partly submitted
  (fewer submissions remain than the people count), then awaiting; this
  page must never recompute that ranking itself.
- **SPEC-9.2.15**: A work day's segment kind must default to the empty
  string when absent; any value other than the literal `"break"` must
  render as a work segment, not a validated closed set.
- **SPEC-9.2.16**: A work day record's user id, date, module-derived
  flags, segment list, and the clocked/break/logged/unlogged minute
  totals must each default to a safe empty value (no user, empty date,
  all flags false, empty segments, zero minutes) so an older or newer
  server payload still decodes.
- **SPEC-9.2.17**: Reopening an already-closed work segment must send its
  end time as an explicit null, distinct from omitting the field, which
  the wire format treats as "leave the end time alone."

### 9.3 Configuration

- **SPEC-9.3.1**: The ticket list must page at 25 rows per page.
- **SPEC-9.3.2**: The ticket journal must show at most 50 merged entries,
  with an explicit "Showing N of M" notice whenever more exist.
- **SPEC-9.3.3**: A ticket attachment must be rejected client-side before
  any upload request is sent whenever it exceeds 5 MiB.
- **SPEC-9.3.4**: A saved ticket view's name must be capped at 100
  characters on submit.
- **SPEC-9.3.5**: A contact-facing approval-request note must be between
  1 and 2000 characters.
- **SPEC-9.3.6**: A request-form link's people count is capped at 50.
- **SPEC-9.3.7**: `SentRequestLinksPanel` must show at most the 8 most
  recently sent links before truncating, with a count of how many older
  links are hidden.
- **SPEC-9.3.8**: A work day is considered stale, for client-side display
  purposes only, after 16 hours; this is a fixed client constant because
  no tenant-level setting for it exists server-side.
- **SPEC-9.3.9**: None beyond the above for `sla.rs`: it reads no
  environment variable, feature flag, or config key of its own beyond the
  shared `app`/`web` build-feature gates described in SPEC-9.8.1.

### 9.4 Security

- **SPEC-9.4.1**: The ticket Reopen action must require both a status
  name containing "closed" or "resolved" (case-insensitive) and the
  `tickets:reopen` capability; a status missing either condition must
  hide the action even when the capability is present.
- **SPEC-9.4.2**: A contact may edit a ticket's title and description only
  when the session holds the `tickets:edit_own` capability and the
  ticket's contact id exactly matches the caller's own contact id; a
  missing id on either side must fail closed (no edit access).
- **SPEC-9.4.3**: Ticket deletion must require the manager-or-above role
  tier independent of mutate/online state, showing the reason "Only a
  manager or above can delete a ticket" when that tier is absent; it must
  separately disable, with a distinct reason, whenever the server is
  unreachable, and disable with no reason while a delete for that ticket
  is already in flight.
- **SPEC-9.4.4**: A contact session must never see the ticket audit
  history, the staff approvals section, or staff-only time entries; the
  staff approvals section must be hidden entirely, not merely disabled,
  for a contact session.
- **SPEC-9.4.5**: A contact session's note composer must default new
  notes to the public note type; a staff session's composer must default
  to the internal note type.
- **SPEC-9.4.6**: A contact session's ticket edit must go through the
  partial-update endpoint (title and description only), while a staff
  session's edit must go through the full-update endpoint; the choice
  must be made by session plane, not by which fields changed.
- **SPEC-9.4.7**: A note editable per the server's own answer wins
  outright when the server supplies one; otherwise local fallback rules
  require either the admin role or (author match and the note's own
  row-state rules), and an unrecognized note type must offer no edit
  control.
- **SPEC-9.4.8**: A note that was emailed to a customer, authored by a
  contact, or of the time-entry type must never be editable, even by an
  admin, regardless of the fallback rules' outcome.
- **SPEC-9.4.9**: The "email this note to the client" control must be
  absent, not merely disabled, on any note type other than the public
  type.
- **SPEC-9.4.10**: `SlaManagementPage` must gate every one of its three
  tabs on the admin role client-side; this is a display gate only, and
  every mutating SLA endpoint remains server-enforced independent of it.
- **SPEC-9.4.11**: Every SLA-mutating control (create, edit, save, remove,
  across policies, business hours, holiday calendars, and targets) must
  be disabled, with an explanatory tooltip, whenever the server is
  unreachable.
- **SPEC-9.4.12**: `RequestFormPage` must never attach a bearer or session
  token to either its load or its submit request; the token embedded in
  the route path is the sole credential.
- **SPEC-9.4.13**: `RequestFormPage` must not disclose, in its own copy,
  why a link is unusable beyond the two 410 cases the server's own message
  text distinguishes (already submitted vs. retired); all other failure
  causes (expired, unknown, malformed token) must collapse to the single
  message "This link is expired or invalid."
- **SPEC-9.4.14**: `RequestFormPage` performs no CAPTCHA or client-side
  rate-limit tracking of its own; a 429 response is treated as
  non-terminal and surfaced through the same generic error path as any
  other non-terminal failure.
- **SPEC-9.4.15**: `SendRequestLinkModal` must offer only contacts that
  have a non-empty email address as selectable recipients.
- **SPEC-9.4.16**: `SendRequestLinkModal` must send the typed email
  address, when present, as the request's recipient address, overriding
  any address implied by a chosen contact; when the typed field is empty,
  the request must omit the recipient address and let the server resolve
  it from the chosen contact.
- **SPEC-9.4.17**: `WorkDayStrip`'s correction permission is a display
  gate only: an "off" policy must hide the Correct action for every
  viewer including the segment's own owner; an "owner_or_manager" policy
  must show it for the owner or any manager; any other or unrecognized
  policy value must fall back to owner-or-admin, matching the server's
  own documented fallback.
- **SPEC-9.4.18**: A correction time typed in `HH:MM` form must be
  resolved against the viewer's own profile time zone, never the
  browser's local zone, and must be rejected, not guessed, when that
  local time does not exist (a spring-forward gap) or is ambiguous (a
  fall-back overlap) in that zone.

### 9.5 Error handling and observability

- **SPEC-9.5.1**: A ticket detail fetch failure while the server is
  unreachable must render the shared unreachable-content state; the same
  failure while the server is reachable (a real not-found) must render a
  distinct "Ticket not found" page with a link back to the list.
- **SPEC-9.5.2**: A failed journal source (notes, history, or time
  entries) must degrade that one source out of the merged journal rather
  than failing the whole ticket detail page; the page must list which
  sources failed.
- **SPEC-9.5.3**: A note-edit refusal from the server must render next to
  the note being edited, not as a page-level toast or banner.
- **SPEC-9.5.4**: Bulk ticket deletion must report one combined
  success/partial-failure toast after all deletions settle, never one
  toast per ticket.
- **SPEC-9.5.5**: The ticket create submit must route a server field
  error to the title, description, or company field in that priority
  order before falling back to a generic banner.
- **SPEC-9.5.6**: A pending-approvals fetch failure while the server is
  unreachable must render the shared unreachable-content state; the same
  failure while the server is reachable must render an inline error
  banner instead.
- **SPEC-9.5.7**: A failed approval decision must set a single
  page-level error message and must not remove the row from the list; a
  successful decision must show a success toast and refresh the list.
  Because this error message is a single page-level field rather than
  scoped per row, a later successful decision on a different row does
  not clear an earlier row's still-visible failure message; this is
  tracked as a defect, not a requirement of this section.
- **SPEC-9.5.8**: An SLA primary-list load failure must render the shared
  unreachable-content state when the server is unreachable, and an inline
  error banner otherwise; each of the three SLA tabs (policies, business
  hours, holiday calendars) must log its own distinct failure message.
- **SPEC-9.5.9**: A business-hours save with a schedule field that fails
  client-side JSON parsing must show the parse error inline on that
  field and must send no request; an invalid IANA time zone string must
  show a dedicated error and send no request; an empty time zone must be
  accepted and coerced to `UTC`.
- **SPEC-9.5.10**: A server-side validation failure on an SLA
  business-hours or holiday-calendar save that names the schedule or
  holidays field must be shown inline on that field; any other save
  error must be shown as a page-level banner.
- **SPEC-9.5.11**: `RequestFormPage`'s client-side required-field check on
  submit must evaluate every field and collect every failing field before
  returning, never stopping at the first failure.
- **SPEC-9.5.12**: `RequestFormPage`'s server-side 422 field errors on
  submit must be split: an error naming a field that exists on the form
  goes to that field; an error naming an unknown field, or one about the
  submission as a whole, is joined into the page-level banner.
- **SPEC-9.5.13**: `request_links.rs`'s per-company and tenant-wide link
  list fetch failures must each log their own distinct message and each
  render as "Could not load request forms."/"Could not load sent forms."
  respectively, distinct from their own empty-result message.
- **SPEC-9.5.14**: A send-link failure must route a server field error
  naming the recipient address or the people count to that field; any
  other failure must fall back to a generic error banner.
- **SPEC-9.5.15**: A work-day load failure must log a warning and resolve
  to the shared "Could not load today's clock." banner, distinct from the
  module-off notice shown when the timesheets or time-tracking module is
  disabled for the tenant.
- **SPEC-9.5.16**: A failed segment-edit policy load must fall back
  silently to the default policy rather than surfacing an error to the
  user.
- **SPEC-9.5.17**: A work-day correction whose typed time does not exist
  in the viewer's time zone must show "That start/end time does not exist
  on this day in your time zone."; a work day whose own date fails to
  parse must show "This day's date could not be read, so a correction
  cannot be timed." and must send no request.

### 9.6 Concurrency and ordering

- **SPEC-9.6.1**: The ticket detail page's own resource value must be
  read through the hook's documented non-reactive accessor, not its
  reactive one, to avoid a known rendering panic and a stale Edit button.
- **SPEC-9.6.2**: Every hook called by the ticket detail body (including
  the auth hook) must be called unconditionally before any early return
  in that component, so the render framework's per-render hook ordering
  stays consistent.
- **SPEC-9.6.3**: The ticket journal's render loop must be keyed on each
  entry's own id (or, failing that, its timestamp), never on list
  position, so prepending a new entry cannot hand a newly-added row the
  in-progress draft state of whatever entry used to sit at that position.
- **SPEC-9.6.4**: The merged ticket journal (notes, history, time
  entries) must be built by a stable sort on each entry's timestamp,
  newest first, so entries sharing a timestamp keep their original
  relative order.
- **SPEC-9.6.5**: Each inline sidebar field edit (status, priority,
  assignee) on the ticket detail page must send only its own changed
  field, independent of the others, and must refresh both the ticket
  record and its history after a successful save.
- **SPEC-9.6.6**: The ticket family card's fetch must depend on whether
  the ticket has a parent: it must fetch children only when the ticket
  has no parent of its own, never fetching both a parent and a children
  list for the same ticket.
- **SPEC-9.6.7**: The pending-approvals fetch must refetch on tenant
  switch, on a server-reachability transition, and on an explicit
  version bump after a decision; the version bump and the resource
  restart after a decision both trigger a refetch, which is redundant
  but not contradictory.
- **SPEC-9.6.8**: Each SLA form's (policy, business hours, holiday
  calendar, target) save or delete action must no-op immediately if a
  save or delete for that same form or row is already in flight, so a
  double-click cannot issue a second concurrent request.
- **SPEC-9.6.9**: An SLA target's delete must be two-step: opening its
  confirmation dialog must not itself delete anything; the delete request
  fires only on explicit confirmation, and the dialog's in-flight state
  must not clear until the request settles.
- **SPEC-9.6.10**: `RequestFormPage`'s submit action must no-op
  immediately if a submission for that form is already in flight.
- **SPEC-9.6.11**: `RequestFormPage`'s "add the next person" action must
  set the page back to its loading state and clear any terminal state
  before re-fetching, so the next person can never submit against the
  stale form definition from the prior person.
- **SPEC-9.6.12**: `request_links.rs`'s send-link action must no-op
  immediately if a send for that modal is already in flight.
- **SPEC-9.6.13**: `WorkDayStrip`'s clock-in, clock-out, break, and
  segment mutations must each no-op immediately if a mutation from that
  same strip is already in flight, so only one can be outstanding at a
  time.
- **SPEC-9.6.14**: `WorkDayStrip` must run two independent timers: one
  that restarts the day fetch every 60 seconds, and one that only forces
  a re-render every 10 seconds to advance the visible elapsed time without
  a matching fetch.
- **SPEC-9.6.15**: `WorkDayStrip`'s live clocked-minutes, break-minutes,
  and session-minutes figures must all be derived from one single
  "now" reading per render, so the three figures can never disagree with
  each other within the same render.
- **SPEC-9.6.16**: `WorkDayStrip`'s module-flag check that decides
  whether to skip the day fetch must be read from inside the fetch's own
  async closure, not via the reactive module-flags hook called from the
  component body, to keep the render framework's hook hook-hook ordering
  stable across renders.

### 9.7 Integrations

- **SPEC-9.7.1**: The ticket detail page's SLA panel must fetch
  `GET /tickets/{id}/sla` on the dual-plane (contact-or-staff) bearer
  path, not the staff-only path, so a contact session can see it too.
- **SPEC-9.7.2**: An inline image uploaded from any Markdown field on the
  ticket detail page must post to the inline-attachment endpoint, distinct
  from the whole-file attach flow's endpoint; the create-ticket form's
  editor must offer no inline-image upload at all, since no ticket exists
  yet to attach to.
- **SPEC-9.7.3**: The contact-facing "Request approval" sidebar action
  must post only a note to a ticket-specific request endpoint, distinct
  from the staff approvals section's full approver-assignment endpoint;
  the two must never be conflated into one flow.
- **SPEC-9.7.4**: The staff approvals section's endpoint must be
  parameterized by entity kind (ticket, quote, change request, or time
  entry) so the same component serves all four, rather than being
  ticket-specific.
- **SPEC-9.7.5**: The ticket attach-file flow must check the file's size
  against the 5 MiB limit before uploading, and the hidden file input
  must clear its own previous value before each open, so re-selecting the
  same file still fires a fresh upload.
- **SPEC-9.7.6**: Toggling a task-list checkbox rendered inside a ticket's
  description must save only the description field, then refresh both
  the ticket record and its history.
- **SPEC-9.7.7**: `ApprovalsPage` must fetch the pending queue and post a
  decision on the dual-plane bearer path, never the staff-only path, so a
  contact holding the decide capability can act on the contact plane.
- **SPEC-9.7.8**: Each SLA tab's primary list (policies, business hours,
  holiday calendars) and its related sub-resources (business-hours
  options, targets, ticket priorities) must each be fetched from their
  own dedicated endpoint; a target save must use a single upsert endpoint
  that the server keys on the (policy, priority) pair, so the same
  request both creates and updates a target.
- **SPEC-9.7.9**: `RequestFormPage`'s load and submit must each call the
  one public, token-keyed request-form endpoint; no other endpoint may
  be used to read or submit a public request form.
- **SPEC-9.7.10**: `request_links.rs` must load the contacts for a
  company's recipient picker through the paged, uncapped listing path,
  not a path that clamps to the server's 100-row page limit.
- **SPEC-9.7.11**: Sending a request-form link (from either the
  company-card entry point or the forms-builder entry point) must go
  through the single shared send-link endpoint, so the two entry points
  cannot drift into different request shapes or validation.
- **SPEC-9.7.12**: `WorkDayStrip` must fetch the day's clock state from
  the workday endpoint (optionally parameterized by date and viewed
  user), the segment-edit policy from the settings endpoint, and must
  issue clock-in, clock-out, break-start, break-end, segment-update, and
  segment-delete each against its own dedicated endpoint.
- **SPEC-9.7.13**: `WorkDayStrip`'s "Viewing" picker must source its user
  list from the shared user-roster cache, not from a fetch owned by this
  component.

### 9.8 Build and deployment

- **SPEC-9.8.1**: Every network-calling code path in this section's files
  must compile to a no-op on a build that does not enable the `app`
  feature it depends on, consistent with the pattern established in
  SPEC-6.3.6, except where noted as a defect in the PR body.
- **SPEC-9.8.2**: `work_day.rs`'s test module covering its async resource
  dependency tracking must be additionally gated to exclude the
  `wasm32` target, because it depends on a dev-only async runtime
  unavailable there.

### 9.9 Retention and cleanup

- **SPEC-9.9.1**: The ticket journal must state that it excludes
  attachments and approvals, since no audit row exists for either, in a
  permanent footer note.
- **SPEC-9.9.2**: Closing the ticket attach-file dialog without
  completing an upload must clear the hidden file input's previous
  selection before it can be reopened.
- **SPEC-9.9.3**: Closing an SLA policy, business-hours, or
  holiday-calendar form without saving must discard all in-progress
  edits with no draft retained.
- **SPEC-9.9.4**: Removing an SLA target is an immediate, non-reversible
  delete; the policy reverts to having no commitment for that priority
  until a new target is set.
- **SPEC-9.9.5**: `RequestFormPage`'s "add the next person" action must
  clear the current person's answers and field errors before fetching the
  next slot, so no prior person's answers can leak into the next
  person's submission.
- **SPEC-9.9.6**: `RequestFormPage` must discard a blank or
  whitespace-only text answer from the submission payload entirely,
  matching the server's own "blank is absent" rule.
- **SPEC-9.9.7**: `WorkDayStrip`'s open correction form and its pending
  "removing" confirmation must each be cleared immediately before the
  corresponding mutation is sent, so a stale open form cannot be
  resubmitted against an id the mutation has already made stale.

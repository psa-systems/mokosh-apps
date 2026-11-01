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
  (`knowledge_base.rs:5054-5150`).
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
  root, non-deleted comments as anchor owners, must silently skip a
  root comment with no anchor or an unparsable anchor, and must
  classify an anchor that fails to resolve against the current
  rendered text as an orphan (its comment id collected) distinct from
  one that resolves; resolved ranges must be sorted by start offset
  before overlaps are split, and the resulting fragments converted
  1:1 to `MarkSpec { start, end, ids }` in document order.
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
  before the request resolves (`knowledge_base.rs:2196-2209`).
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
  load (`kb_activity.rs:176,263-265`). Resolve/unresolve failure must
  render "Could not {verb} the thread: {message}"; comment edit
  failure must render "Could not save the comment: {e}"; comment
  delete failure must render inside the confirm dialog itself, not the
  page-level error slot; a new comment or reply post failure must
  render "Could not post: {message}"
  (`kb_activity.rs:445,626,631,685,792`).
- **SPEC-12.5.7**: If the inline-comment dialog's `open_at` ids no
  longer match any loaded comment, it must render "These comments are
  no longer in the list ({n})." instead of an empty list
  (`kb_inline.rs:214-216`). If `anchor_dom::text_of` resolves to
  `None` (the body element is not yet in the DOM), both the placement
  effect and the selection-capture handler must return without
  marking, logging, or showing an error
  (`kb_inline.rs:117-119,184-186`).

### 12.6 Concurrency and ordering

- **SPEC-12.6.1**: Every list/detail page's resources must read
  `active_tenant_generation()` inside the `use_resource` closure so an
  org switch or token swap re-fetches (`knowledge_base.rs:564,580,
  1038,1068,1371,1385,1601,1612,1635,1658,1732,2354,2519,3063`).
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

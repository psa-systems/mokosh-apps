# mokosh-apps specification

This document describes the application as implemented in this repository. It is derived from the source code; where
another document disagrees with the code, the code is authoritative.

## 1. Purpose and scope

mokosh-apps is the client application of the Mokosh Platform, a professional services automation (PSA) product for
managed service providers (MSPs). It is a single Dioxus (Rust) crate that compiles to:

- a WebAssembly single-page application served as static files by a Caddy container (feature `web`, the default), and
- a native desktop application using the Dioxus desktop renderer (feature `desktop`).

Users of the application:

- Staff of an MSP (agents, managers, administrators) working in a tenant.
- Platform administrators, who sign in on a separate credential plane.
- Portal contacts: people at an MSP's customer companies, who sign in on a third plane and see a capability-limited
  subset of the same workspace.
- Anonymous visitors following an emailed link (request forms, password setup, invitations, grant invitations).

What the application does NOT do:

- It has no database, no business logic of record, and no background job processing. All business data lives in the
  Mokosh API server, which the application calls over HTTP. The wire-format types are taken from a shared types crate
  (a git dependency pinned by `Cargo.lock`).
- It does not issue tokens. Staff sessions are obtained from an OpenID Connect provider (OP) or from the API server's
  standalone login endpoint; portal and platform sessions are obtained from the API server.
- Security decisions are made by the API server. Client-side role and capability checks only decide what to show.

## 2. Architecture

### 2.1 Build targets and crate layout

One package, `mokosh-apps`, with a library (`src/lib.rs`, crate `mokosh_apps`) and a binary (`src/main.rs`). Cargo
features:

| Feature | Effect |
| --- | --- |
| `web` (default) | Enables `app` and `dioxus/web`. Required for any `wasm32` build; `src/main.rs` fails compilation otherwise. |
| `desktop` | Enables `app` and `dioxus/desktop`. |
| `app` | The app-runtime gate: the API client, global signals and page logic. On for both renderers. |
| `multi-tenant` (default) | Compiles the `/admin/teams` and `/settings/members` routes. |
| `server` | Gates files that mirror server code; never compiled in this crate (no server dependencies are pulled in). |
| `dev_admin_bypass` | Debug builds only: when `ADMIN_EMAIL` and `ADMIN_PASSWORD` are set at compile time, starts pre-authenticated as that admin. Off by default. |

Source layout under `src/`:

- `main.rs`: process entry point and the `App` root component.
- `lib.rs`: the `Route` enum, `AuthGuard`, `RouteErrorFallback`, the contact-forbidden path check, and one wrapper
  component per route.
- `pages/`: one module per screen or screen family (plus `pages/contact_portal/` for the portal login flow).
- `components/`: shared UI (layout shell, forms, tables, modals, Markdown editor, pickers, banners).
- `hooks/`: fetch layer (`fetch.rs`), authentication (`auth.rs`, `contact_auth.rs`), capability gating, shared caches of
  reference lists, theme, update check, server-status monitoring.
- `modules/`: OIDC client (`oidc/`), runtime configuration, feature flags, tenant settings reader, models per domain.
- `platform/`: the host boundary, implemented once for `wasm32` (browser) and once for native: HTTP, session and durable
  storage, timers, location, DOM, clipboard, downloads, config. Browser-only crates (`web-sys`, `js-sys`, `gloo-*`,
  `wasm-bindgen`) are declared only for `wasm32`, so a browser call outside `platform/` does not resolve on the
  desktop build.
- `utils/`: pagination, validation rules, Markdown rendering (pulldown-cmark, sanitized with ammonia), money, dates,
  roles labels, rate-limit parsing, diffs.
- `build.rs`: bakes `APP_GIT_HASH` (from `GIT_SHA` when set, otherwise `git rev-parse`; 12 characters), `APP_GIT_TAG`
  (the Cargo package version) and `APP_BUILD_DATE` into the binary.

### 2.2 Runtime and data flow

1. `main` installs the wasm panic hook (wasm) or the `ring` rustls crypto provider (native), snapshots the initial URL
   query string so the OIDC response survives router mounting, then launches `App`.
2. `App` mounts app-wide providers and background loops once (auth context, sidebar, page title, shared caches for the
   user roster, mention directory, work types, asset types, task statuses, KB categories, payment terms, tax rates,
   ticket saved views, dashboard report; token refresh, standalone token refresh, a 30 second `/auth/me` heartbeat,
   active organization loader, current user loader, a 60 second feature-map refresh, bfcache invalidation, desktop
   session-end watcher and close guard, theme and brand application, build update check, server reachability
   monitor), then renders the stylesheet, the `Router::<Route>` and the desktop close-confirm modal.
3. Pages call the fetch layer (`hooks::fetch::api`) which builds the URL from the resolved API base, attaches the
   bearer for the correct plane, classifies the response, and returns typed results or `ApiError`.
4. Pages re-fetch when `TENANT_GENERATION` changes (bumped when the access token changes, on tenant switch, or on
   demand).

### 2.3 Routing layers

The `Route` enum has three tiers:

1. Public routes (no guard).
2. Routes under the `AuthGuard` layout. Within it, chromeless routes (onboarding, TV dashboard, big-view kiosks) and
   routes under the `AppShell` layout (top bar, sidebar, banners, toasts).
3. A catch-all `NotFound`.

`AuthGuard` behavior, in order:

1. While a contact session rehydrate may still succeed (web only), render a placeholder and call
   `POST /contact/auth/refresh`; the guard releases whatever the outcome.
2. While tokens are loading, render a placeholder.
3. Not authenticated: allow through if a platform bearer is held; else if a contact session is held, redirect any
   contact-forbidden path to `/dashboard` and otherwise render the route; else if on a `/portal/*` URL or a stored
   portal hint exists, redirect to the portal login (`/portal/<portal id>/login`, then the legacy slug login, then
   `/portal/login`); else if the deployment is standalone, navigate to `/login`; else start the OIDC authorization flow.
4. Authenticated but the server reports `profile_completed = false` after the first `/auth/me` reconcile: navigate to
   `/onboarding/profile`.
5. Otherwise render the route inside an `ErrorBoundary` whose fallback offers "Go to dashboard" and "Reload page".

`pathname_is_contact_forbidden` denies portal contacts these path prefixes: `/companies`, `/contacts`, `/calendar`,
`/dispatch`, `/scheduling-templates`, `/rate-cards`, `/payments`, `/tax-rates`, `/payment-gateways`, `/reports`,
`/time`, `/timesheets`, `/dashboards`, `/dashboard/tv`, `/big`, `/admin`, `/pick-tenant`, `/create-org`, `/invite`,
`/dev`; the KB create and edit paths; and every `/settings/*` path except `/settings`, `/settings/portal-branding`
and `/settings/appearance`.

### 2.4 Technology stack

Rust 2021, Dioxus 0.8 alpha (router feature), serde, chrono and chrono-tz, rust_decimal, uuid, validator,
pulldown-cmark and ammonia (Markdown), sha2/base64/rand (PKCE). Browser: gloo-net, web-sys. Native: reqwest (rustls,
`ring` provider), tokio, dirs, open, iana-time-zone. Styling: Tailwind CSS built from `input.css` with bun into
`assets/styles.css`. Build tool: the Dioxus CLI (`dx`). Container runtime: Caddy on Alpine.

## 3. Functional requirements

Each statement below is implemented by the pages under `src/pages/` and the routes in section 4.

### 3.1 Authentication and session

- FR-A1: The app supports three sign-in planes: staff (OIDC authorization code with PKCE, or standalone email and
  password), platform administrator, and portal contact.
- FR-A2: With no OIDC issuer or no OIDC client id configured, the deployment is standalone and the app uses the API
  server's `POST /auth/login`; otherwise it redirects to the issuer's `/oauth2/authorize`.
- FR-A3: `/login` first attempts the platform-administrator login (`POST /platform/login`) when no MFA or approval code
  is supplied, falls back to the tenant login (`POST /auth/login`) on failure, and handles MFA, sign-in approval code,
  membership selection (`POST /auth/select-tenant`) and zero-membership setup (`/create-org`).
- FR-A4: Access tokens are renewed before expiry (within 60 seconds) on the request path and by a 30 second background
  loop; concurrent renewals share one in-flight refresh. A 401 on the staff bearer triggers one renewal, and a failed
  renewal ends the session.
- FR-A5: A `410` response with error code `ACCOUNT_DELETED` raises a terminal overlay; the heartbeat (`/auth/me` every
  30 seconds while the tab is visible) detects it without user action.
- FR-A6: Sign-out revokes the API server session (`POST /auth/logout`, or `/auth/logout-all` for sign out everywhere),
  revokes the OP refresh token, clears local state, and navigates to the OP logout URL or the landing page.
- FR-A7: Portal contacts sign in by Portal ID plus email and password, or by an emailed single-use link
  (`/portal/find`, `/portal/pick`); the portal session is rehydrated on cold load from a server-set HttpOnly cookie.
- FR-A8: Password reset, forgot password, first-time password setup, invitation acceptance, self-signup, MFA setup and
  disable (staff and platform), and profile completion are provided as screens.
- FR-A9: A forced onboarding screen collects the user's name when the server reports the profile incomplete.

### 3.2 Workspace areas

For each area the screens list, create, view, edit and (where the route exists) act on records through the API.

- Dashboard: summary dashboard, saved dashboards (list, view, edit layout), a TV view scoped to a team, and
  chromeless "big view" kiosks for tickets, dispatch and calendar with `?refresh=<seconds>` (default 30), `?status=`
  and `?priority=` query parameters.
- Tickets: list with saved views, create, detail with notes, attachments (including inline), SLA, reopen, approvals
  request; lookups for statuses, priorities, types, queues and categories are editable in settings.
- Time: time entries, timesheets (submit, withdraw), approvals queue (approve, reject), work-day clock (clock in and
  out, break start and end, segment edit), and a cross-target "My approvals" page.
- Projects and tasks, with project types and task statuses configurable.
- Companies and contacts: CRUD, sites, per-company documentation view, contact import from Google, iCloud, Microsoft 365
  and vCard files with preview, selection, review queue, per-contact sync locks, portal access grant, revoke, resend
  and access requests, company-scoped and tenant-wide portal roles, company request-form links.
- Calendar and dispatch: appointments, dispatch board, scheduling templates.
- Sales and billing: quotes (send, convert, PDF), contracts and block hours, rate cards, invoices (create, from time
  entries, pay, void, write off, amend, PDF, delivery options), payments, credit notes (void, PDF), statements (PDF),
  tax rates, payment gateways, payment terms, products, bills and vendors, expenses and expense categories, invoice
  issuer integrations (connect, disconnect, mappings).
- Assets: list with column preferences, detail drawer, catalog and bulk order, asset types, RMM connections, device
  mappings and alert rules.
- Knowledge base: categories, articles (Markdown with preview, templates, attachments, inline image paste), versions
  and restore, comments, inline comments anchored to rendered text, helpful votes, per-company views, article table of
  contents, and a list of top ticket-driving articles.
- Reports: ticket, time, billing, project and client reports with export, custom report schema.
- Administration: audit log, feature toggles (super admin), request-form builder, SLA management (policies, business
  hours, holiday calendars, targets), invitations, members and teams, platform-account self-service.
- Settings: organization name, branding (tenant and portal), appearance (theme and accent), tenant modules, invoice
  numbering, note editing policy, segment editing policy, time tracking limits, scheduling, payment reminders, email
  (SMTP), Google OAuth client, import and export of tenant data, contact roles.

### 3.3 Cross-cutting behavior

- FR-X1: If the API server is unreachable, an app-wide banner is shown, mutating controls are disabled
  (`use_can_mutate`), and `/ready` is polled every 10 seconds until it answers. The banner trips after 2 consecutive
  failures (transport failure or 5xx); any response proves reachability.
- FR-X2: A feature toggle that is missing, not yet loaded, or failed to load reads as off.
- FR-X3: Tenant modules (billing, projects, calendar, contracts, assets, knowledge_base, rmm_integration, reports,
  time_tracking, timesheets) are read from `GET /settings/modules` and hide the corresponding navigation and pages.
- FR-X4: Organizations (tenant switcher and create-team action) are hidden unless the runtime config
  `organizations_enabled` is `1`, `true` or `yes` (case-insensitive).
- FR-X5: Forms validate input with shared rules (required, length, email, uuid, number) and show server-provided field
  errors next to the field.
- FR-X6: A 429 response is rendered with the wait derived from `retry_after_seconds` when present.
- FR-X7: Unsaved-changes guards prompt on tab close or reload (browser, `beforeunload`) and on window close (desktop,
  through the close-confirm modal).
- FR-X8: In the browser, the app polls `/_mokosh_config.json` every 5 minutes and on version-skew status codes; when
  `build_sha` differs from the running bundle it shows an update banner and reloads at the next tab-hidden event or
  after 30 minutes. The check is inert on desktop and in dev builds with an unknown hash.
- FR-X9: Rendered Markdown is sanitized with ammonia before display.
- FR-X10: Lists page with a maximum of 100 items per page.

## 4. Interfaces

### 4.1 UI routes

Public (no guard):

| Path | Screen |
| --- | --- |
| `/` | Landing page |
| `/login` | Unified sign-in (standalone or OIDC kickoff) |
| `/client/login`, `/platform/login` | Redirect stubs to `/login` |
| `/pick-tenant`, `/create-org` | Membership picker and zero-membership setup (state from the login flow) |
| `/auth/callback` | OIDC redirect target |
| `/forgot-password`, `/reset-password/:token`, `/set-password/:token` | Password flows |
| `/invite/:token` | Invitation acceptance |
| `/request-forms/:token` | Client request form (token is the credential) |
| `/signup`, `/signup/:token` | Self-signup |
| `/portal/login`, `/portal/:handle/login`, `/portal/:slug/set-password?:token`, `/portal/find?:email`, `/portal/pick?:token` | Portal contact login family. `:handle` is nine ASCII digits (Portal ID) or a legacy slug. |

Under `AuthGuard`, chromeless: `/onboarding/profile`, `/dashboard/tv`, `/big/tickets`, `/big/dispatch`,
`/big/calendar`.

Under `AuthGuard` and `AppShell`:

- `/dashboard`, `/dashboards`, `/dashboards/:id/view`, `/accept-grant?:token`
- `/tickets`, `/tickets/new`, `/tickets/:id`
- `/time`, `/time/new`, `/timesheets`, `/timesheets/approvals`, `/approvals`
- `/projects`, `/projects/new`, `/projects/:id`, `/projects/:id/tasks`
- `/companies`, `/companies/new`, `/companies/:id`, `/companies/:id/edit`, `/companies/:company_id/documentation`,
  `/companies/:company_id/roles/:id`
- `/contacts`, `/contacts/new`, `/contacts/import-review`, `/contacts/:id`, `/contacts/:id/edit`
- `/calendar`, `/dispatch`, `/scheduling-templates`
- `/quotes`, `/quotes/new`, `/quotes/:id`, `/quotes/:id/edit`
- `/contracts`, `/contracts/new`, `/contracts/:id`, `/contracts/:id/edit`
- `/rate-cards`, `/rate-cards/new`, `/rate-cards/:id`
- `/invoices`, `/invoices/new`, `/invoices/:id`, `/bills`, `/bills/:id`, `/expenses`, `/expenses/:id`, `/payments`,
  `/tax-rates`, `/payment-gateways`, `/credit-notes`, `/credit-notes/:id`, `/statements`
- `/assets`, `/assets/new`, `/assets/:id`
- `/kb`, `/kb/articles?:q&:tag&:category`, `/kb/articles/new?:parent_article_id&:visibility`, `/kb/articles/:id`,
  `/kb/articles/:id/edit`, `/kb/companies/:id`
- `/reports`, `/reports/:report_type`
- `/settings` and its sub-routes: `/settings/active-tenant`, `/settings/branding`, `/settings/portal-branding`,
  `/settings/group/{service-types,billing,tickets,integrations,data}`, `/settings/{work-types,task-statuses,
  asset-types,company-industries,project-types,sla,scheduling,payment-reminders,appearance,tv-view,time-tracking,
  rate-cards,tax-rates,gateways,payment-terms,products,vendors,expense-categories,ticket-statuses,ticket-priorities,
  ticket-types,ticket-queues,ticket-categories,email,integrations,import-export,contact-roles,organization,modules,
  invoice-numbering,note-editing,segment-editing,payment-methods}`, `/settings/rmm/{connections,device-mappings,
  alert-rules}`, `/settings/integrations/{google-client,google-contacts,google-contacts/import,icloud-contacts,
  icloud-contacts/import,microsoft-contacts,microsoft-contacts/import,xero,quickbooks,xero/mappings,
  quickbooks/mappings,vcard-import}`, `/settings/contact-roles/:id`, `/settings/members?:tab` (`multi-tenant`)
- `/profile`, `/system-status`, `/dev/buttons`
- `/admin/audit`, `/admin/features`, `/admin/forms`, `/admin/sla`, `/admin/invitations`, `/admin/team` (redirect),
  `/admin/teams` (redirect stub, `multi-tenant`), `/admin/platform-account`

Catch-all: `/:..route` renders the not-found page.

### 4.2 API server calls

All calls are relative to the resolved API base (section 6.2), under `/api/v1`. The client never exposes a server; it
only consumes these. Authentication column: staff = staff bearer, contact = contact bearer plus the HttpOnly session
cookie, platform = platform bearer, none = unauthenticated.

| Area | Representative paths | Auth |
| --- | --- | --- |
| Staff session | `POST /auth/login`, `POST /auth/select-tenant`, `POST /auth/refresh`, `POST /auth/logout`, `POST /auth/logout-all`, `GET/PUT /auth/me`, `POST /auth/me/mfa/{setup,enable,disable}`, `POST /auth/me/complete-onboarding`, `GET /auth/memberships`, `POST /auth/switch-tenant/:id`, `GET /auth/users`, `GET /auth/directory` | none for login, refresh and password flows; staff otherwise |
| Password flows | `POST /auth/forgot-password`, `POST /auth/reset-password`, `GET /auth/set-password/context/:token` | none |
| Platform | `POST /platform/login`, `/platform/me/password`, `/platform/me/mfa/{setup,enable,disable}` | none, then platform |
| Contact | `POST /contact/auth/{login,refresh,logout,set-password,login-link,login-link/redeem,login-link/select,login-link/verify-mfa}`, `GET/PUT /contact/auth/me`, `GET /contact/dashboard/summary`, `/contact/payment-methods`, `/contact/companies/self/branding`, `/contact/portal/:portal/{host,resolve-to-portal-id}` | none for login; contact otherwise |
| Public | `GET /public/config`, `GET/POST /public/request-forms/:token`, `GET /version`, `GET /ready`, `/public/tenants/:id/logo`, `/public/tickets/attachments/:id` | none |
| Tickets | `/tickets`, `/tickets/:id`, `/tickets/:id/{notes,attachments,reopen,sla,approvals/request}`, `/tickets/{statuses,priorities,types,queues,categories,saved-views}` | staff or contact |
| Time | `/time-entries`, `/time-entries/summary`, `/timesheets`, `/timesheets/approvals`, `/timesheets/:user/:week/{submit,withdraw,approve,reject}`, `/workday/{clock-in,clock-out,break/start,break/end}`, `/workday/segments/:id`, `/time-rounding-rules`, `/work-types` | staff |
| Projects | `/projects`, `/projects/:id`, `/projects/:id/tasks`, `/projects/summary`, `/project-types`, `/task-statuses` | staff |
| CRM | `/contacts/companies`, `/contacts/contacts`, `/contacts/sites`, `/contacts/portal-roles`, `/contacts/company-industries`, `/contacts/field-values`, `/companies/:id/documentation`, `/portal-roles`, `/form-request-links`, `/forms`, `/forms/drafts` | staff |
| Calendar | `/calendar`, `/dispatch`, `/appointments/:id`, `/scheduling-templates` | staff |
| Billing | `/quotes`, `/contracts`, `/contract-items/:id`, `/rate-cards`, `/rate-card-items/:id`, `/invoices`, `/invoices/:id/{pay,void,write-off,amend,pdf,payments,delivery-options,payment-readiness}`, `/payments`, `/credit-notes`, `/statements`, `/statements/pdf`, `/tax-rates`, `/payment-gateways`, `/payment-terms`, `/products`, `/vendors`, `/bills`, `/expenses`, `/expense-categories`, `/invoice-issuer-mappings/*`, `/integrations/invoice-issuer/:provider/*` | staff; some invoice reads also contact |
| Assets and RMM | `/assets`, `/asset-types`, `/configuration-items/:id`, `/rmm/{connections,device-mappings,alert-rules}`, `/status/companies/:id/backup` | staff |
| Knowledge base | `/kb/articles`, `/kb/articles/:id/{render,versions,comments,attachments,vote,helpful,not_helpful}`, `/kb/categories`, `/kb/top-ticket-driving-articles` | staff or contact |
| Reports and audit | `/reports/{tickets,time,billing,projects,clients}`, `/reports/:key/export`, `/reports/custom`, `/reports/dashboard`, `/audit-log` | staff |
| Contact sync | `/integrations/contact-sync/*` (runs, selection, preview, schedule, review-queue, vcard uploads, per-provider authorize, connect and disconnect), `/public/contact-sync/google/callback` | staff; callback none |
| Settings | `/settings/:category`, `/settings/modules`, `/settings/feature-toggles`, `/settings/email`, `/tenants/current`, `/tenants/current/{logo,organization,branding/:asset}`, `/data/{export,import,seed-demo}`, `/sla/*`, `/teams`, `/invitations`, `/grants/*`, `/my-grants/*`, `/notifications` | staff |

Request conventions: JSON bodies with `Content-Type: application/json`; multipart for file uploads; `Authorization:
Bearer <token>` on authenticated calls; `X-Forwarded-Host` carrying the browser host on the unauthenticated typed
helpers (omitted on desktop). Error envelope: `{"error":{"code","message","errors":[{field,message,code}]}}`; a 429 body
is `{"error":"rate_limited","message","retry_after_seconds"}`.

### 4.3 OIDC provider calls

When an issuer is configured: `GET <issuer>/oauth2/authorize`, `POST <issuer>/oauth2/token` (authorization code and
refresh grants), `POST <issuer>/oauth2/revoke`, and `GET <issuer>/v1/auth/logout?url=<target>` for sign-out. Requested
scope default: `openid email offline_access`.

### 4.4 Static endpoints served by the container

`/_mokosh_config.js` (sets `window.__MOKOSH_CONFIG__`), `/_mokosh_config.json` (same fields as data), `/index.html`
(patched), and every other path falls back to the patched `index.html`.

## 5. Data model

The application stores no relational data. Its client-side state:

| Store | Key | Content |
| --- | --- | --- |
| In memory (thread-local) | staff, portal and contact access tokens; contact refresh token; contact company id and contact id; contact capabilities | Never persisted |
| sessionStorage (browser) / in-process map (desktop) | `mokosh_auth_bundle_v1` | OIDC access, id and refresh tokens, expiry, scope |
| same | `mokosh_standalone_session_v1` | Standalone access and refresh tokens, expiry, current user |
| same | `mokosh_oidc_flow_v1` | Pending OIDC flow: code verifier, state, nonce, return target |
| same | `mokosh_oidc_callback_retry_v1` | Count of silent login restarts from `/auth/callback` |
| sessionStorage (browser only; the desktop build holds no platform bearer) | `mokosh:platform_token`, `mokosh:platform_mfa_enabled` | Platform bearer and MFA flag |
| localStorage (browser) / `prefs.json` in the per-user config directory (desktop) | `mokosh_theme`, `mokosh_accent`, `mokosh_tv_view`, `mokosh_tv_team`, `mokosh_device_id`, `mokosh:contact_last_slug`, `mokosh:contact_last_portal_id`, `mokosh.portal.next`, `mokosh:assets:columns:v1`, `mokosh:assets:signature_dismissed:v1`, `mokosh:contact-new-prefill`, `mokosh-update-dismissed-*`, `kb_density`, `kb_left_rail`, `kb_right_rail` | UI preferences and portal routing hints; never tokens |

The wire-format records (tickets, invoices, contacts, and so on) are defined by the shared types crate and the
per-domain models in `src/modules/*/models.rs`. The database schema and migrations belong to the API server and are not
in this repository.

## 6. Configuration

### 6.1 Resolution order

For each runtime field: (1) browser `window.__MOKOSH_CONFIG__.<field>` or, on desktop, environment variable
`MOKOSH_<FIELD>` then the `config.json` in the per-user config directory (both keyed by the lowercase field name);
(2) host-prefix derivation for hosts beginning with `msp.`; (3) compile-time `option_env!` defaults. Absent and empty
values are treated alike. On desktop `config.json` is read once per run; an invalid file is logged at error level.

### 6.2 Runtime fields (container environment variable -> config field)

| Variable | Field | Purpose |
| --- | --- | --- |
| `MOKOSH_API_BASE` | `api_base` | API base URL including `/api/v1`; trailing slashes are stripped. Also used to build the CSP `connect-src` and `img-src` origin. |
| `MOKOSH_OIDC_ISSUER` | `oidc_issuer` | OP base URL. Empty (and not derivable) selects standalone login. |
| `MOKOSH_OIDC_CLIENT_ID` | `oidc_client_id` | Public client id. Unset selects standalone login. |
| `MOKOSH_OIDC_REDIRECT_URI` | `oidc_redirect_uri` | Overrides the derived `<origin>/auth/callback`. |
| `MOKOSH_OIDC_SCOPES` | `oidc_scopes` | Scope string; default `openid email offline_access`. |
| `MOKOSH_HUB_BASE_URL` | `hub_base_url` | Origin of the identity hub used for legacy login bookmarks and sign-out fallback. Compile-time default `http://localhost:4400`. |
| `MOKOSH_PORTAL_HOST` | `portal_host` | Bare host the portal is served from; also read at compile time. |
| `MOKOSH_DOCS_BASE_URL` | `docs_base_url` | Documentation site base URL; hides the Documentation entries when unresolved. The container still honors the deprecated `MOKOSH_DOCS_URL` with a warning. |
| `MOKOSH_ORGANIZATIONS_ENABLED` | `organizations_enabled` | Shows the organizations surface (`1`, `true`, `yes`). |
| `MOKOSH_KB_ATTACHMENT_MAX_BYTES` | `kb_attachment_max_bytes` | Client-side attachment size check; default 5 MiB. |
| `MOKOSH_BRAND_NAME`, `MOKOSH_BRAND_LOGO_URL`, `MOKOSH_BRAND_HERO_URL` | `brand_name`, `brand_logo_url`, `brand_hero_url` | Deployment branding; defaults are the built-in product name and artwork. |
| `GIT_SHA` | `build_sha` | First 12 characters, used by the update check. |

Derivations when a field is unset and the page host is `msp.<apex>`: API base `https://api.msp.<apex>/api/v1`, issuer
`https://api.<apex>`, hub `https://<apex>`, docs `https://docs.<apex>`. When the host equals the portal host, the API
base is `https://api.<portal host without its first label>/api/v1` (same-origin `/api/v1` for `localhost` hosts).
Otherwise the browser default is the same-origin `/api/v1`; the desktop default is the compile-time `MOKOSH_API_BASE`
or `http://localhost:8080/api/v1`.

### 6.3 Container-only variables (read by `oci-build/entrypoint.sh`)

| Variable | Default | Purpose |
| --- | --- | --- |
| `PORT` | `8080` | Caddy listen port. |
| `MOKOSH_RUNTIME_DIR` | `/tmp/mokosh-runtime` | Writable directory for the generated config files and patched `index.html`. The entrypoint exits non-zero if it cannot be prepared. |
| `MOKOSH_BRAND_DESCRIPTION` | `Mokosh Platform - Professional Services Automation for MSPs` | Link-preview description. |
| `MOKOSH_PUBLIC_URL` | unset | Public base URL used to make a relative brand logo absolute for link previews. |
| `PUID`, `PGID`, `APP_USER`, `DATA_DIRS` | `APP_USER=appuser`, `DATA_DIRS` empty | Consumed by the shared gosu entrypoint. |

### 6.4 Compile-time variables

`MOKOSH_OIDC_ISSUER`, `MOKOSH_OIDC_CLIENT_ID`, `MOKOSH_OIDC_REDIRECT_URI`, `MOKOSH_OIDC_SCOPES`,
`MOKOSH_HUB_BASE_URL`, `MOKOSH_DOCS_BASE_URL`, `MOKOSH_PORTAL_HOST`, `MOKOSH_API_BASE` (desktop default), `GIT_SHA`,
and, with `dev_admin_bypass` in debug builds, `ADMIN_EMAIL` and `ADMIN_PASSWORD`.

## 7. Security

- Staff OIDC uses the authorization code flow with PKCE (S256, 32 random bytes verifier) and random 256-bit `state`
  and `nonce`. There is no client secret. The ID token is decoded without signature verification; the code exchange
  runs over TLS directly with the configured issuer.
- Token storage: access tokens in memory; the staff token bundle and standalone session in sessionStorage (tab-scoped)
  on the browser and in an in-process map on desktop; tokens are never written to localStorage. Portal contact tokens
  live in memory, with the durable portal session carried by a server-set HttpOnly cookie.
- Planes are kept apart: the staff, portal, contact and platform bearers have separate holders, and only a request that
  carried the held staff bearer can trigger the staff renewal and sign-out path on a 401. Signing in on one plane clears the opposite side.
- Authorization: the server is authoritative. The client hides navigation and routes using `use_capability`
  (staff and platform bearers pass; a contact session is checked against its capability snapshot), the contact-forbidden
  path list, per-page role checks (`is_admin`, `is_super_admin`), and tenant module flags. A test asserts that each
  `/admin/*` route reads the caller's role.
- Input handling: shared validation rules, server field errors, URI-component encoding helpers for query strings, Markdown
  sanitized with ammonia (script stripped, restricted class and attribute allow-lists), `return_to` sanitized to
  same-origin non-plumbing paths.
- Transport hardening in the container (Caddy): `Content-Security-Policy` (`default-src 'self'`, `script-src 'self'
  'wasm-unsafe-eval'`, `style-src 'self' 'unsafe-inline'`, `img-src 'self' data:` plus the API origin, `connect-src
  'self'` plus the API and OIDC origins, `object-src 'none'`, `base-uri 'self'`, `frame-ancestors 'none'`,
  `form-action 'self'`), `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy:
  strict-origin-when-cross-origin`; the `Server` header is removed. Content-hashed assets are cacheable for a year;
  everything else is `no-cache, no-store, must-revalidate`.
- Rate limits are enforced by the server; the client only parses and displays the wait from a 429.
- Desktop: the OIDC redirect uses an RFC 8252 loopback listener bound to `127.0.0.1` on an ephemeral port, reading one
  request line capped at 8 KiB.
- Secrets: none are stored in the repository or image. `.env.example` documents dev-only compile-time admin bypass
  values, which are readable by anyone who downloads a bundle built with them.

## 8. Integrations and dependencies

- Mokosh API server: every data call (section 4.2). Origin and CORS are the server's concern; desktop origins are
  mirrored in `src/platform/desktop_origin.rs` (`dioxus://index.html` on macOS, Linux and iOS,
  `http://dioxus.index.html` on Windows, `https://dioxus.index.html` on Android).
- OpenID Connect provider: staff sign-in, token refresh, revocation, logout, and legacy-bookmark redirects to the
  identity hub.
- Payment gateway checkout (Stripe Checkout in setup mode) for portal payment methods, reached through the API server.
- Contact-sync providers (Google, iCloud, Microsoft 365, vCard) and invoice issuers (Xero, QuickBooks), reached through
  the API server; the OAuth callbacks land on `/settings/integrations/*` pages.
- The shared wire-format types crate, fetched over anonymous HTTPS git; `scripts/check-types-pin.sh` enforces that `Cargo.lock`
  tracks the server's head, and a scheduled workflow checks drift.
- Caddy (static serving), Tailwind CSS via bun, Dioxus CLI.

## 9. Deployment and operations

### 9.1 Build

- Web bundle: `bun x @tailwindcss/cli --input input.css --output assets/styles.css --minify`, then
  `dx build --release --features web`. `dx` substitutes its own feature list, so `--features web` is mandatory and
  `src/main.rs` rejects a wasm build without it. `Dioxus.toml` sets wasm-opt level `z`.
- Release profile: `opt-level = "z"`, fat LTO, `codegen-units = 1`, `panic = "abort"`, stripped. Profiles `wasm-release`
  (thin LTO) and `wasm-dev` exist for memory-constrained development.
- Desktop: `dx build|bundle --platform desktop --no-default-features --features desktop,multi-tenant`; bundle identity
  and icons are in `Dioxus.toml`.
- The version shown in the UI is the Cargo package version; the commit hash comes from `GIT_SHA` or git.

### 9.2 Image

`oci-build/Dockerfile` is multi-stage: a builder pinned to the build platform produces the WASM bundle; the runtime is
Alpine with Caddy, gosu and shadow. Root-level icons and the two raw scripts (`theme-init.js`,
`scrollbar-autohide.js`) are copied into the web root. The container starts as root, remaps `appuser` (uid and gid 1001
by default) through `gosu-entrypoint.sh`, then runs `entrypoint.sh`, which writes `_mokosh_config.js`,
`_mokosh_config.json` and a patched `index.html` (config script tag and link-preview metadata) into the runtime
directory, derives CSP origins, and execs `caddy run --config /etc/caddy/Caddyfile`. A `user:` pin skips the remap and
runs directly as the pinned uid. Exposed port: 8080. Labels carry source, description and revision. There is no
volume requirement beyond a writable runtime directory, and no Docker `HEALTHCHECK` is declared.

The Caddyfile derives CSP origins from the request host (`msp.<apex>` -> `https://api.msp.<apex>` for the API and
`https://api.<apex>` for the OP) when the explicit variables are unset.

### 9.3 Development

`compose.yml` runs `dx serve --features web` on port 4301 with source bind-mounted and cargo and node_modules volumes;
`Dioxus.toml` proxies `/api/` to the server container on port 8080 so the SPA and API share one origin in dev. `just dev`
binds to a private LAN address or loopback. `compose.dev-sso.yml` is an overlay that routes the dev server through a reverse proxy with OIDC variables for SSO testing.

### 9.4 CI and checks

Forgejo workflows: `check.yml` (push and pull request), `build-oci-image.yml` (branch and tag pushes, manual),
`create-release.yml`, `types-pin-drift.yml` (weekly), `renovate.yml` (every 4 hours), `claude-fix.yml` (manual). The
`just check` umbrella runs clippy on host and wasm targets with `-D warnings`, formatting, the desktop check, and the
repository guard scripts under `scripts/`.

## 10. Non-functional behavior

- Logging: `tracing` on native; on the browser, errors that are not shown are written to the console through
  `platform::log`. A route render error is logged under target `route_error_boundary`; auth-guard redirects under
  `auth_guard`.
- Error reporting to users: API failures are mapped to toasts or inline messages by `ApiError::user_message`
  (network, 401/403 plane-aware permission text, 404, 409, 422 field messages, 429 wait text, 5xx generic). Debug builds
  also show error detail on the fallback page.
- Panics: the wasm panic hook forwards panics to `console.error`; the release profile aborts on panic, so the route
  `ErrorBoundary` covers returned errors, not panics.
- Reachability and staleness are surfaced app-wide by the server-status banner and the update banner.
- Caching: shared reference lists are fetched once per session; `api_base` and the OIDC configuration are resolved once
  per process.
- Accessibility and theming: light, dark and system themes with a curated accent catalog checked against WCAG contrast
  math; brand colors from the effective branding are applied as CSS custom properties.
- Observability beyond logs and the in-app System Status page (build versions and `/ready` dependency health) is not
  provided by the client.

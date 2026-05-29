# Client / server integration - client perspective

How `mokosh-apps` and `mokosh-server` fit together. Read this when
wiring a UI surface to backend, or when wondering "is there an
endpoint I can call yet?"

A symmetric view of the same content lives at
[`mokosh-server/dev-docs/client-server-integration.md`](../../mokosh-server/dev-docs/client-server-integration.md).

## At a glance

- This client ships **18 functional surfaces** (a dashboard plus 14
  router sections plus 3 portal screens).
- The server has real, DB-backed handlers for **4** of them (auth,
  tickets, contacts, tenants).
- The other 14 hit `stub_routes()` and return HTTP 501.
- The client currently does **zero `/api/v1/*` requests** in normal
  operation. Empirically observed via Chrome network tracking on
  2026-05-06 across `/dashboard`, `/tickets`, and
  `/portal/tickets/new`. The SPA renders mock data without ever
  contacting the backend.

The implication: even where server endpoints exist (4 modules), the
client cannot consume them today. Adding an HTTP layer is the
single highest-leverage move in either repo.

## DTO sharing

Both repos define types under `src/modules/<module>/models.rs`. As of
2026-05-06 the four real-module trees are **byte-identical** between
this repo and `mokosh-server`:

```
src/modules/auth/{mod,models,routes,service,middleware,bootstrap}.rs
src/modules/contacts/{mod,models,routes,service}.rs
src/modules/tenants/{mod,models,routes,service}.rs
src/modules/tickets/{mod,models,routes,service,automation}.rs
```

Each `mod.rs` uses `#[cfg(feature = "server")]` so this WASM build
omits the handler / service / middleware / bootstrap files and keeps
only the model types. This is **manual sync via copy-paste**, not a
shared crate, not codegen.

The pattern is currently in lock-step. It will silently drift the
moment one side is edited without porting. Three options:

- **(a) Shared crate.** Extract a `mokosh-types` workspace crate;
  both repos depend on it. Best long-term answer.
- **(b) Drift CI.** Keep the copy-paste, formalize it: a CI check
  that diffs the four shared modules between repos and fails the
  build on non-zero. Cheapest today.
- **(c) Manual sync.** Lowest setup cost, highest drift risk.

Recommendation: **(b) today, (a) when a fifth shared module is
added.**

## Section-by-section gap

Read this as "is the server ready to feed this UI surface?" Sections
are listed in the router order
([`src/lib.rs`](../src/lib.rs)).

| # | Section | This client (status) | Server | DTOs in sync? | Wire-now action | Wait-for | Priority |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Auth (`/login`, `/forgot-password`, `/reset-password/:token`) | mocked, 1s `TimeoutFuture`, `// TODO: Call API` | real `/api/v1/auth/*` (14 endpoints) | yes | swap [`hooks/auth.rs`](../src/hooks/auth.rs) mocked login for `client.post("/auth/login")`; keep dev bypass behind a build flag | server rate limit + email send | P0 |
| 2 | Dashboard | mock data | no aggregate endpoint | n/a | leave decorative | future `/reports/dashboard` | P3 |
| 3 | Tickets list / new / detail / notes | mock data, dead row clicks, Add Note unwired | real `/api/v1/tickets/*` (11 endpoints) but DTOs return empty status / priority / company / contact / assignee names | yes | wire list + get + create + add-note; tolerate empty joined names in UI for now | server F3 (DTO joins) | P1 |
| 4 | Time tracking (`/time`, `/timesheets`) | rich UI, all stub | 501 | n/a | leave decorative | server `time_tracking` module | P1 |
| 5 | Projects (`/projects`, `/projects/:id/tasks`) | rich UI, all stub | 501 | n/a | decorative | server `projects` module | P1 |
| 6 | Contacts + Companies | mock list / detail | real `/api/v1/contacts/*` (16 endpoints), but `update_site` is a silent no-op | yes | wire list + detail | server F4 (`update_site` fix) | P1 |
| 7 | Calendar / Dispatch | rich UI, dispatch board doesn't render appointments (F14) | 501 | n/a | F14 client-side fix is independent | server `calendar` module | P2 |
| 8 | Contracts | rich UI, all stub | 501 | n/a | decorative | server `contracts` module | P2 |
| 9 | Billing (invoices + payments) | rich UI, dead Pay Now etc. | 501 | n/a | decorative | server `billing` module | P1 |
| 10 | Assets | rich UI, all stub | 501 | n/a | decorative | server `assets` module | P2 |
| 11 | Knowledge base | rich UI, CategoryCards styled clickable but unwired (F13) | 501 | n/a | F13 client-side fix is independent | server `knowledge_base` module | P2 |
| 12 | Reports | rich UI, bare `<select>` doesn't render (F11) | 501 | n/a | F11 client-side fix is independent | server `reports` module | P2 |
| 13 | Settings (6 sub-pages) | rich UI, all `Connect`/`Configure` buttons stub | 501 (`/settings`); module-config helpers exist on server `tenant_service` but unrouted | n/a | decorative | server F5 (expose module config) | P2 |
| 14 | Admin (`/admin/tenants`) | mock list | real `/api/v1/tenants/*` (7 endpoints, super_admin gated) | yes | wire list + detail; disable for non-super_admin | nothing | P2 |
| 15 | Portal (7 routes) | rich UI; **`/portal/tickets/new` has critical GET-leak P0 bug** (F8) | all `/api/v1/portal/*` 501 | n/a | fix client GET-leak first; everything else decorative | server F6 (portal contact-scoped session) | **P0** |
| 16 | RMM (lives in `/settings/integrations`) | rich UI, stub | 501 | n/a | decorative | server `rmm` module | P3 |
| 17 | Notifications (bell + `/settings/notifications`) | rich UI, stub | 501 | n/a | decorative; bell can stay hidden | server `notifications` module | P2 |

The "Wire-now action" column for sections 1, 3, 6, 14 is what the
next mokosh-apps implementation pass should target. Everything
else should stay decorative until the corresponding server module
exists.

## Cross-cutting integration concerns

1. **No HTTP layer in this client.** Empirically confirmed: zero
   `/api/*` calls during normal navigation. Adding a single
   `use_api()` hook with a base URL from env is prerequisite to
   removing every `// TODO: Call API` stub. Highest-leverage single
   move in either repo.
2. **DTO sharing via copy-paste.** See
   [DTO sharing](#dto-sharing) above.
3. **Auth bypass + mocked client = the server is never exercised
   end-to-end.** The client has a hardcoded login bypass at
   [`src/hooks/auth.rs:90-118`](../src/hooks/auth.rs#L90) that
   skips the network entirely. Removing the bypass without a real
   fetch path would break login. The two changes land together.
4. **Companies route alias is dead on the server.** The server's
   empty `.nest("/companies", Router::new())` advertises an alias
   that doesn't exist. `/api/v1/companies` returns 404. Companies
   are reachable only at `/api/v1/contacts/companies`.
5. **Schema is dramatically ahead of handlers.** 71 tables defined
   in the server's schema, 13 read or written by the four
   implemented modules, ~58 unreachable over HTTP. The shape of the
   future is clear, most of it is still cardboard.

## Suggested next implementation pass

Concrete, ordered checklist for the next session. Estimated as one
focused day.

1. **Add `src/api/client.rs`** wrapping
   [`gloo-net::http::Request`](https://docs.rs/gloo-net) with a
   base URL from `option_env!("MOKOSH_API_BASE_URL")` (falls back to
   `/api/v1` for same-origin dev). One file, ~80 LOC. Add token
   storage and the `Authorization: Bearer ...` header automatically.
2. **Add `src/api/<module>.rs`** for each of the four wireable
   modules (auth, tickets, contacts, tenants). Each is a thin
   wrapper that calls `client.get/post/put/delete` and decodes into
   the DTOs already in
   [`src/modules/<module>/models.rs`](../src/modules/).
3. **Replace `hooks/auth.rs:90-118` mocked login** with a real
   `api::auth::login(...)` call. Persist
   `LoginResponse.access_token` to localStorage. Keep the bypass
   behind a `dev-bypass` cargo feature, not env var (so production
   builds physically can't have it).
4. **Wire tickets list / detail / create / add-note** to the
   existing server endpoints. Tolerate empty joined names in the UI
   - the server has `String::new()` placeholders for status name,
   priority name, company name, etc. (server F3 fixes this later).
5. **Wire contacts list + detail.**
6. **Wire `/admin/tenants` list + detail** (super_admin only).
7. **Fix the `/portal/tickets/new` GET-leak (F8)** at the same time
   - it is independent but ships together to clear the P0 list.
8. **Leave every other section decorative.** Do not touch billing,
   time tracking, projects, calendar, KB, reports, settings, etc.
   They have no backend; wiring them now means writing a second
   round of code when the server module finally lands.

## Toggling between mock data and real backend

A pattern that fits the existing dev-bypass plumbing:

- **Cargo feature `mock-data`** (default on while the server is
  unfinished). Each `api::<module>::list()` etc. has a
  `#[cfg(feature = "mock-data")]` branch that returns hardcoded
  responses, and a `#[cfg(not(...))]` branch that calls
  `client.get(...)`. Production builds opt out of `mock-data`.
- Or **runtime flag** read at compile time via
  `option_env!("MOKOSH_USE_MOCK")`. Same shape as the existing
  `ADMIN_EMAIL` / `ADMIN_PASSWORD` bypass mechanism in
  [`src/hooks/auth.rs:90-118`](../src/hooks/auth.rs#L90).

Either way the seed data in
[`mokosh-server/migrations/002_seed_data.sql`](../../mokosh-server/migrations/002_seed_data.sql)
is the natural backing for the "real" path - shapes match because
the DTOs are byte-identical across repos.

## Smoke check

The integration table is true if all three of these hold:

- **Section 3 (Tickets):** the server's
  [`routes.rs`](../../mokosh-server/src/modules/tickets/routes.rs#L31)
  registers 11 endpoints; this client's
  [`pages/tickets.rs:188-192`](../src/pages/tickets.rs#L188) has a
  `TableRow { onclick: |_| {}, ... }` empty closure. Server DTOs
  come back empty per
  [`routes.rs:71`](../../mokosh-server/src/modules/tickets/routes.rs#L71).
- **Section 9 (Billing):**
  [`mokosh-server/src/modules/billing/mod.rs`](../../mokosh-server/src/modules/billing/mod.rs)
  is one line; the server's router maps `/invoices` and `/payments`
  to `stub_routes()`. This client ships
  `/invoices`, `/invoices/new`, `/invoices/:id`, `/payments` with
  mock data.
- **Section 15 (Portal):** the server's router shows all
  `/api/v1/portal/*` paths going through `stub_routes()`. This
  client at [`pages/portal.rs:269-329`](../src/pages/portal.rs#L269)
  has a `<form>` with no `onsubmit`, causing the P0 GET-leak.

If any of those don't hold, this doc is stale - please update.

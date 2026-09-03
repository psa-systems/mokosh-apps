# How mokosh-apps talks to mokosh-server

Two things: the path a request takes out of this client, and what the
two repositories share on the wire with a compiler checking it.

This file used to be a client/server status document instead: a
per-section table of which endpoints were real and which returned 501,
a priority ranking, and a recommended order in which to wire the UI to
the backend, all of it describing one day in 2026 and none of it
maintained. mokosh-server retired its own copy for the same reason
(PMS-848): duplicated status in prose is exactly what goes stale. The
status content is gone from here too, along with the counts and
checklists that were the parts that rotted. What is left is the part
that is a contract rather than a snapshot.

**"Is there an endpoint I can call yet?"** is answered by
mokosh-server's `CLAUDE.md`, under "Routing model", which names every
top-level nest under `/api/v1` and what authenticates a request to
each one. That file is maintained; a table here would not be.

## The request path

`crate::hooks::fetch::api` (`src/hooks/fetch.rs`) is the single entry
point. A page calls a helper on it and passes a path; no page under
`src/pages/` names a transport crate or holds a token of its own, so a
change to how this client reaches the server is a change to that module
and nowhere else. Two things do reach past the helpers, both
deliberately: a page that calls a `_with_auth` variant reads the bearer
with `api::current_access_token()` first, and the audit-log CSV export
builds an `<a href>` from `api::api_base()` because the download is a
real browser navigation rather than a fetch.

`crate::platform::http` (`src/platform/http.rs`) is the transport under
it, and the split is on `target_arch`, not on a cargo feature: the
browser build uses `gloo-net` verbatim, the desktop build maps the same
builder surface (`Request::get(url)`, `.header()`, `.json()`, `.send()`,
then `status()` / `ok()` / `text()` / `json()` / `binary()`) onto
`reqwest`. `multipart_file` is the one method `gloo-net` does not have,
added for the tenant-logo `PUT` (MAPPS-429).

### Where the requests go

`api::api_base()` resolves the base once per call, in this order:

1. Whatever the operator supplied, read through
   `crate::modules::runtime_config` and so through
   `crate::platform::config`: `window.__MOKOSH_CONFIG__.api_base` in the
   browser, which is how a self-hoster on a custom hostname overrides
   without rebuilding the image, and `MOKOSH_API_BASE` or `config.json`
   on the desktop (see [`desktop.md`](desktop.md)).
2. Host-prefix derivation for the canonical deploys: an SPA on
   `msp.<tld>` calls `https://api.msp.<tld>/api/v1`. Browser only, since
   a desktop window has no origin to derive from.
3. Otherwise the per-host default. In a browser that is the same-origin
   `/api/v1`, which the `dx` dev server and Caddy both proxy. On the
   desktop it is `option_env!("MOKOSH_API_BASE")` if the build baked one
   in and `http://localhost:8080/api/v1` if not, which is right for
   development and wrong everywhere else: a real desktop install is
   expected to have answered at step 1.

Two related helpers exist because the base is not the origin.
`normalize_api_base` strips a trailing slash, since every call site
joins with `format!("{}{}", api_base(), path)` and staging is configured
with one (PMS-751). `api_origin` drops the `/api/v1` suffix, for the
values the server hands back that are already paths from the origin,
`branding.logo_url` being the one that broke (PMS-758).

### The bearer, and renewing it

The access token lives in a thread-local `ACCESS_TOKEN` inside `api`. It
is held in memory only and never written to `localStorage`; where the
OIDC bundle it comes from is stored, and why, is
[`oidc-token-storage.md`](oidc-token-storage.md).

The client portal runs on a second identity (a `contacts` row, token
`typ: "portal_access"`) with its own holder, and the two lanes never
cross-populate (MAPPS-395). `src/hooks/fetch.rs` carries source-scanning
tests that hold that boundary, because the request helpers themselves
need a browser to run.

Renewal is on the request path, not only in the 30-second loop in
`crate::hooks::auth` (MAPPS-435): a tab the browser discarded and
re-created fires its first fetches before any loop has evaluated
anything. So an agent-lane helper calls `ensure_fresh_access_token`
before the send, which renews when the held token is inside
`REFRESH_WINDOW_SECS` of expiry, and a 401 that comes back anyway runs
`note_agent_unauthorized` for one recovery attempt. Renewal is
single-flight and shared with the auth loops, so a loop tick and a
request-time renewal spend one refresh token between them instead of
racing into a reuse detection, and a completed renewal answers for the
401s still in flight for `RENEWAL_DEBOUNCE_SECS` so a 401 no renewal can
fix cannot spend a refresh token per request forever. When renewal
cannot help, `end_agent_session` clears the session and puts the user on
the login screen: a hard redirect in the browser, and the
`SESSION_ENDED` signal on the desktop, which has no reload (MAPPS-504).

Only the agent lane reaches any of that. A `/portal/*` 401 belongs to
the other identity and a `POST /auth/login` 401 means the password was
wrong; neither may sign an agent out.

### How a failure reaches the user

mokosh-server answers a non-2xx with
`{"error":{"code","message","errors":[...]}}`, decoded here as
`crate::utils::error::ErrorResponse`. Both halves of the fetch layer
parse it:

- The `_typed` helpers return `ApiError` (`Network`, `Status { code,
  message, fields }`, `Decode`). Pages render `ApiError::user_message()`
  into a toast, and `field_message` routes a 422's per-field message
  next to the input that caused it instead of showing the envelope's
  generic one (MAPPS-210).
- The older `String`-returning helpers get the same envelope flattened
  by `status_error`, falling back to a user-facing message keyed on the
  status class rather than "Request failed with status: 422"
  (MAPPS-282).

Three conditions are wider than one call and are classified centrally
rather than at each call site. Any response at all proves the server is
answering and a 5xx or a transport failure does not, which drives the
`SERVER_REACHABLE` signal behind the outage banner (MAPPS-333). A `410`
carrying `error.code == "ACCOUNT_DELETED"` is terminal and one-way
(MAPPS-348). And every token change bumps a generation counter that
pages read inside their `use_resource` closure, so an org switch
re-fetches instead of leaving the previous tenant's rows on screen.

### Reading a whole collection

`get_all_authed` and its siblings are the only correct way to read a
list in full. mokosh-server clamps an over-large `per_page` instead of
rejecting it, so a page that asked for 200 got the cap and no sign the
rest existed; the helpers request `MAX_PER_PAGE` (re-exported from the
server's own constant) and keep going until a short page arrives,
failing loudly rather than returning a silently short list (MAPPS-528).

## DTO sharing

The shared-crate option is now live for part of the tree. MAPPS-383
added a git dependency on `mokosh-types` (the workspace crate inside
`mokosh-server`) and re-exports two module trees straight from it, so
the compiler enforces the wire contract instead of a human diffing two
copies:

```
src/modules/contacts/mod.rs      -> pub use mokosh_types::contacts::*;
src/modules/time_tracking/mod.rs -> pub use mokosh_types::time_tracking::*;
```

`src/modules/tenants/models.rs` has since joined them, MAPPS-536
finished `auth` and `tickets`, and MAPPS-535 finished `forms`, so six
module trees now re-export the crate wholesale. No module under
`src/modules/` still hand-copies a shared wire type.

MAPPS-536 is worth reading for what it did NOT buy, because the
headline is misleading. Both files it retired were dead: nothing
outside their own directories imported `Ticket`, `TicketFilter`,
`AuthState`, `User` or `UserResponse`, so they were never a contract
the compiler was checking. Deleting them removed 520 lines of copy and
three real drifts (`Ticket::procedure_kb_article_id`,
`UpdateTicketRequest::asset_id` as a clearable double option,
`TicketFilter::my_teams` / `asset_id`), and it means anything written
against those modules later inherits the live shape. It did not change
what the running SPA deserialises.

**Where the copies that matter actually live: `src/pages/`.** There are
far more `Deserialize` derives than there are page files that mention
`mokosh_types` at all (`grep -c 'derive(.*Deserialize' src/pages/*.rs`
against `grep -l mokosh_types src/pages/*.rs`). `src/pages/tickets.rs`
is the pattern: its own ticket struct, deserialised straight off the
server's `TicketResponse`, carrying `procedure_kb_article_id` and
`asset_name` and a dozen more, with a comment explaining which field it
deliberately dropped. Narrowing a payload to what a page renders is a
defensible client design, and it is also a hand copy that no compiler
compares against the producer. The audit at the end of this file is
where each of those pages now has a decision.

Where a module still carries handler / service files copied from the
server (`tenants`, `billing`), they sit behind a `server` cargo
feature so the WASM build keeps only the model types. Note that
the `server` feature cannot actually compile here: this crate has no
axum or sqlx dependency, so those files are dead weight and are
deleted as each module moves to `mokosh-types`.

Forms was the last of these and the most instructive, because it was
copied twice: once in `src/modules/forms/models.rs` and again in
`src/pages/request_form.rs`, for the public subset. PMS-898 moved the
fourteen wire types into `crates/mokosh-types/src/forms.rs` and
MAPPS-535 deleted both copies here. Two details of that move are worth
carrying forward to the next module.

The names the pages already used survived as aliases rather than
copies (`FormDefinition` for `FormDefinitionResponse`, and three more).
An alias cannot drift from what it points at, which is the whole point;
a wrapper struct would have reintroduced the thing being removed.
`FieldType` and `RequestLinkResponse` also became foreign types, so
their helper methods could no longer be inherent impls and became
extension traits.

The second detail is a trap. Adopting a typed enum where the client had
been holding a `String` is a REGRESSION unless the enum has a
catch-all: `request_form.rs` used to render an unknown field type as a
text input, and a bare enum would have failed to deserialise the whole
public form instead, blanking a page that a customer reaches from an
emailed link with no account. `FormRule::Unknown` and
`FieldType::Unknown` exist for that, with the server refusing both on
write, so the read stays tolerant and the write stays strict.

One deliberate exception to the direction of travel is worth naming, so
the next reader does not file it as an oversight.
`src/modules/audit/enrichment.rs` (PMS-870) declares the
`/api/v1/ip-enrichment` response by hand. It has a single consumer on
each side, and moving it would turn a client-only change into a
two-repo, two-merge sequence; it belongs in the crate if that endpoint
ever grows a second caller.

## Pinning the crate

The crate is fetched over anonymous HTTPS rather than `ssh://`, because
neither the pre-commit container nor the Forgejo check runner carries
SSH credentials.

The dependency names no `rev`, `tag` or `branch`, so the resolved
commit in `Cargo.lock` is the whole pin, and for three parity audits
running nothing advanced it: the compile-time gate the crate was
adopted for never fired, because the client was still compiling
against the shape the crate had three weeks and 214 server commits
earlier. `scripts/check-types-pin.sh` (a `just check` recipe and a
`check.yml` step) closes that. It runs `cargo update
--package mokosh-types`, fails if `Cargo.lock` moved, and prints the
pinned revision, the server head, and the `crates/mokosh-types`
commits in between. Bumping the pin is therefore a reviewed edit that
lands with whatever source changes the new DTOs require. The guard
needs network access to `dev.a8n.run`, the same access the git
dependency itself already needs.

Because the dependency names no revision, `cargo update` resolves it
to the head of mokosh-server's default branch, so the rule as first
written was red whenever that branch had moved at all, not only when
the crate changed. Over the window the stale pin covered, 214
mokosh-server commits produced 6 that touched `crates/mokosh-types`,
so about 97% of the red runs were a catch-up bump rather than a DTO
change, and a reviewed, green mokosh-apps PR went red because another
repository merged. MAPPS-532 paid that twice inside an hour, the
second time for a documentation-only server commit.

MAPPS-537 narrowed it. The guard reads the `crates/mokosh-types` diff
between the pinned revision and the head off the cargo git mirror it
already consults for the commit list, and fails only when that diff is
non-empty. A move with no crate change passes with a note naming both
revisions and the catch-up distance. A mirror that cannot answer is a
failure, not a pass: "I could not tell" and "nothing changed" must not
produce the same green check.

What that gives up is the pressure that stopped the pin drifting, so
`.forgejo/workflows/types-pin-drift.yml` keeps it. It runs
`check-types-pin.sh --strict` (`just check-types-pin-strict`, the
pre-MAPPS-537 rule where any move is a finding) at 07:00 UTC on
Mondays, plus on demand via `workflow_dispatch`. It gates no pull
request and is allowed to be red, so closing the distance is scheduled
work rather than a toll on whoever opened a PR that morning. Filing
the catch-up as an issue is deliberately not automated: CI holds no
YouTrack credential, and a red weekly run is the signal.

## Where this leaves the wire contract

Two things are true at once, and the second is the one worth acting
on.

The module trees listed above are enforced: change a shared DTO on
mokosh-server and this build stops compiling. That is not theoretical.
PMS-893 changed `compute_sla_status` in `mokosh-types`, and the change
arrived here as `check-types-pin` failing on a real crate change and
then a test in `src/modules/tickets/models.rs` failing on the bumped
pin, with nobody diffing anything.

The pages are not, by default. `src/pages/` deserialises server
payloads into structs declared per page, narrowed to what each page
renders. That is a defensible client design, and it is also a hand copy
no compiler compares against the producer. MAPPS-627 measured what that
costs and what to do about it, page by page; the rest of this section
is that answer.

MAPPS-627 was found while doing MAPPS-626, which bumped the pin for
PMS-942. That server change made `CreateTimeEntryRequest.company_id`
an `Option<Uuid>` instead of a `Uuid` and added `entry_kind` to both
the request and the response. `check-types-pin.sh` went red and named
the commit, exactly as designed. Bumping the pin and running
`cargo check --all-targets` was then clean with no source change at
all, because the page that owns
time tracking named none of those types: it declared its own private
decode struct and built its POST and PUT bodies as `serde_json::json!`
literals. The guard can say a DTO moved. Only the compiler can say
whether a page cares, and it was not being asked.

### The gate

The fix is one small pattern, not a migration. A page that sends a body
declares a `Serialize` struct for it and builds that instead of a
`json!` literal, and a `#[cfg(test)]` function destructures the shared
DTO exhaustively and feeds the bindings into the local struct:

```rust
#[allow(dead_code)]
fn create_request_fields_are_all_considered(req: CreateTimeEntryRequest) {
    let CreateTimeEntryRequest { user_id, date, /* ...every field... */ } = req;
    let _ = CreateTimeEntryBody { user_id, date, /* ...the ones we send... */ };
    let _ = (start_time, end_time, /* ...the ones we deliberately do not... */);
}
```

It never runs; compiling it is the whole check. A field added, removed,
renamed or retyped on mokosh-server fails this build, and the tuple of
unused bindings is a written record of what the page chooses not to
send. `src/pages/login.rs` has had this since MAPPS-397 and
`src/pages/time.rs` since MAPPS-627.

Using the shared type directly is better where it is available, and it
usually is not. `mokosh-types` derives `Deserialize` on request DTOs
and `Serialize` on response DTOs, which is the server's half of the
wire and the opposite of what a client needs, so neither can cross to
this side. `forms` is the exception: PMS-898 derived both on
`CreateFormDefinitionRequest` and its siblings, which is why
`src/pages/forms.rs` can build one and post it with no local struct at
all. Everything else needs the derive added on mokosh-server first.

### The audit

Every page that decodes or posts a payload whose producing DTO lives in
`mokosh-types`, and what was decided for it. Pages not listed touch no
shared-DTO endpoint (`approvals`, `team`, `portal*`, `reports`,
`credit_notes`, `products`, `dashboards`, `system_status`, and the
rest). Re-derive the set with:

```sh
grep -ohE '"/(time-entries|timesheets|work-types|tickets|companies|contacts|sites|auth/users|auth/me|tenants|forms|mileage)[a-z/-]*' src/pages/*.rs
```

Gated, with the destructuring function above:

| Page | Shared DTOs | Issue |
| --- | --- | --- |
| `login.rs` | `LoginRequest`, `LoginResponse` | MAPPS-397 |
| `request_form.rs`, `forms.rs` | `forms::*` used directly | MAPPS-535 |
| `time.rs` | `Create`/`Update`/`RejectTimesheet` requests, `TimeEntry`/`TimesheetSummary`/`WorkType`/`TimeRoundingRule` responses | MAPPS-627 |

To gate, one page per issue under MAPPS-685, because each owns a
feature's whole write surface and none of them reviews as part of
another:

| Page | What is ungated | Issue |
| --- | --- | --- |
| `tickets.rs` | `Create`/`UpdateTicketRequest`, `Create`/`UpdateNoteRequest`, the ticket and note decodes | MAPPS-686 |
| `contacts.rs` | `Create`/`Update` for company, contact and site | MAPPS-687 |
| `settings.rs` | `Upsert*` for the ticket taxonomy and work types, `UpdateTenantRequest`, `OrganizationProfileRequest` | MAPPS-688 |
| `profile.rs` | `UpdateMeRequest` is already a typed `Serialize` struct against `UpdateUserRequest`, with only prose holding it there | MAPPS-689 |

Deliberately not gated, and this is the decision that keeps the pattern
from becoming a tax. These pages decode a **picker subset** off a
shared-DTO endpoint: a primary key plus a display label, sometimes a
number or a company id, feeding a `<select>` or a name column. The key
and the label are the two fields that do not move, the page renders
nothing else, and a destructuring function per picker would be dozens
of assertions about `id` and `name` carrying no signal. If one of these
grows a write path or starts rendering a business field, it moves to
the table above.

`assets.rs`, `audit_log.rs`, `big_view.rs`, `billing.rs`, `calendar.rs`,
`contracts.rs`, `dashboard.rs`, `dashboards_view.rs`,
`knowledge_base.rs`, `projects.rs`, `quotes.rs`, `request_links.rs`,
`sla.rs`, `statements.rs`, and the ticket / project / task / user
pickers inside `time.rs` and `tickets.rs`. A few of these declare their
picker struct under `src/modules/` rather than in the page: `sla.rs`
reads `/tickets/priorities` into `TicketPriorityOption` in
`src/modules/sla/models.rs`, an id and a name. The decision is the same
wherever the struct sits.

Two sit just outside that rule and stay ungated for their own reason.
`admin.rs` narrows `TenantResponse` to six fields for a read-only
super-admin list and sends no body. `onboarding.rs` reads a single
completion flag.

## Not only DTOs

The crate carries two things that are not response shapes but are just
as much a wire contract, and both got there the same way: a hand copy
went stale and the drift was invisible.

`mokosh_types::sort` (PMS-897) is the set of `?sort=` keys each list
endpoint accepts, re-exported by `src/utils/sort_keys.rs`. It used to
mirror allow-lists that lived as locals inside the server's service
functions, and the mirror went stale within a day of being written:
PMS-894 added five ticket sort keys and nothing obliged the copy to
follow. MAPPS-533 also made the server answer 422 for a key it does not
accept, so a drift that survives the re-export now fails a request
instead of quietly reordering a page.

`mokosh_types::validation` (PMS-898) holds `validate_slug`, spelled out
rather than compiled as a regex so the crate stays dependency-light and
still builds for WASM. That constraint is worth remembering before
moving anything else in: whatever lands in the crate has to compile on
both targets.

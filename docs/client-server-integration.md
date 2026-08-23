# Shared DTOs between mokosh-apps and mokosh-server

What the two repositories share on the wire, and how much of it the
compiler is actually checking.

This file used to be a client/server status document: a per-section
table of which endpoints were real and which returned 501, a priority
ranking, and a recommended order in which to wire the UI to the
backend. All of it described 2026-05-06. It said the client made
"zero `/api/v1/*` requests in normal operation" while 25 of the 36
files in `src/pages/` call an authed fetch helper, and it sent readers
to `stub_routes()`, which does not exist in mokosh-server.

mokosh-server retired its own copy of that document for the same
reason (PMS-848): it was a status table end to end, and duplicated
status in prose is exactly what goes stale. The status content is
gone from here too. What is left is the part that is a contract
rather than a snapshot.

**"Is there an endpoint I can call yet?"** is answered by
mokosh-server's `CLAUDE.md`, under "Routing model", which names every
top-level nest under `/api/v1` and what authenticates a request to
each one. That file is maintained; a table here would not be.

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

`src/modules/tenants/models.rs` has since joined them, and MAPPS-536
finished `auth` and `tickets`, so five module trees now re-export the
crate wholesale. One module still declares wire types by hand:

```
src/modules/forms/models.rs    -> whole module hand-copied                    (MAPPS-535)
```

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
172 `Deserialize` derives across 29 page files, and three of those
files mention `mokosh_types` at all. `src/pages/tickets.rs` is the
pattern: its own ticket struct, deserialised straight off the server's
`TicketResponse`, carrying `procedure_kb_article_id` and
`asset_name` and a dozen more, with a comment explaining which field it
deliberately dropped. Narrowing a payload to what a page renders is a
defensible client design, and it is also a hand copy that no compiler
compares against the producer. Nothing tracks that yet.

Where a module still carries handler / service files copied from the
server (`tenants`, `billing`), they sit behind a `server` cargo
feature so the WASM build keeps only the model types. Note that
the `server` feature cannot actually compile here: this crate has no
axum or sqlx dependency, so those files are dead weight and are
deleted as each module moves to `mokosh-types`.

The forms copies are the awkward case, because the server keeps those
wire types in its own `src/modules/forms/models.rs` rather than in the
crate, so there is nothing to re-export yet, and
`src/pages/request_form.rs` carries a second copy of the public subset
on top. Moving them into `crates/mokosh-types/src/forms.rs` and
re-exporting both is MAPPS-535, which is blocked on the mokosh-server
half landing first.

Direction of travel: migrate `forms` the same way, and decide what to
do about the page-level structs. The
crate is fetched over anonymous HTTPS rather than `ssh://`, because
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

The pages are not. `src/pages/` deserialises server payloads into
structs declared per page, narrowed to what each page renders. That is
a defensible client design, and it is also a hand copy no compiler
compares against the producer. Nothing tracks it.

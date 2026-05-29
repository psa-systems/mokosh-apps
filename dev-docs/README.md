# Mokosh-apps developer docs

Internal reference for developers working on `mokosh-apps`. The
content here is derived from a 2026-05-06 UI/UX audit (every route in
the router was clicked through in Chrome via MCP browser automation,
combined with static analysis); treat it as a living snapshot and
update it alongside the code changes that invalidate any of its
claims. The same audit produced matching documentation in
[`mokosh-server/dev-docs/`](../../mokosh-server/dev-docs/).

## Contents

| Document | Purpose |
| --- | --- |
| [`codebase-state.md`](codebase-state.md) | What this client looks like to a user today: cross-cutting bugs, the 27 ranked UI/UX issues, the per-page status, and the proposed fixes (`F1..F19`). |
| [`client-server-integration.md`](client-server-integration.md) | How `mokosh-apps` and `mokosh-server` fit together. Section-by-section gap table, shared-DTO pattern, and the recommended order in which to wire UI to backend. |

## Recommended reading order

1. **[`codebase-state.md`](codebase-state.md)** first if you are
   touching client code: it tells you which UI patterns are dead and
   why, which tickets are mass-fix candidates, and which fixes are
   purely cosmetic.
2. **[`client-server-integration.md`](client-server-integration.md)**
   when wiring UI to backend: it shows which client surfaces the
   server already supports, which need to wait for a server module,
   and which DTOs are shared between repos.

## Conventions

- File paths are relative to the repo root (e.g.
  [`src/pages/tickets.rs`](../src/pages/tickets.rs)).
- "F1..F19" identifiers reference proposed fixes in
  [`codebase-state.md`](codebase-state.md#proposed-fixes). They are
  reused in `client-server-integration.md` for cross-doc continuity.
- Priority tags (P0..P3) follow the same scheme on both sides:
  - **P0:** break-the-app, data loss, or customer-facing dead-ends
  - **P1:** highly visible affordance bugs and missing wiring
  - **P2:** contrast / readability / cleanup
  - **P3:** layout polish, scrollbar nits, hover-state subtleties

## Keeping these docs honest

If you land a change that:

- closes a numbered issue or fix (P0..P3, F1..F19), **strike or
  remove the entry from**
  [`codebase-state.md`](codebase-state.md);
- adds a new dead interaction or visual bug, **add it as the next
  numbered entry** in the appropriate priority section;
- wires a UI surface to the server, **flip the row in**
  [`client-server-integration.md`](client-server-integration.md#section-by-section-gap)
  from `decorative` / `mocked` to `wired`.

These files are versioned with the source so the project history is
the change log.

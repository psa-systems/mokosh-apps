# Mokosh-apps developer docs

Internal reference for developers working on `mokosh-apps`. The
content here is derived from a 2026-05-06 UI/UX audit (every route in
the router was clicked through in Chrome via MCP browser automation,
combined with static analysis); treat it as a living snapshot and
update it alongside the code changes that invalidate any of its
claims. The same audit produced matching documentation in
`mokosh-server/docs/dev-docs/`, in that repository.

## Contents

| Document | Purpose |
| --- | --- |
| [`codebase-state.md`](codebase-state.md) | **Historical.** The record of a 2026-05-06 walk through every route: cross-cutting bugs, 27 ranked UI/UX issues, per-page findings and the proposed fixes (`F1..F19`). Not a description of the client now. |
| [`../client-server-integration.md`](../client-server-integration.md) | What the two repositories share on the wire: which modules re-export `mokosh-types`, how the pin is guarded, and where the copies the compiler cannot see still are. Its status content was retired in MAPPS-540. |
| [`versioning.md`](../versioning.md) | Where the displayed version comes from (build-time wiring in `build.rs` -> footer/banner) and how staging-vs-production update targets work. |
| [`spa-rollout-runbook.md`](../spa-rollout-runbook.md) | How to roll the SPA so the load balancer never serves two builds at once: pin a versioned image tag per deploy, roll replicas in lockstep, and verify a single `build_sha` from `_mokosh_config.js`. |
| [`qa-test-plan.md`](qa-test-plan.md) | End-to-end functional QA coverage: what to exercise across every route and flow. |
| [`qa-input-validation-prompt.md`](qa-input-validation-prompt.md) | The full per-field input-validation matrix: which values each field must accept and reject. |
| [`qa-field-and-button-audit-prompt.md`](qa-field-and-button-audit-prompt.md) | The technique layer: HOW to probe every field and button (HTML-constraint enumeration, `validity` probing, post-create API persistence checks, button audit) so defects surface instead of hiding. |

## Recommended reading order

1. **[`../client-server-integration.md`](../client-server-integration.md)**
   when a change crosses the wire: which DTOs come from the shared
   `mokosh-types` crate, what the pin guard enforces, and which copies
   nothing checks. For "is there an endpoint yet?", read
   mokosh-server's `CLAUDE.md` under "Routing model" - it is
   maintained, and no table here is.
2. **[`codebase-state.md`](codebase-state.md)** for the `F1..F19` ids
   that source comments and issues cite, and for what the 2026-05-06
   walk found. Historical: it describes a client that made no API
   calls at all, which stopped being true long ago.

## Conventions

- File paths are relative to the repo root (e.g.
  [`src/pages/tickets.rs`](../../src/pages/tickets.rs)).
- "F1..F19" identifiers reference the 2026-05-06 proposed fixes in
  [`codebase-state.md`](codebase-state.md#proposed-fixes). Several
  shipped; the ids survive because source comments and YouTrack issues
  cite them.
- Priority tags (P0..P3) follow the same scheme on both sides:
  - **P0:** break-the-app, data loss, or customer-facing dead-ends
  - **P1:** highly visible affordance bugs and missing wiring
  - **P2:** contrast / readability / cleanup
  - **P3:** layout polish, scrollbar nits, hover-state subtleties

## Keeping these docs honest

If you land a change that:

- changes what the two repositories share on the wire (a module moved
  onto `mokosh-types`, a new hand copy, a change to the pin guard),
  **update**
  [`../client-server-integration.md`](../client-server-integration.md);
- closes a fix the `F1..F19` ids name, **say so in the YouTrack issue**
  rather than editing [`codebase-state.md`](codebase-state.md). That
  file is the record of one day and is not maintained; editing it to
  track today's state is what made it misleading in the first place.

These files are versioned with the source so the project history is
the change log.

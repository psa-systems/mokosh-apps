# Destructive-action confirmation (PMS-369)

Companion to `form-conventions.md`. Every destructive action (Delete, Remove,
Revoke, Disable, "End Contract", ...) must confirm before it mutates state. A
single misclick must never destroy data.

## The shared component

Route every destructive action through `crate::components::ConfirmDialog`
(`src/components/modal.rs`). It renders a modal with:

- a title and a message that **names the thing** being destroyed, and says
  what would stop the action ("Delete this company? Its sites are removed and
  its contacts are unlinked ... A company that still has tickets, contracts,
  invoices, ... cannot be deleted."),
- a red destructive button (`destructive: true` -> `ButtonVariant::Danger`)
  with an explicit verb label (`confirm_text: "Delete"` / `"Remove"` / ...),
- a Cancel button,
- and cancel-on-backdrop-click and **cancel-on-Esc** (handled by the underlying
  `Modal`, which focuses itself on mount and fires `onclose` on `Escape`).

Wiring pattern: the Delete/Remove button's only job is to open the dialog
(set a `confirming_delete`/`pending_delete` signal). The actual DELETE fires
from the dialog's `onconfirm`. Never issue the mutation straight from the
button `onclick`.

## Simple confirm vs type-to-confirm

| Action shape | Pattern |
| --- | --- |
| Non-cascading delete (one row, no downstream effect): ticket, contact, project, asset, contract, line item, time entry, invitation revoke, ... | plain `ConfirmDialog` (one click on the red button) |
| **Cascading / catastrophic** delete that takes downstream entities with it: **delete company** (removes its sites, unlinks its contacts); deleting a tenant if/when the SPA gains that action | `ConfirmDialog` with `confirm_phrase` set - the destructive button stays disabled until the user **types the entity's name** |

`confirm_phrase` (default empty = no gate): set it to the entity's display name
(e.g. the company name). The gate logic is `confirm_phrase_satisfied(typed,
required)` - exact match after trim, case-insensitive - unit-tested in
`modal.rs`. An empty or non-matching input keeps the button disabled, so a
different entity's name never carries over an enabled state.

## Reporting the outcome (MAPPS-574)

A confirmed delete has three outcomes, and all three must reach the user.

- **Success**: a toast, then navigate. Silence plus a route change reads as an
  accident.
- **Refused**: keep the dialog **open** and pass the server's own message to
  `ConfirmDialog`'s `error` prop. It renders above the phrase input with
  `role="alert"`, so the reason arrives next to the control that produced it and
  is announced rather than only drawn. A refusal is routine, not exceptional:
  the server declines a company delete for any ticket on it, and for an FK
  RESTRICT from contracts, invoices, payments, projects, assets, time entries,
  appointments or sub-companies (PMS-170). Both carry an actionable message
  already; the client's job is to show it, not to invent one.
- **Never discarded**. `delete_authed` returns `Result<(), String>` and that
  `String` *is* the server's message; `delete_authed_typed` returns an
  `ApiError` whose `user_message()` is. Reducing either to `.is_ok()` throws the
  reason away, and the user sees a dialog close with nothing changed, which is
  indistinguishable from a dead button. That is what MAPPS-574 was: the reason
  for the reported failure existed, correctly, and was only visible in devtools.

`scripts/check-delete-result.sh` (run by `just check` and by the Forgejo check
job) fails on `delete_authed(...).await.is_ok()` and on `let _ =
delete_authed(...)`. Aggregating shapes stay legal: a bulk delete that collects
results and reports how many failed has not discarded anything.

## Current coverage

`ConfirmDialog` is wired across tickets, contacts, projects, assets, contracts
(incl. line items), billing, SLA (the "Remove" the review flagged), knowledge
base, team (Revoke), settings (incl. the RMM device-mapping and alert-rule rows
and the organization logo Remove), calendar, time, dashboards, request forms
(draft Discard), and quotes (Cancel quote). The one cascading delete - delete
company - uses the type-to-confirm gate. There is no tenant-delete in the SPA
today (the admin tenant page is a read-only roster).

## Testing

DOM-interaction tests (click button -> modal opens -> DELETE) are not supported
by the host-side `cargo test --lib` harness (no wasm/browser test runner is set
up). The testable risk - the type-to-confirm gate - is covered by unit tests on
`confirm_phrase_satisfied`. If a wasm-bindgen-test + headless-browser harness is
added later, assert the open-before-DELETE flow there.

The wiring rule and the reporting rule are enforced statically instead:
`scripts/check-confirm-destructive.sh` (MAPPS-436, run by `just check` and by
the Forgejo check job) fails on any `onclick:` handler that reaches
`delete_authed` / `delete_authed_typed` / `delete_lookup`, directly or through
a same-file helper. A delete that fires from `onconfirm` is invisible to it by
construction. That is what stops MAPPS-189's fix regressing again: three row
Deletes added after it shipped with no confirmation at all.
`scripts/check-delete-result.sh` (MAPPS-574) covers the other half, that the
confirmed delete then reports what the server answered.

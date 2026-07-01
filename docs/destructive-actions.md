# Destructive-action confirmation (PMS-369)

Companion to `form-conventions.md`. Every destructive action (Delete, Remove,
Revoke, Disable, "End Contract", ...) must confirm before it mutates state. A
single misclick must never destroy data.

## The shared component

Route every destructive action through `crate::components::ConfirmDialog`
(`src/components/modal.rs`). It renders a modal with:

- a title and a message that **names the thing** being destroyed
  ("Delete this company? This will also remove its sites and unlink its
  contacts/tickets."),
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
| **Cascading / catastrophic** delete that takes downstream entities with it: **delete company** (removes its sites, unlinks its contacts/tickets); deleting a tenant if/when the SPA gains that action | `ConfirmDialog` with `confirm_phrase` set - the destructive button stays disabled until the user **types the entity's name** |

`confirm_phrase` (default empty = no gate): set it to the entity's display name
(e.g. the company name). The gate logic is `confirm_phrase_satisfied(typed,
required)` - exact match after trim, case-insensitive - unit-tested in
`modal.rs`. An empty or non-matching input keeps the button disabled, so a
different entity's name never carries over an enabled state.

## Current coverage

`ConfirmDialog` is wired across tickets, contacts, projects, assets, contracts
(incl. line items), billing, SLA (the "Remove" the review flagged), knowledge
base, team (Revoke), settings, calendar, and time. The one cascading delete -
delete company - uses the type-to-confirm gate. There is no tenant-delete in
the SPA today (the admin tenant page is a read-only roster).

## Testing

DOM-interaction tests (click button -> modal opens -> DELETE) are not supported
by the host-side `cargo test --lib` harness (no wasm/browser test runner is set
up). The testable risk - the type-to-confirm gate - is covered by unit tests on
`confirm_phrase_satisfied`. If a wasm-bindgen-test + headless-browser harness is
added later, assert the open-before-DELETE flow there.

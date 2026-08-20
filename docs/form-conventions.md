# Create/edit form conventions (PMS-367)

External review found two create-form inconsistencies: the Company field used a
search-autocomplete on some forms and a native dropdown on others, and some
creates are full pages while others are modals. This codifies the conventions so
new forms pick correctly.

## Reference pickers: autocomplete vs native Select

Use the shared **autocomplete picker** for any reference field whose option list
can grow past a native dropdown's practical limit (roughly a few dozen rows).
Use a plain `Select` for small, bounded lookups.

| Field | Picker | Why |
| --- | --- | --- |
| **Company** | `crate::components::CompanyPicker` (autocomplete, server-filtered, inline-create) | A tenant can have thousands of companies. |
| Assignee / User | native `Select` | A tenant's user roster is small and bounded. |
| Work type, status, priority, type, queue, category, tax rate, payment term | native `Select` | Fixed/short tenant-config lookups. |
| Asset (on a ticket) | `crate::components::AssetPicker` (autocomplete) | Assets can be numerous. |
| Contact (attach to company) | `crate::components::ContactPicker` (autocomplete) | Contacts can be numerous. |

Native `Select` popups are themed by element-level `option` / `optgroup` rules in
`input.css` (MAPPS-479), so no call site adds option styling. Colors come from the
semantic variables, so both base modes follow automatically.

A hand-built floating dropdown panel (picker list, overflow menu, user menu,
notification list) takes its surface from the `dropdown-panel` class in `input.css`
and never re-declares `bg-raised` plus `shadow-lg` by hand; the call site keeps only
positioning, width, max-height, z-index and padding (MAPPS-483, guarded by
`scripts/check-kit-adoption.sh`).

### Company picker - every call site uses `CompanyPicker`

Ticket, Contact, Asset, Contract (create), Project, Invoice, and the Record-Payment
form all use `CompanyPicker`. The Contract **edit** form keeps a disabled `Select`
because a contract's company is immutable after creation (not a drift).

`CompanyPicker` props: `value` (display name), `selected_id: Option<String>`,
`required: bool`, `allow_inline_create: bool` (the "+ Create new company"
affordance, PMS-352), `onselect: (id, name)`, `onclear`.

### Known follow-up

The time-entry **Work Item** picker (a combined Ticket/Project select on the Log
Time form) is still a native `Select`. It is a dual-source picker (tickets +
projects, value-prefixed `ticket:` / `project:`) with no shared autocomplete
component yet; converting it needs a new combined picker. The open-ticket list is
usually modest, so this is deferred rather than blocking - see PMS-367 AC4.

## Required-field validation

Submit handlers validate **every** required field before bailing, and report each
failure in its own slot, so one missing field never masks another (PMS-514).

- Each required field gets its own inline error slot where it has one (e.g. the
  Title input's `error:` prop); cross-field / picker errors with no inline slot
  fall back to the form-level `error` signal at the top of the form.
- On submit: clear all slots, evaluate every required field and set each failed
  field's slot, then bail **once** (`is_submitting.set(false); return;`) if any
  failed - never short-circuit on the first failure. POST only after all pass.
- Do not trust the browser's native `required`: it accepts whitespace-only input,
  so trim and check client-side (MAPPS-281). The server stays the backstop - a
  server `field_message("<field>")` still routes to that field's inline slot.

Concretely, the new-ticket form evaluates Title and Company together and sets both
slots before returning, so submitting with both empty surfaces both errors. The
broader effort to make `required` actually enforce across every form, and to unify
the validation system, is the PMS-515 epic.

## Website fields

The company create/edit form's Website field (`validate_website_field` in
`src/pages/contacts.rs`) is the single place the app validates a web address, and it
normalizes rather than rejects (MAPPS-480):

- A scheme-less value is treated as a bare host and saved as `https://<value>`, keeping
  any path, query or fragment typed after it, so `DentalArtsPractice.com` is accepted.
  The server's own deserializer applies the same rule (PMS-805), so the two agree.
- `http://` and `https://` pass through unchanged. Every other scheme
  (`javascript:`, `data:`, `vbscript:`, `mailto:`, ...) is still rejected, as is any
  value carrying whitespace or control characters, because the stored value later
  becomes an `href` (MAPPS-213). Scheme detection goes through `utils::url::scheme_of`,
  so `java\tscript:` cannot slip past.
- A host must have at least one dot with a non-empty label either side, so `localhost`
  and `no-dot` do not become `https://` URLs.

On blur an invalid value surfaces its message in the field's inline error slot instead
of waiting for submit, and typing again clears it. A valid value is probed through
`GET /contacts/companies/website-probe` (the contacts router is nested under
`/contacts`), and what came back renders as help text under the field: in flight, the
address that answered (replacing the field value with it), or the reason the site could
not be reached together with the value that will be saved. The probe is **advisory**: it
never gates validation, never blocks or delays submit, and a save with a probe still in
flight uses whatever is in the field. Every failure path logs at `warn` before it
renders the note.

## Modal vs full page

The choice is **structural**, not stylistic:

- **Full page** (`/x/new` route) - top-level entity creates that stand on their
  own: Company, Contact, Project, Contract, Ticket, Asset, Invoice. The user is
  "going to create an X."
- **Modal** - child-of-parent creates where the parent must stay in view so the
  user keeps their place: Site under Company, Appointment on the calendar/project,
  Note under Ticket, Add-Contact-to-company, Record Payment, and the
  settings-lookup add/edit rows (work types, tax rates, ticket statuses, ...).

Rule of thumb: if the create only makes sense in the context of a specific parent
record (and the user should return to that parent afterward), it is a modal;
otherwise it is a full page.

The current app already follows this split; no mismatches were found during the
PMS-367 sweep. Consult this table before adding a new create form.

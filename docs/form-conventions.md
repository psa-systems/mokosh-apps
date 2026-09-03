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
| Company scope (KB article, MAPPS-515) | `crate::components::CompanyPicker`, multi-select, `allow_inline_create: false` | Same roster, so the same picker; an article takes several companies, and scoping an article is not a place to create a CRM company record. |

However a picker gets its options, it reads the WHOLE list through the
`get_all_*` helpers in `src/hooks/fetch.rs`, which page until a short page
arrives. mokosh-server caps `per_page` at 100 and clamps a larger ask instead
of rejecting it, so a picker that fetches one big page renders a truncated
option list that looks complete: the record the operator is looking for is
simply not offered, and nothing says why (MAPPS-528, enforced by
`scripts/check-per-page-cap.sh`). A list with its own pager is the exception:
it keeps `page={n}` and a per_page below the cap.

Native `Select` popups are themed by element-level `option` / `optgroup` rules in
`input.css` (MAPPS-479), so no call site adds option styling. Colors come from the
semantic variables, so both base modes follow automatically.

A hand-built floating dropdown panel (picker list, overflow menu, user menu,
notification list) takes its surface from the `dropdown-panel` class in `input.css`
and never re-declares `bg-raised` plus `shadow-lg` by hand; the call site keeps only
positioning, width, max-height, z-index and padding (MAPPS-483, guarded by
`scripts/check-kit-adoption.sh`).

### The dropdown keyboard contract (MAPPS-503)

Every typeahead behaves the same way, because every one of them uses
`use_dropdown_nav` (`src/hooks/dropdown_nav.rs`). A new typeahead uses the hook
too rather than growing its own handlers; five hand-rolled copies is how
`GlobalSearch` ended up the only surface with an Escape key.

| Input | Behaviour |
| --- | --- |
| Focus (tab in) or click on the field | Opens the list, including with an empty query, where the picker's unfiltered fetch supplies the rows. |
| Down / Up | Move the highlight one row, clamped at both ends (never wraps), opening the list first if it is closed. The active row scrolls into view. |
| Enter | Takes the highlighted row, or the first row when none is highlighted in a record picker (MAPPS-653). Does not submit the form. |
| Tab | Takes the highlighted row, or the first row when none is highlighted, then moves focus to the next field. Shift+Tab takes nothing. |
| Escape | Closes without committing, leaving the typed text alone, so a following Tab leaves the field with what was typed. |

The mechanics that go with it:

- The handlers sit on the field's wrapper `div`, never on the shared `Input`.
  Keydown bubbles up from the focused input, and MAPPS-347 already moved a
  handler off `Input` because handlers there interfered with inline-error
  rendering on the ticket-create form. The wrapper holds the input only: a
  wrapper that also contained the panel would re-open the list on the click that
  just picked a row.
- Result rows carry `tabindex="-1"`, so Tab commits and leaves instead of walking
  into the list.
- An inline "+ Create new ..." row is part of the list: it is the last navigable
  item, and committing it opens the create modal exactly as clicking it does.
  With nothing matching the typed text it is the ONLY navigable row, which is
  what makes Enter on an unknown name start a new record with that name
  prefilled (MAPPS-653), on the call sites that pass `allow_inline_create`.
- Whether Enter takes an unhighlighted first row is per surface, opted in with
  `DropdownNav::enter_takes_first_match` (MAPPS-653):

  | Surface | Enter with no highlight | Why |
  | --- | --- | --- |
  | `CompanyPicker`, `ContactPicker`, `AssetPicker`, `ProductPicker` | takes the first row | The field has to end up holding a record, so a typed name that matched something is accepted without arrowing onto it. |
  | `SuggestInput` (Industry, Title, Department) | takes nothing | Free text: the value is what was typed, and a suggestion is optional, so Enter must not overwrite it. |
  | `GlobalSearch` | takes nothing | Committing navigates the app away rather than filling a field. |
  | `MentionAutocomplete` | takes nothing | The popover sits over a textarea where Enter is the newline key. |
- ARIA comes with the hook: `role="combobox"` with `aria-expanded` and
  `aria-controls` on the field wrapper, `role="listbox"` on the panel,
  `role="option"` plus `aria-selected` on the rows, and `aria-activedescendant`
  naming the active row.
- A failed search is its own panel state: "Could not search. Try again.",
  distinct from "Searching…" and "No matches.", logged at `warn`. No picker
  drops the fetch error.

Native `Select` fields need none of this; the browser already gives
click-to-open, arrow navigation and Tab-commit.

### The inline-create modal keeps the keyboard (MAPPS-694)

Committing the create row opens a modal, and the keyboard path used to stop at
that boundary: `ModalDialog` focuses the dialog PANEL on mount, so the prefilled
name field was several Tabs away, and Enter in it did nothing. Typing an unknown
name and pressing Enter twice now creates the record.

| Input | Behaviour |
| --- | --- |
| The modal opens | Focus lands in the prefilled field: "Company name" on `CompanyPicker`, "First name" on `ContactPicker`. |
| Enter, anywhere in the modal body | Runs the same create action as the Create button, and is consumed, so it never reaches the form the modal opened on top of. |
| Escape | Cancels, from `ModalChrome`'s own handler. Closing restores focus to the picker, as every close path already did. |

The two pieces, both opt-in per call site so no other modal changes:

- `Input` takes `autofocus: bool` (default false), which sets the HTML attribute
  AND focuses the field on mount. The attribute alone is not enough: a browser
  ignores `autofocus` on an element inserted after load, which is every field in
  a modal. A call site never hand-rolls a raw `input` to get focus;
  `scripts/check-kit-adoption.sh` exists to keep fields on the shared component.
- `form::submit_on_enter` wraps the create action and goes on the modal BODY,
  never on `Input` (same MAPPS-347 rule as the dropdown handlers). On the body it
  commits from any field in the modal; it `prevent_default`s first, because the
  same Enter is otherwise the implicit submit of the parent form behind the
  modal. Only Enter is touched, which is what leaves Escape to the dialog.

Focusing the first focusable child from `Modal` itself would have covered both
pickers at once and is deliberately not done: it also lands the caret in a
destructive `ConfirmDialog`'s type-to-confirm box, which is a delete gate and no
place to start typing.

### Company picker - every call site uses `CompanyPicker`

Ticket, Contact, Asset, Contract (create), Project, Invoice, the Record-Payment
form, and the KB article form's company scope all use `CompanyPicker`. The Contract **edit** form keeps a disabled `Select`
because a contract's company is immutable after creation (not a drift).

`CompanyPicker` props: `value` (display name), `selected_id: Option<String>`,
`required: bool`, `allow_inline_create: bool` (the "+ Create new company"
affordance, PMS-352), `show_create_button: bool` (MAPPS-484, the same create
modal on a "+ New company" button beside the input, so it is reachable without
opening the dropdown; needs `allow_inline_create`, which owns the modal),
`onselect: (id, name)`, `onclear`.

### The contact form's two company paths

The Contact form is the one form that can save a company name **without** a
company (MAPPS-251 / PMS-402: a bare name and phone with no CRM record to point
at). The two paths produce different data and are named for it (MAPPS-484):

| Path | Control | Result |
| --- | --- | --- |
| Linked | `CompanyPicker`, including its "+ New company" button, reached from the "Add another company" button | A `contact_companies` row per link (PMS-806). The names appear under Companies. |
| Typed | the "Enter a name without creating a company" text link under the list | `company_name` on the contact only. No `companies` row, no link. |

Rules that follow from the split:

- The only control on the form labelled like a create is the picker's, and it
  creates. A label of the "+ Add Company" shape on the typed path is the defect
  MAPPS-484 fixed, and `company_source_tests` in `src/pages/contacts.rs` fails
  if it returns. Since MAPPS-481 the company block also carries an "Add another
  company" button; "add" there means add another LINK, never create a record,
  which is why the create wording stays on the picker's own button.
- The typed path is the **no-linked-company** case, so it is offered only while
  the list is empty, and linking the first company clears the typed value. The
  server rejects a non-empty `companies` list alongside a non-empty
  `company_name` with a 422.
- Switching paths clears the other path's value, so exactly one company source
  is ever submitted (the server rejects both together).
- While the typed field holds a value, the form states the outcome in that
  value: "Saved as a typed name. `<value>` will not appear under Companies."
- **Any surface showing a company name says which of the two it is.** Link
  colour is not a signal on its own: the contact detail page prints a muted
  "not a company record" note under a typed name, and the contacts list appends
  a muted "(typed)" in the company cell. A linked company shows neither.
- A typed name is recoverable: the contact detail page's "Create this company"
  POSTs `/contacts/companies` and then PUTs the contact's `company_id`. The
  server clears the stored freeform name when `company_id` is set, so the name
  becomes a link. Both calls report failure inline and log at `warn`; a created
  company with a failed link says the company exists and the contact still
  needs linking.

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
- **An inline error never outlives the value that caused it** (MAPPS-581). The
  message describes what was submitted, so the next edit of that field clears it
  and its red border, and it comes back only if the next submit fails again.
  Every field that renders an `error:` slot clears that slot from its own
  `oninput` / `onchange`: `clear_on_edit(value, error)` from
  `src/components/form.rs` for the plain set-the-signal case, or the clear as the
  first statement of a handler that does more. A repeating child row clears its
  own `error` field; a picker clears on select, because typing a search term
  does not yet satisfy "a company is required". The clear lives at the call site,
  not inside `Input`, because only the parent knows a submit happened and can
  re-raise the same message when the corrected value still fails.

Concretely, the new-ticket form evaluates Title and Company together and sets both
slots before returning, so submitting with both empty surfaces both errors. The
broader effort to make `required` actually enforce across every form, and to unify
the validation system, is the PMS-515 epic.

## Invisible characters: trimming is not enough (MAPPS-582)

**A value that looks the same to a person is the same value.** No text a user types
or pastes into this app carries a character that renders as nothing, and no exotic
Unicode space reaches a validator as anything other than a plain space.

`str::trim` does not get you there. It removes characters where `char::is_whitespace`
is true, and the characters that cause the trouble are Unicode format characters
(general category `Cf`) plus the soft hyphen, none of which are whitespace.
`char::is_control` does not either: it is true only for `Cc`, so it answers `false`
for U+200B and U+FEFF. Measured against the reported value `919-397-4144` with each
character appended:

| Appended character | Survives `.trim()` | Old phone validator |
| --- | --- | --- |
| U+200B zero width space | yes | rejected |
| U+FEFF BOM / zero width no-break space | yes | rejected |
| U+00AD soft hyphen | yes | rejected |
| U+200E left-to-right mark | yes | rejected |
| U+202F narrow no-break space | no | rejected |
| U+00A0 no-break space | no | accepted |

Both halves of that are defects. The visible half is a correct validator giving a
message the user cannot act on, because the offending character renders as nothing.
The silent half is worse: a field with no format rule (name, title, description,
note) accepts the character, saves it, and nothing ever says so, at which point
`Acme\u{200B}` and `Acme` are two records that look identical in every list, search
box and picker.

The rule, and where it is enforced:

- **Sanitize at the component boundary, once.** `Input`, `Textarea` and `SearchInput`
  in `src/components/form.rs` pass their `oninput` event through `sanitized`, which
  replaces the event's value with `crate::utils::text::strip_invisible`. Every
  text-entry surface in the app routes through those three, including the ones that
  look like they do not (`SuggestInput`, `GlobalSearch`, `CompanyPicker`,
  `ContactPicker`, `AssetPicker` all render `Input`), so a new form is covered
  without opting in. A raw `input {}` / `textarea {}` element calls
  `strip_invisible` in its own handler instead.
- **`strip_invisible` does not trim or collapse.** Trimming per keystroke makes the
  space in "John Smith" untypable. It removes the invisibles and maps every
  non-ASCII whitespace character (U+00A0, U+202F, U+2007, U+3000, ...) to a plain
  space; ASCII whitespace, including a textarea's newlines, passes through.
- **ZWJ (U+200D) and ZWNJ (U+200C) are not invisibles here.** They carry meaning in
  Persian, Arabic and Indic text and in emoji sequences, so removing them from free
  text corrupts legitimate names. Only `clean_strict` removes them.
- **Structured validators use `clean_strict`**, which is `strip_invisible` plus
  ZWJ / ZWNJ plus a trim: phone, postal code, country, email, URL, UUID, timezone,
  slug, date, money and the other numerics. A validator that strips whitespace does
  it with `char::is_whitespace`, never a hand-written set of space characters, which
  is how U+202F reached the E.164 check.
- **Password fields are exempt, and this is the one place "sanitize everything" is
  wrong.** A password may legitimately contain any character, and silently rewriting
  one turns a correct credential into a failed login with no diagnosis. `Input` skips
  sanitizing when `r#type == "password"`, which covers every password, secret and
  API-key field in the app, because they all set that type. A new secret field sets
  `r#type: "password"` for this reason as much as for the masking.

The server accepts and stores the same characters through the API, so this is the
usability half only; the data-integrity half is PMS-924.

## Repeating child rows

A field that holds several values of the same shape (a contact's phone numbers
and its company links, MAPPS-481 / PMS-806) is a **list of rows**, not a fixed
set of numbered fields. One `Signal<Vec<Row>>` holds the collection, an
"Add <thing>" button appends a row, and each row carries a remove control. The
same three rules apply to every such list:

- **Validate every row.** Each row struct carries its own `error: String`, bound
  to that row's inline slot. The submit handler clears every slot, evaluates
  every row, sets each failing row's own message, and bails **once** afterwards
  - the same rule as required fields above, applied per row, so one bad row
  never masks another's message. `validate_phone_rows` in
  `src/pages/contacts.rs` is the reference implementation and
  `contact_child_row_tests` covers it. A server field error naming an entry
  (PMS-806 answers `phones[2].number`) is routed back to that row's slot rather
  than to the form-level banner.
- **Exactly one primary.** The flag is a radio, so marking one row clears every
  other. The payload is built with the flag on the marked row, or on the first
  row when none is marked, which is also what the server does with a list that
  arrives with none flagged.
- **Order is the payload.** Rows are sent in the order they appear, and the
  server derives `sort_order` from the array index. Nothing re-sorts the list
  behind the user.

A row the user added and left blank is dropped rather than rejected, and a
contact with zero rows is valid: adding a row is not a commitment to fill it.
The list is always sent, including as `[]`, so removing the last row really
does unlink or delete.

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
of waiting for submit; typing again clears it under the form-wide rule in
`## Required-field validation`, not as anything specific to this field. A valid value
is probed through
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

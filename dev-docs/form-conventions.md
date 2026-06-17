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

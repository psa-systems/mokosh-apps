# Button variants - the canonical convention (PMS-357)

External review found primary "create" buttons drifting in colour (a white
"New Company" next to a blue "New Ticket"). This documents the one correct
variant per action so the drift does not recur. A live render of every variant
lives at the `/dev/buttons` route (`src/pages/button_showcase.rs`).

The component is `crate::components::Button` with `variant: ButtonVariant`
(`src/components/button.rs`). `Primary` is the `#[default]`, so a `Button` with
no `variant` is already blue - never rely on the default for a non-primary
action; set the variant explicitly.

## Variants

| Variant | Looks | Use it for |
| --- | --- | --- |
| `Primary` | Blue fill, white text | The page's single main action: every **"New &lt;thing&gt;"** / create / submit button. |
| `Secondary` | Gray fill | Neutral companion next to a Primary: **Cancel, Close, Filter, Back**. |
| `Danger` | Red fill | Destructive confirmation: **Delete, Revoke, Remove** (usually inside a confirm dialog). |
| `Ghost` | Transparent, hover tint | Low-emphasis inline / toolbar action where a filled button is too heavy: **Edit** on a row. |
| `Link` | Blue text, no fill | Inline action that reads as a hyperlink: **"View all"**, "Change". |

## Sizes

`ButtonSize`: `Small` (dense toolbars / table rows), `Medium` (default - forms,
page headers), `Large` (hero CTAs).

## States

- `loading: true` - shows a spinner and disables the button (blocks double-submit).
- `disabled: true` - dims and ignores input.

## Rules

1. A "New &lt;thing&gt;" / create / submit action is **always** `Primary`. No
   exceptions - this is the rule the review flagged.
2. Each form has exactly one `Primary`, and its Cancel is `Secondary`. In a single-step form that `Primary` is the submit. In a multi-section modal that builds one record across tabs (the request-form editor in `src/pages/forms.rs`), the `Primary` is the action that carries the operator forward: `Next: <section>` while a later section exists during creation, and the submit on the last section.
3. Destructive actions are `Danger`, and should be guarded by a confirm dialog.
4. Do not invent ad-hoc Tailwind colour classes on a raw `button` to fake a
   variant - extend `ButtonVariant` if a genuinely new style is needed, so the
   set stays enumerable and reviewable.

## Reviewing

When reviewing a new screen, open `/dev/buttons` beside it: the create button
should be the same blue as the showcase's Primary, and Cancel the same gray as
Secondary. Anything else is drift - fix the call site's `variant`.

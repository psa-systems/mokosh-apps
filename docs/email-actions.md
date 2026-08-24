# Email-sending actions (MAPPS-482)

Companion to `destructive-actions.md`. A click that makes the server email a
client or a colleague is a one-way door: the message is out, and the operator
finds out what it said by asking the recipient. Two affordances, on every such
trigger, so that stops being true.

## The pattern

1. The button renders `MailIcon` at `IconSize::Small` before its label, exactly
   as `PlusIcon` is used elsewhere:

   ```rust
   Button {
       variant: ButtonVariant::Primary,
       onclick: handle_send,
       MailIcon { size: IconSize::Small, class: "mr-2".to_string() }
       "Send link"
   }
   ```

   The icon is decorative beside a visible label, so it carries no accessible
   name of its own.

2. `crate::components::EmailPreview` sits beside the button, taking the
   notification event type the send dispatches and the context the form already
   holds:

   ```rust
   crate::components::EmailPreview {
       event_type: "forms.request_link".to_string(),
       context: serde_json::json!({ "recipient_email": to, "company_name": name }),
   }
   ```

   It renders a "Preview email" text button that opens a modal and calls
   `POST /notifications/preview` (mokosh-server, PMS-808), which renders exactly
   what `dispatch` would render and sends nothing. Per returned entry the modal
   shows the recipients, the subject and the body.

## Rules the component holds

- **`body_html` never reaches the DOM as markup.** It is a tenant-editable
  template; rendering it inside an authenticated app is an XSS surface bought
  for a cosmetic gain. The modal shows `body_text`, and when that is all there
  is, the HTML source escaped inside a `pre`. There is no `dangerous_inner_html`
  in `components/email_preview.rs` and there must never be one.
- **Send-time values are named, not faked.** A minted token and its link do not
  exist when the preview runs, so `render_template` leaves `{{link}}` literal
  and names the key in `unresolved`. The modal lists each one under the body as
  "filled in when sent: link", so the operator reads the braces as expected
  rather than as a broken template.
- **Failures are visible.** An empty response renders "No email rule matches
  this action, so nothing will be sent", which is a genuinely important thing to
  learn before clicking Send. A failed request renders an inline error in the
  modal and logs with `tracing::warn!`.
- **The preview never gates the send.** Slow, empty or failed, the send button
  is exactly as it was. The preview answers a question; it does not hold a lock.

## Current coverage

| Trigger | File | Event type |
| --- | --- | --- |
| Send a request form to a client | `src/pages/request_links.rs` (`SendRequestLinkModal`, reached directly from the company page and via `SendFormToClientModal` from the form builder) | `forms.request_link` |
| Invite a colleague | `src/pages/team.rs` | `invitations.created` |
| Send a quote to the client | `src/pages/quotes.rs` | `quote.sent` |
| Send an invoice to the client | `src/pages/billing.rs` (`InvoiceDetailPage`) | `billing.invoice_pay_now` |
| Email a ticket note to the client | `src/pages/tickets.rs` (the journal composer on `TicketDetailPage`) | `ticket.note` |

Only `forms.request_link` is a notification rule today. mokosh-server builds the
invite mail in `invitations/service.rs`, the quote mail in `quotes/service.rs`,
the invoice mail in `billing/service.rs` and the ticket-note mail in
`tickets/service.rs` (`send_note_email`), each with a built-in template and none
through `dispatch`, so their previews come back empty. Those four call sites
pass `empty_note` so the modal says the message is built into the server
and an email is still sent, rather than leaving the operator to read "nothing
will be sent" and believe it. MAPPS-489 moves the sends onto the dispatcher and
removes the note.

The invoice send and the ticket note are **conditional server-side**, which the
other two are not. `notify_invoice_pay_now` fires only on the first transition
into `sent`, and skips the mail entirely when the tenant has no active payment
gateway, when the invoice has no billing contact, or when that contact has no
email on file. The page says so under the header rather than promising an email
that may not go.

The ticket note (MAPPS-517) is conditional twice over. `add_note` mails only a
PUBLIC note whose `send_email` flag is on, and only when the ticket has a
contact with an address; an internal note never leaves the building whatever the
flag says. So the trigger is a checkbox on the journal composer rather than a
button of its own: it is off by default, disabled while the note is internal,
and its help text names both conditions. The affordances hang off the composer's
submit, which renders `MailIcon` in place of `PlusIcon` exactly when the click
will mail someone. What actually happened is not guessed at afterwards either:
the server records `is_email_sent` on the note row, and the journal line reads
"added a public note and emailed the client" or "(not emailed)" from it.

The invoice entry is the only one not keyed on a URL. The send is `PUT /invoices/{id}`
with `{"status": "sent"}`, a path shared with Edit, Void and the invoice delete
in `src/pages/contacts.rs`, and matching that shape flags two files that send
nothing (`src/pages/contacts.rs` and `src/modules/billing/routes.rs`). The guard
keys on `invoice_send_path(` instead, a helper that exists because the call
sends mail. A send whose endpoint is not dedicated needs a symbol like that.

## Keeping it true

### What the guard actually promises

Worth knowing before trusting it. It resolves each entry to the set of **files**
that call it, then requires each of those files to render `MailIcon {` and
`EmailPreview {` somewhere. `src/pages/billing.rs` is 3,700 lines and holds
Send, Edit and Void; one rendered icon anywhere in it satisfies the check, and
the guard cannot tell which button it sits on.

So it catches the case it was written for - a send trigger added with no email
affordance anywhere near it - and not a misplaced one. Matching the rendered
element rather than the bare name (MAPPS-539) closed a weaker hole: a plain
`grep MailIcon` was satisfied by the import line, so a file could import the
icon, render none, and pass. Making it call-site aware needs a proximity rule,
and the fetch usually sits in a handler while the icon sits in the `rsx!`, far
apart; that is a separate change with its own false positives.

`scripts/check-email-affordance.sh` (run by `just check`) holds the list of API
paths known to send email. For each one it fails when a file that calls it
renders no `MailIcon`, when that file offers no `EmailPreview`, and when nothing
calls the path any more.

**That list is maintained by hand, and adding a new email-sending endpoint means
adding it to `EMAIL_PATHS` in the script.** There is no way to derive it:
whether an endpoint sends mail is a fact about mokosh-server, not about anything
visible in this repo's source.

The guard self-tests (`--self-test`) over generated fixtures, so an edit that
neuters either check fails instead of reporting clean. MAPPS-490 adds it to the
Forgejo `check.yml` job, which lists the guards one step each.

## Testing

The modal's DOM interaction is not testable by the host-side `cargo test --lib`
harness (no wasm or browser runner is set up), the same limitation
`destructive-actions.md` records. What is testable is the decision that keeps
the XSS rule: `preview_body` picks text over HTML and only ever returns HTML as
source, unit-tested in `components/email_preview.rs`. The wiring itself is
enforced by the guard rather than by a test.

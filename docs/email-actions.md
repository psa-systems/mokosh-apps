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

Only `forms.request_link` is a notification rule today. mokosh-server builds the
invite mail in `invitations/service.rs` and the quote mail in
`quotes/service.rs`, both with a built-in template and neither through
`dispatch`, so their previews come back empty. Those two call sites pass
`empty_note` so the modal says the message is built into the server and an email
is still sent, rather than leaving the operator to read "nothing will be sent"
and believe it. MAPPS-489 moves both sends onto the dispatcher and removes the
note.

## Keeping it true

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

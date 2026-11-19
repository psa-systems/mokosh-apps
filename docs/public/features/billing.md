# Billing

Billing is where invoices and quotes are created, approved, sent, paid, and reconciled. This page covers the lifecycle end-to-end as it appears in mokosh-apps.

The server side of billing (the money model, delegated issuing, overdue derivation) is in the mokosh-server documentation. This page is for the people clicking.

## Invoices

### Creating an invoice

From Billing, click New invoice. Pick:

- **Customer**: the company the invoice is for.
- **Recipient**: the contact (or mail audience) the invoice is sent to.
- **Currency**: the invoice currency. Defaults to the tenant currency; override only when billing a customer in another currency.
- **Net terms**: the number of days from issue to due. Defaults to the tenant default net terms.

Add lines. Each line has a product (or service), a quantity, and a unit price. The line total, the tax, and the invoice total are computed; the operator does not type them.

Save as Draft, or Send to issue and mail in one action.

### Draft, Issued, Paid

An invoice moves through:

1. **Draft**: editable in place. No customer-visible state.
2. **Issued**: the invoice has been sent. Editing is limited; see Amendments below. The PDF bytes are frozen at first send.
3. **Paid**: a payment matching the balance has been recorded. The invoice is marked Paid and the activity feed records who recorded the payment (or which integration imported it).

An invoice that misses its due date becomes Overdue (a derived state, not a status change). The client flags overdue items in the Overdue widget on the Dashboard; the invoice itself keeps Issued until paid.

### Amendments

An issued invoice is replaced, not edited. Open the invoice, click Amend. Mokosh clones the invoice as a fresh draft, voids the original, and marks the new draft as the replacement. Send the amendment and the original closes out.

Amending is audited; the customer-facing log shows the original being voided and the replacement being issued.

### Sending

Send from the invoice's Actions menu. Mokosh mails the PDF to the mail audience the invoice names, records the send on the activity feed, and starts the net-terms clock. Sending is what moves the invoice to Issued; a draft that was never sent stays a draft indefinitely.

Resend from the same menu when the contact says they did not receive it. A resend does not change the state; the invoice is still Issued.

### Payments

Record a payment manually from the invoice: enter the amount, date, and method, and save. Mokosh updates the invoice balance and marks it Paid when the sum of recorded payments equals the invoice total.

Payments can also arrive through integrations (Stripe, PayPal, or a delegated-issuing provider like Xero or QuickBooks). An imported payment attaches to the invoice automatically when the integration carries the reference.

### Delegated issuing

When the tenant issues invoices through Xero or QuickBooks, the Send action hands the invoice to the provider instead of mailing it. The invoice shows Sent to Xero (or QuickBooks) with a link to the provider's record. Payment reconciliation flows back through the same integration.

## Quotes

### Creating a quote

Quotes behave like invoices with an approval step and a convert-to-invoice action.

From Billing, click New quote. Pick the customer and recipient, add lines, save as Draft or Send.

### The approval chain

Some tenants require approval before a quote is sent. The quote carries an Approval card showing the pending approver; the approver sees the request in their notifications and approves or rejects from the quote.

Approving sends the quote to the customer. Rejecting records a reason and returns the quote to Draft.

### Accept, decline, expire

Once sent, a quote waits for the customer. Record the customer's decision from the quote:

- **Accept**: converts the quote to an invoice (draft by default, issued if the Convert and send action is used instead). The quote is marked Accepted.
- **Decline**: records a reason. The quote is marked Declined and read-only.
- **Expire**: a quote past its expiry is marked Expired automatically. The operator sets the expiry when creating the quote; the tenant default applies when none is picked.

## Prepaid blocks

A prepaid block is a chunk of hours or value a customer paid for in advance. Time entries, tickets, or invoices draw down against the block; overage falls through to a new invoice at the block's rate. Unused hours roll at the period close, per the block's rollover rule.

The Billing section shows the block's remaining balance and the current period's draw-down. The block is managed from the customer's billing tab, not from Billing itself.

## The customer-facing view

Every invoice and every quote has a customer-facing view. The URL is unique per document, carries the tenant's branding, and shows the document, the payment options, and the activity log stripped of staff-only entries.

The staff view of the same document carries the staff bar and a View as visitor action to see what the customer sees. See `The shell`.

## Payments appear where

A payment recorded in Mokosh (manually, or through an integration) appears:

- On the invoice it closes out, with the amount, date, and method.
- On the customer's billing tab, in the payment history.
- In the Dashboard revenue widget for the range the payment date falls in.
- In the activity feed for the day the payment was recorded.

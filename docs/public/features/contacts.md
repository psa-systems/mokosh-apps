# Contacts

A contact is a person at a customer. A contact raises a ticket, signs a quote, or receives a customer-facing email. This page covers creating and editing contacts, the role field on the contact's company, and how mail audiences choose who gets which message.

## Creating a contact

From a customer overview, open Contacts and click New. Or from the global New menu in the top bar, pick New contact and choose a customer.

Fields:

- **Name**: first and last. Required.
- **Email**: the primary email Mokosh sends customer-facing messages to.
- **Phone**: optional; shown on the customer overview.
- **Role on company**: the contact's role at the customer. See below.
- **Portal access**: off by default. When on, the contact can sign in to the client portal.

Save. The contact is added to the customer's Contacts list.

## Role on company

Role on company is the contact's job function at the customer. Common values the tenant can preset: Owner, Operations, Billing, IT lead. The role is attached to the contact-company link, not to the contact itself; the same person at a different customer can hold a different role.

Role drives mail audiences (see below) and is the label every customer-facing feature uses when identifying the recipient.

## Mail audiences

A mail audience is a named group of contacts the tenant can send to in one action. Audiences are evaluated per customer, not per tenant. Example audiences:

- **All contacts**: every contact at the customer.
- **Billing contacts**: every contact whose role is Billing.
- **Portal users**: every contact with portal access on.

A tenant customises the audience list. Common tenant-defined audiences: Owners only, IT leads only, Monthly report recipients.

Each message goes to one audience. A contact that matches two audiences receives two events, never one merged mail; the customer-facing log shows both.

## Editing a contact

Change any field in place from the contact's card on the customer overview. Email changes are audited and show up in the activity feed.

Deactivate a contact when the person has left the customer. A deactivated contact is hidden from default filter views, is not evaluated for mail audiences, and cannot sign in to the portal. Reactivate by flipping the status back.

Deleting a contact is reserved for records that were never real: a typo, a test entry. A contact with any ticket, quote, or invoice history is deactivated, not deleted, so the history keeps its reference.

## Portal access

Turning Portal access on invites the contact to the client portal. On save, Mokosh sends the invite email through Bunyip so the contact can sign in with a password or an identity provider.

The contact sees only their own customer in the portal. The staff bar in the signed-in shell is where staff see the same page as the contact would (the View as visitor action; see `The shell`).

## Where contacts show up

- On the customer overview under Contacts.
- On a ticket as the Contact field, when the ticket names one.
- On an invoice or a quote as the recipient (the mailing list the message went to).
- In the activity feed when the contact was involved.

A ticket, invoice, or quote keeps the contact it was created with; later changes to the contact (name, role, email) propagate, but changing the contact on an item is a deliberate action, not an automatic update.

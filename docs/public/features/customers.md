# Customers

In Mokosh a customer is a company the tenant does work for. This page covers what sits on the customer record, how it differs from a contact, and how to find a customer in the listing.

## Customer versus contact

A **customer** is a company: the organisation Mokosh bills, the entity on the invoice, the record tickets and quotes and time entries attach to.

A **contact** is a person: the human at the customer who raises a ticket, signs a quote, or reads a client-portal link.

One customer has zero or more contacts. One contact belongs to one customer at a time. Contacts have their own page: see [Contacts](contacts.md).

## Creating a customer

Open Customers from the sidebar, click New. Fill in:

- **Company name**: the display name used across the tenant.
- **Legal name**: the name that appears on invoices and PDFs. Defaults to company name; override when the legal entity differs.
- **Website**: optional. If set, it is a link from the customer overview.
- **Primary contact**: optional. Pick an existing contact or create a new one from the New customer dialog itself.
- **Address**: used on invoices and quotes.
- **Tax identifier**: used on invoices where the tenant country requires one.
- **Tags**: free-form labels for grouping. Filter by tag on the Customers list.

Click Create. The customer is saved and the overview opens.

## The customer overview

The overview card is the customer's home page in Mokosh. Sections, each a subpage:

- **Summary**: the identifying fields (name, address, tax id), the primary contact, the active SLA if any.
- **Contacts**: every contact at the customer. Add, edit, or remove from here.
- **Tickets**: the customer's open and recent tickets. Click a row to open the ticket.
- **Billing**: the customer's invoices, quotes, and payment history. Click an invoice to open it in Billing.
- **Time**: billable time logged against the customer, grouped by month.
- **Notes**: a running log of free-form notes. One line per event; the latest at the top.

The overview also carries the Create action menu: new ticket, new quote, new invoice, new contact, each scoped to this customer.

## Listing and filtering

The Customers list is the sortable, filterable table of every customer in the tenant. Filters:

- **Status**: Active, Prospect, Inactive. The Status field on the customer drives this.
- **Tag**: pick one or more tags. The list shows customers carrying any of them.
- **Owner**: the account manager assigned to the customer, when the tenant tracks one.
- **Country**: filters by the address country.

Sort by company name, last activity, or outstanding balance.

The URL carries the active filter and sort, so a saved URL is a saved view.

## Deactivating a customer

A customer the tenant no longer works with is deactivated, not deleted. Deactivation hides the customer from default filter views (filter Status includes only Active by default) and prevents new tickets, quotes, and invoices from attaching to it. History stays intact.

Reactivate by setting Status back to Active.

## Merging customers

When the same company is in Mokosh twice (an import duplicate, or a manual re-entry), merge them. From the overview of the surviving record, open Actions and pick Merge. Enter the display name of the duplicate, confirm the choice, and Mokosh moves every contact, ticket, quote, invoice, and time entry to the surviving record. The duplicate is deactivated and tagged so the merge is reversible in the audit log, but not in the UI.

Merging across tenants is not supported.

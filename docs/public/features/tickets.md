# Tickets

Tickets are the unit of work in Mokosh. One ticket covers one customer request from intake to resolution. This page covers creating, viewing, editing, assigning, approving, and resolving a ticket, and how tickets that originated outside Mokosh stay read-only.

## Creating a ticket

Open Tickets from the sidebar, click New. Fill in:

- **Customer**: pick the company the ticket is for. If the customer does not exist yet, create it first in Customers.
- **Contact**: the person at the customer who raised the request. A ticket can carry no contact; many internal tickets have none.
- **Title**: one line that says what the ticket is about.
- **Description**: the detail. Markdown is supported; images can be pasted in.
- **Assignee**: who owns the ticket. Defaults to the signed-in user; leave unassigned to triage later.
- **Priority**: Low, Normal, High, Urgent. Priority drives the SLA clock.
- **Due date**: optional, set by the SLA when the priority has one configured.

Click Create. The ticket is saved, the activity feed records who created it, and the ticket detail opens.

## Viewing a ticket

The ticket detail shows:

- **Header**: title, customer, contact, status, priority, due date. Click any of these to edit in place.
- **Timeline**: every change in reverse-chronological order: notes, status changes, assignments, approvals, attachments.
- **Composer**: a markdown field for the next note. Notes attach to the timeline in order.
- **Side panel**: the SLA clock, the linked customer card, the linked time entries, and the related tickets.

The composer has three send modes: a private note that only staff see, a customer reply that emails the contact and attaches to the timeline, and a resolution that moves the ticket to Resolved.

## Assigning a ticket

Change the assignee from the header or from the Assignee action on the Tickets list.

Rules:

- A ticket can have one assignee at a time.
- Unassigning is explicit; set the assignee to Nobody.
- The new assignee is notified. The old assignee, if there was one, is notified too.
- Assignment is recorded on the timeline.

## Approving a ticket

Some tenants require approval before certain changes take effect: a billable quote attached to the ticket, a scope change beyond the agreed estimate, a resolution that would send a customer-facing message. Approvals rise to the approver configured for the customer or the tenant; the approver receives the request in their notifications and the ticket's timeline shows the pending state.

Approve or reject from the approval card on the ticket. A rejection records a reason and returns the ticket to the previous state.

## Resolving a ticket

Resolve from the composer's resolution mode, or from the Status action in the header. Fill in a resolution note and pick a resolution kind (Fixed, Duplicate, Not reproducible, Won't fix, or a tenant-defined custom kind).

A resolved ticket does not re-open on its own. To re-open, click Reopen in the header; the resolution moves to the timeline and the ticket returns to its previous status. Repeated re-opens are recorded.

## Pushed tickets

Tickets that originate outside Mokosh (for example, an import from a legacy system or an inbound email processed by an integration) are marked as pushed. A pushed ticket is read-only in the client, with every edit control disabled and a note in the header naming the source. The timeline is still writable: notes, assignments, and the SLA clock continue.

To take a pushed ticket off the read-only track, click Detach source on the header. Detaching is permanent for that ticket; the next push from the same source does not overwrite it.

## Filters and search

The Tickets list has filters for customer, assignee, status, priority, created range, and tags. Combine filters; the result updates live. The URL carries the active filter set, so a saved URL is a saved view.

The global search in the top bar searches ticket titles and descriptions across the tenant. Click a result to open the ticket.

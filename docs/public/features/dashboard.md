# Dashboard

The Dashboard is the home screen after sign-in. It shows the signed-in user what is open, what is overdue, and what changed recently in the tenant.

## The ranges

Every widget on the Dashboard takes one time range. Change it with the range picker at the top of the Dashboard. Options:

- **Today**: midnight to now in the user's time zone.
- **This week**: the current ISO week.
- **Last 7 days**: rolling seven-day window ending now.
- **This month**: first of the month to now.
- **Last 30 days**: rolling thirty-day window.
- **Custom**: pick a start and end date.

The pick persists for the signed-in user; the next visit opens on the same range.

Today is always the user's own time zone, not the tenant's. A tenant-level number (for example, "tenant invoices paid today") is computed in the tenant's day.

## The widgets

The exact set on the Dashboard depends on the signed-in user's role. These are the widgets most roles see.

### My open work

A list of the tickets assigned to the signed-in user, grouped by status: In progress, Waiting on customer, Scheduled. Each row names the ticket, the customer, and the last update. Click a row to open the ticket.

### Overdue

Everything past its due date: invoices awaiting payment, quotes awaiting approval, tickets with an overdue SLA. Each row carries the overdue-by period and the owner. Click a row to open the item.

### Activity feed

A reverse-chronological stream of what happened in the tenant during the selected range. One line per event: a new ticket, a payment received, a quote approved, a user invited. Click a row to open the originating item.

The feed is scoped to the tenant, not the user. An admin sees every event; a technician sees events they are involved in or that affect their customers.

### Revenue this period

For roles that can see billing, a running total of paid invoices in the selected range. Click through to Billing for the detail.

### Hours this period

For roles that can log time, a sum of billable segments the user recorded in the selected range. Click through to Time tracking for the breakdown by customer and day.

## What the Dashboard does not show

The Dashboard is a summary, not an alert queue. Alerts that need action route through notifications or email, not through a Dashboard widget. The feature-flag badges in the top bar are the place to see what is on or off for the current deployment.

## What the operator configures

Nothing. The Dashboard has no per-deployment configuration; it reads from the same API every other screen does. The widgets honour the signed-in user's role, so a role that cannot see billing sees no revenue widget.

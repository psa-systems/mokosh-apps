# Time tracking

Time tracking is how staff record what they worked on and when. This page covers starting and stopping segments, entry kinds, the work-day view, and approving timesheets.

## Segments

A segment is one continuous span of work on one ticket or project. Start a segment from a ticket's Start timer button, from the Time section's Start new action, or from the top-bar timer control.

Fields on a segment:

- **Customer**: the company the work is for.
- **Ticket or project**: the work the time attaches to.
- **Entry kind**: see below.
- **Notes**: a short description of what was done. Shown on timesheets and on the customer's time tab.

The timer runs until stopped. Only one segment can run at a time for one user; starting a new segment stops the previous one. The stop time and duration are recorded.

Edit a segment after the fact to correct the time, the notes, or the ticket. Edits are audited; the audit log shows who changed what and when.

## Entry kinds

Entry kind splits client work from the MSP's own time.

- **Billable**: counts against the customer's prepaid block or produces a billable line on the next invoice.
- **Non-billable**: tracked but not billed. Used for internal support, no-charge goodwill, or work covered by a flat-rate SLA.
- **Internal**: the MSP's own time (team meetings, training, admin). Does not attach to a customer; the customer field is empty.

Internal time shows up on the staff member's timesheet and in the tenant's internal-time reports, but not on any customer's time tab.

## Work days

The work-day view shows the signed-in user's time for one day, broken into segments. Each segment is a row with its range, duration, customer, ticket, and notes. The totals at the bottom add up to the Billable, Non-billable, and Internal hours for the day.

Today is always the user's own time zone, not the tenant's. A tenant-level report (for example, the tenant's billable hours per day) is computed in the tenant's day instead; the two never overlap in the UI.

Switch days with the date picker above the view. Gaps in the day are visible as empty strips so an untracked hour is visible, not hidden.

## Editing a work day

A segment is editable until the work day is submitted for approval (see below). Edits inside an unapproved day:

- **Change the time**: drag the segment's edges on the day view, or edit the start and end fields.
- **Change the customer or ticket**: open the segment, pick the new one, save.
- **Split a segment**: at a point in the middle, click Split. One segment becomes two.
- **Delete a segment**: open the segment, click Delete. The audit log keeps the original.

Overlap is not allowed within a user's day; the editor refuses an overlap and names the segment that would collide.

## Timesheets

A timesheet is one week of a staff member's time, submitted as a unit. From Time, open Timesheet. The week view shows every segment for the user, grouped by day, with Billable, Non-billable, and Internal totals.

Submit the timesheet from the Submit for approval action. The timesheet moves to Pending approval; the configured approver sees it in their notifications and on their Approvals list.

Timesheets are a gated feature. A tenant that does not require timesheet approval can log time without ever submitting a week; the Time section is still available, and work days are still editable.

## Approvals

The approver opens the timesheet from their Approvals list. The view is the same week view the submitter saw, with an Approve and a Reject action.

Approve:

- Locks every segment in the week against edits.
- Produces the billable lines on the customer's next invoice.
- Clears the timesheet from the Approvals list.

Reject:

- Returns the timesheet to Draft.
- Records a reason the submitter sees on their timesheet.
- Leaves the segments editable so the submitter can fix the week and resubmit.

A segment in an approved week can still be corrected; the correction opens an audit entry and requires re-approval on the correction itself.

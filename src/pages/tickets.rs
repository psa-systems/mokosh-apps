//! Ticket pages

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    clear_selection, ticket_status_badge, use_bulk_selection, use_page_title, AlertType, Badge,
    BadgeVariant, BulkActionsBar, BulkSelection, Button, ButtonVariant, Card, Checkbox, ClockIcon,
    DataTable, ErrorBanner, IconSize, MailIcon, Modal, PageHeader, PencilIcon, PlusIcon,
    SearchInput, Select, SelectAllHeader, SelectOption, SelectRowCell, SortDirection, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow, Textarea,
    UserCircleIcon,
};
use crate::components::{ChangeDetails, ChangeLine};
// MAPPS-596: shared with the project, task and asset change-history panes.
use crate::modules::audit::{action_label, fields_label, title_field};
use crate::utils::{FormGuard, Paginated, Rule};

/// MAPPS-546: rows per page on the ticket list, sent to the server rather than
/// written into the table as a constant.
const PER_PAGE: usize = 25;

/// MAPPS-517: entries rendered in the ticket journal. The stream merges three
/// sources, each read in full, so a long-running ticket would otherwise render
/// hundreds of list items; the count of the rest is stated under the stream.
const JOURNAL_LIMIT: usize = 50;

/// What the email preview says for a public note. mokosh-server builds the
/// note mail in `tickets/service.rs` with a built-in template rather than
/// through the notification dispatcher, so the preview comes back empty and
/// would otherwise read as "nothing will be sent". Same shape as
/// `QUOTE_PREVIEW_NOTE` in `quotes.rs` (docs/email-actions.md).
const NOTE_PREVIEW_NOTE: &str = "The ticket-note email is built into the server rather than by a notification rule, so there is nothing to render yet. The ticket's contact is still emailed the note.";
use crate::Route;

/// Subset of mokosh-server's `TicketResponse` we render in the list. The
/// server returns more fields; serde silently drops the ones we don't
/// ask for, so adding columns later just means extending this struct.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicket {
    id: uuid::Uuid,
    ticket_number: String,
    title: String,
    #[serde(default)]
    company_name: String,
    #[serde(default)]
    status: RemoteSummary,
    #[serde(default)]
    priority: RemoteSummary,
    #[serde(default)]
    assigned_to_name: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RemoteSummary {
    /// Server-side `TicketStatusSummary` / `TicketPrioritySummary` always
    /// carry an `id`; the field is optional here only because legacy code
    /// paths may omit it. PMS-359 reads it as the canonical "currently
    /// saved" selection for the inline editors on the ticket detail page.
    #[serde(default)]
    id: Option<uuid::Uuid>,
    #[serde(default)]
    name: String,
}

/// The fields of mokosh-server's `TicketResponse` the detail page renders.
/// Serde drops every field we don't ask for. The SLA pair (`sla_due_date`
/// + `sla_status`, a snake_case enum) drives the at-risk / breach badge.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketDetail {
    #[serde(default)]
    ticket_number: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    company_id: Option<uuid::Uuid>,
    #[serde(default)]
    company_name: String,
    #[serde(default)]
    contact_name: Option<String>,
    #[serde(default)]
    queue_name: String,
    #[serde(default)]
    status: RemoteSummary,
    #[serde(default)]
    priority: RemoteSummary,
    #[serde(default)]
    assigned_to_id: Option<uuid::Uuid>,
    // PMS-359: assigned_to_name is no longer read on the detail page
    // (the inline Assignee editor renders the chosen user by looking up
    // the id in the cached `/auth/users` list); dropped from the
    // deserialise shape entirely to keep the type honest. The list page
    // still reads `RemoteTicket.assigned_to_name`.
    /// PMS-344: the asset this ticket is associated with, if any. Both
    /// the id (for the inline AssetPicker editor + the asset-detail
    /// link) and the name (so the sidebar can render the asset's
    /// display name without an extra fetch) come straight off the
    /// server's joined TicketResponse.
    #[serde(default)]
    asset_id: Option<uuid::Uuid>,
    #[serde(default)]
    asset_name: Option<String>,
    /// PMS-730: the KB article describing HOW to perform this ticket's
    /// work, stamped by the request-form flow. The server joins the
    /// title on every read so the sidebar links it without a second
    /// fetch. Not `source_kb_article_id`, which is the article the
    /// ticket was opened FROM (server migration 099 keeps them apart).
    #[serde(default)]
    procedure_kb_article_id: Option<uuid::Uuid>,
    #[serde(default)]
    procedure_kb_article_title: Option<String>,
    #[serde(default)]
    created_by_name: String,
    created_at: DateTime<Utc>,
    #[serde(default)]
    sla_due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    sla_status: SlaStatus,
}

/// One row from `GET /tickets/statuses` (PMS-359). Tenant-scoped lookup
/// powering the inline Status editor on the ticket detail sidebar. The
/// `is_closed` flag is read so the renderer can keep the badge colour
/// stable when the user picks a Resolved / Closed row.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketStatus {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    is_closed: bool,
}

/// One row from `GET /tickets/priorities` (PMS-358 / PMS-359). Tenant-
/// scoped lookup the New Ticket form (PMS-358, defaults to the row with
/// `is_default = true`) and the detail-page inline editor (PMS-359)
/// both consume. Server's `CreateTicketRequest` / `UpdateTicketRequest`
/// both accept `priority_id: Option<Uuid>`, so any non-UUID would be
/// silently coerced away.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketPriority {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    is_default: bool,
}

/// MAPPS-296: minimal shape of a row from `GET /tickets/types` /
/// `GET /tickets/categories`. Both endpoints share the `(id, name)`
/// projection the New Ticket form needs; serde drops every other
/// field. Created at the top so the new lookups in `TicketNewPage`
/// resolve.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTicketLookup {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
}

/// MAPPS-296: minimal user-row shape for the New Ticket assignee
/// dropdown. The server's user list lives at `/auth/users`. `full_name`
/// is the precomputed `first last` projection; we fall back to `email`
/// when it is empty.
#[derive(Clone, Debug, Deserialize)]
struct RemoteUserLookup {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    email: String,
}

/// A ticket note (`GET /tickets/:id/notes`), rendered as a journal entry.
#[derive(Clone, Debug, Deserialize)]
struct RemoteNote {
    /// MAPPS-593: needed to address the note for an edit. The server has always
    /// sent it (`TicketNoteResponse.id`); this DTO simply never read it.
    id: uuid::Uuid,
    #[serde(default)]
    note_type: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    created_by_name: String,
    /// MAPPS-517: the server records on the note row whether the public-note
    /// email actually went out (`TicketNoteResponse.is_email_sent`), so the
    /// journal states what happened rather than what was asked for.
    #[serde(default)]
    is_email_sent: bool,
    /// MAPPS-593: who wrote it, so the viewer can be told apart from everyone
    /// else without comparing display names.
    #[serde(default)]
    created_by_id: Option<uuid::Uuid>,
    /// PMS-449: set when a portal contact wrote the note. An agent never edits
    /// the customer's own words, so this is one of the reasons a note carries
    /// no Edit control.
    #[serde(default)]
    created_by_contact_id: Option<uuid::Uuid>,
    created_at: DateTime<Utc>,
    /// PMS-931: when it was last written. Equal to `created_at` for a note
    /// nobody has edited, because both come from the same transaction's `NOW()`,
    /// so "edited" is a strict `>`. Optional so a client running against a
    /// server that predates PMS-931 decodes rather than failing the whole list.
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
}

/// MAPPS-593: whether this viewer may edit this note.
///
/// Mirrors `TicketService::update_note`'s two gates so the affordance and the
/// answer agree; a control that 403s or 409s is worse than no control. The
/// server is the authority and its refusal is still handled, because these
/// rules can only be enforced there.
///
/// The state half: a note the customer wrote through the portal is never an
/// agent's to edit, an emailed public note is frozen because the customer holds
/// the original in their inbox, and a `time_entry` note is edited through its
/// time entry. The permission half: the author, or an admin.
fn note_is_editable(note: &RemoteNote, viewer: Option<uuid::Uuid>, viewer_is_admin: bool) -> bool {
    if note.created_by_contact_id.is_some() {
        return false;
    }
    let kind_allows = match note.note_type.as_str() {
        "internal" | "resolution" => true,
        "public" => !note.is_email_sent,
        // `time_entry`, and anything a future server adds. Unknown means no:
        // guessing wrong here offers a control that cannot work.
        _ => false,
    };
    if !kind_allows {
        return false;
    }
    viewer_is_admin || (viewer.is_some() && viewer == note.created_by_id)
}

/// One change-history entry (`GET /audit-log/entity/tickets/:id`, PMS-182).
/// `changed_fields` is the set of columns the edit touched; `changes` carries
/// their before/after values (PMS-204).
#[derive(Clone, Debug, Deserialize)]
struct HistoryEntry {
    #[serde(default)]
    action: String,
    #[serde(default)]
    user_id: Option<uuid::Uuid>,
    #[serde(default)]
    changed_fields: Vec<String>,
    #[serde(default)]
    changes: Vec<FieldChange>,
    timestamp: DateTime<Utc>,
}

/// The before/after value of one changed column (PMS-204).
#[derive(Clone, Debug, Deserialize)]
struct FieldChange {
    #[serde(default)]
    field: String,
    #[serde(default)]
    old: Option<serde_json::Value>,
    #[serde(default)]
    new: Option<serde_json::Value>,
}

/// User option for resolving history actor ids to names (`/auth/users`).
#[derive(Clone, Debug, Deserialize)]
struct UserOpt {
    id: uuid::Uuid,
    #[serde(default)]
    full_name: String,
}

/// A time entry (`GET /time-entries?ticket_id=:id`), summed into Time Logged.
#[derive(Clone, Debug, Deserialize)]
struct RemoteTimeEntry {
    date: NaiveDate,
    // `i32` to match the server `TimeEntryResponse.duration_minutes`
    // (mokosh-types::time_tracking, MAPPS-138). Was `i64`; harmless over
    // JSON but the types disagreed.
    #[serde(default)]
    duration_minutes: i32,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    is_billable: bool,
    // MAPPS-517: the journal orders every source on one clock, so it needs the
    // entry's creation instant rather than the work date, and the author to
    // attribute the line. Both are on `TimeEntryResponse`; `Option` because the
    // page decoded neither before and a missing field must not blank the list.
    #[serde(default)]
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    user_id: Option<uuid::Uuid>,
}

/// Mirror of the server `SlaStatus` enum (snake_case wire form). Defaults
/// to `NotApplicable` so a ticket with no SLA configured still decodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SlaStatus {
    OnTrack,
    Warning,
    Breached,
    #[default]
    NotApplicable,
}

impl SlaStatus {
    /// (badge variant, label) for rendering. `NotApplicable` returns
    /// `None` so the caller can skip the badge entirely.
    fn badge(self) -> Option<(BadgeVariant, &'static str)> {
        match self {
            SlaStatus::OnTrack => Some((BadgeVariant::Green, "On Track")),
            SlaStatus::Warning => Some((BadgeVariant::Yellow, "At Risk")),
            SlaStatus::Breached => Some((BadgeVariant::Red, "Breached")),
            SlaStatus::NotApplicable => None,
        }
    }
}

/// Format an SLA due date as an absolute timestamp plus a coarse
/// remaining/overdue hint, e.g. "Jan 15, 2025 5:00 PM (2 hours left)".
/// PMS-253: honours the per-user format pref for the absolute part.
fn format_sla_due(due: DateTime<Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    let absolute = match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(due, Some(fmt)),
        None => due.format("%b %-d, %Y %-I:%M %p").to_string(),
    };
    let now = Utc::now();
    let delta = due.signed_duration_since(now);
    let secs = delta.num_seconds();
    let hint = if secs <= 0 {
        let overdue = (-secs).max(0);
        if overdue < 3600 {
            format!("{} min overdue", (overdue / 60).max(1))
        } else if overdue < 86_400 {
            format!("{} hr overdue", overdue / 3600)
        } else {
            format!("{} days overdue", overdue / 86_400)
        }
    } else if secs < 3600 {
        format!("{} min left", (secs / 60).max(1))
    } else if secs < 86_400 {
        format!("{} hr left", secs / 3600)
    } else {
        format!("{} days left", secs / 86_400)
    };
    format!("{absolute} ({hint})")
}

/// Render a `DateTime<Utc>` as a coarse "X ago" string. Good enough
/// for a list view where exact times live on the detail page.
fn relative_time(when: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(when);
    let secs = delta.num_seconds();
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        let hours = secs / 3600;
        if hours == 1 {
            "1 hour ago".into()
        } else {
            format!("{hours} hours ago")
        }
    } else {
        let days = secs / 86_400;
        if days == 1 {
            "1 day ago".into()
        } else {
            format!("{days} days ago")
        }
    }
}

/// Convert the lowercase status name the server returns into the
/// title-case label `TicketRow` keys its badge color on. Unknown
/// values pass through so future statuses don't disappear.
fn humanize_ticket_status(raw: &str) -> String {
    match raw {
        "" => "Open".into(),
        "open" => "Open".into(),
        "in_progress" | "in progress" => "In Progress".into(),
        "pending" => "Pending".into(),
        "resolved" => "Resolved".into(),
        "closed" => "Closed".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn humanize_priority(raw: &str) -> String {
    match raw {
        "" => "Medium".into(),
        "critical" => "Critical".into(),
        "high" => "High".into(),
        "medium" => "Medium".into(),
        "low" => "Low".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Absolute timestamp for created / activity lines, e.g. "Jun 05, 2026 14:30".
/// PMS-253: honours the per-user format pref when set.
fn fmt_datetime(dt: DateTime<Utc>) -> String {
    let pref = crate::utils::datetime::user_format_pref();
    match pref.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(fmt) => crate::utils::datetime::format_user_datetime(dt, Some(fmt)),
        None => dt.format("%b %d, %Y %H:%M").to_string(),
    }
}

/// Resolve a history actor id to a display name; "-" when unknown so the
/// change-history feed never shows a bare UUID (PMS-182).
fn actor_name(users: &[UserOpt], id: &Option<uuid::Uuid>) -> String {
    match id {
        Some(uid) => users
            .iter()
            .find(|u| &u.id == uid)
            .map(|u| u.full_name.clone())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "-".to_string()),
        None => "-".to_string(),
    }
}

/// MAPPS-517: one line of the ticket journal, whatever source it came from.
///
/// The journal replaced an Activity card that rendered notes alone, so a
/// reader could not tell from the record that a ticket had been reassigned,
/// changed state or had time logged against it.
#[derive(Clone, Debug, PartialEq)]
struct JournalEntry {
    at: DateTime<Utc>,
    /// Display name of whoever did it, or "Someone" when unresolvable.
    who: String,
    /// Predicate, rendered after the name: "added an internal note".
    action: String,
    /// Body block under the headline: the note text or a time entry's notes.
    /// `None` renders the headline alone.
    ///
    /// MAPPS-596: an edit's before/after is NOT here. It used to be, flattened
    /// into this string, which is how a description edit came to print two
    /// 160-character values into the middle of the journal. It carries its own
    /// field below instead, so it can collapse when it is large while a note
    /// stays visible: a note's text is the entry, not metadata about it.
    body: Option<String>,
    /// The before/after lines when this entry came from the audit log. Empty
    /// for a note or a time entry.
    changes: Vec<ChangeLine>,
    /// MAPPS-593: the note this entry came from, when it is one THIS viewer may
    /// edit. `None` on a change-history line, a time entry, or a note the
    /// viewer may not edit, and that is what decides whether the Edit control
    /// is rendered at all.
    editable_note: Option<uuid::Uuid>,
    /// MAPPS-593: the note has been edited since it was written. Marked on
    /// screen, because an unmarked edit means the reader cannot tell that the
    /// text in front of them is not what was written.
    edited: bool,
}

/// The name to attribute a journal line to. `actor_name` yields "-" for an
/// id no `/auth/users` row matches, which reads as a broken row in a sentence.
fn journal_actor(users: &[UserOpt], id: &Option<uuid::Uuid>) -> String {
    let name = actor_name(users, id);
    if name == "-" {
        "Someone".to_string()
    } else {
        name
    }
}

/// Headline for an audit-log entry. A single-column edit gets the phrasing a
/// reader expects for that column ("changed the status"); anything wider falls
/// back to naming the columns, as the change-history pane always did.
fn history_action(entry: &HistoryEntry) -> String {
    match entry.action.as_str() {
        "create" => return "created the ticket".to_string(),
        "delete" => return "deleted the ticket".to_string(),
        _ => {}
    }
    match entry.changed_fields.as_slice() {
        [] => format!("{} the ticket", action_label(&entry.action).to_lowercase()),
        [one] => match one.as_str() {
            "status_id" => "changed the status".to_string(),
            "assigned_to_id" => "changed the assignee".to_string(),
            other => format!("updated {}", title_field(other)),
        },
        many => format!("updated {}", fields_label(many)),
    }
}

/// The before/after lines for an audit entry. Empty when every change on it is
/// a bare reference (a FK swap the audit log records as two UUIDs), which
/// `ChangeLine::build` drops because it reads as "(reference) → (reference)".
fn history_changes(entry: &HistoryEntry) -> Vec<ChangeLine> {
    entry
        .changes
        .iter()
        .filter_map(|c| ChangeLine::build(&c.field, &c.old, &c.new))
        .collect()
}

/// MAPPS-517: merge every source this page already fetches into one
/// newest-first stream: notes, the ticket's audit log (state and assignment
/// changes among them) and its time entries.
///
/// There is no single server activity feed for a ticket, so this is assembled
/// here. It degrades rather than empties: a source that returns nothing (the
/// audit log 404s, no time is logged) simply contributes no lines, leaving the
/// notes-only stream the Activity card used to show. What it cannot include is
/// anything no source records - attachments and approvals write no audit row
/// against the ticket - which is why the journal says so on screen instead of
/// reading as complete.
fn build_journal(
    notes: &[RemoteNote],
    history: &[HistoryEntry],
    time_entries: &[RemoteTimeEntry],
    users: &[UserOpt],
    viewer: Option<uuid::Uuid>,
    viewer_is_admin: bool,
) -> Vec<JournalEntry> {
    let mut entries: Vec<JournalEntry> =
        Vec::with_capacity(notes.len() + history.len() + time_entries.len());

    for n in notes {
        // The email only ever goes out for a public note, and only when the
        // composer's toggle was on AND the ticket's contact has an address
        // (mokosh-server `send_note_email`). `is_email_sent` is the outcome,
        // so the line states what happened, not what was requested.
        let action = if n.note_type == "internal" {
            "added an internal note".to_string()
        } else if n.is_email_sent {
            "added a public note and emailed the client".to_string()
        } else {
            "added a public note (not emailed)".to_string()
        };
        entries.push(JournalEntry {
            at: n.created_at,
            who: if n.created_by_name.trim().is_empty() {
                "Someone".to_string()
            } else {
                n.created_by_name.clone()
            },
            action,
            body: (!n.content.trim().is_empty()).then(|| n.content.clone()),
            changes: Vec::new(),
            editable_note: note_is_editable(n, viewer, viewer_is_admin).then_some(n.id),
            // Strictly greater: both timestamps come from the same
            // transaction's `NOW()` on insert, so an unedited note has them
            // exactly equal.
            edited: n.updated_at.is_some_and(|u| u > n.created_at),
        });
    }

    for h in history {
        entries.push(JournalEntry {
            at: h.timestamp,
            who: journal_actor(users, &h.user_id),
            action: history_action(h),
            body: None,
            changes: history_changes(h),
            editable_note: None,
            edited: false,
        });
    }

    for e in time_entries {
        // `created_at` is when the entry was written, which is the journal's
        // clock; the work date is in the headline. Older rows decoded without
        // it fall back to midnight on the work date rather than dropping out.
        let at = e.created_at.unwrap_or_else(|| {
            DateTime::<Utc>::from_naive_utc_and_offset(e.date.and_time(chrono::NaiveTime::MIN), Utc)
        });
        let billable = if e.is_billable { " (billable)" } else { "" };
        entries.push(JournalEntry {
            at,
            who: journal_actor(users, &e.user_id),
            action: format!("logged {} min on {}{billable}", e.duration_minutes, e.date),
            body: e.notes.clone().filter(|s| !s.trim().is_empty()),
            changes: Vec::new(),
            editable_note: None,
            edited: false,
        });
    }

    // Newest first, matching the ordering the Activity card had. `Reverse` on a
    // key rather than a flipped comparator: `DateTime<Utc>` is `Copy`, so the
    // key costs nothing, and `clippy::unnecessary_sort_by` rejects the
    // comparator form. Both are stable sorts, so entries sharing a timestamp
    // keep the order they were pushed in.
    entries.sort_by_key(|e| std::cmp::Reverse(e.at));
    entries
}

/// MAPPS-289: sortable columns on the ticket list. Mirrors the
/// `ContactSortKey` pattern in `contacts.rs`: an enum tracks which
/// column is active, paired with a `SortDirection`, and the table
/// reorders client-side over the already-filtered set.
#[derive(Clone, Copy, PartialEq)]
enum TicketSortKey {
    Ticket,
    Company,
    Status,
    Priority,
    Assigned,
    Updated,
}

/// MAPPS-546: the server's name for each sortable column.
///
/// These are the keys PMS-894 put on `list_ticket_responses`' allow-list. They
/// are deliberately not SQL: the server maps them to expressions, so this
/// client cannot name a column even by accident.
///
/// PMS-897: every value here is asserted against `mokosh_types::sort::TICKETS`
/// by the test below. `scripts/check-sort-keys.sh` cannot see this function -
/// it forbids a hardcoded `sort=` LITERAL, and these reach the query string
/// through interpolation - so until that test existed, six sort keys went to
/// the server unchecked against any allow-list. Since MAPPS-533 an unlisted one
/// is a 422, so a drift here breaks the ticket list rather than reordering it
/// quietly.
fn ticket_sort_param(key: TicketSortKey) -> &'static str {
    match key {
        TicketSortKey::Ticket => "ticket_number",
        TicketSortKey::Company => "company_name",
        TicketSortKey::Status => "status",
        TicketSortKey::Priority => "priority",
        TicketSortKey::Assigned => "assigned_to_name",
        TicketSortKey::Updated => "updated_at",
    }
}

#[cfg(test)]
mod pms897_sort_tests {
    use super::{ticket_sort_param, TicketSortKey};

    /// Every key this page can send is one the server accepts.
    ///
    /// The enum is matched exhaustively rather than iterated, so adding a
    /// variant fails to compile here until someone decides what the server
    /// calls it - which is the check `check-sort-keys.sh` structurally cannot
    /// make, since these values never appear as literals in a query string.
    #[test]
    fn every_sort_key_this_page_sends_is_one_the_server_accepts() {
        let all = [
            TicketSortKey::Ticket,
            TicketSortKey::Company,
            TicketSortKey::Status,
            TicketSortKey::Priority,
            TicketSortKey::Assigned,
            TicketSortKey::Updated,
        ];
        for key in all {
            let param = ticket_sort_param(key);
            assert!(
                mokosh_types::sort::TICKETS.contains(&param),
                "`{param}` is not in the server's ticket sort allow-list; it would 422"
            );
        }
    }

    /// The other direction: a key the server offers that no control sends is a
    /// sort the user cannot reach. Not a failure, but worth knowing about, so
    /// the assertion is on the count and the message names what is unused.
    #[test]
    fn the_page_offers_every_column_the_server_can_sort_by() {
        let sent: Vec<&str> = [
            TicketSortKey::Ticket,
            TicketSortKey::Company,
            TicketSortKey::Status,
            TicketSortKey::Priority,
            TicketSortKey::Assigned,
            TicketSortKey::Updated,
        ]
        .into_iter()
        .map(ticket_sort_param)
        .collect();

        let unreachable: Vec<&&str> = mokosh_types::sort::TICKETS
            .iter()
            .filter(|k| !sent.contains(k))
            .collect();

        // `created_at` and `sla_due_date` are accepted by the server and have
        // no column header on this page. That is a deliberate gap, not drift:
        // the table shows neither.
        assert_eq!(
            unreachable,
            vec![&"created_at", &"sla_due_date"],
            "the set of server sort keys with no control on this page changed"
        );
    }
}

fn ticket_sort_dir_for(
    current: &Option<(TicketSortKey, SortDirection)>,
    key: TicketSortKey,
) -> Option<SortDirection> {
    current.and_then(|(k, dir)| if k == key { Some(dir) } else { None })
}

/// MAPPS-546: takes the page signal too, and resets it. The sort is applied by
/// the server now, so re-sorting while on page 3 would otherwise hand back
/// page 3 of a different ordering - a jump to unrelated rows that reads as a
/// bug. `contacts.rs::toggle_sort` has taken the page for the same reason since
/// it was paged.
fn toggle_ticket_sort(
    current: &mut Signal<Option<(TicketSortKey, SortDirection)>>,
    key: TicketSortKey,
    page: &mut Signal<usize>,
) {
    let next = match *current.read() {
        Some((k, SortDirection::Ascending)) if k == key => Some((key, SortDirection::Descending)),
        Some((k, SortDirection::Descending)) if k == key => Some((key, SortDirection::Ascending)),
        _ => Some((key, SortDirection::Ascending)),
    };
    current.set(next);
    page.set(1);
}

/// Ticket list page
#[component]
pub fn TicketListPage() -> Element {
    use_page_title("Tickets");
    let mut search = use_signal(String::new);
    let mut page = use_signal(|| 1usize);
    let mut status_filter = use_signal(String::new);
    let mut priority_filter = use_signal(String::new);
    // MAPPS-289: sortable-column state. Sorting here is entirely client-side
    // over the fetched page; the list query sends no `?sort=` at all.
    let mut sort = use_signal(|| Some((TicketSortKey::Updated, SortDirection::Descending)));
    // MAPPS-290: page-scoped bulk selection. The header `SelectAllHeader`
    // toggles every visible row in/out; per-row `SelectRowCell` toggles
    // single rows; the `BulkActionsBar` renders the verb buttons when
    // non-empty and clears itself when a verb fires.
    let mut selection = use_bulk_selection();
    // MAPPS-310: confirm-before-delete for the bulk delete action.
    // `None` = no dialog open; `Some(snapshot)` = dialog open against
    // the snapshotted selection (we freeze the id list at click time
    // so a user un-checking a row mid-dialog doesn't smuggle past the
    // confirmation prompt). Other destructive surfaces in the app
    // (Companies / Contracts / Assets detail) gate on `ConfirmDialog`;
    // the bulk path bypassed it before this fix.
    let mut bulk_delete_confirm = use_signal::<Option<Vec<String>>>(|| None);
    let mut bulk_delete_running = use_signal(|| false);

    // MAPPS-295: source the status-filter options from the tenant's
    // configured `ticket_statuses` table instead of a hand-rolled slug
    // list. The list previously hardcoded `new/open/in_progress/pending/
    // resolved/closed`, while the ticket-detail inline-edit dropdown
    // already fetches `/tickets/statuses` (PMS-359) - so a tenant whose
    // workflow includes "Waiting on Client / Waiting on Vendor /
    // Scheduled" saw those on the detail page but couldn't filter by
    // them on the list, and the list offered a "Pending" the records
    // never carried. Source-of-truth is whichever set the server hands
    // back. Empty fetch (offline / 403) falls back to no options so the
    // filter just renders the "All Statuses" placeholder, which is
    // strictly better than fabricating slugs.
    let status_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketStatus>("/tickets/statuses")
            .await
            .ok()
            .unwrap_or_default()
    });
    // MAPPS-546: the tenant's own priorities, for the same reason as the
    // statuses above - the filter sends an id, and the previous hardcoded
    // critical/high/medium/low list offered values a renamed priority would
    // never match.
    let priority_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketPriority>("/tickets/priorities")
            .await
            .ok()
            .unwrap_or_default()
    });

    // MAPPS-438: `None` is a failed load, exactly like the other list pages.
    // The page renders only what the backend returned.
    // MAPPS-249: a company context card's "View All" lands here with
    // `?company_id=<uuid>`. When present, scope the fetch to that company so the
    // list shows only its tickets and every row stays inside the same company.
    // MAPPS-546: one page at a time, with every filter and the sort sent to the
    // server. MAPPS-543 had this fetching every ticket in the tenant so the
    // browser could filter and sort them, which was correct and unbounded on
    // the busiest page in the product.
    //
    // The search reaches company and assignee because PMS-894 widened `q` to
    // match them; before that, moving this filter server-side would have
    // stopped finding tickets by client name, which is how most people look for
    // one. The sort keys are the ones PMS-894 added to the allow-list - and
    // note what `order_by` does with a key that is NOT on it: it drops it and
    // sorts by the default, returning a 200. A page that looks sorted and is
    // not is why those had to land first.
    //
    // Every reactive input is read INSIDE the closure so the resource
    // subscribes to it (MAPPS-148).
    let mut tickets_resource = use_resource(move || {
        let q = search.read().trim().to_string();
        let status_id = status_filter.read().clone();
        let priority_id = priority_filter.read().clone();
        let sort_snapshot = *sort.read();
        let current_page = (*page.read()).max(1);
        async move {
            // MAPPS-357: subscribe to reachability so the list auto-refetches the
            // instant the server returns, and so a failed load stays distinguishable
            // from an empty one.
            let _reachable = crate::hooks::use_server_reachable();
            let token = crate::hooks::fetch::api::current_access_token()?;
            let mut path = format!("/tickets?page={current_page}&per_page={PER_PAGE}");
            if let Some(company_id) = crate::utils::url::current_query_param("company_id") {
                path.push_str(&format!("&company_id={company_id}"));
            }
            if !q.is_empty() {
                path.push_str(&format!(
                    "&q={}",
                    crate::utils::url::encode_uri_component(&q)
                ));
            }
            if !status_id.is_empty() {
                path.push_str(&format!("&status_id={status_id}"));
            }
            if !priority_id.is_empty() {
                path.push_str(&format!("&priority_id={priority_id}"));
            }
            if let Some((key, dir)) = sort_snapshot {
                path.push_str(&format!(
                    "&sort={}&sort_dir={}",
                    ticket_sort_param(key),
                    match dir {
                        SortDirection::Ascending => "asc",
                        SortDirection::Descending => "desc",
                    }
                ));
            }
            crate::hooks::fetch::api::get_with_auth::<Paginated<RemoteTicket>>(&path, &token)
                .await
                .ok()
        }
    });

    let resource_snapshot = tickets_resource.read_unchecked();
    let is_loading = resource_snapshot.is_none();
    let fetch_failed = matches!(*resource_snapshot, Some(None));
    let (remote_tickets, total_matches): (Vec<RemoteTicket>, usize) = match &*resource_snapshot {
        Some(Some(envelope)) => (envelope.data.clone(), envelope.meta.total as usize),
        _ => (Vec::new(), 0),
    };

    // MAPPS-357: the ticket list is this page's PRIMARY resource. A failed load
    // while the server is flagged down is an outage, not an empty list, so
    // render the honest unavailable state (which keeps the nav + banner) instead
    // of an empty table. A fetch that fails while the server is still reachable
    // (a 4xx) keeps the inline error banner below. Writes are blocked while
    // down; `can_mutate` disables the bulk-delete control.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Tickets".to_string() }
        };
    }

    // MAPPS-546: the search, the status and priority filters and the sort are
    // all the server's now, so these rows ARE the matches for this page, in
    // order. `total_matches` is the server's count of every match, not of the
    // rows on screen.
    let filtered_tickets: Vec<RemoteTicket> = remote_tickets;
    // MAPPS-546: which empty state to show used to be decided by comparing the
    // filtered rows with the unfiltered ones. Server-side, those are the same
    // list, so the question "is this empty because the tenant has no tickets,
    // or because the filters exclude them?" is answered by whether any filter
    // is set.
    let filters_active = !search.read().trim().is_empty()
        || !status_filter.read().is_empty()
        || !priority_filter.read().is_empty();

    // MAPPS-295: build the Status filter options from the tenant's actual
    // status set. A still-loading or empty resource just shows the "All
    // Statuses" placeholder option.
    let tenant_statuses = status_resource.read_unchecked().clone().unwrap_or_default();
    // MAPPS-546: the option VALUE is the id, because the server filters on
    // `status_id`. The label stays the tenant's own name.
    let mut status_options = vec![SelectOption::new("", "All Statuses")];
    for s in tenant_statuses.iter() {
        status_options.push(SelectOption::new(s.id.to_string(), s.name.clone()));
    }

    // MAPPS-546: built from the tenant's own priorities, not from a hardcoded
    // four-level slug list. The server filters on `priority_id`, so an id is
    // needed anyway - and the hardcoded set was wrong for any tenant that had
    // renamed or added a priority, silently offering filters that matched
    // nothing.
    let tenant_priorities = priority_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let mut priority_options = vec![SelectOption::new("", "All Priorities")];
    for p in tenant_priorities.iter() {
        priority_options.push(SelectOption::new(
            p.id.to_string(),
            humanize_priority(&p.name),
        ));
    }

    rsx! {
        PageHeader {
            title: "Tickets",
            subtitle: "Manage support tickets and service requests",
            actions: rsx! {
                Link {
                    to: Route::TicketNew {},
                    Button {
                        variant: ButtonVariant::Primary,
                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "New Ticket"
                    }
                }
            },
        }

        // MAPPS-321: surface the active company scope when the user
        // arrived via a "View all" link from CompanyDetail. The
        // fetch above already narrows the list to ?company_id=;
        // without this chip the user has no signal that the list
        // is already scoped (the header reads a plain "Tickets").
        crate::components::ContextFilterBanner {
            scope: crate::components::ContextFilterScope::Tickets,
        }

        // Filters
        Card { class: "mb-6",
            div { class: "flex flex-col sm:flex-row gap-4",
                div { class: "flex-1",
                    SearchInput {
                        value: search.read().clone(),
                        placeholder: "Search tickets…",
                        oninput: move |e: FormEvent| search.set(e.value()),
                    }
                }
                div { class: "flex gap-4",
                    Select {
                        name: "status",
                        options: status_options,
                        value: status_filter.read().clone(),
                        placeholder: "Status",
                        onchange: move |e: FormEvent| {
                            status_filter.set(e.value());
                            page.set(1);
                        },
                    }
                    Select {
                        name: "priority",
                        options: priority_options,
                        value: priority_filter.read().clone(),
                        placeholder: "Priority",
                        onchange: move |e: FormEvent| {
                            priority_filter.set(e.value());
                            page.set(1);
                        },
                    }
                }
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load tickets. Refresh the page to retry." }
        }

        // MAPPS-290: bulk actions bar. Renders only when at least one
        // row is selected. The Delete verb issues parallel DELETE
        // calls and clears the selection on completion. Adding more
        // verbs (bulk assign, set priority) follows the same shape.
        BulkActionsBar {
            selection,
            label: "ticket".to_string(),
            Button {
                variant: ButtonVariant::Danger,
                // MAPPS-357: block bulk delete while the server is unreachable.
                disabled: bulk_delete_running() || !can_mutate,
                title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                onclick: move |_| {
                    // MAPPS-310: stash the snapshot + open the
                    // confirmation dialog. The actual delete fanout
                    // runs from the dialog's confirm handler so an
                    // accidental click is recoverable.
                    let ids: Vec<String> = selection.read().iter().cloned().collect();
                    if !ids.is_empty() {
                        bulk_delete_confirm.set(Some(ids));
                    }
                },
                "Delete selected"
            }
        }

        // MAPPS-310: confirmation dialog for the bulk delete. The
        // pending-id list lives in `bulk_delete_confirm`; the
        // onconfirm handler runs the same join_all delete fanout
        // the inline onclick used before this fix, then clears
        // the selection and restarts the resource.
        {
            let pending = bulk_delete_confirm.read().clone();
            let pending_count = pending.as_ref().map(|v| v.len()).unwrap_or(0);
            let dialog_message = format!(
                "Delete {pending_count} selected ticket(s)? Notes, attachments, and time entries on these tickets are also removed. This cannot be undone."
            );
            let confirm_text = format!("Delete {pending_count} ticket(s)");
            rsx! {
                crate::components::ConfirmDialog {
                    open: pending.is_some(),
                    title: "Delete selected tickets".to_string(),
                    message: dialog_message,
                    confirm_text,
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: bulk_delete_running(),
                    onconfirm: move |_| {
                        let Some(ids) = bulk_delete_confirm.read().clone() else { return };
                        if ids.is_empty() || bulk_delete_running() { return; }
                        bulk_delete_running.set(true);
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                use futures_util::future::join_all;
                                let futs = ids.iter().map(|id| {
                                    let path = format!("/tickets/{id}");
                                    async move {
                                        crate::hooks::fetch::api::delete_authed(&path).await
                                    }
                                });
                                let results = join_all(futs).await;
                                let failures = results.iter().filter(|r| r.is_err()).count();
                                if failures == 0 {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Success,
                                        format!("Deleted {} ticket(s).", ids.len()),
                                    );
                                } else {
                                    crate::hooks::toast::push_toast(
                                        crate::components::AlertType::Error,
                                        format!("Deleted {} of {}; {} failed.", ids.len() - failures, ids.len(), failures),
                                    );
                                }
                            }
                            clear_selection(&mut selection);
                            tickets_resource.restart();
                            bulk_delete_confirm.set(None);
                            bulk_delete_running.set(false);
                        });
                    },
                    oncancel: move |_| {
                        if !bulk_delete_running() {
                            bulk_delete_confirm.set(None);
                        }
                    },
                }
            }
        }

        // Ticket table
        DataTable {
            loading: is_loading,
            total_items: total_matches,
            current_page: (*page.read()).max(1),
            per_page: PER_PAGE,
            onpagechange: move |p| page.set(p),
            columns: 6,
            Table {
                striped: true,
                TableHead {
                    TableRow {
                        // MAPPS-290: select-all checkbox for the visible page.
                        SelectAllHeader {
                            selection,
                            ids: filtered_tickets.iter().map(|t| t.id.to_string()).collect::<Vec<_>>(),
                        }
                        {
                            let sort_snap = *sort.read();
                            rsx! {
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Ticket),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Ticket, &mut page),
                                    "Ticket"
                                }
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Company),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Company, &mut page),
                                    "Company"
                                }
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Status),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Status, &mut page),
                                    "Status"
                                }
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Priority),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Priority, &mut page),
                                    "Priority"
                                }
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Assigned),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Assigned, &mut page),
                                    "Assigned To"
                                }
                                TableHeader {
                                    sortable: true,
                                    sort_direction: ticket_sort_dir_for(&sort_snap, TicketSortKey::Updated),
                                    onsort: move |_| toggle_ticket_sort(&mut sort, TicketSortKey::Updated, &mut page),
                                    "Updated"
                                }
                            }
                        }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 5 }
                } else if filtered_tickets.is_empty() {
                    if !filters_active {
                        // PMS-354: helpful empty state with a primary CTA,
                        // matching the Contracts reference pattern.
                        TableEmpty {
                            columns: 6,
                            title: "No tickets yet".to_string(),
                            description: "Create your first ticket to start tracking support work."
                                .to_string(),
                            actions: rsx! {
                                Link {
                                    to: Route::TicketNew {},
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                        "New Ticket"
                                    }
                                }
                            },
                        }
                    } else {
                        // Filtered to nothing: MAPPS-291 adds a one-click
                        // "Clear filters" affordance so the user does not
                        // have to find every filter control and reset
                        // each one to recover. Resets the three signals
                        // the toolbar above mounts.
                        TableEmpty {
                            columns: 6,
                            title: "No tickets match your filters".to_string(),
                            description: "Adjust the filters above, or clear them to see every ticket again.".to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        search.set(String::new());
                                        status_filter.set(String::new());
                                        priority_filter.set(String::new());
                                    },
                                    "Clear filters"
                                }
                            },
                        }
                    }
                } else {
                    TableBody {
                        for ticket in filtered_tickets.iter().cloned() {
                            TicketRow {
                                key: "{ticket.id}",
                                id: ticket.id.to_string(),
                                number: ticket.ticket_number,
                                title: ticket.title,
                                company: ticket.company_name,
                                status: humanize_ticket_status(&ticket.status.name),
                                priority: humanize_priority(&ticket.priority.name),
                                assigned_to: ticket.assigned_to_name.unwrap_or_else(|| "Unassigned".to_string()),
                                updated: relative_time(ticket.updated_at),
                                updated_iso: ticket.updated_at.to_rfc3339(),
                                updated_title: fmt_datetime(ticket.updated_at),
                                // MAPPS-290: hand the page-scoped
                                // selection signal down so each
                                // row's first cell renders a
                                // checkbox bound to it.
                                selection,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TicketRowProps {
    id: String,
    number: String,
    title: String,
    company: String,
    status: String,
    priority: String,
    assigned_to: String,
    updated: String,
    /// MAPPS-409: RFC3339 form of `updated_at` for the `<time datetime>`
    /// wrapper, plus the absolute-time string for its hover title.
    updated_iso: String,
    updated_title: String,
    /// MAPPS-290: the page-scoped bulk-selection signal the row's first
    /// cell binds its checkbox to.
    selection: BulkSelection,
}

#[component]
fn TicketRow(props: TicketRowProps) -> Element {
    let status_variant = ticket_status_badge(&props.status);

    let priority_variant = match props.priority.as_str() {
        "Critical" => BadgeVariant::Red,
        "High" => BadgeVariant::Red,
        "Medium" => BadgeVariant::Yellow,
        "Low" => BadgeVariant::Green,
        _ => BadgeVariant::Gray,
    };

    let navigator = use_navigator();
    let id = props.id.clone();

    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| { navigator.push(Route::TicketDetail { id: id.clone() }); },
            // MAPPS-290: per-row checkbox in the first column. The cell
            // stops propagation so toggling the checkbox doesn't also
            // navigate to the detail page.
            SelectRowCell { selection: props.selection, id: props.id.clone() }
            TableCell {
                div {
                    Link {
                        to: Route::TicketDetail { id: props.id.clone() },
                        class: "font-medium text-accent hover:opacity-90",
                        "{props.number}"
                    }
                    p { class: "text-muted text-sm truncate max-w-xs", "{props.title}" }
                }
            }
            TableCell { "{props.company}" }
            TableCell {
                Badge { variant: status_variant, "{props.status}" }
            }
            TableCell {
                Badge { variant: priority_variant, "{props.priority}" }
            }
            TableCell {
                if props.assigned_to == "Unassigned" {
                    span { class: "text-subtle italic", "Unassigned" }
                } else {
                    span { "{props.assigned_to}" }
                }
            }
            TableCell { class: "text-muted",
                time {
                    datetime: "{props.updated_iso}",
                    title: "{props.updated_title}",
                    "{props.updated}"
                }
            }
        }
    }
}

/// MAPPS-207: optional company prefill carried on the New Ticket URL
/// (`/tickets/new?company_id=<uuid>&company_name=<name>`). The company
/// detail "New Ticket" affordance links here so the form opens with the
/// company already selected.
#[derive(Clone, Debug, Default, PartialEq)]
struct CompanyPrefill {
    id: String,
    name: String,
}

fn read_company_prefill_from_url() -> CompanyPrefill {
    #[cfg(feature = "web")]
    {
        if let Some(search) = crate::platform::location::search() {
            {
                let params = crate::utils::url::QueryString::parse(&search);
                let id = params.get("company_id").unwrap_or_default();
                let name = params.get("company_name").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return CompanyPrefill { id, name };
                }
            }
        }
    }
    CompanyPrefill::default()
}

/// PMS-482: KB-article prefill captured off the URL when the
/// ticket-new page is reached via "Open ticket about this article"
/// from a KB article. Only the `id` is needed for the create body's
/// `source_kb_article_id`; `title` + `url` are folded into the
/// title and description signals so the user lands on a pre-filled
/// form they can immediately edit and submit.
#[derive(Clone, Debug, Default, PartialEq)]
struct KbArticlePrefill {
    id: String,
    title: String,
    url: String,
}

fn read_kb_prefill_from_url() -> KbArticlePrefill {
    #[cfg(feature = "web")]
    {
        if let Some(search) = crate::platform::location::search() {
            {
                let params = crate::utils::url::QueryString::parse(&search);
                let id = params.get("from_kb_article").unwrap_or_default();
                let title = params.get("from_kb_title").unwrap_or_default();
                let url = params.get("from_kb_url").unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_ok() {
                    return KbArticlePrefill { id, title, url };
                }
            }
        }
    }
    KbArticlePrefill::default()
}

/// New ticket page
///
/// MAPPS-357: this is a create form, so there is no primary *entity* resource
/// whose failure means "no content" - the only fetches (types / categories /
/// users / priorities) are SECONDARY dropdown lookups that keep degrading to a
/// default. So the page does not swap in `ContentUnavailable`; instead the form
/// stays mounted (the user can keep composing) and the Create submit is disabled
/// while the server is unreachable so a write cannot silently fail.
#[component]
pub fn TicketNewPage() -> Element {
    use_page_title("New Ticket");
    // MAPPS-207: seed the company from the URL when linked from a company.
    let prefill = use_signal(read_company_prefill_from_url);
    let prefill = prefill.read().clone();
    // PMS-482: seed title + description from the source KB article when
    // the user clicked "Open ticket about this article". The article id
    // rides into the create body as `source_kb_article_id`.
    let kb_prefill = use_signal(read_kb_prefill_from_url);
    let kb_prefill = kb_prefill.read().clone();
    let kb_article_id_for_body = kb_prefill.id.clone();

    let initial_title = kb_prefill.title.clone();
    let initial_description = if kb_prefill.title.is_empty() {
        String::new()
    } else {
        let link = if kb_prefill.url.is_empty() {
            String::new()
        } else {
            format!("\nLink: {}", kb_prefill.url)
        };
        format!("Article: {}{}", kb_prefill.title, link)
    };
    let mut title = use_signal(|| initial_title);
    let mut description = use_signal(|| initial_description);
    // The company field holds a real company UUID (string) plus its human
    // name, both fed by the CompanyPicker. The old hardcoded "1"/"2"/"3"
    // Select submitted non-UUID ids that fell back to the nil UUID, so the
    // create always failed against the server (MAPPS-122).
    let mut company_id = use_signal(|| prefill.id.clone());
    let mut company_name = use_signal(|| prefill.name.clone());
    // MAPPS-207: optional contact association. Scoped to the selected
    // company via the ContactPicker's `company_filter`.
    let mut contact_id = use_signal(String::new);
    let mut contact_name = use_signal(String::new);
    // PMS-358: priority is a tenant-scoped lookup whose IDs are UUIDs. The
    // signal stores the selected UUID as a string (empty = "use tenant
    // default"). The hardcoded ["critical", "high", "medium", "low"]
    // string options the form used previously never landed in the request
    // body at all (the submit handler did not read the signal), and even if
    // they had they would not match the server's `priority_id: Option<Uuid>`
    // contract. Fetch the tenant's priorities on mount, default to the row
    // flagged `is_default = true`, and bind the Select's value to the UUID.
    let mut priority_id = use_signal(String::new);
    // PMS-344: optional asset association. Empty signals = no asset.
    // Both id and human name are tracked so the AssetPicker can render
    // its "selected chip" state without an extra fetch.
    let mut asset_id = use_signal(String::new);
    let mut asset_name = use_signal(String::new);
    // MAPPS-296: capture every common field at create time instead of
    // forcing the user to follow up with an edit. Type and Category are
    // tenant-scoped lookups (`/tickets/types` and `/tickets/categories`),
    // Assignee comes from `/auth/users` (already used by the calendar /
    // dispatch surfaces), and the Due Date stamps `scheduled_end` so
    // SLA / dispatch view it on creation.
    let mut type_id = use_signal(String::new);
    let mut category_id = use_signal(String::new);
    let mut assigned_to_id = use_signal(String::new);
    let mut due_date = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let mut error = use_signal(String::new);
    // Per-field server validation message routed next to the Title input
    // (MAPPS-210). Cleared on each submit and on edit.
    let mut title_error = use_signal(String::new);
    // PMS-518: per-field error for the now-enforced required Description.
    let mut description_error = use_signal(String::new);
    // MAPPS-592: who `@handle` completes against while the ticket is written.
    let mention_directory = crate::hooks::use_mention_directory(true);
    // MAPPS-322: per-field error for the required Company, routed into the
    // CompanyPicker's own inline slot instead of the form-level banner.
    let mut company_error = use_signal(String::new);

    // MAPPS-296: tenant lookups for Type + Category + Assignee. Each
    // hits its own endpoint and falls back to an empty list on a 403 /
    // network drop so the form mounts even when the lookups are
    // unavailable; the matching signal stays empty and the server
    // applies its defaults.
    let types_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketLookup>("/tickets/types")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: the server applies its default type.
                tracing::warn!("ticket-type lookup failed: {e}");
                Vec::new()
            })
    });
    let categories_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketLookup>("/tickets/categories")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: the server applies its default category.
                tracing::warn!("ticket-category lookup failed: {e}");
                Vec::new()
            })
    });
    let users_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_all_authed::<RemoteUserLookup>("/auth/users")
            .await
            .unwrap_or_else(|e| {
                // Best-effort: the assignee picker stays empty.
                tracing::warn!("assignee lookup failed: {e}");
                Vec::new()
            })
    });

    let type_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "(none)")];
        if let Some(rows) = types_resource.read().as_ref() {
            for r in rows.iter() {
                opts.push(SelectOption::new(r.id.to_string(), r.name.clone()));
            }
        }
        opts
    };
    let category_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "(none)")];
        if let Some(rows) = categories_resource.read().as_ref() {
            for r in rows.iter() {
                opts.push(SelectOption::new(r.id.to_string(), r.name.clone()));
            }
        }
        opts
    };
    let assignee_options: Vec<SelectOption> = {
        let mut opts = vec![SelectOption::new("", "Unassigned")];
        if let Some(rows) = users_resource.read().as_ref() {
            for u in rows.iter() {
                let name = if u.full_name.trim().is_empty() {
                    u.email.clone()
                } else {
                    u.full_name.clone()
                };
                opts.push(SelectOption::new(u.id.to_string(), name));
            }
        }
        opts
    };

    // Fetch the tenant's ticket priorities on mount. The Paginated envelope
    // matches the server's PaginatedResponse wire shape; meta is ignored
    // here (lookup tables are short enough to fit one page).
    let priorities_resource = use_resource(|| async move {
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketPriority>("/tickets/priorities")
            .await
            .unwrap_or_default()
    });

    // Once priorities load, seed the signal with the tenant's default row
    // (or the first row if none is flagged default). use_effect re-runs
    // every time the resource transitions to Ready, but a non-empty
    // priority_id signal short-circuits so an in-progress user selection
    // is never clobbered by a late-arriving fetch.
    use_effect(move || {
        if !priority_id.read().is_empty() {
            return;
        }
        if let Some(rows) = priorities_resource.read().as_ref() {
            let chosen = rows
                .iter()
                .find(|p| p.is_default)
                .or_else(|| rows.first())
                .map(|p| p.id.to_string())
                .unwrap_or_default();
            if !chosen.is_empty() {
                priority_id.set(chosen);
            }
        }
    });

    let priority_options: Vec<SelectOption> = match priorities_resource.read().as_ref() {
        Some(rows) => rows
            .iter()
            .map(|p| SelectOption::new(p.id.to_string(), p.name.clone()))
            .collect(),
        // While the fetch is in flight, render an empty single-option
        // placeholder so the Select component does not blow up; the
        // effect above will populate the real options on the next tick.
        None => vec![SelectOption::new("", "Loading…")],
    };

    let navigator = use_navigator();
    let handle_submit = move |e: FormEvent| {
        e.prevent_default();
        is_submitting.set(true);
        error.set(String::new());

        // PMS-518: validate every required field through the shared FormGuard,
        // so all failures surface at once (each in its own inline slot) and the
        // first invalid field is focused. Generalises the PMS-514 fix; each
        // `field` call also clears a stale message when the value now passes.
        //
        // MAPPS-281: Title/Description are trimmed so a whitespace-only value
        // does not satisfy the (inert) native `required` and reach the server.
        let mut guard = FormGuard::new();

        let title_v = title.read().trim().to_string();
        title_error.set(guard.field("title", &title_v, "Title", &[Rule::Required]));

        // PMS-518: Description is now enforced (it carried the asterisk but was
        // never validated). The server accepts an empty body, so this is a
        // purely client-side rule, applied here and on blur via the field's
        // `rules`.
        let description_v = description.read().trim().to_string();
        description_error.set(guard.field(
            "description",
            &description_v,
            "Description",
            &[Rule::Required],
        ));

        // MAPPS-322: Company is required. Route the failure into the picker's
        // own inline error slot (it now takes an `error:` prop) instead of the
        // form-level banner, and focus its search input. `Rule::Uuid` guards a
        // malformed id; in practice the picker only ever yields a real UUID or
        // an empty string, so the surfaced message is `Rule::Required`'s
        // "Company is required."
        company_error.set(guard.field(
            "company_search",
            company_id.read().as_str(),
            "Company",
            &[Rule::Required, Rule::Uuid],
        ));

        if guard.blocked() {
            is_submitting.set(false);
            return;
        }
        // Past the guard: every required field is valid, so the company UUID
        // parses (re-bound here without an unwrap/expect).
        let Some(company_uuid) = uuid::Uuid::parse_str(company_id.read().as_str()).ok() else {
            is_submitting.set(false);
            return;
        };

        // PMS-358: send the selected priority UUID. An empty string means
        // the priorities fetch was still in flight; let the server apply
        // its default rather than blocking the submit.
        let priority_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(priority_id.read().as_str()).ok();

        // PMS-344: asset is optional. Empty string = not picked.
        let asset_uuid: Option<uuid::Uuid> = uuid::Uuid::parse_str(asset_id.read().as_str()).ok();

        // MAPPS-207: contact is optional. Empty string = not picked.
        let contact_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(contact_id.read().as_str()).ok();

        // MAPPS-296: optional type / category / assignee. Each empty
        // signal sends `null` so the server falls back to its default
        // (no enforced type / category / assignee).
        let type_uuid: Option<uuid::Uuid> = uuid::Uuid::parse_str(type_id.read().as_str()).ok();
        let category_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(category_id.read().as_str()).ok();
        let assignee_uuid: Option<uuid::Uuid> =
            uuid::Uuid::parse_str(assigned_to_id.read().as_str()).ok();
        // MAPPS-296: Due Date stamps `scheduled_end` (the server-side
        // SLA / dispatch consume the same field). The date input only
        // emits `YYYY-MM-DD`, so we land it at 23:59 UTC on the chosen
        // day - "due by end of day" is the intuitive read for "Due
        // Date" on a ticket.
        let due_value = due_date.read().clone();
        let scheduled_end: Option<String> = if due_value.trim().is_empty() {
            None
        } else {
            Some(format!("{}T23:59:00Z", due_value.trim()))
        };

        // Title + Description are already validated and captured (trimmed)
        // above; send the trimmed Description (now a required, non-empty value).
        // PMS-482: clone the captured KB id into a per-call binding so
        // the FnMut submit handler can be called more than once.
        let kb_article_id = kb_article_id_for_body.clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                // PMS-482: stamp `source_kb_article_id` when the
                // page was reached from a KB article. Parsed
                // defensively (the prefill already validated it as
                // a UUID, but a hand-edited URL could still slip
                // through) and folded into the body as Null when
                // absent so the server uses its default.
                let kb_article_uuid: serde_json::Value = match uuid::Uuid::parse_str(&kb_article_id)
                {
                    Ok(u) => serde_json::Value::String(u.to_string()),
                    Err(_) => serde_json::Value::Null,
                };
                let body = serde_json::json!({
                    "title": title_v,
                    // MAPPS-322: Description is required and validated non-empty
                    // above, so send the (trimmed) string verbatim. The old
                    // "collapse empty to null" branch made the asterisk a lie:
                    // it let a blank description through as `null`.
                    "description": description_v,
                    "company_id": company_uuid,
                    "contact_id": contact_uuid,
                    "priority_id": priority_uuid,
                    "asset_id": asset_uuid,
                    // MAPPS-296: new fields. The server ignores `null`
                    // and uses its own defaults; this captures every
                    // field a dispatcher needs at creation instead of
                    // forcing a follow-up edit.
                    "type_id": type_uuid,
                    "category_id": category_uuid,
                    "assigned_to_id": assignee_uuid,
                    "scheduled_end": scheduled_end,
                    // PMS-482: KB-article provenance.
                    "source_kb_article_id": kb_article_uuid,
                });

                #[derive(serde::Deserialize)]
                struct CreatedTicket {
                    id: uuid::Uuid,
                }

                match crate::hooks::fetch::api::post_authed_typed::<CreatedTicket, _>(
                    "/tickets", &body,
                )
                .await
                {
                    Ok(created) => {
                        // MAPPS-293: confirming success toast.
                        crate::hooks::toast::push_toast(
                            crate::components::AlertType::Success,
                            "Ticket created.",
                        );
                        navigator.push(Route::TicketDetail {
                            id: created.id.to_string(),
                        });
                    }
                    Err(err) => {
                        // Surface the failure in the form and keep it
                        // mounted so the user can retry without losing
                        // their text. Route a server-flagged Title
                        // validation message next to that input; otherwise
                        // fall back to the general user-facing message
                        // (MAPPS-210).
                        if let Some(msg) = err.field_message("title") {
                            title_error.set(msg);
                        } else if let Some(msg) = err.field_message("description") {
                            // MAPPS-322: a server-side description validation
                            // failure (empty / over-cap) lands inline.
                            description_error.set(msg);
                        } else if let Some(msg) = err.field_message("company_id") {
                            // MAPPS-322: route the company validation failure
                            // next to the picker, same as title/description.
                            company_error.set(msg);
                        } else {
                            error.set(format!("Could not create ticket: {}", err.user_message()));
                        }
                    }
                }
            }

            is_submitting.set(false);
        });
    };

    let picker_selected_id: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };

    let contact_company_filter: Option<String> =
        if uuid::Uuid::parse_str(company_id.read().as_str()).is_ok() {
            Some(company_id.read().clone())
        } else {
            None
        };
    let picker_contact_selected_id: Option<String> =
        if uuid::Uuid::parse_str(contact_id.read().as_str()).is_ok() {
            Some(contact_id.read().clone())
        } else {
            None
        };

    // MAPPS-357: gate the create submit while the server is unreachable.
    let can_mutate = crate::hooks::use_can_mutate();

    rsx! {
        PageHeader {
            title: "New Ticket",
            subtitle: "Create a new support ticket",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: vec![
                        crate::components::BreadcrumbItem {
                            label: "Tickets".to_string(),
                            route: Some(Route::TicketList {}),
                        },
                        crate::components::BreadcrumbItem {
                            label: "New Ticket".to_string(),
                            route: None,
                        },
                    ],
                }
            },
        }

        Card {
            form {
                class: "space-y-6",
                onsubmit: handle_submit,

                if !error.read().is_empty() {
                    ErrorBanner { "{error.read()}" }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    crate::components::Input {
                        name: "title",
                        label: "Title",
                        placeholder: "Brief description of the issue",
                        required: true,
                        // Server caps the title at 500 chars; mirror it
                        // client-side as a UX nicety (MAPPS-210).
                        maxlength: 500,
                        rules: vec![Rule::Required],
                        error: title_error.read().clone(),
                        value: title.read().clone(),
                        oninput: move |e: FormEvent| {
                            title_error.set(String::new());
                            title.set(e.value());
                        },
                    }

                    crate::components::CompanyPicker {
                        value: company_name.read().clone(),
                        selected_id: picker_selected_id,
                        required: true,
                        // MAPPS-322: surface the required-company error
                        // inline on the picker, cleared on select/clear
                        // like the title/description fields.
                        error: company_error.read().clone(),
                        // PMS-352: opt this picker into the inline
                        // "+ Create new company" affordance so a
                        // first-time technician on a tenant with zero
                        // companies can finish the New Ticket flow
                        // without leaving the form to seed a company.
                        allow_inline_create: true,
                        onselect: move |(id, name): (String, String)| {
                            company_error.set(String::new());
                            company_id.set(id);
                            company_name.set(name);
                            // Contacts are scoped to a company; clear
                            // any prior pick when the company changes.
                            contact_id.set(String::new());
                            contact_name.set(String::new());
                        },
                        onclear: move |_| {
                            company_error.set(String::new());
                            company_id.set(String::new());
                            company_name.set(String::new());
                            contact_id.set(String::new());
                            contact_name.set(String::new());
                        },
                    }

                    // MAPPS-207: optional contact on create, scoped to
                    // the selected company. Sends the server's
                    // `contact_id` field on TicketCreateRequest; empty
                    // signal sends null (no contact).
                    crate::components::ContactPicker {
                        value: contact_name.read().clone(),
                        selected_id: picker_contact_selected_id,
                        label: "Contact".to_string(),
                        company_filter: contact_company_filter,
                        // MAPPS-276: opt this picker into the inline
                        // "+ Create new contact" affordance so the New
                        // Ticket flow doesn't dead-end when the calling
                        // user isn't in the company's contacts yet. The
                        // picker inherits the form's selected company
                        // via `company_filter`, so the new contact
                        // lands attached to the right company.
                        allow_inline_create: true,
                        onselect: move |(id, name): (String, String)| {
                            contact_id.set(id);
                            contact_name.set(name);
                        },
                        onclear: move |_| {
                            contact_id.set(String::new());
                            contact_name.set(String::new());
                        },
                    }
                }

                // MAPPS-592: the same field as the edit modal's, so it gets the
                // same editor. A description written here is rendered as
                // Markdown from the moment the ticket exists.
                crate::components::MarkdownEditor {
                    name: "description".to_string(),
                    label: "Description".to_string(),
                    placeholder: "Provide detailed information about the issue…".to_string(),
                    rows: 8,
                    required: true,
                    rules: vec![Rule::Required],
                    error: description_error.read().clone(),
                    value: description.read().clone(),
                    people: crate::hooks::mention_people(&mention_directory),
                    oninput: move |next: String| {
                        description_error.set(String::new());
                        description.set(next);
                    },
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "priority",
                        label: "Priority",
                        options: priority_options,
                        value: priority_id.read().clone(),
                        onchange: move |e: FormEvent| priority_id.set(e.value()),
                    }

                    // PMS-344: optional asset on create. Wires to the
                    // server's `asset_id` field on TicketCreateRequest;
                    // empty signal sends null (no asset).
                    {
                        let picker_asset_selected_id: Option<String> =
                            if uuid::Uuid::parse_str(asset_id.read().as_str()).is_ok() {
                                Some(asset_id.read().clone())
                            } else {
                                None
                            };
                        rsx! {
                            crate::components::AssetPicker {
                                value: asset_name.read().clone(),
                                selected_id: picker_asset_selected_id,
                                onselect: move |(id, name): (String, String)| {
                                    asset_id.set(id);
                                    asset_name.set(name);
                                },
                                onclear: move |_| {
                                    asset_id.set(String::new());
                                    asset_name.set(String::new());
                                },
                            }
                        }
                    }
                }

                // MAPPS-296: richer create form. Type + Category +
                // Assignee + Due Date so the dispatcher captures
                // everything a service-desk ticket needs at
                // creation, instead of opening the new ticket and
                // immediately editing in four more fields.
                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "type",
                        label: "Type",
                        options: type_options,
                        value: type_id.read().clone(),
                        onchange: move |e: FormEvent| type_id.set(e.value()),
                    }
                    Select {
                        name: "category",
                        label: "Category",
                        options: category_options,
                        value: category_id.read().clone(),
                        onchange: move |e: FormEvent| category_id.set(e.value()),
                    }
                }

                div { class: "grid grid-cols-1 gap-6 sm:grid-cols-2",
                    Select {
                        name: "assigned_to",
                        label: "Assigned To",
                        options: assignee_options,
                        value: assigned_to_id.read().clone(),
                        onchange: move |e: FormEvent| assigned_to_id.set(e.value()),
                    }
                    crate::components::DateField {
                        name: "due_date",
                        label: "Due Date".to_string(),
                        value: due_date.read().clone(),
                        help: "Stamps the ticket's scheduled-end so SLA + dispatch view it on creation.".to_string(),
                        oninput: move |e: FormEvent| due_date.set(e.value()),
                    }
                }

                div { class: "flex justify-end space-x-3",
                    Link {
                        to: Route::TicketList {},
                        Button {
                            variant: ButtonVariant::Secondary,
                            "Cancel"
                        }
                    }
                    Button {
                        r#type: "submit",
                        variant: ButtonVariant::Primary,
                        loading: *is_submitting.read(),
                        // MAPPS-357: block create while the server is unreachable.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't create a ticket while the server is unreachable".to_string()),
                        "Create Ticket"
                    }
                }
            }
        }
    }
}

/// Ticket detail page
#[derive(Props, Clone, PartialEq)]
pub struct TicketDetailPageProps {
    pub id: String,
}

#[component]
#[allow(unused_variables)]
pub fn TicketDetailPage(props: TicketDetailPageProps) -> Element {
    let mut note_type = use_signal(|| "internal".to_string());
    let mut note_content = use_signal(String::new);
    // MAPPS-517: the per-note send-email flag the server has carried since
    // PMS-15, surfaced on the composer. Off by default: most notes are
    // internal working discussion and defaulting to on leaks it to the client.
    let mut note_send_email = use_signal(|| false);
    // PMS-518: inline error for the now-enforced required note Content,
    // surfaced in the textarea's own slot by the FormGuard in the composer's
    // submit handler.
    let mut note_content_error = use_signal(String::new);
    let mut note_submitting = use_signal(|| false);
    let mut note_error = use_signal(String::new);
    let ticket_id_for_note = props.id.clone();

    // Drive the whole page off the real ticket, its notes, and its time
    // entries. Each resource yields `Option<Option<T>>`: `None` while the
    // fetch is in flight, `Some(None)` on failure / no token.
    let id_for_ticket = props.id.clone();
    let ticket_resource = use_resource(move || {
        let id = id_for_ticket.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-357: the ticket entity is this page's PRIMARY resource.
            // Subscribe to reachability so it auto-refetches on reconnect, and
            // keep `.ok()` (Option) so a failed load stays distinguishable from
            // a real "ticket not found" 404.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<RemoteTicketDetail>(&format!("/tickets/{id}"))
                .await
                .ok()
        }
    });
    let id_for_notes = props.id.clone();
    let notes_resource = use_resource(move || {
        let id = id_for_notes.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_all_authed::<RemoteNote>(&format!("/tickets/{id}/notes"))
                .await
                .ok()
        }
    });
    let id_for_time = props.id.clone();
    let time_resource = use_resource(move || {
        let id = id_for_time.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            crate::hooks::fetch::api::get_all_authed::<RemoteTimeEntry>(&format!(
                "/time-entries?ticket_id={id}"
            ))
            .await
            .ok()
        }
    });
    let id_for_history = props.id.clone();
    let history_resource = use_resource(move || {
        let id = id_for_history.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-517: keep the `Option` rather than collapsing a failed
            // fetch to an empty Vec here. The journal is now the ticket's
            // record, so "the history did not load" has to stay
            // distinguishable from "nothing has been edited".
            crate::hooks::fetch::api::get_all_authed::<HistoryEntry>(&format!(
                "/audit-log/entity/tickets/{id}"
            ))
            .await
            .ok()
        }
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<UserOpt>("/auth/users")
            .await
            .ok()
            .unwrap_or_default()
    });
    // PMS-359: the tenant's ticket statuses + priorities, fetched once
    // and reused by the three inline editors on the sidebar. Same
    // Paginated envelope the New Ticket form's priorities fetch uses
    // (PMS-358).
    let statuses_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketStatus>("/tickets/statuses")
            .await
            .ok()
            .unwrap_or_default()
    });
    let priorities_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<RemoteTicketPriority>("/tickets/priorities")
            .await
            .ok()
            .unwrap_or_default()
    });
    // PMS-359: per-field error message surfaced inline below the editor.
    // One signal is enough since at most one of the three editors fires
    // at a time and we always clear before the next attempt.
    let mut field_error = use_signal(String::new);

    // PMS-182 description edit state. MAPPS-188 folds the title into the
    // same modal, so the edit form now carries both `e_title` and `e_desc`.
    let mut editing_desc = use_signal(|| false);
    let mut e_title = use_signal(String::new);
    let mut e_desc = use_signal(String::new);
    // PMS-518: per-field inline errors for the Edit Ticket modal's required
    // Title + Description, surfaced in each field's own slot by the FormGuard
    // in `on_save`.
    let mut e_title_error = use_signal(String::new);
    let mut e_desc_error = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    // MAPPS-592: who `@handle` completes against in the description editor.
    // Same list the renderer already resolves a chip from, so a mention typed
    // here is one the reader will see resolved.
    let mention_directory = crate::hooks::use_mention_directory(true);
    let id_for_save = props.id.clone();

    // MAPPS-313: delete-ticket affordance on the detail page. The
    // existing list bulk-delete (MAPPS-310) and the detail-page
    // Delete here use the same `ConfirmDialog` shape; success
    // toasts and navigates back to the list.
    let mut confirming_ticket_delete = use_signal(|| false);
    let mut deleting_ticket = use_signal(|| false);
    let mut delete_ticket_error = use_signal(String::new);
    let delete_nav = use_navigator();
    let id_for_delete = props.id.clone();

    // MAPPS-198: keep the failure state distinct from the in-flight one.
    // Each resource yields `Option<Option<T>>`: `None` while in flight,
    // `Some(None)` on a failed fetch (e.g. a 404 for a missing/deleted/
    // cross-tenant ticket). `.flatten()` collapses both to `None`, so on its
    // own it cannot tell loading from a 404 and the page hangs on "Loading…"
    // forever. Capture the failure before flattening and short-circuit to a
    // "Ticket not found" state, mirroring the explicit `Some(None)` arm on the
    // invoice/contract/company detail pages.
    let ticket_snapshot = ticket_resource.read_unchecked().clone();
    let ticket_fetch_failed = matches!(ticket_snapshot, Some(None));
    let ticket = ticket_snapshot.flatten();
    // MAPPS-357: split the failed-fetch case. A failure while the server is
    // flagged down is an outage - render the honest ContentUnavailable state
    // (which keeps the nav + banner and auto-recovers on reconnect) instead of
    // the misleading "Ticket not found". A failure while the server is still
    // reachable is a real 404 (deleted / cross-tenant / bad link) and keeps the
    // existing not-found body below. Writes are blocked while down; `can_mutate`
    // disables every mutating control on the page.
    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    // MAPPS-366: set the tab title once, before the early returns (use_page_title
    // is a hook, so it must run on every render path). Mirrors the header label
    // used in the body: the ticket's "number: title" once loaded, "Loading…" in
    // flight, or "Ticket not found" on a 404.
    let header_title = match ticket.as_ref() {
        Some(t) if !t.title.trim().is_empty() => {
            if t.ticket_number.trim().is_empty() {
                t.title.clone()
            } else {
                format!("{}: {}", t.ticket_number, t.title)
            }
        }
        Some(_) => format!("Ticket {}", props.id),
        None => "Loading…".to_string(),
    };
    let title = if ticket_fetch_failed {
        "Ticket not found".to_string()
    } else {
        header_title.clone()
    };
    use_page_title(&title);
    if ticket_fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Ticket".to_string() }
        };
    }
    if ticket_fetch_failed {
        return rsx! {
            PageHeader { title: "Ticket not found" }
            Card {
                div { class: "py-8 text-center",
                    p {
                        class: "text-sm text-red-600 dark:text-red-300 mb-2",
                        "Ticket not found. It may have been deleted, or the link may be incorrect."
                    }
                    Link {
                        to: Route::TicketList {},
                        class: "text-sm text-accent hover:opacity-90",
                        "Back to tickets"
                    }
                }
            }
        };
    }
    // MAPPS-517: each journal source is read as a snapshot first, so a failed
    // fetch (`Some(None)`) stays separable from an empty one and can be named
    // under the stream instead of reading as "nothing happened".
    let history_snap = history_resource.read_unchecked().clone();
    let history_failed = matches!(history_snap, Some(None));
    let history = history_snap.flatten().unwrap_or_default();
    let users = users_resource.read_unchecked().clone().unwrap_or_default();
    // PMS-359: lookups for the inline editors. Empty until the fetches
    // land; the renderer falls back to a "Loading…" badge in that window.
    let statuses = statuses_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let priorities = priorities_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    // "Edited" marker for the description: most recent history entry that
    // touched the description column.
    let desc_edited = history
        .iter()
        .find(|e| e.action == "update" && e.changed_fields.iter().any(|f| f == "description"))
        .map(|e| {
            let who = actor_name(&users, &e.user_id);
            let when = fmt_datetime(e.timestamp);
            if who == "-" {
                format!("Edited {when}")
            } else {
                format!("Edited {when} by {who}")
            }
        });
    let notes_snap = notes_resource.read_unchecked().clone();
    let notes_failed = matches!(notes_snap, Some(None));
    let notes: Vec<RemoteNote> = notes_snap.flatten().unwrap_or_default();
    let time_snap = time_resource.read_unchecked().clone();
    let time_failed = matches!(time_snap, Some(None));
    let time_entries: Vec<RemoteTimeEntry> = time_snap.flatten().unwrap_or_default();
    let total_minutes: i32 = time_entries.iter().map(|e| e.duration_minutes).sum();
    let total_hours_label = format!("{:.1} hours", total_minutes as f64 / 60.0);

    // MAPPS-593: who is looking, so the journal knows which notes carry an Edit
    // control. Read once here rather than per entry.
    let (viewer_id, viewer_is_admin) = {
        let auth = crate::hooks::use_auth();
        let a = auth.read();
        (
            a.user.as_ref().map(|u| u.id),
            a.has_role(crate::modules::auth::UserRole::Admin)
                || a.has_role(crate::modules::auth::UserRole::SuperAdmin),
        )
    };

    // MAPPS-517: one stream out of the three sources the page already holds.
    let journal = build_journal(
        &notes,
        &history,
        &time_entries,
        &users,
        viewer_id,
        viewer_is_admin,
    );
    let shown_journal_count = journal.len().min(JOURNAL_LIMIT);
    // Whichever of the three did not load, named. An unreadable source has to
    // be visible here: the journal is the record, and a silently short stream
    // reads as a quiet ticket.
    let journal_gaps: Vec<&str> = [
        (notes_failed, "notes"),
        (history_failed, "change history"),
        (time_failed, "time entries"),
    ]
    .iter()
    .filter_map(|(failed, label)| failed.then_some(*label))
    .collect();
    let journal_gaps_label = journal_gaps.join(", ");
    // Composer state the markup reads more than once.
    let note_is_public = note_type.read().as_str() == "public";
    let note_will_email = note_is_public && note_send_email();
    let note_email_help = if note_is_public {
        "Sent to the ticket's contact. Nothing is sent when the ticket has no contact with an email address."
    } else {
        "Internal notes are never emailed. Switch the note to public to email it."
    }
    .to_string();

    // PMS-362: carry the ticket into the Log Time flow so the work-item picker
    // opens preselected. A plain <a href> (not a routed Link) because the
    // TimeEntryNew route declares no query params, so a Link would strip
    // `?ticket_id=`; the router still intercepts the same-origin anchor click.
    let log_time_href = format!("/time/new?ticket_id={}", props.id);

    rsx! {
        PageHeader {
            title: "{header_title}",
            // PMS-746: a route back to the list, matching ContractDetailPage.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Tickets", Route::TicketList {}, &header_title),
                }
            },
            // MAPPS-517: no "Add Note" button here any more. The composer is
            // open in the journal below, so a note takes typing, not a click
            // that opens a modal first.
            actions: rsx! {
                a {
                    href: "{log_time_href}",
                    Button {
                        variant: ButtonVariant::Primary,
                        ClockIcon { size: IconSize::Small, class: "mr-2".to_string() }
                        "Log Time"
                    }
                }
                // MAPPS-313: per-ticket Delete affordance, matching
                // the pattern on Company / Contract / Asset detail.
                Button {
                    variant: ButtonVariant::Danger,
                    // MAPPS-357: block delete while the server is unreachable.
                    disabled: deleting_ticket() || !can_mutate,
                    title: (!can_mutate).then(|| "Can't delete while the server is unreachable".to_string()),
                    onclick: move |_| {
                        delete_ticket_error.set(String::new());
                        confirming_ticket_delete.set(true);
                    },
                    "Delete"
                }
            },
        }
        // MAPPS-313: confirm-before-delete for the ticket. Success
        // toasts, navigates back to the list. Failure surfaces the
        // server message inline in the dialog so the user can
        // retry without losing their place.
        {
            let ticket_label = ticket
                .as_ref()
                .map(|t| {
                    if t.ticket_number.trim().is_empty() {
                        t.title.clone()
                    } else {
                        format!("{} - {}", t.ticket_number, t.title)
                    }
                })
                .unwrap_or_else(|| "this ticket".to_string());
            let id_for_confirm = id_for_delete.clone();
            rsx! {
                crate::components::ConfirmDialog {
                    open: confirming_ticket_delete(),
                    title: "Delete ticket".to_string(),
                    message: {
                        let mut msg = format!(
                            "Delete {ticket_label}? Notes, attachments, and time entries on this ticket are also removed. This cannot be undone."
                        );
                        if !delete_ticket_error.read().is_empty() {
                            msg.push_str(&format!("\n\n{}", delete_ticket_error.read()));
                        }
                        msg
                    },
                    confirm_text: "Delete ticket".to_string(),
                    cancel_text: "Cancel".to_string(),
                    destructive: true,
                    loading: deleting_ticket(),
                    oncancel: move |_| {
                        if !deleting_ticket() {
                            confirming_ticket_delete.set(false);
                            delete_ticket_error.set(String::new());
                        }
                    },
                    onconfirm: move |_| {
                        if deleting_ticket() { return; }
                        deleting_ticket.set(true);
                        delete_ticket_error.set(String::new());
                        let id = id_for_confirm.clone();
                        spawn(async move {
                            #[cfg(feature = "web")]
                            {
                                let path = format!("/tickets/{id}");
                                match crate::hooks::fetch::api::delete_authed(&path).await {
                                    Ok(()) => {
                                        crate::hooks::toast::push_toast(
                                            crate::components::AlertType::Success,
                                            "Ticket deleted.",
                                        );
                                        confirming_ticket_delete.set(false);
                                        delete_nav.push(Route::TicketList {});
                                    }
                                    Err(err) => {
                                        delete_ticket_error.set(format!("Could not delete ticket: {err}"));
                                    }
                                }
                            }
                            #[cfg(not(feature = "web"))]
                            let _ = &id;
                            deleting_ticket.set(false);
                        });
                    },
                }
            }
        }

        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
            // Main content
            div { class: "lg:col-span-2 space-y-6",
                // Description (real ticket description, editable - PMS-182)
                {
                    let ticket_loaded = ticket.is_some();
                    let cur_desc = ticket
                        .as_ref()
                        .and_then(|t| t.description.clone())
                        .unwrap_or_default();
                    // MAPPS-188: seed the title field from the saved ticket
                    // so the edit modal opens with the current title too.
                    let cur_title = ticket
                        .as_ref()
                        .map(|t| t.title.clone())
                        .unwrap_or_default();
                    let open_edit = move |_| {
                        e_title.set(cur_title.clone());
                        e_desc.set(cur_desc.clone());
                        e_error.set(String::new());
                        editing_desc.set(true);
                    };
                    let marker = desc_edited.clone();
                    rsx! {
                        Card {
                            title: "Description",
                            actions: if ticket_loaded {
                                Some(rsx! {
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        // MAPPS-357: block the edit modal while the server is down.
                                        disabled: !can_mutate,
                                        title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                                        onclick: open_edit,
                                        PencilIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                        "Edit"
                                    }
                                })
                            } else {
                                None
                            },
                            if let Some(t) = ticket.as_ref() {
                                if let Some(desc) = t.description.as_ref().filter(|d| !d.trim().is_empty()) {
                                    // PMS-309: render Markdown (sanitized). PMS-348:
                                    // task-list checkboxes are clickable - toggling
                                    // flips the source marker and persists.
                                    {
                                        let desc_src = desc.clone();
                                        let tid = props.id.clone();
                                        rsx! {
                                            crate::components::Markdown {
                                                content: desc.clone(),
                                                // MAPPS-357: task-list checkboxes PUT on
                                                // toggle, so drop interactivity while the
                                                // server is unreachable (on_toggle is a
                                                // no-op when not interactive) to block the
                                                // silent-fail write.
                                                interactive: can_mutate,
                                                on_toggle: move |i: usize| {
                                                    let Some(new_desc) =
                                                        crate::utils::markdown::toggle_task(&desc_src, i)
                                                    else {
                                                        return;
                                                    };
                                                    let tid = tid.clone();
                                                    let mut tr = ticket_resource;
                                                    let mut hr = history_resource;
                                                    spawn(async move {
                                                        let body = serde_json::json!({ "description": new_desc });
                                                        match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                                                                &format!("/tickets/{tid}"),
                                                                &body,
                                                            )
                                                            .await
                                                        {
                                                            Ok(_) => {
                                                                tr.restart();
                                                                hr.restart();
                                                            }
                                                            Err(e) => {
                                                                crate::hooks::push_toast(
                                                                    crate::components::AlertType::Error,
                                                                    format!("Could not update checklist: {e}"),
                                                                );
                                                                tr.restart();
                                                            }
                                                        }
                                                    });
                                                },
                                            }
                                        }
                                    }
                                } else {
                                    p { class: "text-sm text-subtle italic", "No description provided." }
                                }
                                if let Some(m) = marker {
                                    p { class: "text-xs text-subtle italic mt-3", "{m}" }
                                }
                            } else {
                                p { class: "text-sm text-subtle", "Loading…" }
                            }
                        }
                    }
                }

                // PMS-486: ticket-detail Approvals section. Self-contained
                // component owns its own fetch, modal state, and refresh
                // cycle; rendered above the journal so the open approval
                // requests are immediately visible.
                ApprovalsSection { entity_id: props.id.clone() }

                // MAPPS-517: the journal. The composer sits at the top of it,
                // open, and the stream below carries every source this page
                // fetches rather than notes alone.
                Card { title: "Journal",
                    div { class: "space-y-4",
                        if !note_error.read().is_empty() {
                            ErrorBanner { "{note_error}" }
                        }
                        Textarea {
                            name: "content",
                            label: "Add a note",
                            placeholder: "Enter your note…",
                            rows: 4,
                            required: true,
                            rules: vec![Rule::Required],
                            error: note_content_error.read().clone(),
                            value: note_content.read().clone(),
                            oninput: move |e: FormEvent| {
                                note_content_error.set(String::new());
                                note_content.set(e.value());
                            },
                        }
                        div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                            Select {
                                name: "note_type",
                                label: "Note Type",
                                options: vec![
                                    SelectOption::new("internal", "Internal Note"),
                                    SelectOption::new("public", "Public Note (visible to customer)"),
                                ],
                                value: note_type.read().clone(),
                                onchange: move |e: FormEvent| {
                                    // An internal note never leaves the building,
                                    // whatever the flag says (mokosh-server
                                    // `add_note`), so switching back to internal
                                    // clears the toggle rather than leaving a
                                    // checked box that does nothing.
                                    if e.value() == "internal" {
                                        note_send_email.set(false);
                                    }
                                    note_type.set(e.value());
                                },
                            }
                            div { class: "flex items-end",
                                Checkbox {
                                    name: "note_send_email",
                                    label: "Email this note to the client",
                                    checked: note_send_email(),
                                    disabled: !note_is_public,
                                    help: note_email_help,
                                    onchange: move |e: FormEvent| note_send_email.set(e.checked()),
                                }
                            }
                        }
                        div { class: "flex items-center justify-end gap-3",
                            // MAPPS-482 / docs/email-actions.md: this submit
                            // mails a client whenever the toggle is on, so it
                            // carries the mail affordances exactly then.
                            if note_will_email {
                                crate::components::EmailPreview {
                                    event_type: "ticket.note".to_string(),
                                    context: serde_json::json!({
                                        "ticket_number": ticket
                                            .as_ref()
                                            .map(|t| t.ticket_number.clone())
                                            .unwrap_or_default(),
                                        "title": header_title.clone(),
                                        "content": note_content.read().clone(),
                                    }),
                                    empty_note: NOTE_PREVIEW_NOTE.to_string(),
                                }
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                loading: *note_submitting.read(),
                                // MAPPS-357: block the add-note POST while the server is down.
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't add a note while the server is unreachable".to_string()),
                                onclick: move |_| {
                                    note_error.set(String::new());
                                    // PMS-518: validate the required Content through
                                    // the shared FormGuard before submitting so the
                                    // failure lands in the textarea's own inline slot
                                    // and the field is focused. Runs before
                                    // `note_submitting` is set, so the bail path
                                    // leaves it untouched.
                                    let mut guard = FormGuard::new();
                                    let content_v = note_content.read().clone();
                                    note_content_error.set(guard.field(
                                        "content",
                                        content_v.trim(),
                                        "Content",
                                        &[Rule::Required],
                                    ));
                                    if guard.blocked() {
                                        return;
                                    }
                                    note_submitting.set(true);
                                    let id = ticket_id_for_note.clone();
                                    let type_v = note_type.read().clone();
                                    let email_v = type_v == "public" && note_send_email();
                                    spawn(async move {
                                        #[cfg(feature = "web")]
                                        {
                                            let body = serde_json::json!({
                                                "note_type": type_v,
                                                "content": content_v,
                                                "send_email": email_v,
                                            });
                                            let path = format!("/tickets/{id}/notes");
                                            match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(&path, &body).await {
                                                Ok(_) => {
                                                    note_content.set(String::new());
                                                    note_error.set(String::new());
                                                    // Back to the default for the next
                                                    // note rather than staying armed.
                                                    note_send_email.set(false);
                                                    // Refresh the journal so the new note shows.
                                                    let mut nr = notes_resource;
                                                    nr.restart();
                                                }
                                                Err(err) => {
                                                    note_error.set(format!("Could not add note: {err}"));
                                                }
                                            }
                                        }
                                        note_submitting.set(false);
                                    });
                                },
                                if note_will_email {
                                    MailIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                } else {
                                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                }
                                "Add note"
                            }
                        }
                    }

                    div { class: "mt-6 border-t border-line pt-6",
                        if journal.is_empty() {
                            // MAPPS-517: an untouched ticket has no notes and no
                            // events, and that is an empty journal, not an error.
                            // A source that failed to load is a different thing
                            // and says so below instead of claiming this.
                            if journal_gaps.is_empty() {
                                p { class: "text-sm text-subtle italic",
                                    "Nothing has happened on this ticket yet. Notes, state and assignment changes and logged time land here."
                                }
                            }
                        } else {
                            div { class: "flow-root",
                                ul { class: "-mb-8",
                                    for (i , entry) in journal.iter().take(JOURNAL_LIMIT).enumerate() {
                                        // MAPPS-593: keyed, and load-bearing now
                                        // that an entry owns edit state. Without
                                        // a key Dioxus reuses component state
                                        // positionally, so a note arriving at the
                                        // top after a refetch would inherit the
                                        // draft of whatever used to be first. A
                                        // note has an id; the other sources are
                                        // identified by when they happened.
                                        {
                                            let entry_key = match entry.editable_note {
                                                Some(id) => id.to_string(),
                                                None => format!("{}-{i}", entry.at),
                                            };
                                            rsx! {
                                        TimelineItem {
                                            key: "{entry_key}",
                                            user: entry.who.clone(),
                                            action: entry.action.clone(),
                                            time: fmt_datetime(entry.at),
                                            content: entry.body.clone(),
                                            changes: entry.changes.clone(),
                                            editable_note: entry.editable_note,
                                            edited: entry.edited,
                                            ticket_id: props.id.clone(),
                                            can_edit: can_mutate,
                                            on_saved: move |()| {
                                                let mut nr = notes_resource;
                                                nr.restart();
                                            },
                                            is_last: i + 1 == shown_journal_count,
                                        }
                                            }
                                        }
                                    }
                                }
                            }
                            if journal.len() > JOURNAL_LIMIT {
                                p { class: "text-xs text-subtle",
                                    "Showing the {JOURNAL_LIMIT} most recent of {journal.len()} entries."
                                }
                            }
                        }
                        if !journal_gaps.is_empty() {
                            p { class: "mt-4 text-xs text-red-600 dark:text-red-300",
                                "This ticket's {journal_gaps_label} could not be loaded, so the journal is missing entries. Reload the page to try again."
                            }
                        }
                        // MAPPS-517: the journal is assembled here from three
                        // endpoints because the server exposes no single feed
                        // for a ticket. Say what is missing rather than let the
                        // stream read as the whole record.
                        p { class: "mt-4 text-xs text-subtle",
                            "Built from this ticket's notes, its change history and its time entries. Attachments and approvals are not in the stream yet: they record no ticket history entry, so they arrive when the server exposes one activity feed."
                        }
                    }
                }
            }

            // Sidebar
            div { class: "space-y-6",
                Card { title: "Details",
                    if let Some(t) = ticket.as_ref() {
                        dl { class: "space-y-4",
                            // PMS-359: inline Status / Priority / Assigned To editors.
                            // Each renders a native Select bound to the
                            // currently-saved id; onchange fires a PUT
                            // /tickets/{id} with the matching field and
                            // refreshes the ticket + history resources on
                            // success so the change-history pane records
                            // the edit alongside any prior description edits.
                            {
                                // Statuses Select. Empty options list (still
                                // fetching) falls back to a single "Loading…"
                                // entry so the component does not collapse.
                                let current_status = t
                                    .status
                                    .id
                                    .map(|u| u.to_string())
                                    .unwrap_or_default();
                                let mut status_options: Vec<SelectOption> = statuses
                                    .iter()
                                    .map(|s| SelectOption::new(s.id.to_string(), s.name.clone()))
                                    .collect();
                                if status_options.is_empty() {
                                    status_options.push(SelectOption::new("", "Loading…"));
                                }
                                let save_id = props.id.clone();
                                let mut tr = ticket_resource;
                                let mut hr = history_resource;
                                let onchange = move |e: FormEvent| {
                                    let Ok(new_id) = uuid::Uuid::parse_str(&e.value()) else {
                                        return;
                                    };
                                    let save_id = save_id.clone();
                                    spawn(async move {
                                        field_error.set(String::new());
                                        let body =
                                            serde_json::json!({ "status_id": new_id });
                                        match crate::hooks::fetch::api::put_authed::<
                                            serde_json::Value,
                                            _,
                                        >(
                                            &format!("/tickets/{save_id}"), &body
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                tr.restart();
                                                hr.restart();
                                            }
                                            Err(err) => {
                                                field_error.set(format!(
                                                    "Could not update status: {err}"
                                                ));
                                            }
                                        }
                                    });
                                };
                                rsx! {
                                    DetailItem {
                                        label: "Status",
                                        value: rsx! {
                                            Select {
                                                name: "status_id",
                                                label: "",
                                                options: status_options,
                                                value: current_status,
                                                // MAPPS-357: this Select PUTs on change; block it while down.
                                                disabled: !can_mutate,
                                                onchange,
                                            }
                                        },
                                    }
                                }
                            }
                            {
                                let current_priority = t
                                    .priority
                                    .id
                                    .map(|u| u.to_string())
                                    .unwrap_or_default();
                                let mut priority_options: Vec<SelectOption> = priorities
                                    .iter()
                                    .map(|p| SelectOption::new(p.id.to_string(), p.name.clone()))
                                    .collect();
                                if priority_options.is_empty() {
                                    priority_options.push(SelectOption::new("", "Loading…"));
                                }
                                let save_id = props.id.clone();
                                let mut tr = ticket_resource;
                                let mut hr = history_resource;
                                let onchange = move |e: FormEvent| {
                                    let Ok(new_id) = uuid::Uuid::parse_str(&e.value()) else {
                                        return;
                                    };
                                    let save_id = save_id.clone();
                                    spawn(async move {
                                        field_error.set(String::new());
                                        let body =
                                            serde_json::json!({ "priority_id": new_id });
                                        match crate::hooks::fetch::api::put_authed::<
                                            serde_json::Value,
                                            _,
                                        >(
                                            &format!("/tickets/{save_id}"), &body
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                tr.restart();
                                                hr.restart();
                                            }
                                            Err(err) => {
                                                field_error.set(format!(
                                                    "Could not update priority: {err}"
                                                ));
                                            }
                                        }
                                    });
                                };
                                rsx! {
                                    DetailItem {
                                        label: "Priority",
                                        value: rsx! {
                                            Select {
                                                name: "priority_id",
                                                label: "",
                                                options: priority_options,
                                                value: current_priority,
                                                // MAPPS-357: this Select PUTs on change; block it while down.
                                                disabled: !can_mutate,
                                                onchange,
                                            }
                                        },
                                    }
                                }
                            }
                            {
                                // Assignee uses the same users list the change-
                                // history viewer consumes. Empty value = unassigned,
                                // which serialises to JSON null so the server
                                // clears assigned_to_id.
                                let current_assignee = t
                                    .assigned_to_id
                                    .map(|u| u.to_string())
                                    .unwrap_or_default();
                                let mut user_options: Vec<SelectOption> =
                                    vec![SelectOption::new("", "Unassigned")];
                                for u in users.iter() {
                                    let label = if u.full_name.trim().is_empty() {
                                        u.id.to_string()
                                    } else {
                                        u.full_name.clone()
                                    };
                                    user_options.push(SelectOption::new(u.id.to_string(), label));
                                }
                                let save_id = props.id.clone();
                                let mut tr = ticket_resource;
                                let mut hr = history_resource;
                                let onchange = move |e: FormEvent| {
                                    let raw = e.value();
                                    let new_id: Option<uuid::Uuid> = if raw.is_empty() {
                                        None
                                    } else {
                                        match uuid::Uuid::parse_str(&raw) {
                                            Ok(u) => Some(u),
                                            Err(_) => return,
                                        }
                                    };
                                    let save_id = save_id.clone();
                                    spawn(async move {
                                        field_error.set(String::new());
                                        let body =
                                            serde_json::json!({ "assigned_to_id": new_id });
                                        match crate::hooks::fetch::api::put_authed::<
                                            serde_json::Value,
                                            _,
                                        >(
                                            &format!("/tickets/{save_id}"), &body
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                tr.restart();
                                                hr.restart();
                                            }
                                            Err(err) => {
                                                field_error.set(format!(
                                                    "Could not update assignee: {err}"
                                                ));
                                            }
                                        }
                                    });
                                };
                                rsx! {
                                    DetailItem {
                                        label: "Assigned To",
                                        value: rsx! {
                                            Select {
                                                name: "assigned_to_id",
                                                label: "",
                                                options: user_options,
                                                value: current_assignee,
                                                // MAPPS-357: this Select PUTs on change; block it while down.
                                                disabled: !can_mutate,
                                                onchange,
                                            }
                                        },
                                    }
                                }
                            }
                            if !field_error.read().is_empty() {
                                p { class: "text-xs text-red-600 dark:text-red-400",
                                    "{field_error}"
                                }
                            }
                            // PMS-344: Asset row with inline AssetPicker.
                            // Selecting an asset fires PUT /tickets/{id}
                            // with `asset_id`, the same shape the inline
                            // status/priority/assignee editors above use.
                            // Clearing the picker sends `null` so the
                            // server unsets the association.
                            {
                                let current_asset_id = t.asset_id.map(|u| u.to_string());
                                let current_asset_name =
                                    t.asset_name.clone().unwrap_or_default();
                                let save_id = props.id.clone();
                                let mut tr = ticket_resource;
                                let mut hr = history_resource;
                                let put_asset = move |new_id: Option<uuid::Uuid>| {
                                    let save_id = save_id.clone();
                                    spawn(async move {
                                        field_error.set(String::new());
                                        let body = serde_json::json!({
                                            "asset_id": new_id,
                                        });
                                        match crate::hooks::fetch::api::put_authed::<
                                            serde_json::Value,
                                            _,
                                        >(
                                            &format!("/tickets/{save_id}"),
                                            &body,
                                        )
                                        .await
                                        {
                                            Ok(_) => {
                                                tr.restart();
                                                hr.restart();
                                            }
                                            Err(err) => {
                                                field_error.set(format!(
                                                    "Could not update asset: {err}"
                                                ));
                                            }
                                        }
                                    });
                                };
                                let put_asset_for_select = put_asset.clone();
                                let put_asset_for_clear = put_asset.clone();
                                rsx! {
                                    DetailItem {
                                        label: "Asset",
                                        value: rsx! {
                                            // MAPPS-357: the AssetPicker PUTs on
                                            // select/clear but exposes no `disabled`
                                            // prop, so it cannot be gated from this
                                            // file (unlike the Selects above). A write
                                            // it fires while down surfaces the inline
                                            // "Could not update asset" error rather
                                            // than silently succeeding; gating it needs
                                            // a `disabled` prop on the shared component.
                                            crate::components::AssetPicker {
                                                // PMS-344 follow-up
                                                // (layout): suppress
                                                // the picker's own
                                                // label here because
                                                // DetailItem already
                                                // renders "Asset" on
                                                // the left, matching
                                                // how the inline
                                                // Status/Priority/
                                                // Assignee Select
                                                // editors mount.
                                                label: String::new(),
                                                value: current_asset_name,
                                                selected_id: current_asset_id,
                                                onselect: move |(id, _name): (String, String)| {
                                                    if let Ok(uid) = uuid::Uuid::parse_str(&id) {
                                                        put_asset_for_select(Some(uid));
                                                    }
                                                },
                                                onclear: move |_| {
                                                    put_asset_for_clear(None);
                                                },
                                            }
                                        },
                                    }
                                }
                            }
                            // PMS-730 / MAPPS-529: the procedure article the
                            // request form attached, read-only because the
                            // server has no update path for the column outside
                            // that flow. Absent on every other ticket, so the
                            // row is omitted rather than left blank.
                            if let Some(pid) = t.procedure_kb_article_id {
                                DetailItem {
                                    label: "Procedure",
                                    value: rsx! {
                                        Link {
                                            to: Route::KBArticleDetail { id: pid.to_string() },
                                            class: "text-accent hover:opacity-90",
                                            // The title comes joined off the same read; fall
                                            // back to a label rather than an empty link if the
                                            // article is gone.
                                            {
                                                t.procedure_kb_article_title
                                                    .clone()
                                                    .filter(|s| !s.trim().is_empty())
                                                    .unwrap_or_else(|| "View procedure".to_string())
                                            }
                                        }
                                    },
                                }
                            }
                            if !t.company_name.is_empty() {
                                DetailItem {
                                    label: "Company",
                                    value: rsx! {
                                        if let Some(cid) = t.company_id {
                                            Link {
                                                to: Route::CompanyDetail { id: cid.to_string() },
                                                class: "text-accent hover:opacity-90",
                                                "{t.company_name}"
                                            }
                                        } else {
                                            span { "{t.company_name}" }
                                        }
                                    },
                                }
                            }
                            if let Some(contact) = t.contact_name.as_ref().filter(|s| !s.is_empty()) {
                                DetailItem { label: "Contact", value: rsx!(span { "{contact}" }) }
                            }
                            if !t.queue_name.is_empty() {
                                DetailItem { label: "Queue", value: rsx!(span { "{t.queue_name}" }) }
                            }
                            {
                                let created = if t.created_by_name.is_empty() {
                                    fmt_datetime(t.created_at)
                                } else {
                                    format!("{} by {}", fmt_datetime(t.created_at), t.created_by_name)
                                };
                                rsx! {
                                    DetailItem { label: "Created", nowrap: true, value: rsx!(span { "{created}" }) }
                                }
                            }
                            if let Some((variant , label)) = t.sla_status.badge() {
                                DetailItem { label: "SLA Status", value: rsx!(Badge { variant, "{label}" }) }
                            }
                            if let Some(due) = t.sla_due_date.map(format_sla_due) {
                                DetailItem { label: "SLA Due", nowrap: true, value: rsx!(span { "{due}" }) }
                            }
                        }
                    } else {
                        p { class: "text-sm text-subtle", "Loading…" }
                    }

                    // MAPPS-517: the Time Logged card folded in here as a total.
                    // Its per-entry list moved into the journal, where each
                    // logged entry sits in order beside the notes and the state
                    // changes instead of in a box of its own.
                    div { class: "mt-6 border-t border-line pt-4",
                        div { class: "flex justify-between items-center",
                            span { class: "text-sm text-muted", "Time Logged" }
                            span { class: "text-lg font-semibold", "{total_hours_label}" }
                        }
                        if time_entries.is_empty() {
                            p { class: "mt-1 text-sm text-subtle italic", "No time logged yet." }
                        } else {
                            p { class: "mt-1 text-xs text-subtle",
                                "Every entry is in the journal."
                            }
                        }
                    }
                }

                // MAPPS-517: the Change History card (PMS-182) is gone. Its
                // entries are journal lines now, in one stream with the notes
                // and the logged time rather than in a second box that a reader
                // had to cross-reference by timestamp.
            }
        }

        // PMS-182 description edit modal.
        {
            let mut ticket_res = ticket_resource;
            let mut history_res = history_resource;
            let save_id = id_for_save.clone();
            let on_save = move |_| {
                if e_submitting() {
                    return;
                }
                e_error.set(String::new());
                // PMS-518: validate the required Title + Description through
                // the shared FormGuard so both failures surface inline at
                // once and the first invalid field is focused. The ids match
                // each field component's `name` prop. MAPPS-188: Title is
                // still trimmed (the server validates length >= 1) so a
                // whitespace-only value never reaches the PUT.
                let mut guard = FormGuard::new();
                let title_v = e_title().trim().to_string();
                e_title_error
                    .set(guard.field("edit-title", &title_v, "Title", &[Rule::Required]));
                let desc_v = e_desc().trim().to_string();
                e_desc_error.set(guard.field(
                    "edit-description",
                    &desc_v,
                    "Description",
                    &[Rule::Required],
                ));
                if guard.blocked() {
                    return;
                }
                let save_id = save_id.clone();
                spawn(async move {
                    e_submitting.set(true);
                    e_error.set(String::new());
                    // MAPPS-322: send the trimmed, guard-validated values.
                    // `desc_v` is already non-empty (the FormGuard above
                    // blocks a blank edit), so an edit can no longer blank
                    // an existing description.
                    let body = serde_json::json!({
                        "title": title_v,
                        "description": desc_v,
                    });
                    match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                        &format!("/tickets/{save_id}"),
                        &body,
                    )
                    .await
                    {
                        Ok(_) => {
                            e_submitting.set(false);
                            editing_desc.set(false);
                            ticket_res.restart();
                            history_res.restart();
                        }
                        Err(err) => {
                            e_submitting.set(false);
                            e_error.set(err);
                        }
                    }
                });
            };
            rsx! {
                Modal {
                    open: editing_desc(),
                    title: "Edit Ticket",
                    size: crate::components::ModalSize::Large,
                    onclose: move |_| editing_desc.set(false),
                    footer: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| editing_desc.set(false),
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            loading: e_submitting(),
                            // MAPPS-357: block the save PUT while the server is down.
                            disabled: !can_mutate,
                            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                            onclick: on_save,
                            "Save Changes"
                        }
                    },
                    div { class: "space-y-3",
                        if !e_error().is_empty() {
                            p { class: "text-sm text-red-600 dark:text-red-400", "{e_error}" }
                        }
                        // MAPPS-188: title is now editable alongside the
                        // description (previously description-only).
                        crate::components::Input {
                            name: "edit-title",
                            label: "Title",
                            required: true,
                            rules: vec![Rule::Required],
                            error: e_title_error.read().clone(),
                            value: "{e_title}",
                            oninput: move |e: FormEvent| {
                                e_title_error.set(String::new());
                                e_title.set(e.value());
                            },
                        }
                        // MAPPS-592: the description is Markdown and is
                        // rendered as Markdown, but it was edited in a bare
                        // textarea: same syntax as a KB article, none of the
                        // help. This is the KB write pane, minus the preview
                        // (the modal has nowhere to put a second pane) and
                        // minus uploading (the upload route belongs to an
                        // article; a ticket has nothing to attach a file to).
                        crate::components::MarkdownEditor {
                            name: "edit-description".to_string(),
                            label: "Description".to_string(),
                            rows: 10,
                            required: true,
                            disabled: !can_mutate,
                            rules: vec![Rule::Required],
                            error: e_desc_error.read().clone(),
                            value: e_desc.read().clone(),
                            people: crate::hooks::mention_people(&mention_directory),
                            oninput: move |next: String| {
                                e_desc_error.set(String::new());
                                e_desc.set(next);
                            },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DetailItemProps {
    label: String,
    value: Element,
    /// Keep the value on a single line (no wrapping). Used for Created and
    /// SLA Due, whose timestamps would otherwise wrap onto a second line
    /// (PMS-181).
    #[props(default = false)]
    nowrap: bool,
}

#[component]
fn DetailItem(props: DetailItemProps) -> Element {
    // `flex-1 min-w-0` on dd so the value cell grows to fill the row
    // after the (shrink-0) label, instead of being sized to its content.
    // The Select-based inline editors stay visually unchanged because
    // their content is small and stays right-anchored by `text-right`,
    // but the AssetPicker chip (PMS-344) can now `w-full` itself into
    // the dd cell and truncate its long asset name / uuid inline rather
    // than escaping the row and rendering on top of the next field.
    // `items-start` (instead of `items-baseline`) keeps a multi-line
    // value cell (chip + buttons) aligned to the label's top, not its
    // baseline.
    let dd_class = if props.nowrap {
        "text-sm text-content text-right whitespace-nowrap flex-1 min-w-0"
    } else {
        "text-sm text-content text-right flex-1 min-w-0"
    };
    rsx! {
        div { class: "flex justify-between items-start gap-3",
            dt { class: "text-sm text-muted flex-shrink-0 pt-0.5", "{props.label}" }
            dd { class: "{dd_class}", {props.value} }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct TimelineItemProps {
    user: String,
    action: String,
    time: String,
    content: Option<String>,
    /// MAPPS-596: an edit's before/after, which collapses behind a `Details`
    /// toggle when it is large. Empty for a note or a time entry, whose text
    /// is the entry itself and always shows.
    #[props(default)]
    changes: Vec<ChangeLine>,
    /// MAPPS-593: the note behind this entry, when THIS viewer may edit it.
    /// `None` renders no control, which covers a change-history line, a time
    /// entry, and every note the viewer may not edit.
    #[props(default)]
    editable_note: Option<uuid::Uuid>,
    /// MAPPS-593: the note has been edited since it was written.
    #[props(default = false)]
    edited: bool,
    /// Ticket the note belongs to, for the PUT path.
    #[props(default)]
    ticket_id: String,
    /// Whether an edit may be attempted at all (MAPPS-357's write gate).
    #[props(default = true)]
    can_edit: bool,
    /// Fires after a successful save, so the host can refetch the journal.
    #[props(default)]
    on_saved: EventHandler<()>,
    is_last: bool,
}

#[component]
fn TimelineItem(props: TimelineItemProps) -> Element {
    // MAPPS-593: the edit state belongs to this entry, so two open notes on one
    // ticket do not share a draft. Component-per-entry is what makes that free.
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut edit_error = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let original = props.content.clone().unwrap_or_default();
    let original_for_open = original.clone();
    let note_dom_id = props
        .editable_note
        .map(|id| id.to_string())
        .unwrap_or_default();

    let save_note = {
        let ticket_id = props.ticket_id.clone();
        let note_id = props.editable_note;
        let on_saved = props.on_saved;
        move |_| {
            if saving() {
                return;
            }
            let Some(note_id) = note_id else { return };
            let next = draft.read().trim().to_string();
            if next.is_empty() {
                // The same rule the server applies. An edit that blanks a note
                // is a delete wearing an edit's clothes.
                edit_error.set("A note cannot be empty.".to_string());
                return;
            }
            let path = format!("/tickets/{ticket_id}/notes/{note_id}");
            spawn(async move {
                saving.set(true);
                let body = serde_json::json!({ "content": next });
                match crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                {
                    Ok(_) => {
                        saving.set(false);
                        editing.set(false);
                        on_saved.call(());
                    }
                    Err(e) => {
                        saving.set(false);
                        // Next to the note, not as a toast: the refusal is
                        // about THIS note and a toast outlives the context it
                        // belongs to. The server owns the rules that can
                        // refuse (a 409 on an emailed or portal-authored note),
                        // and its sentence is what the author needs to read.
                        edit_error.set(e);
                    }
                }
            });
        }
    };

    rsx! {
        li {
            div { class: "relative pb-8",
                if !props.is_last {
                    span {
                        class: "absolute left-4 top-4 -ml-px h-full w-0.5 bg-surface-2",
                        aria_hidden: "true",
                    }
                }
                div { class: "relative flex space-x-3",
                    div {
                        span { class: "h-8 w-8 rounded-full bg-accent-100 dark:bg-accent-900 flex items-center justify-center ring-8 ring-surface",
                            UserCircleIcon { size: IconSize::Small, class: "text-accent".to_string() }
                        }
                    }
                    div { class: "flex min-w-0 flex-1 justify-between space-x-4 pt-1.5",
                        div {
                            p { class: "text-sm text-muted",
                                span { class: "font-medium text-content", "{props.user}" }
                                " {props.action}"
                                if props.edited {
                                    // MAPPS-593: an unmarked edit means the
                                    // reader cannot tell that the text in front
                                    // of them is not what was written.
                                    span { class: "ml-1 text-xs text-subtle italic", "(edited)" }
                                }
                                if props.editable_note.is_some() && !editing() {
                                    button {
                                        r#type: "button",
                                        class: "ml-2 text-xs text-accent hover:underline disabled:opacity-40 disabled:cursor-not-allowed disabled:no-underline",
                                        disabled: !props.can_edit,
                                        title: if props.can_edit { None } else { Some("Can't edit while the server is unreachable".to_string()) },
                                        onclick: move |_| {
                                            draft.set(original_for_open.clone());
                                            edit_error.set(String::new());
                                            editing.set(true);
                                        },
                                        "Edit"
                                    }
                                }
                            }
                            if editing() {
                                // MAPPS-593: inline, not a modal. A note is
                                // short and the thread around it is the context
                                // for the edit; a modal would hide exactly what
                                // the author is correcting against.
                                div { class: "mt-2 space-y-2",
                                    Textarea {
                                        name: "edit-note-{note_dom_id}",
                                        label: "Edit note",
                                        rows: 4,
                                        required: true,
                                        rules: vec![Rule::Required],
                                        error: edit_error.read().clone(),
                                        value: draft.read().clone(),
                                        oninput: move |e: FormEvent| {
                                            edit_error.set(String::new());
                                            draft.set(e.value());
                                        },
                                    }
                                    div { class: "flex gap-2",
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            loading: saving(),
                                            onclick: save_note,
                                            "Save"
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            onclick: move |_| {
                                                // Cancel restores what was
                                                // written; nothing is sent.
                                                draft.set(original.clone());
                                                edit_error.set(String::new());
                                                editing.set(false);
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            } else if let Some(content) = &props.content {
                                div { class: "mt-2 text-sm text-content bg-surface-2 rounded-md p-3 whitespace-pre-wrap",
                                    "{content}"
                                }
                            }
                            ChangeDetails { changes: props.changes.clone() }
                        }
                        div { class: "whitespace-nowrap text-right text-sm text-muted",
                            "{props.time}"
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// PMS-486: Approvals section on the ticket-detail page.
// ============================================================================

#[derive(Clone, Debug, Deserialize)]
struct TicketApprovalRow {
    id: uuid::Uuid,
    #[serde(default)]
    state: String,
    #[serde(default)]
    requested_by_name: Option<String>,
    #[serde(default)]
    approver_user_name: Option<String>,
    #[serde(default)]
    approver_role: Option<String>,
    #[serde(default)]
    decision: Option<String>,
    #[serde(default)]
    decision_notes: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    requested_at: Option<DateTime<Utc>>,
    #[serde(default)]
    decided_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
struct UserPickerRow {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    email: String,
}

/// PMS-675: the approvals surface is polymorphic server-side
/// (`target` = ticket | change_request | quote | time_entry), so this
/// section is parameterised by entity rather than forked per entity.
/// The defaults keep it a drop-in for its original ticket call site.
#[derive(Props, Clone, PartialEq)]
pub struct ApprovalsSectionProps {
    /// The parent entity's id.
    pub entity_id: String,
    /// URL segment the approvals hang off: `tickets`, `quotes`, ...
    #[props(default = "tickets".to_string())]
    pub entity_segment: String,
    /// Noun used in the empty / error copy ("this ticket", "this quote").
    #[props(default = "ticket".to_string())]
    pub entity_noun: String,
}

#[component]
pub fn ApprovalsSection(props: ApprovalsSectionProps) -> Element {
    let mut version = use_signal(|| 0u32);
    let mut show_request = use_signal(|| false);
    let mut approver_user_id = use_signal(String::new);
    let mut approver_role = use_signal(String::new);
    let mut request_notes = use_signal(String::new);
    let mut request_submitting = use_signal(|| false);
    let mut request_error = use_signal(String::new);

    let id_for_list = props.entity_id.clone();
    let segment_for_list = props.entity_segment.clone();
    let approvals_resource = use_resource(move || {
        let id = id_for_list.clone();
        let segment = segment_for_list.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _v = version.read();
            crate::hooks::fetch::api::get_authed::<Vec<TicketApprovalRow>>(&format!(
                "/{segment}/{id}/approvals"
            ))
            .await
            .ok()
        }
    });
    let users_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<UserPickerRow>("/auth/users")
            .await
            .ok()
            .unwrap_or_default()
    });

    let snap = approvals_resource.read_unchecked();
    let rows: Vec<TicketApprovalRow> = match &*snap {
        Some(Some(rows)) => rows.clone(),
        _ => Vec::new(),
    };
    let loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let users = users_resource.read_unchecked().clone().unwrap_or_default();

    // MAPPS-357: this is a SECONDARY section embedded in the ticket-detail page,
    // not a routed page - the parent already swaps in ContentUnavailable when the
    // ticket entity fails to load, so a failed approvals fetch keeps its inline
    // "could not load" message rather than blanking the page. But its write
    // controls (Request approval) still get disabled while the server is down so
    // a request cannot silently fail.
    let can_mutate = crate::hooks::use_can_mutate();

    let ticket_for_submit = props.entity_id.clone();
    let segment_for_submit = props.entity_segment.clone();
    let on_submit = move |_| {
        let user_id = approver_user_id.read().trim().to_string();
        let role = approver_role.read().trim().to_string();
        let notes = request_notes.read().trim().to_string();
        // PMS-518: the approver is an XOR rule (exactly one of a specific user
        // OR a role). Neither side owns an inline error slot, so a violation
        // goes to the form-level banner; `note_invalid` blocks the submit and
        // focuses the approver picker. Runs before `request_submitting` is set,
        // so the bail path leaves it untouched.
        let mut guard = FormGuard::new();
        if user_id.is_empty() && role.is_empty() {
            request_error.set("Pick an approver or enter a role.".to_string());
            guard.note_invalid(Some("approver_user_id"));
        }
        if !user_id.is_empty() && !role.is_empty() {
            request_error.set("Pick either an approver or a role, not both.".to_string());
            guard.note_invalid(Some("approver_user_id"));
        }
        if guard.blocked() {
            return;
        }
        let ticket = ticket_for_submit.clone();
        // Cloned per invocation, like `ticket` above: the closure is an
        // `onclick` handler and must stay `FnMut`, so it cannot move the
        // captured segment out of itself.
        let segment = segment_for_submit.clone();
        request_submitting.set(true);
        request_error.set(String::new());
        spawn(async move {
            let mut body = serde_json::Map::new();
            if !user_id.is_empty() {
                if let Ok(u) = uuid::Uuid::parse_str(&user_id) {
                    body.insert(
                        "approver_user_id".to_string(),
                        serde_json::Value::String(u.to_string()),
                    );
                }
            }
            if !role.is_empty() {
                body.insert("approver_role".to_string(), serde_json::Value::String(role));
            }
            if !notes.is_empty() {
                body.insert("notes".to_string(), serde_json::Value::String(notes));
            }
            let json = serde_json::Value::Object(body);
            match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                &format!("/{segment}/{ticket}/approvals"),
                &json,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(AlertType::Success, "Approval requested");
                    show_request.set(false);
                    approver_user_id.set(String::new());
                    approver_role.set(String::new());
                    request_notes.set(String::new());
                    version += 1;
                }
                Err(e) => {
                    request_error.set(format!("Could not create approval: {e}"));
                }
            }
            request_submitting.set(false);
        });
    };

    let mut user_options: Vec<SelectOption> = users
        .iter()
        .map(|u| {
            let label = if u.name.trim().is_empty() {
                u.email.clone()
            } else {
                u.name.clone()
            };
            SelectOption::new(u.id.to_string(), label)
        })
        .collect();
    user_options.insert(0, SelectOption::new("", "- Pick approver -"));

    rsx! {
        Card { title: "Approvals",
            div { class: "flex justify-end mb-3",
                Button {
                    variant: ButtonVariant::Primary,
                    // MAPPS-357: block requesting an approval while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                    onclick: move |_| show_request.set(true),
                    "Request approval"
                }
            }
            if loading {
                p { class: "text-sm text-subtle italic", "Loading approvals…" }
            } else if fetch_failed {
                p { class: "text-sm text-red-600 dark:text-red-300", "Could not load approvals for this {props.entity_noun}." }
            } else if rows.is_empty() {
                // PMS-747: "No approvals requested yet" read as an obligation
                // not yet met, which is how a ticket raised from a client's own
                // request form looked like it was being held for sign-off.
                // Nothing gates a ticket on an approval; one exists only if
                // somebody here asks for it.
                p { class: "text-sm text-subtle italic",
                    "No approvals on this {props.entity_noun}. Approval is optional: this {props.entity_noun} is not waiting on one unless you request it."
                }
            } else {
                ul { class: "space-y-3",
                    for row in rows.iter().cloned() {
                        {
                            let key = row.id.to_string();
                            let state_variant = match row.state.as_str() {
                                "approved" => BadgeVariant::Green,
                                "rejected" => BadgeVariant::Red,
                                _ => BadgeVariant::Yellow,
                            };
                            let approver_label = match (row.approver_user_name.clone(), row.approver_role.clone()) {
                                (Some(n), _) if !n.trim().is_empty() => format!("To: {n}"),
                                (_, Some(r)) if !r.trim().is_empty() => format!("Role: {r}"),
                                _ => "(unassigned)".to_string(),
                            };
                            let requester = row.requested_by_name.clone().unwrap_or_default();
                            let when = row
                                .requested_at
                                .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                .unwrap_or_default();
                            let decided = row
                                .decided_at
                                .map(|d| d.format("%b %-d, %Y %H:%M UTC").to_string())
                                .unwrap_or_default();
                            let notes = row.notes.clone().unwrap_or_default();
                            let decision = row.decision.clone().unwrap_or_default();
                            let decision_notes = row.decision_notes.clone().unwrap_or_default();
                            rsx! {
                                li { key: "{key}", class: "rounded border border-line p-3",
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        Badge { variant: state_variant, "{row.state}" }
                                        span { class: "text-sm text-content", "{approver_label}" }
                                    }
                                    if !requester.is_empty() {
                                        p { class: "text-xs text-subtle mt-1", "Requested by {requester}" }
                                    }
                                    if !when.is_empty() {
                                        p { class: "text-xs text-subtle", "Requested {when}" }
                                    }
                                    if !notes.is_empty() {
                                        p { class: "text-sm text-muted mt-2 whitespace-pre-wrap", "{notes}" }
                                    }
                                    if !decision.is_empty() {
                                        p { class: "text-xs text-subtle mt-2",
                                            "Decision: " strong { "{decision}" }
                                            if !decided.is_empty() { " on {decided}" }
                                        }
                                    }
                                    if !decision_notes.is_empty() {
                                        p { class: "text-sm text-muted mt-1 whitespace-pre-wrap italic",
                                            "{decision_notes}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            open: *show_request.read(),
            title: "Request approval",
            size: crate::components::ModalSize::Medium,
            onclose: move |_| show_request.set(false),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| show_request.set(false),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *request_submitting.read(),
                    // MAPPS-357: block the approval-request POST while the server is down.
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                    onclick: on_submit,
                    "Request"
                }
            },
            div { class: "space-y-4",
                if !request_error().is_empty() {
                    ErrorBanner { "{request_error}" }
                }
                p { class: "text-xs text-subtle",
                    "Pick a specific approver OR enter a role. The server requires exactly one."
                }
                Select {
                    name: "approver_user_id",
                    label: "Approver",
                    options: user_options,
                    value: approver_user_id.read().clone(),
                    onchange: move |e: FormEvent| approver_user_id.set(e.value()),
                }
                crate::components::Input {
                    name: "approver_role",
                    label: "Approver role (optional)",
                    value: "{approver_role}",
                    placeholder: "e.g. manager, finance",
                    oninput: move |e: FormEvent| approver_role.set(e.value()),
                }
                Textarea {
                    name: "request_notes",
                    label: "Notes (optional)",
                    rows: 3,
                    value: "{request_notes}",
                    oninput: move |e: FormEvent| request_notes.set(e.value()),
                }
            }
        }
    }
}

#[cfg(test)]
mod procedure_kb_tests {
    use super::RemoteTicketDetail;

    /// A minimal `TicketResponse` body plus whatever extra fields the
    /// case under test needs.
    fn detail(extra: &str) -> RemoteTicketDetail {
        let body = format!(
            r#"{{"ticket_number":"T-1","title":"t","company_name":"c","queue_name":"q","created_at":"2026-08-14T00:00:00Z"{extra}}}"#
        );
        serde_json::from_str(&body).expect("deserialise ticket detail")
    }

    #[test]
    fn reads_the_procedure_pair() {
        let t = detail(
            r#","procedure_kb_article_id":"11111111-1111-4111-8111-111111111111","procedure_kb_article_title":"How to add a mailbox""#,
        );
        assert_eq!(
            t.procedure_kb_article_id.map(|u| u.to_string()).as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert_eq!(
            t.procedure_kb_article_title.as_deref(),
            Some("How to add a mailbox")
        );
    }

    #[test]
    fn absent_procedure_stays_none() {
        let t = detail("");
        assert!(t.procedure_kb_article_id.is_none());
        assert!(t.procedure_kb_article_title.is_none());
    }

    #[test]
    fn source_article_is_not_the_procedure() {
        // PMS-452's `source_kb_article_id` is the article the ticket was
        // opened FROM; it must never feed the Procedure row.
        let t = detail(r#","source_kb_article_id":"22222222-2222-4222-8222-222222222222""#);
        assert!(t.procedure_kb_article_id.is_none());
    }
}

#[cfg(test)]
mod mapps517_journal_tests {
    use super::{
        build_journal, ChangeLine, HistoryEntry, JournalEntry, RemoteNote, RemoteTimeEntry, UserOpt,
    };

    fn note(json: &str) -> RemoteNote {
        serde_json::from_str(json).expect("deserialise note")
    }

    fn history(json: &str) -> HistoryEntry {
        serde_json::from_str(json).expect("deserialise history entry")
    }

    fn time_entry(json: &str) -> RemoteTimeEntry {
        serde_json::from_str(json).expect("deserialise time entry")
    }

    fn users() -> Vec<UserOpt> {
        vec![serde_json::from_str(
            r#"{"id":"11111111-1111-4111-8111-111111111111","full_name":"Dana Reeve"}"#,
        )
        .expect("deserialise user")]
    }

    fn actions(entries: &[JournalEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| format!("{} {}", e.who, e.action))
            .collect()
    }

    /// The whole point of the journal: four kinds of thing on one clock,
    /// newest first, rather than notes in one box and edits in another.
    #[test]
    fn merges_every_source_newest_first() {
        let notes = vec![note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000001","note_type":"internal","content":"Rebooted the switch","created_by_name":"Dana Reeve","created_at":"2026-08-20T09:00:00Z"}"#,
        )];
        let history = vec![
            history(
                r#"{"action":"update","user_id":"11111111-1111-4111-8111-111111111111","changed_fields":["status_id"],"changes":[{"field":"status_id","old":"33333333-3333-4333-8333-333333333333","new":"44444444-4444-4444-8444-444444444444"}],"timestamp":"2026-08-20T11:00:00Z"}"#,
            ),
            history(
                r#"{"action":"update","user_id":"11111111-1111-4111-8111-111111111111","changed_fields":["assigned_to_id"],"changes":[],"timestamp":"2026-08-20T10:00:00Z"}"#,
            ),
        ];
        let time = vec![time_entry(
            r#"{"date":"2026-08-20","duration_minutes":45,"is_billable":true,"user_id":"11111111-1111-4111-8111-111111111111","created_at":"2026-08-20T12:00:00Z"}"#,
        )];

        let journal = build_journal(&notes, &history, &time, &users(), None, false);

        assert_eq!(
            actions(&journal),
            vec![
                "Dana Reeve logged 45 min on 2026-08-20 (billable)".to_string(),
                "Dana Reeve changed the status".to_string(),
                "Dana Reeve changed the assignee".to_string(),
                "Dana Reeve added an internal note".to_string(),
            ]
        );
    }

    /// A source that yields nothing contributes nothing: the stream degrades
    /// to the notes-only shape the Activity card had, never to an error.
    #[test]
    fn falls_back_to_notes_when_the_other_sources_are_empty() {
        let notes = vec![note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000002","note_type":"public","content":"Emailed the client","created_by_name":"Dana Reeve","is_email_sent":true,"created_at":"2026-08-20T09:00:00Z"}"#,
        )];

        let journal = build_journal(&notes, &[], &[], &users(), None, false);

        assert_eq!(
            actions(&journal),
            vec!["Dana Reeve added a public note and emailed the client".to_string()]
        );
    }

    /// An untouched ticket is an empty journal, not a failure.
    #[test]
    fn an_empty_ticket_yields_an_empty_journal() {
        assert!(build_journal(&[], &[], &[], &users(), None, false).is_empty());
    }

    /// Whether the email went out is the note's own outcome, so the line says
    /// which of the three cases it was.
    #[test]
    fn a_note_line_records_whether_the_email_was_sent() {
        let public_unsent = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000003","note_type":"public","content":"c","created_by_name":"Dana Reeve","created_at":"2026-08-20T09:00:00Z"}"#,
        );
        let internal = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000004","note_type":"internal","content":"c","created_by_name":"Dana Reeve","created_at":"2026-08-20T08:00:00Z"}"#,
        );

        let journal = build_journal(&[public_unsent, internal], &[], &[], &users(), None, false);

        assert_eq!(
            actions(&journal),
            vec![
                "Dana Reeve added a public note (not emailed)".to_string(),
                "Dana Reeve added an internal note".to_string(),
            ]
        );
    }

    /// A FK swap the audit log records as two UUIDs carries no readable
    /// before/after, and "(reference) → (reference)" is noise. MAPPS-596 moved
    /// the before/after off `body` and into `changes`, so it can collapse when
    /// it is large; the drop rule is unchanged and now lives in
    /// `ChangeLine::build`.
    #[test]
    fn a_reference_only_change_renders_no_body() {
        let h = history(
            r#"{"action":"update","user_id":"11111111-1111-4111-8111-111111111111","changed_fields":["status_id"],"changes":[{"field":"status_id","old":"33333333-3333-4333-8333-333333333333","new":"44444444-4444-4444-8444-444444444444"}],"timestamp":"2026-08-20T11:00:00Z"}"#,
        );
        let readable = history(
            r#"{"action":"update","user_id":"11111111-1111-4111-8111-111111111111","changed_fields":["priority_id"],"changes":[{"field":"priority","old":"Low","new":"High"}],"timestamp":"2026-08-20T10:00:00Z"}"#,
        );

        let journal = build_journal(&[], &[h, readable], &[], &users(), None, false);

        assert!(journal[0].changes.is_empty(), "{:?}", journal[0].changes);
        assert_eq!(
            journal[1].changes,
            vec![ChangeLine {
                field: "Priority".to_string(),
                old: "Low".to_string(),
                new: "High".to_string(),
            }]
        );
        // An edit's before/after never rides on `body` any more; that is what
        // put two 160-character values into the middle of the journal.
        assert!(journal.iter().all(|e| e.body.is_none()));
    }

    /// An actor no `/auth/users` row matches reads as "Someone", never as the
    /// bare "-" the change-history pane used in a column of its own.
    #[test]
    fn an_unresolvable_actor_reads_as_someone() {
        let h = history(
            r#"{"action":"create","changed_fields":[],"changes":[],"timestamp":"2026-08-20T09:00:00Z"}"#,
        );

        let journal = build_journal(&[], &[h], &[], &users(), None, false);

        assert_eq!(
            actions(&journal),
            vec!["Someone created the ticket".to_string()]
        );
    }
}

#[cfg(test)]
mod mapps592_description_editor_tests {
    const SRC: &str = include_str!("tickets.rs");

    /// The shipping code with runs of whitespace collapsed, excluding this
    /// module: every assertion quotes the pattern it looks for, so a scan
    /// including its own source matches itself and passes regardless.
    fn code_only() -> String {
        let end = SRC
            .find("mod mapps592_description_editor_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// MAPPS-592: both description fields are the KB write pane.
    ///
    /// A ticket description is Markdown, is rendered as Markdown by the same
    /// component a KB article is, and was written in a bare textarea: the same
    /// syntax with none of the help. Both fields, because they are the same
    /// field: the one on the create form and the one in the edit modal.
    #[test]
    fn both_description_fields_get_the_editor() {
        let code = code_only();
        assert_eq!(
            code.matches("crate::components::MarkdownEditor {").count(),
            2,
            "the create form and the edit modal"
        );
        assert!(
            !code.contains("Textarea { name: \"description\","),
            "and neither is a bare textarea any more"
        );
        assert!(
            !code.contains("Textarea { name: \"edit-description\","),
            "including the one in the modal"
        );
    }

    /// A mention typed in the description has to be one the READER will see
    /// resolved, so the completion list and the renderer's list are the same
    /// list. They are, because both come from `use_mention_directory`.
    #[test]
    fn the_completion_list_is_the_one_the_renderer_resolves_against() {
        let code = code_only();
        assert_eq!(
            code.matches("crate::hooks::use_mention_directory(true)")
                .count(),
            2,
            "one per page component, the list page's create form and the detail page"
        );
        assert!(
            code.contains("people: crate::hooks::mention_people(&mention_directory)"),
            "and it is what the editor completes against"
        );
    }

    /// The edit modal's editor follows the same write gate as its Save button.
    /// MAPPS-357's rule: while the server is unreachable, a control that leads
    /// to a PUT should not invite the click.
    #[test]
    fn the_modal_editor_is_disabled_with_the_rest_of_the_form() {
        let code = code_only();
        let modal = code
            .find("name: \"edit-description\".to_string()")
            .expect("the modal's editor");
        let window = &code[modal..code.len().min(modal + 400)];
        assert!(
            window.contains("disabled: !can_mutate"),
            "the editor is gated with the form it sits in: {window}"
        );
    }
}

#[cfg(test)]
mod mapps593_note_edit_tests {
    use super::{build_journal, note_is_editable, RemoteNote, UserOpt};

    const VIEWER: &str = "11111111-1111-4111-8111-111111111111";
    const SOMEONE_ELSE: &str = "22222222-2222-4222-8222-222222222222";
    const CONTACT: &str = "33333333-3333-4333-8333-333333333333";

    fn viewer() -> uuid::Uuid {
        VIEWER.parse().expect("viewer uuid")
    }

    fn note(json: &str) -> RemoteNote {
        serde_json::from_str(json).expect("deserialise note")
    }

    /// A note of `kind` authored by `author`, with the two frozen-state flags.
    fn make(kind: &str, author: &str, emailed: bool, contact: Option<&str>) -> RemoteNote {
        let contact = match contact {
            Some(c) => format!(r#","created_by_contact_id":"{c}""#),
            None => String::new(),
        };
        note(&format!(
            r#"{{"id":"aaaaaaaa-0000-4000-8000-00000000000f","note_type":"{kind}",
                "content":"c","created_by_name":"Dana Reeve","is_email_sent":{emailed},
                "created_by_id":"{author}","created_at":"2026-08-20T09:00:00Z"{contact}}}"#
        ))
    }

    /// The reported case: the author corrects their own internal note.
    #[test]
    fn the_author_may_edit_their_own_internal_note() {
        assert!(note_is_editable(
            &make("internal", VIEWER, false, None),
            Some(viewer()),
            false
        ));
    }

    /// And the title's case: the MSP owner corrects anyone's.
    #[test]
    fn an_admin_may_edit_somebody_elses() {
        let n = make("internal", SOMEONE_ELSE, false, None);
        assert!(!note_is_editable(&n, Some(viewer()), false), "not as staff");
        assert!(note_is_editable(&n, Some(viewer()), true), "yes as admin");
    }

    /// The state rules, mirrored from `TicketService::update_note` so the
    /// affordance and the answer agree. A control that 409s is worse than no
    /// control, and the server is still the authority.
    #[test]
    fn a_frozen_note_offers_no_control_even_to_an_admin() {
        // Emailed to the customer: they hold the original in their inbox.
        assert!(!note_is_editable(
            &make("public", VIEWER, true, None),
            Some(viewer()),
            true
        ));
        // The customer's own words, through the portal.
        assert!(!note_is_editable(
            &make("internal", VIEWER, false, Some(CONTACT)),
            Some(viewer()),
            true
        ));
        // Edited through its time entry, not here.
        assert!(!note_is_editable(
            &make("time_entry", VIEWER, false, None),
            Some(viewer()),
            true
        ));
    }

    /// A public note nobody emailed is still editable: the customer may have
    /// read it in the portal, but no copy exists outside the system.
    #[test]
    fn a_public_note_that_was_never_emailed_is_editable() {
        assert!(note_is_editable(
            &make("public", VIEWER, false, None),
            Some(viewer()),
            false
        ));
    }

    /// An unfamiliar note type offers nothing. Guessing yes would put a control
    /// on a row the server refuses, which is the failure this mirror exists to
    /// avoid.
    #[test]
    fn an_unknown_note_type_offers_no_control() {
        assert!(!note_is_editable(
            &make("something_new", VIEWER, false, None),
            Some(viewer()),
            true
        ));
    }

    /// A signed-out or unresolved viewer is nobody's author, and `None == None`
    /// must not read as a match against a note with no recorded author.
    #[test]
    fn an_unknown_viewer_is_not_the_author_of_an_unattributed_note() {
        let orphan = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-00000000000e","note_type":"internal",
                "content":"c","created_by_name":"","created_at":"2026-08-20T09:00:00Z"}"#,
        );
        assert!(!note_is_editable(&orphan, None, false));
        assert!(
            note_is_editable(&orphan, None, true),
            "an admin still may, because the permission does not rest on authorship"
        );
    }

    /// The journal carries the decision per entry, and only a note carries one.
    /// A change-history line or a time entry growing an Edit control would send
    /// a PUT to a note endpoint with no note.
    #[test]
    fn only_a_note_line_carries_an_edit_handle() {
        let users: Vec<UserOpt> = Vec::new();
        let mine = make("internal", VIEWER, false, None);
        let theirs = make("internal", SOMEONE_ELSE, false, None);
        let history = vec![serde_json::from_str(
            r#"{"action":"update","user_id":"11111111-1111-4111-8111-111111111111",
                "changed_fields":["status_id"],"changes":[],
                "timestamp":"2026-08-20T11:00:00Z"}"#,
        )
        .expect("history entry")];

        let journal = build_journal(
            &[mine, theirs],
            &history,
            &[],
            &users,
            Some(viewer()),
            false,
        );

        let with_handles = journal.iter().filter(|e| e.editable_note.is_some()).count();
        assert_eq!(
            with_handles, 1,
            "the viewer's own note, and neither the other author's nor the history line"
        );
    }

    /// An edit has to be visible as one. Both timestamps come from the same
    /// transaction's `NOW()` on insert, so an untouched note has them exactly
    /// equal and the marker is a strict `>`.
    #[test]
    fn a_note_is_marked_edited_only_after_it_was() {
        let users: Vec<UserOpt> = Vec::new();
        let untouched = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-00000000000a","note_type":"internal","content":"c",
                "created_by_name":"D","created_at":"2026-08-20T09:00:00Z",
                "updated_at":"2026-08-20T09:00:00Z"}"#,
        );
        let edited = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-00000000000b","note_type":"internal","content":"c",
                "created_by_name":"D","created_at":"2026-08-20T08:00:00Z",
                "updated_at":"2026-08-20T10:00:00Z"}"#,
        );
        let journal = build_journal(&[untouched, edited], &[], &[], &users, None, false);
        // Newest first: the untouched note (09:00) sorts above the edited one
        // (created 08:00), because the journal is ordered on when it happened.
        assert!(!journal[0].edited, "{:?}", journal[0]);
        assert!(journal[1].edited, "{:?}", journal[1]);
    }

    /// A server that predates PMS-931 sends no `updated_at`. That has to decode
    /// as "never edited" rather than failing the whole notes list, which would
    /// empty the journal on a version skew.
    #[test]
    fn a_note_without_updated_at_still_decodes() {
        let users: Vec<UserOpt> = Vec::new();
        let old = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-00000000000c","note_type":"internal","content":"c",
                "created_by_name":"D","created_at":"2026-08-20T09:00:00Z"}"#,
        );
        let journal = build_journal(&[old], &[], &[], &users, None, false);
        assert_eq!(journal.len(), 1);
        assert!(!journal[0].edited);
    }

    /// MAPPS-593: the journal loop is keyed, and that became load-bearing the
    /// moment an entry started owning edit state. Dioxus reuses component state
    /// positionally, so an unkeyed loop hands a newly-prepended note the draft
    /// of whatever used to be first. This is the same trap MAPPS-596 hit with
    /// the change-history panes.
    #[test]
    fn the_journal_is_keyed_so_a_draft_cannot_follow_the_wrong_note() {
        const SRC: &str = include_str!("tickets.rs");
        let end = SRC
            .find("mod mapps593_note_edit_tests")
            .expect("this module is part of this file");
        let code = SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ");
        let loop_at = code
            .find("for (i , entry) in journal.iter()")
            .expect("the journal loop");
        let window = &code[loop_at..code.len().min(loop_at + 700)];
        assert!(
            window.contains("key: \"{entry_key}\""),
            "the journal loop is keyed: {window}"
        );
        assert!(
            window.contains("Some(id) => id.to_string()"),
            "and a note is keyed on its own id, not its position: {window}"
        );
    }

    /// The refusal belongs next to the note. A toast outlives the context it is
    /// about, and the server's 409 sentence is the thing the author has to read
    /// to know which of four rules they hit.
    #[test]
    fn a_refusal_lands_next_to_the_note_it_is_about() {
        const SRC: &str = include_str!("tickets.rs");
        let end = SRC
            .find("mod mapps593_note_edit_tests")
            .expect("this module is part of this file");
        let code = SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            code.contains("edit_error.set(e);"),
            "the server's message becomes the field's error"
        );
        let save = code.find("let path = format!(\"/tickets/{ticket_id}/notes/{note_id}\");");
        let save = save.expect("the save handler");
        let window = &code[save..code.len().min(save + 900)];
        assert!(!window.contains("push_toast"), "and not a toast: {window}");
    }
}

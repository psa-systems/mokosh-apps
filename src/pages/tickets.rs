//! Ticket pages

use chrono::{DateTime, NaiveDate, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    clear_selection, ticket_status_badge, use_bulk_selection, use_page_title, AlertType, Badge,
    BadgeVariant, BulkActionsBar, BulkSelection, Button, ButtonVariant, Card, Checkbox, ClockIcon,
    DataTable, ErrorBanner, IconSize, Input, MailIcon, Modal, PageHeader, PencilIcon, PlusIcon,
    SearchInput, Select, SelectAllHeader, SelectOption, SelectRowCell, SortDirection, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow, Textarea,
    UserCircleIcon,
};
use crate::components::{ChangeDetails, ChangeLine};
// MAPPS-596: shared with the project, task and asset change-history panes.
use crate::modules::audit::{action_label, fields_label, title_field};
use crate::utils::{FormGuard, Paginated, Rule};
use mokosh_types::tickets::NoteType;

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
/// MAPPS-613: shown under the email checkbox, which is now only rendered on a
/// public note. It no longer has to explain why the control is greyed out,
/// because there is no greyed-out control; it states the one thing that can
/// still surprise the author.
const NOTE_EMAIL_HELP: &str = "Sent to the ticket's contact. Nothing is sent when the ticket has no contact with an email address.";

const NOTE_PREVIEW_NOTE: &str = "The ticket-note email is built into the server rather than by a notification rule, so there is nothing to render yet. The ticket's contact is still emailed the note.";
use crate::Route;

/// MAPPS-607: cap on a client-side attachment upload before base64
/// encoding. Larger files are rejected with an inline copy string so the
/// SPA never fires a `POST /tickets/{id}/attachments` that the server
/// would slice down anyway. 5 MB, in bytes.
pub(crate) const TICKET_ATTACHMENT_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// MAPPS-607: should the Reopen button render? Split out of the render
/// body so the status-name match is unit-testable on the native target
/// (touching `use_capability` from here would panic on the non-web
/// `cargo test --lib` path). Matches case-insensitively on `contains`
/// because tenants coin their own status names (e.g. `Closed - won`,
/// `Resolved (dup)`), so an exact-match against `"closed"` /
/// `"resolved"` would miss the common shapes. The extra cap gate is
/// AND-ed on top so a status match alone doesn't render the button for
/// a contact who lacks `tickets:reopen`; staff sessions bypass the cap
/// unconditionally via `use_capability`.
pub(crate) fn should_show_reopen(status_name: &str, has_reopen_cap: bool) -> bool {
    if !has_reopen_cap {
        return false;
    }
    let s = status_name.to_ascii_lowercase();
    s.contains("closed") || s.contains("resolved")
}

/// MAPPS-609: can THIS contact edit this ticket? The Edit affordance on
/// the ticket-detail Description card renders for staff unconditionally
/// (staff bypass `use_capability`) and for a contact only when BOTH:
///
/// - `has_edit_own` is true (i.e. the contact holds `tickets:edit_own`), AND
/// - the ticket's reporter contact id matches this contact's own id.
///
/// Any `None` on either side (server pre-PMS-937 that omits
/// `reporter_contact_id`, or a pre-PMS-937 login response that never
/// stashed `contact_id`) short-circuits to false so the button hides
/// rather than surfacing a guaranteed 403 on submit.
///
/// Split out of the render body so the four combinations are unit-testable
/// on the native `cargo test --lib` target (touching `use_capability`
/// from here would panic without the wasm environment).
pub(crate) fn contact_can_edit_ticket(
    reporter_contact_id: Option<uuid::Uuid>,
    my_contact_id: Option<uuid::Uuid>,
    has_edit_own: bool,
) -> bool {
    if !has_edit_own {
        return false;
    }
    match (reporter_contact_id, my_contact_id) {
        (Some(r), Some(m)) => r == m,
        _ => false,
    }
}

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
    /// MAPPS-609: the Contact who reported this ticket. Used by the
    /// Description-card Edit button's ownership gate for a contact
    /// session (`contact_can_edit_ticket`). `#[serde(default)]` so a
    /// pre-PMS-937 server that omits the field still deserialises; a
    /// `None` here short-circuits the gate to false and the button hides.
    #[serde(default)]
    reporter_contact_id: Option<uuid::Uuid>,
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

/// MAPPS-613: every `NoteType`, in the order the composer considers them.
///
/// Written out rather than iterated, because the shared enum offers no such
/// list; `composer_label` below is what keeps it honest. A new variant fails
/// to compile there, three lines from here.
const ALL_NOTE_TYPES: [NoteType; 4] = [
    NoteType::Internal,
    NoteType::Public,
    NoteType::Resolution,
    NoteType::TimeEntry,
];

/// MAPPS-613: how the composer labels a note type, or `None` for one an agent
/// must not author.
///
/// An exhaustive match rather than a list, because two of the four are not the
/// same kind of thing. `time_entry` mirrors a time entry and nothing writes
/// one: the server refuses to edit it on the grounds that "a time-entry note
/// is edited through its time entry", so composing one by hand makes a note
/// belonging to an entry that does not exist and that nobody can then correct.
///
/// The shape matters as much as the answer. A hand-written `vec!` is what let
/// `resolution` go missing; iterating every variant would put `time_entry`
/// back. Only a match fails the build on a fifth variant until somebody
/// decides whether an agent may write it.
fn composer_label(kind: NoteType) -> Option<&'static str> {
    match kind {
        NoteType::Internal => Some("Internal Note"),
        NoteType::Public => Some("Public Note (visible to customer)"),
        // Internal, like `internal`: the portal serves `note_type='public'`
        // and nothing else, so a customer never sees one. The label says so,
        // because the type name alone reads like a status the client is told.
        NoteType::Resolution => Some("Resolution Note (internal)"),
        NoteType::TimeEntry => None,
    }
}

/// The composer's Note Type options.
fn note_type_options() -> Vec<SelectOption> {
    ALL_NOTE_TYPES
        .iter()
        .filter_map(|kind| {
            composer_label(*kind).map(|label| SelectOption::new(kind.as_str(), label))
        })
        .collect()
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
        // MAPPS-613: this sentence is the only place a note's type is visible
        // to a reader; there is no badge on the note itself. It used to read
        // "internal, else public", so once `resolution` became composable
        // every resolution note would have been announced as one a customer
        // can see. The portal serves `note_type='public'` and nothing else, so
        // a resolution note is as invisible to them as an internal one.
        let action = match n.note_type.as_str() {
            "internal" => "added an internal note".to_string(),
            "resolution" => "added a resolution note (internal)".to_string(),
            "public" if n.is_email_sent => "added a public note and emailed the client".to_string(),
            "public" => "added a public note (not emailed)".to_string(),
            // A type this build does not know. Say the least that is certainly
            // true rather than assert it is public, which is the claim that
            // costs something if it is wrong.
            _ => "added a note".to_string(),
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
    // mokosh-contact-login prompt 006: gate the "New Ticket" CTA on
    // `tickets:write`. Staff / platform sessions always see it (the
    // hook returns true unconditionally for them); contacts see it
    // only when their role carries the cap.
    let can_create_ticket = crate::hooks::capabilities::use_capability("tickets:write");
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
    //
    // mokosh-contact-login (MAPPS-604): allow either a staff bearer OR a
    // contact bearer to drive this fetch, using `get_authed_any` so a
    // signed-in contact sees only their Company's tickets (server scopes on
    // `typ: "contact"`). Staff sessions still use the workspace bearer.
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
            let has_staff = crate::hooks::fetch::api::current_access_token().is_some();
            let has_contact = crate::hooks::fetch::api::has_contact_session();
            if !has_staff && !has_contact {
                return None;
            }
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
            crate::hooks::fetch::api::get_authed_any::<Paginated<RemoteTicket>>(&path)
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
                if can_create_ticket {
                    Link {
                        to: Route::TicketNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Ticket"
                        }
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
                            #[cfg(feature = "app")]
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
                                if can_create_ticket {
                                    Link {
                                        to: Route::TicketNew {},
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                            "New Ticket"
                                        }
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
    #[cfg(feature = "app")]
    {
        // MAPPS-664: same router-strip fix as contacts.rs. See the
        // long comment there. `initial_search()` returns the boot-
        // time snapshot of `window.location.search` captured before
        // the Dioxus router mounted and replaceState-erased the
        // query.
        let search = crate::modules::oidc::initial_search();
        let params = crate::utils::url::QueryString::parse(&search);
        let id = params.get("company_id").unwrap_or_default();
        let name = params.get("company_name").unwrap_or_default();
        if uuid::Uuid::parse_str(&id).is_ok() {
            return CompanyPrefill { id, name };
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
    #[cfg(feature = "app")]
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
    // PMS-791 phase 3 / MAPPS-464: optional team routing on ticket create.
    // Populated by a lightweight dropdown fed from GET /api/v1/teams.
    // Personal tenants render neither the signal nor the input.
    let mut team_id = use_signal(String::new);
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
        // PMS-791 phase 3: team_id read once so the JSON body below can
        // consume it without re-borrowing across the closure.
        let team_id_v = team_id.read().trim().to_string();

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
            #[cfg(feature = "app")]
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
                // PMS-791 phase 3: parse the team_id input to JSON; empty
                // or unparseable = null (server ignores and leaves NULL).
                let team_uuid: serde_json::Value = {
                    let raw = team_id_v.trim();
                    if raw.is_empty() {
                        serde_json::Value::Null
                    } else {
                        match uuid::Uuid::parse_str(raw) {
                            Ok(u) => serde_json::Value::String(u.to_string()),
                            Err(_) => serde_json::Value::Null,
                        }
                    }
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
                    // PMS-791 phase 3 / MAPPS-464: optional team routing.
                    // Client-side unparseable UUID collapses to null so
                    // an empty or garbage input just omits the field
                    // (server ignores null and leaves team_id NULL).
                    "team_id": team_uuid,
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
                    // MAPPS-610: the same switcher the KB body has. One key for
                    // both description editors, so a reporter who writes in
                    // split view keeps it when they come back to edit.
                    views: true,
                    view_pref_key: "ticket_desc_view_mode".to_string(),
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

                // PMS-791 phase 3 / MAPPS-464: optional team routing.
                // First-pass UX is a plain UUID input; a proper Team
                // dropdown component fed by GET /api/v1/teams is filed
                // as a follow-up. Personal tenants hide the field
                // entirely per Q4 default.
                if !crate::hooks::use_auth().read().is_personal_tenant() {
                    Input {
                        name: "team_id",
                        label: "Team (UUID, optional)",
                        r#type: "text".to_string(),
                        value: team_id.read().clone(),
                        oninput: move |e: FormEvent| team_id.set(e.value()),
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
    // mokosh-contact-login prompt 006: capability gates. `can_comment`
    // covers the customer-facing reply surface; every other mutation
    // control (Log Time, Delete, inline status/priority/assignee
    // editors, internal-note type) sits behind `STAFF_ONLY` so a
    // mis-configured contact JWT can never render them.
    let can_comment = crate::hooks::capabilities::use_capability("tickets:comment");
    let staff_only =
        crate::hooks::capabilities::use_capability(crate::hooks::capabilities::STAFF_ONLY);
    // MAPPS-607: new dual-plane caps introduced by PMS-936. Staff and
    // platform-admin sessions bypass unconditionally via `use_capability`,
    // so the buttons still render for them regardless of the contact
    // grant. Reopen is additionally guarded by the ticket's status name;
    // Attach hides when the contact lacks the cap.
    let can_reopen = crate::hooks::capabilities::use_capability("tickets:reopen");
    let can_attach = crate::hooks::capabilities::use_capability("tickets:attach_file");
    // MAPPS-609: two new dual-plane caps introduced by PMS-937. Staff
    // and platform-admin sessions bypass `use_capability` unconditionally,
    // so both controls stay visible for them regardless of the contact
    // grant. `tickets:edit_own` gates the Description-card Edit button
    // for a contact and is additionally ownership-scoped
    // (`contact_can_edit_ticket`); `tickets:request_approval` gates the
    // sidebar "Request approval" affordance that fires
    // `POST /tickets/{id}/approvals/request`.
    let can_edit_own = crate::hooks::capabilities::use_capability("tickets:edit_own");
    let can_request_approval =
        crate::hooks::capabilities::use_capability("tickets:request_approval");
    // MAPPS-607: transient state for the reopen POST and the attach
    // upload. The reopen button doubles as its own spinner; the attach
    // flow uses a hidden `<input type="file">` triggered from a button
    // (labels retain default browser text which reads poorly on the
    // page's chrome).
    let mut reopen_submitting = use_signal(|| false);
    let mut reopen_error = use_signal(String::new);
    let mut attach_submitting = use_signal(|| false);
    let mut attach_error = use_signal(String::new);
    let ticket_id_for_reopen = props.id.clone();
    let ticket_id_for_attach = props.id.clone();
    // MAPPS-609: state for the contact-facing "Request approval" modal.
    // Server validates 1-2000 chars for `note`; the client applies the
    // same bounds via FormGuard's Required + max-length rules so a
    // caller sees the failure inline rather than the raw 422 envelope.
    let mut show_request_approval = use_signal(|| false);
    let mut request_approval_note = use_signal(String::new);
    let mut request_approval_note_error = use_signal(String::new);
    let mut request_approval_submitting = use_signal(|| false);
    let mut request_approval_error = use_signal(String::new);
    let ticket_id_for_request_approval = props.id.clone();
    // mokosh-contact-login: the legacy "Add Note" modal (`show_note_modal`)
    // retired here. MAPPS-610 replaced its bare Textarea with the shared
    // MarkdownEditor and moved the composer to the top of the Journal card
    // below, which is now the sole path for adding a note. See the MAPPS-594
    // test in `mapps594_in_page_edit_tests` for the "only the approvals modal
    // remains" pin. Contacts never see the note-type selector; the default
    // has to be `public` for them so the inline composer's submit does not
    // post an internal note (mokosh-server prompt 008 rejects it anyway).
    let default_note_type = if staff_only { "internal" } else { "public" };
    let mut note_type = use_signal(|| default_note_type.to_string());
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
            //
            // MAPPS-605: get_authed_any tries the contact bearer first,
            // falls back to staff. Prior get_authed only read the staff
            // bearer, so a contact clicking a ticket from the Dashboard
            // recent-activity list failed client-side with "not
            // authenticated" before the request even left the browser.
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed_any::<RemoteTicketDetail>(&format!(
                "/tickets/{id}"
            ))
            .await
            .ok()
        }
    });
    let id_for_notes = props.id.clone();
    let notes_resource = use_resource(move || {
        let id = id_for_notes.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-605: contact-plane GET is dual-planed on the server
            // (redacts internal notes for contacts, keeps them for
            // staff). Pick the caller's bearer via get_authed_any so a
            // contact hitting the ticket detail page from the Dashboard
            // gets the redacted view instead of a client-side auth error.
            crate::hooks::fetch::api::get_authed_any::<Paginated<RemoteNote>>(&format!(
                "/tickets/{id}/notes"
            ))
            .await
            .ok()
            .map(|p| p.data)
        }
    });
    let id_for_time = props.id.clone();
    let time_resource = use_resource(move || {
        let id = id_for_time.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            // MAPPS-605: time entries are staff-only per prompt 008 scope
            // enforcement. Skip the fetch entirely on a contact session so
            // the SPA doesn't chase a guaranteed 403 (or client-side "not
            // authenticated" when no staff bearer).
            #[cfg(feature = "web")]
            if crate::hooks::fetch::api::has_contact_session() {
                return Some(Vec::<RemoteTimeEntry>::new());
            }
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
    // MAPPS-606: statuses + priorities are tenant-wide lookup lists
    // and must be reachable on a contact session so the Details
    // sidebar's Status + Priority pills render the correct labels
    // (Select maps the ticket's status_id / priority_id to an option
    // label; with an empty options list the pill renders blank).
    // get_authed_any tries the contact bearer first, falls back to
    // staff.
    let statuses_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed_any::<Paginated<RemoteTicketStatus>>(
            "/tickets/statuses",
        )
        .await
        .ok()
        .map(|p| p.data)
        .unwrap_or_default()
    });
    let priorities_resource = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed_any::<Paginated<RemoteTicketPriority>>(
            "/tickets/priorities",
        )
        .await
        .ok()
        .map(|p| p.data)
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
    // PMS-518: per-field inline errors for the in-page ticket editor's required
    // Title + Description, surfaced in each field's own slot by the FormGuard
    // in `on_save`.
    let mut e_title_error = use_signal(String::new);
    let mut e_desc_error = use_signal(String::new);
    let mut e_submitting = use_signal(|| false);
    let mut e_error = use_signal(String::new);
    // MAPPS-594: Cancel asks first, but only when there is something to lose.
    let mut confirming_cancel = use_signal(|| false);
    // What the editor opened with, so "dirty" survives a refetch of the ticket
    // and so retyping a value back to what it was stops being dirty.
    let mut e_baseline_title = use_signal(String::new);
    let mut e_baseline_desc = use_signal(String::new);
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
    // MAPPS-594: `read_unchecked`, deliberately, and NOT because it is best.
    //
    // It skips the reactive subscription, so `restart()` refetches the ticket
    // and this component does not re-render from the result: after a save the
    // page shows the OLD description until a reload. That is a real defect and
    // it is not fixed here, because both obvious fixes are worse and were
    // measured to be, in a browser against a real server:
    //
    //   * `read()` suspends while the fetch is in flight, which aborts this
    //     render part-way through its hook list. The next render then panics in
    //     dioxus-core with "Unable to retrieve the hook that was initialized at
    //     this index" and the page renders nothing at all.
    //   * `value().read()` neither suspends nor re-renders, and additionally
    //     stopped the Edit button opening the editor.
    //
    // `read_unchecked` on a resource is the pattern this whole codebase uses
    // (13 reads in contracts.rs, 13 in assets.rs, and so on), so the staleness
    // is app-wide rather than this page's, and picking at it inside a UX ticket
    // is how a layout change takes a detail page down. Filed separately.
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
    // MAPPS-593: who is looking, so the journal knows which notes carry an Edit
    // control. Read once here rather than per entry.
    //
    // MAPPS-602: this HAS to sit above the two early returns below. `use_auth`
    // is `use_context`, which is a hook, and a hook after a `return` runs on
    // some renders and not others. The render where `ticket_fetch_failed` is
    // true then leaves the component one hook short, and the next render panics
    // in dioxus-core with "Unable to retrieve the hook that was initialized at
    // this index". WASM does not unwind, so that panic poisons the runtime: the
    // page stops responding entirely, and a save that reaches the database goes
    // on rendering the old value, which reads as a stale-data bug and is not
    // one.
    let (viewer_id, viewer_is_admin) = {
        let auth = crate::hooks::use_auth();
        let a = auth.read();
        (
            a.user.as_ref().map(|u| u.id),
            a.has_role(crate::modules::auth::UserRole::Admin)
                || a.has_role(crate::modules::auth::UserRole::SuperAdmin),
        )
    };
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

    // MAPPS-594: is there unsaved work in the in-page editor?
    //
    // A `use_memo` whose body reads only a plain local computes once and never
    // again, because it has no reactive dependency to re-run on; the guard would
    // then be stuck on whatever the first render decided. Every input here is a
    // signal, so the memo actually tracks.
    let editor_dirty = use_memo(move || {
        editing_desc()
            && (*e_title.read() != *e_baseline_title.read()
                || *e_desc.read() != *e_baseline_desc.read())
    });
    // Covers the browser-level exits: reload, tab close. It does NOT cover an
    // in-app route change, because `beforeunload` never fires on one; that gap
    // belongs to the router and is why Cancel asks separately below. The modal
    // this replaced could not be navigated away from at all, so the risk is
    // created by editing in the page and is answered here rather than inherited.
    crate::hooks::use_unsaved_guard(editor_dirty.into());

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

    // MAPPS-594: the save path is unchanged from the modal it replaced;
    // only where the fields live changed. Hoisted out of the modal block so
    // the Description card can drive it.
    let mut ticket_res = ticket_resource;
    let mut history_res = history_resource;
    let save_id = id_for_save.clone();
    let on_save = move |_: MouseEvent| {
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
        e_title_error.set(guard.field("edit-title", &title_v, "Title", &[Rule::Required]));
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
            let path = format!("/tickets/{save_id}");
            // MAPPS-609 (mokosh-contact-login): contact sessions hit
            // `PATCH /tickets/{id}` (server accepts only title + description
            // on that verb and verifies the caller is the reporter); staff
            // sessions keep the existing PUT so priority/status/etc callers
            // that share this endpoint shape aren't affected.
            #[cfg(feature = "web")]
            let is_contact = crate::hooks::fetch::api::has_contact_session();
            #[cfg(not(feature = "web"))]
            let is_contact = false;
            let result: Result<(), String> = if is_contact {
                #[cfg(feature = "web")]
                {
                    crate::hooks::fetch::api::patch_authed_any_typed::<serde_json::Value, _>(
                        &path, &body,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|e| e.user_message())
                }
                #[cfg(not(feature = "web"))]
                {
                    Err("Editing tickets is only available in the browser.".to_string())
                }
            } else {
                crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(&path, &body)
                    .await
                    .map(|_| ())
            };
            match result {
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

    // PMS-362: carry the ticket into the Log Time flow so the work-item picker
    // opens preselected. A plain <a href> (not a routed Link) because the
    // TimeEntryNew route declares no query params, so a Link would strip
    // `?ticket_id=`; the router still intercepts the same-origin anchor click.
    let log_time_href = format!("/time/new?ticket_id={}", props.id);

    // MAPPS-607: Reopen renders only when the ticket landed in a closed/
    // resolved state AND the caller holds `tickets:reopen`. The status
    // check reuses the pure helper at the top of this file so the
    // "closed" / "resolved" name-match stays testable on the native
    // target. `ticket` is still `Option<_>` here (fetch may not have
    // resolved), so a loading page renders no button.
    let show_reopen = ticket
        .as_ref()
        .map(|t| should_show_reopen(&t.status.name, can_reopen))
        .unwrap_or(false);

    rsx! {
        PageHeader {
            title: "{header_title}",
            // MAPPS-594: while editing, the title is edited where the title is.
            // The reference in the report does exactly this, and it is what
            // makes an in-page edit read as editing the ticket rather than
            // editing a copy of it behind a scrim.
            title_slot: editing_desc().then(|| rsx! {
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
            }),
            // PMS-746: a route back to the list, matching ContractDetailPage.
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Tickets", Route::TicketList {}, &header_title),
                }
            },
            // MAPPS-517: no "Add Note" button here any more. The composer is
            // open in the journal below, so a note takes typing, not a click
            // that opens a modal first.
            // MAPPS-607: Reopen renders only when the ticket is in a closed
            // state AND the caller holds `tickets:reopen`; staff always pass.
            actions: rsx! {
                if show_reopen {
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *reopen_submitting.read(),
                        // MAPPS-357 parity: block reopening while the server is down.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't reopen while the server is unreachable".to_string()),
                        onclick: move |_| {
                            if *reopen_submitting.read() {
                                return;
                            }
                            reopen_error.set(String::new());
                            reopen_submitting.set(true);
                            let id = ticket_id_for_reopen.clone();
                            let mut tr = ticket_resource;
                            let mut hr = history_resource;
                            spawn(async move {
                                #[cfg(feature = "web")]
                                {
                                    let path = format!("/tickets/{id}/reopen");
                                    let empty = serde_json::json!({});
                                    match crate::hooks::fetch::api::post_authed_any_typed::<
                                        serde_json::Value,
                                        _,
                                    >(&path, &empty)
                                    .await
                                    {
                                        Ok(_) => {
                                            tr.restart();
                                            hr.restart();
                                            crate::hooks::toast::push_toast(
                                                crate::components::AlertType::Success,
                                                "Ticket reopened.",
                                            );
                                        }
                                        Err(err) => {
                                            reopen_error
                                                .set(format!("Could not reopen ticket: {}", err.user_message()));
                                        }
                                    }
                                }
                                #[cfg(not(feature = "web"))]
                                let _ = &id;
                                reopen_submitting.set(false);
                            });
                        },
                        "Reopen"
                    }
                }
                // mokosh-contact-login: the standalone "Add Note" button that
                // opened `show_note_modal` retired here. MAPPS-610 moved the
                // composer to the top of the Journal card below, so it is
                // already the first thing on the page: a second button in the
                // header that opens a modal wrapping the same composer would
                // be a no-op the reader has to reason about. See the MAPPS-594
                // test in `mapps594_in_page_edit_tests` for the pin. The
                // `public` default for contacts moved into `note_type`'s
                // initial value above.
                // MAPPS-607: Attach file. Rendered next to the composer so
                // the composer surface holds every content-add control
                // together. The button triggers the hidden
                // `<input type="file">` further down (browser file
                // pickers require an actual `input` element in the
                // DOM; a synthesised click on a detached input is
                // silently swallowed on Safari).
                if can_attach {
                    Button {
                        variant: ButtonVariant::Secondary,
                        loading: *attach_submitting.read(),
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't attach a file while the server is unreachable".to_string()),
                        onclick: move |_| {
                            attach_error.set(String::new());
                            #[cfg(target_arch = "wasm32")]
                            {
                                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                    if let Some(el) = doc.get_element_by_id("mapps-607-attach-input") {
                                        use wasm_bindgen::JsCast;
                                        if let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() {
                                            // Clear so re-selecting the same file
                                            // still fires `change` (browsers
                                            // dedupe the identical FileList).
                                            input.set_value("");
                                            input.click();
                                        }
                                    }
                                }
                            }
                        },
                        "Attach file"
                    }
                }
                if staff_only {
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
                }
            },
        }
        // MAPPS-607: surface reopen / attach failures inline just below
        // the header actions so the caller does not lose their place
        // scrolling the sidebar. Success paths self-clear these signals
        // before the next click.
        if !reopen_error.read().is_empty() {
            ErrorBanner { class: "mb-3", "{reopen_error}" }
        }
        if !attach_error.read().is_empty() {
            ErrorBanner { class: "mb-3", "{attach_error}" }
        }
        // MAPPS-607: hidden file input backing the Attach button. The
        // button's `onclick` synthesises a click on this input, and its
        // `onchange` reads the FileData, base64-encodes the bytes, and
        // POSTs `/tickets/{id}/attachments`. Kept in the DOM (rather
        // than created on demand) so the click event actually opens
        // the picker on Safari, which discards synthetic clicks on
        // detached inputs.
        if can_attach {
            crate::components::FileField {
                name: "mapps-607-attach-input".to_string(),
                hidden: true,
                onchange: move |evt: FormEvent| {
                    if *attach_submitting.read() {
                        return;
                    }
                    let Some(file) = evt.files().into_iter().next() else {
                        return;
                    };
                    let filename = file.name();
                    let content_type = file
                        .content_type()
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let size = file.size();
                    if size > TICKET_ATTACHMENT_MAX_BYTES {
                        attach_error.set("File too large".to_string());
                        return;
                    }
                    attach_submitting.set(true);
                    attach_error.set(String::new());
                    let id = ticket_id_for_attach.clone();
                    let mut nr = notes_resource;
                    spawn(async move {
                        #[cfg(feature = "web")]
                        {
                            use base64::{engine::general_purpose::STANDARD, Engine as _};
                            match file.read_bytes().await {
                                Ok(bytes) => {
                                    let data_base64 = STANDARD.encode(&bytes);
                                    let body = serde_json::json!({
                                        "filename": filename,
                                        "content_type": content_type,
                                        "data_base64": data_base64,
                                    });
                                    let path = format!("/tickets/{id}/attachments");
                                    match crate::hooks::fetch::api::post_authed_any_typed::<
                                        serde_json::Value,
                                        _,
                                    >(&path, &body)
                                    .await
                                    {
                                        Ok(_) => {
                                            nr.restart();
                                            crate::hooks::toast::push_toast(
                                                crate::components::AlertType::Success,
                                                "File attached.",
                                            );
                                        }
                                        Err(err) => {
                                            attach_error.set(format!(
                                                "Could not attach file: {}",
                                                err.user_message()
                                            ));
                                        }
                                    }
                                }
                                Err(_) => {
                                    attach_error
                                        .set("Could not read the selected file.".to_string());
                                }
                            }
                        }
                        #[cfg(not(feature = "web"))]
                        let _ = (&id, &filename, &content_type);
                        attach_submitting.set(false);
                    });
                },
            }
        }
        // MAPPS-594: Cancel used to be a modal's footer button, and a modal
        // cannot be navigated away from, so discarding was always deliberate.
        // Editing in the page removes that guarantee: a sidebar link, a
        // breadcrumb or a journal link would drop a reworked description with no
        // confirmation and no way back. `use_unsaved_guard` above does not cover
        // it, because `beforeunload` never fires on an in-app route change.
        //
        // Only when there is something to lose. Cancelling an untouched editor
        // still leaves immediately, because a confirmation there is a dialog
        // whose answer is always the same.
        crate::components::ConfirmDialog {
            open: confirming_cancel(),
            title: "Discard your changes?".to_string(),
            message: "The edits to this ticket have not been saved. Discarding them cannot be undone."
                .to_string(),
            confirm_text: "Discard".to_string(),
            cancel_text: "Keep editing".to_string(),
            destructive: true,
            oncancel: move |_| confirming_cancel.set(false),
            onconfirm: move |_| {
                confirming_cancel.set(false);
                e_error.set(String::new());
                e_title_error.set(String::new());
                e_desc_error.set(String::new());
                editing_desc.set(false);
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
                            #[cfg(feature = "app")]
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
                            #[cfg(not(feature = "app"))]
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
                    // MAPPS-609: on a contact session, the Edit button
                    // is ownership-scoped - it renders only when this
                    // contact reported the ticket AND holds
                    // `tickets:edit_own`. Staff sessions bypass
                    // `use_capability` unconditionally, so `can_edit_own`
                    // is `true` for them and the existing staff Edit
                    // behavior is preserved.
                    #[cfg(feature = "web")]
                    let my_contact_id = crate::hooks::fetch::api::current_contact_id();
                    #[cfg(not(feature = "web"))]
                    let my_contact_id: Option<uuid::Uuid> = None;
                    #[cfg(feature = "web")]
                    let is_contact_session =
                        crate::hooks::fetch::api::has_contact_session();
                    #[cfg(not(feature = "web"))]
                    let is_contact_session = false;
                    let reporter_contact_id =
                        ticket.as_ref().and_then(|t| t.reporter_contact_id);
                    let show_edit = if is_contact_session {
                        contact_can_edit_ticket(
                            reporter_contact_id,
                            my_contact_id,
                            can_edit_own,
                        )
                    } else {
                        // Staff / platform-admin path: the button has
                        // always rendered here on staff sessions since
                        // PMS-182, gated only on the ticket having loaded.
                        true
                    };
                    let open_edit = move |_| {
                        e_title.set(cur_title.clone());
                        e_desc.set(cur_desc.clone());
                        // The baseline is what the editor opened with, which is
                        // what "unchanged" means for the rest of this edit.
                        e_baseline_title.set(cur_title.clone());
                        e_baseline_desc.set(cur_desc.clone());
                        e_error.set(String::new());
                        editing_desc.set(true);
                    };
                    let marker = desc_edited.clone();
                    rsx! {
                        Card {
                            title: "Description",
                            // MAPPS-594: no Edit button while editing. The Save
                            // and Cancel pair at the foot of the editor is the
                            // whole control surface for the edit, and a second
                            // way in from the header would be a no-op the reader
                            // has to reason about.
                            // MAPPS-609: on a contact session, the Edit button
                            // is ownership-scoped via `show_edit`; staff always
                            // pass through. `show_edit` gates the button itself
                            // rather than the wrapping condition so the outer
                            // "actions slot open when the ticket is loaded and
                            // not being edited" invariant stays legible - see
                            // the MAPPS-594 test in `mapps594_in_page_edit_tests`.
                            actions: if ticket_loaded && !editing_desc() {
                                if show_edit {
                                    Some(rsx! {
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            // MAPPS-357: block editing while the server is down.
                                            disabled: !can_mutate,
                                            title: (!can_mutate).then(|| "Can't edit while the server is unreachable".to_string()),
                                            onclick: open_edit,
                                            PencilIcon { size: IconSize::Small, class: "mr-1.5".to_string() }
                                            "Edit"
                                        }
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            },
                            if editing_desc() {
                                // MAPPS-594: the editor is here, in the page,
                                // where the description it replaces was. The
                                // sidebar, the journal and the change history
                                // stay readable beside it, which is the point:
                                // the author is reworking a long document
                                // against material a modal used to cover.
                                div { class: "space-y-3",
                                    if !e_error().is_empty() {
                                        p { class: "text-sm text-red-600 dark:text-red-400", "{e_error}" }
                                    }
                                    crate::components::MarkdownEditor {
                                        name: "edit-description".to_string(),
                                        label: "Description".to_string(),
                                        // The card is already titled
                                        // "Description"; a second copy of the
                                        // word between the two is noise. The
                                        // label still names the field in a
                                        // validation message and to a screen
                                        // reader.
                                        label_hidden: true,
                                        // Sized for a page rather than for a
                                        // 672px dialog, which is what the report
                                        // was about.
                                        rows: 24,
                                        views: true,
                                        view_pref_key: "ticket_desc_view_mode".to_string(),
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
                                    div { class: "flex justify-end gap-2",
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            onclick: move |_| {
                                                // Only asks when there is
                                                // something to lose; a
                                                // confirmation whose answer is
                                                // always the same is a dialog
                                                // nobody reads.
                                                if editor_dirty() {
                                                    confirming_cancel.set(true);
                                                } else {
                                                    e_error.set(String::new());
                                                    editing_desc.set(false);
                                                }
                                            },
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
                                    }
                                }
                            } else if let Some(t) = ticket.as_ref() {
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
                //
                // MAPPS-606: hide the Approvals section entirely on a
                // contact session. The staff /approvals fetch + the
                // /auth/users users_resource fetch both require a
                // staff bearer (approvals is a staff workflow contacts
                // have no cap for). Rendering the card only to show
                // "Could not load approvals" is noise; the workflow is
                // not one the contact can participate in.
                if !crate::hooks::fetch::api::has_contact_session() {
                    ApprovalsSection { entity_id: props.id.clone() }
                }

                // MAPPS-517: the journal. The composer sits at the top of it,
                // open, and the stream below carries every source this page
                // fetches rather than notes alone.
                Card { title: "Journal",
                    div { class: "space-y-4",
                        if !note_error.read().is_empty() {
                            ErrorBanner { "{note_error}" }
                        }
                        // MAPPS-610: the same editor as the description, so a
                        // note can carry a list, a table or an `@handle`
                        // instead of being the one Markdown field on this page
                        // with no help writing it.
                        crate::components::MarkdownEditor {
                            name: "content".to_string(),
                            label: "Add a note".to_string(),
                            placeholder: "Enter your note…".to_string(),
                            rows: 4,
                            views: true,
                            view_pref_key: "ticket_note_view_mode".to_string(),
                            required: true,
                            rules: vec![Rule::Required],
                            error: note_content_error.read().clone(),
                            value: note_content.read().clone(),
                            people: crate::hooks::mention_people(&mention_directory),
                            oninput: move |next: String| {
                                note_content_error.set(String::new());
                                note_content.set(next);
                            },
                        }
                        div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                            Select {
                                name: "note_type",
                                label: "Note Type",
                                options: note_type_options(),
                                value: note_type.read().clone(),
                                onchange: move |e: FormEvent| {
                                    // Only a public note ever leaves the building,
                                    // whatever the flag says (mokosh-server
                                    // `add_note`). MAPPS-613: the checkbox is now
                                    // absent on every other type, so this clear is
                                    // what stops a flag set while public from
                                    // surviving into a note nobody can see it on.
                                    if e.value() != "public" {
                                        note_send_email.set(false);
                                    }
                                    note_type.set(e.value());
                                },
                            }
                            // MAPPS-613: absent, not greyed, on a note that
                            // cannot be emailed. The server has always refused
                            // to send one, but that refusal is invisible: a
                            // disabled control on an internal note still tells
                            // the reader that internal commentary is the sort
                            // of thing this app can mail to a customer.
                            if note_is_public {
                                div { class: "flex items-end",
                                    Checkbox {
                                        name: "note_send_email",
                                        label: "Email this note to the client",
                                        checked: note_send_email(),
                                        help: NOTE_EMAIL_HELP.to_string(),
                                        onchange: move |e: FormEvent| note_send_email.set(e.checked()),
                                    }
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
                                onclick: {
                                    let ticket_id_for_note = ticket_id_for_note.clone();
                                    move |_| {
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
                                        #[cfg(feature = "app")]
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
                                }
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
                // MAPPS-609: contact-facing "Request approval" affordance.
                // Sits in its own Actions card at the top of the sidebar
                // so it lives outside the staff Approvals section (MAPPS-606
                // hides that section entirely on contact sessions). Staff
                // and platform-admin sessions bypass `use_capability`
                // unconditionally, so the button also renders for them; the
                // shared `post_authed_any_typed` endpoint accepts either
                // bearer.
                //
                // mokosh-contact-login (MAPPS-594 pin): the request note used
                // to live in a separate Modal, which drifted the "only the
                // approvals modal remains" count. The form now expands inline
                // inside this Actions card: the same fields, same handler,
                // same endpoint, but the reader stays on the page they were
                // reading.
                if can_request_approval {
                    Card { title: "Actions",
                        if *show_request_approval.read() {
                            div { class: "space-y-3",
                                if !request_approval_error.read().is_empty() {
                                    ErrorBanner { "{request_approval_error}" }
                                }
                                p { class: "text-xs text-subtle",
                                    "Add a short note explaining what you're asking your MSP to review (1-2000 characters)."
                                }
                                Textarea {
                                    name: "request-approval-note",
                                    label: "Note",
                                    placeholder: "What would you like your MSP to review?",
                                    rows: 5,
                                    required: true,
                                    rules: vec![Rule::Required, Rule::MaxLen(2000)],
                                    error: request_approval_note_error.read().clone(),
                                    value: request_approval_note.read().clone(),
                                    oninput: move |e: FormEvent| {
                                        request_approval_note_error.set(String::new());
                                        request_approval_note.set(e.value());
                                    },
                                }
                                div { class: "flex justify-end gap-2",
                                    Button {
                                        variant: ButtonVariant::Secondary,
                                        onclick: move |_| show_request_approval.set(false),
                                        "Cancel"
                                    }
                                    Button {
                                        variant: ButtonVariant::Primary,
                                        loading: *request_approval_submitting.read(),
                                        // MAPPS-357 parity: block the request POST while the server is down.
                                        disabled: !can_mutate,
                                        title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                                        onclick: move |_| {
                                            if *request_approval_submitting.read() {
                                                return;
                                            }
                                            request_approval_error.set(String::new());
                                            let mut guard = FormGuard::new();
                                            let note_v = request_approval_note.read().trim().to_string();
                                            request_approval_note_error.set(guard.field(
                                                "request-approval-note",
                                                &note_v,
                                                "Note",
                                                &[Rule::Required, Rule::MaxLen(2000)],
                                            ));
                                            if guard.blocked() {
                                                return;
                                            }
                                            let id = ticket_id_for_request_approval.clone();
                                            request_approval_submitting.set(true);
                                            spawn(async move {
                                                #[cfg(feature = "web")]
                                                {
                                                    let body = serde_json::json!({ "note": note_v });
                                                    let path = format!("/tickets/{id}/approvals/request");
                                                    match crate::hooks::fetch::api::post_authed_any_typed::<
                                                        serde_json::Value,
                                                        _,
                                                    >(&path, &body)
                                                    .await
                                                    {
                                                        Ok(_) => {
                                                            crate::hooks::toast::push_toast(
                                                                crate::components::AlertType::Success,
                                                                "Approval requested. Your MSP will follow up.",
                                                            );
                                                            request_approval_note.set(String::new());
                                                            show_request_approval.set(false);
                                                        }
                                                        Err(err) => {
                                                            request_approval_error.set(format!(
                                                                "Could not request approval: {}",
                                                                err.user_message()
                                                            ));
                                                        }
                                                    }
                                                }
                                                #[cfg(not(feature = "web"))]
                                                let _ = &id;
                                                request_approval_submitting.set(false);
                                            });
                                        },
                                        "Send request"
                                    }
                                }
                            }
                        } else {
                            div { class: "flex flex-col gap-2",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    // MAPPS-357 parity: block the request POST while the server is down.
                                    disabled: !can_mutate,
                                    title: (!can_mutate).then(|| "Can't request an approval while the server is unreachable".to_string()),
                                    onclick: move |_| {
                                        request_approval_error.set(String::new());
                                        request_approval_note_error.set(String::new());
                                        request_approval_note.set(String::new());
                                        show_request_approval.set(true);
                                    },
                                    "Request approval"
                                }
                            }
                        }
                    }
                }
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
                                                // Prompt 006: contacts see the value but cannot mutate it.
                                                disabled: !can_mutate || !staff_only,
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
                                                // Prompt 006: contacts see the value but cannot mutate it.
                                                disabled: !can_mutate || !staff_only,
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
                                                // Prompt 006: contacts see the value but cannot mutate it.
                                                disabled: !can_mutate || !staff_only,
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

        // mokosh-contact-login: the "Add Note" modal that used to live
        // here (`show_note_modal`) retired with the header button that
        // opened it. MAPPS-610 moved the composer into the Journal card
        // above and swapped the bare Textarea for the shared
        // MarkdownEditor, which is now the only path for adding a
        // note. The MAPPS-594 pin in `mapps594_in_page_edit_tests`
        // enforces the "only the approvals modal remains" invariant.

        // mokosh-contact-login (MAPPS-594 pin): the standalone MAPPS-609
        // "Request approval" Modal that used to live here retired with the
        // outer ticket-page cover. The same form (fields, handler,
        // dual-plane endpoint) now expands inline inside the sidebar
        // Actions card above, so the reader stays on the ticket they were
        // reading. `mapps594_in_page_edit_tests` enforces the "only the
        // approvals modal remains" invariant (the sole surviving Modal is
        // the staff `ApprovalsSection` request-approver picker).
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
    // MAPPS-610: the directory the inline editor completes `@` against. The
    // hook shares one fetch across the page, so a journal of thirty entries
    // still makes one request. MAPPS-602: it is a hook, so it sits with the
    // others at the top, above anything that can return early.
    let mention_directory = crate::hooks::use_mention_directory(true);

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
                                    // MAPPS-610: the same editor the composer
                                    // has. A correction is written the same way
                                    // the note was.
                                    crate::components::MarkdownEditor {
                                        // `format!`, not a literal with braces
                                        // in it: the id has to be unique per
                                        // entry or every inline editor in the
                                        // journal answers to the same one, and
                                        // the toolbar addresses the field by it.
                                        name: format!("edit-note-{note_dom_id}"),
                                        label: "Edit note".to_string(),
                                        rows: 4,
                                        views: true,
                                        view_pref_key: "ticket_note_view_mode".to_string(),
                                        required: true,
                                        rules: vec![Rule::Required],
                                        error: edit_error.read().clone(),
                                        value: draft.read().clone(),
                                        people: crate::hooks::mention_people(&mention_directory),
                                        oninput: move |next: String| {
                                            edit_error.set(String::new());
                                            draft.set(next);
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
                                // MAPPS-610: Markdown, not raw text in a
                                // `whitespace-pre-wrap` box. The composer now
                                // has a formatting toolbar, and a toolbar over
                                // a plain-text renderer posts `**bold**` and
                                // shows `**bold**`. The description has
                                // rendered this way since PMS-309.
                                //
                                // Consequence, taken deliberately: notes
                                // written before this are now parsed as
                                // Markdown, so a line that happens to start
                                // with `#` reads as a heading. The output is
                                // sanitized either way, and `@handle` now
                                // resolves (MAPPS-578), which is the upside.
                                div { class: "mt-2 bg-surface-2 rounded-md p-3",
                                    crate::components::Markdown { content: content.clone() }
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

/// MAPPS-607: pure-function tests for the Reopen button's visibility.
/// Kept native-safe (no `web_sys`, no `use_signal`), so they run under
/// `cargo test --lib`. Also pins the ticket-detail route resolution so
/// a future rename of the URL shape doesn't silently 404 the reopen
/// entry point.
#[cfg(test)]
mod mapps_607_tests {
    use super::{contact_can_edit_ticket, should_show_reopen, TICKET_ATTACHMENT_MAX_BYTES};
    use crate::Route;
    use std::str::FromStr;

    // Reopen visibility

    #[test]
    fn reopen_hidden_without_capability() {
        assert!(!should_show_reopen("Closed", false));
        assert!(!should_show_reopen("Resolved", false));
    }

    #[test]
    fn reopen_visible_on_closed_status_with_capability() {
        assert!(should_show_reopen("Closed", true));
        assert!(should_show_reopen("closed", true));
        assert!(should_show_reopen("Closed - won", true));
    }

    #[test]
    fn reopen_visible_on_resolved_status_with_capability() {
        assert!(should_show_reopen("Resolved", true));
        assert!(should_show_reopen("resolved (dup)", true));
    }

    #[test]
    fn reopen_hidden_on_open_status_even_with_capability() {
        assert!(!should_show_reopen("Open", true));
        assert!(!should_show_reopen("In Progress", true));
        assert!(!should_show_reopen("New", true));
        assert!(!should_show_reopen("", true));
    }

    // Attachment size cap
    #[test]
    fn attachment_max_is_five_megabytes() {
        assert_eq!(TICKET_ATTACHMENT_MAX_BYTES, 5 * 1024 * 1024);
    }

    // MAPPS-609: contact ownership gate for the Description-card Edit
    // button. Renders iff the contact holds `tickets:edit_own` AND the
    // ticket's reporter contact id matches the caller's own; every
    // other combination hides the button so a mis-configured JWT can
    // never surface a guaranteed 403.

    #[test]
    fn edit_hidden_without_edit_own_capability() {
        let mine = uuid::Uuid::from_u128(0x1);
        // Even a perfect ownership match must not open the door without
        // the capability - the cap is the necessary first gate.
        assert!(!contact_can_edit_ticket(Some(mine), Some(mine), false));
    }

    #[test]
    fn edit_visible_when_cap_and_ownership_match() {
        let mine = uuid::Uuid::from_u128(0x1);
        assert!(contact_can_edit_ticket(Some(mine), Some(mine), true));
    }

    #[test]
    fn edit_hidden_when_cap_but_ownership_mismatch() {
        let mine = uuid::Uuid::from_u128(0x1);
        let other = uuid::Uuid::from_u128(0x2);
        assert!(!contact_can_edit_ticket(Some(other), Some(mine), true));
    }

    #[test]
    fn edit_hidden_when_reporter_and_my_ids_missing() {
        // A pre-PMS-937 server that omits `reporter_contact_id` and a
        // pre-PMS-937 login response that never stashed `contact_id`
        // both fall through to `None`. Both must fail-closed.
        assert!(!contact_can_edit_ticket(None, None, true));
        let mine = uuid::Uuid::from_u128(0x1);
        assert!(!contact_can_edit_ticket(None, Some(mine), true));
        assert!(!contact_can_edit_ticket(Some(mine), None, true));
    }

    // Route resolution
    #[test]
    fn ticket_detail_route_resolves() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let r = Route::from_str(&format!("/tickets/{uuid}")).expect("ticket detail parses");
        match r {
            Route::TicketDetail { id } => assert_eq!(id, uuid),
            other => panic!("expected TicketDetail, got {other:?}"),
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

    /// MAPPS-592: both description fields are the KB write pane. MAPPS-610:
    /// so are both note fields.
    ///
    /// A ticket description is Markdown, is rendered as Markdown by the same
    /// component a KB article is, and was written in a bare textarea: the same
    /// syntax with none of the help. A note was further behind still - no
    /// toolbar at all - and it is the field on this page people actually spend
    /// the day in.
    #[test]
    fn every_markdown_field_on_this_page_gets_the_editor() {
        let code = code_only();
        assert_eq!(
            code.matches("crate::components::MarkdownEditor {").count(),
            4,
            "the create form, the in-page description edit, the note composer \
             and the inline note edit"
        );
        for bare in [
            "Textarea { name: \"description\",",
            "Textarea { name: \"edit-description\",",
            "Textarea { name: \"content\",",
        ] {
            assert!(
                !code.contains(bare),
                "{bare} is not a bare textarea any more"
            );
        }
    }

    /// MAPPS-610: a toolbar over a plain-text renderer posts `**bold**` and
    /// shows `**bold**`. The note renderer changes with the note editor.
    #[test]
    fn a_note_is_rendered_as_markdown_not_as_raw_text() {
        let code = code_only();
        assert!(
            code.contains("crate::components::Markdown { content: content.clone() }"),
            "the journal renders a note through the shared renderer"
        );
        assert!(
            !code.contains(r#"rounded-md p-3 whitespace-pre-wrap", "{content}""#),
            "and not as the raw string in a pre-wrap box"
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
            3,
            "one per component that hosts an editor: the list page's create form, \
             the detail page, and the journal entry with the inline note editor"
        );
        assert!(
            code.contains("people: crate::hooks::mention_people(&mention_directory)"),
            "and it is what the editor completes against"
        );
    }

    /// The description editor follows the same write gate as its Save button.
    /// MAPPS-357's rule: while the server is unreachable, a control that leads
    /// to a PUT should not invite the click. MAPPS-594 moved it out of the modal
    /// and into the page; the gate came with it.
    #[test]
    fn the_description_editor_is_disabled_with_the_rest_of_the_form() {
        let code = code_only();
        let modal = code
            .find("name: \"edit-description\".to_string()")
            .expect("the description editor");
        // Wide enough to clear the comments between the props: the assertion is
        // about the prop being present on this editor, not about its position.
        let window = &code[modal..code.len().min(modal + 900)];
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

#[cfg(test)]
mod mapps594_in_page_edit_tests {
    const SRC: &str = include_str!("tickets.rs");

    /// The shipping code with runs of whitespace collapsed, excluding this
    /// module: every assertion quotes the pattern it looks for.
    fn code_only() -> String {
        let end = SRC
            .find("mod mapps594_in_page_edit_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// MAPPS-594: the edit modal is gone, not resized.
    ///
    /// `ModalSize::Full` exists and would have taken the panel to `max-w-7xl`
    /// in one enum value. That is the cheap answer and the wrong one: a wider
    /// cover is still a cover, and what the author is editing against is the
    /// page underneath. Pinned because "make the modal bigger" is the obvious
    /// thing for a later change to reach for.
    #[test]
    fn the_edit_modal_is_gone_rather_than_bigger() {
        let code = code_only();
        assert!(
            !code.contains(r#"title: "Edit Ticket""#),
            "no Edit Ticket modal"
        );
        assert!(
            !code.contains("ModalSize::Full"),
            "and it was not simply widened"
        );
        // The one Modal left on this page is the approvals request, which is a
        // short self-contained task and is what a modal is for.
        assert_eq!(
            code.matches("Modal { open:").count(),
            1,
            "only the approvals modal remains: {code:?}"
        );
    }

    /// The editor renders in the Description card, where the description it
    /// replaces was, and the title in the header where the title was.
    #[test]
    fn the_editor_renders_in_the_page() {
        let code = code_only();
        let card = code
            .find(r#"Card { title: "Description""#)
            .expect("the Description card");
        let window = &code[card..code.len().min(card + 4000)];
        assert!(
            window.contains("if editing_desc() {"),
            "the card switches into edit mode"
        );
        assert!(
            window.contains("crate::components::MarkdownEditor {"),
            "and the editor is what it switches to"
        );
        assert!(
            code.contains("title_slot: editing_desc().then("),
            "the title is edited where the title is"
        );
    }

    /// A second way into an edit that is already open is a no-op the reader has
    /// to reason about.
    #[test]
    fn the_edit_button_hides_while_editing() {
        let code = code_only();
        assert!(
            code.contains("actions: if ticket_loaded && !editing_desc() {"),
            "the header Edit button is not offered during an edit"
        );
    }

    /// A modal cannot be navigated away from; a page can. That risk is created
    /// by this change rather than inherited, so both halves of the answer are
    /// pinned: the browser-level exits and the in-app one.
    #[test]
    fn unsaved_work_is_guarded_both_ways() {
        let code = code_only();
        assert!(
            code.contains("crate::hooks::use_unsaved_guard(editor_dirty.into())"),
            "reload and tab close warn"
        );
        assert!(
            code.contains("confirming_cancel.set(true)"),
            "and Cancel asks before discarding"
        );
        assert!(
            code.contains("ConfirmDialog { open: confirming_cancel()"),
            "through a real confirmation rather than a native confirm()"
        );
    }

    /// Only when there is something to lose. A confirmation whose answer is
    /// always the same is a dialog nobody reads, so an untouched editor closes
    /// immediately.
    #[test]
    fn cancelling_an_untouched_editor_does_not_ask() {
        let code = code_only();
        let cancel = code
            .find("if editor_dirty() { confirming_cancel.set(true); } else {")
            .expect("Cancel is conditional on there being changes");
        let window = &code[cancel..code.len().min(cancel + 200)];
        assert!(
            window.contains("editing_desc.set(false)"),
            "an untouched editor closes straight away: {window}"
        );
    }

    /// The dirty flag has to TRACK. A `use_memo` whose body reads only a plain
    /// local computes once and never again, because it has no reactive
    /// dependency to re-run on, and the guard would then be stuck on whatever
    /// the first render decided. Every input is a signal read.
    #[test]
    fn the_dirty_flag_is_computed_from_signals() {
        let code = code_only();
        let memo = code
            .find("let editor_dirty = use_memo(move || {")
            .expect("the dirty memo");
        let window = &code[memo..code.len().min(memo + 300)];
        for read in [
            "editing_desc()",
            "*e_title.read()",
            "*e_baseline_title.read()",
            "*e_desc.read()",
            "*e_baseline_desc.read()",
        ] {
            assert!(window.contains(read), "the memo must read {read}: {window}");
        }
    }

    /// Nothing about what is saved changed: the same PUT, the same guard, the
    /// same write gate. Only where the fields live did.
    #[test]
    fn the_save_path_is_the_one_the_modal_used() {
        let code = code_only();
        assert!(
            code.contains(r#"&format!("/tickets/{save_id}")"#),
            "the same PUT the modal made"
        );
        assert!(
            code.contains(r#"guard.field("edit-title","#),
            "the title still validates through the shared FormGuard"
        );
        assert!(
            code.contains(r#"guard.field( "edit-description","#),
            "and so does the description"
        );
    }
}

#[cfg(test)]
mod mapps613_note_type_and_email_affordance_tests {
    use super::*;

    const SRC: &str = include_str!("tickets.rs");

    /// The shipping code with runs of whitespace collapsed, excluding this
    /// module: every assertion quotes the pattern it looks for, so a scan
    /// including its own source would match itself and pass regardless.
    fn code_only() -> String {
        let end = SRC
            .find("mod mapps613_note_type_and_email_affordance_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn note(json: &str) -> RemoteNote {
        serde_json::from_str(json).expect("deserialise note")
    }

    /// Three of the four, and the omission is the deliberate half.
    ///
    /// `resolution` was storable, editable and renderable all along and could
    /// not be composed. `time_entry` must stay out: nothing writes one, and
    /// the server refuses to edit one because "a time-entry note is edited
    /// through its time entry", so a hand-written one belongs to an entry that
    /// does not exist and nobody can then correct it.
    #[test]
    fn only_a_type_an_agent_may_write_is_offered() {
        let offered: Vec<(String, String)> = note_type_options()
            .into_iter()
            .map(|o| (o.value, o.label))
            .collect();
        assert_eq!(
            offered,
            vec![
                ("internal".to_string(), "Internal Note".to_string()),
                (
                    "public".to_string(),
                    "Public Note (visible to customer)".to_string()
                ),
                (
                    "resolution".to_string(),
                    "Resolution Note (internal)".to_string()
                ),
            ]
        );
        assert!(
            composer_label(NoteType::TimeEntry).is_none(),
            "an agent cannot author a note about a time entry that does not exist"
        );
    }

    /// David's actual objection. The server has always refused to mail an
    /// internal note, but that refusal happens where nobody can see it: a
    /// greyed-out `Email this note to the client` on an internal note still
    /// says this app will send internal commentary to a customer if you ask
    /// the right way.
    #[test]
    fn the_email_control_is_absent_rather_than_disabled() {
        let code = code_only();
        assert!(
            code.contains("if note_is_public { div { class: \"flex items-end\", Checkbox {"),
            "the checkbox is rendered only on a public note"
        );
        assert!(
            !code.contains("disabled: !note_is_public"),
            "and never as a greyed-out control on a note that cannot be emailed"
        );
    }

    /// The flag has to be cleared on the way out of public, because the box
    /// that would otherwise show it checked is no longer on screen. Before
    /// this branch a stale flag was merely invisible-but-disabled; now it
    /// would be invisible outright, so the clear carries more weight than it
    /// did, and the submit's own `type == "public"` guard is the second line.
    #[test]
    fn leaving_public_clears_the_email_flag() {
        let code = code_only();
        assert!(
            code.contains("if e.value() != \"public\" { note_send_email.set(false); }"),
            "any type but public clears the flag, not just internal"
        );
        assert!(
            code.contains("let email_v = type_v == \"public\" && note_send_email();"),
            "and the submit re-checks it"
        );
    }

    /// The journal sentence is the only place a note's type reaches a reader.
    /// It read "internal, else public", which was true while those were the
    /// only two composable types and becomes a false claim about customer
    /// visibility the moment `resolution` joins them: the portal serves
    /// `note_type='public'` and nothing else.
    #[test]
    fn the_journal_never_calls_an_invisible_note_public() {
        let resolution = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000021","note_type":"resolution","content":"Replaced the PSU","created_by_name":"Dana Reeve","created_at":"2026-08-20T09:00:00Z"}"#,
        );
        let unknown = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000022","note_type":"something_new","content":"x","created_by_name":"Dana Reeve","created_at":"2026-08-20T08:00:00Z"}"#,
        );

        let journal = build_journal(&[resolution, unknown], &[], &[], &[], None, false);
        let actions: Vec<String> = journal.iter().map(|e| e.action.clone()).collect();

        assert_eq!(
            actions,
            vec![
                "added a resolution note (internal)".to_string(),
                "added a note".to_string(),
            ]
        );
        for action in &actions {
            assert!(
                !action.contains("public"),
                "neither note is visible to the customer, so neither line may say public: {action}"
            );
        }
    }

    /// The two lines that were already right stay right: this change must not
    /// move what a public note reads as.
    #[test]
    fn a_public_note_still_reads_exactly_as_it_did() {
        let emailed = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000023","note_type":"public","content":"x","created_by_name":"Dana Reeve","is_email_sent":true,"created_at":"2026-08-20T09:00:00Z"}"#,
        );
        let unsent = note(
            r#"{"id":"aaaaaaaa-0000-4000-8000-000000000024","note_type":"public","content":"x","created_by_name":"Dana Reeve","created_at":"2026-08-20T08:00:00Z"}"#,
        );

        let journal = build_journal(&[emailed, unsent], &[], &[], &[], None, false);
        let actions: Vec<String> = journal.iter().map(|e| e.action.clone()).collect();

        assert_eq!(
            actions,
            vec![
                "added a public note and emailed the client".to_string(),
                "added a public note (not emailed)".to_string(),
            ]
        );
    }
}

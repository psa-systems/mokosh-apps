# Codebase audit, 2026-05-06 - mokosh-apps

> **STALE - snapshot from 2026-05-06.** This is a record of one walk
> through the client on 2026-05-06, not a description of the client
> now. Read it for what that walk found and for the `F1..F19` ids that
> source comments and YouTrack issues still cite. Do not read it to
> learn what a page currently does.
>
> The client-portal SPA the walk documented has since been retired on
> the `mokosh-contact-login` line: the `/portal/*` rows and every
> "GET-leak P0" reference describe pages that no longer exist. Traffic
> at those URLs now reaches the contact-plane pages under
> `src/pages/contact_portal/*` (see `docs/mokosh-client-login/remaining.md`
> for the current punch list). The agent-side rows are also drifting: a
> lot of "501" / "stub" / "mock" entries below have shipped since. A
> re-audit is a follow-up; do not use this as an authoritative map of
> what does or doesn't work today.

The method was to walk every route in the router via Chrome (MCP
browser automation), clicking every button, link, row, input and
pagination control, and to record the result alongside the
static-analysis intent. That is worth keeping: the walk is expensive
to reproduce and nothing else in the tree records what it found.

What has changed since is not tracked here, and this file is not
updated when it does. Most of it is wrong by now. The client made no
API calls at all on the day of the audit; 25 of the 36 files in
`src/pages/` call an authed fetch helper today. Anything below that
reads as present tense is 2026-05-06 present tense.

Line-number citations have been removed rather than corrected
(MAPPS-540). A `file.rs:line` pointer into a tree that has moved on by
a year sends a reader to the wrong code with the confidence of a
precise reference; the file name alone is honest about what it can
offer.

## What the walk found, on 2026-05-06

| Metric | Value on the day of the audit |
| --- | --- |
| Total routes in router | **54** |
| Pages walked in browser | **54 / 54** |
| Fully working (no broken interactions) | 6 |
| Partial (renders, some interactions no-op) | **45** - the dominant category |
| Placeholder pages ("form would go here" stubs) | 5 |
| Broken (page errors / blank) | 0 |
| Critical bugs (data-loss / break-the-app) | **1** (the `/portal/tickets/new` GET-leak) |
| Sections with rich UI but zero backend | **14 of 18** |

On that day the client rendered mock data on every page: the Chrome
network panel showed no `/api/*` requests during normal navigation
across `/dashboard`, `/tickets` and `/portal/tickets/new`.

That is the single claim most worth not carrying forward. It has not
been true for a long time, and it is the one a reader is most likely
to act on: it says the HTTP layer does not exist, so anyone planning
work from it starts by building one that has been there for months.

## Cross-cutting findings

These appeared across many pages on 2026-05-06 and were judged best
fixed once at the component or infrastructure layer. Several have
since been fixed; the list is not maintained.

1. **No `data-testid` or `aria-label` on action buttons.** Test /
   automation selectors fall back to button text + DOM position.
   Adding `data-testid` to `Button` / `IconButton` / `TableRow`
   would make selectors stable.
   ([`src/components/button.rs`](../../src/components/button.rs))
2. **`TableRow { clickable: true }` with no `onclick` is everywhere.**
   The dominant broken interaction. The `clickable` prop styles the
   row as interactive (cursor-pointer, hover) but no
   navigation/onclick is wired unless the parent passes one. Many
   list pages use this with an inner cell `Link` to detail pages, so
   clicking row whitespace does nothing and only clicking exactly on
   the cell text navigates. Affected pages (~12): dashboard recent
   tickets, tickets list (also has empty closure), companies list,
   companies detail (sub-tables), contacts list, contracts list,
   invoices list, assets list, assets detail (sub-table), KB
   articles list, admin tenants, portal ticket list (which
   additionally has no inner Link, so portal rows are completely
   inert).
3. **Mocked auth/data layer.** Login
   ([`src/hooks/auth.rs`](../../src/hooks/auth.rs)),
   `ForgotPasswordPage`, `ResetPasswordPage`, `TicketNewPage` all
   simulate success after a 1s delay with no API call. With ADMIN
   bypass, this is fine for dev rendering. `// TODO: Call API`
   markers at
   `src/pages/auth.rs` (since split into `login.rs`, `auth_callback.rs` and `portal_login.rs`),
   `auth.rs`,
   [`src/pages/tickets.rs`](../../src/pages/tickets.rs).
4. **Detail pages have hardcoded titles.** Every `*DetailPage`
   ignores its `props.id` and shows fixed sample text (always
   "TKT-1234: Email server not responding", "Acme Corp", "Bob
   Johnson", "Network Infrastructure Upgrade", "Managed Services
   Agreement", "INV-2025-001", "Exchange Server 01", "How to Reset
   a User's Password..."). The only detail page that varies by route
   param is `/reports/:report_type` (4-way match, default "Report").
5. **`<a href="#">` dead links scattered across pages.** Footer,
   related articles, contract documents, "Contact us" auth link,
   integration notification template links, KB related articles,
   asset RMM link, portal KB items. Either remove or convert to
   real Routes / `mailto:` / external URLs.
6. **Recurring "row of action buttons with no onclick" pattern on
   detail page headers.** Buttons rendered without handlers, so
   clicking them does nothing. Examples seen in browser: `Edit`,
   `Renew`, `Add Task`, `Pay Now`, `Send`, `Download PDF`, `Record
   Payment`, `Connect`, `Configure`, `Remote Connect`, `Submit
   Timesheet`, `Apply Filters`, `Today` (calendar/dispatch),
   `Schedule`, `Export PDF`, `Export CSV`, `New Appointment`,
   `Schedule Appointment`, `Add User`, `Add Team`, `Add Tenant`,
   page-prev/page-next chevron buttons.
7. **Form components have stub `oninput: |_| {}` in some pages.**
   Notably the Add Note modal in `TicketDetailPage` and the entire
   Submit Ticket form in `PortalTicketNewPage` have unbound inputs:
   the user can type but nothing is captured.
8. **Portal pages render the layout title prop AND the page-level
   `h1`/heading independently**, leading to visible duplicate titles
   ("Submit Ticket" + "Submit a Ticket", "Invoices" + "Invoices",
   "Knowledge Base" + "Knowledge Base", "My Tickets" + "My Tickets"
   - visible in `/portal/tickets`, `/portal/tickets/new`,
   `/portal/invoices`, `/portal/invoices/:id`, `/portal/kb`).
9. **HTML `<title>` is duplicated** ("Mokosh PlatformMokosh
   Platform") on every page. Likely caused by both the
   `Dioxus.toml` static title and the `AppLayout` / `PortalLayout`
   setting it.
10. **`/reports/:report_type` Date Range and Group By selects render
    blank** despite `selected: true` on default options in code
    ([`src/pages/reports.rs`](../../src/pages/reports.rs)).
    Dioxus 0.7 `<select>` rendering quirk - the `selected`
    attribute on `<option>` elements may not be applied correctly.

## The 27 most-impactful UI/UX issues

Ranked by visibility × user-trust impact. P0 / P1 issues are user-
facing and should be fixed before any v1 ship; P2 / P3 are polish.

### P0 - break-the-app or data-loss

1. **`/portal/tickets/new` form does a native GET submit on
   success.** The form has no `onsubmit`, so the browser submits
   to the same URL with `subject` + `description` + `priority` as
   query parameters. URL becomes
   `?subject=...&description=...&priority=medium`, then the SPA
   doesn't render anything for that URL state and **the page goes
   completely blank**. Users on this form lose their input AND
   their session. One-line fix: add `onsubmit: move |e| {
   e.prevent_default(); /* TODO POST */ }`. See
   [F8](#f8-portaltickenewpage-form-is-fully-decorative).

### P1 - most-visible affordance / wiring bugs

2. **Fake-link blue text.** At least two places (likely more) where
   text is styled link-blue but isn't a `Link`:
   - Dashboard Recent Tickets `TKT-1234..1230` ticket numbers
     ([`dashboard.rs`](../../src/pages/dashboard.rs))
   - Company detail Statistics "Open Tickets: **5**" value
     ([`contacts.rs`](../../src/pages/contacts.rs))

   Users click expecting navigation; nothing happens.
3. **`TableRow { clickable: true }` without `onclick` everywhere**
   - on ~12 list pages plus dashboard + asset detail sub-tables.
   Cursor turns to pointer on hover, but the row body is dead. Fix:
   either F1 (component-level patch suppressing `clickable` styling
   when no `onclick` provided) or wire navigation per-page.
4. **Stub-submit forms get stuck loading or do nothing visible.**
   `/tickets/new`, `/time/new`, `/projects/new`, `/companies/new`,
   `/contacts/new` all set `is_submitting=true` (some with a 1s
   delay) then leave the user staring at an unchanged page with no
   toast / no nav. Fix: F9.
5. **All search inputs accept text but don't filter.** Affects
   `/tickets`, `/companies`, `/contacts`, `/projects`, `/assets`,
   `/kb`, `/portal/kb`, and the global top-bar search. Either bind
   the signal to a `.filter()` over the rows or hide the input.
6. **All Status / Priority / Type filters don't filter, all
   sortable headers don't sort (no sort-arrow ever shown), all
   pagination buttons don't paginate.** Same root cause: state
   exists but nothing reads it.
7. **All detail-page header buttons are decorative** (Edit, Renew,
   Pay Now, Connect, Configure, etc.). See cross-cutting #6 for the
   full list. Either wire them or hide them behind a feature flag.
8. **Sidebar "active route" indicator is invisible** across every
   AppLayout page. The current page's NavItem has no visible
   distinction (no bg-highlight, no left bar, no color shift).
   Probably the single most disorienting thing for new users.
9. **HTML `<title>` reads "Mokosh PlatformMokosh Platform"** on
   every page (concatenated twice). See F7.
10. **Portal layout pages render the page title twice.** See F6.

### P2 - visual / contrast / readability

11. **"Breached" SLA pill** - dark red text on dark red row bg.
    Almost certainly fails WCAG AA. Most-visible on `/dashboard`
    SLA Warnings card.
12. **Yellow-on-yellow** SLA "45 minutes remaining" / TKT-1234
    pill - readable but borderline.
13. **Browser-native HTML5 validation tooltips** ("Please fill out
    this field") clash with the dark theme and aren't accessible.
    Replace with the existing `Input.error` / `Select.error` props
    in [`form.rs`](../../src/components/form.rs).
14. **`/reports/:report_type` Date Range and Group By selects
    render visually empty** despite the DOM having `option ...
    (selected)`. Dioxus 0.7 native `<select>` rendering bug; fix by
    using `value:` on the `<select>` and binding to a signal (or
    just use the `Select` component). See F11.
15. **`[Chart placeholder - ...]` literal text** is shipped on
    `/reports/:type` (4 placeholder cards). Looks like a dev tag
    accidentally left in. Either implement charts or replace with a
    styled "Coming soon" card.
16. **Required-field marker is just a small red `*`.** Not
    accessible to colorblind users; pair with `aria-label
    ="required"` or an explicit "Required" badge.
17. **"+3" and "+8.5" delta indicators on stat cards** are tiny
    (~11 px) compared to the 32 px stat number. Easy to miss.
18. **`<a href="#">` dead links** - at least 14+ instances
    scattered across HomePage footer, KB related-articles, asset
    RMM link, contract-document links, notification email-template
    list, portal article items, login "Contact us". See F15.
19. **KB CategoryCards** on `/kb` use `cursor-pointer` but have no
    Link wrapping them. See F13.

### P3 - layout / scrollbar / minor nits

20. **Multi-scrollbar nesting on `/dashboard` at narrow heights** -
    sidebar gets its own internal scrollbar AND main content
    scrolls. The bottom of the page is clipped without a visible
    "more below" affordance.
21. **Dispatch board appointment rendering** - appointment data is
    in props but not visually positioned in the time-slot grid.
    See F14.
22. **Pagination prev "<" on page 1** should be visibly disabled
    (currently looks identical to the active ">").
23. **Hover state on `clickable` rows** has no bg-color change,
    only cursor-pointer. Hover affordance is too subtle.
24. **Notification bell** is wired (MAPPS-132): it fetches the in-app
    inbox from `GET /notifications`, shows a dropdown panel, drives the
    red dot off the real unread count, and marks items read via
    `POST /notifications/{id}/read`.
25. **Top-bar search dropdown caret (▾)** is shown but no dropdown
    opens. Remove the caret or wire a results panel.
26. **Avatar (top-right) has no dropdown** - most users will hunt
    for "Logout" / "Profile" here.
27. **Top-bar global search value is lost on every navigation** -
    the input is in the layout but its state isn't persisted.

## Per-page status

Every route in [`src/lib.rs`](../../src/lib.rs) was walked. The status
column tracks whether a typical user can productively use the page
end-to-end.

| Route | Component | File | Status |
| --- | --- | --- | --- |
| `/` | HomePage | `pages/home.rs` | partial (3 footer dead links) |
| `/login` | LoginPage | `pages/auth.rs` | working (always redirects under bypass) |
| `/forgot-password` | ForgotPasswordPage | `pages/auth.rs` | working (mock submit) |
| `/reset-password/:token` | ResetPasswordPage | `pages/auth.rs` | working (validation only, mock submit) |
| `/dashboard` | DashboardPage | `pages/dashboard.rs` | partial (5 dead row clicks; sidebar nav works) |
| `/tickets` | TicketListPage | `pages/tickets.rs` | partial (filters bind but don't filter; row click is `\|_\| {}`) |
| `/tickets/new` | TicketNewPage | `pages/tickets.rs` | partial (mock submit, no nav) |
| `/tickets/:id` | TicketDetailPage | `pages/tickets.rs` | partial (Add Note opens modal; submit + inputs unwired) |
| `/time` | TimeEntryListPage | `pages/time.rs` | partial (dead row clicks) |
| `/time/new` | TimeEntryNewPage | `pages/time.rs` | partial (stub submit; button stuck loading) |
| `/timesheets` | TimesheetsPage | `pages/time.rs` | partial (Submit Timesheet + week-prev/next dead) |
| `/projects` | ProjectListPage | `pages/projects.rs` | working (whole-card Links navigate) |
| `/projects/new` | ProjectNewPage | `pages/projects.rs` | partial (stub submit) |
| `/projects/:id` | ProjectDetailPage | `pages/projects.rs-...` | partial (header buttons dead) |
| `/projects/:id/tasks` | ProjectTasksPage | `pages/projects.rs:...` | partial (Add Task dead) |
| `/companies` | CompanyListPage | `pages/contacts.rs` | partial (dead row clicks) |
| `/companies/new` | CompanyNewPage | `pages/contacts.rs` | partial (stub submit) |
| `/companies/:id` | CompanyDetailPage | `pages/contacts.rs` | partial (fake-link "5" stat; sub-table rows dead) |
| `/contacts` | ContactListPage | `pages/contacts.rs` | partial (dead row clicks) |
| `/contacts/new` | ContactNewPage | `pages/contacts.rs-...` | partial (stub submit) |
| `/contacts/:id` | ContactDetailPage | `pages/contacts.rs` | partial (header buttons dead) |
| `/calendar` | CalendarPage | `pages/calendar.rs` | partial (Week toggle, next-month chevron, day-cell click all dead) |
| `/dispatch` | DispatchBoardPage | `pages/calendar.rs` | partial (appointments not rendered in grid - F14) |
| `/contracts` | ContractListPage | `pages/contracts.rs` | partial (dead row clicks) |
| `/contracts/new` | ContractNewPage | `pages/contracts.rs` | placeholder ("Contract creation form would go here") |
| `/contracts/:id` | ContractDetailPage | `pages/contracts.rs` | partial (Edit / Renew dead; PDF / SLA links dead) |
| `/invoices` | InvoiceListPage | `pages/billing.rs` | partial (dead row clicks) |
| `/invoices/new` | InvoiceNewPage | `pages/billing.rs` | placeholder |
| `/invoices/:id` | InvoiceDetailPage | `pages/billing.rs` | partial (Download PDF / Send / Record Payment dead) |
| `/payments` | PaymentListPage | `pages/billing.rs` | partial (rows fully inert - F17) |
| `/assets` | AssetListPage | `pages/assets.rs` | partial (dead row clicks) |
| `/assets/new` | AssetNewPage | `pages/assets.rs` | placeholder |
| `/assets/:id` | AssetDetailPage | `pages/assets.rs` | partial (sub-table dead; "Remote Connect" / "Open in RMM" dead) |
| `/kb` | KBHomePage | `pages/knowledge_base.rs` | partial (CategoryCards `cursor-pointer` but no Link - F13) |
| `/kb/articles` | KBArticleListPage | `pages/knowledge_base.rs` | partial (dead row clicks) |
| `/kb/articles/new` | KBArticleNewPage | `pages/knowledge_base.rs` | placeholder |
| `/kb/articles/:id` | KBArticleDetailPage | `pages/knowledge_base.rs` | partial (related articles dead) |
| `/reports` | ReportsPage | `pages/reports.rs` | partial (report tile clicks navigate) |
| `/reports/:report_type` | ReportDetailPage | `pages/reports.rs` | partial (Date Range / Group By selects render blank - F11) |
| `/settings` | SettingsPage | `pages/settings.rs` | working (sub-page navigation works) |
| `/settings/users` | UserManagementPage | `pages/settings.rs` | partial (Add User dead) |
| `/settings/teams` | TeamManagementPage | `pages/settings.rs` | partial (Add Team dead) |
| `/settings/notifications` | NotificationSettingsPage | `pages/settings.rs` | partial (5 email-template dead links) |
| `/settings/integrations` | IntegrationSettingsPage | `pages/settings.rs` | partial (Connect / Configure all dead) |
| `/settings/billing` | BillingSettingsPage | `pages/settings.rs` | partial (Manage Subscription dead) |
| `/admin/team` | TeamPage | `pages/team.rs` | working (invite + revoke wired; PMS-247). Role picker hidden behind `ROLE_ASSIGNMENT_ENABLED=false` until full RBAC lands - invites go out as Technician (PMS-513) |
| `/admin/tenants` | TenantManagementPage | `pages/admin.rs` | partial (dead row clicks) |
| `/portal` | PortalHomePage | `pages/portal.rs` | working |
| `/portal/tickets` | PortalTicketListPage | `pages/portal.rs` | partial (rows fully inert - no inner Link either) |
| `/portal/tickets/new` | PortalTicketNewPage | `pages/portal.rs` | **broken (P0 GET-leak)** |
| `/portal/tickets/:id` | PortalTicketDetailPage | `pages/portal.rs` | partial (header buttons dead) |
| `/portal/invoices` | PortalInvoiceListPage | `pages/portal.rs` | partial (dead row clicks) |
| `/portal/invoices/:id` | PortalInvoiceDetailPage | `pages/portal.rs` | placeholder |
| `/portal/kb` | PortalKBPage | `pages/portal.rs-...` | partial (article items dead) |
| `/:..route` | NotFoundPage | `pages/not_found.rs` | working |

## Proposed fixes

Concrete code patches. File paths and line numbers reference the
2026-05-06 codebase state. Every patch is small enough to land as
its own commit / PR.

### F1. `TableRow.clickable` should imply onclick or do nothing

**File:** [`src/components/table.rs`](../../src/components/table.rs).
**Why:** `clickable: true` adds hover/cursor styles that promise an
interactive row. When parents pass it without an `onclick`, users
see a clickable cursor over a dead row.

**Patch (component-side):** drop the `cursor-pointer` / hover class
when `clickable` is set without an `onclick`:

```rust
let is_truly_clickable = props.clickable && props.onclick.is_some();
let row_class = if is_truly_clickable {
    "cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-800/50"
} else {
    ""
};
```

**Alternative (caller-side):** prefer this if the rows really should
navigate:

```rust
TableRow {
    clickable: true,
    onclick: {
        let id = props.id.clone();
        move |_| { use_navigator().push(Route::TicketDetail { id: id.clone() }); }
    },
    /* ... cells ... */
}
```

Apply caller-side fix to: `dashboard.rs`, `tickets.rs`,
`time.rs`, `contacts.rs`,
`contracts.rs`, `billing.rs`, `assets.rs`,
`knowledge_base.rs`, `admin.rs`, `portal.rs`.

### F2. `/tickets` row-click empty closure

**File:**
[`src/pages/tickets.rs`](../../src/pages/tickets.rs).

```rust
// Replace:
TableRow { clickable: true,
    onclick: move |_| {
        // Navigate to ticket detail
    },

// With:
let nav = use_navigator();
let id = props.id.clone();
TableRow { clickable: true,
    onclick: move |_| { nav.push(Route::TicketDetail { id: id.clone() }); },
```

### F3. `TicketDetailPage` Add Note modal: wire submit + inputs

**File:**
[`src/pages/tickets.rs`](../../src/pages/tickets.rs).

Add signals for note type and content, bind the Select / Textarea,
and give the footer Add Note button an `onclick`:

```rust
let mut note_type = use_signal(|| "internal".to_string());
let mut note_content = use_signal(String::new);

Modal {
    /* ... */
    footer: rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| show_note_modal.set(false),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            onclick: move |_| {
                // TODO: POST to /api/v1/tickets/:id/notes once server is wired.
                note_content.set(String::new());
                show_note_modal.set(false);
            },
            "Add Note"
        }
    },
    div { class: "space-y-4",
        Select {
            /* ... */
            onchange: move |e: FormEvent| note_type.set(e.value()),
        }
        Textarea {
            /* ... */
            value: note_content.read().clone(),
            oninput: move |e: FormEvent| note_content.set(e.value()),
        }
    }
}
```

### F4. `TicketDetailPage` "Log Time" button

**File:**
[`src/pages/tickets.rs`](../../src/pages/tickets.rs).

Replace the no-onclick button with a Link to `/time/new`
pre-populated with the current ticket id:

```rust
Link {
    to: Route::TimeEntryNew {},  // or extend route with ?ticket=:id
    Button {
        variant: ButtonVariant::Primary,
        ClockIcon { size: IconSize::Small, class: "mr-2".to_string() }
        "Log Time"
    }
}
```

### F5. Detail-page header action buttons

**Pattern (applies to many files):** Edit / Renew / Pay Now / Add
Task / Send / Download PDF / Record Payment / Connect / Configure /
Remote Connect render but do nothing. Two reasonable options:

- **(a)** wire them to real handlers (eventually backend calls), or
- **(b)** hide them behind a `cfg(feature = "experimental")` so
  they don't show as broken.

Recommended: (b) for now. The cheapest correct fix is to delete the
unwired buttons and re-add them when the implementation lands.

Optional component improvement: add a `placeholder: bool` prop on
`Button` that visually dims and tooltips "Coming soon", so the UI
is honest until each button is wired.

### F6. Portal layout title duplication

**Files:** `src/components/layout.rs` (PortalLayout) and
`src/pages/portal.rs`.

**Option A - drop the page-level h1:**

```rust
// Before (portal.rs):
PortalLayout { title: "My Tickets",
    div { class: "flex items-center justify-between mb-6",
        h1 { class: "text-2xl font-bold ...", "My Tickets" }
        // ...
    }

// After:
PortalLayout { title: "My Tickets",
    div { class: "flex items-center justify-end mb-6",
        // h1 removed; PortalLayout owns the title
        Link { to: Route::PortalTicketNew {}, /* ... */ }
    }
```

**Option B - drop the title prop and keep the per-page h1:** simpler
if you want each page to control its own header layout. Either way
pick one.

### F7. HTML `<title>` duplication ("Mokosh PlatformMokosh Platform")

**Files:** `Dioxus.toml` and `src/components/layout.rs` (AppLayout /
PortalLayout where they call `document::Title { ... }` or set the
title prop).

The browser-tab title concatenates the static `Dioxus.toml`
`[application] name` with the layout-level title prop. Either:

- Remove the `name` from `Dioxus.toml`, or
- Stop the layouts from prepending "Mokosh Platform" before the
  page title - render only the page-specific title.

Standard fix is `document::Title { "{props.title} - Mokosh Platform" }`
once, instead of the duplication.

### F8. PortalTicketNewPage form is fully decorative

**File:**
`src/pages/portal.rs`.

This is the **P0 critical bug**. The form has no `onsubmit`, so
the browser falls back to a native GET submit, leaking values to the
URL and blanking the SPA.

Patch: add signals + bind every input + give the form an `onsubmit`:

```rust
let mut subject = use_signal(String::new);
let mut description = use_signal(String::new);
let mut priority = use_signal(|| "medium".to_string());
let nav = use_navigator();

PortalLayout { title: "Submit Ticket",
    Card {
        form {
            class: "space-y-6",
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                // TODO: POST to /api/v1/portal/tickets once server is wired.
                nav.push(Route::PortalTicketList {});
            },
            crate::components::Input {
                name: "subject", /* ... */
                value: subject.read().clone(),
                oninput: move |e: FormEvent| subject.set(e.value()),
            }
            crate::components::Textarea {
                name: "description", /* ... */
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
            crate::components::Select {
                name: "priority", /* ... */
                value: priority.read().clone(),
                onchange: move |e: FormEvent| priority.set(e.value()),
            }
            // ... file upload zone (defer to F19)
            div { class: "flex justify-end space-x-3",
                Link { to: Route::PortalTicketList {}, Button { variant: ButtonVariant::Secondary, "Cancel" } }
                Button { r#type: "submit", variant: ButtonVariant::Primary, "Submit Ticket" }
            }
        }
    }
}
```

Even before the server endpoint is real, the `e.prevent_default()`
alone fixes the data-loss bug.

### F9. Stub-submit pages get stuck loading

**Files:**
- [`src/pages/time.rs`](../../src/pages/time.rs) (TimeEntryNewPage)
- [`src/pages/projects.rs`](../../src/pages/projects.rs) (ProjectNewPage)
- [`src/pages/contacts.rs`](../../src/pages/contacts.rs) (CompanyNewPage)
- [`src/pages/contacts.rs`](../../src/pages/contacts.rs) (ContactNewPage)

All four set `is_submitting=true` and never reset, leaving the
button stuck. Until real APIs land, mock with the same 1s pattern as
TicketNewPage:

```rust
onsubmit: move |e: FormEvent| {
    e.prevent_default();
    is_submitting.set(true);
    let nav = use_navigator();
    spawn(async move {
        #[cfg(feature = "web")]
        gloo_timers::future::TimeoutFuture::new(1000).await;
        is_submitting.set(false);
        nav.push(Route::TimeEntryList {}); // or Route::ProjectList {} etc.
    });
}
```

### F10. Hardcoded detail-page titles

**Files:** every `*DetailPage` in the codebase. Cheapest intermediate
step until real data fetching lands - include the route param in the
placeholder so the page is at least visibly route-aware:

```rust
// Before (e.g. tickets.rs):
title: "TKT-1234: Email server not responding",

// After:
title: "Ticket {props.id}",
```

When server data fetching lands, replace `props.id` with the resolved
ticket title.

### F11. `<select>` with declarative `selected` not rendering on `/reports/:report_type`

**File:**
[`src/pages/reports.rs`](../../src/pages/reports.rs).

Dioxus 0.7's `<select>` requires `value:` on the select itself,
not `selected:` on each `<option>`. Fix:

```rust
// Before:
select { class: "rounded-md border-gray-300 text-sm",
    option { "Last 7 days" }
    option { selected: true, "Last 30 days" }
    option { "This Month" }
}

// After:
let mut date_range = use_signal(|| "last_30_days".to_string());
select {
    class: "rounded-md border-gray-300 text-sm",
    value: date_range.read().clone(),
    onchange: move |e: FormEvent| date_range.set(e.value()),
    option { value: "last_7_days", "Last 7 days" }
    option { value: "last_30_days", "Last 30 days" }
    option { value: "this_month", "This Month" }
}
```

Or better: switch to `<Select>` from
[`crate::components::form`](../../src/components/form.rs) which already
handles `value:` / `onchange` plumbing.

### F12. Placeholder pages

**Files:**
[`src/pages/contracts.rs`](../../src/pages/contracts.rs)
(ContractNew),
[`billing.rs`](../../src/pages/billing.rs) (InvoiceNew),
[`assets.rs`](../../src/pages/assets.rs) (AssetNew),
[`knowledge_base.rs`](../../src/pages/knowledge_base.rs)
(KBArticleNew),
`portal.rs`
(PortalInvoiceDetail).

These show literal "X creation form would go here" text. Until the
real forms exist:

- **Hide the routes:** comment out the `Route::*New {}` variants in
  [`src/lib.rs`](../../src/lib.rs) and remove the "+ New" buttons that
  link to them. Honest UX.
- **Implement minimal forms** matching the data models in
  [`src/modules/<area>/models.rs`](../../src/modules/) (T&M / fixed-
  price for contracts; line items for invoices; etc.).

### F13. KB CategoryCards are styled clickable but have no Link

**File:**
[`src/pages/knowledge_base.rs`](../../src/pages/knowledge_base.rs).

Wrap the Card in a Link so the cursor-pointer styling is honest:

```rust
fn CategoryCard(props: CategoryCardProps) -> Element {
    rsx! {
        Link {
            to: Route::KBArticleList {},
            // (later: extend route with ?category=X query param)
            Card { class: "hover:shadow-lg transition-shadow cursor-pointer",
                /* ... */
            }
        }
    }
}
```

### F14. Dispatch board appointments not rendered

**File:**
[`src/pages/calendar.rs`](../../src/pages/calendar.rs).

`TechnicianRow` receives `appointments: Vec<(start, end, label,
type)>` but only renders empty divs (`for _ in 0..9 { div { ... }
}`). Position each appointment absolutely within the time-slot row
using its start/end times:

```rust
fn TechnicianRow(props: TechnicianRowProps) -> Element {
    rsx! {
        div { class: "grid grid-cols-[200px_repeat(9,1fr)] border-b ... relative",
            div { class: "p-2 flex items-center", /* technician avatar */ }
            for _ in 0..9 {
                div { class: "border-l border-gray-200 dark:border-gray-700 relative" }
            }
            for (start, end, label, kind) in props.appointments.iter() {
                div {
                    class: "absolute top-1 bottom-1 ... rounded {color_for(kind)}",
                    style: "left: {compute_left(start)}; width: {compute_width(start, end)};",
                    span { class: "text-xs px-1", "{label}" }
                }
            }
        }
    }
}
```

### F15. Dead `<a href="#">` cleanup pass

Sweep these files and either delete the dead links or replace with
real Routes / `mailto:` / external URLs:

- [`src/pages/home.rs`](../../src/pages/home.rs) (footer
  Privacy / Terms / Contact)
- `src/pages/auth.rs` (since split into `login.rs`, `auth_callback.rs` and `portal_login.rs`) ("Contact
  us" on login)
- [`src/pages/contracts.rs`](../../src/pages/contracts.rs)
  (Contract PDF / SLA Agreement)
- [`src/pages/assets.rs`](../../src/pages/assets.rs) ("Open in
  Tactical RMM")
- [`src/pages/knowledge_base.rs`](../../src/pages/knowledge_base.rs)
  (3x Related Articles)
- [`src/pages/settings.rs`](../../src/pages/settings.rs)
  (5x Email Templates)
- `src/pages/portal.rs`
  (PortalArticleItem - 5 entries)

### F16. Add `data-testid` to core components

**Files:** `src/components/{button,table,modal,form}.rs`. Add an
optional `data_testid: Option<String>` prop and pass it through to
the rendered element. Lets future automation (Playwright, MCP browser
walks) target stable selectors instead of button text.

```rust
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    /* ... existing ... */
    #[props(into)]
    pub data_testid: Option<String>,
}

button {
    /* ... */
    "data-testid": props.data_testid.as_deref(),
}
```

Then per-page, pass `data_testid: "ticket-list-new-button"` etc.

### F17. PaymentList rows are completely inert

**File:**
[`src/pages/billing.rs`](../../src/pages/billing.rs).

Styled-blue invoice numbers are just spans. Either link them to
invoice detail:

```rust
// Replace:
TableCell { class: "text-blue-600", "INV-2024-097" }

// With:
TableCell {
    Link {
        to: Route::InvoiceDetail { id: "...".to_string() },
        class: "text-blue-600 hover:text-blue-500",
        "INV-2024-097"
    }
}
```

Or remove the blue styling so it doesn't promise interactivity.

### F18. Dashboard `RecentTicketRow` should navigate

**File:**
[`src/pages/dashboard.rs`](../../src/pages/dashboard.rs).

Same pattern as F2 - pass `onclick` to `TableRow` to navigate to
ticket detail by `props.number` / `id`.

### F19. Portal new-ticket attachment zone is decorative

**File:**
`src/pages/portal.rs`.

The "Drag and drop files here" div has no `<input type="file">`.
Either remove the zone (until attachments are supported by the API)
or add a real input:

```rust
// Inside the existing border-dashed div:
input {
    r#type: "file",
    multiple: true,
    class: "absolute inset-0 opacity-0 cursor-pointer",
    onchange: move |e: FormEvent| {
        // bind to a Vec<File> signal once we know what we'll do with them
    },
}
```

## Recommended fix order

Roughly in increasing complexity / decreasing impact:

### P0 - mass-fix patterns (one PR each, big surface)

1. **F1** - `TableRow.clickable` honest-cursor fix (component-level
   patch in [`src/components/table.rs`](../../src/components/table.rs)).
   Fixes ~12 list pages in one diff.
2. **F5** - Hide unwired detail-page header buttons behind a feature
   flag. Affects every `*DetailPage`.
3. **F6** - Portal layout title duplication (5 portal pages).
4. **F7** - HTML `<title>` duplication (every page).
5. **F8** - Critical `/portal/tickets/new` GET-leak. One-line fix.

### P1 - targeted fixes per page

6. **F2** - `/tickets` row click stub.
7. **F3** - TicketDetail Add Note modal.
8. **F4** - TicketDetail "Log Time" button.
9. **F9** - Stub-submit pages stuck loading.
10. **F10** - Hardcoded detail-page titles.
11. **F11** - `<select>` rendering on `/reports/:type`.
12. **F12** - Placeholder pages (hide or implement).
13. **F14** - Dispatch board appointments.
14. **F18** - Dashboard ticket-row navigation.

### P2 - cleanup

15. **F13** - KB CategoryCards link wrapping.
16. **F15** - Dead `<a href="#">` sweep.
17. **F16** - `data-testid` on core components.
18. **F17** - PaymentList row interactivity.
19. **F19** - Portal attachment zone (real input or remove).

Server-dependent fixes are deliberately not on this list. They are
in
[`client-server-integration.md`](../client-server-integration.md)
under "Suggested next implementation pass".

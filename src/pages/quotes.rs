//! Quote pages, wired to the mokosh-server `/quotes` endpoints
//! (PMS-671 through PMS-674). Patterns (resource +
//! `active_tenant_generation`, loading / empty / error states,
//! `serde`-typed request bodies, the minimal query-string encoder)
//! mirror `src/pages/contracts.rs`.
//!
//! Two server rules shape the whole UI, and both are mirrored in
//! `modules::quotes::status` rather than re-derived here:
//!
//!   * **Totals are server-computed** from the line items. The editor
//!     shows a running total as a preview only; whatever comes back from
//!     the save is what is displayed afterwards.
//!   * **A quote freezes as it advances.** Content is editable only in
//!     `draft` / `rejected`, and each action is offered only in the state
//!     the server accepts it in, so the UI never presents a control that
//!     would come back 409.
//!
//! Finance-gated to match the server's `RequireBilling` + `RequireFinance`
//! pair, the same gate `contracts` and `billing` use.

use dioxus::prelude::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, DataTable, ErrorBanner,
    IconSize, Input, Modal, ModalSize, PageHeader, PlusIcon, Select, SelectOption, Table,
    TableBody, TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow, Textarea,
};
use crate::hooks::use_can_mutate;
use crate::modules::quotes::{
    status, ConvertQuoteRequest, CreateQuoteRequest, QuoteLineRequest, QuoteResponse,
    UpdateQuoteRequest,
};
use crate::utils::money::format_money;
use crate::utils::url::urlencoding_minimal;
use crate::utils::validation::Rule;
use crate::utils::{FormGuard, Paginated};
use crate::Route;

const PER_PAGE: usize = 25;

/// True when the signed-in user holds a finance role, matching the
/// server's `RequireFinance` guard on every quote route. Mirrors the
/// identical helper in `contracts.rs`.
fn use_can_manage_billing() -> bool {
    let auth = crate::hooks::use_auth();
    let state = auth.read();
    state
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false)
}

/// Colour for a status tag. The split is deliberate: internal workflow
/// states are neutral/blue, the client's own decisions are green / red,
/// and `converted` is purple because it is the end of the quote's life
/// rather than another pending step.
fn quote_status_variant(status: &str) -> BadgeVariant {
    match status {
        "draft" => BadgeVariant::Gray,
        "submitted" => BadgeVariant::Yellow,
        "approved" => BadgeVariant::Blue,
        "rejected" => BadgeVariant::Red,
        "sent" => BadgeVariant::Orange,
        "accepted" => BadgeVariant::Green,
        "declined" => BadgeVariant::Red,
        "expired" => BadgeVariant::Gray,
        "converted" => BadgeVariant::Purple,
        "cancelled" => BadgeVariant::Gray,
        _ => BadgeVariant::Gray,
    }
}

/// One-line explanation of what a status means, shown on the detail page.
/// `approved` and `accepted` are a word apart but mean completely
/// different things (we cleared it to go out vs the customer signed), so
/// the UI says which is which rather than relying on the label alone.
fn status_explainer(status: &str) -> &'static str {
    match status {
        "draft" => "Still being worked up. Editable.",
        "submitted" => "Waiting on internal approval. Content is locked.",
        "approved" => "Cleared internally. Not yet sent to the client.",
        "rejected" => "Turned down internally. Editable again.",
        "sent" => "With the client, awaiting their decision.",
        "accepted" => "The client signed off. Ready to convert to a project.",
        "declined" => "The client turned it down.",
        "expired" => "Passed its valid-until date without a decision.",
        "converted" => "Became a project.",
        "cancelled" => "Withdrawn.",
        _ => "",
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CompanyOption {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// The finance gate, rendered as the same `PermissionRequired` empty
/// state every other billing surface uses. The server's quote routes are
/// `RequireBilling` + `RequireFinance`, so without this a non-finance
/// role sees a generic "could not load" banner for what is actually a
/// permanent permission boundary.
fn permission_required() -> Element {
    rsx! {
        crate::components::PermissionRequired {
            title: "Quotes".to_string(),
            body: "Quotes are restricted to administrator and finance roles. Ask an administrator to grant you access."
                .to_string(),
        }
    }
}

#[component]
pub fn QuoteListPage() -> Element {
    use_page_title("Quotes");
    // MAPPS-604: a contact with `quotes:read` reaches the list too.
    // `use_capability` returns true for staff and platform sessions
    // unconditionally, so the pre-pivot behaviour is preserved: only
    // roles WITHOUT `quotes:read` (a non-finance staff role, no contact
    // cap) still land on the `PermissionRequired` splash.
    let contact_can_read = crate::hooks::capabilities::use_capability("quotes:read");
    if !use_can_manage_billing() && !contact_can_read {
        return permission_required();
    }

    // MAPPS-377: mount the data body only past the permission gate so its
    // fetch hooks run unconditionally within it (and never fire for a
    // non-finance role, preserving the pre-fix behaviour).
    rsx! { QuoteListBody {} }
}

#[component]
fn QuoteListBody() -> Element {
    // mokosh-contact-login prompt 006: the "New Quote" CTA is
    // staff-only. Contact-facing accept/decline lives in the detail
    // body, not this list.
    let staff_only =
        crate::hooks::capabilities::use_capability(crate::hooks::capabilities::STAFF_ONLY);
    let mut company_filter =
        use_signal(|| crate::utils::url::current_query_param("company_id").unwrap_or_default());
    let mut status_filter = use_signal(String::new);
    let mut page = use_signal(|| 1usize);

    let status_options = vec![
        SelectOption::new("", "All Statuses"),
        SelectOption::new("draft", "Draft"),
        SelectOption::new("submitted", "Submitted"),
        SelectOption::new("approved", "Approved"),
        SelectOption::new("rejected", "Rejected"),
        SelectOption::new("sent", "Sent"),
        SelectOption::new("accepted", "Accepted"),
        SelectOption::new("declined", "Declined"),
        SelectOption::new("expired", "Expired"),
        SelectOption::new("converted", "Converted"),
        SelectOption::new("cancelled", "Cancelled"),
    ];

    let companies_resource = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<Paginated<CompanyOption>>(
            "/contacts/companies?page=1&per_page=100&sort=name&sort_dir=asc",
        )
        .await
        .ok()
    });
    let company_options = {
        let mut opts = vec![SelectOption::new("", "All Companies")];
        if let Some(Some(resp)) = &*companies_resource.read_unchecked() {
            for c in &resp.data {
                opts.push(SelectOption::new(c.id.to_string(), c.name.clone()));
            }
        }
        opts
    };

    let company_text = company_filter.read().clone();
    let status_text = status_filter.read().clone();
    let current_page = (*page.read()).max(1);

    let company_for_resource = company_text.clone();
    let status_for_resource = status_text.clone();
    let quotes_resource = use_resource(move || {
        let company = company_for_resource.clone();
        let status = status_for_resource.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            // MAPPS-604: allow either staff or contact bearer; server
            // scopes to `contact.company_id` for the contact plane.
            if crate::hooks::fetch::api::current_access_token().is_none()
                && !crate::hooks::fetch::api::has_contact_session()
            {
                return None;
            }
            let mut path = format!("/quotes?page={current_page}&per_page={PER_PAGE}");
            if !company.is_empty() {
                path.push_str(&format!("&company_id={}", urlencoding_minimal(&company)));
            }
            if !status.is_empty() {
                path.push_str(&format!("&status={}", urlencoding_minimal(&status)));
            }
            crate::hooks::fetch::api::get_authed_any::<Paginated<QuoteResponse>>(&path)
                .await
                .ok()
        }
    });

    let snapshot = quotes_resource.read_unchecked();
    let is_loading = snapshot.is_none();
    let fetch_failed = matches!(*snapshot, Some(None));
    let (page_rows, total): (Vec<QuoteResponse>, u64) = match &*snapshot {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let has_filters = !company_text.is_empty() || !status_text.is_empty();

    let reachable = crate::hooks::use_server_reachable();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Quotes".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Quotes",
            subtitle: "Scope and price work, get it signed off, turn it into a project",
            actions: rsx! {
                if staff_only {
                    Link {
                        to: Route::QuoteNew {},
                        Button {
                            variant: ButtonVariant::Primary,
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "New Quote"
                        }
                    }
                }
            },
        }

        Card { class: "mb-6",
            div { class: "flex flex-col sm:flex-row gap-4",
                div { class: "flex-1",
                    Select {
                        name: "company",
                        options: company_options,
                        value: company_filter.read().clone(),
                        onchange: move |e: FormEvent| {
                            company_filter.set(e.value());
                            page.set(1);
                        },
                    }
                }
                Select {
                    name: "status",
                    options: status_options,
                    value: status_filter.read().clone(),
                    onchange: move |e: FormEvent| {
                        status_filter.set(e.value());
                        page.set(1);
                    },
                }
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load quotes. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 6,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Quote" }
                        TableHeader { "Client" }
                        TableHeader { "Total" }
                        TableHeader { "Valid until" }
                        TableHeader { "Status" }
                        TableHeader { "Created" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 6, rows: 5 }
                } else if page_rows.is_empty() {
                    if has_filters {
                        TableEmpty {
                            columns: 6,
                            title: "No quotes match your filters".to_string(),
                            description: "Adjust the filters above, or clear them to see every quote again."
                                .to_string(),
                            actions: rsx! {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    onclick: move |_| {
                                        company_filter.set(String::new());
                                        status_filter.set(String::new());
                                    },
                                    "Clear filters"
                                }
                            },
                        }
                    } else {
                        TableEmpty {
                            columns: 6,
                            title: "No quotes yet".to_string(),
                            description: "Create a quote to scope and price work before it starts."
                                .to_string(),
                            actions: rsx! {
                                if staff_only {
                                    Link {
                                        to: Route::QuoteNew {},
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                            "New Quote"
                                        }
                                    }
                                }
                            },
                        }
                    }
                } else {
                    TableBody {
                        for quote in page_rows.iter().cloned() {
                            QuoteRow {
                                key: "{quote.id}",
                                id: quote.id.to_string(),
                                number: quote
                                    .quote_number
                                    .clone()
                                    .unwrap_or_else(|| "-".to_string()),
                                title: quote.title.clone(),
                                company: quote
                                    .company_name
                                    .clone()
                                    .unwrap_or_else(|| "-".to_string()),
                                total: format_money(quote.total),
                                valid_until: quote
                                    .valid_until
                                    .map(|d| d.format("%b %-d, %Y").to_string())
                                    .unwrap_or_else(|| "No expiry".to_string()),
                                status: quote.status.clone(),
                                created: quote.created_at.format("%b %-d, %Y").to_string(),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct QuoteRowProps {
    id: String,
    number: String,
    title: String,
    company: String,
    total: String,
    valid_until: String,
    status: String,
    created: String,
}

#[component]
fn QuoteRow(props: QuoteRowProps) -> Element {
    let navigator = use_navigator();
    let id = props.id.clone();
    rsx! {
        TableRow {
            clickable: true,
            onclick: move |_| {
                navigator.push(Route::QuoteDetail { id: id.clone() });
            },
            TableCell {
                div { class: "font-medium", "{props.number}" }
                div { class: "text-xs text-subtle", "{props.title}" }
            }
            TableCell { "{props.company}" }
            TableCell { class: "font-medium", "{props.total}" }
            TableCell { "{props.valid_until}" }
            TableCell {
                Badge { variant: quote_status_variant(&props.status), "{status::label(&props.status)}" }
            }
            TableCell { class: "text-subtle", "{props.created}" }
        }
    }
}

// ============================================================================
// Detail
// ============================================================================

#[component]
pub fn QuoteDetailPage(id: String) -> Element {
    use_page_title("Quote");
    if !use_can_manage_billing() {
        return permission_required();
    }

    // MAPPS-377: mount the data body only past the permission gate so its
    // fetch hooks run unconditionally within it (and never fire for a
    // non-finance role, preserving the pre-fix behaviour).
    rsx! { QuoteDetailBody { id } }
}

#[component]
fn QuoteDetailBody(id: String) -> Element {
    // mokosh-contact-login prompt 006: all quote lifecycle controls
    // on this detail (Submit/Approve/Reject/Send/Convert/Cancel/Edit)
    // are staff-only. The customer-facing Accept/Decline UI is not
    // in this codebase today; skipped gracefully.
    let staff_only =
        crate::hooks::capabilities::use_capability(crate::hooks::capabilities::STAFF_ONLY);
    // MAPPS-607: PMS-936 exposes `GET /quotes/{id}/pdf` behind the
    // `quotes:download_pdf` cap. Staff / platform bypass unconditionally
    // through `use_capability`.
    let can_download_pdf = crate::hooks::capabilities::use_capability("quotes:download_pdf");
    let mut pdf_downloading = use_signal(|| false);
    let mut pdf_error = use_signal(String::new);
    let id_for_pdf = id.clone();
    let mut version = use_signal(|| 0u32);
    // Not `mut` locally: these are handed by value to the action helpers,
    // which take their own mutable binding.
    let action_error = use_signal(String::new);
    let busy = use_signal(|| false);
    let mut show_convert = use_signal(|| false);

    let id_for_fetch = id.clone();
    let quote_resource = use_resource(move || {
        let qid = id_for_fetch.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            let _v = version.read();
            crate::hooks::fetch::api::get_authed::<QuoteResponse>(&format!("/quotes/{qid}"))
                .await
                .ok()
        }
    });

    let snapshot = quote_resource.read_unchecked();
    let loading = snapshot.is_none();
    let quote: Option<QuoteResponse> = match &*snapshot {
        Some(Some(q)) => Some(q.clone()),
        _ => None,
    };
    let fetch_failed = matches!(*snapshot, Some(None));

    // MAPPS-377: read this before the outage early return so every hook in
    // this component runs unconditionally; it only reads a global signal.
    let can_mutate = use_can_mutate();

    if fetch_failed {
        return rsx! {
            crate::components::ContentUnavailable { title: "Quote".to_string() }
        };
    }

    rsx! {
        if loading {
            Card { p { class: "text-sm text-subtle italic", "Loading quote…" } }
        } else if let Some(q) = quote.clone() {
            {
                let st = q.status.clone();
                let quote_id = q.id.to_string();
                // Every control is gated on the same predicate the
                // server enforces, so a disabled button means "not in
                // this state" rather than "this might 409".
                let can_edit = status::allows_content_edit(&st) && can_mutate;
                rsx! {
                    PageHeader {
                        title: q.quote_number.clone().unwrap_or_else(|| "Quote".to_string()),
                        subtitle: q.title.clone(),
                        actions: rsx! {
                            Link {
                                to: Route::QuoteList {},
                                Button { variant: ButtonVariant::Secondary, "Back to Quotes" }
                            }
                            if can_download_pdf {
                                Button {
                                    variant: ButtonVariant::Secondary,
                                    loading: *pdf_downloading.read(),
                                    onclick: {
                                        let id_for_pdf = id_for_pdf.clone();
                                        move |_| {
                                            if *pdf_downloading.read() {
                                                return;
                                            }
                                            pdf_error.set(String::new());
                                            pdf_downloading.set(true);
                                            let id = id_for_pdf.clone();
                                            spawn(async move {
                                                #[cfg(feature = "web")]
                                                {
                                                    let path = format!("/quotes/{id}/pdf");
                                                    match crate::hooks::fetch::api::get_authed_any_bytes(&path).await {
                                                        Ok((bytes, filename)) => {
                                                            let name = filename
                                                                .unwrap_or_else(|| format!("quote-{id}.pdf"));
                                                            if let Err(e) =
                                                                crate::utils::download::save_bytes_as_file(&bytes, &name)
                                                            {
                                                                pdf_error.set(format!(
                                                                    "Fetched the PDF but could not save it: {e}"
                                                                ));
                                                            }
                                                        }
                                                        Err(crate::hooks::fetch::api::ApiError::Status {
                                                            code: 501,
                                                            ..
                                                        }) => {
                                                            pdf_error.set(
                                                                "PDF generation not available yet".to_string(),
                                                            );
                                                        }
                                                        Err(err) => {
                                                            pdf_error.set(format!(
                                                                "Could not download PDF: {}",
                                                                err.user_message()
                                                            ));
                                                        }
                                                    }
                                                }
                                                #[cfg(not(feature = "web"))]
                                                let _ = &id;
                                                pdf_downloading.set(false);
                                            });
                                        }
                                    },
                                    "Download PDF"
                                }
                            }
                            if can_edit && staff_only {
                                Link {
                                    to: Route::QuoteEdit { id: quote_id.clone() },
                                    Button { variant: ButtonVariant::Secondary, "Edit" }
                                }
                            }
                        },
                    }

                    if !action_error.read().is_empty() {
                        ErrorBanner { class: "mb-3", "{action_error}" }
                    }
                    if !pdf_error.read().is_empty() {
                        ErrorBanner { class: "mb-3", "{pdf_error}" }
                    }

                    div { class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                        div { class: "lg:col-span-2 space-y-6",
                            Card { title: "Scope",
                                div { class: "flex items-center gap-2 mb-3",
                                    Badge { variant: quote_status_variant(&st), "{status::label(&st)}" }
                                    span { class: "text-xs text-subtle", "{status_explainer(&st)}" }
                                }
                                if let Some(summary) = q.summary.clone().filter(|s| !s.is_empty()) {
                                    p { class: "text-sm font-medium mb-2", "{summary}" }
                                }
                                if let Some(desc) = q.description.clone().filter(|s| !s.is_empty()) {
                                    p { class: "text-sm whitespace-pre-wrap", "{desc}" }
                                } else {
                                    p { class: "text-sm text-subtle italic", "No scope description." }
                                }
                            }

                            Card { title: "Line items",
                                Table {
                                    TableHead {
                                        TableRow {
                                            TableHeader { "Description" }
                                            TableHeader { "Type" }
                                            TableHeader { "Qty" }
                                            TableHeader { "Unit price" }
                                            TableHeader { "Total" }
                                        }
                                    }
                                    TableBody {
                                        for line in q.lines.clone().unwrap_or_default() {
                                            TableRow { key: "{line.id}",
                                                TableCell { "{line.description}" }
                                                TableCell { class: "text-subtle", "{line.line_type}" }
                                                TableCell { "{line.quantity}" }
                                                TableCell { "{format_money(line.unit_price)}" }
                                                TableCell { class: "font-medium", "{format_money(line.total)}" }
                                            }
                                        }
                                    }
                                }
                                if q.lines.clone().unwrap_or_default().is_empty() {
                                    p { class: "text-sm text-subtle italic mt-2", "No line items on this quote." }
                                }
                                div { class: "mt-4 flex flex-col items-end gap-1 text-sm",
                                    div { "Subtotal: " span { class: "font-medium", "{format_money(q.subtotal)}" } }
                                    div { "Tax: " span { class: "font-medium", "{format_money(q.tax_amount)}" } }
                                    div { class: "text-base",
                                        "Total: " span { class: "font-semibold", "{format_money(q.total)}" }
                                    }
                                }
                            }

                            // Internal sign-off runs on the shared
                            // polymorphic approvals surface; this is the
                            // same component the ticket detail uses,
                            // pointed at the quote target.
                            crate::pages::tickets::ApprovalsSection {
                                entity_id: quote_id.clone(),
                                entity_segment: "quotes".to_string(),
                                entity_noun: "quote".to_string(),
                            }
                        }

                        div { class: "space-y-6",
                            Card { title: "Details",
                                dl { class: "space-y-2 text-sm",
                                    div { dt { class: "text-subtle", "Client" } dd { "{q.company_name.clone().unwrap_or_else(|| \"-\".to_string())}" } }
                                    div {
                                        dt { class: "text-subtle", "Valid until" }
                                        dd {
                                            "{q.valid_until.map(|d| d.format(\"%b %-d, %Y\").to_string()).unwrap_or_else(|| \"No expiry\".to_string())}"
                                        }
                                    }
                                    if let Some(sent) = q.sent_at {
                                        div { dt { class: "text-subtle", "Sent" } dd { "{sent.format(\"%b %-d, %Y\")}" } }
                                    }
                                    if let Some(decided) = q.decided_at {
                                        div {
                                            dt { class: "text-subtle", "Client decided" }
                                            dd { "{decided.format(\"%b %-d, %Y\")}" }
                                        }
                                    }
                                    if let Some(notes) = q.decision_notes.clone().filter(|s| !s.is_empty()) {
                                        div { dt { class: "text-subtle", "Client notes" } dd { "{notes}" } }
                                    }
                                    if let Some(project_id) = q.converted_project_id {
                                        div {
                                            dt { class: "text-subtle", "Project" }
                                            dd {
                                                Link {
                                                    to: Route::ProjectDetail { id: project_id.to_string() },
                                                    class: "text-accent hover:underline",
                                                    "View project"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if staff_only {
                            Card { title: "Actions",
                                div { class: "flex flex-col gap-2",
                                    if status::can_submit(&st) {
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: {
                                                let qid = quote_id.clone();
                                                move |_| {
                                                    set_status(qid.clone(), "submitted", version, busy, action_error);
                                                }
                                            },
                                            "Submit for approval"
                                        }
                                    }
                                    if status::can_approve(&st) {
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: {
                                                let qid = quote_id.clone();
                                                move |_| {
                                                    set_status(qid.clone(), "approved", version, busy, action_error);
                                                }
                                            },
                                            "Mark approved"
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: {
                                                let qid = quote_id.clone();
                                                move |_| {
                                                    set_status(qid.clone(), "rejected", version, busy, action_error);
                                                }
                                            },
                                            "Reject"
                                        }
                                    }
                                    if status::can_send(&st) {
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: {
                                                let qid = quote_id.clone();
                                                move |_| {
                                                    send_quote(qid.clone(), version, busy, action_error);
                                                }
                                            },
                                            "Send to client"
                                        }
                                        p { class: "text-xs text-subtle",
                                            "Emails the billing contact a link to accept or decline."
                                        }
                                    }
                                    if status::awaiting_client(&st) {
                                        p { class: "text-xs text-subtle",
                                            "Waiting on the client. Their decision arrives through the portal."
                                        }
                                    }
                                    if status::can_convert(&st) {
                                        Button {
                                            variant: ButtonVariant::Primary,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: move |_| show_convert.set(true),
                                            "Convert to project"
                                        }
                                    }
                                    if status::can_cancel(&st) {
                                        Button {
                                            variant: ButtonVariant::Danger,
                                            disabled: !can_mutate || *busy.read(),
                                            onclick: {
                                                let qid = quote_id.clone();
                                                move |_| {
                                                    cancel_quote(qid.clone(), version, busy, action_error);
                                                }
                                            },
                                            "Cancel quote"
                                        }
                                    }
                                }
                            }
                            }
                        }
                    }

                    ConvertQuoteModal {
                        open: *show_convert.read(),
                        quote_id: quote_id.clone(),
                        onclose: move |_| show_convert.set(false),
                        onconverted: move |_| {
                            show_convert.set(false);
                            version += 1;
                        },
                    }
                }
            }
        }
    }
}

/// PUT the quote's status. Used for the internal-workflow transitions
/// (`submitted` / `approved` / `rejected`); the client-owned statuses are
/// deliberately not reachable from here, matching the server.
fn set_status(
    quote_id: String,
    next: &'static str,
    mut version: Signal<u32>,
    mut busy: Signal<bool>,
    mut action_error: Signal<String>,
) {
    busy.set(true);
    action_error.set(String::new());
    spawn(async move {
        let body = UpdateQuoteRequest {
            status: Some(next.to_string()),
            ..Default::default()
        };
        match crate::hooks::fetch::api::put_authed::<QuoteResponse, _>(
            &format!("/quotes/{quote_id}"),
            &body,
        )
        .await
        {
            Ok(_) => {
                crate::hooks::toast::push_toast(
                    crate::components::AlertType::Success,
                    "Quote updated",
                );
                version += 1;
            }
            Err(e) => action_error.set(format!("Could not update the quote: {e}")),
        }
        busy.set(false);
    });
}

fn send_quote(
    quote_id: String,
    mut version: Signal<u32>,
    mut busy: Signal<bool>,
    mut action_error: Signal<String>,
) {
    busy.set(true);
    action_error.set(String::new());
    spawn(async move {
        let empty = serde_json::json!({});
        match crate::hooks::fetch::api::post_authed::<QuoteResponse, _>(
            &format!("/quotes/{quote_id}/send"),
            &empty,
        )
        .await
        {
            Ok(_) => {
                crate::hooks::toast::push_toast(
                    crate::components::AlertType::Success,
                    "Quote sent to the client",
                );
                version += 1;
            }
            Err(e) => action_error.set(format!("Could not send the quote: {e}")),
        }
        busy.set(false);
    });
}

fn cancel_quote(
    quote_id: String,
    mut version: Signal<u32>,
    mut busy: Signal<bool>,
    mut action_error: Signal<String>,
) {
    busy.set(true);
    action_error.set(String::new());
    spawn(async move {
        match crate::hooks::fetch::api::delete_authed(&format!("/quotes/{quote_id}")).await {
            Ok(_) => {
                crate::hooks::toast::push_toast(
                    crate::components::AlertType::Success,
                    "Quote cancelled",
                );
                version += 1;
            }
            Err(e) => action_error.set(format!("Could not cancel the quote: {e}")),
        }
        busy.set(false);
    });
}

#[derive(Props, Clone, PartialEq)]
struct ConvertQuoteModalProps {
    open: bool,
    quote_id: String,
    onclose: EventHandler<()>,
    onconverted: EventHandler<()>,
}

/// Collects the fields a quote cannot supply (who runs the project, when,
/// how it bills) and posts the conversion. Name, scope, client, and
/// budget are intentionally not editable here: the server maps them from
/// the quote so the project cannot disagree with what the client signed.
#[component]
fn ConvertQuoteModal(props: ConvertQuoteModalProps) -> Element {
    let mut start_date = use_signal(String::new);
    let mut end_date = use_signal(String::new);
    let mut billing_method = use_signal(|| "fixed_price".to_string());
    let mut error = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    let billing_options = vec![
        SelectOption::new("fixed_price", "Fixed price"),
        SelectOption::new("time_and_materials", "Time and materials"),
        SelectOption::new("not_billable", "Not billable"),
    ];

    let quote_id = props.quote_id.clone();
    let onconverted = props.onconverted;
    let on_submit = move |_| {
        let start = start_date.read().trim().to_string();
        let end = end_date.read().trim().to_string();
        let method = billing_method.read().clone();
        let qid = quote_id.clone();

        // Mirror the server's schema validation so an obviously-inverted
        // range never costs a round trip.
        let mut guard = FormGuard::new();
        if !start.is_empty() && !end.is_empty() && end < start {
            error.set("Target end date cannot be before the start date.".to_string());
            guard.note_invalid(Some("target_end_date"));
        }
        if guard.blocked() {
            return;
        }

        submitting.set(true);
        error.set(String::new());
        spawn(async move {
            let body = ConvertQuoteRequest {
                project_manager_id: None,
                start_date: start.parse().ok(),
                target_end_date: end.parse().ok(),
                billing_method: Some(method),
            };
            match crate::hooks::fetch::api::post_authed::<QuoteResponse, _>(
                &format!("/quotes/{qid}/convert"),
                &body,
            )
            .await
            {
                Ok(_) => {
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        "Quote converted to a project",
                    );
                    onconverted.call(());
                }
                Err(e) => error.set(format!("Could not convert the quote: {e}")),
            }
            submitting.set(false);
        });
    };

    let onclose = props.onclose;
    rsx! {
        Modal {
            open: props.open,
            title: "Convert to project",
            size: ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: *submitting.read(),
                    onclick: on_submit,
                    "Convert"
                }
            },
            div { class: "space-y-4",
                p { class: "text-sm text-subtle",
                    "The project takes its name, scope, client, and budget from this quote. Set how it runs."
                }
                if !error.read().is_empty() {
                    p { class: "text-sm text-red-600 dark:text-red-300", "{error}" }
                }
                Input {
                    name: "start_date",
                    label: "Start date",
                    r#type: "date".to_string(),
                    value: start_date.read().clone(),
                    oninput: move |e: FormEvent| start_date.set(e.value()),
                }
                Input {
                    name: "target_end_date",
                    label: "Target end date",
                    r#type: "date".to_string(),
                    value: end_date.read().clone(),
                    oninput: move |e: FormEvent| end_date.set(e.value()),
                }
                Select {
                    name: "billing_method",
                    label: "Billing method",
                    options: billing_options,
                    value: billing_method.read().clone(),
                    onchange: move |e: FormEvent| billing_method.set(e.value()),
                }
            }
        }
    }
}

// ============================================================================
// Editor (create + edit share one form)
// ============================================================================

/// A line being edited. Held as strings because that is what the inputs
/// produce and what the server takes; parsing happens only for the
/// running-total preview.
#[derive(Clone, PartialEq, Default)]
struct DraftLine {
    line_type: String,
    description: String,
    quantity: String,
    unit_price: String,
}

impl DraftLine {
    fn new() -> Self {
        Self {
            line_type: "service".to_string(),
            description: String::new(),
            quantity: "1".to_string(),
            unit_price: "0".to_string(),
        }
    }

    /// Line total for the preview. Unparseable input contributes zero
    /// rather than erroring: the field is still being typed in, and the
    /// server recomputes the real figure on save anyway.
    fn preview_total(&self) -> Decimal {
        let q: Decimal = self.quantity.trim().parse().unwrap_or_default();
        let p: Decimal = self.unit_price.trim().parse().unwrap_or_default();
        q * p
    }
}

#[component]
pub fn QuoteNewPage() -> Element {
    if !use_can_manage_billing() {
        return permission_required();
    }
    rsx! { QuoteEditor { quote_id: None } }
}

#[component]
pub fn QuoteEditPage(id: String) -> Element {
    if !use_can_manage_billing() {
        return permission_required();
    }
    rsx! { QuoteEditor { quote_id: Some(id) } }
}

#[derive(Props, Clone, PartialEq)]
struct QuoteEditorProps {
    quote_id: Option<String>,
}

#[component]
fn QuoteEditor(props: QuoteEditorProps) -> Element {
    let navigator = use_navigator();
    let editing = props.quote_id.clone();
    let is_edit = editing.is_some();
    let title = if is_edit { "Edit Quote" } else { "New Quote" };
    use_page_title(title);

    let mut company_id = use_signal(String::new);
    let mut company_name = use_signal(String::new);
    let mut title = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut valid_until = use_signal(String::new);
    let mut tax_amount = use_signal(|| "0".to_string());
    let mut lines = use_signal(|| vec![DraftLine::new()]);
    let mut error = use_signal(String::new);
    let mut company_error = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut loaded = use_signal(|| false);

    // On edit, seed the form from the server once.
    let id_for_load = editing.clone();
    let existing = use_resource(move || {
        let qid = id_for_load.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let qid = qid?;
            crate::hooks::fetch::api::get_authed::<QuoteResponse>(&format!("/quotes/{qid}"))
                .await
                .ok()
        }
    });
    if let Some(Some(q)) = &*existing.read_unchecked() {
        if !*loaded.read() {
            company_id.set(q.company_id.to_string());
            company_name.set(q.company_name.clone().unwrap_or_default());
            title.set(q.title.clone());
            summary.set(q.summary.clone().unwrap_or_default());
            description.set(q.description.clone().unwrap_or_default());
            valid_until.set(
                q.valid_until
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default(),
            );
            tax_amount.set(q.tax_amount.to_string());
            let seeded: Vec<DraftLine> = q
                .lines
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|l| DraftLine {
                    line_type: l.line_type,
                    description: l.description,
                    quantity: l.quantity.to_string(),
                    unit_price: l.unit_price.to_string(),
                })
                .collect();
            if !seeded.is_empty() {
                lines.set(seeded);
            }
            loaded.set(true);
        }
    }

    let line_type_options = vec![
        SelectOption::new("service", "Service"),
        SelectOption::new("product", "Product"),
        SelectOption::new("labour", "Labour"),
        SelectOption::new("expense", "Expense"),
        SelectOption::new("adjustment", "Adjustment"),
        SelectOption::new("discount", "Discount"),
    ];

    // Running preview only. The server recomputes from the saved lines,
    // and what it returns is what the detail page shows.
    let subtotal_preview: Decimal = lines.read().iter().map(|l| l.preview_total()).sum();
    let tax_preview: Decimal = tax_amount.read().trim().parse().unwrap_or_default();
    let total_preview = subtotal_preview + tax_preview;

    let editing_for_submit = editing.clone();
    let on_submit = move |_| {
        let company = company_id.read().trim().to_string();
        let title_text = title.read().trim().to_string();
        let draft_lines = lines.read().clone();

        let mut guard = FormGuard::new();
        company_error.set(String::new());
        error.set(String::new());
        if company.is_empty() {
            company_error.set("Company is required.".to_string());
            guard.note_invalid(Some("company_id"));
        }
        if title_text.is_empty() {
            error.set("Title is required.".to_string());
            guard.note_invalid(Some("title"));
        }
        if draft_lines
            .iter()
            .any(|l| l.description.trim().is_empty() && !l.quantity.trim().is_empty())
        {
            error.set("Every line needs a description.".to_string());
            guard.note_invalid(Some("lines"));
        }
        if guard.blocked() {
            return;
        }

        let payload_lines: Vec<QuoteLineRequest> = draft_lines
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.description.trim().is_empty())
            .map(|(i, l)| QuoteLineRequest {
                line_type: l.line_type.clone(),
                description: l.description.trim().to_string(),
                quantity: l.quantity.trim().to_string(),
                unit_price: l.unit_price.trim().to_string(),
                sort_order: i as i32,
            })
            .collect();

        let summary_text = summary.read().trim().to_string();
        let description_text = description.read().trim().to_string();
        let valid_text = valid_until.read().trim().to_string();
        let tax_text = tax_amount.read().trim().to_string();
        let editing_id = editing_for_submit.clone();

        submitting.set(true);
        spawn(async move {
            let result = match editing_id {
                Some(qid) => {
                    let body = UpdateQuoteRequest {
                        title: Some(title_text),
                        summary: Some(summary_text),
                        description: Some(description_text),
                        valid_until: valid_text.parse().ok(),
                        tax_amount: (!tax_text.is_empty()).then_some(tax_text),
                        lines: Some(payload_lines),
                        ..Default::default()
                    };
                    crate::hooks::fetch::api::put_authed::<QuoteResponse, _>(
                        &format!("/quotes/{qid}"),
                        &body,
                    )
                    .await
                }
                None => {
                    let Ok(company_uuid) = company.parse::<Uuid>() else {
                        error.set("Pick a company from the list.".to_string());
                        submitting.set(false);
                        return;
                    };
                    let body = CreateQuoteRequest {
                        company_id: company_uuid,
                        billing_contact_id: None,
                        title: title_text,
                        summary: (!summary_text.is_empty()).then_some(summary_text),
                        description: (!description_text.is_empty()).then_some(description_text),
                        valid_until: valid_text.parse().ok(),
                        tax_amount: (!tax_text.is_empty()).then_some(tax_text),
                        lines: payload_lines,
                    };
                    crate::hooks::fetch::api::post_authed::<QuoteResponse, _>("/quotes", &body)
                        .await
                }
            };
            match result {
                Ok(saved) => {
                    crate::hooks::toast::push_toast(
                        crate::components::AlertType::Success,
                        "Quote saved",
                    );
                    navigator.push(Route::QuoteDetail {
                        id: saved.id.to_string(),
                    });
                }
                Err(e) => error.set(format!("Could not save the quote: {e}")),
            }
            submitting.set(false);
        });
    };

    rsx! {
        PageHeader {
            title: if is_edit { "Edit Quote" } else { "New Quote" },
            subtitle: "Scope the work and price it. Totals are calculated from the line items.",
        }

        if !error.read().is_empty() {
            ErrorBanner { class: "mb-3", "{error}" }
        }

        Card { class: "mb-6", title: "Details",
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                div {
                    // The client cannot change after creation: the
                    // quote's identity as "for this customer" is what
                    // the portal scopes on.
                    if is_edit {
                        Input {
                            name: "company",
                            label: "Client",
                            value: company_name.read().clone(),
                            disabled: true,
                        }
                    } else {
                        crate::components::CompanyPicker {
                            value: company_name.read().clone(),
                            selected_id: (!company_id.read().is_empty())
                                .then(|| company_id.read().clone()),
                            required: true,
                            error: company_error.read().clone(),
                            onselect: move |(id, name): (String, String)| {
                                company_id.set(id);
                                company_name.set(name);
                                company_error.set(String::new());
                            },
                            onclear: move |_| {
                                company_id.set(String::new());
                                company_name.set(String::new());
                            },
                        }
                    }
                }
                Input {
                    name: "valid_until",
                    label: "Valid until",
                    r#type: "date".to_string(),
                    value: valid_until.read().clone(),
                    oninput: move |e: FormEvent| valid_until.set(e.value()),
                }
            }
            Input {
                name: "title",
                label: "Title",
                value: title.read().clone(),
                rules: vec![Rule::Required, Rule::MaxLen(255)],
                maxlength: 255,
                oninput: move |e: FormEvent| title.set(e.value()),
            }
            Input {
                name: "summary",
                label: "Summary",
                value: summary.read().clone(),
                rules: vec![Rule::MaxLen(2000)],
                oninput: move |e: FormEvent| summary.set(e.value()),
            }
            Textarea {
                name: "description",
                label: "Scope of work",
                rows: 6,
                value: description.read().clone(),
                oninput: move |e: FormEvent| description.set(e.value()),
            }
        }

        Card { class: "mb-6", title: "Line items",
            div { class: "space-y-3",
                for (idx, line) in lines.read().clone().into_iter().enumerate() {
                    div { key: "{idx}", class: "grid grid-cols-1 md:grid-cols-12 gap-2 items-end",
                        div { class: "md:col-span-5",
                            Input {
                                name: "description-{idx}",
                                label: "Description",
                                value: line.description.clone(),
                                oninput: move |e: FormEvent| {
                                    let mut current = lines.read().clone();
                                    if let Some(l) = current.get_mut(idx) {
                                        l.description = e.value();
                                    }
                                    lines.set(current);
                                },
                            }
                        }
                        div { class: "md:col-span-2",
                            Select {
                                name: "line_type-{idx}",
                                label: "Type",
                                options: line_type_options.clone(),
                                value: line.line_type.clone(),
                                onchange: move |e: FormEvent| {
                                    let mut current = lines.read().clone();
                                    if let Some(l) = current.get_mut(idx) {
                                        l.line_type = e.value();
                                    }
                                    lines.set(current);
                                },
                            }
                        }
                        div { class: "md:col-span-2",
                            Input {
                                name: "quantity-{idx}",
                                label: "Qty",
                                r#type: "number".to_string(),
                                step: "0.01".to_string(),
                                value: line.quantity.clone(),
                                oninput: move |e: FormEvent| {
                                    let mut current = lines.read().clone();
                                    if let Some(l) = current.get_mut(idx) {
                                        l.quantity = e.value();
                                    }
                                    lines.set(current);
                                },
                            }
                        }
                        div { class: "md:col-span-2",
                            Input {
                                name: "unit_price-{idx}",
                                label: "Unit price",
                                r#type: "number".to_string(),
                                step: "0.01".to_string(),
                                value: line.unit_price.clone(),
                                oninput: move |e: FormEvent| {
                                    let mut current = lines.read().clone();
                                    if let Some(l) = current.get_mut(idx) {
                                        l.unit_price = e.value();
                                    }
                                    lines.set(current);
                                },
                            }
                        }
                        div { class: "md:col-span-1",
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| {
                                    let mut current = lines.read().clone();
                                    if current.len() > 1 {
                                        current.remove(idx);
                                        lines.set(current);
                                    }
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
            div { class: "mt-3",
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| {
                        let mut current = lines.read().clone();
                        current.push(DraftLine::new());
                        lines.set(current);
                    },
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "Add line"
                }
            }

            div { class: "mt-4 flex flex-col items-end gap-2 text-sm",
                div { class: "w-40",
                    Input {
                        name: "tax_amount",
                        label: "Tax",
                        r#type: "number".to_string(),
                        step: "0.01".to_string(),
                        value: tax_amount.read().clone(),
                        oninput: move |e: FormEvent| tax_amount.set(e.value()),
                    }
                }
                // Preview only: the server is the source of truth and
                // recomputes from the saved lines.
                div { "Subtotal: " span { class: "font-medium", "{format_money(subtotal_preview)}" } }
                div { class: "text-base",
                    "Total: " span { class: "font-semibold", "{format_money(total_preview)}" }
                }
                p { class: "text-xs text-subtle", "Calculated on save." }
            }
        }

        div { class: "flex justify-end gap-2",
            Link {
                to: Route::QuoteList {},
                Button { variant: ButtonVariant::Secondary, "Cancel" }
            }
            Button {
                variant: ButtonVariant::Primary,
                disabled: *submitting.read(),
                onclick: on_submit,
                "Save quote"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_variants_are_distinct_for_internal_and_client_outcomes() {
        // `approved` (we cleared it) and `accepted` (the client signed)
        // must not look the same, or the list is unreadable at a glance.
        assert_ne!(
            quote_status_variant("approved"),
            quote_status_variant("accepted")
        );
        assert_eq!(quote_status_variant("accepted"), BadgeVariant::Green);
        assert_eq!(quote_status_variant("converted"), BadgeVariant::Purple);
    }

    #[test]
    fn every_status_has_an_explainer() {
        for s in [
            "draft",
            "submitted",
            "approved",
            "rejected",
            "sent",
            "accepted",
            "declined",
            "expired",
            "converted",
            "cancelled",
        ] {
            assert!(!status_explainer(s).is_empty(), "{s} needs an explainer");
        }
    }

    #[test]
    fn draft_line_preview_tolerates_half_typed_input() {
        // The fields are strings mid-keystroke; a partial number must
        // contribute zero rather than panicking or erroring the form.
        let mut l = DraftLine::new();
        l.quantity = "2".into();
        l.unit_price = "500".into();
        assert_eq!(l.preview_total(), Decimal::from(1000));

        l.unit_price = "".into();
        assert_eq!(l.preview_total(), Decimal::ZERO);
        l.unit_price = "-".into();
        assert_eq!(l.preview_total(), Decimal::ZERO);
    }

    #[test]
    fn negative_lines_reduce_the_preview_total() {
        // A discount line is legitimate and must subtract.
        let mut l = DraftLine::new();
        l.unit_price = "-25".into();
        assert_eq!(l.preview_total(), Decimal::from(-25));
    }
}

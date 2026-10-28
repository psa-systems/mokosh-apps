//! Employee expense claims (MAPPS-1163, server MAPPS-1100 Phase B).
//!
//! The other half of the Bills and Expenses subsystem, and deliberately
//! not the bills page with a different noun. A bill is an obligation to a
//! vendor that an admin enters; a claim is made BY a person, decided by
//! somebody else, and paid back by finance:
//!
//! ```text
//! draft -> submitted -> approved -> reimbursed
//!                    \-> rejected
//! ```
//!
//! `reimbursed` is terminal and immutable, and so is `rejected`: a
//! correction is a reversing claim, the model PMS-1334 uses for invoice
//! amendments. The buttons offer exactly the transitions the server
//! accepts from the current status, because every other one comes back 409.
//!
//! # A line is an amount, not a quantity
//!
//! An expense line carries `amount_minor` directly: a taxi fare is an
//! amount, not two of something. That is the one shape difference from a
//! bill line, and it is why this page has its own line editor rather than
//! sharing the bills one.
//!
//! # Who reaches this page (MAPPS-1165, server PMS-1535)
//!
//! Anybody signed in. The server opened the claim surface to every member
//! of the tenant: they can file a claim, read their own and submit it,
//! while approve, reject and reimburse stay finance's, and the LIST is
//! scoped server-side to the caller's own claims.
//!
//! So the page renders for everyone and shows a non-finance member less:
//! no Who column (every row is them), and no decision buttons, because
//! the server answers 403 and a button that always fails is worse than no
//! button. `transitions_for` takes the role as its second input for that
//! reason, and stays pure so both sides of the check are testable without
//! a server.

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Card, ConfirmDialog, DataTable,
    ErrorBanner, IconSize, Modal, PageHeader, PlusIcon, Select, SelectOption, Table, TableBody,
    TableCell, TableEmpty, TableHead, TableHeader, TableLoading, TableRow,
};
use crate::pages::bills::minor_from_str;
use crate::utils::money::format_money_minor;
use crate::utils::url::urlencoding_minimal;
use crate::utils::Paginated;
use crate::Route;

const PER_PAGE: usize = 25;

/// `Expense`, as `GET /expenses` serves it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct RemoteExpense {
    pub id: Uuid,
    /// The submitter. An expense is per-user by schema.
    pub user_id: Uuid,
    #[serde(default)]
    pub incurred_on: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub subtotal_minor: i64,
    #[serde(default)]
    pub tax_minor: i64,
    #[serde(default)]
    pub total_minor: i64,
    #[serde(default)]
    pub status: String,
    /// Who decided it, once somebody has.
    #[serde(default)]
    pub approver_id: Option<Uuid>,
    /// Where the reimbursement was recorded, when the tenant delegated it
    /// (MAPPS-1168, server PMS-1541). Both NULL until a claim is paid back
    /// through a provider, and on every claim where it was not.
    #[serde(default)]
    pub external_provider: Option<String>,
    #[serde(default)]
    pub external_expense_id: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteExpenseLine {
    id: Uuid,
    #[serde(default)]
    category_id: Option<Uuid>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    amount_minor: i64,
    #[serde(default)]
    tax_minor: i64,
}

/// `ExpenseWithLines`, which the server FLATTENS the way it flattens
/// `BillWithLines`: the claim's own fields at the top level with `lines`
/// beside them. MAPPS-1164 is the bug this shape prevents here.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteExpenseWithLines {
    #[serde(flatten)]
    expense: RemoteExpense,
    #[serde(default)]
    lines: Vec<RemoteExpenseLine>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
struct NamedRow {
    id: Uuid,
    #[serde(default)]
    name: String,
}

/// Just enough of `MemberRow` to put a name against a submitter.
///
/// `#[serde(tag = "kind")]` on the server, so the discriminant picks the
/// variant; an unplaced guest has no `users` row and so can never be a
/// submitter, which is what the catch-all is for.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum MemberName {
    User {
        user_id: Uuid,
        #[serde(default)]
        first_name: String,
        #[serde(default)]
        last_name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MembersEnvelope {
    #[serde(default)]
    rows: Vec<MemberName>,
}

/// The list path for a page, a status and a search (MAPPS-1167, server
/// PMS-1539).
///
/// The server scopes a non-finance caller to their own claims before any
/// filter applies (PMS-1535), so these narrow what that caller can see
/// and never widen it.
pub(crate) fn list_path(page: usize, status: &str, q: &str) -> String {
    let mut path = format!("/expenses?page={page}&per_page={PER_PAGE}");
    let status = status.trim();
    if !status.is_empty() {
        path.push_str(&format!("&status={status}"));
    }
    let q = q.trim();
    if !q.is_empty() {
        path.push_str(&format!("&q={}", urlencoding_minimal(q)));
    }
    path
}

/// The five statuses a claim moves through, with the label the picker
/// shows (MAPPS-1167).
///
/// One list rather than a second copy inside the `Select`: a test asserts
/// `status_badge` gives each of these its own label, so the filter and the
/// column cannot drift apart.
pub(crate) const EXPENSE_STATUSES: &[(&str, &str)] = &[
    ("draft", "Draft"),
    ("submitted", "Submitted"),
    ("approved", "Approved"),
    ("reimbursed", "Reimbursed"),
    ("rejected", "Rejected"),
];

/// The badge for one of the five statuses the server's CHECK allows.
///
/// `submitted` is amber because it is waiting on somebody, which is the
/// state a reader scanning the column is looking for. An unknown status
/// renders as itself: it means the server is ahead of this build.
pub(crate) fn status_badge(status: &str) -> (String, BadgeVariant) {
    match status {
        "draft" => ("Draft".to_string(), BadgeVariant::Gray),
        "submitted" => ("Submitted".to_string(), BadgeVariant::Yellow),
        "approved" => ("Approved".to_string(), BadgeVariant::Blue),
        "reimbursed" => ("Reimbursed".to_string(), BadgeVariant::Green),
        "rejected" => ("Rejected".to_string(), BadgeVariant::Red),
        other => (other.to_string(), BadgeVariant::Gray),
    }
}

/// The transitions this caller may make from one status, in the order a
/// claim moves through them.
///
/// One function rather than four predicates, because the rule they encode
/// is one table and a reader checking it against `transition_expense`
/// should not have to assemble it from four places. `(label, action)`, so
/// the button copy and the path segment cannot drift apart.
///
/// `can_decide` is the caller's half of the rule (PMS-1535): submitting is
/// the submitter's own act and open to anybody, while approve, reject and
/// reimburse are finance's. A claim waiting on finance offers a
/// non-finance member nothing, which is the honest answer and not an
/// omission.
pub(crate) fn transitions_for(status: &str, can_decide: bool) -> Vec<(&'static str, &'static str)> {
    match status {
        "draft" => vec![("Submit", "submit")],
        "submitted" if can_decide => vec![("Approve", "approve"), ("Reject", "reject")],
        "approved" if can_decide => vec![("Mark reimbursed", "reimburse")],
        // `reimbursed` and `rejected` are terminal, and so is anything
        // this build does not recognise: offering a transition on a status
        // we cannot reason about is how a 409 reaches a user.
        _ => Vec::new(),
    }
}

/// Which of those transitions needs confirming before it fires.
///
/// Reject is the one that cannot be walked back: the server has no path
/// from `rejected` to anything, so a misclick is a claim the submitter has
/// to enter again.
pub(crate) fn needs_confirmation(action: &str) -> bool {
    action == "reject"
}

/// The `CreateExpenseRequest` body, or the first reason it cannot be
/// built.
///
/// Fallible and pure together, the shape MAPPS-1157 used for bills: every
/// amount is converted here, so a refusal names the line and nothing
/// half-converted reaches the server.
pub(crate) fn expense_body(
    incurred_on: &str,
    currency: &str,
    notes: &str,
    lines: &[(String, String, String, String)],
) -> Result<serde_json::Value, String> {
    if incurred_on.trim().is_empty() {
        return Err("A claim needs the date the cost was incurred.".to_string());
    }
    let currency = currency.trim().to_uppercase();
    if currency.chars().count() != 3 {
        return Err("Currency is a 3-letter code, like USD.".to_string());
    }
    if lines.is_empty() {
        return Err("A claim needs at least one line.".to_string());
    }
    let mut built = Vec::with_capacity(lines.len());
    for (position, (description, category_id, amount, tax)) in lines.iter().enumerate() {
        let what = if description.trim().is_empty() {
            format!("Line {}", position + 1)
        } else {
            description.trim().to_string()
        };
        if description.trim().is_empty() {
            return Err(format!("{what} needs a description."));
        }
        let amount_minor = minor_from_str(amount)
            .ok_or_else(|| format!("{what}: amount is a value with at most 2 decimals."))?;
        let tax_minor = if tax.trim().is_empty() {
            0
        } else {
            minor_from_str(tax)
                .ok_or_else(|| format!("{what}: tax is a value with at most 2 decimals."))?
        };
        built.push(serde_json::json!({
            "category_id": if category_id.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(category_id.trim().to_string())
            },
            "description": description.trim(),
            "amount_minor": amount_minor,
            "tax_minor": tax_minor,
            "position": position as i64,
        }));
    }
    Ok(serde_json::json!({
        "incurred_on": incurred_on.trim(),
        "currency": currency,
        "notes": if notes.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(notes.trim().to_string())
        },
        "lines": built,
    }))
}

/// How many members to ask for per request, and how many requests to make.
///
/// Below the server's cap of 100 on purpose: at the cap a truncated page
/// and a complete one look identical, which is the mistake
/// `check-per-page-cap` exists to stop. Paged to the end rather than read
/// once, because a submitter on page two would otherwise render as a
/// former member.
const MEMBERS_PER_PAGE: usize = 50;
const MEMBERS_MAX_PAGES: usize = 20;

/// Read `/members` and keep an id-to-name map, plus whether the loop
/// exhausted [`MEMBERS_MAX_PAGES`] without ever seeing a short page
/// (MAPPS-1172).
///
/// `pub(crate)` because the Mappings page needs the same map to list
/// people beside the contact they are in the provider (MAPPS-1168), and
/// the paging below is the part worth not writing twice.
///
/// A column of uuids answers nothing, and both `user_id` and `approver_id`
/// are uuids. `get_all_authed` cannot do this one: `/members` answers
/// `{rows, total, ...}` where that helper reads `{data, meta}`, so the
/// paging is here in the same shape, stopping on a short page.
///
/// The `bool` is `true` only when the cap was hit before a short page: a
/// network failure mid-page returns whatever was accumulated with `false`,
/// because that is a partial read, not a tenant too big for the cap, and
/// the caller already has a `fetch_failed` signal for the former.
pub(crate) async fn member_names() -> (HashMap<Uuid, String>, bool) {
    member_names_paged(|page| async move {
        let path = format!("/members?page={page}&per_page={MEMBERS_PER_PAGE}");
        crate::hooks::fetch::api::get_authed::<MembersEnvelope>(&path).await
    })
    .await
}

/// The paging loop behind [`member_names`], with the page fetch taken as
/// a closure so a test can hand it canned pages instead of a live
/// `/members` endpoint `cargo test` cannot reach.
async fn member_names_paged<F, Fut>(mut fetch_page: F) -> (HashMap<Uuid, String>, bool)
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Result<MembersEnvelope, String>>,
{
    let mut names = HashMap::new();
    for page in 1..=MEMBERS_MAX_PAGES {
        let Ok(envelope) = fetch_page(page)
            .await
            .inspect_err(|e| tracing::error!("member list load failed: {e}"))
        else {
            return (names, false);
        };
        let full = envelope.rows.len() >= MEMBERS_PER_PAGE;
        for row in envelope.rows {
            if let MemberName::User {
                user_id,
                first_name,
                last_name,
            } = row
            {
                names.insert(
                    user_id,
                    format!("{first_name} {last_name}").trim().to_string(),
                );
            }
        }
        if !full {
            return (names, false);
        }
    }
    (names, true)
}

/// The label for a claim/approval row's member id: the real name when the
/// lookup hits, otherwise "A former member" UNLESS the map is known to be
/// truncated by the page cap, in which case the id simply never made it
/// into the first `MEMBERS_MAX_PAGES * MEMBERS_PER_PAGE` rows and a current
/// member must not be told they left (MAPPS-1172).
fn member_label(names: &HashMap<Uuid, String>, truncated: bool, id: Uuid) -> String {
    match names.get(&id).cloned().filter(|n| !n.is_empty()) {
        Some(name) => name,
        None if truncated => format!("Member {}", &id.to_string()[..8]),
        None => "A former member".to_string(),
    }
}

/// Whether this caller may decide claims, which is finance's job and the
/// second input to [`transitions_for`].
fn use_can_decide() -> bool {
    let auth = crate::hooks::use_auth();
    // Copied out of the guard before it drops, the shape the sibling pages
    // use: the signal's read guard cannot outlive this function.
    let decides = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);
    decides
}

#[component]
pub fn ExpenseListPage() -> Element {
    use_page_title("Expense Claims");
    // No role gate (MAPPS-1165): the server lets any member file a claim
    // and scopes the list to their own, so there is nothing here to
    // withhold.
    rsx! { ExpenseListBody {} }
}

#[component]
fn ExpenseListBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut creating = use_signal(|| false);
    let mut status = use_signal(String::new);
    let mut search = use_signal(String::new);

    let current_page = (*page.read()).max(1);
    let status_text = status.read().clone();
    let search_text = search.read().clone();
    // MAPPS-931: one refetch per pause in typing, not one per keystroke.
    let search_debounced = crate::hooks::use_debounced_signal(search, 300);
    let mut resource = use_resource(move || {
        // Read inside the closure so the resource subscribes to the page
        // and the filters itself (MAPPS-847).
        let path = list_path(
            (*page.read()).max(1),
            &status.read(),
            &search_debounced.read(),
        );
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteExpense>>(&path)
                .await
                .inspect_err(|e| tracing::error!("expense list load failed: {e}"))
                .ok()
        }
    });
    let members = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        member_names().await
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteExpense>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };
    let (names, members_truncated) = members.read_unchecked().clone().unwrap_or_default();

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    let can_decide = use_can_decide();
    let navigator = use_navigator();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Expense Claims".to_string() }
        };
    }

    // MAPPS-1165: the server scopes a non-finance caller's list to their
    // own claims, so the Who column would be the same name on every row
    // and the copy would be about people they cannot see.
    let columns = if can_decide { 5 } else { 4 };

    // An empty list under a filter is "nothing matches", not "nothing
    // here": the second offers a New Claim button to somebody whose claims
    // are one filter away.
    let filtered = !status_text.trim().is_empty() || !search_text.trim().is_empty();
    let mut status_options = vec![SelectOption::new("", "All claims")];
    status_options.extend(
        EXPENSE_STATUSES
            .iter()
            .map(|(value, label)| SelectOption::new(value.to_string(), label.to_string())),
    );

    rsx! {
        PageHeader {
            title: "Expense Claims",
            subtitle: if can_decide { "What your people spent, and what has been paid back" } else { "What you spent, and what has been paid back" },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| creating.set(true),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Claim"
                }
            },
        }

        p { class: "mb-4 text-sm text-muted",
            if can_decide {
                "A claim is submitted by the person who spent the money, approved or rejected by somebody else, and marked reimbursed once it has been paid back. A claim you create here is yours."
            } else {
                "These are your own claims. Enter what you spent and submit it; somebody in finance approves it and marks it reimbursed once you have been paid back."
            }
        }

        Card { class: "mb-4",
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-3",
                crate::components::Input {
                    name: "expense_search",
                    label: "Search",
                    r#type: "search",
                    placeholder: "What it was for",
                    value: search_text.clone(),
                    oninput: move |e: FormEvent| {
                        search.set(e.value());
                        page.set(1);
                    },
                }
                Select {
                    name: "expense_status",
                    label: "Status",
                    options: status_options,
                    value: status_text.clone(),
                    onchange: move |e: FormEvent| {
                        status.set(e.value());
                        page.set(1);
                    },
                }
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load expense claims. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        if can_decide {
                            TableHeader { "Who" }
                        }
                        TableHeader { "Incurred" }
                        TableHeader { class: "text-right", "Total" }
                        TableHeader { "Decided by" }
                        TableHeader { "Status" }
                    }
                }
                if is_loading {
                    TableLoading { columns, rows: 4 }
                } else if rows.is_empty() && !fetch_failed && filtered {
                    TableEmpty {
                        columns,
                        message: "No claims match this filter.".to_string(),
                    }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns,
                        title: "No claims yet".to_string(),
                        description: "Enter what was spent, with a line per receipt, and submit it for approval.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                                onclick: move |_| creating.set(true),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Claim"
                            }
                        },
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let id = row.id;
                                let who = member_label(&names, members_truncated, row.user_id);
                                let decided = row
                                    .approver_id
                                    .and_then(|a| names.get(&a).cloned())
                                    .filter(|n| !n.is_empty())
                                    .unwrap_or_default();
                                let incurred = row.incurred_on.clone();
                                let total = format_money_minor(row.total_minor);
                                let (label, tone) = status_badge(&row.status);
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: move |_| {
                                            navigator.push(Route::ExpenseDetail { id: id.to_string() });
                                        },
                                        if can_decide {
                                            TableCell {
                                                // MAPPS-569: the row navigates, so this cell is
                                                // the keyboard path.
                                                onactivate: move |_| {
                                                    navigator.push(Route::ExpenseDetail { id: id.to_string() });
                                                },
                                                span { class: "font-medium text-accent", "{who}" }
                                            }
                                            TableCell { "{incurred}" }
                                        } else {
                                            // The first cell carries the keyboard path, so
                                            // without the Who column the date takes it.
                                            TableCell {
                                                onactivate: move |_| {
                                                    navigator.push(Route::ExpenseDetail { id: id.to_string() });
                                                },
                                                span { class: "font-medium text-accent", "{incurred}" }
                                            }
                                        }
                                        TableCell { class: "text-right font-medium", "{total}" }
                                        TableCell {
                                            if decided.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { "{decided}" }
                                            }
                                        }
                                        TableCell {
                                            Badge { variant: tone, "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if creating() {
            NewExpenseModal {
                onclose: move |_| creating.set(false),
                onsaved: move |_| {
                    creating.set(false);
                    resource.restart();
                },
            }
        }
    }
}

// ============================================================================
// Create
// ============================================================================

/// One line being entered: an amount, not a quantity and a price.
#[derive(Clone, Debug, PartialEq, Default)]
struct LineDraft {
    description: String,
    category_id: String,
    amount: String,
    tax: String,
}

#[derive(Props, Clone, PartialEq)]
struct NewExpenseModalProps {
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn NewExpenseModal(props: NewExpenseModalProps) -> Element {
    let mut incurred_on = use_signal(String::new);
    let mut currency = use_signal(|| "USD".to_string());
    let mut notes = use_signal(String::new);
    let mut lines = use_signal(|| vec![LineDraft::default()]);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let categories = use_resource(move || async move {
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/expense-categories")
            .await
            .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
            .ok()
    });
    let category_rows = categories
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();
    let mut category_options = vec![SelectOption::new("", "No category")];
    category_options.extend(
        category_rows
            .iter()
            .map(|c| SelectOption::new(c.id.to_string(), c.name.clone())),
    );

    // What the server will total, shown while it is typed: a total that
    // only appears after saving is one nobody can check against the
    // receipts in front of them.
    let preview: i64 = lines
        .read()
        .iter()
        .map(|l| {
            minor_from_str(&l.amount).unwrap_or(0)
                + if l.tax.trim().is_empty() {
                    0
                } else {
                    minor_from_str(&l.tax).unwrap_or(0)
                }
        })
        .sum();

    let handle_save = move |_| {
        if *saving.read() {
            return;
        }
        error.set(String::new());
        let drafts: Vec<(String, String, String, String)> = lines
            .read()
            .iter()
            .map(|l| {
                (
                    l.description.clone(),
                    l.category_id.clone(),
                    l.amount.clone(),
                    l.tax.clone(),
                )
            })
            .collect();
        let body = match expense_body(
            &incurred_on.read(),
            &currency.read(),
            &notes.read(),
            &drafts,
        ) {
            Ok(body) => body,
            Err(message) => {
                error.set(message);
                return;
            }
        };
        saving.set(true);
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                    "/expenses",
                    &body,
                )
                .await
                {
                    Ok(_) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not create the claim: {err}")),
                }
            }
            #[cfg(not(feature = "app"))]
            let _ = body;
            saving.set(false);
        });
    };

    rsx! {
        Modal {
            open: true,
            title: "New Claim".to_string(),
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| onclose.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Primary,
                    loading: *saving.read(),
                    onclick: handle_save,
                    data_testid: "create-expense",
                    "Create Claim"
                }
            },
            div { class: "space-y-4",
                if !error().is_empty() {
                    ErrorBanner { "{error}" }
                }
                div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                    crate::components::Input {
                        name: "expense_incurred_on",
                        label: "Incurred",
                        r#type: "date",
                        required: true,
                        help: "The day the money was spent, not today.",
                        value: incurred_on.read().clone(),
                        oninput: move |e: FormEvent| incurred_on.set(e.value()),
                    }
                    crate::components::Input {
                        name: "expense_currency",
                        label: "Currency",
                        required: true,
                        maxlength: 3,
                        value: currency.read().clone(),
                        oninput: move |e: FormEvent| currency.set(e.value()),
                    }
                }
                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        p { class: "text-sm font-medium text-content", "Lines" }
                        span { class: "text-sm text-muted", "Total {format_money_minor(preview)}" }
                    }
                    for (index, line) in lines.read().clone().into_iter().enumerate() {
                        div { key: "{index}", class: "grid grid-cols-1 gap-3 rounded-md border border-line p-3 sm:grid-cols-12",
                            div { class: "sm:col-span-5",
                                crate::components::Input {
                                    name: "expense_line_description_{index}",
                                    label: "Description",
                                    required: true,
                                    maxlength: 500,
                                    value: line.description.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].description = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-3",
                                Select {
                                    name: "expense_line_category_{index}",
                                    label: "Category",
                                    options: category_options.clone(),
                                    value: line.category_id.clone(),
                                    onchange: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].category_id = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-2",
                                crate::components::Input {
                                    name: "expense_line_amount_{index}",
                                    label: "Amount",
                                    r#type: "number",
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "0.00",
                                    value: line.amount.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].amount = e.value());
                                    },
                                }
                            }
                            div { class: "sm:col-span-2",
                                crate::components::Input {
                                    name: "expense_line_tax_{index}",
                                    label: "Tax",
                                    r#type: "number",
                                    step: "0.01".to_string(),
                                    min: "0".to_string(),
                                    placeholder: "0.00",
                                    value: line.tax.clone(),
                                    oninput: move |e: FormEvent| {
                                        lines.with_mut(|ls| ls[index].tax = e.value());
                                    },
                                }
                            }
                        }
                    }
                    div { class: "flex gap-2",
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| lines.with_mut(|ls| ls.push(LineDraft::default())),
                            PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                            "Add line"
                        }
                        // The server refuses a claim with no lines, so the last
                        // one cannot be removed.
                        if lines.read().len() > 1 {
                            Button {
                                variant: ButtonVariant::Secondary,
                                onclick: move |_| lines.with_mut(|ls| { ls.pop(); }),
                                "Remove last line"
                            }
                        }
                    }
                }
                crate::components::Textarea {
                    name: "expense_notes",
                    label: "Notes",
                    placeholder: "Optional",
                    rows: 2,
                    value: notes.read().clone(),
                    oninput: move |e: FormEvent| notes.set(e.value()),
                }
            }
        }
    }
}

// ============================================================================
// Detail
// ============================================================================

/// Post one lifecycle transition and reload the claim.
///
/// A free function rather than a closure the buttons share: a closure that
/// captures signals is not `Copy`, so a second button could not have it.
/// The signals themselves are `Copy`, which is what makes this work.
fn fire_transition(
    expense_id: String,
    action: &'static str,
    mut busy: Signal<bool>,
    mut error: Signal<String>,
    mut confirming: Signal<Option<&'static str>>,
    mut resource: Resource<Option<RemoteExpenseWithLines>>,
) {
    let path = format!("/expenses/{expense_id}/{action}");
    busy.set(true);
    error.set(String::new());
    confirming.set(None);
    spawn(async move {
        #[cfg(feature = "app")]
        {
            if let Err(e) = crate::hooks::fetch::api::post_authed_no_content(&path).await {
                error.set(format!("Could not {action} the claim: {e}"));
            }
        }
        #[cfg(not(feature = "app"))]
        let _ = path;
        busy.set(false);
        resource.restart();
    });
}

#[component]
pub fn ExpenseDetailPage(id: String) -> Element {
    use_page_title("Expense Claim");
    // No role gate, for the reason the list has none. The server refuses
    // somebody else's claim with a 403, and the detail read below surfaces
    // that as the page's own error rather than being pre-empted by a gate
    // this app cannot evaluate: whose claim it is, is not known until it
    // has been read.
    rsx! { ExpenseDetailBody { id } }
}

#[component]
fn ExpenseDetailBody(id: String) -> Element {
    let expense_id = id.clone();
    // Not `mut`: every write goes through `fire_transition`, which takes
    // them by value, and `Signal` is `Copy`.
    let error = use_signal(String::new);
    let busy = use_signal(|| false);
    let mut confirming = use_signal(|| None::<&'static str>);

    let resource = use_resource(use_reactive(&expense_id, |expense_id| async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        crate::hooks::fetch::api::get_authed::<RemoteExpenseWithLines>(&format!(
            "/expenses/{expense_id}"
        ))
        .await
        .inspect_err(|e| tracing::error!("expense load failed: {e}"))
        .ok()
    }));

    let members = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        member_names().await
    });
    let categories = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_all_authed::<NamedRow>("/expense-categories")
            .await
            .inspect_err(|e| tracing::error!("expense category list load failed: {e}"))
            .ok()
    });

    let snap = resource.read_unchecked().clone();
    let data = snap.as_ref().and_then(|o| o.as_ref()).cloned();
    let (names, members_truncated) = members.read_unchecked().clone().unwrap_or_default();
    let category_names: HashMap<Uuid, String> = categories
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    let can_mutate = crate::hooks::use_can_mutate();
    let can_decide = use_can_decide();

    rsx! {
        PageHeader {
            title: "Expense Claim",
            subtitle: "What was spent, and where the claim has got to",
            breadcrumbs: rsx! {
                crate::components::Breadcrumbs {
                    items: crate::components::detail_breadcrumbs("Expense Claims", Route::ExpenseList {}, "Claim"),
                }
            },
        }
        if !error().is_empty() {
            ErrorBanner { class: "mb-4", "{error}" }
        }
        match data {
            None => rsx! { crate::components::DetailSkeleton {} },
            Some(loaded) => {
                let claim = loaded.expense.clone();
                let (label, tone) = status_badge(&claim.status);
                let who = member_label(&names, members_truncated, claim.user_id);
                let decided = claim
                    .approver_id
                    .and_then(|a| names.get(&a).cloned())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_default();
                let notes = claim.notes.clone().unwrap_or_default();
                let offered = if can_mutate {
                    transitions_for(&claim.status, can_decide)
                } else {
                    Vec::new()
                };
                let claim_id = claim.id.to_string();
                rsx! {
                    Card {
                        title: who.clone(),
                        actions: rsx! { Badge { variant: tone, "{label}" } },
                        div { class: "space-y-4",
                            dl { class: "grid grid-cols-1 gap-4 text-sm sm:grid-cols-4",
                                div {
                                    dt { class: "text-muted", "Incurred" }
                                    dd { class: "font-medium text-content", "{claim.incurred_on}" }
                                }
                                div {
                                    dt { class: "text-muted", "Currency" }
                                    dd { class: "font-medium text-content", "{claim.currency}" }
                                }
                                div {
                                    dt { class: "text-muted", "Decided by" }
                                    dd { class: "font-medium text-content",
                                        if decided.is_empty() { "Nobody yet" } else { "{decided}" }
                                    }
                                }
                                div {
                                    dt { class: "text-muted", "Total" }
                                    dd { class: "font-medium text-content", "{format_money_minor(claim.total_minor)}" }
                                }
                            }
                            if let (Some(provider), Some(external)) = (
                                claim.external_provider.clone(),
                                claim.external_expense_id.clone(),
                            ) {
                                // MAPPS-1168: what a reconciliation reads. The
                                // pair is written when the claim is paid back
                                // through the provider and never before, so its
                                // presence is also how somebody can tell this
                                // claim went through one.
                                crate::components::StatusBanner { tone: crate::components::BannerTone::Info,
                                    "This reimbursement is recorded in {provider} as {external}. It was written there when the claim was marked reimbursed."
                                }
                            }
                            if !notes.is_empty() {
                                p { class: "text-sm text-muted", "{notes}" }
                            }
                            div { class: "flex gap-2 pt-2",
                                for (button_label, action) in offered.clone() {
                                    {
                                        let claim_id = claim_id.clone();
                                        rsx! {
                                            Button {
                                                key: "{action}",
                                                variant: if action == "reject" { ButtonVariant::Danger } else { ButtonVariant::Primary },
                                                loading: busy(),
                                                onclick: move |_| {
                                                    if needs_confirmation(action) {
                                                        confirming.set(Some(action));
                                                    } else {
                                                        fire_transition(
                                                            claim_id.clone(),
                                                            action,
                                                            busy,
                                                            error,
                                                            confirming,
                                                            resource,
                                                        );
                                                    }
                                                },
                                                data_testid: "expense-{action}",
                                                "{button_label}"
                                            }
                                        }
                                    }
                                }
                            }
                            if offered.is_empty() {
                                p { class: "text-sm text-muted",
                                    if claim.status == "submitted" || claim.status == "approved" {
                                        // Not finished, waiting on somebody
                                        // else (MAPPS-1165): saying "finished"
                                        // here would be wrong, and offering
                                        // the buttons would be a 403.
                                        "This claim is with finance now. They approve it and mark it reimbursed once you have been paid back."
                                    } else {
                                        "This claim is finished: a reimbursed or rejected claim cannot be moved again. A correction is a new claim, the way an invoice amendment is a new invoice."
                                    }
                                }
                            }
                            ConfirmDialog {
                                open: confirming().is_some(),
                                title: "Reject this claim?".to_string(),
                                message: "The claim stays on record as rejected and cannot be moved again. If it was rejected by mistake, the person who spent the money has to enter it a second time.".to_string(),
                                confirm_text: "Reject".to_string(),
                                destructive: true,
                                loading: busy(),
                                onconfirm: {
                                    let claim_id = claim_id.clone();
                                    move |_| {
                                        if let Some(action) = confirming() {
                                            fire_transition(
                                                claim_id.clone(),
                                                action,
                                                busy,
                                                error,
                                                confirming,
                                                resource,
                                            );
                                        }
                                    }
                                },
                                oncancel: move |_| confirming.set(None),
                            }
                        }
                    }

                    Card { title: "Lines".to_string(), class: "mt-4",
                        Table {
                            TableHead {
                                TableRow {
                                    TableHeader { "Description" }
                                    TableHeader { "Category" }
                                    TableHeader { class: "text-right", "Amount" }
                                    TableHeader { class: "text-right", "Tax" }
                                }
                            }
                            TableBody {
                                for line in loaded.lines.iter().cloned() {
                                    {
                                        let key = line.id.to_string();
                                        let category = line
                                            .category_id
                                            .and_then(|c| category_names.get(&c).cloned())
                                            .unwrap_or_default();
                                        rsx! {
                                            TableRow { key: "{key}",
                                                TableCell { "{line.description}" }
                                                TableCell {
                                                    if category.is_empty() {
                                                        span { class: "text-subtle", "-" }
                                                    } else {
                                                        span { "{category}" }
                                                    }
                                                }
                                                TableCell { class: "text-right", "{format_money_minor(line.amount_minor)}" }
                                                TableCell { class: "text-right", "{format_money_minor(line.tax_minor)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        dl { class: "mt-4 space-y-1 text-sm",
                            div { class: "flex justify-between",
                                dt { class: "text-muted", "Subtotal" }
                                dd { "{format_money_minor(claim.subtotal_minor)}" }
                            }
                            div { class: "flex justify-between",
                                dt { class: "text-muted", "Tax" }
                                dd { "{format_money_minor(claim.tax_minor)}" }
                            }
                            div { class: "flex justify-between font-medium",
                                dt { "Total" }
                                dd { "{format_money_minor(claim.total_minor)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_path_carries_only_the_filters_that_are_set() {
        assert_eq!(list_path(1, "", ""), "/expenses?page=1&per_page=25");
        assert_eq!(
            list_path(2, "submitted", ""),
            "/expenses?page=2&per_page=25&status=submitted"
        );
        assert_eq!(
            list_path(1, "", " taxi "),
            "/expenses?page=1&per_page=25&q=taxi"
        );
        assert_eq!(
            list_path(1, "reimbursed", "taxi"),
            "/expenses?page=1&per_page=25&status=reimbursed&q=taxi"
        );
        assert_eq!(list_path(1, "  ", "  "), "/expenses?page=1&per_page=25");
    }

    /// The picker offers exactly the statuses the column can render.
    #[test]
    fn every_filterable_status_has_its_own_badge() {
        for (value, label) in EXPENSE_STATUSES {
            let (rendered, _) = status_badge(value);
            assert_eq!(&rendered, label, "{value} renders as its own label");
        }
        assert_eq!(EXPENSE_STATUSES.len(), 5, "the five a claim moves through");
    }

    /// Changing a filter goes back to page one.
    #[test]
    fn changing_a_filter_resets_the_page() {
        let src = include_str!("expenses.rs");
        let body = &src[src.find("fn ExpenseListBody").expect("the body")
            ..src.find("fn NewExpenseModal").expect("the modal")];
        for name in ["expense_search", "expense_status"] {
            let at = body
                .find(name)
                .unwrap_or_else(|| panic!("{name} is rendered"));
            let handler = &body[at..];
            let end = handler.find("\n                }").unwrap_or(handler.len());
            assert!(
                handler[..end].contains("page.set(1)"),
                "{name} resets the page"
            );
        }
    }

    /// The five statuses the server's CHECK allows each get a badge, and
    /// an unknown one renders as itself.
    #[test]
    fn every_status_has_a_badge() {
        assert_eq!(status_badge("draft").0, "Draft");
        assert!(matches!(status_badge("submitted").1, BadgeVariant::Yellow));
        assert!(matches!(status_badge("approved").1, BadgeVariant::Blue));
        assert!(matches!(status_badge("reimbursed").1, BadgeVariant::Green));
        assert!(matches!(status_badge("rejected").1, BadgeVariant::Red));
        assert_eq!(status_badge("escalated").0, "escalated");
    }

    /// The buttons are exactly what `transition_expense` accepts from each
    /// status, and nothing is offered on a terminal one.
    #[test]
    fn the_offered_transitions_match_the_server() {
        assert_eq!(transitions_for("draft", true), vec![("Submit", "submit")]);
        assert_eq!(
            transitions_for("submitted", true),
            vec![("Approve", "approve"), ("Reject", "reject")]
        );
        assert_eq!(
            transitions_for("approved", true),
            vec![("Mark reimbursed", "reimburse")]
        );
        assert!(
            transitions_for("reimbursed", true).is_empty(),
            "reimbursed is terminal and immutable"
        );
        assert!(transitions_for("rejected", true).is_empty());
        assert!(
            transitions_for("something_new", true).is_empty(),
            "a status this build cannot reason about gets no buttons rather than a 409"
        );
    }

    /// PMS-1535: submitting is open to anybody, deciding is finance's.
    ///
    /// The three transitions a non-finance caller must not see are the
    /// three the server answers 403 for, so a button offering one would
    /// fail every time it was pressed.
    #[test]
    fn a_member_who_cannot_decide_is_offered_only_submit() {
        assert_eq!(
            transitions_for("draft", false),
            vec![("Submit", "submit")],
            "their own draft is theirs to send"
        );
        assert!(
            transitions_for("submitted", false).is_empty(),
            "approving and rejecting are finance's"
        );
        assert!(
            transitions_for("approved", false).is_empty(),
            "and so is marking it reimbursed"
        );
        for status in ["reimbursed", "rejected"] {
            assert!(transitions_for(status, false).is_empty(), "{status}");
        }
    }

    /// The page renders for everybody, which is the point of MAPPS-1165.
    ///
    /// Read off the source because the gate was a `return` before the
    /// body: a reinstated `NoFinancePermission` would leave every test
    /// above passing while no member could reach the page at all.
    #[test]
    fn neither_page_withholds_itself_from_a_member() {
        let src = include_str!("expenses.rs");
        let pages = &src[src.find("pub fn ExpenseListPage").expect("the list page")
            ..src.find("fn ExpenseListBody").expect("the list body")];
        assert!(
            !pages.contains("NoFinancePermission"),
            "the list page renders for any member"
        );
        let detail = &src[src
            .find("pub fn ExpenseDetailPage")
            .expect("the detail page")
            ..src.find("fn ExpenseDetailBody").expect("the detail body")];
        assert!(
            !detail.contains("NoFinancePermission"),
            "and so does the detail page"
        );
    }

    /// Reject confirms and the rest do not.
    ///
    /// It is the one with no way back: the server has no path out of
    /// `rejected`, so a misclick costs the submitter a second entry.
    #[test]
    fn only_reject_asks_first() {
        assert!(needs_confirmation("reject"));
        assert!(!needs_confirmation("submit"));
        assert!(!needs_confirmation("approve"));
        assert!(!needs_confirmation("reimburse"));
    }

    fn one_line() -> Vec<(String, String, String, String)> {
        vec![(
            "Taxi to site".to_string(),
            String::new(),
            "32.50".to_string(),
            String::new(),
        )]
    }

    #[test]
    fn the_body_matches_create_expense_request() {
        let body = expense_body("2026-10-23", "usd", "  ", &one_line()).expect("the body builds");
        assert_eq!(body["incurred_on"], "2026-10-23");
        assert_eq!(body["currency"], "USD", "upper-cased for the server");
        assert!(body["notes"].is_null(), "blank notes are absent, not empty");
        assert_eq!(
            body["lines"][0]["amount_minor"], 3250,
            "a line is an amount, with no quantity to multiply"
        );
        assert_eq!(body["lines"][0]["tax_minor"], 0, "blank tax is none");
        assert_eq!(body["lines"][0]["position"], 0);
        assert!(body["lines"][0]["category_id"].is_null());
        assert!(
            body["lines"][0].get("quantity_hundredths").is_none(),
            "an expense line has no quantity: sending one would be a field the server ignores"
        );
    }

    /// Every refusal names what to fix, and none of them reaches the
    /// server.
    #[test]
    fn a_body_that_cannot_be_built_says_which_field() {
        let no_date = expense_body("", "USD", "", &one_line()).unwrap_err();
        assert!(no_date.contains("date"), "{no_date}");
        let bad_currency = expense_body("2026-10-23", "US", "", &one_line()).unwrap_err();
        assert!(bad_currency.contains("3-letter"), "{bad_currency}");
        let no_lines = expense_body("2026-10-23", "USD", "", &[]).unwrap_err();
        assert!(no_lines.contains("at least one line"), "{no_lines}");
        let bad_amount = expense_body(
            "2026-10-23",
            "USD",
            "",
            &[(
                "Taxi to site".to_string(),
                String::new(),
                "32.509".to_string(),
                String::new(),
            )],
        )
        .unwrap_err();
        assert!(
            bad_amount.contains("Taxi to site") && bad_amount.contains("amount"),
            "the refusal names the line and the field: {bad_amount}"
        );
        let no_description = expense_body(
            "2026-10-23",
            "USD",
            "",
            &[(
                String::new(),
                String::new(),
                "32.50".to_string(),
                String::new(),
            )],
        )
        .unwrap_err();
        assert!(no_description.contains("Line 1"), "{no_description}");
    }

    /// The detail response is flattened, like the bills one (MAPPS-1164).
    ///
    /// A fixture in the server's own shape, not a round trip through our
    /// `Serialize`: that is the test that would have caught MAPPS-1164,
    /// and `ExpenseWithLines` carries the same `#[serde(flatten)]`.
    #[test]
    fn the_detail_response_is_read_flattened() {
        let body = serde_json::json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "user_id": "33333333-3333-3333-3333-333333333333",
            "incurred_on": "2026-10-23",
            "currency": "USD",
            "subtotal_minor": 3250,
            "tax_minor": 0,
            "total_minor": 3250,
            "status": "submitted",
            "approver_id": null,
            "approved_at": null,
            "reimbursed_at": null,
            "notes": null,
            "external_provider": "xero",
            "external_expense_id": "7c1f4e2a-9b3d-4f5e-8a6b-2d4f6a8c0e22",
            "created_at": "2026-10-23T00:00:00Z",
            "updated_at": "2026-10-23T00:00:00Z",
            "lines": [{
                "id": "44444444-4444-4444-4444-444444444444",
                "tenant_id": "22222222-2222-2222-2222-222222222222",
                "expense_id": "11111111-1111-1111-1111-111111111111",
                "category_id": null,
                "description": "Taxi to site",
                "amount_minor": 3250,
                "tax_minor": 0,
                "position": 0,
                "created_at": "2026-10-23T00:00:00Z",
            }],
        });
        let loaded: RemoteExpenseWithLines =
            serde_json::from_value(body).expect("the server's own shape deserialises");
        assert_eq!(loaded.expense.status, "submitted");
        assert_eq!(loaded.expense.total_minor, 3250);
        assert_eq!(loaded.lines.len(), 1);
        assert_eq!(loaded.lines[0].amount_minor, 3250);
        assert_eq!(
            loaded.expense.external_provider.as_deref(),
            Some("xero"),
            "and the delegated pair survives the flatten (MAPPS-1168)"
        );
        assert_eq!(
            loaded.expense.external_expense_id.as_deref(),
            Some("7c1f4e2a-9b3d-4f5e-8a6b-2d4f6a8c0e22")
        );
    }

    /// A member row is read by its `kind` discriminant, and a row that is
    /// not a user is skipped rather than failing the whole page.
    #[test]
    fn member_rows_are_read_by_kind() {
        let envelope: MembersEnvelope = serde_json::from_value(serde_json::json!({
            "rows": [
                {
                    "kind": "user",
                    "user_id": "33333333-3333-3333-3333-333333333333",
                    "email": "a@example.test",
                    "first_name": "Ada",
                    "last_name": "Lovelace",
                    "role": "finance",
                    "status": "active",
                    "last_login_at": null,
                    "team_memberships": [],
                },
                {
                    "kind": "unplaced_guest",
                    "grant_id": "g1",
                    "grantee_email": "b@example.test",
                    "grantee_name": "Grace",
                    "role": "admin",
                    "granted_at": null,
                },
            ],
            "total": 2,
            "page": 1,
            "per_page": 100,
            "bunyip_reachable": true,
        }))
        .expect("the members envelope deserialises");
        assert_eq!(envelope.rows.len(), 2);
        let users: Vec<_> = envelope
            .rows
            .iter()
            .filter(|r| matches!(r, MemberName::User { .. }))
            .collect();
        assert_eq!(
            users.len(),
            1,
            "an unplaced guest has no users row, so it can never be a submitter"
        );
    }

    fn member_row(id: Uuid) -> serde_json::Value {
        serde_json::json!({
            "kind": "user",
            "user_id": id.to_string(),
            "first_name": "Member",
            "last_name": id.to_string(),
        })
    }

    /// MAPPS-1172: a tenant with more than `MEMBERS_MAX_PAGES *
    /// MEMBERS_PER_PAGE` members hits the cap before a short page, so the
    /// loop must say so rather than returning a map indistinguishable from
    /// a tenant that genuinely has fewer than 1000 members.
    #[cfg(all(feature = "app", not(target_arch = "wasm32")))]
    #[tokio::test]
    async fn paging_past_the_cap_reports_truncation() {
        let total_rows = MEMBERS_MAX_PAGES * MEMBERS_PER_PAGE + 1;
        let ids: Vec<Uuid> = (0..total_rows).map(|_| Uuid::new_v4()).collect();
        let (names, truncated) = member_names_paged(|page| {
            let ids = ids.clone();
            async move {
                let start = (page - 1) * MEMBERS_PER_PAGE;
                let rows: Vec<serde_json::Value> = ids
                    [start..(start + MEMBERS_PER_PAGE).min(ids.len())]
                    .iter()
                    .map(|id| member_row(*id))
                    .collect();
                Ok(serde_json::from_value(serde_json::json!({ "rows": rows }))
                    .expect("the envelope deserialises"))
            }
        })
        .await;
        assert!(
            truncated,
            "every one of the {MEMBERS_MAX_PAGES} pages fetched was full, so the loop hit the cap"
        );
        assert_eq!(
            names.len(),
            MEMBERS_MAX_PAGES * MEMBERS_PER_PAGE,
            "only the rows actually fetched before the cap are in the map"
        );
        let past_the_cap = ids[MEMBERS_MAX_PAGES * MEMBERS_PER_PAGE];
        assert!(
            !names.contains_key(&past_the_cap),
            "the 1001st member never made it into the result"
        );
    }

    /// A short page (the ordinary case) is not truncation.
    #[cfg(all(feature = "app", not(target_arch = "wasm32")))]
    #[tokio::test]
    async fn a_short_page_is_not_truncation() {
        let id = Uuid::new_v4();
        let (names, truncated) = member_names_paged(|page| {
            let id = id;
            async move {
                let rows = if page == 1 {
                    vec![member_row(id)]
                } else {
                    vec![]
                };
                Ok(serde_json::from_value(serde_json::json!({ "rows": rows }))
                    .expect("the envelope deserialises"))
            }
        })
        .await;
        assert!(!truncated, "a short first page ends the loop normally");
        assert!(names.contains_key(&id));
    }

    /// MAPPS-1172: the claims list's fallback label. A lookup miss on a
    /// truncated map is a member who simply never made it into the first
    /// `MEMBERS_MAX_PAGES * MEMBERS_PER_PAGE` rows, not someone who left,
    /// so it must not render "A former member".
    #[test]
    fn a_lookup_miss_is_a_former_member_only_when_the_map_is_not_truncated() {
        let names = HashMap::new();
        let id = Uuid::new_v4();
        assert_eq!(member_label(&names, false, id), "A former member");
        let neutral = member_label(&names, true, id);
        assert_ne!(neutral, "A former member");
        assert!(
            neutral.contains(&id.to_string()[..8]),
            "the neutral label still names who this is: {neutral}"
        );
    }

    /// A hit in the map always wins, truncated or not.
    #[test]
    fn a_lookup_hit_is_always_the_real_name_regardless_of_truncation() {
        let id = Uuid::new_v4();
        let mut names = HashMap::new();
        names.insert(id, "Ada Lovelace".to_string());
        assert_eq!(member_label(&names, false, id), "Ada Lovelace");
        assert_eq!(member_label(&names, true, id), "Ada Lovelace");
    }
}

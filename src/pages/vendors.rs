//! Who the MSP buys from (MAPPS-1157, server MAPPS-1100 Phase A).
//!
//! `vendors` is the other side of `companies`: a company is a customer the
//! MSP invoices, a vendor is somebody the MSP is billed by, and the two are
//! separate tables on purpose (migration 279). Until this page existed the
//! server had full CRUD and nothing called it, so a tenant could not enter
//! the vendor a bill is owed to, and the vendor mapping surface MAPPS-1156
//! added listed rows nobody could create.
//!
//! It sits under Settings beside Products because it is the same kind of
//! thing: reference data an administrator curates, not a working surface.
//! It carries the FINANCE gate, not the admin one, because that is what the
//! server answers (`super_admin | admin | finance`) and a page that hid
//! itself from a finance user would be lying about what they may do.
//!
//! Delete is offered and the server's refusal is shown. A vendor named by a
//! bill is refused by the foreign key, and `.is_ok()` on that response
//! would close the dialog on nothing (MAPPS-574).

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{
    use_page_title, Button, ButtonVariant, DataTable, ErrorBanner, IconSize, PageHeader, PlusIcon,
    SettingFormModal, Table, TableBody, TableCell, TableEmpty, TableHead, TableHeader,
    TableLoading, TableRow,
};
use crate::utils::{FormGuard, Paginated, Rule};
use crate::Route;

const PER_PAGE: usize = 25;

/// `Vendor`, as `GET /vendors` serves it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct RemoteVendor {
    id: uuid::Uuid,
    #[serde(default)]
    name: String,
    #[serde(default)]
    account_number: Option<String>,
    #[serde(default)]
    default_currency: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

/// The list path for a page.
///
/// No search box, and no `q`: `GET /vendors` takes pagination and nothing
/// else. A field that posted a filter the server ignores would look like a
/// search that finds nothing, which is worse than no search.
pub(crate) fn list_path(page: usize) -> String {
    format!("/vendors?page={page}&per_page={PER_PAGE}")
}

/// A blank optional field is `null`, not `""`.
///
/// The server caps `account_number` and validates `default_currency` as
/// exactly three characters, so an empty string is a value that fails
/// validation where absence is what the admin meant.
fn optional_json(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

/// `/settings/vendors`. Finance-gated like Products.
#[component]
pub fn VendorsSettingsPage() -> Element {
    let auth = crate::hooks::use_auth();
    let has_finance = auth
        .read()
        .user
        .as_ref()
        .map(|u| u.role.can_manage_billing())
        .unwrap_or(false);

    use_page_title("Vendors");
    if !has_finance {
        return rsx! { crate::pages::billing::NoFinancePermission { title: "Vendors" } };
    }
    rsx! { VendorsSettingsBody {} }
}

#[component]
fn VendorsSettingsBody() -> Element {
    let mut page = use_signal(|| 1usize);
    let mut editing = use_signal(|| None::<VendorFormState>);

    let current_page = (*page.read()).max(1);
    let mut resource = use_resource(move || {
        // Read inside the closure so the resource's own subscription attaches
        // to the page signal rather than to a re-render of this component
        // (MAPPS-847).
        let path = list_path((*page.read()).max(1));
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let _reachable = crate::hooks::use_server_reachable();
            crate::hooks::fetch::api::get_authed::<Paginated<RemoteVendor>>(&path)
                .await
                .inspect_err(|e| tracing::error!("vendor list load failed: {e}"))
                .ok()
        }
    });

    let snap = resource.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let (rows, total): (Vec<RemoteVendor>, u64) = match &*snap {
        Some(Some(resp)) => (resp.data.clone(), resp.meta.total),
        _ => (Vec::new(), 0),
    };

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Vendors".to_string() }
        };
    }

    rsx! {
        PageHeader {
            title: "Vendors",
            subtitle: "Who you buy from, and who your bills are owed to",
            breadcrumbs: rsx! {
                crate::pages::settings::SettingsBreadcrumb { current: Route::SettingsVendors {} }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                    onclick: move |_| editing.set(Some(VendorFormState::new())),
                    PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                    "New Vendor"
                }
            },
        }

        p { class: "mb-4 text-sm text-muted",
            "A vendor is the other side of a client: somebody who bills you. A bill names one, and so does the mapping that lets a bill be raised in Xero or QuickBooks."
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load vendors. Refresh the page to retry." }
        }

        DataTable {
            loading: is_loading,
            total_items: total as usize,
            current_page,
            per_page: PER_PAGE,
            columns: 4,
            onpagechange: move |p| page.set(p),
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Account number" }
                        TableHeader { "Bills in" }
                        TableHeader { "Notes" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 4, rows: 4 }
                } else if rows.is_empty() && !fetch_failed {
                    TableEmpty {
                        columns: 4,
                        title: "No vendors yet".to_string(),
                        description: "Add who you buy from, so a bill can name the vendor it is owed to.".to_string(),
                        actions: rsx! {
                            Button {
                                variant: ButtonVariant::Primary,
                                disabled: !can_mutate,
                                title: (!can_mutate).then(|| "Can't do this while the server is unreachable".to_string()),
                                onclick: move |_| editing.set(Some(VendorFormState::new())),
                                PlusIcon { size: IconSize::Small, class: "mr-2".to_string() }
                                "New Vendor"
                            }
                        },
                    }
                } else {
                    TableBody {
                        for row in rows.iter().cloned() {
                            {
                                let key = row.id.to_string();
                                let edit_state = VendorFormState::from_existing(&row);
                                let name = row.name.clone();
                                let account = row.account_number.clone().unwrap_or_default();
                                let currency = row.default_currency.clone().unwrap_or_default();
                                let notes = row.notes.clone().unwrap_or_default();
                                rsx! {
                                    TableRow { key: "{key}", clickable: true,
                                        onclick: {
                                            let edit_state = edit_state.clone();
                                            move |_| editing.set(Some(edit_state.clone()))
                                        },
                                        TableCell {
                                            // MAPPS-569: the row opens a modal, so this cell is
                                            // the keyboard path.
                                            onactivate: move |_| editing.set(Some(edit_state.clone())),
                                            span { class: "font-medium text-accent", "{name}" }
                                        }
                                        TableCell {
                                            if account.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { class: "font-mono text-xs", "{account}" }
                                            }
                                        }
                                        TableCell {
                                            if currency.is_empty() {
                                                span { class: "text-subtle", "-" }
                                            } else {
                                                span { "{currency}" }
                                            }
                                        }
                                        TableCell { class: "max-w-xs truncate text-muted", "{notes}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(state) = editing.read().clone() {
            VendorFormModal {
                state,
                onclose: move |_| editing.set(None),
                onsaved: move |_| {
                    editing.set(None);
                    resource.restart();
                },
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct VendorFormState {
    id: Option<String>,
    name: String,
    account_number: String,
    default_currency: String,
    notes: String,
}

impl VendorFormState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            account_number: String::new(),
            default_currency: String::new(),
            notes: String::new(),
        }
    }

    fn from_existing(v: &RemoteVendor) -> Self {
        Self {
            id: Some(v.id.to_string()),
            name: v.name.clone(),
            account_number: v.account_number.clone().unwrap_or_default(),
            default_currency: v.default_currency.clone().unwrap_or_default(),
            notes: v.notes.clone().unwrap_or_default(),
        }
    }

    /// The `UpsertVendorRequest` body. Pure, so the null-for-blank rule is
    /// pinned by a test rather than by reading the form.
    fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name.trim(),
            "account_number": optional_json(&self.account_number),
            "default_currency": optional_json(&self.default_currency.to_uppercase()),
            "notes": optional_json(&self.notes),
        })
    }
}

#[derive(Props, Clone, PartialEq)]
struct VendorFormModalProps {
    state: VendorFormState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
}

#[component]
fn VendorFormModal(props: VendorFormModalProps) -> Element {
    let initial = props.state.clone();
    let is_edit = initial.id.is_some();

    let mut name = use_signal(|| initial.name.clone());
    let mut account_number = use_signal(|| initial.account_number.clone());
    let mut default_currency = use_signal(|| initial.default_currency.clone());
    let mut notes = use_signal(|| initial.notes.clone());
    let mut saving = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut name_err = use_signal(String::new);
    let mut account_err = use_signal(String::new);
    let mut currency_err = use_signal(String::new);

    let onclose = props.onclose;
    let onsaved = props.onsaved;

    let save_id = initial.id.clone();
    let handle_save = move |_| {
        if *saving.read() || *deleting.read() {
            return;
        }
        error.set(String::new());

        // PMS-518: every field reported at once, first invalid focused. The
        // caps mirror `UpsertVendorRequest`.
        let mut guard = FormGuard::new();
        let name_v = name.read().trim().to_string();
        let account_v = account_number.read().trim().to_string();
        let currency_v = default_currency.read().trim().to_string();
        name_err.set(guard.field(
            "vendor_name",
            &name_v,
            "Name",
            &[Rule::Required, Rule::MaxLen(200)],
        ));
        account_err.set(guard.field(
            "vendor_account_number",
            &account_v,
            "Account number",
            &[Rule::MaxLen(100)],
        ));
        // Three characters exactly, which is what the server validates. A
        // two-letter country code typed here is the mistake worth catching
        // before it is a 400 nobody can place.
        currency_err.set(
            if currency_v.is_empty() || currency_v.chars().count() == 3 {
                String::new()
            } else {
                "Currency is a 3-letter code, like USD.".to_string()
            },
        );
        if guard.blocked() || !currency_err.read().is_empty() {
            return;
        }

        saving.set(true);
        let body = VendorFormState {
            id: save_id.clone(),
            name: name_v,
            account_number: account_v,
            default_currency: currency_v,
            notes: notes.read().clone(),
        }
        .body();
        let id = save_id.clone();
        spawn(async move {
            #[cfg(feature = "app")]
            {
                let result = match id {
                    None => crate::hooks::fetch::api::post_authed::<serde_json::Value, _>(
                        "/vendors", &body,
                    )
                    .await
                    .map(|_| ()),
                    Some(id) => crate::hooks::fetch::api::put_authed::<serde_json::Value, _>(
                        &format!("/vendors/{id}"),
                        &body,
                    )
                    .await
                    .map(|_| ()),
                };
                match result {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not save vendor: {err}")),
                }
            }
            saving.set(false);
        });
    };

    // The server refuses a vendor a bill names, and that refusal is what the
    // admin needs to read: it lands in `error` so the dialog stays open
    // saying why rather than closing on nothing (MAPPS-574).
    let delete_id = initial.id.clone();
    let handle_delete = move |_| {
        let Some(id) = delete_id.clone() else { return };
        if *saving.read() || *deleting.read() {
            return;
        }
        deleting.set(true);
        error.set(String::new());
        spawn(async move {
            #[cfg(feature = "app")]
            {
                match crate::hooks::fetch::api::delete_authed(&format!("/vendors/{id}")).await {
                    Ok(()) => onsaved.call(()),
                    Err(err) => error.set(format!("Could not delete vendor: {err}")),
                }
            }
            deleting.set(false);
        });
    };

    rsx! {
        SettingFormModal {
            title: if is_edit { "Edit Vendor".to_string() } else { "New Vendor".to_string() },
            is_edit,
            saving: *saving.read(),
            deleting: *deleting.read(),
            error: error.read().clone(),
            onclose: move |_| onclose.call(()),
            onsave: handle_save,
            ondelete: handle_delete,
            deletable: is_edit,
            create_label: "Create Vendor".to_string(),
            delete_title: "Delete vendor".to_string(),
            delete_message: "Delete this vendor? A vendor no bill names is removed. One that is on a bill is refused, because the bill still names it.".to_string(),
            crate::components::Input {
                name: "vendor_name",
                label: "Name",
                placeholder: "e.g. Rack Co",
                required: true,
                maxlength: 200,
                rules: vec![Rule::Required, Rule::MaxLen(200)],
                error: name_err(),
                value: name.read().clone(),
                oninput: move |e: FormEvent| {
                    name_err.set(String::new());
                    name.set(e.value());
                },
            }
            div { class: "grid grid-cols-1 gap-4 sm:grid-cols-2",
                crate::components::Input {
                    name: "vendor_account_number",
                    label: "Account number",
                    placeholder: "Optional",
                    maxlength: 100,
                    rules: vec![Rule::MaxLen(100)],
                    help: "Your account with them, as it appears on their invoices.",
                    error: account_err(),
                    value: account_number.read().clone(),
                    oninput: move |e: FormEvent| {
                        account_err.set(String::new());
                        account_number.set(e.value());
                    },
                }
                crate::components::Input {
                    name: "vendor_default_currency",
                    label: "Bills in",
                    placeholder: "USD",
                    maxlength: 3,
                    help: "The currency they bill you in, if it is not yours.",
                    error: currency_err(),
                    value: default_currency.read().clone(),
                    oninput: move |e: FormEvent| {
                        currency_err.set(String::new());
                        default_currency.set(e.value());
                    },
                }
            }
            crate::components::Textarea {
                name: "vendor_notes",
                label: "Notes",
                placeholder: "Optional",
                rows: 2,
                value: notes.read().clone(),
                oninput: move |e: FormEvent| notes.set(e.value()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pagination and nothing else, because that is all `GET /vendors`
    /// reads. A `q` here would be dropped by the server and read on the
    /// page as a search that finds nothing.
    #[test]
    fn the_list_path_asks_only_for_a_page() {
        assert_eq!(list_path(1), "/vendors?page=1&per_page=25");
        assert_eq!(list_path(3), "/vendors?page=3&per_page=25");
    }

    /// A blank optional field is `null`, not `""`.
    ///
    /// `default_currency` is the one that matters: the server validates it
    /// as exactly three characters, so an empty string is a 400 where
    /// absence is what the admin meant.
    #[test]
    fn a_blank_optional_field_is_sent_as_null() {
        let body = VendorFormState {
            id: None,
            name: "  Rack Co  ".into(),
            account_number: "   ".into(),
            default_currency: String::new(),
            notes: String::new(),
        }
        .body();
        assert_eq!(body["name"], "Rack Co", "and the name is trimmed");
        assert!(body["account_number"].is_null());
        assert!(body["default_currency"].is_null());
        assert!(body["notes"].is_null());
    }

    /// A currency typed in lower case reaches the server upper-cased.
    ///
    /// The server upper-cases what it stores on a bill but validates this
    /// field as it arrives, and a tenant reading `usd` back in a column
    /// headed by ISO codes would reasonably think it was wrong.
    #[test]
    fn a_currency_is_sent_upper_case() {
        let body = VendorFormState {
            id: None,
            name: "Rack Co".into(),
            account_number: String::new(),
            default_currency: "usd".into(),
            notes: String::new(),
        }
        .body();
        assert_eq!(body["default_currency"], "USD");
    }
}
